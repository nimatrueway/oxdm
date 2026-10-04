# OxDM 0.6.9

## Changes

- Decoder details are now **hidden by default**. Enable **App settings >
  Live video > Show video decoder details** when you want the diagnostic overlay.
- The preference is saved across restarts and included in settings backups.
  Existing configurations and older backups default to hidden.
- The same setting controls live video, snapshots, recording replay, and both
  existing and newly opened picture-in-picture windows.
- The decoder overlay no longer has a close button. Changing the setting updates
  visibility without replacing the player or restarting decoding.
- Playback errors remain visible regardless of this setting. WebGL2 remains
  the default renderer, with automatic Canvas 2D fallback and the existing
  `OXDM_VIDEO_RENDERER=2d` override.

## Downloads

| Platform | Asset |
|----------|-------|
| macOS (Apple Silicon) | `oxdm-0.6.9-macos-aarch64.dmg` |
| Windows (x86_64) installer | `oxdm-0.6.9-windows-x86_64.msi` |
| Windows (x86_64) portable | `oxdm-0.6.9-windows-x86_64-portable.zip` |
| Ubuntu 24.04+ (x86_64) | `oxdm-0.6.9-ubuntu-x86_64.deb` |
| Arch Linux (x86_64) | `oxdm-0.6.9-1-x86_64.pkg.tar.zst` and `.sha256` |

Bundles are not developer-signed or notarized. Windows builds require the
WebView2 runtime. Linux decoding depends on system WebKitGTK/GStreamer and
compatible codecs/drivers; see the README for setup guidance.

For Arch Linux, download the package and checksum, then run:

```sh
sha256sum -c oxdm-0.6.9-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.9-1-x86_64.pkg.tar.zst
```

For Ubuntu 24.04+:

```sh
sudo apt install ./oxdm-0.6.9-ubuntu-x86_64.deb
```

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.9/CHANGELOG.md
