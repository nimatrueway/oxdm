# OxDM 0.6.8

## Changes

- WebGL2 is now the default renderer for WebCodecs video in live view,
  picture-in-picture, and recording replay. It uploads decoded frames directly
  as textures instead of drawing through Canvas 2D.
- Source resolution, playback scheduling, digital zoom, audio, and decoder
  selection are unchanged.
- Unsupported WebGL2, shader/upload failures, and context loss automatically
  fall back to Canvas 2D without switching the decoder. Set
  `OXDM_VIDEO_RENDERER=2d` before launching to force the previous renderer;
  `OXDM_VIDEO_RENDERER=webgl2` explicitly selects the new default.
- WebView console statistics report the active renderer, and automatic
  renderer fallback emits a warning.

## Performance and compatibility

A paired 30-second 1080p camera comparison on an Apple M2 Max measured combined
app and primary WebKit CPU use at 27.8% of one core with Canvas 2D versus 5.9%
with WebGL2, approximately 79% lower in that pair. This is a single-system
measurement, not a guaranteed improvement on every platform.

Final WebKit pixel comparisons covered orientation, colors, cropping,
downscaling, BT.709 conversion, and 4K-wide detail; the maximum difference
from Canvas 2D was one 8-bit channel value. Displayed frame rate and end-to-end
latency were not independently measured. Shader precision was increased after
the CPU comparison to protect high-resolution detail; CPU was not remeasured
after that change. WebGL2 is not a guarantee of zero-copy rendering or hardware
video decoding. If the graphics context is lost while paused, the picture
returns when playback resumes.

## Downloads

| Platform | Asset |
|----------|-------|
| macOS (Apple Silicon) | `oxdm-0.6.8-macos-aarch64.dmg` |
| Windows (x86_64) installer | `oxdm-0.6.8-windows-x86_64.msi` |
| Windows (x86_64) portable | `oxdm-0.6.8-windows-x86_64-portable.zip` |
| Ubuntu 24.04+ (x86_64) | `oxdm-0.6.8-ubuntu-x86_64.deb` |
| Arch Linux (x86_64) | `oxdm-0.6.8-1-x86_64.pkg.tar.zst` and `.sha256` |

Bundles are not developer-signed or notarized. Windows builds require the
WebView2 runtime. Linux decoding still depends on system WebKitGTK/GStreamer
and compatible codecs/drivers; see the README for setup guidance.

For Arch Linux, download the package and checksum, then run:

```sh
sha256sum -c oxdm-0.6.8-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.8-1-x86_64.pkg.tar.zst
```

For Ubuntu 24.04+:

```sh
sudo apt install ./oxdm-0.6.8-ubuntu-x86_64.deb
```

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.8/CHANGELOG.md
