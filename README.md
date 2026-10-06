# OxDM — ONVIF Device Manager

**OxDM** (*oxvif Device Manager*) is a modern, cross-platform ONVIF IP camera
manager — a contemporary successor to the classic **ONVIF Device Manager
(ODM)**. It is written in Rust with [Dioxus](https://dioxuslabs.com/) and built
on the [`oxvif`](https://github.com/smiti1642/oxvif) ONVIF client library.

![OxDM managing an ONVIF camera — device list, profile panel, and the device identification settings tab](https://raw.githubusercontent.com/smiti1642/oxdm/main/docs/screenshot.png)

> **Project status — pre-release (v0.6.9).** Built on oxvif 0.16.0. Core device management works
> end-to-end against real cameras and the `oxvif` mock server. Release bundles
> are not yet code-signed, so the operating system may warn about an
> unidentified developer on first launch.

Version 0.6.0 introduces a camera-first workspace, compact PTZ controls,
saved PTZ speed and sidebar collapse preferences, and learned support for
optional snapshot and imaging-status operations.
See the [changelog](./CHANGELOG.md) for details.

## Contents

- [Installation](#installation)
- [Features](#features)
- [Diagnostics — the ONVIF health check](#diagnostics--the-onvif-health-check)
- [Camera clones and the Quirks diff](#camera-clones-and-the-quirks-diff)
- [Trying it without a camera](#trying-it-without-a-camera)
- [Development](#development)

## Installation

### Prebuilt bundles

Bundles for each release are attached to the corresponding
[GitHub Release](https://github.com/nimatrueway/oxdm/releases):

| Platform | Asset | Notes |
|----------|-------|-------|
| macOS (Apple Silicon) | `oxdm-<version>-macos-aarch64.dmg` | `aarch64` only |
| Windows (x86-64) — installer | `oxdm-<version>-windows-x86_64.msi` | Start-menu shortcut |
| Windows (x86-64) — portable | `oxdm-<version>-windows-x86_64-portable.zip` | unzip and run `oxdm.exe` |
| Linux — Ubuntu / Debian (x86-64) | `oxdm-<version>-ubuntu-x86_64.deb` | `sudo apt install ./<file>.deb` |
| Linux — Arch (x86-64) | `oxdm-<version>-1-x86_64.pkg.tar.zst` | `sudo pacman -U ./<file>.pkg.tar.zst` |

The bundles are **not code-signed**, so every OS shows a first-run warning.
Notes:

- **macOS: "oxdm is damaged and can't be opened".** The app is not damaged —
  this is Gatekeeper blocking an unsigned, un-notarized app. Drag **oxdm** to
  **Applications**, then clear the quarantine flag and launch normally:
  ```sh
  xattr -dr com.apple.quarantine /Applications/oxdm.app
  ```
  Apple Silicon (`aarch64`) only — the build will not run on an Intel Mac; on
  Intel, build from source.
- **Windows: SmartScreen "Windows protected your PC".** Click **More info** →
  **Run anyway**. The bundles rely on the **WebView2 runtime**, preinstalled on
  Windows 10/11; if the window stays blank on an older or stripped-down system,
  install the WebView2 runtime from Microsoft.
- **Fedora / RHEL-based distributions are not yet supported** as a prebuilt
  package (a different WebKitGTK layout, no `.deb`). Native support is planned
  via Flatpak. Until then, build from source (below).

### Arch Linux

Download the `.pkg.tar.zst` package and its `.sha256` file from
[GitHub Releases](https://github.com/nimatrueway/oxdm/releases), then verify
and install them from the download directory:

```sh
sha256sum -c oxdm-0.6.9-1-x86_64.pkg.tar.zst.sha256
sudo pacman -U ./oxdm-0.6.9-1-x86_64.pkg.tar.zst
```

Pacman installs the required system libraries, desktop launcher, and icons.
Launch **OxDM** from your app launcher or run `oxdm`. A working Secret Service
provider, such as GNOME Keyring or KeePassXC, is needed to save camera credentials.
The package is not in AUR or an automatic-update repository; install newer release
packages with the same `pacman -U` command.

To build from source instead, install Arch's `base-devel` and `git`, clone this
repository, and run `makepkg -si` from its root. The recipe prepares the pinned
RTSP dependency and runs its checks; no separate `cargo install` is needed.
The recipe builds the release tag declared in `PKGBUILD`, not uncommitted local
edits. The Retina archive is checksum-verified, and Cargo uses the committed
lockfile.

If upgrading from a user-local Cargo installation, an older
`~/.local/bin/oxdm` or `~/.cargo/bin/oxdm` can shadow `/usr/bin/oxdm`. Likewise,
`~/.local/share/applications/oxdm.desktop` can shadow the packaged desktop entry.
Back up and remove only your old OxDM launcher files after verifying
`/usr/bin/oxdm`; leave `~/.oxdm` intact to preserve cameras and settings.

### Build from source

OxDM builds with a current stable Rust toolchain — no extra tooling is required to
produce a runnable binary (`dx` is only needed for hot-reload development and
for producing installer bundles). The upstream package is available from
[crates.io](https://crates.io/crates/oxvif-device-manager):

```sh
cargo install oxvif-device-manager --locked
```

For this fork's latest changes, build from a checkout so the patched RTSP
dependency can be prepared:

```sh
git clone https://github.com/nimatrueway/oxdm.git
cd oxdm
./scripts/vendor-retina.sh
cargo install --path . --locked
```

Either way the installed command is **`oxdm`** (the crate is published as
`oxvif-device-manager` because the shorter name was already taken).

oxvif 0.16 requires Rust 1.88 or newer. The complete OxDM 0.4.1 build was
validated on Rust 1.97.0; Rust 1.88 has not been verified for the full desktop
dependency graph. `--locked` uses the dependency versions shipped with the crate.

On Linux, install the WebKitGTK/wry development packages first. For example, on
Debian/Ubuntu:

```sh
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev libxdo-dev
```

The equivalent Fedora packages are `webkit2gtk4.1-devel`, `gtk3-devel`,
`libayatana-appindicator-gtk3-devel`, and `libxdo-devel`.

### Linux video decoders

WebKitGTK uses GStreamer for WebCodecs decoding. A capable GPU and its graphics
driver alone are not enough: GStreamer also needs an H.264 decoder plugin.
If the player shows **Software H.264 → MJPEG**, hover over its badge for setup
guidance. Missing codecs are one possible cause; unsupported WebKit versions
or stream configurations can also trigger the fallback.

Arch Linux:

```sh
sudo pacman -S gst-plugin-va gst-libav
```

Debian/Ubuntu:

```sh
sudo apt install gstreamer1.0-plugins-bad gstreamer1.0-libav
```

Hardware decoding also requires a compatible VA-API driver for your GPU.
Check `gst-inspect-1.0 vah264dec` for a hardware decoder and
`gst-inspect-1.0 avdec_h264` for a software decoder. Hardware element names vary
with distribution, GStreamer version, and GPU (older setups may use
`vaapih264dec`). The software decoder can avoid JPEG transcoding even when GPU
decoding is unavailable.

**Restart OxDM after installing codecs or drivers.** The badge should switch
to WebCodecs when the stream configuration is supported. “Hardware preferred”
means acceleration was requested, not verified GPU utilization.

### Hyprland window chrome

On Linux, OxDM detects Hyprland through `HYPRLAND_INSTANCE_SIGNATURE` or a
`Hyprland` entry in `XDG_CURRENT_DESKTOP`. In that session the main window
omits its native title bar and the generic Window/Edit menu, leaving more
space for the camera workspace. Use your compositor's bindings to move,
resize, close, or toggle fullscreen; the in-app toolbar and settings remain
available. Other Linux desktops, Windows, and macOS keep their native chrome.
Picture-in-picture remains frameless as before.

## Features

- **Two-pane workspace** — select a camera to open Live, with lens and stream
  selectors and camera controls in a compact toolbar above the feed. PTZ and
  image adjustments open beside the same player (below it in smaller windows).
  Settings subtabs group device configuration, image/encoder controls, and
  profile management, with Health, Events, and Quirks under Diagnostics.
  Settings and Recordings open from the video toolbar and have a back-to-video
  button; there is no separate camera header. Fleet
  health and saved groups live in the camera sidebar, with filter and sort
  options tucked behind the button beside search. The sidebar collapses or
  resizes with its divider, and camera thumbnails have a configurable size.
  The sidebar's collapsed state is remembered across restarts.
  A hold-and-drag pan control sits beside Playback options and shares the PTZ
  panel's speed setting, saved across navigation and restarts. Movement follows
  mouse motion; pausing the pointer or releasing the button stops the camera.
  During dragging, an overlay
  compares the starting and current camera-reported position and remaining
  travel in each direction. Missing position/range data stays unknown.
  Closing PiP restores the main window.
- **App settings** — the sidebar gear opens theme, language, default camera
  credentials, capture folders, TLS validation, logging, and About. Credentials
  remain in the system keychain, with per-device overrides preserved.
  Export/import uses plain JSON for preferences, cameras, groups, and hidden
  profiles. **Include credentials** is off by default; enabling it includes
  readable credentials in the exported file. Imports merge cameras and groups
  after confirmation and preserve local credentials when the backup omits them.
  Camera settings use a separate wrench in the video toolbar.
- **Discovery** — three-round WS-Discovery scan of the local network, plus
  manually-added devices. Discovered devices persist across restarts. Returned
  I/O errors are shown separately from a successful scan finding no devices;
  some upstream send/receive failures are still not reported. Rediscovery keeps
  a known device's address while the device continues to advertise it.
- **Live video** — always-on MJPEG snapshot stream, or RTSP (H.264/H.265 +
  audio) decoded in-process: a pure-Rust RTSP client feeds the WebView's
  WebCodecs decoder, with an OpenH264 → MJPEG fallback where WebCodecs is
  unavailable. No sidecar binaries or ffmpeg required. Pinch the trackpad to
  zoom the picture digitally (up to 8×); two-finger scroll pans while zoomed.
  If the camera refuses or drops the stream, the player shows its error.
  Failed profile loads, video opens, image streams, and sidebar thumbnail
  refreshes retry automatically without restarting the app.
  Enable **App settings > Live video > Show video decoder details** to show
  a playback badge identifying camera snapshots, WebCodecs
  (hardware preferred or automatic decoder), or software H.264 → MJPEG
  transcoding. Hover over it for details; the software fallback is highlighted
  because it can be CPU-intensive. “Hardware preferred” is a request, not
  confirmation that the GPU is decoding: WebCodecs does not report that.
  This saved setting is off by default and also controls PiP and recording
  replay, including already-open PiP windows. The badge has no close button;
  change its visibility in App settings. Playback errors remain visible
  independently of this setting. On Linux, the software
  fallback tooltip includes GStreamer install and verification guidance.
  WebGL2 is the default renderer for WebCodecs frames, including PiP and replay.
  Set `OXDM_VIDEO_RENDERER=2d` before launching to force Canvas 2D;
  `OXDM_VIDEO_RENDERER=webgl2` explicitly selects the default.
  Unsupported WebGL2, failed frame uploads, or context loss fall back to Canvas 2D
  without changing the video decoder. WebView console frame statistics include
  the active renderer. This is not a guarantee of zero-copy rendering; compare
  CPU usage, frame rate, and image quality on the same stream and window size.
  Player status transition tests run with `node --test tests/oxdm_stream.test.cjs`
  (Node.js, no npm dependencies), alongside the Rust `cargo test` suite.
  The software fallback uses runtime-detected JPEG SIMD acceleration on supported
  x86 CPUs and stops redundant WebSocket video delivery while retaining audio
  and stream status. Resolution, frame rate, and RGB color conversion are
  unchanged. Measure conversion plus JPEG encoding with
  `cargo test --release --bin oxdm benchmark_fallback_jpeg_encoding -- --ignored --nocapture`;
  this synthetic 1080p benchmark excludes H.264 decoding and WebKit rendering.
  RTSP audio stays available when video uses the MJPEG fallback; playback starts
  muted, and the Unmute button enables audio for streams that provide it.
  Linux WebKit audio requires GStreamer's good plugins (`gst-plugins-good` on
  Arch or `gstreamer1.0-plugins-good` on Ubuntu/Debian); the Linux packages
  install this dependency automatically.
- **Theater mode** — the expand button in the live toolbar fills
  the application window, hiding the titlebar, navigation, and camera controls without
  restarting playback. Hover over the video to reveal the top-right exit button
  and restore the previous layout, or press Escape. Double-click the video to
  toggle OS fullscreen; double-click again to return. The toolbar
  expand button does not change OS fullscreen state.
  Theater mode allows resizing down to 320×180; exiting restores the normal
  900×500 workspace minimum. Theater mode requests always-on-top stacking
  through the native cross-platform window API; exiting theater or leaving
  the live view restores normal stacking. Support depends on the window
  manager: some Linux compositors, including Hyprland, ignore this request.
- **Compact live toolbar** — Snapshot, Record, and Theater stay visible.
  The More options (⋯) menu holds PiP, Recordings, Camera settings, and the
  RTSP / Snapshot playback choice. Click outside the menu or press Escape
  to dismiss it.
- **Optional camera operations** — unsupported `GetSnapshotUri` responses are
  remembered per camera/profile, allowing the existing RTSP snapshot fallback
  without repeated SOAP requests. That fallback feeds sidebar thumbnails from the
  lens's smallest H.264 stream. `GetImagingStatus` support is tracked per
  camera/video source. Decisions are scoped to credentials and reset with the
  session or app restart; authentication and transient failures remain retryable.
- **Snapshots and local recording** — snapshots save immediately from any
  profile thumbnail or Live, with unique timestamped JPEG filenames. App
  Settings → Storage controls snapshot and recording folders; defaults are
  `Pictures/OxDM` and `Movies/OxDM` (the OS video folder), respectively.
- **Device settings** — identification and scopes; network (hostname, IPv4 and
  IPv6 manual interfaces, MTU, DNS, NTP, gateway, protocols); system time (with
  PC sync and timezone/DST); user management (create/read/update/delete); and
  maintenance (reboot and factory reset, both confirmation-gated).
- **Media** — profile create/delete, video-encoder configuration (H.264 and
  H.265, with H.265 automatically routed through Media2), imaging controls
  (brightness/contrast plus manual exposure, white-balance gains, and focus
  limits), and OSD management. Focus is driven at a speed the lens declared, not
  a fixed one, and a direction it declares no speed for is disabled rather than
  silently ignored.
- **Multi-sensor aware** — a dual-lens camera is one ONVIF device with several
  video sources, and every per-channel panel names the one it is addressing. If
  the selected profile has no channel of its own, the panel says which one it
  fell back to instead of showing lens 0 as though you had picked it.
- **PTZ** — preset create/read/update/delete, continuous move, home position,
  a live pan/tilt/zoom readout, and absolute positioning bounded by the ranges
  the camera's PTZ node actually declares (a zoom-only head gets a zoom slider
  and nothing else).
- **Capability-driven navigation** — a camera is only offered the tabs it
  advertises, so a fixed dome shows no PTZ button and a camera with no IO board
  shows no IO Control tab. Silence is never read as a denial: if the device
  cannot be reached, or simply does not answer the question, every entry point
  stays available.
- **Events** — live PullPoint subscription with a scrolling, filterable log.
- **Diagnostics** — an on-demand ONVIF health check with baseline diffing and
  fleet-wide batch export. [See below](#diagnostics--the-onvif-health-check).
- **Camera clones (mocks)** — record a real camera, serve it offline, and diff
  its response shapes against a reference.
  [See below](#camera-clones-and-the-quirks-diff).
- **Localisation and theming** — three themes (Dark / Light / Classic);
  English, 繁體中文, and Русский locales; keyboard shortcuts; an in-app log
  viewer; and an optional on-disk log file.

Credentials (a global default plus optional per-device overrides) are stored in
the operating-system keychain and are never written to disk in plaintext.

## Diagnostics — the ONVIF health check

![Health Overview in OxDM: one row per camera with pass/warn/fail/skip counts, Profile S/T/G badges, and live stream and recording probe results](https://raw.githubusercontent.com/smiti1642/oxdm/main/docs/health-check.png)

A readable alternative to the official ONVIF Device Test Tool. Run it on one
device from its **Diagnostics** tab, or on the whole fleet at once from **Health
Overview** — every camera reports **Pass / Warn / Fail / Skip** counts and ends in
a **Profile S/T/G verdict**. Devices can be sorted into named **groups**
(right-click → add to group) so a run can target a floor, a site, or a vendor
rather than everything.

`Declared: M` next to a verdict is the camera's *own* claim read from its scopes,
shown beside what the run actually **assessed** — a device that declares Profile G
and fails replay is the interesting case, and it is only visible when both numbers
are on screen.

**It verifies, it does not just ask.** A check that only confirmed the device
answered a SOAP call would pass a camera whose stream is dead. So the check opens
the RTSP stream, fetches the snapshot and validates it as a real image (rejecting a
0-byte body or an HTML error page served with a `200`), and genuinely exercises
Profile G recording search and replay.

That is what the `snapshot 291 KB` and `RTSP OK` badges in the shot above are:
bytes that actually arrived, not a URL the camera claimed would work.

Beyond that:

- **Baseline diff.** "Save as baseline" stores the run per-device; the next run
  diffs against it automatically. Regressions to FAIL, checks that appeared or
  disappeared, and checks that slowed by **2× or more** are all flagged.
- **Security probe.** With credentials supplied, a credential-free
  `GetDeviceInformation` probe checks the camera actually enforces
  authentication. A camera that serves device info anonymously is flagged.
- **Under-declared services.** Optionally force-verify services the device does
  *not* advertise, to catch firmware that under-declares its own capabilities.
- **Write round-trip (opt-in, batch).** Re-`Set` the first video-encoder config
  unchanged. This catches devices that reject our serialized request body — an
  interop bug no read-only probe can see.
- **Fleet export.** Batch a run across every device and export the rich JSON
  bundle, or **JUnit XML** for a CI dashboard.

The engine is [`oxvif`'s `health` feature](https://github.com/smiti1642/oxvif#health-check-health-feature),
so the same verdicts are available headlessly from a script or CI job — OxDM is
the interactive front end to it, not a separate implementation.

## Camera clones and the Quirks diff

![The Quirks tab in OxDM: a quirk report grouped by service area, each operation showing added and removed element counts, above a note that operations the device declined with a SOAP Fault are correct device behaviour rather than a client problem](https://raw.githubusercontent.com/smiti1642/oxdm/main/docs/quirks.png)

Right-click a device → **"Clone this camera"**. OxDM records its standard read
surface and serves the recording from an **in-app mock server**, then adds it to
the device list labeled *mock*. You can then operate the clone through every tab
— settings, media, PTZ, imaging — **with the real camera unplugged**.

Clones persist to `~/.oxdm/clones/`; the **Saved mocks** list in the Manual tab
reopens one at any time.

A mock device also gains a **Quirks** tab, grouped by service area with an
issues / clean / skipped count per group. Since 0.3.0 it also works against a
**live camera** — pick the operations, watch them run, and see what moved since
last time, without recording a clone first. Selected operations export to JSON.

Two details in that first shot are the ones that make the tab usable rather than
alarming:

- **A declined operation is not a bug.** Operations the camera refused with a SOAP
  Fault are called out as *correct device behaviour, not an oxvif problem* — a
  camera is allowed to not implement something, and burying those in the issue
  count would make every device look broken.
- **Diff vs baseline.** *"unchanged since the baseline — the same operations
  drift, in the same places"* is the answer you want most of the time. Firmware
  upgrades are when it stops saying that.

Quirks baselines record the oxvif version that measured them. Baselines saved
with oxvif 0.15 still load after this upgrade, with a version-mismatch warning:
the library's reference may have changed even when the camera has not. Review
the current result and save a new baseline to compare future runs on oxvif 0.16.

Expanding an operation gives the git-style side-by-side: `oxvif` reference on the
left, the camera on the right, with word-level highlighting on what differs.

![A git-style side-by-side diff of one operation: oxvif's reference response on the left, the cloned camera's on the right, with word-level highlighting on the values that differ and extra vendor blocks the reference does not emit](https://raw.githubusercontent.com/smiti1642/oxdm/main/docs/quirks-diff.png)

This is where a device's personality shows up: the same `GetProfiles` call, and
this camera names its profile `R_H264` rather than `mainStream`, runs the stream at
640×360 rather than 1920×1080, and returns an `<Extension><Rotate>` block and an
`<AudioSourceConfiguration>` the reference never emits. None of that is an error —
it is exactly the shape a client has to survive.

The `__MASKED__` tokens are deliberate: instance values are normalised before the
comparison so the diff shows *shape* drift rather than every serial number, and a
saved clone carries no credential.

**Honest scope.** Both of these are deliberately narrower than they might look:

- A clone covers the **standard read surface**, not the whole device. It is a
  standard-surface snapshot, not a 100% clone.
- Recorded `GetServices` responses and stream URIs embed the **real camera's
  addresses**, so some media/PTZ calls on a clone may still route to the real
  device. Rewriting those to point at the container is later work.
- The Quirks diff is **structural** — which elements are present — not ONVIF
  schema conformance.

These limits are surfaced in the UI too, not only here.

Built on `oxvif`'s
[`metamorph-server`](https://github.com/smiti1642/oxvif#metamorph-metamorph--metamorph-server-features)
feature, which is enabled in the default build.

## Trying it without a camera

OxDM pairs with the `oxvif` mock server, which implements enough of ONVIF to
exercise most of the UI. The `oxvif` library is pulled in from
[crates.io](https://crates.io/crates/oxvif) automatically, but the standalone
mock server ships as an `oxvif` *example*, so it requires a local checkout:

```sh
# One-time: clone the oxvif repository
git clone https://github.com/smiti1642/oxvif ../oxvif

# Terminal 1: start the mock server (default port 18080)
cd ../oxvif && cargo run --example mock_server --features mock-server

# Terminal 2: start OxDM
dx serve --platform desktop
```

In OxDM, open the **Manual** tab → **Add** → enter `127.0.0.1:18080` (no
credentials required). Snapshot thumbnails and the settings tabs will show live
data from the mock device, and the **Diagnostics** tab works against it as well.

## Usage

Once installed, launch OxDM and use the left sidebar to scan for devices or add
one manually. Select a device to access its settings, live video, PTZ, events,
and diagnostics.

## Development

```sh
dx serve --platform desktop
```

`dx serve` provides hot-reload during development and requires
[`dioxus-cli`](https://dioxuslabs.com/learn/0.6/CLI/installation)
(`cargo install dioxus-cli`). A plain `cargo run` also works without it.

Verbose logging:

```sh
RUST_LOG=oxdm=debug dx serve --platform desktop
```

The player regression tests use Node.js's built-in runner, with no npm dependencies:

```sh
node --test assets/js/oxdm-stream.test.cjs
```

## License

Released under the [MIT License](./LICENSE). © 2026 smiti1642

## Support

If oxvif saves you time, consider supporting its development.

[Buy me a coffee](https://buymeacoffee.com/smiti1642)
