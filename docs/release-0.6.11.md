# OxDM 0.6.11

## Changes

- **Theater camera switcher.** Right-click the video in theater mode to open
  a menu listing every device — the active one check-marked. Picking a camera
  switches to it in place, without leaving theater mode.
- **Theater survives camera switches.** Theater state is global, so the live
  view remount keeps it on instead of restoring the window chrome mid-switch.

## Downloads

| Platform | Asset |
|----------|-------|
| macOS (Apple Silicon) | `oxdm-0.6.11-macos-aarch64.dmg` |
| Windows (x86_64) installer | `oxdm-0.6.11-windows-x86_64.msi` |
| Windows (x86_64) portable | `oxdm-0.6.11-windows-x86_64-portable.zip` |
| Ubuntu 24.04+ (x86_64) | `oxdm-0.6.11-ubuntu-x86_64.deb` |
| Arch Linux (x86_64) | `oxdm-0.6.11-1-x86_64.pkg.tar.zst` and `.sha256` |

Bundles are not developer-signed or notarized. Windows builds require the
WebView2 runtime. Linux decoding depends on system WebKitGTK/GStreamer and
compatible codecs/drivers; see the README for setup guidance.

For Arch Linux, download the package and checksum, then run:

```sh
sha256sum -c oxdm-0.6.11-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.11-1-x86_64.pkg.tar.zst
```

For Ubuntu 24.04+:

```sh
sudo apt install ./oxdm-0.6.11-ubuntu-x86_64.deb
```

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.11/CHANGELOG.md
