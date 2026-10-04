# Maintainer: nimatrueway
pkgname=oxdm
pkgver=0.6.7
pkgrel=1
pkgdesc='ONVIF IP camera manager with live video, PTZ, and device diagnostics'
arch=('x86_64')
url='https://github.com/nimatrueway/oxdm'
license=('MIT')
depends=('cairo' 'dbus' 'gdk-pixbuf2' 'glib2' 'glibc' 'gst-plugins-good' 'gtk3'
         'hicolor-icon-theme' 'libayatana-appindicator' 'libgcc' 'libsoup3'
         'libstdc++' 'openssl' 'wayland' 'webkit2gtk-4.1' 'xdotool')
makedepends=('cargo' 'git')
checkdepends=('desktop-file-utils')
optdepends=('org.freedesktop.secrets: secure camera credential storage')
conflicts=('oxdm-git')
options=('!lto' '!debug')
source=("oxdm-source::git+$url.git#tag=v$pkgver"
        'https://static.crates.io/crates/retina/retina-0.4.20.crate')
noextract=('retina-0.4.20.crate')
sha256sums=('SKIP'
            '0e0eb740f743e678e071628ff6bf84ed5ed03df879997cc3d43b5afef64aff93')

# Keep makepkg's cleanup away from the repository's src/ directory.
if [[ ${BUILDDIR:-$PWD} == "$PWD" ]]; then
  BUILDDIR="$PWD/.arch-build"
fi

prepare() {
  cd "$srcdir/oxdm-source"
  test "$(awk -F '"' '/^version = / {print $2; exit}' Cargo.toml)" = "$pkgver"
  RETINA_CRATE="$srcdir/retina-0.4.20.crate" ./scripts/vendor-retina.sh
  cargo fetch --locked
}

build() {
  export CARGO_TARGET_DIR="$srcdir/target"
  cd "$srcdir/oxdm-source"
  cargo build --release --frozen
}

check() {
  export CARGO_TARGET_DIR="$srcdir/target"
  cd "$srcdir/oxdm-source"
  cargo test --release --frozen
  desktop-file-validate packaging/oxdm.desktop
}

package() {
  cd "$srcdir/oxdm-source"
  install -Dm755 "$srcdir/target/release/oxdm" "$pkgdir/usr/bin/oxdm"
  install -Dm644 packaging/oxdm.desktop "$pkgdir/usr/share/applications/oxdm.desktop"
  install -Dm644 assets/icons/icon-256.png "$pkgdir/usr/share/icons/hicolor/256x256/apps/oxdm.png"
  install -Dm644 assets/icons/oxdm.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/oxdm.svg"
  install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
