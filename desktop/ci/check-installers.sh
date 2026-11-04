#!/usr/bin/env bash
set -euo pipefail
OUT="${1:?the folder with the installers}"
HERE=$(cd "$(dirname "$0")/.." && pwd)
VERSION="${2:-}"
[ -n "$VERSION" ] || VERSION=$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' "$HERE/app/tauri.conf.json" | head -1)
PACKAGES_JSON="$HERE/packages.json" . "$HERE/ci/packages.sh"
PLAIN="$OUT/${PLAIN_FILE}_${VERSION}_x64-setup.exe"
WITH="$OUT/${WITH_OBS_FILE}_${VERSION}_x64-setup.exe"
ZIP=$("$HERE/ci/fetch-obs.sh" zip -)
fail=0
check() { if [ "$1" = 0 ]; then echo "ok   $2"; else echo "FAIL $2"; fail=1; fi; }
list() { 7z l -slt "$1" | sed -n 's/^Path = //p' | tr '\\' '/' | sed 's|^\$INSTDIR/||' | sort; }
[ -f "$WITH" ] || { echo "FAIL $(basename "$WITH") is not there"; exit 1; }
if [ -f "$PLAIN" ]; then
  plain=$(list "$PLAIN")
else
  echo "skip $(basename "$PLAIN") was not built in this run: its checks are skipped"
  PLAIN="" plain=""
fi
with=$(list "$WITH")
want=$(unzip -Z1 "$ZIP" | grep -v '/$' | sed 's|^bin/64bit/obs64\.exe$|bin/64bit/reel-capture.exe|; s|^|obs/|' | sort)
missing=$(comm -23 <(echo "$want") <(echo "$with") | head -5)
check "$([ -z "$missing" ] && echo 0 || echo 1)" "$WITH_OBS_NAME has every one of the zip's $(echo "$want" | wc -l) files under obs/${missing:+; missing: $missing}"
extra=$(comm -13 <(echo "$want") <(echo "$with" | grep '^obs/') | tr '\n' ' ')
check "$([ "$extra" = "obs/portable_mode.txt " ] && echo 0 || echo 1)" "and adds portable_mode.txt to them, nothing else (${extra:-nothing})"
path=$(7z l -slt "$WITH" | sed -n 's/^Path = //p' | grep -E 'obs.bin.64bit.reel-capture\.exe$' | head -1)
a=$(7z e -so "$WITH" "$path" 2>/dev/null | sha256sum | cut -d' ' -f1)
b=$(unzip -p "$ZIP" bin/64bit/obs64.exe | sha256sum | cut -d' ' -f1)
check "$([ "$a" = "$b" ] && echo 0 || echo 1)" "its reel-capture.exe is the official obs64.exe, byte for byte ($a), and no obs64.exe is in it"
check "$(echo "$with" | grep -c 'obs64\.exe$' || true)" "no file named obs64.exe in $WITH_OBS_NAME"
grep -q '^obs/data/obs-studio/license/gplv2.txt$' <<<"$with"
check $? "OBS's licence text ships with it (obs/data/obs-studio/license/gplv2.txt)"
both=("$WITH")
[ -n "$PLAIN" ] && both+=("$PLAIN") && check "$(echo "$plain" | grep -c '^obs/' || true)" "$PLAIN_NAME has no OBS in it"
has() { grep -qx${2:-} -- "$1" <<<"$with" && { [ -z "$PLAIN" ] || grep -qx${2:-} -- "$1" <<<"$plain"; }; }
words=$([ -n "$PLAIN" ] && echo both || echo "$WITH_OBS_NAME")
for name in reel-desktop.exe ffmpeg.exe ffprobe.exe binaries/ffmpeg-LICENSE.txt; do
  has "$name"
  check $? "$words: $name"
done
stop='$PLUGINSDIR/reel-stop.exe'
has "$stop" F
check $? "$words: the stop helper their hooks run first ($stop)"
has 'reel-elevate.exe'
check $? "$words: reel-elevate.exe beside the app"
ls -l "${both[@]}" | awk '{print $5, $9}'
exit $fail
