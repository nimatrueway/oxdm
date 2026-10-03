# OxDM 0.6.4

## Changes

- A theater-mode button in the live toolbar fills the application window
  with the active live video. Navigation, the toolbar, camera controls, and PTZ
  overlays are hidden without restarting playback.
- Hover over the video to reveal the exit button in the top-right corner and
  restore the previous layout.
- Theater mode works with both RTSP and Snapshot playback and does not change
  the operating system's fullscreen state.
- Theater-mode labels are available in English, Traditional Chinese, and Russian.
- A compact live toolbar keeps Snapshot, Record, and Theater visible while
  grouping picture-in-picture, Recordings, Camera settings, and playback options
  in the More options menu. Click outside the menu or press Escape to dismiss it.

## Installation

Download the bundle for your platform from this release: Arch Linux x86_64
`.pkg.tar.zst`, Ubuntu 24.04+ x86_64 `.deb`, macOS Apple Silicon `.dmg`, or
Windows x86_64 `.msi` / portable `.zip`. Bundles are not code-signed.

For Arch Linux, download the package and checksum, then run:

```sh
sha256sum -c oxdm-0.6.4-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.4-1-x86_64.pkg.tar.zst
```

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.4/CHANGELOG.md
