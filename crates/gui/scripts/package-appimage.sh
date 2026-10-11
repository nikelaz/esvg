#!/usr/bin/env bash
set -euo pipefail

version="${1:?Usage: crates/gui/scripts/package-appimage.sh <version> [binary-path]}"
binary="${2:-target/release/esvg}"
output="dist/esvg-${version}-linux-x86_64.AppImage"
gui_root="$(cd "$(dirname "$0")/.." && pwd)"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

if [[ ! -x "$binary" ]]; then
    printf 'Release binary not found or not executable: %s\n' "$binary" >&2
    exit 1
fi

mkdir -p "$(dirname "$output")"
output="$(realpath -m "$output")"
cp "$binary" "$work_dir/esvg"
cp "$gui_root/assets/icons/linux/esvg-256x256.png" "$work_dir/esvg.png"

curl --fail --location --silent --show-error \
    https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage \
    --output "$work_dir/linuxdeploy-x86_64.AppImage"
chmod +x "$work_dir/linuxdeploy-x86_64.AppImage"

export LINUXDEPLOY_OUTPUT_VERSION="$version"
export LINUXDEPLOY_OUTPUT_APP_NAME=esvg
export LDAI_OUTPUT="$output"
export APPIMAGE_EXTRACT_AND_RUN=1

cd "$work_dir"
./linuxdeploy-x86_64.AppImage \
    --appdir AppDir \
    --executable esvg \
    --desktop-file "$gui_root/packaging/linux/esvg.desktop" \
    --icon-file esvg.png \
    --output appimage

chmod +x "$output"
