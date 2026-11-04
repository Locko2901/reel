#!/bin/sh
# Under xvfb.sh against a demo.
set -eu
APP="${1:-/target/debug/reel-desktop}"
SERVER="${REEL_E2E_SERVER:-http://127.0.0.1:8746}"
USER_NAME="${REEL_E2E_USER:-sam}"
PASSWORD="${REEL_E2E_PASSWORD:?the password of the demo admin}"
W=$(mktemp -d)
[ -x /target/debug/examples/fake_obs ] || { echo "FAIL no stand-in OBS: make desktop-linux builds it"; exit 1; }
L="$W/d/data/logs"
fail() { echo "FAIL $*"; grep -m1 -A1 "panicked" "$W/app.out" || tail -20 "$W/app.out"; tail -15 "$L"/reel-desktop.log* 2>/dev/null; exit 1; }
cleanup() {
  for c in $(grep -h "upload confirmed" "$L"/reel-desktop.log* 2>/dev/null | sed -n 's/.* clip=\([0-9A-Z]*\).*/\1/p'); do
    curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X DELETE "$SERVER/api/clips/$c" > /dev/null 2>&1 || true
  done
  curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/storage/purge-trash" > /dev/null 2>&1 || true
}
ok() { echo "ok   $*"; }
log_has() { grep -qs -- "$1" "$L"/reel-desktop.log*; }
wait_log() { # wait_log <seconds> <words> <what>
  for _ in $(seq 1 $(($1 * 2))); do log_has "$2" && return 0; sleep 0.5; done
  fail "$3"
}
# two replays of their own, so the demo takes each as new
for n in 1 2; do
  /work/desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu -v error -f lavfi -i "testsrc2=duration=3:size=640x360:rate=30" \
    -f lavfi -i "sine=frequency=$((300 + n * 100)):sample_rate=48000:duration=3" \
    -metadata comment="$(od -An -N8 -tx8 /dev/urandom | tr -d ' ')" -c:v libx264 -pix_fmt yuv420p -c:a aac "$W/clip$n.mp4"
done
mkdir -p "$W/d/config" "$W/rec"
JAR="$W/jar"
trap cleanup EXIT
curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"waited upload\"}" "$SERVER/api/device/pair" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no token from the demo"
printf '%s' "$TOKEN" > "$W/d/config/token"
chmod 600 "$W/d/config/token"
python3 /work/desktop/ci/fake-notifications.py > "$W/notifications.txt" 2>&1 &
python3 /work/desktop/ci/fake-sni.py > "$W/sni.txt" 2>&1 &
/target/debug/examples/fake_obs --port 4498 --dir "$W/rec" --clip "$W/clip1.mp4" > "$W/fake-obs.out" 2>&1 &
printf '{"server_url":"%s","obs":{"enabled":true,"host":"127.0.0.1","port":4498,"password":"","start_replay":false},"clips_follow_obs":true,"clips_dir":"%s","startup":{"autostart":false},"log_level":"debug"}' "$SERVER" "$W/rec" \
  > "$W/d/config/settings.json"
export REEL_DESKTOP_DIR="$W/d" RUST_BACKTRACE=1
"$APP" > "$W/app.out" 2>&1 &
wait_log 60 "obs connected" "the app did not connect to the stand-in OBS"
for _ in $(seq 1 20); do grep -q "^item " "$W/sni.txt" && break; sleep 0.5; done
sleep 3

python3 /work/desktop/ci/fake-sni.py click "Pause uploads" || fail "no Pause uploads in the tray's menu"
sleep 1
echo "$W/clip1.mp4" > "$W/rec/next-clip"
python3 /work/desktop/ci/fake-sni.py click "Save clip" || fail "no Save clip in the tray's menu"
wait_log 5 "uploads paused by the user" "the tray's Pause uploads did not pause"
wait_log 20 "upload paused" "the tray's Save clip gave no held upload"
ok "uploads paused from the tray: the replay of Save clip holds"

echo "$W/clip2.mp4" > "$W/rec/next-clip"
sleep 1.2 # past a double tap
xdotool key --clearmodifiers ctrl+alt+shift+s
wait_log 20 "a waited upload: past the queue and the pause" "Save and share's upload did not go at once"
wait_log 60 "link copied" "Save and share gave no link while the other upload held"
grep "upload confirmed" "$L"/reel-desktop.log* | grep -q "Replay fake 2" || fail "the link came, but not for Save and share's replay"
! grep "upload confirmed" "$L"/reel-desktop.log* | grep -q "Replay fake 1" || fail "the held upload went up while uploads were paused"
ok "Save and share's replay went up past the pause and the held upload, and its link was copied"

python3 /work/desktop/ci/fake-sni.py click "Resume uploads" || fail "no Resume uploads in the tray's menu"
for _ in $(seq 1 60); do grep "upload confirmed" "$L"/reel-desktop.log* | grep -q "Replay fake 1" && break; sleep 0.5; done
grep "upload confirmed" "$L"/reel-desktop.log* | grep -q "Replay fake 1" || fail "the held upload did not go on once uploads were resumed"
ok "uploads resumed: the held replay went up too"
! grep -q "panicked" "$W/app.out" || fail "a thread panicked"
