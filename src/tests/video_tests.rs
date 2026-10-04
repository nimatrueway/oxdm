use crate::i18n;
use crate::state::Locale;
use crate::video::stream_element_html;

#[tokio::test]
async fn video_details_update_independent_windows_without_replacing_players() {
    use crate::video::{EmbedKind, VideoSource, SHOW_VIDEO_DETAILS};
    use crate::views::live_video::{VideoPlayer, VideoPlayerProps};
    use dioxus::dioxus_core::{AttributeValue, Mutation, Mutations, VirtualDom};

    fn details(mutations: &Mutations) -> Option<&str> {
        mutations.edits.iter().find_map(|edit| match edit {
            Mutation::SetAttribute {
                name: "data-video-details",
                value: AttributeValue::Text(value),
                ..
            } => Some(value.as_str()),
            _ => None,
        })
    }

    async fn wait_for_details(dom: &mut VirtualDom, expected: &str) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                dom.wait_for_work().await;
                let mutations = dom.render_immediate_to_vec();
                assert!(mutations.edits.iter().all(|edit| matches!(
                    edit,
                    Mutation::SetAttribute {
                        name: "data-video-details",
                        ..
                    }
                )));
                if details(&mutations) == Some(expected) {
                    break;
                }
            }
        })
        .await
        .expect("video-details preference did not reach the player");
    }

    SHOW_VIDEO_DETAILS.send_replace(false);
    let mut windows: Vec<_> = [EmbedKind::Stream, EmbedKind::Img, EmbedKind::SoftwareMjpeg]
        .into_iter()
        .map(|embed| {
            let mut dom = VirtualDom::new_with_props(
                VideoPlayer,
                VideoPlayerProps {
                    source: VideoSource {
                        id: "test".into(),
                        url: "http://127.0.0.1/test".into(),
                        embed,
                    },
                    locale: Locale::En,
                },
            );
            assert_eq!(details(&dom.rebuild_to_vec()), Some("false"));
            dom
        })
        .collect();
    for enabled in [true, false] {
        SHOW_VIDEO_DETAILS.send_replace(enabled);
        for dom in &mut windows {
            wait_for_details(dom, if enabled { "true" } else { "false" }).await;
        }
    }
}

#[test]
fn stream_markup_selects_configured_renderer() {
    let expected = match std::env::var("OXDM_VIDEO_RENDERER") {
        Err(std::env::VarError::NotPresent) => "webgl2",
        Ok(value) if value == "webgl2" => "webgl2",
        _ => "2d",
    };
    let html = stream_element_html("ws://127.0.0.1/ws/test", Locale::En);
    assert!(html.contains(&format!("data-renderer=\"{expected}\"")));
}

#[test]
fn stream_markup_escapes_urls_and_localizes_decode_status() {
    for locale in [Locale::En, Locale::ZhTw, Locale::Ru] {
        let html = stream_element_html("ws://127.0.0.1/ws/a?x=\"<&>", locale);
        assert!(html.contains("src=\"ws://127.0.0.1/ws/a?x=&quot;&lt;&amp;&gt;\""));
        let encoded_labels = html
            .split("data-labels=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let labels: serde_json::Value = serde_json::from_str(
            &encoded_labels
                .replace("&quot;", "\"")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&"),
        )
        .unwrap();
        for (path, key) in [
            ("waiting", "video_decode_waiting"),
            ("paused", "video_decode_paused"),
            ("hardware-preferred", "video_decode_hardware_preferred"),
            ("automatic", "video_decode_automatic"),
            ("software-mjpeg", "video_decode_software_mjpeg"),
        ] {
            assert_eq!(labels[path]["label"], i18n::t(locale, key));
            assert!(!labels[path]["title"].as_str().unwrap().is_empty());
        }
        assert!(labels.get("dismiss").is_none());
        assert_eq!(
            labels["software-mjpeg"]["title"],
            crate::video::software_decode_hint(locale)
        );
    }
}

#[test]
fn software_decode_guidance_is_localized_and_platform_specific() {
    for locale in [Locale::En, Locale::ZhTw, Locale::Ru] {
        let hint = crate::video::software_decode_hint(locale);
        assert!(hint.starts_with(i18n::t(locale, "video_decode_software_mjpeg_hint")));
        #[cfg(target_os = "linux")]
        {
            assert!(hint.ends_with(i18n::t(locale, "video_decode_linux_hint")));
            for command in [
                "sudo pacman -S gst-plugin-va gst-libav",
                "sudo apt install gstreamer1.0-plugins-bad gstreamer1.0-libav",
                "gst-inspect-1.0 vah264dec",
                "gst-inspect-1.0 avdec_h264",
            ] {
                assert!(hint.contains(command), "{locale:?}: {command}");
            }
        }
        #[cfg(not(target_os = "linux"))]
        assert_eq!(hint, i18n::t(locale, "video_decode_software_mjpeg_hint"));
    }
}
