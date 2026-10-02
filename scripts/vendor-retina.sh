#!/usr/bin/env bash
# Materialise the patched `retina` crate that Cargo.toml's `[patch.crates-io]`
# points at (vendor/retina, git-ignored).
#
# We don't commit retina's sources; instead this downloads the exact registry
# tarball from crates.io, unpacks it, and applies patches/retina-*.patch.
# Run once after clone (and after bumping RETINA_VERSION); CI runs it before
# `dx bundle`. Idempotent: a matching vendor/retina is left alone.
set -euo pipefail

RETINA_VERSION="0.4.20"
RETINA_SHA256="0e0eb740f743e678e071628ff6bf84ed5ed03df879997cc3d43b5afef64aff93"
PATCH="patches/retina-${RETINA_VERSION}-keepalive-get-parameter.patch"

cd "$(dirname "$0")/.."
[ -f "$PATCH" ] || { echo "missing $PATCH" >&2; exit 1; }

sha256() {
  if command -v sha256sum >/dev/null; then
    sha256sum "$1"
  else
    shasum -a 256 "$1"
  fi | cut -c1-64
}

patch_hash=$(sha256 "$PATCH")
stamp="vendor/retina/.oxdm-patched"
if [ -f "$stamp" ] && [ "$(cat "$stamp")" = "${RETINA_VERSION} $patch_hash" ]; then
  echo "vendor/retina ${RETINA_VERSION} already patched"
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
if [ -n "${RETINA_CRATE:-}" ]; then
  cp "$RETINA_CRATE" "$tmp/retina.crate"
else
  curl -fsSL --retry 3 -o "$tmp/retina.crate" \
    "https://static.crates.io/crates/retina/retina-${RETINA_VERSION}.crate"
fi
if [ "$(sha256 "$tmp/retina.crate")" != "$RETINA_SHA256" ]; then
  echo "retina ${RETINA_VERSION} archive checksum mismatch" >&2
  exit 1
fi
tar -xzf "$tmp/retina.crate" -C "$tmp"

src="$tmp/retina-${RETINA_VERSION}"
patch -d "$src" -p1 --silent < "$PATCH"
# Strip the cargo-publish metadata that would otherwise turn the directory into
# a "published crate" cargo refuses to treat as a path dep with a changed lockfile.
rm -f "$src/Cargo.toml.orig" "$src/.cargo_vcs_info.json" "$src/Cargo.lock"

rm -rf vendor/retina
mkdir -p vendor
mv "$src" vendor/retina
echo "${RETINA_VERSION} $patch_hash" > "$stamp"
echo "vendor/retina ${RETINA_VERSION} fetched and patched"
