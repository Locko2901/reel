#!/bin/sh
# Under xvfb.sh with XVFB_SCREEN=1280x900. A standalone AppImage, its newer release on the
# loopback, and "ask, then install": it asks by itself in a notification and in the window;
# Install now from the window, the notification and its link starts the install, Later does not,
# and a link without this run's value does nothing. The release's AppImage is signed with the
# tests' key, so the release key built into the app refuses it and nothing is replaced. With a
# session recording running (the stand-in OBS), the question says Install now waits for it, and
# the install goes on once it stops.
# Paired, against REEL_E2E_SERVER: the server's library asks too. Only a server newer than the app
# offers it an update, with the app's AppImage and its .sig in the server's app-updates folder; a
# server as new as the app offers none, and that part is left out with a note.
set -eu
APP="${1:-/target/debug/reel-desktop}"
SHOTS="${REEL_SHOTS_OUT:-}"
W=$(mktemp -d)
D="$W/d"
L="$D/data/logs"
N="$W/notifications.txt"
R="$W/release.txt"
VERSION=9.9.0
PIDS=""
fail() {
  echo "FAIL $*"
  grep -m1 -A1 "panicked" "$W"/app*.out 2>/dev/null || tail -n 20 "$W"/app*.out 2>/dev/null
  grep -h "update\|ask\|answer\|link\|install" "$L"/reel-desktop.log* 2>/dev/null | grep -v "db.statement" | tail -n 30
  tail -n 5 "$N" "$R"
  exit 1
}
ok() { echo "ok   $*"; }
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; }
trap cleanup EXIT
logged() { cat "$L"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
waitmore() { # waitmore <words> <seconds> <what> <count before>
  for _ in $(seq 1 $(($2 * 2))); do [ "$(logged "$1")" -gt "$4" ] && return 0; sleep 0.5; done
  fail "$3"
}
said() { grep -c -- "$1" "$N" || true; }
start() { "$APP" "$@" > "$W/app$(date +%s%N).out" 2>&1 & PIDS="$PIDS $!"; APP_PID=$!; }
stop() { kill "$APP_PID" 2>/dev/null || true; for _ in $(seq 1 40); do kill -0 "$APP_PID" 2>/dev/null || break; sleep 0.25; done; }
again() { "$APP" "$@" > /dev/null 2>&1 || true; }
window() { xdotool search --onlyvisible --name '^reel$' 2>/dev/null | head -1; }
size() { WIN=$(window); [ -n "$WIN" ] || fail "no window"; xdotool windowmove "$WIN" 0 0 windowsize "$WIN" "$1" "$2"; sleep 2; }
shot() { # shot <name>: the window as it is
  [ -n "$SHOTS" ] || return 0
  import -display "$DISPLAY" -window "$(window)" "$SHOTS/$1.png"
}
press() { # press <notification number> <action>
  echo "$1 $2" > "$W/press"
  for _ in $(seq 1 20); do [ -e "$W/press" ] || return 0; sleep 0.25; done
  fail "the stand-in did not press $2"
}
asked() { grep -c "^asked $1" "$R" || true; }
# the newest notification that asks, its number, and the link of a button on it
last_ask() { grep "is ready to install: " "$N" | tail -1; }
number() { last_ask | sed -n 's/.*\] #\([0-9]*\)$/\1/p'; }
link_of() { last_ask | sed -n "s#.*\[\(.*\)\].*#\1#p" | tr ',' '\n' | sed -n "s#^ *\(reel-desktop://$1/[A-Za-z0-9_-]*\)=.*#\1#p"; }
[ -z "$SHOTS" ] || mkdir -p "$SHOTS"

PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
python3 /work/desktop/ci/standin-release.py "$PORT" "$VERSION" /work/desktop/app/tests/release/reel.AppImage \
  /work/desktop/app/tests/release/reel.AppImage.sig > "$R" 2>&1 &
PIDS="$PIDS $!"
FAKE_NOTIFY_ACTIONS=1 FAKE_NOTIFY_PRESS="$W/press" python3 /work/desktop/ci/fake-notifications.py > "$N" 2>&1 &
PIDS="$PIDS $!"
for _ in $(seq 1 20); do grep -q "release up" "$R" && grep -q "notifications up" "$N" && break; sleep 0.25; done
mkdir -p "$D/config" "$W/clips"
cat > "$D/config/settings.json" <<JSON
{
  "mode": "standalone",
  "standalone": { "hwaccel": "none", "cap_gb": 0, "cap_keeps_unsent": true, "copies_on_hardware": false },
  "obs": { "enabled": false },
  "clips_dir": "$W/clips",
  "clips_follow_obs": false,
  "startup": { "autostart": false, "elevated": false },
  "log_level": "info"
}
JSON
printf 'the AppImage that runs' > "$W/reel.AppImage"
chmod +x "$W/reel.AppImage"
export REEL_DESKTOP_DIR="$D" REEL_GITHUB_API="http://127.0.0.1:$PORT" APPIMAGE="$W/reel.AppImage" RUST_BACKTRACE=1

start --hidden
waitmore "standalone: reel serves on the loopback" 60 "reel on this PC did not start" 0
sleep 2
stop
python3 - "$D/data/library/reel.db" <<'PY'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
db.execute("INSERT INTO setting (key, value) VALUES ('advanced.update_check', 'true') ON CONFLICT(key) DO UPDATE SET value = excluded.value")
db.commit()
PY
ok "a standalone library with Settings, Advanced, Check for updates on"

start
waitmore "standalone: reel serves on the loopback" 60 "reel on this PC did not start again" 1
waitmore "the app asks to install the update" 200 "the app did not ask by itself" 0
[ "$(asked /repos/)" -eq 1 ] || fail "reel on this PC read the release $(asked /repos/) times"
grep -h "the app asks to install the update" "$L"/reel-desktop.log* | grep -q 'window="open: its page asks"' || fail "the ask did not say the window asks"
for _ in $(seq 1 20); do [ "$(said "is ready to install")" -gt 0 ] && break; sleep 0.5; done
INSTALL=$(link_of install)
LATER=$(link_of later)
[ -n "$INSTALL" ] && [ -n "$LATER" ] || fail "the notification has no Install now and Later: $(last_ask)"
ok "asked by itself after reel on this PC read the release: $(last_ask | sed 's/^notification reel: //; s#/[A-Za-z0-9_-]\{22\}=#/<value>=#g')"
size 1280 820
shot "update-ask-library-1280x820-dark"
size 720 820
shot "update-ask-library-720x820-dark"
again --section=status
sleep 3
shot "update-ask-app-status-720x820-dark"
size 1280 820
shot "update-ask-app-status-1280x820-dark"
again --section=startup
sleep 3
shot "update-ask-app-startup-1280x820-dark"
ok "the window's question on the library and on the app's page (shots at 1280x820 and 720x820)"

answers=$(logged "the update's question answered")
ignored=$(logged "an update link without this run's value")
refused=$(logged "a reel-desktop link that is not one of the app's: ignored")
OTHER="AAAAAAAAAAAAAAAAAAAAAA"
again "reel-desktop://install/$OTHER"
again "reel-desktop://later/$OTHER"
again "reel-desktop://install/"
again "reel-desktop://install"
waitmore "an update link without this run's value" 20 "a link with another value was not ignored" $((ignored + 1))
waitmore "a reel-desktop link that is not one of the app's: ignored" 20 "a bare install link was not refused" $((refused + 1))
sleep 2
[ "$(logged "the update's question answered")" -eq "$answers" ] || fail "a link without this run's value answered the question"
[ "$(logged "installing the update")" -eq 0 ] || fail "a link without this run's value started the install"
ok "links with another value, and bare install links: ignored, nothing answered or installed"

press "$(number)" "$LATER"
waitmore 'the update.s question answered: Later.* from="the notification"' 20 "Later on the notification" 0
sleep 2
[ "$(logged "installing the update")" -eq 0 ] || fail "Later started the install"
again
sleep 3
size 1280 820
shot "update-ask-library-after-later-1280x820-dark"
ok "Later from the notification: answered, nothing installed, the question gone from the window"

again --section=startup
sleep 3
shot "update-ask-app-startup-after-later-1280x820-dark"
asks=$(logged "the app asks to install the update")
# shellcheck disable=SC2086
xdotool mousemove ${REEL_CHECK_NOW:-1095 523} click 1
waitmore "Check now found the update: Later holds the question no longer" 30 "Check now did not find the update (Check now at ${REEL_CHECK_NOW:-1095 523})" 0
waitmore "the app asks to install the update" 20 "Check now did not ask at once" "$asks"
for _ in $(seq 1 20); do [ "$(said "is ready to install")" -gt 1 ] && break; sleep 0.5; done
[ "$(said "is ready to install")" -eq 2 ] || fail "Check now after Later did not notify again"
sleep 2
# shellcheck disable=SC2086
xdotool mousemove ${REEL_ASK_LATER:-1224 86} click 1
waitmore "the update.s question answered: Later.* from=\"the window, on the app's page\"" 20 "Later on the app's page" 0
sleep 2
[ "$(logged "installing the update")" -eq 0 ] || fail "Later in the window started the install"
ok "Check now after Later asks at once; Later on the app's page: answered, nothing installed"

asks=$(logged "the app asks to install the update")
# shellcheck disable=SC2086
xdotool mousemove ${REEL_CHECK_NOW:-1095 523} click 1
waitmore "the app asks to install the update" 30 "the second Check now did not ask" "$asks"
for _ in $(seq 1 20); do [ "$(said "is ready to install")" -gt 2 ] && break; sleep 0.5; done
[ "$(link_of install)" = "$INSTALL" ] || fail "the run's value changed within the run"
installs=$(logged "installing the update")
fails=$(logged "the update did not download")
again "$INSTALL"
waitmore 'the update.s question answered: Install now.* from="a reel-desktop link"' 20 "Install now from the notification's link" 0
waitmore "installing the update" 30 "Install now from the link did not start the install" "$installs"
waitmore "the update did not download" 30 "the download was not refused" "$fails"
[ "$(asked /download/v$VERSION/reel_${VERSION}_amd64.AppImage)" -ge 1 ] || fail "the AppImage was not downloaded"
for _ in $(seq 1 20); do grep -q "release key built into this app" "$N" && break; sleep 0.5; done
grep -q "The update did not install: .*release key built into this app" "$N" || fail "the refusal does not name the release key"
ok "Install now from the notification's link: the install starts, and the release key refuses the tests' AppImage"

installs=$(logged "installing the update")
press "$(number)" "$INSTALL"
waitmore 'the update.s question answered: Install now.* from="the notification"' 20 "Install now on the notification" 0
waitmore "installing the update" 30 "Install now on the notification did not start the install" "$installs"
ok "Install now on the notification's button: the install starts"

for _ in $(seq 1 30); do [ "$(logged "the update did not download")" -gt "$((fails + 1))" ] && break; sleep 0.5; done
installs=$(logged "installing the update")
again
sleep 3
size 1280 820
shot "update-ask-library-again-1280x820-dark"
# shellcheck disable=SC2086
xdotool mousemove ${REEL_ASK_INSTALL:-1155 86} click 1
waitmore 'the update.s question answered: Install now.* from="the window, on the library"' 20 "Install now on the library" 0
waitmore "installing the update" 30 "Install now on the library did not start the install" "$installs"
ok "Install now on the library: the install starts"

[ "$(cat "$W/reel.AppImage")" = "the AppImage that runs" ] || fail "the AppImage that runs was replaced"
[ "$(grep -c '^asked' "$R")" -eq "$(grep -c '^asked .* by reel/' "$R")" ] || fail "a request to the release without reel's user agent"
! grep -q "panicked" "$W"/app*.out || fail "a thread panicked"
grep -h "the update's question answered\|the app asks to install\|an update link without" "$L"/reel-desktop.log* | sed 's/^[^ ]* *[A-Z]* /     /'
ok "nothing replaced the AppImage; every request to the release carried reel's user agent"
stop

# A session recording holds the install: the question says so, Install now waits, Stop lets it go on.
mkdir -p "$W/rec"
/work/desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu -v error -f lavfi -i "testsrc2=duration=3:size=640x360:rate=30" \
  -c:v libx264 -pix_fmt yuv420p "$W/clip.mp4" || fail "no clip for the stand-in OBS"
OBS_PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
/target/debug/examples/fake_obs --port "$OBS_PORT" --dir "$W/rec" --clip "$W/clip.mp4" > "$W/fake-obs.out" 2>&1 &
PIDS="$PIDS $!"
python3 - "$D/config/settings.json" "$OBS_PORT" "$W/rec" <<'PY'
import json, sys
path, port, rec = sys.argv[1], int(sys.argv[2]), sys.argv[3]
s = json.load(open(path))
s["obs"] = {"enabled": True, "host": "127.0.0.1", "port": port, "password": "", "start_replay": False}
s.update({"clips_dir": rec, "clips_follow_obs": True, "stable_seconds": 2})
json.dump(s, open(path, "w"))
PY
asks=$(logged "the app asks to install the update")
answers=$(logged 'the update.s question answered: Install now.* from="a reel-desktop link"')
start
waitmore "obs connected" 60 "the app did not connect to the stand-in OBS" 0
again --start-session
waitmore "session recording started from the command line" 20 "--start-session did not start a session recording" 0
waitmore "the app asks to install the update" 200 "the app did not ask while recording" "$asks"
for _ in $(seq 1 20); do [ "$(said "is ready to install: Install now waits for the session recording to stop, then closes reel")" -gt 0 ] && break; sleep 0.5; done
[ "$(said "is ready to install: Install now waits for the session recording to stop, then closes reel")" -eq 1 ] \
  || fail "the question while recording does not say that Install now waits: $(last_ask)"
size 1280 820
shot "update-ask-library-recording-1280x820-dark"
ok "asked while a session recording runs: $(last_ask | sed 's/^notification reel: //; s/ \[.*//')"
installs=$(logged "installing the update")
fails=$(logged "the update did not download")
again "$(link_of install)"
waitmore 'the update.s question answered: Install now.* from="a reel-desktop link"' 20 "Install now from the link while recording" "$answers"
waitmore "the update waits: the session recording" 20 "Install now did not wait for the session recording" 0
for _ in $(seq 1 20); do grep -q "reel $VERSION waits for the session recording: It installs once the session recording stops." "$N" && break; sleep 0.5; done
grep -q "reel $VERSION waits for the session recording: It installs once the session recording stops." "$N" \
  || fail "Install now while recording did not say that it waits: $(tail -n 1 "$N")"
again --section=startup
sleep 6
shot "update-ask-app-startup-recording-1280x820-dark"
[ "$(logged "installing the update")" -eq "$installs" ] || fail "the update installed while the session recording ran"
ok "Install now while recording: it waits and says so ($(tail -n 1 "$N" | sed 's/^notification reel: //; s/ \[.*//'))"
again --stop-session
waitmore "session recording stopped from the command line" 20 "--stop-session did not stop the session recording" 0
waitmore "installing the update" 30 "the install did not go on once the session recording stopped" "$installs"
waitmore "the update did not download" 30 "the download after the session recording was not refused" "$fails"
cat "$L"/reel-desktop.log* | grep "session recording stopped from the command line\|installing the update" | tail -n 2 | head -n 1 \
  | grep -q "session recording stopped" || fail "the install went on before the session recording stopped"
! grep -q "panicked" "$W"/app*.out || fail "a thread panicked"
ok "Stop: the session recording's file is whole, then the install goes on (and the release key refuses the tests' AppImage)"
stop

SERVER="${REEL_E2E_SERVER:-}"
APP_VERSION=$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' /work/desktop/app/tauri.conf.json | head -1)
SERVER_VERSION=""
[ -z "$SERVER" ] || SERVER_VERSION=$(curl -fsS --max-time 5 "$SERVER/healthz" 2>/dev/null | sed -n 's/.*"version":"\([^"]*\)".*/\1/p' || true)
newer() { [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -1)" = "$1" ]; }
if [ -z "$SERVER_VERSION" ] || ! newer "$SERVER_VERSION" "$APP_VERSION"; then
  echo "note the paired part needs a server newer than this app ($APP_VERSION) with its AppImage: ${SERVER:-no server} runs ${SERVER_VERSION:-nothing that answers}; left out"
  echo "the update's question: all checks passed"
  exit 0
fi
USER_NAME="${REEL_E2E_USER:-sam}"
PASSWORD="${REEL_E2E_PASSWORD:?the password of the demo admin}"
HOST=$(echo "$SERVER" | sed -E 's#https?://([^/]+).*#\1#; s#:[0-9]+$##')
JAR="$W/jar"
curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"update question\"}" "$SERVER/api/device/pair" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no token from $SERVER"
D="$W/paired"
L="$D/data/logs"
mkdir -p "$D/config" "$D/data" "$W/clips-paired"
printf '%s' "$TOKEN" > "$D/config/token"
chmod 600 "$D/config/token"
printf '{"server_url":"%s","obs":{"enabled":false},"clips_dir":"%s","clips_follow_obs":false,"startup":{"autostart":false,"elevated":false},"log_level":"info"}' \
  "$SERVER" "$W/clips-paired" > "$D/config/settings.json"
export REEL_DESKTOP_DIR="$D"
start
waitmore "library loaded at=$HOST/signin" 60 "the server's sign-in page in the window" 0
size 1280 820
xdotool windowactivate --sync "$(window)" 2>/dev/null || true
# shellcheck disable=SC2086
xdotool mousemove ${REEL_SIGNIN:-640 328} click 1
xdotool type --delay 30 "$USER_NAME"
xdotool key Tab
xdotool type --delay 30 "$PASSWORD"
xdotool key Return
waitmore "library loaded at=$HOST/\$" 30 "the server's library after the sign-in" 0
waitmore "the app asks to install the update" 120 "the paired app did not ask" 0
grep -h "an update of the app is ready" "$L"/reel-desktop.log* | grep -q 'from="the server"' || fail "the update did not come from the server"
xdotool mousemove 640 880
sleep 4
shot "update-ask-server-library-1280x820-dark"
size 720 820
shot "update-ask-server-library-720x820-dark"
size 1280 820
sleep 2
ok "paired with $SERVER ($SERVER_VERSION): the server offered its version, and the app asked (shots of the server's library)"
installs=$(logged "installing the update")
fails=$(logged "the update did not download")
# shellcheck disable=SC2086
xdotool mousemove ${REEL_ASK_INSTALL:-1155 86} click 1
waitmore "the update.s question answered: Install now.* from=\"the window, on the server's library\"" 20 "Install now on the server's library" 0
waitmore "installing the update" 30 "Install now on the server's library did not start the install" "$installs"
waitmore "the update did not download" 30 "the server's AppImage was not refused" "$fails"
ok "Install now on the server's library: the install starts, and the release key refuses the tests' AppImage"
installs=$(logged "installing the update")
xdotool mousemove 640 880
sleep 4
# shellcheck disable=SC2086
xdotool mousemove ${REEL_ASK_LATER:-1224 86} click 1
waitmore "the update.s question answered: Later.* from=\"the window, on the server's library\"" 20 "Later on the server's library" 0
sleep 2
[ "$(logged "installing the update")" -eq "$installs" ] || fail "Later on the server's library started the install"
xdotool mousemove 640 880
sleep 3
shot "update-ask-server-library-after-later-1280x820-dark"
! grep -q "panicked" "$W"/app*.out || fail "a thread panicked"
ok "Later on the server's library: answered, nothing installed"
stop
echo "the update's question: all checks passed"
