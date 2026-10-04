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
    }
}
