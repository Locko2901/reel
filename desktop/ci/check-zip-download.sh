#!/usr/bin/env bash
set -euo pipefail
APP="${1:-/target/debug/reel-desktop}"
SERVER="${REEL_E2E_SERVER:-http://127.0.0.1:8746}"
USER_NAME="${REEL_E2E_USER:-sam}"
PASSWORD="${REEL_E2E_PASSWORD:?the password of the demo admin}"
SHOTS="${REEL_SHOTS_OUT:-}"
# Where Chromium draws the buttons at 1280 by 820; the Select bar sits above the app's bar.
BAR_DOWNLOAD="${REEL_ZIP_BAR:-637 753}"
DIALOG_DOWNLOAD="${REEL_ZIP_GO:-874 523}"
W=$(mktemp -d)
LOG="$W/d/data/logs"
JAR="$W/jar"
DL="$W/Downloads"
fail() { echo "FAIL $*"; grep -m1 -A1 "panicked" "$W/app.out" 2>/dev/null || tail -20 "$W/app.out" 2>/dev/null; tail -15 "$LOG"/reel-desktop.log* 2>/dev/null; exit 1; }
ok() { echo "ok   $*"; }
log_n() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
wait_more() { # wait_more <seconds> <what> <log words> <count before>
  for _ in $(seq 1 $(($1 * 2))); do [ "$(log_n "$3")" -gt "$4" ] && return 0; sleep 0.5; done
  fail "$2"
}
shown() { cat "$LOG"/reel-desktop.log* | grep "download shown in the page" | sed -n 's/.*shown=\(.*\) download shown in the page.*/\1/p; s/.*download shown in the page.* shown=\(.*\)$/\1/p'; }
shot() { [ -z "$SHOTS" ] || import -display "$DISPLAY" -window "$WIN" "$SHOTS/$1.png"; }
trap 'chmod -R u+w "$W" 2>/dev/null || true' EXIT
HOST=$(echo "$SERVER" | sed -E 's#https?://([^:/]+).*#\1#')
[ -z "$SHOTS" ] || mkdir -p "$SHOTS"

curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"zip download\"}" "$SERVER/api/device/pair" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no token from the demo"
IDS=$(curl -fsS -b "$JAR" "$SERVER/" | grep -o 'data-id="[0-9A-Z]\{26\}"' | cut -d'"' -f2 | sort -u | sed 's/.*/"&"/' | paste -sd, -)
CLIPS=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -H 'content-type: application/json' -X POST -d "{\"ids\":[$IDS]}" "$SERVER/api/bulk/download" \
  | python3 -I -c 'import json,sys; d=json.load(sys.stdin); print(d["clips"], d["choices"]["current"]["files"], d["choices"]["current"]["size"])')
read -r N FILES SIZE <<< "$CLIPS"
[ "$N" -gt 1 ] || fail "the demo needs two ready clips or more"

mkdir -p "$W/d/config" "$W/d/data" "$W/xdg" "$DL"
printf 'XDG_DOWNLOAD_DIR="%s"\n' "$DL" > "$W/xdg/user-dirs.dirs"
printf '%s' "$TOKEN" > "$W/d/config/token"
chmod 600 "$W/d/config/token"
printf '{"server_url":"%s","obs":{"enabled":false},"clips_dir":"%s","clips_follow_obs":false,"startup":{"autostart":false},"log_level":"info"}' \
  "$SERVER" "$W/clips" > "$W/d/config/settings.json"
export REEL_DESKTOP_DIR="$W/d" XDG_CONFIG_HOME="$W/xdg" RUST_BACKTRACE=1
"$APP" > "$W/app.out" 2>&1 &
wait_more 30 "the sign-in page in the window" "library loaded at=$HOST/signin" 0
sleep 2
WIN=$(xdotool search --name '^reel$' | head -1 || true)
[ -n "$WIN" ] || fail "no window"
xdotool windowmove "$WIN" 0 0 2>/dev/null || true
xdotool windowsize "$WIN" 1280 820
xdotool windowactivate --sync "$WIN" 2>/dev/null || true
sleep 1
xdotool mousemove 640 328 click 1
xdotool type --delay 30 "$USER_NAME"
xdotool key Tab
xdotool type --delay 30 "$PASSWORD"
n=$(log_n "library loaded at=$HOST/$")
xdotool key Return
wait_more 20 "the library after the sign-in" "library loaded at=$HOST/$" "$n"
sleep 2
xdotool mousemove 1200 200 click 1
xdotool key ctrl+a
sleep 1
shot picked
xdotool mousemove $BAR_DOWNLOAD click 1
sleep 2
shot dialog
xdotool mousemove $DIALOG_DOWNLOAD click 1
wait_more 20 "the page to hear the zip start" "download shown in the page" 0
FIRST=$(shown | head -1)
case "$FIRST" in "Downloading reel $N clips "*".zip, "*) ;; *) fail "the page said \"$FIRST\" as the zip started" ;; esac
wait_more 120 "the zip to end" "download saved" 0
wait_more 10 "the page to hear it saved" "download shown in the page" 1
sleep 0.5
shot saved
SAID=$(shown | grep "^Saved to Downloads: " | tail -1)
[ -n "$SAID" ] || fail "the page said \"$(shown | tail -1)\" once the zip was saved"
FILE="$DL/${SAID#Saved to Downloads: }"
[ -f "$FILE" ] || fail "no $FILE in the Downloads folder: $(ls "$DL")"
GOT=$(python3 -I - "$FILE" <<'PY'
import sys, zipfile
z = zipfile.ZipFile(sys.argv[1])
assert z.testzip() is None, "a file whose CRC is wrong"
names = z.namelist()
assert names[0] == "README.txt", names[:3]
folders = {n.split("/")[0] for n in names if "/" in n}
videos = [n for n in names if n.endswith(".mp4")]
print(len(folders), len(videos), sum(i.file_size for i in z.infolist() if i.filename.endswith(".mp4")))
PY
) || fail "the zip in Downloads does not read: $GOT"
read -r FOLDERS VIDEOS VBYTES <<< "$GOT"
[ "$FOLDERS" -eq "$N" ] && [ "$VIDEOS" -eq "$FILES" ] || fail "the zip holds $FOLDERS folders and $VIDEOS videos, not $N and $FILES"
ok "Ctrl+A, Download, Download in the window: the page said \"$FIRST\", then \"$SAID\"; the zip in Downloads reads whole: $FOLDERS folders, $VIDEOS videos ($VBYTES bytes), the current version of each ($SIZE said)"
! grep -q "panicked" "$W/app.out" || fail "a thread panicked"
