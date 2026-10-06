# OxDM 0.6.10

## Changes

- **First-launch reliability.** Devices restored from the previous session are
  probed as soon as credentials unlock, so sidebar thumbnails appear without a
  restart. Failed profile loads, video opens, image streams, and thumbnail
  refreshes retry automatically every few seconds, with a manual retry button
  on video errors.
- **Credential changes apply live.** Editing device credentials updates running
  RTSP streams in place — no stream replacement, no interruption for other
  viewers of the same stream.
- **Fullscreen and theater mode reworked.** Double-clicking live video toggles
  OS fullscreen; fullscreen implies theater mode, and leaving fullscreen
  restores the theater state from before entering. The theater exit button and
  Escape always leave theater mode. Theater mode now hides the window titlebar,
  and dragging the video moves the window.

## Downloads

| Platform | Asset |
|----------|-------|
| macOS (Apple Silicon) | `oxdm-0.6.10-macos-aarch64.dmg` |
| Windows (x86_64) installer | `oxdm-0.6.10-windows-x86_64.msi` |
| Windows (x86_64) portable | `oxdm-0.6.10-windows-x86_64-portable.zip` |
| Ubuntu 24.04+ (x86_64) | `oxdm-0.6.10-ubuntu-x86_64.deb` |
| Arch Linux (x86_64) | `oxdm-0.6.10-1-x86_64.pkg.tar.zst` and `.sha256` |

Bundles are not developer-signed or notarized. Windows builds require the
WebView2 runtime. Linux decoding depends on system WebKitGTK/GStreamer and
compatible codecs/drivers; see the README for setup guidance.

For Arch Linux, download the package and checksum, then run:

```sh
sha256sum -c oxdm-0.6.10-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.10-1-x86_64.pkg.tar.zst
```

For Ubuntu 24.04+:

```sh
sudo apt install ./oxdm-0.6.10-ubuntu-x86_64.deb
```

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.10/CHANGELOG.md
