# OxDM 0.6.3

## Changes

- Native Arch Linux packages are available directly from GitHub Releases;
  no AUR account or source build is required to install.
- A repository `PKGBUILD` supports local Arch builds and runs the existing tests.
- In Hyprland sessions, the main window omits the GTK title bar and generic
  Window/Edit menu. The camera toolbar remains available; other desktops retain
  native chrome.
- Retina source downloads are checksum-verified.

## Arch Linux (x86_64)

Download the package and its checksum file, then run:

```sh
sha256sum -c oxdm-0.6.3-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.3-1-x86_64.pkg.tar.zst
```

Launch **OxDM** from your app launcher or run `oxdm`. Pacman installs the
required system libraries. Credentials require a running Secret Service
provider, such as GNOME Keyring or KeePassXC.

If an older user-local installation shadows the package, verify `/usr/bin/oxdm`
before removing its old `~/.local/bin/oxdm` or `~/.cargo/bin/oxdm` launcher and
user desktop entry. Keep `~/.oxdm` to preserve cameras and preferences.

The other platform bundles retain their usual installation process:
macOS Apple Silicon `.dmg`, Windows x86_64 `.msi` or portable `.zip`, and
Ubuntu 24.04+ `.deb`. Bundles are not code-signed.

Full changelog: https://github.com/nimatrueway/oxdm/blob/v0.6.3/CHANGELOG.md
