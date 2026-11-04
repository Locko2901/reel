#!/usr/bin/env bash
# Under xvfb.sh against a demo; REEL_SHOTS_OUT for screenshots.
set -euo pipefail
APP="${1:-/target/debug/reel-desktop}"
SERVER="${REEL_E2E_SERVER:-http://127.0.0.1:8746}"
USER_NAME="${REEL_E2E_USER:-sam}"
PASSWORD="${REEL_E2E_PASSWORD:?the password of the demo admin}"
SHOTS="${REEL_SHOTS_OUT:-}"
W=$(mktemp -d)
LOG="$W/d/data/logs"
JAR="$W/jar"
DL="$W/Downloads"
CLIP=""
fail() { echo "FAIL $*"; grep -m1 -A1 "panicked" "$W/app.out" 2>/dev/null || tail -20 "$W/app.out" 2>/dev/null; tail -15 "$LOG"/reel-desktop.log* 2>/dev/null; exit 1; }
ok() { echo "ok   $*"; }
log_n() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
wait_more() { # wait_more <seconds> <what> <log words> <count before>
  for _ in $(seq 1 $(($1 * 2))); do [ "$(log_n "$3")" -gt "$4" ] && return 0; sleep 0.5; done
  fail "$2"
}
shown() { cat "$LOG"/reel-desktop.log* | grep "download shown in the page" | sed -n 's/.*shown=\(.*\) download shown in the page.*/\1/p; s/.*download shown in the page.* shown=\(.*\)$/\1/p'; }
shot() { [ -z "$SHOTS" ] || import -display "$DISPLAY" -window "$WIN" "$SHOTS/$1.png"; }
cleanup() {
  [ -z "$CLIP" ] || curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X DELETE "$SERVER/api/clips/$CLIP" > /dev/null 2>&1 || true
  curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/storage/purge-trash" > /dev/null 2>&1 || true
  chmod -R u+w "$W" 2>/dev/null || true
}
trap cleanup EXIT
HOST=$(echo "$SERVER" | sed -E 's#https?://([^:/]+).*#\1#')
[ -z "$SHOTS" ] || mkdir -p "$SHOTS"

/work/desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu -v error -f lavfi -i "testsrc2=duration=4:size=1280x720:rate=30" \
  -f lavfi -i "sine=frequency=520:sample_rate=48000:duration=4" -metadata comment="$(od -An -N8 -tx8 /dev/urandom | tr -d ' ')" \
  -c:v libx264 -pix_fmt yuv420p -c:a aac "$W/Download check.mp4"
curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"downloads\"}" "$SERVER/api/device/pair" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no token from the demo"
SIZE=$(stat -c %s "$W/Download check.mp4")
SHA=$(sha256sum "$W/Download check.mp4" | cut -d' ' -f1)
UP=$(curl -fsS -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' -X POST \
  -d "{\"file_name\":\"Download check.mp4\",\"size\":$SIZE,\"sha256\":\"$SHA\",\"title\":\"Download check\"}" "$SERVER/api/device/uploads")
UPLOAD=$(echo "$UP" | sed -n 's/.*"upload_id":"\([^"]*\)".*/\1/p')
[ -n "$UPLOAD" ] || fail "no upload: $UP"
curl -fsS -H "authorization: Bearer $TOKEN" -X PUT --data-binary "@$W/Download check.mp4" "$SERVER/api/device/uploads/$UPLOAD?offset=0" > /dev/null
CLIP=$(curl -fsS -H "authorization: Bearer $TOKEN" -X POST "$SERVER/api/device/uploads/$UPLOAD/finish" | sed -n 's/.*"clip_id":"\([^"]*\)".*/\1/p')
[ -n "$CLIP" ] || fail "the upload did not finish"
for _ in $(seq 1 120); do
  curl -fsS -H "authorization: Bearer $TOKEN" "$SERVER/api/device/clips/$CLIP" | grep -q '"ready":true' && break; sleep 0.5
done
BYTES=$(curl -fsS -b "$JAR" -o /dev/null -w '%{size_download}' "$SERVER/media/clips/$CLIP/download")
[ "$BYTES" -gt 0 ] || fail "the demo serves no download of the clip"

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
n=$(log_n "library loaded at=$HOST/clips/$CLIP")
"$APP" --page="/clips/$CLIP" > /dev/null 2>&1 &
wait_more 20 "the clip page" "library loaded at=$HOST/clips/$CLIP" "$n"
sleep 2
xdotool mousemove 640 880

xdotool key d
wait_more 20 "the page to hear the download start" "download shown in the page" 0
shown | head -1 | grep -q "^Downloading .*\.mp4, [0-9.]* [kMG]B" || fail "the page said \"$(shown | head -1)\" as the download started"
wait_more 30 "the download to end" "download saved" 0
wait_more 10 "the page to hear it saved" "download shown in the page" 1
sleep 0.5
shot saved
SAID=$(shown | sed -n 2p)
case "$SAID" in "Saved to Downloads: "*.mp4) ;; *) fail "the page said \"$SAID\" once the download was saved" ;; esac
FILE="$DL/${SAID#Saved to Downloads: }"
[ -f "$FILE" ] || fail "no $FILE in the Downloads folder: $(ls "$DL")"
[ "$(stat -c %s "$FILE")" -eq "$BYTES" ] || fail "the file has $(stat -c %s "$FILE") bytes, the server served $BYTES"
ok "Download on a clip page: the page said \"$(shown | head -1)\", then \"$SAID\"; the file in the Downloads folder has the server's $BYTES bytes"

chmod a-w "$DL"
xdotool key d
wait_more 30 "the page to hear the download fail" "did not download" 0
sleep 0.5
shot failed
SAID=$(shown | grep "did not download" | tail -1)
[ -n "$SAID" ] || fail "the failed download was not told to the page"
[ "$(log_n "download failed")" -gt 0 ] || fail "the app did not log the failure"
ok "the Downloads folder read-only: the page said \"$SAID\""
! grep -q "panicked" "$W/app.out" || fail "a thread panicked"
