# OxDM 0.6.6

## Changes

- Theater mode fills the application window with the active video, hiding
  navigation, camera controls, and PTZ overlays without restarting playback.
  Hover over the video to reveal the exit button.
- Theater mode allows resizing down to 320×180. Exiting restores the normal
  900×500 workspace minimum, expanding the window only if necessary.
- The compact toolbar keeps Snapshot, Record, and Theater visible.
  Picture-in-picture, Recordings, Camera settings, and playback options are
  grouped in the More options menu.
- Unmute remains available for RTSP streams with audio when video falls back
  to MJPEG on systems without WebCodecs. Audio starts muted.
- Linux packages now install the GStreamer audio-output plugin required by
  WebKit automatically.

## Installation

Download the bundle for your platform: Arch Linux x86_64 `.pkg.tar.zst`,
Ubuntu 24.04+ x86_64 `.deb`, macOS Apple Silicon `.dmg`, or Windows x86_64
`.msi` / portable `.zip`. Bundles are not code-signed.

For Arch Linux, download the package and checksum, then run:

```sh
sha256sum -c oxdm-0.6.6-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.6-1-x86_64.pkg.tar.zst
```

If using a source-built or older user-local Linux installation, install
`gst-plugins-good` on Arch or `gstreamer1.0-plugins-good` on Ubuntu/Debian
for WebKit audio output.

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.6/CHANGELOG.md
