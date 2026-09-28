//! Native RTSP backend: in-process RTSP/RTP client (`retina`) feeding the
//! WebView over a loopback WebSocket, decoded there by WebCodecs.
//!
//! No external binaries. One [`StreamState`] per `(device, profile)` owns a
//! retina session that runs only while somebody is watching; every consumer
//! (WebSocket player, MJPEG fallback) subscribes to its broadcast of
//! [`MediaFrame`]s.
//!
//! ## Wire format (loopback WebSocket, `/ws/{id}` on the video server)
//!
//! * One text message first: JSON [`StreamInit`] — WebCodecs codec string,
//!   base64 `avcC`/`hvcC` description, dimensions, PCM audio format.
//!   Re-sent if the camera changes parameters mid-stream.
//! * Text `{"type":"error","message":str}` whenever the camera session
//!   fails (first line of the error); the next init means it recovered.
//! * Binary messages: `[kind u8][flags u8][ts_us u64 LE][payload]`.
//!   `kind` 0 = video (AVCC-framed access unit; `flags & 1` = key frame),
//!   1 = audio (interleaved s16 LE PCM, already decoded here).
//! * Client → server text: `{"type":"audio","enabled":bool}`. Audio frames
//!   are not sent until asked for; the stage opens muted.
//!
//! ## Fallback (`/mjpeg/{id}`)
//!
//! Where the WebView lacks WebCodecs (older WebKitGTK) the page swaps in an
//! `<img>` pointed here: OpenH264 decodes on a dedicated thread, frames go
//! out as `multipart/x-mixed-replace` JPEG. H.264 only.
//!
//! ## Recording + snapshots
//!
//! [`start_recording`] attaches a third kind of consumer that muxes the raw
//! access units + decoded PCM into an `.mkv` (see [`super::mkv`]).
//! [`snapshot_jpeg`] grabs the next key frame and decodes it with OpenH264,
//! for cameras with no usable ONVIF snapshot URI.

use crate::state::Credentials;
use crate::video::{mkv, EmbedKind, VideoBackend, VideoSource};
use base64::Engine;
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use retina::client::{PlayOptions, SessionOptions, SetupOptions, TcpTransportOptions, Transport};
use retina::codec::{CodecItem, FrameFormat, ParametersRef};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use tracing::{debug, info, warn};

/// How long a session keeps pulling from the camera after its last
/// consumer disconnects. Covers a tab switch or a player reconnect without
/// paying DESCRIBE/SETUP/PLAY and a keyframe wait again.
const IDLE_TIMEOUT: Duration = Duration::from_secs(10);
/// No RTP for this long means the camera or the network is gone; reconnect.
const STALL_TIMEOUT: Duration = Duration::from_secs(15);
const RECONNECT_BACKOFF: Duration = Duration::from_secs(2);
/// Frames buffered per subscriber before it is marked lagged and waits for
/// the next key frame. ~4 s of 15 fps video + audio.
const BROADCAST_CAPACITY: usize = 256;
const INIT_WAIT: Duration = Duration::from_secs(20);
/// Longest we wait for a key frame when taking a snapshot (GOPs run 1–4 s).
const SNAPSHOT_WAIT: Duration = Duration::from_secs(10);
const SNAPSHOT_JPEG_QUALITY: u8 = 90;

// ── Types shared with consumers ──────────────────────────────────────────────

#[derive(Clone, Debug, serde::Serialize)]
pub struct VideoInit {
    /// RFC 6381 string WebCodecs accepts directly, e.g. `avc1.64001F`.
    pub codec: String,
    /// `avcC` / `hvcC` record, base64 — WebCodecs `VideoDecoderConfig.description`.
    pub description: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AudioInit {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct StreamInit {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub video: VideoInit,
    pub audio: Option<AudioInit>,
}

pub enum MediaFrame {
    Video { ts_us: u64, key: bool, data: Bytes },
    Audio { ts_us: u64, pcm: Bytes },
}

impl MediaFrame {
    fn to_wire(&self) -> Vec<u8> {
        let (kind, flags, ts, payload) = match self {
            MediaFrame::Video { ts_us, key, data } => (0u8, u8::from(*key), *ts_us, data),
            MediaFrame::Audio { ts_us, pcm } => (1u8, 0u8, *ts_us, pcm),
        };
        let mut out = Vec::with_capacity(10 + payload.len());
        out.push(kind);
        out.push(flags);
        out.extend_from_slice(&ts.to_le_bytes());
        out.extend_from_slice(payload);
        out
    }
}

// ── Registry ─────────────────────────────────────────────────────────────────

pub struct StreamState {
    id: String,
    /// Latest RTSP URL for this stream; re-read on every (re)connect so a
    /// camera that mints a new URL per `GetStreamUri` doesn't fork sessions.
    url: Mutex<String>,
    creds: Option<retina::client::Credentials>,
    frames: broadcast::Sender<Arc<MediaFrame>>,
    init: watch::Sender<Option<Arc<StreamInit>>>,
    /// First line of the latest failure while the session retries; cleared by PLAY.
    error: watch::Sender<Option<String>>,
    running: AtomicBool,
}

impl StreamState {
    fn ensure_running(self: &Arc<Self>) {
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            // A previous session's failure must not greet this one's consumers.
            self.error.send_replace(None);
            let me = Arc::clone(self);
            tokio::spawn(async move {
                let id = me.id.clone();
                let inner = Arc::clone(&me);
                // A panic inside the session would otherwise vanish into the
                // JoinHandle and leave `running` stuck at true.
                if let Err(e) = tokio::spawn(run_session(inner)).await {
                    tracing::error!(id, error = %e, "RTSP session task panicked");
                }
                me.running.store(false, Ordering::SeqCst);
            });
        }
    }

    fn subscribe(
        &self,
    ) -> (
        broadcast::Receiver<Arc<MediaFrame>>,
        watch::Receiver<Option<Arc<StreamInit>>>,
    ) {
        (self.frames.subscribe(), self.init.subscribe())
    }

    /// Wait for the session to publish stream parameters.
    async fn wait_init(
        &self,
        rx: &mut watch::Receiver<Option<Arc<StreamInit>>>,
    ) -> Result<Arc<StreamInit>, String> {
        let fut = async {
            loop {
                // Clone out before awaiting: the borrow guard isn't `Send`.
                let current = rx.borrow_and_update().clone();
                if let Some(i) = current {
                    return Ok::<_, String>(i);
                }
                rx.changed()
                    .await
                    .map_err(|_| "stream closed".to_string())?;
            }
        };
        tokio::time::timeout(INIT_WAIT, fut)
            .await
            .map_err(|_| format!("no stream parameters within {INIT_WAIT:?}"))?
    }
}

#[derive(Default)]
struct Registry {
    streams: Mutex<HashMap<String, Arc<StreamState>>>,
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::default)
}

