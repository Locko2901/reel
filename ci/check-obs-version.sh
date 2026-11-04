#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
pin=$(sed -n 's/^ *"version": *"\([0-9.]*\)".*/\1/p' desktop/obs/pinned.json | head -1)
[ -n "$pin" ] || { echo "desktop/obs/pinned.json names no version"; exit 1; }
named=$(sed -n '/^## OBS Studio/,/^## /p' THIRD_PARTY.md | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | sort -u)
[ -n "$named" ] || { echo "THIRD_PARTY.md names no version of OBS Studio"; exit 1; }
if [ "$named" != "$pin" ]; then
  echo "OBS $pin is pinned, and THIRD_PARTY.md's OBS Studio section names:"
  echo "$named"
  exit 1
fi
echo "THIRD_PARTY.md names OBS $pin, the pinned version"
