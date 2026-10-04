use crate::i18n;
use crate::state::Locale;
use crate::video::stream_element_html;

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
        assert_eq!(labels["dismiss"], i18n::t(locale, "video_decode_dismiss"));
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
