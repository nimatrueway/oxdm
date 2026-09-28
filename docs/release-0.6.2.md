# OxDM 0.6.2

Digital zoom for the video picture, clearer feedback when a camera refuses its
stream, and lighter sidebar thumbnails for cameras without a snapshot URI.
Built on oxvif 0.16.0.

## Changes

- Pinch the trackpad to zoom the video picture up to 8× around the pointer, in
  the live view, PiP, and recording replay. Two-finger scroll pans while zoomed,
  and the zoom factor shows beside the mute button. The zoom is digital only;
  no camera zoom command is sent.
- When a camera refuses or drops an RTSP stream, the player now shows the
  camera session's error instead of a black frame. The next successful start
  clears it.
- Sidebar thumbnails for cameras without an ONVIF snapshot URI (such as Tapo)
  now decode key frames from the lens's smallest H.264 stream instead of its
  main stream, leaving the main stream to viewers.

## Downloads

| Platform | File |
|---|---|
| macOS (Apple Silicon) | `oxdm-0.6.2-macos-aarch64.dmg` |
| Windows x86-64 installer | `oxdm-0.6.2-windows-x86_64.msi` |
| Windows x86-64 portable | `oxdm-0.6.2-windows-x86_64-portable.zip` |
| Ubuntu 24.04+ x86-64 | `oxdm-0.6.2-ubuntu-x86_64.deb` |

The macOS and Windows builds are not signed with trusted publisher certificates.
The macOS app is not notarized. On macOS, approve the first launch or clear the
quarantine flag on the installed app. Windows requires the WebView2 runtime.
Linux packages require system WebKitGTK; install the `.deb` with a tool that
resolves dependencies, such as `sudo apt install ./oxdm-0.6.2-ubuntu-x86_64.deb`.

Full changelog: [CHANGELOG.md](https://github.com/nimatrueway/oxdm/blob/v0.6.2/CHANGELOG.md).
