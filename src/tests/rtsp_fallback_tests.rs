use super::*;
use openh264::formats::{YUVBuffer, YUVSource};
use std::hint::black_box;

#[test]
fn fallback_and_snapshot_jpegs_preserve_range_colors_and_padded_strides() {
    let (width, height) = (24, 18);
    for (y, u, v) in [
        (16, 128, 128),
        (235, 128, 128),
        (126, 128, 128),
        (81, 90, 240),
        (145, 54, 34),
        (41, 240, 110),
        (235, 240, 240),
        (16, 0, 0),
    ] {
        let mut source = vec![y; width * height];
        source.extend(vec![u; width * height / 4]);
        source.extend(vec![v; width * height / 4]);
        let source = YUVBuffer::from_vec(source, width, height);
        let mut encoder = openh264::encoder::Encoder::new().unwrap();
        let h264 = encoder.encode(&source).unwrap().to_vec();
        let mut decoder = openh264::decoder::Decoder::new().unwrap();
        let yuv = decoder.decode(&h264).unwrap().unwrap();
        assert_eq!(yuv.dimensions(), (width, height));
        assert!(yuv.strides().0 > width);
        let mut reference = vec![0; width * height * 3];
        yuv.write_rgb8(&mut reference);
        for quality in [FALLBACK_JPEG_QUALITY, SNAPSHOT_JPEG_QUALITY] {
            let mut rgb = Vec::new();
            let jpeg = yuv_to_jpeg(&yuv, &mut rgb, quality).unwrap();
            assert_eq!(rgb, reference);
            let mut jpeg_decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(jpeg));
            let decoded = jpeg_decoder.decode().unwrap();
            let info = jpeg_decoder.info().unwrap();
            assert_eq!(usize::from(info.width), width);
            assert_eq!(usize::from(info.height), height);
            assert_eq!(info.pixel_format, jpeg_decoder::PixelFormat::RGB24);
            for (index, (&actual, &expected)) in decoded.iter().zip(&reference).enumerate() {
                assert!(
                    actual.abs_diff(expected) <= 5,
                    "YUV ({y}, {u}, {v}), quality {quality}, byte {index}: {actual} vs {expected}"
                );
            }
        }
    }
}

#[tokio::test]
async fn websocket_video_can_stop_without_losing_audio_status_or_keyframe_resync() {
    let id = "test-fallback-video-preference";
    let state = registry().get_or_insert(
        id,
        "rtsp://127.0.0.1/unused".into(),
        &Credentials::default(),
    );
    // Feed the consumer directly; no camera session is needed for this test.
    state.running.store(true, Ordering::SeqCst);
    let init = Arc::new(StreamInit {
        kind: "init",
        video: VideoInit {
            codec: "avc1.640028".into(),
            description: String::new(),
            width: 1920,
            height: 1080,
        },
        audio: Some(AudioInit {
            sample_rate: 8000,
            channels: 1,
        }),
    });
    state.init.send_replace(Some(Arc::clone(&init)));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (sock, _) = listener.accept().await.unwrap();
        serve_ws(tokio_tungstenite::accept_async(sock).await.unwrap(), id)
            .await
            .unwrap();
    });
    let (mut player, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/"))
        .await
        .unwrap();
    let first = next_message(&mut player).await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(first.to_text().unwrap()).unwrap()["type"],
        "init"
    );
    send_video(&state, true);
    assert_eq!(next_message(&mut player).await.into_data()[0], 0);

    for (kind, enabled) in [("audio", true), ("video", false)] {
        player
            .send(Message::Text(
                serde_json::json!({ "type": kind, "enabled": enabled })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
    }
    // Pong is a barrier: both preferences have been processed before we send frames.
    player.send(Message::Ping(Bytes::new())).await.unwrap();
    assert!(matches!(next_message(&mut player).await, Message::Pong(_)));
    send_video(&state, true);
    send_audio(&state);
    assert_eq!(next_message(&mut player).await.into_data()[0], 1);

    state.error.send_replace(Some("camera reconnecting".into()));
    let error = next_message(&mut player).await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(error.to_text().unwrap()).unwrap()["type"],
        "error"
    );
    player
        .send(Message::Text(r#"{"type":"video","enabled":true}"#.into()))
        .await
        .unwrap();
    player.send(Message::Ping(Bytes::new())).await.unwrap();
    assert!(matches!(next_message(&mut player).await, Message::Pong(_)));
    send_video(&state, false);
    send_audio(&state);
    assert_eq!(next_message(&mut player).await.into_data()[0], 1);
    send_video(&state, true);
    assert_eq!(next_message(&mut player).await.into_data()[0], 0);
    send_video(&state, false);
    assert_eq!(next_message(&mut player).await.into_data()[0], 0);

    state.init.send_replace(Some(init));
    let update = next_message(&mut player).await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(update.to_text().unwrap()).unwrap()["type"],
        "init"
    );

    player.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    registry().remove(id);
}

async fn next_message(
    player: &mut WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>,
) -> Message {
    tokio::time::timeout(Duration::from_secs(5), player.next())
        .await
        .expect("WebSocket message should arrive")
        .unwrap()
        .unwrap()
}

fn send_video(state: &StreamState, key: bool) {
    assert!(state
        .frames
        .send(Arc::new(MediaFrame::Video {
            ts_us: 0,
            key,
            data: Bytes::from_static(&[1]),
        }))
        .is_ok());
}

fn send_audio(state: &StreamState) {
    assert!(state
        .frames
        .send(Arc::new(MediaFrame::Audio {
            ts_us: 0,
            pcm: Bytes::from_static(&[0, 0]),
        }))
        .is_ok());
}

#[test]
#[ignore = "release-mode encoding benchmark"]
fn benchmark_fallback_jpeg_encoding() {
    let (width, height) = (1920, 1080);
    let mut source = vec![0; width * height * 3 / 2];
    for y in 0..height {
        for x in 0..width {
            source[y * width + x] = 16 + ((x / 8 + y / 8) % 220) as u8;
        }
    }
    let chroma_len = width * height / 4;
    for i in 0..chroma_len {
        source[width * height + i] = 96 + (i % 64) as u8;
        source[width * height + chroma_len + i] = 96 + ((i / 16) % 64) as u8;
    }
    let source = YUVBuffer::from_vec(source, width, height);
    let mut encoder = openh264::encoder::Encoder::new().unwrap();
    let h264 = encoder.encode(&source).unwrap().to_vec();
    let mut decoder = openh264::decoder::Decoder::new().unwrap();
    let yuv = decoder.decode(&h264).unwrap().unwrap();
    assert_eq!(yuv.dimensions(), (width, height));
    let mut rgb = Vec::new();
    for _ in 0..5 {
        black_box(yuv_to_jpeg(&yuv, &mut rgb, FALLBACK_JPEG_QUALITY).unwrap());
    }
    let mut timings = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        for _ in 0..30 {
            black_box(yuv_to_jpeg(&yuv, &mut rgb, FALLBACK_JPEG_QUALITY).unwrap());
        }
        timings.push(start.elapsed().as_secs_f64() * 1000.0 / 30.0);
    }
    timings.sort_by(f64::total_cmp);
    eprintln!(
        "1080p YUV-to-RGB + JPEG quality {FALLBACK_JPEG_QUALITY}: median {:.2} ms/frame ({:.1} fps), runs {timings:.2?}",
        timings[2],
        1000.0 / timings[2],
    );
}