impl Registry {
    fn get_or_insert(&self, id: &str, url: String, creds: &Credentials) -> Arc<StreamState> {
        let mut map = self.streams.lock().unwrap();
        if let Some(s) = map.get(id) {
            let mut cur = s.url.lock().unwrap();
            if *cur != url {
                debug!(id, "stream URL changed; will use it on next connect");
                *cur = url;
            }
            return Arc::clone(s);
        }
        let creds = (!creds.username.is_empty()).then(|| retina::client::Credentials {
            username: creds.username.clone(),
            password: creds.password.clone(),
        });
        let (frames, _) = broadcast::channel(BROADCAST_CAPACITY);
        let (init, _) = watch::channel(None);
        let (error, _) = watch::channel(None);
        let state = Arc::new(StreamState {
            id: id.to_string(),
            url: Mutex::new(url),
            creds,
            frames,
            init,
            error,
            running: AtomicBool::new(false),
        });
        map.insert(id.to_string(), Arc::clone(&state));
        state
    }

    fn get(&self, id: &str) -> Option<Arc<StreamState>> {
        self.streams.lock().unwrap().get(id).cloned()
    }

    fn remove(&self, id: &str) {
        self.streams.lock().unwrap().remove(id);
    }
}

// ── Session task ─────────────────────────────────────────────────────────────

async fn run_session(state: Arc<StreamState>) {
    loop {
        match pull_once(&state).await {
            Ok(()) => {
                info!(id = %state.id, "RTSP session idle; stopping");
                break;
            }
            Err(e) => {
                if state.frames.receiver_count() == 0 {
                    info!(id = %state.id, error = %e, "RTSP session ended with no consumers");
                    break;
                }
                warn!(id = %state.id, error = %e, "RTSP session error; reconnecting");
                state
                    .error
                    .send_replace(e.lines().next().map(str::to_owned));
                tokio::time::sleep(RECONNECT_BACKOFF).await;
            }
        }
    }
}

