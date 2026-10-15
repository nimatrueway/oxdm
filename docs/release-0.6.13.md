# OxDM 0.6.13

## Changes

- **Drag the window from anywhere sensible.** Empty areas of the topbar, the
  device-list sidebar, the live toolbar, and the video itself now move the
  window — in windowed mode as well as theater. The drag is handed to the
  OS-native window mover (with a throttled fallback), so fast flicks track
  the cursor at display rate and across monitors without stalling.
- **Double right-click toggles theater.** A quick double right-click on the
  video switches between theater and windowed mode. The theater camera
  switcher now opens after a short delay on a single right-click, so the
  toggle never flashes the menu.
- **No more stray developer menu.** Right-clicking the video no longer pops
  the webview's Reload / Inspect Element menu; text fields keep the native
  Cut/Copy/Paste menu.

## Downloads

| Platform | Asset |
|----------|-------|
| macOS (Apple Silicon) | `oxdm-0.6.13-macos-aarch64.dmg` |
| Windows (x86_64) installer | `oxdm-0.6.13-windows-x86_64.msi` |
| Windows (x86_64) portable | `oxdm-0.6.13-windows-x86_64-portable.zip` |
| Ubuntu 24.04+ (x86_64) | `oxdm-0.6.13-ubuntu-x86_64.deb` |
| Arch Linux (x86_64) | `oxdm-0.6.13-1-x86_64.pkg.tar.zst` and `.sha256` |

Bundles are not developer-signed or notarized. Windows builds require the
WebView2 runtime. Linux decoding depends on system WebKitGTK/GStreamer and
compatible codecs/drivers; see the README for setup guidance.

For Arch Linux, download the package and checksum, then run:

```sh
sha256sum -c oxdm-0.6.13-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.13-1-x86_64.pkg.tar.zst
```

For Ubuntu 24.04+:

```sh
sudo apt install ./oxdm-0.6.13-ubuntu-x86_64.deb
```

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.13/CHANGELOG.md
