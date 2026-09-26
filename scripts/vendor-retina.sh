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
PATCH="patches/retina-${RETINA_VERSION}-keepalive-get-parameter.patch"

cd "$(dirname "$0")/.."
[ -f "$PATCH" ] || { echo "missing $PATCH" >&2; exit 1; }

stamp="vendor/retina/.oxdm-patched"
if [ -f "$stamp" ] && [ "$(cat "$stamp")" = "${RETINA_VERSION} $(shasum -a 256 "$PATCH" | cut -c1-64)" ]; then
  echo "vendor/retina ${RETINA_VERSION} already patched"
  exit 0
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL --retry 3 -o "$tmp/retina.crate" \
  "https://static.crates.io/crates/retina/retina-${RETINA_VERSION}.crate"
tar -xzf "$tmp/retina.crate" -C "$tmp"

src="$tmp/retina-${RETINA_VERSION}"
patch -d "$src" -p1 --silent < "$PATCH"
# Strip the cargo-publish metadata that would otherwise turn the directory into
# a "published crate" cargo refuses to treat as a path dep with a changed lockfile.
rm -f "$src/Cargo.toml.orig" "$src/.cargo_vcs_info.json" "$src/Cargo.lock"

rm -rf vendor/retina
mkdir -p vendor
mv "$src" vendor/retina
echo "${RETINA_VERSION} $(shasum -a 256 "$PATCH" | cut -c1-64)" > "$stamp"
echo "vendor/retina ${RETINA_VERSION} fetched and patched"
