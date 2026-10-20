#!/bin/sh
set -eu
binary=${1:?binary path required}
dist=${2:?dist directory required}
mkdir -p "$dist"
work=$(mktemp -d "$dist/package.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
cp "$binary" "$work/ai-session-hub"
cp config.example.toml LICENSE THIRD_PARTY_NOTICES.md "$work/"
archive="$dist/ai-session-hub-macos-aarch64.tar.gz"
tar -C "$work" -czf "$archive" ai-session-hub config.example.toml LICENSE THIRD_PARTY_NOTICES.md
(cd "$dist" && shasum -a 256 ai-session-hub-macos-aarch64.tar.gz > ai-session-hub-macos-aarch64.tar.gz.sha256)
cp scripts/install.sh "$dist/ai-session-hub-install.sh"
echo "Packaged $archive"
