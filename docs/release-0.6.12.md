# OxDM 0.6.12

## Changes

- **Zoom stays put across fullscreen and resizes.** The pan position of
  zoomed live video (live, PiP, and recording replay) is stored as a ratio of
  the frame instead of absolute pixels and re-applied when the frame changes
  size, so the region you were looking at stays under the pointer when
  entering or leaving fullscreen or resizing the window.

## Downloads

| Platform | Asset |
|----------|-------|
| macOS (Apple Silicon) | `oxdm-0.6.12-macos-aarch64.dmg` |
| Windows (x86_64) installer | `oxdm-0.6.12-windows-x86_64.msi` |
| Windows (x86_64) portable | `oxdm-0.6.12-windows-x86_64-portable.zip` |
| Ubuntu 24.04+ (x86_64) | `oxdm-0.6.12-ubuntu-x86_64.deb` |
| Arch Linux (x86_64) | `oxdm-0.6.12-1-x86_64.pkg.tar.zst` and `.sha256` |

Bundles are not developer-signed or notarized. Windows builds require the
WebView2 runtime. Linux decoding depends on system WebKitGTK/GStreamer and
compatible codecs/drivers; see the README for setup guidance.

For Arch Linux, download the package and checksum, then run:

```sh
sha256sum -c oxdm-0.6.12-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.12-1-x86_64.pkg.tar.zst
```

For Ubuntu 24.04+:

```sh
sudo apt install ./oxdm-0.6.12-ubuntu-x86_64.deb
```

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.12/CHANGELOG.md
