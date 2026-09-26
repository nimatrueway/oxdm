# OxDM 0.6.1

A camera-first workspace with compact live-video controls and fewer repeated
requests to cameras that do not support optional ONVIF operations. Built on
oxvif 0.16.0, with the Rust 1.98 CI compatibility fix included.

## Changes

- Two-pane workspace with a collapsible, resizable camera sidebar and configurable
  thumbnails. The collapsed state is remembered across restarts.
- App settings for appearance, language, credentials, capture folders, TLS, and
  logging. Credentials remain in the system keychain.
- Compact PTZ and image controls share the live player. Diagnostics is under
  Settings; Settings and Recordings have back-to-video buttons.
- Hold-and-drag panning uses the same saved speed as the PTZ panel, suppresses
  duplicate movement commands, and stops on release. An overlay compares the
  starting and current camera-reported position and remaining travel.
- Snapshots save immediately to the configured folder with unique filenames.
  The recording folder is configurable too.
- Closing PiP returns to the main window, with improved toolbar drag areas.
- Unsupported `GetSnapshotUri` and `GetImagingStatus` replies are remembered per
  camera, credentials, and profile/video source. Concurrent callers share the
  decision, so unsupported snapshot paths can use the existing RTSP fallback
  without repeatedly querying the camera. Authentication and transient failures
  remain retryable; decisions reset with session invalidation or app restart.
- The L16 audio byte swap now uses fixed-size sample chunks for Rust 1.98 Clippy
  compatibility, with no change to decoded audio.

## Downloads

| Platform | File |
|---|---|
| macOS (Apple Silicon) | `oxdm-0.6.1-macos-aarch64.dmg` |
| Windows x86-64 installer | `oxdm-0.6.1-windows-x86_64.msi` |
| Windows x86-64 portable | `oxdm-0.6.1-windows-x86_64-portable.zip` |
| Ubuntu 24.04+ x86-64 | `oxdm-0.6.1-ubuntu-x86_64.deb` |

The macOS and Windows builds are not signed with trusted publisher certificates.
The macOS app is not notarized. On macOS, approve the first launch or clear the
quarantine flag on the installed app. Windows requires the WebView2 runtime.
Linux packages require system WebKitGTK; install the `.deb` with a tool that
resolves dependencies, such as `sudo apt install ./oxdm-0.6.1-ubuntu-x86_64.deb`.

Full changelog: [CHANGELOG.md](https://github.com/nimatrueway/oxdm/blob/v0.6.1/CHANGELOG.md).