/// One DESCRIBE→PLAY→pull loop. `Ok(())` means it stopped on purpose (idle).
async fn pull_once(state: &Arc<StreamState>) -> Result<(), String> {
    let url_s = state.url.lock().unwrap().clone();
    info!(id = %state.id, url = %url_s, "RTSP connecting");
    let url = url::Url::parse(&url_s).map_err(|e| format!("bad RTSP URL: {e}"))?;
    let opts = SessionOptions::default()
        .creds(state.creds.clone())
        .user_agent(format!("oxdm/{}", env!("CARGO_PKG_VERSION")));
    let mut session = retina::client::Session::describe(url, opts)
        .await
        .map_err(|e| format!("DESCRIBE: {e}"))?;

    let video_i = session
        .streams()
        .iter()
        .position(|s| s.media() == "video" && matches!(s.encoding_name(), "h264" | "h265"))
        .ok_or_else(|| {
            let seen: Vec<String> = session
                .streams()
                .iter()
                .map(|s| format!("{}/{}", s.media(), s.encoding_name()))
                .collect();
            format!("no H.264/H.265 video stream (offered: {})", seen.join(", "))
        })?;
    let audio_i = session.streams().iter().position(|s| {
        s.media() == "audio"
            && matches!(s.encoding_name(), "pcma" | "pcmu" | "l16" | "mpeg4-generic")
    });

    // Interleaved TCP: one connection, no UDP port juggling, works through
    // the LAN setups cameras actually sit in.
    let setup_opts = || {
        SetupOptions::default()
            .transport(Transport::Tcp(TcpTransportOptions::default()))
            .frame_format(FrameFormat::MP4)
    };
    session
        .setup(video_i, setup_opts())
        .await
        .map_err(|e| format!("SETUP video: {e}"))?;
    if let Some(ai) = audio_i {
        // Audio is best-effort: a camera that refuses SETUP on it still streams video.
        if let Err(e) = session.setup(ai, setup_opts()).await {
            warn!(id = %state.id, error = %e, "SETUP audio failed; continuing video-only");
        }
    }

    let mut demuxed = session
        .play(PlayOptions::default())
        .await
        .map_err(|e| format!("PLAY: {e}"))?
        .demuxed()
        .map_err(|e| format!("demux: {e}"))?;

    let mut audio: Option<AudioPipe> = None;
    let mut init_sent = false;
    let mut idle_since: Option<Instant> = None;
    let mut n_video: u64 = 0;
    let mut n_audio: u64 = 0;
    let mut last_report = Instant::now();
    info!(id = %state.id, "RTSP session playing");
    state.error.send_replace(None);

    loop {
        let item = tokio::time::timeout(STALL_TIMEOUT, demuxed.next())
            .await
            .map_err(|_| format!("no RTP for {STALL_TIMEOUT:?}"))?
            .ok_or_else(|| "RTSP stream ended".to_string())?
            .map_err(|e| format!("RTP: {e}"))?;

        if last_report.elapsed() > Duration::from_secs(5) {
            debug!(id = %state.id, n_video, n_audio, consumers = state.frames.receiver_count(), "RTSP session stats");
            last_report = Instant::now();
        }

        if state.frames.receiver_count() == 0 {
            match idle_since {
                None => idle_since = Some(Instant::now()),
                Some(t) if t.elapsed() > IDLE_TIMEOUT => return Ok(()),
                _ => {}
            }
        } else {
            idle_since = None;
        }

        match item {
            CodecItem::VideoFrame(f) => {
                n_video += 1;
                if !init_sent || f.has_new_parameters() {
                    let Some(ParametersRef::Video(v)) = demuxed.streams()[video_i].parameters()
                    else {
                        if n_video == 1 {
                            warn!(id = %state.id, "video frames arriving without codec parameters yet");
                        }
                        continue;
                    };
                    let (width, height) = v.pixel_dimensions();
                    // Audio params are only certain once its first frame has
                    // arrived; the stream-level clock/channels are enough
                    // for PCM and are what the AudioPipe reports back.
                    let audio_init = audio_i
                        .and_then(|ai| AudioPipe::new(&demuxed.streams()[ai]))
                        .map(|p| {
                            let init = p.init();
                            audio = Some(p);
                            init
                        });
                    let init = Arc::new(StreamInit {
                        kind: "init",
                        video: VideoInit {
                            codec: v.rfc6381_codec().to_string(),
                            description: base64::engine::general_purpose::STANDARD
                                .encode(v.extra_data()),
                            width,
                            height,
                        },
                        audio: audio_init,
                    });
                    info!(id = %state.id, codec = %init.video.codec, width, height, audio = init.audio.is_some(), "stream parameters");
                    let _ = state.init.send(Some(init));
                    init_sent = true;
                }
                let frame = MediaFrame::Video {
                    ts_us: ts_us(f.timestamp()),
                    key: f.is_random_access_point(),
                    data: Bytes::from(f.into_data()),
                };
                let _ = state.frames.send(Arc::new(frame));
            }
            CodecItem::AudioFrame(f) => {
                n_audio += 1;
                if let Some(pipe) = audio.as_mut() {
                    if let Some(pcm) = pipe.decode(f.data()) {
                        let frame = MediaFrame::Audio {
                            ts_us: ts_us(f.timestamp()),
                            pcm: Bytes::from(pcm),
                        };
                        let _ = state.frames.send(Arc::new(frame));
                    }
                }
            }
            _ => {}
        }
    }
}

fn ts_us(t: retina::Timestamp) -> u64 {
    let rate = i128::from(t.clock_rate().get());
    let us = i128::from(t.elapsed()) * 1_000_000 / rate;
    us.max(0) as u64
}

// ── Audio decode (to interleaved s16) ────────────────────────────────────────

enum AudioCodec {
    Pcma,
    Pcmu,
    L16,
    Aac(Box<symphonia::default::codecs::AacDecoder>),
}

struct AudioPipe {
    codec: AudioCodec,
    sample_rate: u32,
    channels: u16,
}

impl AudioPipe {
    fn new(stream: &retina::client::Stream) -> Option<Self> {
        let sample_rate = stream.clock_rate_hz();
        let channels = stream.channels().map(|c| c.get()).unwrap_or(1);
        let codec = match stream.encoding_name() {
            "pcma" => AudioCodec::Pcma,
            "pcmu" => AudioCodec::Pcmu,
            "l16" => AudioCodec::L16,
            "mpeg4-generic" => {
                use symphonia::core::audio::Channels;
                use symphonia::core::codecs::audio::{
                    well_known::CODEC_ID_AAC, AudioCodecParameters, AudioDecoder,
                    AudioDecoderOptions,
                };
                let Some(ParametersRef::Audio(a)) = stream.parameters() else {
                    return None;
                };
                let mut params = AudioCodecParameters::new();
                params
                    .for_codec(CODEC_ID_AAC)
                    .with_sample_rate(a.clock_rate())
                    .with_channels(Channels::Discrete(a.channels().get()))
                    .with_extra_data(a.extra_data().to_vec().into_boxed_slice());
                let dec = symphonia::default::codecs::AacDecoder::try_new(
                    &params,
                    &AudioDecoderOptions::default(),
                )
                .map_err(|e| warn!(error = %e, "AAC decoder init failed; audio disabled"))
                .ok()?;
                let _ = AudioDecoder::codec_params(&dec);
                return Some(Self {
                    codec: AudioCodec::Aac(Box::new(dec)),
                    sample_rate: a.clock_rate(),
                    channels: a.channels().get(),
                });
            }
            _ => return None,
        };
        Some(Self {
            codec,
            sample_rate,
            channels,
        })
    }

