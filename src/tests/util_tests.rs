use crate::components::credentials_dialog::normalize_onvif_addr;
use crate::util::{extract_ip, save_snapshot, urldecode};
use crate::views::settings::identification::{scope_key, scope_value};
use crate::views::settings::time::epoch_days_to_ymd;

#[test]
fn normalize_bare_ip() {
    assert_eq!(
        normalize_onvif_addr("192.168.1.10"),
        "http://192.168.1.10/onvif/device_service"
    );
}

#[test]
fn normalize_ip_with_port() {
    assert_eq!(
        normalize_onvif_addr("192.168.1.10:8080"),
        "http://192.168.1.10:8080/onvif/device_service"
    );
}

#[test]
fn normalize_http_no_path() {
    assert_eq!(
        normalize_onvif_addr("http://192.168.1.10"),
        "http://192.168.1.10/onvif/device_service"
    );
}

#[test]
fn normalize_full_url_preserved() {
    let url = "http://192.168.1.10/onvif/device_service";
    assert_eq!(normalize_onvif_addr(url), url);
}

#[test]
fn normalize_custom_path_preserved() {
    let url = "http://192.168.1.10/custom/path";
    assert_eq!(normalize_onvif_addr(url), url);
}

#[test]
fn normalize_empty() {
    assert_eq!(normalize_onvif_addr(""), "");
}

#[test]
fn normalize_whitespace() {
    assert_eq!(
        normalize_onvif_addr("  192.168.1.10  "),
        "http://192.168.1.10/onvif/device_service"
    );
}

#[test]
fn extract_ip_from_http_url() {
    assert_eq!(
        extract_ip("http://192.168.1.10/onvif/device_service"),
        "192.168.1.10"
    );
}

#[test]
fn extract_ip_from_https_url() {
    assert_eq!(
        extract_ip("https://10.0.0.5/onvif/device_service"),
        "10.0.0.5"
    );
}

#[test]
fn extract_ip_with_port() {
    assert_eq!(
        extract_ip("http://192.168.1.10:8080/onvif/device_service"),
        "192.168.1.10"
    );
}

#[test]
fn extract_ip_bare_ip() {
    assert_eq!(extract_ip("192.168.1.10"), "192.168.1.10");
}

#[test]
fn extract_ip_empty_string() {
    assert_eq!(extract_ip(""), "");
}

#[test]
fn scope_parsing_standard() {
    let s = "onvif://www.onvif.org/type/video_encoder";
    assert_eq!(scope_key(s), "type");
    assert_eq!(scope_value(s), "video_encoder");
}

#[test]
fn scope_parsing_name() {
    let s = "onvif://www.onvif.org/name/MyCamera";
    assert_eq!(scope_key(s), "name");
    assert_eq!(scope_value(s), "MyCamera");
}

#[test]
fn scope_parsing_unknown_format() {
    let s = "some-random-scope";
    assert_eq!(scope_key(s), "scope");
    assert_eq!(scope_value(s), "some-random-scope");
}

#[test]
fn urldecode_multibyte_utf8() {
    assert_eq!(urldecode("caf%C3%A9"), "café");
}

#[test]
fn epoch_days_unix_epoch() {
    // 1970-01-01
    assert_eq!(epoch_days_to_ymd(0), (1970, 1, 1));
}

#[test]
fn epoch_days_known_date() {
    // 2024-01-01 is day 19723 from epoch
    assert_eq!(epoch_days_to_ymd(19723), (2024, 1, 1));
}

#[test]
fn epoch_days_leap_year() {
    // 2024-02-29 is day 19723 + 59 = 19782
    assert_eq!(epoch_days_to_ymd(19782), (2024, 2, 29));
}

#[test]
fn snapshot_saves_create_the_folder_without_overwriting_captures() -> std::io::Result<()> {
    let root = std::env::temp_dir().join(format!(
        "oxdm-snapshot-test-{}-{}",
        std::process::id(),
        time::OffsetDateTime::now_utc().unix_timestamp_nanos(),
    ));
    let directory = root.join("custom snapshots");
    let first = save_snapshot(&directory, "camera/front:main", b"first capture")?;
    let second = save_snapshot(&directory, "camera/front:main", b"second capture")?;

    assert_ne!(first, second);
    assert_eq!(first.parent(), Some(directory.as_path()));
    assert_eq!(second.parent(), Some(directory.as_path()));
    assert!(first
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("camera_front_main-"));
    assert_eq!(std::fs::read(first)?, b"first capture");
    assert_eq!(std::fs::read(second)?, b"second capture");

    let blocked = root.join("not a directory");
    std::fs::write(&blocked, b"existing file")?;
    assert!(save_snapshot(&blocked, "camera", b"capture").is_err());
    assert_eq!(std::fs::read(blocked)?, b"existing file");
    std::fs::remove_dir_all(root)
}
