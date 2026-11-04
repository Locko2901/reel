#!/usr/bin/env bash
# Named with the target triple, as Tauri expects of a sidecar.
set -euo pipefail
TARGET="${1:?linux or windows}"
HERE=$(cd "$(dirname "$0")/.." && pwd)
PIN="$HERE/ffmpeg/pinned.json"
[ "$TARGET" = linux ] && PIN="$HERE/ffmpeg/pinned-linux.json"
BIN="$HERE/app/binaries"
CACHE="${FFMPEG_CACHE:-$HERE/../.ffmpeg-cache}"
mkdir -p "$BIN" "$CACHE"
field() { sed -n "s/.*\"$1\": *\"\([^\"]*\)\".*/\1/p" "$PIN" | head -1; }
URL=$(field url); SHA=$(field sha256); VERSION=$(field version)
ZIP="$CACHE/$(basename "$URL")"
if [ ! -f "$ZIP" ]; then
  echo "sidecars: downloading ffmpeg $VERSION for $TARGET"
  curl -fsSL --retry 3 -o "$ZIP.part" "$URL" && mv "$ZIP.part" "$ZIP"
fi
echo "$SHA  $ZIP" | sha256sum -c - >/dev/null || { echo "sidecars: $ZIP does not match the pinned SHA-256"; rm -f "$ZIP"; exit 1; }
case "$TARGET" in
  linux)
    TMP=$(mktemp -d)
    tar -xJf "$ZIP" -C "$TMP" --wildcards --strip-components=1 '*/bin/ffmpeg' '*/bin/ffprobe' '*/LICENSE.txt'
    install -m 755 "$TMP/bin/ffmpeg" "$BIN/ffmpeg-x86_64-unknown-linux-gnu"
    install -m 755 "$TMP/bin/ffprobe" "$BIN/ffprobe-x86_64-unknown-linux-gnu"
    mv "$TMP/LICENSE.txt" "$BIN/ffmpeg-LICENSE.txt"
    rm -rf "$TMP"
    echo "sidecars: ffmpeg $VERSION for Linux, checksum ok"
    ;;
  windows)
    TMP=$(mktemp -d)
    unzip -q -j -o "$ZIP" '*/bin/ffmpeg.exe' '*/bin/ffprobe.exe' '*/LICENSE' -d "$TMP"
    mv "$TMP/ffmpeg.exe" "$BIN/ffmpeg-x86_64-pc-windows-msvc.exe"
    mv "$TMP/ffprobe.exe" "$BIN/ffprobe-x86_64-pc-windows-msvc.exe"
    mv "$TMP/LICENSE" "$BIN/ffmpeg-LICENSE.txt"
    rm -rf "$TMP"
    echo "sidecars: ffmpeg $VERSION for Windows, checksum ok"
    ;;
  *) echo "linux or windows"; exit 2 ;;
esac