    fn init(&self) -> AudioInit {
        AudioInit {
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }

    fn decode(&mut self, data: &[u8]) -> Option<Vec<u8>> {
        match &mut self.codec {
            AudioCodec::Pcma => Some(g711_to_s16(data, alaw_to_i16)),
            AudioCodec::Pcmu => Some(g711_to_s16(data, ulaw_to_i16)),
            // RFC 3551 L16 is big-endian; the wire format is little-endian.
            AudioCodec::L16 => Some(
                data.as_chunks::<2>()
                    .0
                    .iter()
                    .flat_map(|sample| [sample[1], sample[0]])
                    .collect(),
            ),
            AudioCodec::Aac(dec) => {
                use symphonia::core::codecs::audio::AudioDecoder;
                use symphonia::core::packet::PacketRef;
                use symphonia::core::units::{Duration as SDuration, Timestamp};
                let pkt = PacketRef::new(0, Timestamp::new(0), SDuration::ZERO, data);
                let buf = dec
                    .decode_ref(&pkt)
                    .map_err(|e| debug!(error = %e, "AAC decode error"))
                    .ok()?;
                let mut out: Vec<i16> = Vec::with_capacity(buf.samples_interleaved());
                buf.copy_to_vec_interleaved(&mut out);
                Some(out.iter().flat_map(|s| s.to_le_bytes()).collect())
            }
        }
    }
}

fn g711_to_s16(data: &[u8], f: fn(u8) -> i16) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 2);
    for &b in data {
        out.extend_from_slice(&f(b).to_le_bytes());
    }
    out
}

/// ITU-T G.711 A-law expansion.
fn alaw_to_i16(a: u8) -> i16 {
    let a = a ^ 0x55;
    let sign = a & 0x80;
    let exponent = (a >> 4) & 0x07;
    let mantissa = i32::from(a & 0x0F);
    let mut sample = (mantissa << 4) + 8;
    if exponent != 0 {
        sample = (sample + 0x100) << (exponent - 1);
    }
    let sample = sample as i16;
    if sign == 0 {
        -sample
    } else {
        sample
    }
}

/// ITU-T G.711 µ-law expansion.
fn ulaw_to_i16(u: u8) -> i16 {
    let u = !u;
    let sign = u & 0x80;
    let exponent = (u >> 4) & 0x07;
    let mantissa = i32::from(u & 0x0F);
    let sample = (((mantissa << 3) + 0x84) << exponent) - 0x84;
    let sample = sample as i16;
    if sign != 0 {
        -sample
    } else {
        sample
    }
}

// ── WebSocket consumer ───────────────────────────────────────────────────────

