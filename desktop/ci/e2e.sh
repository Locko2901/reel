#!/usr/bin/env bash
# The end to end run of the Linux build under xvfb.sh against a reel demo: uploads, hotkeys,
# Save and share with a fake OBS, This app's pages, standalone and the server out of reach.
set -euo pipefail
SERVER="${REEL_E2E_SERVER:-http://127.0.0.1:8746}"
USER_NAME="${REEL_E2E_USER:-sam}"
PASSWORD="${REEL_E2E_PASSWORD:?the demo admin password}"
APP="${REEL_E2E_APP:-/target/debug/reel-desktop}"
FAKE_OBS="${REEL_E2E_FAKE_OBS:-/target/debug/examples/fake_obs}"
OBS_PORT="${REEL_E2E_OBS_PORT:-4466}"
SHOTS="${REEL_E2E_SHOTS:-/work/shots/desktop-e2e}"
ROOT="${REEL_E2E_DIR:-/tmp/reel-e2e}"
rm -rf "$ROOT"
mkdir -p "$ROOT/config" "$ROOT/data" "$ROOT/clips" "$SHOTS"
JAR="$ROOT/cookies"
LOG="$ROOT/data/logs"
say() { printf '\n== %s\n' "$*"; }
fail() { printf '\nFAILED: %s\n' "$*"; for l in "$LOG" "${LOG2:-}" "${LOG3:-}"; do [ -n "$l" ] && cat "$l"/reel-desktop.log* 2>/dev/null | tail -30; done; exit 1; }
wait_for() { # wait_for <seconds> <description> <command...>
  local secs=$1 what=$2; shift 2
  for _ in $(seq 1 "$secs"); do
    if "$@" >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  fail "timed out waiting for $what"
}
ledger_has() { grep -qE "\"state\": \"($1)\"" "$ROOT/data/ledger.json" 2>/dev/null; }
log_has() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -q -- "$1"; }
log_n() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
wait_more() { # wait_more <seconds> <description> <log pattern> <count before>
  local secs=$1 what=$2 pat=$3 before=$4
  for _ in $(seq 1 "$secs"); do
    if [ "$(log_n "$pat")" -gt "$before" ]; then return 0; fi
    sleep 1
  done
  fail "timed out waiting for $what"
}

if [ "${APP%.AppImage}" != "$APP" ]; then
  say "the AppImage $(basename "$APP"), opened as a desktop without FUSE runs it"
  mkdir -p "$ROOT/appimage"
  (cd "$ROOT/appimage" && "$APP" --appimage-extract >/dev/null)
  export APPIMAGE="$APP"
  APP="$ROOT/appimage/squashfs-root/AppRun"
  [ -x "$APP" ] || fail "no AppRun in the AppImage"
fi

say "sign in to $SERVER as $USER_NAME and make a pairing code"
curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
[ -n "$CODE" ] || fail "no pairing code from the server"
echo "code ${CODE:0:2}******"

