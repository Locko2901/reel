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
count() { cat "$L"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
cues() { count "cue $1 played"; }
log_has() { grep -qs -- "$1" "$L"/reel-desktop.log*; }
wait_log() { # wait_log <seconds> <words> <what>
  for _ in $(seq 1 $(($1 * 2))); do log_has "$2" && return 0; sleep 0.5; done
  fail "$3"
}
clip_of() {
  grep -h "upload confirmed" "$L"/reel-desktop.log* 2>/dev/null | grep -- "$1" | sed -n 's/.* clip=\([0-9A-Z]*\).*/\1/p' | head -1
}
# four replays of their own, so the demo takes each as new
for n in 1 2 3 4; do
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
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"presses\"}" "$SERVER/api/device/pair" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no token from the demo"
printf '%s' "$TOKEN" > "$W/d/config/token"
chmod 600 "$W/d/config/token"
python3 /work/desktop/ci/fake-notifications.py > "$W/notifications.txt" 2>&1 &
python3 /work/desktop/ci/fake-sni.py > "$W/sni.txt" 2>&1 &
/target/debug/examples/fake_obs --port 4496 --dir "$W/rec" --clip "$W/clip1.mp4" > "$W/fake-obs.out" 2>&1 &
printf '{"server_url":"%s","obs":{"enabled":true,"host":"127.0.0.1","port":4496,"password":"","start_replay":false},"clips_follow_obs":true,"clips_dir":"%s","startup":{"autostart":false},"log_level":"debug"}' "$SERVER" "$W/rec" \
  > "$W/d/config/settings.json"
export REEL_DESKTOP_DIR="$W/d" RUST_BACKTRACE=1
"$APP" > "$W/app.out" 2>&1 &
wait_log 60 "obs connected" "the app did not connect to the stand-in OBS"
for _ in $(seq 1 20); do grep -q "^item " "$W/sni.txt" && break; sleep 0.5; done
sleep 3

printf '%s\n' "$W/clip1.mp4" "$W/clip2.mp4" "$W/clip3.mp4" > "$W/rec/next-clip"
echo 2000 > "$W/rec/write-ms"
for i in 1 2 3; do
  xdotool key --clearmodifiers ctrl+alt+shift+s
  [ "$i" = 3 ] || sleep 1.5
done
wait_log 120 "link copied" "three presses copied no link"
for _ in $(seq 1 60); do [ -n "$(clip_of "Replay fake 1")" ] && [ -n "$(clip_of "Replay fake 2")" ] && break; sleep 0.5; done
sleep 3
C3=$(clip_of "Replay fake 3")
[ -n "$C3" ] || fail "the third replay did not go up"
[ -n "$(clip_of "Replay fake 1")" ] && [ -n "$(clip_of "Replay fake 2")" ] || fail "the first two replays did not go up"
[ "$(cues captured)" -eq 3 ] || fail "three presses sounded \"clip captured\" $(cues captured) times"
[ "$(cues uploaded)" -eq 0 ] || fail "a Save and share's clip sounded \"uploaded\" ($(cues uploaded) times)"
[ "$(cues link_copied)" -eq 1 ] || fail "three presses sounded \"link copied\" $(cues link_copied) times"
grep -h "link copied" "$L"/reel-desktop.log* | grep -q "clip=$C3" || fail "the link copied is not the third clip's: $(grep -h "link copied" "$L"/reel-desktop.log*)"
[ "$(count "a newer one was pressed")" -ge 2 ] || fail "the first two presses did not give way to the newer ones"
URL=$(python3 -c '
import json, sys
recs = json.load(open(sys.argv[1]))
recs = recs.get("clips", recs) if isinstance(recs, dict) else recs
print(next((r["share_url"] for r in recs if r.get("clip_id") == sys.argv[2] and r.get("share_url")), ""))
' "$W/d/data/ledger.json" "$C3")
[ -n "$URL" ] || fail "no link for the third clip in the ledger"
curl -fsS -o /dev/null -w '%{http_code}\n' "$URL" | grep -q 200 || fail "the third clip's link does not answer 200: $URL"
ok "three presses 1.5 s apart: three \"clip captured\", no \"uploaded\", one \"link copied\", the third clip's, whose link answers; the first two went up without links"

s=$(count "obs saved a replay"); c=$(cues captured); k=$(cues link_copied)
rm -f "$W/rec/write-ms"
echo "$W/clip4.mp4" > "$W/rec/next-clip"
sleep 1.2
xdotool key --clearmodifiers ctrl+alt+shift+s
sleep 0.3
xdotool key --clearmodifiers ctrl+alt+shift+s
wait_log 10 "a double tap, nothing saved" "a second press 0.3 s after the first was not taken as a double tap"
for _ in $(seq 1 240); do [ "$(cues link_copied)" -gt "$k" ] && break; sleep 0.5; done
sleep 2
[ "$(count "obs saved a replay")" -eq $((s + 1)) ] || fail "a double tap saved $(( $(count "obs saved a replay") - s )) replays"
[ "$(cues captured)" -eq $((c + 1)) ] || fail "a double tap sounded \"clip captured\" $(( $(cues captured) - c )) times"
[ "$(cues link_copied)" -eq $((k + 1)) ] || fail "a double tap sounded \"link copied\" $(( $(cues link_copied) - k )) times"
ok "a double tap (0.3 s apart): one save, one \"clip captured\", one \"link copied\""
! grep -q "panicked" "$W/app.out" || fail "a thread panicked"