/// Serve one player connection. `ws` has already completed its handshake.
pub async fn serve_ws(mut ws: WebSocketStream<TcpStream>, id: &str) -> Result<(), String> {
    let state = registry()
        .get(id)
        .ok_or_else(|| format!("unknown stream {id}"))?;
    info!(id, "ws consumer connected");
    state.ensure_running();
    let (mut frames, mut init_rx) = state.subscribe();
    let mut error_rx = state.error.subscribe();
    // Relay a failure that happened before this player connected, too.
    error_rx.mark_changed();

    let init = loop {
        tokio::select! {
            init = state.wait_init(&mut init_rx) => break init?,
            Ok(()) = error_rx.changed() => {
                let error = error_rx.borrow_and_update().clone();
                if let Some(message) = error {
                    ws.send(error_message(&message))
                        .await
                        .map_err(|e| format!("ws send error: {e}"))?;
                }
            }
        }
    };
    let init_json = serde_json::to_string(&*init).map_err(|e| e.to_string())?;
    ws.send(Message::Text(init_json.into()))
        .await
        .map_err(|e| format!("ws send init: {e}"))?;
    info!(id, "ws consumer sent init");

    let mut audio_enabled = false;
    // Decoders must start on a key frame; also true after a lag or a
    // parameter change.
    let mut want_key = true;

    loop {
        tokio::select! {
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                        if v.get("type").and_then(|t| t.as_str()) == Some("audio") {
                            audio_enabled = v.get("enabled").and_then(|b| b.as_bool()).unwrap_or(false);
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(e)) => { debug!(id, error = %e, "ws read error"); break; }
                _ => {}
            },
            fr = frames.recv() => match fr {
                Ok(f) => {
                    match &*f {
                        MediaFrame::Video { key, .. } => {
                            if want_key && !key { continue; }
                            want_key = false;
                        }
                        MediaFrame::Audio { .. } => {
                            if !audio_enabled { continue; }
                        }
                    }
                    if ws.send(Message::Binary(f.to_wire().into())).await.is_err() { break; }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(id, skipped = n, "ws consumer lagged; waiting for key frame");
                    want_key = true;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            changed = init_rx.changed() => {
                if changed.is_err() { break; }
                let latest = init_rx.borrow_and_update().clone();
                if let Some(i) = latest {
                    let json = serde_json::to_string(&*i).map_err(|e| e.to_string())?;
                    if ws.send(Message::Text(json.into())).await.is_err() { break; }
                    want_key = true;
                }
            }
            Ok(()) = error_rx.changed() => {
                let error = error_rx.borrow_and_update().clone();
                if let Some(message) = error {
                    if ws.send(error_message(&message)).await.is_err() { break; }
                }
            }
        }
    }
    let _ = ws.close(None).await;
    info!(id, "ws consumer disconnected");
    Ok(())
}

fn error_message(message: &str) -> Message {
    Message::Text(
        serde_json::json!({ "type": "error", "message": message })
            .to_string()
            .into(),
    )
}

// ── MJPEG fallback consumer ──────────────────────────────────────────────────

const MULTIPART_BOUNDARY: &str = "oxdm-native-frame";
const FALLBACK_JPEG_QUALITY: u8 = 75;

/// Serve `GET /mjpeg/{id}`: software-decode H.264 and stream JPEGs. `sock`
/// has had its request consumed; nothing has been written yet.
pub async fn serve_mjpeg(mut sock: TcpStream, id: &str) -> Result<(), String> {
    let state = registry()
        .get(id)
        .ok_or_else(|| format!("unknown stream {id}"))?;
    info!(id, "mjpeg consumer connected");
    state.ensure_running();
    let (mut frames, mut init_rx) = state.subscribe();
    let init = state.wait_init(&mut init_rx).await?;
    if !init.video.codec.starts_with("avc1") {
        let body = format!(
            "fallback decoder supports H.264 only (stream is {})",
            init.video.codec
        );
        let resp = format!(
            "HTTP/1.1 415 Unsupported Media Type\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        return Err(body);
    }
    let avcc = base64::engine::general_purpose::STANDARD
        .decode(&init.video.description)
        .map_err(|e| format!("bad avcC: {e}"))?;
    let param_sets = annexb_parameter_sets(&avcc)?;

    let head = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: multipart/x-mixed-replace; boundary={MULTIPART_BOUNDARY}\r\n\
         Cache-Control: no-cache, no-store, must-revalidate\r\n\
         Connection: close\r\n\r\n"
    );
    sock.write_all(head.as_bytes())
        .await
        .map_err(|e| format!("write head: {e}"))?;

    // Decode + encode on a plain thread: OpenH264 is synchronous and a 2K
    // frame takes tens of ms — too long to sit on a tokio worker.
    let (to_dec, from_net) = std::sync::mpsc::sync_channel::<Arc<MediaFrame>>(4);
    let (to_net, mut from_dec) = tokio::sync::mpsc::channel::<Vec<u8>>(2);
    std::thread::Builder::new()
        .name("oxdm-h264-mjpeg".into())
        .spawn(move || decode_thread(from_net, to_net, param_sets))
        .map_err(|e| format!("spawn decoder thread: {e}"))?;

    let mut want_key = true;
    loop {
        tokio::select! {
            fr = frames.recv() => match fr {
                Ok(f) => {
                    if let MediaFrame::Video { key, .. } = &*f {
                        if want_key && !key { continue; }
                        want_key = false;
                        // Drop (and resync on a key frame) rather than queue
                        // if the decoder can't keep up.
                        if to_dec.try_send(f).is_err() { want_key = true; }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => want_key = true,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            jpeg = from_dec.recv() => {
                let Some(jpeg) = jpeg else { break };
                let part = format!(
                    "--{MULTIPART_BOUNDARY}\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                    jpeg.len()
                );
                if sock.write_all(part.as_bytes()).await.is_err()
                    || sock.write_all(&jpeg).await.is_err()
                    || sock.write_all(b"\r\n").await.is_err()
                {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn decode_thread(
    rx: std::sync::mpsc::Receiver<Arc<MediaFrame>>,
    tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    param_sets: Vec<u8>,
) {
    let mut dec = match openh264::decoder::Decoder::new() {
        Ok(d) => d,
        Err(e) => {
            warn!(error = %e, "OpenH264 init failed");
            return;
        }
    };
    let mut annexb = Vec::new();
    let mut rgb = Vec::new();
    while let Ok(frame) = rx.recv() {
        let MediaFrame::Video { key, data, .. } = &*frame else {
            continue;
        };
        annexb.clear();
        if *key {
            annexb.extend_from_slice(&param_sets);
        }
        avcc_to_annexb(data, &mut annexb);
        let yuv = match dec.decode(&annexb) {
            Ok(Some(y)) => y,
            Ok(None) => continue,
            Err(e) => {
                debug!(error = %e, "OpenH264 decode error");
                continue;
            }
        };
        let Some(jpeg) = yuv_to_jpeg(&yuv, &mut rgb, FALLBACK_JPEG_QUALITY) else {
            continue;
        };
        if tx.blocking_send(jpeg).is_err() {
            break;
        }
    }
}

fn yuv_to_jpeg(
    yuv: &openh264::decoder::DecodedYUV<'_>,
    rgb: &mut Vec<u8>,
    quality: u8,
) -> Option<Vec<u8>> {
    use openh264::formats::YUVSource;
    let (w, h) = yuv.dimensions();
    rgb.resize(w * h * 3, 0);
    yuv.write_rgb8(rgb);
    let mut jpeg = Vec::with_capacity(w * h / 4);
    let enc = jpeg_encoder::Encoder::new(&mut jpeg, quality);
    if let Err(e) = enc.encode(rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb) {
        debug!(error = %e, "JPEG encode error");
        return None;
    }
    Some(jpeg)
}

// ── Snapshot ──────────────────────────────────────────────────────────────────────────

/// Decode the next key frame of the device's stream to a JPEG. Opens the
/// stream if nothing is watching it yet (the session then idles out on its
/// own). H.264 only, like the MJPEG fallback.
pub async fn snapshot_jpeg(
    device_addr: &str,
    profile_token: &str,
    creds: &Credentials,
) -> Result<Vec<u8>, String> {
    let src = RtspBackend.open(device_addr, profile_token, creds).await?;
    let state = registry()
        .get(&src.id)
        .ok_or_else(|| format!("unknown stream {}", src.id))?;
    state.ensure_running();
    let (mut frames, mut init_rx) = state.subscribe();
    let init = state.wait_init(&mut init_rx).await?;
    if !init.video.codec.starts_with("avc1") {
        return Err(format!(
            "snapshot decoder supports H.264 only (stream is {})",
            init.video.codec
        ));
    }
    let avcc = base64::engine::general_purpose::STANDARD
        .decode(&init.video.description)
        .map_err(|e| format!("bad avcC: {e}"))?;
    let param_sets = annexb_parameter_sets(&avcc)?;

    let key_frame = tokio::time::timeout(SNAPSHOT_WAIT, async {
        loop {
            match frames.recv().await {
                Ok(f) => {
                    if let MediaFrame::Video { key: true, .. } = &*f {
                        return Ok(f);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return Err("stream closed".to_string()),
            }
        }
    })
    .await
    .map_err(|_| format!("no key frame within {SNAPSHOT_WAIT:?}"))??;

    tokio::task::spawn_blocking(move || {
        let MediaFrame::Video { data, .. } = &*key_frame else {
            unreachable!("filtered to video above");
        };
        let mut annexb = param_sets;
        avcc_to_annexb(data, &mut annexb);
        let mut dec =
            openh264::decoder::Decoder::new().map_err(|e| format!("OpenH264 init: {e}"))?;
        let yuv = match dec.decode(&annexb).map_err(|e| format!("decode: {e}"))? {
            Some(y) => y,
            // A lone IDR may sit in the reorder buffer until flushed.
            None => {
                let mut rest = dec.flush_remaining().map_err(|e| format!("decode: {e}"))?;
                rest.pop().ok_or("decoder produced no picture")?
            }
        };
        let mut rgb = Vec::new();
        yuv_to_jpeg(&yuv, &mut rgb, SNAPSHOT_JPEG_QUALITY)
            .ok_or_else(|| "JPEG encode failed".to_string())
    })
    .await
    .map_err(|e| format!("snapshot task: {e}"))?
}

// ── Recording ─────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct RecordingStatus {
    pub path: PathBuf,
    pub started: Instant,
}

struct Recorder {
    status: RecordingStatus,
    stop: oneshot::Sender<()>,
    done: tokio::task::JoinHandle<Result<(), String>>,
}

static RECORDERS: OnceLock<Mutex<HashMap<String, Recorder>>> = OnceLock::new();

fn recorders() -> &'static Mutex<HashMap<String, Recorder>> {
    RECORDERS.get_or_init(Default::default)
}

/// The stream id `open` assigns to `(device, profile)`; keys the recording API.
pub fn stream_id_for(device_addr: &str, profile_token: &str) -> String {
    stream_name_for(device_addr, profile_token)
}

pub fn recording(stream_id: &str) -> Option<RecordingStatus> {
    recorders()
        .lock()
        .unwrap()
        .get(stream_id)
        .map(|r| r.status.clone())
}

/// Start writing `<dir>/<base_name>-<stamp>.mkv` from an already-opened
/// stream. Returns the path; the recording keeps the session alive until
/// [`stop_recording`], even if every player disconnects.
pub fn start_recording(stream_id: &str, dir: &Path, base_name: &str) -> Result<PathBuf, String> {
    let state = registry()
        .get(stream_id)
        .ok_or_else(|| "stream is not open".to_string())?;
    let mut map = recorders().lock().unwrap();
    if map.contains_key(stream_id) {
        return Err("already recording".to_string());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join(format!("{base_name}-{}.mkv", crate::util::now_file_stamp()));
    let (stop, stop_rx) = oneshot::channel();
    let done = tokio::spawn(record_task(state, path.clone(), stop_rx));
    info!(id = stream_id, path = %path.display(), "recording started");
    map.insert(
        stream_id.to_string(),
        Recorder {
            status: RecordingStatus {
                path: path.clone(),
                started: Instant::now(),
            },
            stop,
            done,
        },
    );
    Ok(path)
}

/// Stop and finalise. Errors from the writer surface here.
pub async fn stop_recording(stream_id: &str) -> Result<RecordingStatus, String> {
    let rec = recorders()
        .lock()
        .unwrap()
        .remove(stream_id)
        .ok_or_else(|| "not recording".to_string())?;
    let _ = rec.stop.send(());
    rec.done
        .await
        .map_err(|e| format!("recorder task: {e}"))??;
    info!(id = stream_id, path = %rec.status.path.display(), secs = rec.status.started.elapsed().as_secs(), "recording stopped");
    Ok(rec.status)
}

async fn record_task(
    state: Arc<StreamState>,
    path: PathBuf,
    mut stop: oneshot::Receiver<()>,
) -> Result<(), String> {
    state.ensure_running();
    let (mut frames, mut init_rx) = state.subscribe();
    let init = state.wait_init(&mut init_rx).await?;
    let config = base64::engine::general_purpose::STANDARD
        .decode(&init.video.description)
        .map_err(|e| format!("bad codec config: {e}"))?;
    let video = mkv::VideoTrack {
        rfc6381_codec: &init.video.codec,
        config_record: &config,
        width: init.video.width,
        height: init.video.height,
    };
    let audio = init.audio.as_ref().map(|a| mkv::AudioTrack {
        sample_rate: a.sample_rate,
        channels: a.channels,
    });
    let mut w = mkv::MkvWriter::create(&path, &video, audio.as_ref())
        .map_err(|e| format!("create {}: {e}", path.display()))?;

    // Recording time zero is the first key frame; earlier audio is dropped.
    let mut base_us: Option<u64> = None;
    let mut want_key = true;
    loop {
        tokio::select! {
            _ = &mut stop => break,
            fr = frames.recv() => match fr {
                Ok(f) => match &*f {
                    MediaFrame::Video { ts_us, key, data } => {
                        if want_key && !key { continue; }
                        want_key = false;
                        let base = *base_us.get_or_insert(*ts_us);
                        w.write_video(ts_us.saturating_sub(base) / 1000, *key, data)
                            .map_err(|e| format!("write: {e}"))?;
                    }
                    MediaFrame::Audio { ts_us, pcm } => {
                        let Some(base) = base_us else { continue };
                        if *ts_us < base { continue; }
                        w.write_audio((ts_us - base) / 1000, pcm)
                            .map_err(|e| format!("write: {e}"))?;
                    }
                },
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(id = %state.id, skipped = n, "recorder lagged; resuming at next key frame");
                    want_key = true;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        }
    }
    w.finish().map_err(|e| format!("finalise: {e}"))
}

/// Convert 4-byte-length-prefixed NAL units to Annex B start codes.
fn avcc_to_annexb(avcc: &[u8], out: &mut Vec<u8>) {
    let mut i = 0;
    while i + 4 <= avcc.len() {
        let len = u32::from_be_bytes([avcc[i], avcc[i + 1], avcc[i + 2], avcc[i + 3]]) as usize;
        i += 4;
        let end = (i + len).min(avcc.len());
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&avcc[i..end]);
        i = end;
    }
}

/// Pull SPS/PPS out of an `AVCDecoderConfigurationRecord` (ISO 14496-15
/// §5.2.4.1) and emit them Annex-B framed, for prepending to key frames.
fn annexb_parameter_sets(avcc: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut i = 5;
    let num_sps = *avcc.get(i).ok_or("avcC truncated")? & 0x1F;
    i += 1;
    for _ in 0..num_sps {
        i = push_ps(avcc, i, &mut out)?;
    }
    let num_pps = *avcc.get(i).ok_or("avcC truncated")?;
    i += 1;
    for _ in 0..num_pps {
        i = push_ps(avcc, i, &mut out)?;
    }
    Ok(out)
}

fn push_ps(avcc: &[u8], i: usize, out: &mut Vec<u8>) -> Result<usize, String> {
    let len = u16::from_be_bytes([
        *avcc.get(i).ok_or("avcC truncated")?,
        *avcc.get(i + 1).ok_or("avcC truncated")?,
    ]) as usize;
    let start = i + 2;
    let end = start + len;
    let nal = avcc.get(start..end).ok_or("avcC truncated")?;
    out.extend_from_slice(&[0, 0, 0, 1]);
    out.extend_from_slice(nal);
    Ok(end)
}

// ── Backend ──────────────────────────────────────────────────────────────────

pub struct RtspBackend;

#[async_trait::async_trait]
impl VideoBackend for RtspBackend {
    fn id(&self) -> &'static str {
        "rtsp"
    }
    fn display_name(&self) -> &'static str {
        "RTSP"
    }
    async fn is_available(&self) -> bool {
        crate::video::server_port().is_some()
    }

    async fn open(
        &self,
        device_addr: &str,
        profile_token: &str,
        creds: &Credentials,
    ) -> Result<VideoSource, String> {
        let stream = crate::api::get_stream_uri(device_addr, creds, profile_token)
            .await
            .map_err(|e| format!("GetStreamUri: {e}"))?;
        let url = rtsp_target(&stream.uri, device_addr);
        let id = stream_name_for(device_addr, profile_token);
        self.source_for(&id, url, creds)
    }

    async fn open_rtsp(
        &self,
        rtsp_url: &str,
        device_addr: &str,
        creds: &Credentials,
    ) -> Result<VideoSource, String> {
        let url = rtsp_target(rtsp_url, device_addr);
        let id = replay_stream_name(&url);
        self.source_for(&id, url, creds)
    }

    async fn close(&self, source_id: &str) {
        registry().remove(source_id);
    }
}

impl RtspBackend {
    fn source_for(
        &self,
        id: &str,
        url: String,
        creds: &Credentials,
    ) -> Result<VideoSource, String> {
        let port = crate::video::server_port().ok_or("video server not running")?;
        registry().get_or_insert(id, url, creds);
        Ok(VideoSource {
            id: id.to_string(),
            url: format!("ws://127.0.0.1:{port}/ws/{id}"),
            embed: EmbedKind::Stream,
        })
    }
}

/// `<img>`-embeddable MJPEG URL for a stream previously returned by `open`.
/// Used by the snapshot backend for cameras with no ONVIF snapshot URI.
pub fn mjpeg_url(stream_id: &str) -> Option<String> {
    crate::video::server_port().map(|p| format!("http://127.0.0.1:{p}/mjpeg/{stream_id}"))
}

// ── URL helpers ──────────────────────────────────────────────────────────────

fn stream_name_for(device_addr: &str, profile_token: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    device_addr.hash(&mut h);
    profile_token.hash(&mut h);
    format!("live-{:016x}", h.finish())
}

fn replay_stream_name(rtsp_url: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    rtsp_url.hash(&mut h);
    format!("replay-{:016x}", h.finish())
}

/// Normalise a camera-supplied RTSP URI into something we can connect to.
///
/// Credentials embedded by the camera are dropped (ours go through RTSP
/// auth, not the URL). The camera's `host:port` is trusted — RTSP and ONVIF
/// live on different ports — except when the host obviously can't be
/// reached from here (empty, `0.0.0.0`, loopback: Hikvision/Dahua do this),
/// in which case only the host is swapped for the device's.
pub(crate) fn rtsp_target(rtsp_uri: &str, device_addr: &str) -> String {
    let after_scheme = rtsp_uri.strip_prefix("rtsp://").unwrap_or(rtsp_uri);
    let after_creds = match after_scheme.find('@') {
        Some(i) => &after_scheme[i + 1..],
        None => after_scheme,
    };
    let (host_port, path) = match after_creds.find('/') {
        Some(i) => (&after_creds[..i], &after_creds[i..]),
        None => (after_creds, ""),
    };
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (host_port, None),
    };
    let host_port = if host_unreachable(host) {
        let fallback = host_only_from_addr(device_addr).unwrap_or_else(|| host.to_string());
        match port {
            Some(p) => format!("{fallback}:{p}"),
            None => fallback,
        }
    } else {
        host_port.to_string()
    };
    format!("rtsp://{host_port}{path}")
}

fn host_unreachable(host: &str) -> bool {
    host.is_empty() || host == "0.0.0.0" || host == "127.0.0.1" || host == "localhost"
}

fn host_only_from_addr(addr: &str) -> Option<String> {
    let stripped = addr
        .strip_prefix("http://")
        .or_else(|| addr.strip_prefix("https://"))
        .unwrap_or(addr);
    let host_port = stripped.split('/').next()?;
    if host_port.is_empty() {
        return None;
    }
    Some(
        host_port
            .rsplit_once(':')
            .map(|(h, _)| h)
            .unwrap_or(host_port)
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_keeps_camera_host_and_path_and_drops_its_creds() {
        assert_eq!(
            rtsp_target(
                "rtsp://other:wrong@cam.local:554/s1",
                "http://192.168.1.10/onvif"
            ),
            "rtsp://cam.local:554/s1"
        );
    }

    #[test]
    fn target_does_not_force_onvif_port_onto_rtsp() {
        assert_eq!(
            rtsp_target(
                "rtsp://192.168.1.10/s1",
                "http://192.168.1.10:80/onvif/device"
            ),
            "rtsp://192.168.1.10/s1"
        );
    }

    #[test]
    fn target_substitutes_host_when_camera_returns_zero_addr() {
        assert_eq!(
            rtsp_target(
                "rtsp://0.0.0.0:8554/live",
                "http://192.168.1.10/onvif/device"
            ),
            "rtsp://192.168.1.10:8554/live"
        );
    }

    #[test]
    fn avcc_nals_become_annexb() {
        let mut out = Vec::new();
        avcc_to_annexb(&[0, 0, 0, 2, 0x65, 0xAA, 0, 0, 0, 1, 0x41], &mut out);
        assert_eq!(out, [0, 0, 0, 1, 0x65, 0xAA, 0, 0, 0, 1, 0x41]);
    }

    #[test]
    fn parameter_sets_are_extracted_from_avcc() {
        // version, profile, compat, level, lengthSizeMinusOne|0xFC, numSPS|0xE0,
        // SPS len=2, SPS, numPPS=1, PPS len=1, PPS
        let avcc = [
            1, 0x64, 0, 0x1F, 0xFF, 0xE1, 0, 2, 0x67, 0x64, 1, 0, 1, 0x68,
        ];
        assert_eq!(
            annexb_parameter_sets(&avcc).unwrap(),
            [0, 0, 0, 1, 0x67, 0x64, 0, 0, 0, 1, 0x68]
        );
    }

    #[test]
    fn g711_silence_decodes_near_zero() {
        // 0xD5 is A-law zero, 0xFF is µ-law zero.
        assert!(alaw_to_i16(0xD5).abs() <= 8);
        assert!(ulaw_to_i16(0xFF).abs() <= 8);
    }

    #[tokio::test]
    async fn a_failing_session_tells_the_player_why() {
        // The listener is dropped at once, so every DESCRIBE is refused.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let id = "test-refused-session";
        registry().get_or_insert(
            id,
            format!("rtsp://127.0.0.1:{port}/stream1"),
            &Credentials::default(),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws_addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_async(sock).await.unwrap();
            let _ = serve_ws(ws, id).await;
        });
        let (mut player, _) = tokio_tungstenite::connect_async(format!("ws://{ws_addr}/"))
            .await
            .unwrap();
        let first = tokio::time::timeout(Duration::from_secs(10), player.next())
            .await
            .expect("an error arrives long before the init wait gives up")
            .unwrap()
            .unwrap();
        let msg: serde_json::Value = serde_json::from_str(first.to_text().unwrap()).unwrap();
        assert_eq!(msg["type"], "error");
        let text = msg["message"].as_str().unwrap();
        assert!(
            text.starts_with("DESCRIBE:") && !text.contains('\n'),
            "{text}"
        );
        registry().remove(id);
    }
}
