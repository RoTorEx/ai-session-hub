#!/bin/sh
set -eu
install_dir="${SOVA_INSTALL_DIR:-$HOME/.x-cli-sova}"
config_file="${SOVA_CONFIG:-${XDG_CONFIG_HOME:-$HOME/.config}/sova/config.toml}"
version=latest
archive_path=""
update_path=1
fail() { echo "ERROR: $*" >&2; exit 1; }
while [ "$#" -gt 0 ]; do
    case "$1" in
        --archive) archive_path=${2:?missing archive}; shift 2 ;;
        --install-dir) install_dir=${2:?missing directory}; shift 2 ;;
        --version) version=${2:?missing version}; shift 2 ;;
        --no-path-update) update_path=0; shift ;;
        -h|--help) echo 'Usage: sova-install.sh [--version VERSION|latest] [--archive PATH] [--install-dir PATH] [--no-path-update]'; exit 0 ;;
        *) fail "Unknown argument: $1" ;;
    esac
done
[ "$(uname -s)" = Darwin ] || fail 'Sova releases support macOS only'
[ "$(uname -m)" = arm64 ] || fail 'Sova releases support Apple Silicon only'
work=$(mktemp -d "${TMPDIR:-/tmp}/sova-install.XXXXXX")
stage=""
trap 'rm -rf "$work"; if [ -n "$stage" ]; then rm -f "$stage"; fi' EXIT HUP INT TERM
archive=sova-macos-aarch64.tar.gz
if [ -n "$archive_path" ]; then
    cp "$archive_path" "$work/$archive"
    cp "$archive_path.sha256" "$work/$archive.sha256"
else
    base=https://github.com/RoTorEx/sova/releases
    if [ "$version" = latest ]; then base="$base/latest/download"; else base="$base/download/v${version#v}"; fi
    curl -fsSL --connect-timeout 15 --max-time 120 "$base/$archive" -o "$work/$archive"
    curl -fsSL --connect-timeout 15 --max-time 120 "$base/$archive.sha256" -o "$work/$archive.sha256"
fi
expected=$(awk 'NR==1 {print $1}' "$work/$archive.sha256")
actual=$(shasum -a 256 "$work/$archive" | awk '{print $1}')
[ "$expected" = "$actual" ] || fail 'Archive checksum mismatch'
tar -tzf "$work/$archive" | LC_ALL=C sort > "$work/entries"
printf '%s\n' LICENSE THIRD_PARTY_NOTICES.md config.example.toml sova | LC_ALL=C sort > "$work/expected"
cmp -s "$work/entries" "$work/expected" || fail 'Unexpected archive entries'
tar -tvzf "$work/$archive" > "$work/types"
awk 'substr($1,1,1)!="-" {bad=1} END {exit bad}' "$work/types" || fail 'Archive must contain regular files only'
tar -xzf "$work/$archive" -C "$work"
[ -f "$work/sova" ] && [ -x "$work/sova" ] || fail 'Missing executable'
installed_version=$("$work/sova" --version)
case "$installed_version" in 'sova '[0-9]*.[0-9]*.[0-9]*) ;; *) fail 'Invalid binary version' ;; esac
if [ "$version" != latest ]; then [ "$installed_version" = "sova ${version#v}" ] || fail 'Binary version does not match requested release'; fi
mkdir -p "$install_dir/bin" "$(dirname "$config_file")"
stage="$install_dir/bin/.sova-install-$$"
cp "$work/sova" "$stage"
chmod 0755 "$stage"
mv -f "$stage" "$install_dir/bin/sova"
stage=""
if [ ! -e "$config_file" ]; then
    legacy_config="${XDG_CONFIG_HOME:-$HOME/.config}/fleet-scope/config.toml"
    if [ -z "${SOVA_CONFIG:-}" ] && [ -f "$legacy_config" ]; then
        (set -C; cat "$legacy_config" > "$config_file")
        echo "Migrated settings from $legacy_config"
    else
        (set -C; cat "$work/config.example.toml" > "$config_file")
    fi
    echo "Created $config_file"
else echo "Preserved $config_file"; fi
if [ "$update_path" -eq 1 ]; then
    case "${SHELL:-}" in */bash) profile="$HOME/.bashrc" ;; *) profile="$HOME/.zshrc" ;; esac
    # Paths with shell metacharacters must not become executable profile text.
    case "$install_dir" in *'"'*|*'$'*|*'`'*|*'\'*|*'
'*) fail 'Install path cannot be added safely; use --no-path-update' ;; esac
    line="export PATH=\"$install_dir/bin:\$PATH\""
    touch "$profile"
    if ! grep -Fqx "$line" "$profile"; then
        printf '\n# x-cli-sova\n%s\n# /x-cli-sova\n' "$line" >> "$profile"
    fi
fi
echo "Installed $install_dir/bin/sova ($installed_version)"
