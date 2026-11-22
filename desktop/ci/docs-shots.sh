#!/bin/sh
#   XVFB_SCREEN=1280x900 desktop/ci/xvfb.sh run sh desktop/ci/docs-shots.sh
# The docs' pictures of the app's own pages, from the real Linux app paired with the server at
# REEL_E2E_SERVER (best a fresh one, its clips none but the one sent here), in the app's first
# window, 1280x820: the stand-in OBS connected, one clip sent with Save and share. Each picture
# lands at its place under docs/ in 256 colours, as the docs keep them. The pictures in Windows'
# words come from ci/browser/app-pages.mjs.
set -eu
SERVER="${REEL_E2E_SERVER:-http://127.0.0.1:8746}"
USER_NAME="${REEL_E2E_USER:-sam}"
PASSWORD="${REEL_E2E_PASSWORD:?the admin password}"
APP_BIN="${REEL_E2E_APP:-/target/debug/reel-desktop}"
FAKE_OBS="${REEL_FAKE_OBS:-/target/debug/examples/fake_obs}"
DOCS="${REEL_DOCS_OUT:-/work/docs}"
FFMPEG=/work/desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu
# the paths the pictures show are a made-up user's
[ -w /home/sam ] && export HOME=/home/sam
W=$(mktemp -d)
D="$W/pc"
L="$D/data/logs"
REC="$HOME/Videos/reel"
mkdir -p "$D/config" "$REC" "$HOME/.local/bin"
cp "$APP_BIN" "$HOME/.local/bin/reel-desktop"
APP="$HOME/.local/bin/reel-desktop"
PIDS=""
fail() { echo "FAIL $*"; tail -n 30 "$L"/reel-desktop.log* 2>/dev/null; exit 1; }
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; rm -rf "$W"; }
trap cleanup EXIT
logged() { cat "$L"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
waitlog() { # waitlog <words> <seconds> <what> [count before]
  for _ in $(seq 1 $(($2 * 2))); do [ "$(logged "$1")" -gt "${4:-0}" ] && return 0; sleep 0.5; done
  fail "$3"
}
window() { xdotool search --onlyvisible --name '^reel$' 2>/dev/null | head -1; }
shot() { # shot <path under docs>
  win=$(window)
  [ -n "$win" ] || fail "no window for $1"
  xdotool mousemove 1279 899
  sleep 0.3
  import -display "$DISPLAY" -window "$win" "$W/shot.png"
  mkdir -p "$(dirname "$DOCS/$1")"
  "$FFMPEG" -y -v error -i "$W/shot.png" -vf 'split[a][b];[a]palettegen=max_colors=256:reserve_transparent=0[p];[b][p]paletteuse=dither=none' "$DOCS/$1"
  echo "shot $1"
}
page() { # page <path under docs> <the app's page>
  b=$(logged "app page section=${2%%/*}")
  "$APP" "--section=$2" > /dev/null 2>&1 || true
  waitlog "app page section=${2%%/*}" 20 "$2 did not open" "$b"
  sleep 2
  shot "$1"
}
# the app's log names the host without its port
HOST=$(echo "$SERVER" | sed -E 's#https?://([^:/]+).*#\1#')

"$FFMPEG" -v error -f lavfi -i "testsrc2=duration=4:size=1280x720:rate=30" -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=4" \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest "$W/clip.mp4"
JAR="$W/cookies"
curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"GAMING-PC\"}" "$SERVER/api/device/pair" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no token"
printf '%s' "$TOKEN" > "$D/config/token"
chmod 600 "$D/config/token"
cat > "$D/config/settings.json" <<JSON
{ "mode": "connected", "server_url": "$SERVER", "device_name": "GAMING-PC",
  "obs": { "enabled": true, "host": "127.0.0.1", "port": 4466, "password": "", "start_replay": false },
  "hotkeys": { "save": "Alt+F10" },
  "clips_follow_obs": true, "clips_dir": "$REC", "stable_seconds": 2,
  "startup": { "autostart": false, "elevated": false }, "log_level": "info" }
JSON
python3 /work/desktop/ci/fake-notifications.py > "$W/notifications.txt" 2>&1 &
PIDS="$PIDS $!"
"$FAKE_OBS" --port 4466 --dir "$REC" --clip "$W/clip.mp4" > "$W/fake-obs.out" 2>&1 &
PIDS="$PIDS $!"
export REEL_DESKTOP_DIR="$D"
"$APP" > "$W/app.out" 2>&1 &
PIDS="$PIDS $!"
waitlog "library loaded at=$HOST/signin" 30 "the sign-in page in the window"
waitlog "obs connected" 30 "the app did not connect to the stand-in OBS"
sleep 2
win=$(window)
[ -n "$win" ] || fail "no window"
xdotool windowmove "$win" 0 0 windowsize "$win" 1280 820 2>/dev/null || true
sleep 1.5
xdotool mousemove 640 328 click 1
xdotool type --delay 30 "$USER_NAME"
xdotool key Tab
xdotool type --delay 30 "$PASSWORD"
b=$(logged "library loaded at=$HOST/$")
xdotool key Return
waitlog "library loaded at=$HOST/$" 20 "the library after the sign-in" "$b"
b=$(logged "link copied")
"$APP" --save-and-share > /dev/null 2>&1 || true
waitlog "link copied" 120 "Save and share did not copy a link" "$b"
sleep 2
page img/app-log.png status/log
page img/app-status.png status
page setup/img/app-settings.png first
page setup/img/app-hotkeys-linux.png hotkeys
page setup/img/app-settings-capture.png capture
page setup/img/app-settings-connection.png server
echo done