say "pair as the app would, and keep the token the way the Linux build keeps it"
PAIRED=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"xvfb\"}" "$SERVER/api/device/pair")
TOKEN=$(echo "$PAIRED" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no token: $PAIRED"
printf '%s' "$TOKEN" > "$ROOT/config/token"
chmod 600 "$ROOT/config/token"
say "a fake OBS on $OBS_PORT with a 12 s replay to hand out"
ffmpeg -loglevel error -y -f lavfi -i "testsrc2=size=1280x720:rate=30" -f lavfi -i "sine=frequency=330" \
  -t 12 -c:v libx264 -preset veryfast -g 30 -pix_fmt yuv420p -c:a aac -shortest -metadata comment="e2e $(date +%s%N)" \
  -movflags +faststart "$ROOT/fake-replay.mp4"
"$FAKE_OBS" --port "$OBS_PORT" --dir "$ROOT/clips" --clip "$ROOT/fake-replay.mp4" >"$ROOT/fake-obs.out" 2>&1 &
OBS_PID=$!
cat > "$ROOT/config/settings.json" <<JSON
{
  "server_url": "$SERVER",
  "device_name": "xvfb",
  "obs": { "enabled": true, "host": "127.0.0.1", "port": $OBS_PORT, "password": "", "start_replay": true },
  "clips_dir": "$ROOT/clips",
  "stable_seconds": 2,
  "upload": { "keep_last": 5 },
  "cache": { "gb": 0, "days": 14 },
  "sounds": { "link_copied": { "enabled": true, "volume": 0.5 } },
  "startup": { "autostart": false, "elevated": false },
  "log_level": "debug"
}
JSON

say "start the app hidden"
export REEL_DESKTOP_DIR="$ROOT"
"$APP" --hidden >"$ROOT/app.out" 2>&1 &
APP_PID=$!
trap 'kill $APP_PID $OBS_PID 2>/dev/null || true' EXIT
wait_for 30 "the app to start" log_has "reel desktop starts"
wait_for 30 "the watcher" log_has "watching"
sleep 2
log_has "paired\|whoami\|device" || true

say "drop a synthetic clip into the watched folder"
ffmpeg -loglevel error -y -f lavfi -i "testsrc2=size=1280x720:rate=30" -f lavfi -i "sine=frequency=440" \
  -t 3 -c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a aac -shortest -f matroska "$ROOT/clips/Replay_2026-10-08_23-00-00.mkv.part"
mv "$ROOT/clips/Replay_2026-10-08_23-00-00.mkv.part" "$ROOT/clips/Replay_2026-10-08_23-00-00.mkv"
wait_for 20 "the clip to be found" log_has "clip found"
wait_for 10 "the captured cue" log_has "cue captured played"
wait_for 180 "the upload to be confirmed" ledger_has "confirmed|deleted"
wait_for 10 "the uploaded cue" log_has "cue uploaded played"
CLIP_ID=$(sed -n 's/.*"clip_id": "\([^"]*\)".*/\1/p' "$ROOT/data/ledger.json" | head -1)
echo "clip $CLIP_ID confirmed (the server has the whole file)"
server_ready() { curl -fsS -H "Authorization: Bearer $TOKEN" "$SERVER/api/device/clips/$CLIP_ID" | grep -q '"ready":true'; }
wait_for 120 "the server to publish the clip" server_ready
wait_for 10 "the local copy to go (cache 0 GB)" ledger_has deleted
[ ! -e "$ROOT/clips/Replay_2026-10-08_23-00-00.mkv" ] || fail "the local copy is still there"

say "press the hotkey"
xdotool key --clearmodifiers ctrl+alt+s
wait_for 120 "the link" log_has "link copied"
wait_for 10 "the link cue" log_has "cue link_copied played"
URL=$(sed -n 's/.*"share_url": "\([^"]*\)".*/\1/p' "$ROOT/data/ledger.json" | head -1)
[ -n "$URL" ] || fail "no share url in the ledger"
curl -fsS -o /dev/null -w '%{http_code}\n' "$URL" | grep -q 200 || fail "the share page does not answer 200: $URL"
echo "link $URL"

say "a desktop's own shortcut: the command line reaches the running app"
n=$(log_n "link copied")
"$APP" --share-last >/dev/null 2>&1
wait_for 10 "the second start to hand over --share-last" log_has "an action from outside the app"
wait_more 60 "the link from the command line" "link copied" "$n"

say "save and share: OBS saves the replay, the app uploads it, the server keeps its last 5 s, the link is copied"
wait_for 20 "OBS to be connected" log_has "OBS connected"
n=$(log_n "link copied")
xdotool key --clearmodifiers ctrl+alt+shift+s
wait_for 20 "OBS to save the replay" log_has "save and share: the replay is saved"
grep -q "fake obs: SaveReplayBuffer" "$ROOT/fake-obs.out" || fail "the fake OBS was not asked to save"
wait_more 240 "the link of the saved replay" "link copied" "$n"
SAVED=$(python3 -c '
import json, sys
recs = json.load(open(sys.argv[1]))
recs = recs.get("clips", recs) if isinstance(recs, dict) else recs
for r in recs:
    if r.get("name", "").startswith("Replay fake") and r.get("clip_id") and r.get("share_url"):
        print(r["clip_id"], r["share_url"])
        break
' "$ROOT/data/ledger.json")
SAVED_ID=${SAVED%% *}
SAVED_URL=${SAVED#* }
[ -n "$SAVED_ID" ] && [ "$SAVED_ID" != "$SAVED" ] || fail "no clip and link for the saved replay in the ledger"
curl -fsS -o /dev/null -w '%{http_code}\n' "$SAVED_URL" | grep -q 200 || fail "the saved replay's link does not answer 200: $SAVED_URL"
curl -fsS -b "$JAR" "$SERVER/api/clips/$SAVED_ID" | python3 -c '
import json, sys
d = json.load(sys.stdin)
rs = {r["id"]: r for r in d["renditions"]}
cur, orig = rs[d["clip"]["current_id"]], rs[d["clip"]["master_id"]]
print("current", cur["kind"], cur["duration"], "original", orig["duration"])
assert cur["kind"] == "cut" and 4.9 <= cur["duration"] <= 7.6, "the link shows the last seconds"
assert d["clip"]["game"] == "Valorant", "the game from OBS game capture source, not the foreground: %r" % d["clip"]["game"]
assert abs(orig["duration"] - 12) < 0.3, "the whole replay stays the original"
' || fail "the saved replay was not trimmed to its last seconds"
echo "saved replay $SAVED_ID, link $SAVED_URL"
wait_for 90 "the app's update check against the server (a Linux build: nothing to offer)" log_has "the app is up to date"
kill $OBS_PID 2>/dev/null || true

say "a hotkey recorded: Share the last clip's field listens, Ctrl+Alt+Y is tried, saved, and shares"
settings_has() { python3 -c 'import json, sys; s = json.load(open(sys.argv[1])); sys.exit(0 if s["hotkeys"][sys.argv[2]] == sys.argv[3] else 1)' "$ROOT/config/settings.json" "$1" "$2"; }
n=$(log_n 'app page section=hotkeys')
"$APP" --section=hotkeys >/dev/null 2>&1 &
wait_more 20 "the Hotkeys page" 'app page section=hotkeys' "$n"
WIN=$(xdotool search --name '^reel$' | head -1 || true)
[ -n "$WIN" ] || fail "no window for the hotkeys"
xdotool windowmove "$WIN" 0 0 2>/dev/null || true
xdotool windowactivate --sync "$WIN" 2>/dev/null || true
sleep 1
import -display "$DISPLAY" -window "$WIN" "$SHOTS/hotkeys.png"
# Share the last clip's field, the first in the Hotkeys panel, at y 234 to 264 under its head
SHARE_FIELD="1137 249"
n=$(log_n "hotkeys off while a field listens")
xdotool mousemove $SHARE_FIELD click 1
wait_more 10 "the app's hotkeys to go off while the field listens" "hotkeys off while a field listens" "$n"
xdotool key --clearmodifiers ctrl+alt+y
wait_for 10 "the app to try Ctrl+Alt+Y" log_has 'hotkey free.*Ctrl+Alt+Y'
wait_for 10 "Ctrl+Alt+Y saved" settings_has share_last Ctrl+Alt+Y
sleep 1
n=$(log_n "link copied")
xdotool key --clearmodifiers ctrl+alt+y
wait_more 120 "the link from the recorded hotkey" "link copied" "$n"
echo "Ctrl+Alt+Y recorded, saved and shares the last clip"

say "a combination another app holds is refused, and the field keeps Ctrl+Alt+Y"
python3 desktop/ci/hold-key.py h >"$ROOT/hold-key.out" 2>&1 &
HOLD_PID=$!
wait_for 10 "the other app to hold Ctrl+Alt+H" grep -q holding "$ROOT/hold-key.out"
xdotool mousemove $SHARE_FIELD click 1
sleep 1
xdotool key --clearmodifiers ctrl+alt+h
wait_for 10 "the refusal" log_has 'hotkey refused.*Ctrl+Alt+H is held by the desktop or another app; Ctrl+Alt+Y stays'
sleep 1
settings_has share_last Ctrl+Alt+Y || fail "the held combination replaced the hotkey"
kill $HOLD_PID 2>/dev/null || true
n=$(log_n "link copied")
xdotool key --clearmodifiers ctrl+alt+y
wait_more 120 "Ctrl+Alt+Y still sharing after the refusal" "link copied" "$n"
xdotool mousemove 640 760
echo "Ctrl+Alt+H refused as held by another app; Ctrl+Alt+Y stays and works"

HOST=$(echo "$SERVER" | sed -E 's#https?://([^:/]+).*#\1#')
REEL_PAGE="library loaded at=$HOST/settings/general"
APP_PAGE="library loaded at=localhost/index.html"
SIGNIN_PAGE="library loaded at=$HOST/signin"
FRONT_PAGE="library loaded at=$HOST/$"

say "screenshots"
"$APP" --section=status >/dev/null 2>&1 &
sleep 3
WIN=$(xdotool search --name '^reel$' | head -1 || true)
if [ -n "$WIN" ]; then
  xdotool windowmove "$WIN" 0 0 2>/dev/null || true
  xdotool windowactivate --sync "$WIN" 2>/dev/null || true
  for section in status server obs clips upload hotkeys sounds startup log; do
    "$APP" "--section=$section" >/dev/null 2>&1 &
    sleep 1.5
    import -display "$DISPLAY" -window "$WIN" "$SHOTS/settings-$section.png"
  done
  n=$(log_n "$SIGNIN_PAGE")
  "$APP" >/dev/null 2>&1 &
  wait_more 20 "the sign-in page in the window" "$SIGNIN_PAGE" "$n"
  sleep 1
  say "sign in through the window, as a user does once"
  xdotool mousemove 640 328 click 1
  xdotool type --delay 30 "$USER_NAME"
  xdotool key Tab
  xdotool type --delay 30 "$PASSWORD"
  n=$(log_n "$FRONT_PAGE")
  xdotool key Return
  wait_more 20 "the library after the sign-in" "$FRONT_PAGE" "$n"
  sleep 2
  xdotool mousemove 640 760
  import -display "$DISPLAY" -window "$WIN" "$SHOTS/library.png"
else
  echo "no window found for a screenshot"
fi
xdotool mousemove 1266 14 click 3 2>/dev/null || true
sleep 1
import -display "$DISPLAY" -window root "$SHOTS/tray-menu.png"
xdotool key Escape 2>/dev/null || true

say "This app: pages of reel's shape under Settings, the same header and nav as the server's"
WIN=$(xdotool search --name '^reel$' | head -1 || true)
[ -n "$WIN" ] || fail "no window for the shape check"
xdotool mousemove 640 760
n=$(log_n "$REEL_PAGE")
"$APP" --page=/settings/general >/dev/null 2>&1 &
wait_more 20 "the server's settings page" "$REEL_PAGE" "$n"
sleep 2
import -display "$DISPLAY" -window "$WIN" "$SHOTS/shape-reel.png"
n=$(log_n "$APP_PAGE")
"$APP" --section=status >/dev/null 2>&1 &
wait_more 20 "This app" "$APP_PAGE" "$n"
sleep 2
import -display "$DISPLAY" -window "$WIN" "$SHOTS/shape-app.png"
# The nav's current entry differs by design (General there, This app here), hence the room.
same_enough() { # same_enough <crop> <name> <most differing pixels>
  local crop=$1 name=$2 most=$3 a b n
  a="$ROOT/shape-$name-reel.png"; b="$ROOT/shape-$name-app.png"
  convert "$SHOTS/shape-reel.png" -crop "$crop" +repage "$a"
  convert "$SHOTS/shape-app.png" -crop "$crop" +repage "$b"
  n=$({ compare -metric AE -fuzz 8% "$a" "$b" null: 2>&1 || true; } | awk '{printf "%d", $1}')
  echo "$name: $n pixels differ, at most $most allowed"
  [ "${n:-999999}" -le "$most" ] || fail "the $name of This app differs from reel's by $n pixels"
}
same_enough 1280x49+0+0 header 600
same_enough 230x660+0+100 nav 2000
convert "$SHOTS/shape-reel.png" "$SHOTS/shape-app.png" +append -crop 2560x560+0+0 +repage "$SHOTS/shape-side-by-side.png"

say "Back and Forward across the switch, a section of reel's from This app, This app from reel's page"
n=$(log_n "$REEL_PAGE")
xdotool key --clearmodifiers alt+Left
wait_more 15 "Back to the server's page" "$REEL_PAGE" "$n"
n=$(log_n "$APP_PAGE")
xdotool key --clearmodifiers alt+Right
wait_more 15 "Forward to This app" "$APP_PAGE" "$n"
sleep 1
# The nav's entries are 30 px rows from y=110, General first; Status follows the head This app.
n=$(log_n "$REEL_PAGE")
xdotool mousemove 60 125 click 1
wait_more 15 "General from This app" "$REEL_PAGE" "$n"
sleep 2
n=$(log_n "$APP_PAGE")
xdotool mousemove 60 468 click 1
wait_more 15 "Status under This app from reel's page" "$APP_PAGE" "$n"
log_has "handed over the shape" || fail "the server's page did not hand the shape over"
n=$(log_n "$REEL_PAGE")
xdotool key --clearmodifiers alt+Left
wait_more 15 "Back to the server's page again" "$REEL_PAGE" "$n"
xdotool mousemove 640 760
echo "the switch works both ways, and so does Back"

say "every entry of This app opens its page from the app's own pages, and Back walks them in order"
n=$(log_n 'app page section=status')
"$APP" --section=status >/dev/null 2>&1 &
wait_more 20 "the app's Status page" 'app page section=status' "$n"
sleep 1
# the group's entries, 31 px apart from Status at y=468
walk=(server obs clips upload hotkeys sounds startup log)
for i in "${!walk[@]}"; do
  sec=${walk[$i]}
  n=$(log_n "app page section=$sec")
  xdotool mousemove 60 $((499 + i * 31)) click 1
  wait_more 10 "the $sec page by its entry" "app page section=$sec" "$n"
done
back=(startup sounds hotkeys upload clips obs server status)
for sec in "${back[@]}"; do
  n=$(log_n "app page section=$sec")
  xdotool key --clearmodifiers alt+Left
  wait_more 10 "Back to the $sec page" "app page section=$sec" "$n"
done
xdotool mousemove 640 760
echo "the nine entries open from the app's pages, and Back walks them in order"

say "cues played, from the log"
cat "$LOG"/reel-desktop.log* | grep -o 'cue [a-z_]* played ([^)]*)' | sort | uniq -c

say "standalone: reel inside the app, the library on this machine"
kill $APP_PID 2>/dev/null || true
sleep 2
ROOT2="$ROOT-standalone"
rm -rf "$ROOT2"
mkdir -p "$ROOT2/config" "$ROOT2/data" "$ROOT2/clips"
LOG2="$ROOT2/data/logs"
cat > "$ROOT2/config/settings.json" <<JSON
{
  "mode": "standalone",
  "standalone": { "hwaccel": "none", "cap_gb": 0, "cap_keeps_unsent": true, "copies_on_hardware": false },
  "obs": { "enabled": false },
  "clips_dir": "$ROOT2/clips",
  "clips_follow_obs": false,
  "stable_seconds": 2,
  "startup": { "autostart": false, "elevated": false },
  "log_level": "debug"
}
JSON
log2_has() { cat "$LOG2"/reel-desktop.log* 2>/dev/null | grep -q -- "$1"; }
export REEL_DESKTOP_DIR="$ROOT2"
"$APP" --hidden >"$ROOT2/app.out" 2>&1 &
APP_PID=$!
trap 'kill $APP_PID 2>/dev/null || true' EXIT
wait_for 30 "the app to start" log2_has "reel desktop starts"
wait_for 60 "reel on the loopback" log2_has "standalone: reel serves on the loopback"
LOCAL=$(cat "$LOG2"/reel-desktop.log* | grep 'standalone: reel serves on the loopback' | grep -o 'addr=[0-9.:]*' | head -1 | cut -d= -f2)
[ -n "$LOCAL" ] || fail "no loopback address in the log"
echo "reel on $LOCAL"
curl -fsS -o /dev/null -w '%{http_code}\n' "http://$LOCAL/" | grep -q 200 || fail "the local reel does not answer"
ffmpeg -loglevel error -y -f lavfi -i "testsrc2=size=1280x720:rate=30" -f lavfi -i "sine=frequency=440" \
  -t 3 -c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a aac -shortest -f matroska "$ROOT2/clips/Replay_2026-10-09_12-00-00.mkv.part"
mv "$ROOT2/clips/Replay_2026-10-09_12-00-00.mkv.part" "$ROOT2/clips/Replay_2026-10-09_12-00-00.mkv"
wait_for 20 "the clip to be found" log2_has "clip found"
wait_for 120 "the clip in the library" log2_has "standalone: the clip is published"
wait_for 10 "the uploaded cue" log2_has "cue uploaded played"
[ ! -e "$ROOT2/clips/Replay_2026-10-09_12-00-00.mkv" ] || fail "the raw file was not taken into the library"
curl -fsS "http://$LOCAL/" | grep -q 'card-title' || fail "the local library shows no clip"
curl -fsS "http://$LOCAL/" | grep -q 'id="app-save" type="button" aria-label="Save clip" hidden' || fail "the header has no slot for the app's parts"
! curl -fsS "http://$LOCAL/" | grep -q 'id="app-share"' || fail "standalone's header has Share the last clip"
grep -q '"state": "local"' "$ROOT2/data/ledger.json" || fail "the ledger does not say local"
find "$ROOT2/data/library/library" -maxdepth 1 -mindepth 1 | head -3
say "standalone screenshot"
"$APP" >/dev/null 2>&1 &
sleep 4
WIN=$(xdotool search --name '^reel$' | head -1 || true)
if [ -n "$WIN" ]; then
  xdotool windowmove "$WIN" 0 0 2>/dev/null || true
  xdotool windowactivate --sync "$WIN" 2>/dev/null || true
  wait_for 20 "the page to reach the app" log2_has "the server's page reached the app"
  import -display "$DISPLAY" -window "$WIN" "$SHOTS/standalone-library.png"
  "$APP" --section=status >/dev/null 2>&1 &
  sleep 2
  import -display "$DISPLAY" -window "$WIN" "$SHOTS/standalone-status.png"
fi

say "the server out of reach: the app's own page, the settings, standalone, the way back"
kill $APP_PID 2>/dev/null || true
sleep 2
offline_case() { # offline_case <name> <server url> <expected code fragment>
  local name=$1 url=$2 expect=$3
  ROOT3="$ROOT-offline-$name"
  rm -rf "$ROOT3"
  mkdir -p "$ROOT3/config" "$ROOT3/data" "$ROOT3/clips"
  LOG3="$ROOT3/data/logs"
  cat > "$ROOT3/config/settings.json" <<JSON
{ "server_url": "$url", "obs": { "enabled": false }, "clips_dir": "$ROOT3/clips", "clips_follow_obs": false,
  "startup": { "autostart": false, "elevated": false }, "log_level": "debug" }
JSON
  log3_has() { cat "$LOG3"/reel-desktop.log* 2>/dev/null | grep -q -- "$1"; }
  export REEL_DESKTOP_DIR="$ROOT3"
  "$APP" >"$ROOT3/app.out" 2>&1 &
  APP_PID=$!
  trap 'kill $APP_PID 2>/dev/null || true' EXIT
  wait_for 30 "the app to start" log3_has "reel desktop starts"
  wait_for 40 "the fallback ($name)" log3_has "cannot reach the server"
  log3_has "$expect" || { cat "$LOG3"/reel-desktop.log* | grep 'cannot reach' | tail -2; fail "the code does not say $expect"; }
  wait_for 10 "the fallback page shown" log3_has 'window shows view="offline"'
  echo "fallback shown for $name"
}
offline_case nothing "http://127.0.0.1:1" "refused"
WIN=$(xdotool search --name '^reel$' | head -1 || true)
if [ -n "$WIN" ]; then
  xdotool windowmove "$WIN" 0 0 2>/dev/null || true
  xdotool windowactivate --sync "$WIN" 2>/dev/null || true
  sleep 1
  import -display "$DISPLAY" -window "$WIN" "$SHOTS/offline.png"
  say "the settings by the shortcut and from a second start, then standalone"
  xdotool key --clearmodifiers ctrl+comma
  sleep 1
  xdotool key --clearmodifiers ctrl+comma
  wait_for 10 "the shortcut" log3_has "settings opened by the shortcut"
  "$APP" --section=server >/dev/null 2>&1 &
  wait_for 10 "the settings view" log3_has 'window shows view="settings"'
  "$APP" --mode=standalone >/dev/null 2>&1 &
  wait_for 60 "standalone from the fallback" log3_has "standalone: reel serves on the loopback"
  "$APP" >/dev/null 2>&1 &
  wait_for 30 "the local library" log3_has "the server's page reached the app"
  echo "standalone reached from the fallback"
else
  fail "no window for the offline case"
fi
kill $APP_PID 2>/dev/null || true
sleep 2

say "a certificate for another name"
TLSDIR="$ROOT-tls"; rm -rf "$TLSDIR"; mkdir -p "$TLSDIR"
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$TLSDIR/key.pem" -out "$TLSDIR/cert.pem" -days 2 -subj "/CN=other.example" >/dev/null 2>&1
TLSPORT=18443
openssl s_server -accept "$TLSPORT" -cert "$TLSDIR/cert.pem" -key "$TLSDIR/key.pem" -www >/dev/null 2>&1 &
TLS_PID=$!
offline_case tls "https://127.0.0.1:$TLSPORT" "ertificate"
kill $APP_PID 2>/dev/null || true; kill $TLS_PID 2>/dev/null || true
sleep 2

say "a server answering 502, then the real one in its place"
BADPORT=18502
python3 - "$BADPORT" >/dev/null 2>&1 <<'PY' &
import sys, http.server
class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"
    def do_GET(self):
        self.send_response(502); self.send_header("content-type", "text/plain"); self.send_header("connection", "close"); self.end_headers(); self.wfile.write(b"bad gateway")
    def log_message(self, *a): pass
http.server.ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
BAD_PID=$!
sleep 1
offline_case bad "http://127.0.0.1:$BADPORT" "HTTP 502"
kill $BAD_PID 2>/dev/null || true
sleep 1
python3 - "$BADPORT" "$SERVER" >/dev/null 2>&1 <<'PY' &
import sys, asyncio
from urllib.parse import urlparse
port = int(sys.argv[1]); up = urlparse(sys.argv[2])
async def pipe(r, w):
    try:
        while True:
            b = await r.read(65536)
            if not b: break
            w.write(b); await w.drain()
    except Exception: pass
    finally:
        try: w.close()
        except Exception: pass
async def handle(r, w):
    try:
        ur, uw = await asyncio.open_connection(up.hostname, up.port or 80)
    except Exception:
        w.close(); return
    await asyncio.gather(pipe(r, uw), pipe(ur, w))
async def main():
    s = await asyncio.start_server(handle, "127.0.0.1", port)
    async with s: await s.serve_forever()
asyncio.run(main())
PY
FWD_PID=$!
wait_for 90 "the server back and the library with it" log3_has "the server is back; the library returns"
echo "the library came back by itself"
kill $FWD_PID 2>/dev/null || true
say "done"
