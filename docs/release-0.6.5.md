# OxDM 0.6.5

## Fixes

- The Unmute button remains available for RTSP streams with audio when video
  falls back to MJPEG on systems without WebCodecs. Audio still starts muted
  and is enabled by clicking Unmute.
- Theater mode allows resizing the window down to 320×180. Exiting theater
  mode or leaving the live view restores the normal workspace minimum of
  900×500, expanding the window only if necessary.

This release also includes the v0.6.4 theater mode and compact live toolbar.
Snapshot, Record, and Theater stay visible; picture-in-picture, Recordings,
Camera settings, and playback options are in the More options menu.

## Installation

Download the bundle for your platform: Arch Linux x86_64 `.pkg.tar.zst`,
Ubuntu 24.04+ x86_64 `.deb`, macOS Apple Silicon `.dmg`, or Windows x86_64
`.msi` / portable `.zip`. Bundles are not code-signed.

For Arch Linux, download the package and checksum, then run:

```sh
sha256sum -c oxdm-0.6.5-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.5-1-x86_64.pkg.tar.zst
```

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.5/CHANGELOG.md
