#!/usr/bin/env bash
# Under xvfb.sh against a demo with a ready clip; REEL_SHOTS_OUT for screenshots.
set -euo pipefail
APP="${1:-/target/debug/reel-desktop}"
SERVER="${REEL_E2E_SERVER:-http://127.0.0.1:8746}"
USER_NAME="${REEL_E2E_USER:-sam}"
PASSWORD="${REEL_E2E_PASSWORD:?the password of the demo admin}"
SHOTS="${REEL_SHOTS_OUT:-}"
W=$(mktemp -d)
LOG="$W/d/data/logs"
JAR="$W/jar"
fail() { echo "FAIL $*"; grep -m1 -A1 "panicked" "$W/app.out" 2>/dev/null || tail -20 "$W/app.out" 2>/dev/null; grep "library loaded" "$LOG"/reel-desktop.log* 2>/dev/null | tail -8; exit 1; }
ok() { echo "ok   $*"; }
log_n() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
wait_more() { # wait_more <seconds> <what> <log words> <count before>
  for _ in $(seq 1 $(($1 * 2))); do [ "$(log_n "$3")" -gt "$4" ] && return 0; sleep 0.5; done
  fail "$2"
}
shot() { [ -z "$SHOTS" ] || import -display "$DISPLAY" -window "$WIN" "$SHOTS/$1.png"; }
HOST=$(echo "$SERVER" | sed -E 's#https?://([^/]+).*#\1#; s#:[0-9]+$##')
LIBRARY="library loaded at=$HOST/$"
CLIP="library loaded at=$HOST/clips/"
[ -z "$SHOTS" ] || mkdir -p "$SHOTS"
# goes <what> <log words> <action...>: the action, then that page loaded once more
goes() {
  local what="$1" words="$2"; shift 2
  local n; n=$(log_n "$words")
  "$@"
  wait_more 15 "$what" "$words" "$n"
  sleep 1.5
  last=$(cat "$LOG"/reel-desktop.log* | grep "library loaded" | tail -1)
  printf '%s\n' "$last" | grep -q -- "$words" || fail "$what, then on to another page: $last"
  ok "$what"
}

curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
curl -fsS -b "$JAR" "$SERVER/" | grep -q 'data-state="ready"' || fail "no ready clip on the demo"
CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"back\"}" "$SERVER/api/device/pair" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no token from the demo"

mkdir -p "$W/d/config" "$W/d/data"
printf '%s' "$TOKEN" > "$W/d/config/token"
chmod 600 "$W/d/config/token"
printf '{"server_url":"%s","obs":{"enabled":false},"clips_dir":"%s","clips_follow_obs":false,"startup":{"autostart":false},"log_level":"info"}' \
  "$SERVER" "$W/clips" > "$W/d/config/settings.json"
export REEL_DESKTOP_DIR="$W/d" RUST_BACKTRACE=1
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
goes "the library after the sign-in" "$LIBRARY" xdotool key Return
sleep 2
shot library

# the first card of the library (its poster), the clip page with Library on its button
goes "a card opens its clip" "$CLIP" xdotool mousemove 160 260 click 1
shot clip-from-library
goes "the clip page's button goes back to the library" "$LIBRARY" xdotool mousemove 70 75 click 1
goes "the mouse's forward button goes to the clip again" "$CLIP" xdotool mousemove 640 500 click 9
goes "the mouse's back button goes back to the library" "$LIBRARY" xdotool click 8
goes "Alt+Right goes to the clip again" "$CLIP" xdotool key alt+Right
goes "Alt+Left goes back to the library" "$LIBRARY" xdotool key alt+Left

goes "Settings, Status opens in the window" "library loaded at=localhost/" "$APP" --section=status
shot app-status
goes "the mouse's back button goes from it to the library" "$LIBRARY" xdotool mousemove 640 500 click 8
! grep -q "panicked" "$W/app.out" || fail "a thread panicked"
