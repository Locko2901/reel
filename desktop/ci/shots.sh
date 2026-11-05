#!/usr/bin/env bash
#   XVFB_SCREEN=1280x900 desktop/ci/xvfb.sh run desktop/ci/shots.sh
set -euo pipefail
SERVER="${REEL_E2E_SERVER:-http://127.0.0.1:8746}"
USER_NAME="${REEL_E2E_USER:-sam}"
PASSWORD="${REEL_E2E_PASSWORD:?the demo admin password}"
APP="${REEL_E2E_APP:-/target/debug/reel-desktop}"
OUT="${REEL_SHOTS_OUT:-/work/shots/desktop}"
ROOT="${REEL_E2E_DIR:-/tmp/reel-shots}"
rm -rf "$ROOT"
mkdir -p "$ROOT/config" "$ROOT/data" "$ROOT/clips" "$OUT"
JAR="$ROOT/cookies"
LOG="$ROOT/data/logs"
fail() { printf '\nFAILED: %s\n' "$*"; cat "$LOG"/reel-desktop.log* 2>/dev/null | tail -20; exit 1; }
log_n() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
wait_more() { # wait_more <seconds> <description> <log pattern> <count before>
  local secs=$1 what=$2 pat=$3 before=$4
  for _ in $(seq 1 "$secs"); do
    if [ "$(log_n "$pat")" -gt "$before" ]; then return 0; fi
    sleep 1
  done
  fail "timed out waiting for $what"
}
HOST=$(echo "$SERVER" | sed -E 's#https?://([^:/]+).*#\1#')

curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
PAIRED=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"shots\"}" "$SERVER/api/device/pair")
TOKEN=$(echo "$PAIRED" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no token: $PAIRED"
printf '%s' "$TOKEN" > "$ROOT/config/token"
chmod 600 "$ROOT/config/token"
cat > "$ROOT/config/settings.json" <<JSON
{ "server_url": "$SERVER", "device_name": "shots", "obs": { "enabled": false }, "clips_dir": "$ROOT/clips",
  "startup": { "autostart": false, "elevated": false }, "log_level": "info" }
JSON
export REEL_DESKTOP_DIR="$ROOT"
"$APP" >"$ROOT/app.out" 2>&1 &
APP_PID=$!
trap 'kill $APP_PID 2>/dev/null || true' EXIT
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
xdotool mousemove 640 880

shoot() { # shoot <name>
  for size in 1280x800 720x850; do
    xdotool windowsize "$WIN" "${size%x*}" "${size#*x}"
    sleep 1.5
    import -display "$DISPLAY" -window "$WIN" "$OUT/$1-$size.png"
    echo "shot $OUT/$1-$size.png"
  done
}
sleep 1
shoot library
n=$(log_n "library loaded at=$HOST/collections")
"$APP" --page=/collections >/dev/null 2>&1 &
wait_more 20 "the Collections page" "library loaded at=$HOST/collections" "$n"
sleep 1
shoot collections
n=$(log_n "library loaded at=$HOST/settings/general")
"$APP" --page=/settings/general >/dev/null 2>&1 &
wait_more 20 "the server's settings" "library loaded at=$HOST/settings/general" "$n"
sleep 1
shoot settings-general
n=$(log_n "library loaded at=$HOST/clips/")
"$APP" --page=/__shot/clip >/dev/null 2>&1 &
wait_more 20 "a clip page" "library loaded at=$HOST/clips/" "$n"
sleep 2
xdotool mousemove 640 400
xdotool click --repeat 8 --delay 80 5
xdotool mousemove 640 880
shoot clip-edit
n=$(log_n "library loaded at=localhost/index.html")
"$APP" --section=status >/dev/null 2>&1 &
wait_more 20 "the app's Status page" "library loaded at=localhost/index.html" "$n"
sleep 1
shoot this-app
n=$(log_n "library loaded at=localhost/index.html")
"$APP" --section=obs >/dev/null 2>&1 &
sleep 2
shoot this-app-obs

kill $APP_PID 2>/dev/null || true
sleep 2
ROOT2="$ROOT-offline"
rm -rf "$ROOT2"
mkdir -p "$ROOT2/config" "$ROOT2/data" "$ROOT2/clips"
LOG="$ROOT2/data/logs"
cat > "$ROOT2/config/settings.json" <<JSON
{ "server_url": "http://127.0.0.1:1", "obs": { "enabled": false }, "clips_dir": "$ROOT2/clips", "clips_follow_obs": false,
  "startup": { "autostart": false, "elevated": false }, "log_level": "info" }
JSON
export REEL_DESKTOP_DIR="$ROOT2"
"$APP" >"$ROOT2/app.out" 2>&1 &
APP_PID=$!
trap 'kill $APP_PID 2>/dev/null || true' EXIT
wait_more 40 "the fallback page" 'window shows view="offline"' 0
sleep 2
WIN=$(xdotool search --name '^reel$' | head -1 || true)
[ -n "$WIN" ] || fail "no window for the fallback"
xdotool windowmove "$WIN" 0 0 2>/dev/null || true
xdotool mousemove 640 880
shoot offline
echo "done"
