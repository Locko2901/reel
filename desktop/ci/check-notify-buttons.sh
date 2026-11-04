#!/usr/bin/env bash
# Under xvfb.sh against a demo.
set -euo pipefail
APP="${1:-/target/debug/reel-desktop}"
SERVER="${REEL_E2E_SERVER:-http://127.0.0.1:8746}"
USER_NAME="${REEL_E2E_USER:-sam}"
PASSWORD="${REEL_E2E_PASSWORD:?the password of the demo admin}"
FFMPEG=/work/desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu
W=$(mktemp -d)
D="$W/d"
JAR="$W/jar"
N="$W/notifications.txt"
APP_PID=""
fail() { echo "FAIL $*"; grep -m1 -A1 "panicked" "$W"/app*.out 2>/dev/null || tail -20 "$W"/app*.out 2>/dev/null; tail -20 "$D"/data/logs/reel-desktop.log* 2>/dev/null; tail -5 "$N"; exit 1; }
ok() { echo "ok   $*"; }
log_n() { cat "$D"/data/logs/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
wait_more() { # wait_more <seconds> <what> <log words> <count before>
  for _ in $(seq 1 $(($1 * 2))); do [ "$(log_n "$3")" -gt "$4" ] && return 0; sleep 0.5; done
  fail "$2"
}
said() { grep -c -- "$1" "$N" || true; }
wait_said() { # wait_said <seconds> <what> <words> <count before>
  for _ in $(seq 1 $(($1 * 2))); do [ "$(said "$2")" -gt "$4" ] && return 0; sleep 0.5; done
  fail "$3"
}
cleanup() {
  [ -z "$APP_PID" ] || kill "$APP_PID" 2>/dev/null || true
  for c in $(grep -h "upload confirmed" "$W"/d*/data/logs/reel-desktop.log* 2>/dev/null | sed -n 's/.* clip=\([0-9A-Z]*\).*/\1/p'); do
    curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X DELETE "$SERVER/api/clips/$c" > /dev/null 2>&1 || true
  done
  curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/storage/purge-trash" > /dev/null 2>&1 || true
}
trap cleanup EXIT
HOST=$(echo "$SERVER" | sed -E 's#https?://([^/]+).*#\1#; s#:[0-9]+$##')
clip() { # clip <folder> <name>: a replay of its own, so the server takes it as new
  "$FFMPEG" -v error -f lavfi -i "testsrc2=duration=3:size=640x360:rate=30" -f lavfi -i "sine=frequency=$((RANDOM % 500 + 200)):sample_rate=48000:duration=3" \
    -metadata comment="$(od -An -N8 -tx8 /dev/urandom | tr -d ' ')" -c:v libx264 -pix_fmt yuv420p -c:a aac -f matroska "$1/$2.part"
  mv "$1/$2.part" "$1/$2"
}
start() { # start <args...>: the app, its own log folder
  "$APP" "$@" > "$W/app$(date +%s%N).out" 2>&1 &
  APP_PID=$!
}
stop() {
  kill "$APP_PID" 2>/dev/null || true
  for _ in $(seq 1 40); do kill -0 "$APP_PID" 2>/dev/null || break; sleep 0.25; done
  APP_PID=""
}
again() { "$APP" "$@" > /dev/null 2>&1 || true; }
press() { # press <notification number> <action>
  echo "$1 $2" > "$W/press"
  for _ in $(seq 1 20); do [ -e "$W/press" ] || return 0; sleep 0.25; done
  fail "the stand-in did not press $2"
}
number() { grep -- "$1" "$N" | tail -1 | sed -n 's/.*\] #\([0-9]*\)$/\1/p'; }

curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"notification buttons\"}" "$SERVER/api/device/pair" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no token from the demo"
mkdir -p "$D/config" "$D/data" "$W/clips"
printf '%s' "$TOKEN" > "$D/config/token"
chmod 600 "$D/config/token"
printf '{"server_url":"%s","obs":{"enabled":false},"clips_dir":"%s","clips_follow_obs":false,"stable_seconds":2,"startup":{"autostart":false},"log_level":"debug"}' \
  "$SERVER" "$W/clips" > "$D/config/settings.json"
FAKE_NOTIFY_ACTIONS=1 FAKE_NOTIFY_PRESS="$W/press" python3 /work/desktop/ci/fake-notifications.py > "$N" 2>&1 &
for _ in $(seq 1 20); do grep -q "notifications up" "$N" && break; sleep 0.25; done
export REEL_DESKTOP_DIR="$D" RUST_BACKTRACE=1
start --hidden
wait_more 30 "the app to start" "reel desktop starts" 0
sleep 2

clip "$W/clips" "Replay_2026-11-01_10-00-00.mkv"
wait_more 90 "the first clip to go up" "upload confirmed" 0
wait_said 20 "Clip uploaded: .*\[copy=Copy link, open=Open\]" "no Clip uploaded with Copy link and Open" 0
ONE=$(grep -h "upload confirmed" "$D"/data/logs/reel-desktop.log* | sed -n 's/.* clip=\([0-9A-Z]*\).*/\1/p' | head -1)
ok "Clip uploaded with Copy link and Open: $(grep "Clip uploaded" "$N" | tail -1 | sed 's/^notification reel: //')"
n=$(log_n "link copied")
press "$(number "Clip uploaded")" copy
wait_more 60 "Copy link to copy the clip's link" "link copied" "$n"
cat "$D"/data/logs/reel-desktop.log* | grep "link copied" | tail -1 | grep -q "clip=$ONE" || fail "Copy link copied another clip's link"
grep -q "Link copied. Go paste it." "$N" || fail "no Link copied notification"
ok "Copy link copied the first clip's link, and said so"

clip "$W/clips" "Replay_2026-11-01_10-05-00.mkv"
wait_more 90 "the second clip to go up" "upload confirmed" 1
for _ in $(seq 1 40); do [ "$(said "Clip uploaded")" -ge 2 ] && break; sleep 0.5; done
TWO=$(grep -h "upload confirmed" "$D"/data/logs/reel-desktop.log* | sed -n 's/.* clip=\([0-9A-Z]*\).*/\1/p' | tail -1)
n=$(log_n "library goes to to=$HOST/clips/$TWO")
press "$(number "Clip uploaded")" open
wait_more 30 "Open to open the clip's page" "library goes to to=$HOST/clips/$TWO" "$n"
ok "Open opened the second clip's page in the window"

n=$(log_n "library goes to to=$HOST/clips/$ONE")
again "reel-desktop://open/$ONE"
wait_more 20 "a second start's open link" "library goes to to=$HOST/clips/$ONE" "$n"
ok "reel-desktop://open/<clip> from a second start opened it"
n=$(log_n "link copied")
again "reel-desktop://copy/$TWO/"
wait_more 60 "a second start's copy link" "link copied" "$n"
ok "reel-desktop://copy/<clip>/ copied its link"
pages=$(log_n "library goes to")
ignored=$(log_n "ignored")
again "reel-desktop://open/01HZZZZZZZZZZZZZZZZZZZZZZZ"
again "reel-desktop://delete/$ONE"
again "reel-desktop://open/$ONE?then=delete"
wait_more 10 "the three other links to be ignored" "ignored" $((ignored + 2))
sleep 2
[ "$(log_n "library goes to")" -eq "$pages" ] || fail "a link that is none opened a page"
[ "$(curl -fsS -o /dev/null -w '%{http_code}' -b "$JAR" "$SERVER/api/clips/$ONE")" = 200 ] || fail "the clip is gone"
ok "a clip it does not know, another verb, a query: ignored, no page opened, the clip as it was"
n=$(log_n "library goes to to=$HOST/clips/$ONE")
again "reel-desktop://open/$ONE" --quit-for-install
wait_more 20 "a link with a flag after it" "library goes to to=$HOST/clips/$ONE" "$n"
sleep 2
kill -0 "$APP_PID" 2>/dev/null || fail "a flag after a link made the app quit"
ok "a flag after a link is not acted on: the link alone, the app still running"

stop
n=$(log_n "link copied")
start "reel-desktop://copy/$ONE"
wait_more 60 "a link to start the app and copy" "link copied" "$n"
grep -h "start: no window: started hidden" "$D"/data/logs/reel-desktop.log* > /dev/null || fail "the start for a link showed the window"
ok "with the app not running, reel-desktop://copy/<clip> started it, hidden, and copied the link"
stop
n=$(log_n "library goes to to=$HOST/clips/$TWO")
start "reel-desktop://open/$TWO"
wait_more 60 "a link to start the app and open" "library goes to to=$HOST/clips/$TWO" "$n"
ok "with the app not running, reel-desktop://open/<clip> started it on the clip's page"
stop

D="$W/d2"
mkdir -p "$D/config" "$D/data" "$W/clips2"
cat > "$D/config/settings.json" <<JSON
{
  "mode": "standalone",
  "standalone": { "hwaccel": "none", "cap_gb": 0, "cap_keeps_unsent": true, "copies_on_hardware": false },
  "obs": { "enabled": false },
  "clips_dir": "$W/clips2",
  "clips_follow_obs": false,
  "stable_seconds": 2,
  "startup": { "autostart": false, "elevated": false },
  "log_level": "debug"
}
JSON
export REEL_DESKTOP_DIR="$D"
start --hidden
wait_more 60 "reel on the loopback" "standalone: reel serves on the loopback" 0
clip "$W/clips2" "Replay_2026-11-01_11-00-00.mkv"
wait_more 120 "the clip in the library" "standalone: the clip is published" 0
wait_said 20 "Clip in the library: .*\[open=Open\]" "no Clip in the library with Open" 0
! grep "Clip in the library" "$N" | grep -q "copy=Copy link" || fail "Clip in the library has Copy link"
LOCAL=$(grep -h "standalone: the clip is published" "$D"/data/logs/reel-desktop.log* | sed -n 's/.* clip=\([0-9A-Z]*\).*/\1/p' | head -1)
press "$(number "Clip in the library")" open
wait_more 30 "Open to open the clip in the library on this PC" "library loaded at=127.0.0.1/clips/$LOCAL" 0
ok "standalone: Clip in the library's Open opened the clip on this PC"
before=$(said "Copy link is not part of standalone")
again "reel-desktop://copy/$LOCAL"
wait_said 20 "Copy link is not part of standalone" "an old notification's Copy link said nothing in standalone" "$before"
ok "standalone: an old notification's Copy link is refused with a word"
! grep -q "panicked" "$W"/app*.out || fail "a thread panicked"
