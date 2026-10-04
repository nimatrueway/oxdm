# OxDM 0.6.7

## Changes

- Theater mode now requests always-on-top stacking through the native
  cross-platform window API. Exiting theater mode or leaving the live view
  restores normal window stacking and the normal workspace minimum size.
  Some Linux compositors, including Hyprland, do not honor the stacking request.
- Playback badges identify camera snapshots, WebCodecs decoding preferences,
  and software H.264 → MJPEG transcoding. The close button hides the badge for
  the current stream without interrupting video or audio.
- Linux fallback tooltips include GStreamer codec installation and verification
  guidance. Hardware preference is not presented as proof of GPU utilization.
- Software fallback uses runtime-detected SIMD JPEG encoding on supported CPUs
  and no longer sends unused compressed video over WebSocket. Frame rate,
  resolution, JPEG quality, and color conversion remain unchanged.
- MJPEG pause/resume restores the image, stale decoder-capability failures are
  ignored after teardown, and re-enabled WebSocket video waits for a keyframe.

## Installation

Download the bundle for your platform: Arch Linux x86_64 `.pkg.tar.zst`,
Ubuntu 24.04+ x86_64 `.deb`, macOS Apple Silicon `.dmg`, or Windows x86_64
`.msi` / portable `.zip`. Bundles are not code-signed.

For Arch Linux, download the package and checksum, then run:

```sh
sha256sum -c oxdm-0.6.7-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.7-1-x86_64.pkg.tar.zst
```

For Ubuntu 24.04+:

```sh
sudo apt install ./oxdm-0.6.7-ubuntu-x86_64.deb
```

## Linux video decoding

WebKitGTK needs GStreamer H.264 decoder plugins in addition to a compatible GPU
driver. If playback uses the software MJPEG fallback, install the missing codecs:

```sh
# Arch Linux
sudo pacman -S gst-plugin-va gst-libav

# Debian/Ubuntu
sudo apt install gstreamer1.0-plugins-bad gstreamer1.0-libav
```

Check `gst-inspect-1.0 vah264dec` (hardware) and
`gst-inspect-1.0 avdec_h264` (software), then restart OxDM. Hardware decoder
names vary with distribution and version. Missing codecs are one possible
fallback cause; WebKit version and stream compatibility also matter.

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.7/CHANGELOG.md
