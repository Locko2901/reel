#!/usr/bin/env bash
# Usage: fetch-obs.sh windows|sources|linux|zip <folder>; zip prints the cached zip's path.
set -euo pipefail
WHAT="${1:?windows, sources, linux or zip}"
INTO="${2:?a folder}"
HERE=$(cd "$(dirname "$0")/.." && pwd)
PIN="$HERE/obs/pinned.json"
CACHE="${OBS_CACHE:-$HERE/../.obs-cache}"
field() { sed -n "s/.*\"$1\": *\"\([^\"]*\)\".*/\1/p" "$PIN" | head -1; }
VERSION=$(field version)
case "$WHAT" in
  windows|zip) URL=$(field url); SHA=$(field sha256) ;;
  sources) URL=$(field sources_url); SHA=$(field sources_sha256) ;;
  linux) URL=$(field linux_url); SHA=$(field linux_sha256) ;;
  *) echo "windows, sources, linux or zip"; exit 2 ;;
esac
mkdir -p "$CACHE"
FILE="$CACHE/$(basename "$URL")"
if [ ! -f "$FILE" ]; then
  echo "obs: downloading OBS $VERSION ($WHAT)" >&2
  curl -fsSL --retry 3 -o "$FILE.part" "$URL" && mv "$FILE.part" "$FILE"
fi
echo "$SHA  $FILE" | sha256sum -c - >/dev/null || { echo "obs: $FILE does not match the pinned SHA-256" >&2; rm -f "$FILE"; exit 1; }
case "$WHAT" in
  zip) echo "$FILE" ;;
  windows)
    rm -rf "$INTO" && mkdir -p "$INTO"
    unzip -q "$FILE" -d "$INTO"
    [ -f "$INTO/bin/64bit/obs64.exe" ] || { echo "obs: the zip has no bin/64bit/obs64.exe"; exit 1; }
    mv "$INTO/bin/64bit/obs64.exe" "$INTO/bin/64bit/reel-capture.exe"
    printf 'reel keeps this OBS portable: its settings live in the config folder beside this file.\n' > "$INTO/portable_mode.txt"
    echo "obs: OBS $VERSION for Windows, checksum ok, $(find "$INTO" -type f | wc -l) files"
    ;;
  *)
    mkdir -p "$INTO"
    cp "$FILE" "$INTO/"
    echo "obs: $(basename "$FILE"), checksum ok"
    ;;
esac
