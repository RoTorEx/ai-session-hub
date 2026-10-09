#!/bin/sh
set -eu
binary=${1:?binary path required}
dist=${2:?dist directory required}
mkdir -p "$dist"
work=$(mktemp -d "$dist/package.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
cp "$binary" "$work/sova"
cp config.example.toml LICENSE THIRD_PARTY_NOTICES.md "$work/"
archive="$dist/sova-macos-aarch64.tar.gz"
tar -C "$work" -czf "$archive" sova config.example.toml LICENSE THIRD_PARTY_NOTICES.md
(cd "$dist" && shasum -a 256 sova-macos-aarch64.tar.gz > sova-macos-aarch64.tar.gz.sha256)
cp scripts/install.sh "$dist/sova-install.sh"
echo "Packaged $archive"
