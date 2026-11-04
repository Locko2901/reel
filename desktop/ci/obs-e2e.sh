#!/usr/bin/env bash
# The app with the real OBS under xvfb.sh, beside the user's own OBS, whose settings stay the
# same. With REEL_E2E_PASSWORD the clip goes to REEL_E2E_SERVER; without, it is checked here.
set -euo pipefail
SERVER="${REEL_E2E_SERVER:-http://127.0.0.1:8746}"
USER_NAME="${REEL_E2E_USER:-sam}"
PASSWORD="${REEL_E2E_PASSWORD:-}"
APP="${REEL_E2E_APP:-/target/debug/reel-desktop}"
SHOTS="${REEL_E2E_SHOTS:-}"
ROOT="${REEL_E2E_DIR:-/tmp/reel-obs-e2e}"
HERE=$(cd "$(dirname "$0")" && pwd)
rm -rf "$ROOT"
mkdir -p "$ROOT/config" "$ROOT/data" "$ROOT/clips"
[ -n "$SHOTS" ] && mkdir -p "$SHOTS"
LOG="$ROOT/data/logs"
OBSCONF="$ROOT/data/obs/obs-studio"
say() { printf '\n== %s\n' "$*"; }
ok() { printf 'ok   %s\n' "$*"; }
fail() { printf '\nFAILED: %s\n' "$*"; cat "$LOG"/reel-desktop.log* 2>/dev/null | tail -40; grep -i -A5 panicked "$ROOT/app.out" 2>/dev/null | head -20; ls -t "$OBSCONF"/logs/*.txt 2>/dev/null | head -1 | xargs -r -d '\n' tail -n 30; exit 1; }
wait_for() { # wait_for <seconds> <description> <command...>
  local secs=$1 what=$2; shift 2
  for _ in $(seq 1 "$secs"); do
    if "$@" >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  fail "timed out waiting for $what"
}
log_has() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -q -- "$1"; }
log_n() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
ms_after() { python3 "$HERE/ms-after.py" "$1" "$2" "$LOG"; }
# reel's OBS is the one whose XDG_CONFIG_HOME is reel's; the user's own runs beside it.
obs_pid() { local p; for p in $(pgrep -x reel-capture; pgrep -x obs); do tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | grep -qx "XDG_CONFIG_HOME=$ROOT/data/obs" && { echo "$p"; return; }; done; true; }
no_reel_obs() { [ -z "$(obs_pid)" ]; }
reel_obs_not() { local p; p=$(obs_pid); [ -n "$p" ] && [ "$p" != "$1" ]; }
MAIN_CONF="$HOME/.config/obs-studio"
main_sum() { (cd "$MAIN_CONF" && find . -type f ! -path './logs/*' ! -path './profiler_data/*' ! -path './crashes/*' ! -path './.sentinel/*' -print0 | sort -z | xargs -0 sha256sum); }
main_alive() { kill -0 "$MAIN_PID" 2>/dev/null; }
ROOT_WINDOW=$(xwininfo -root -int | sed -n 's/.*Window id: \([0-9]*\).*/\1/p')
# OBS's windows on the screen but its tray icon, a window of a few pixels.
visible_obs_windows() {
  local p w n=0 g
  p=$(obs_pid); [ -n "$p" ] || { echo 0; return; }
  for w in $({ xdotool search --all --onlyvisible --pid "$p" --name "" 2>/dev/null || true; } | grep -vx "$ROOT_WINDOW"); do
    g=$(xdotool getwindowgeometry "$w" 2>/dev/null | sed -n 's/.*Geometry: \([0-9]*\)x\([0-9]*\).*/\1 \2/p')
    set -- $g
    [ "${1:-0}" -le 64 ] && [ "${2:-0}" -le 64 ] && continue
    n=$((n + 1))
  done
  echo "$n"
}
# The cursor is in the capture: moved somewhere of this run's own, or a still screen would give
# a replay the bytes of an earlier run's, which the server takes for that clip.
own_frames() { xdotool mousemove $((RANDOM % 1200 + 20)) $((RANDOM % 700 + 20)); sleep 2; }
shot() { [ -n "$SHOTS" ] && import -display "$DISPLAY" -window root "$SHOTS/$1.png" || true; }
obs_window_names() { local p; p=$(obs_pid); for w in $(xdotool search --all --onlyvisible --pid "$p" --name "" 2>/dev/null | grep -vx "$ROOT_WINDOW"); do printf '  window %s "%s" %s\n' "$w" "$(xdotool getwindowname "$w" 2>/dev/null)" "$(xdotool getwindowgeometry "$w" 2>/dev/null | sed -n 's/.*Geometry: //p')"; done; }

TOKEN=""
if [ -n "$PASSWORD" ]; then
  say "pair with $SERVER as $USER_NAME"
  JAR="$ROOT/cookies"
  curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
  curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
    --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
  CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
  [ -n "$CODE" ] || fail "no pairing code from the server"
  PAIRED=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"xvfb obs\"}" "$SERVER/api/device/pair")
  TOKEN=$(echo "$PAIRED" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
  [ ${#TOKEN} -eq 43 ] || fail "no token: $PAIRED"
  printf '%s' "$TOKEN" > "$ROOT/config/token"
  chmod 600 "$ROOT/config/token"
  SERVER_URL="$SERVER"
else
  SERVER_URL=""
fi

say "the app's settings: the OBS that came with it, a 20 s clip, the Save hotkey"
cat > "$ROOT/config/settings.json" <<JSON
{
  "server_url": "$SERVER_URL",
  "device_name": "xvfb obs",
  "obs": { "bundled": true, "start_replay": true,
           "setup": { "done": true, "clip_seconds": 20, "quality": "smooth", "encoder": "obs_x264", "game": false,
                      "screen": { "width": 1280, "height": 800, "hz": 30 }, "format": "mkv",
                      "mic": "micsink.monitor", "noise": "rnnoise", "gate": true, "own_tracks": true } },
  "clips_dir": "$ROOT/clips",
  "stable_seconds": 2,
  "cache": { "gb": 5, "days": 14 },
  "hotkeys": { "share_last": "", "save_share": "", "upload_last": "", "save": "Alt+F10" },
  "startup": { "autostart": false, "elevated": false },
  "log_level": "debug"
}
JSON

# The microphone a silent source of its own: the null sink's monitor, which desktop audio
# records, would bring every sound into the full mix twice.
pactl load-module module-null-sink sink_name=micsink >/dev/null
pactl set-default-sink null

say "the user's own OBS, with its settings where OBS keeps them"
rm -rf "$MAIN_CONF"
mkdir -p "$MAIN_CONF"
printf '[General]\nEnableAutoUpdates=false\nInfoIncrement=-1\n' > "$MAIN_CONF/global.ini"
printf '[General]\nFirstRun=true\nConfirmOnExit=false\n' > "$MAIN_CONF/user.ini"
env -u XDG_CONFIG_HOME obs --minimize-to-tray --disable-updater >"$ROOT/main-obs.out" 2>&1 &
MAIN_PID=$!
wait_for 60 "the user's OBS to write its profile" sh -c "ls $MAIN_CONF/basic/profiles/*/basic.ini"
sleep 8
MAIN_BEFORE=$(main_sum)
ok "the user's OBS runs, pid $MAIN_PID, its settings in $MAIN_CONF ($(echo "$MAIN_BEFORE" | wc -l) files)"

say "start the app hidden: it finds the OBS installed here"
export REEL_DESKTOP_DIR="$ROOT"
"$APP" --hidden >"$ROOT/app.out" 2>&1 &
APP_PID=$!
trap 'kill -9 $APP_PID 2>/dev/null || true; pkill -9 -x obs 2>/dev/null || true; pkill -9 -x reel-capture 2>/dev/null || true' EXIT
wait_for 30 "the app to start" log_has "reel desktop starts"
log_has "reel's own OBS is the one installed here" || fail "the app did not find the OBS installed here"
wait_for 30 "the app to start OBS" log_has "the bundled OBS started"
wait_for 10 "reel's OBS, run as reel-capture" sh -c "pgrep -x reel-capture"
FIRST=$(obs_pid)
[ -n "$FIRST" ] || fail "no OBS process of reel's"
main_alive || fail "the user's OBS stopped"
! log_has "your own OBS is running" || fail "reel's OBS waited for the user's"
ok "two OBS side by side: the user's, pid $MAIN_PID, and reel's, pid $FIRST, with --multi and its own settings ($(log_has 'the OBS installed on this PC (/usr/local/bin/obs)' && echo '/usr/local/bin/obs' || echo 'which?'))"

say "what the app wrote for OBS"
[ -f "$OBSCONF/basic/profiles/reel/basic.ini" ] || fail "no profile"
grep -q '^RecRB=true$' "$OBSCONF/basic/profiles/reel/basic.ini" || fail "the profile has no replay buffer"
grep -q '^RecRBTime=20$' "$OBSCONF/basic/profiles/reel/basic.ini" || fail "the replay buffer is not 20 s"
grep -q "^RecFilePath=$ROOT/clips$" "$OBSCONF/basic/profiles/reel/basic.ini" || fail "OBS records elsewhere than the clips folder"
python3 -c 'import json,sys; c=json.load(open(sys.argv[1])); ids={s["id"] for s in c["sources"]} | {c.get("DesktopAudioDevice1", {}).get("id")}; assert {"scene","xshm_input","pulse_output_capture"} <= ids, ids' "$OBSCONF/basic/scenes/reel.json" || fail "the scene collection"
python3 -c 'import json,sys; w=json.load(open(sys.argv[1])); assert w["server_port"]==4466 and w["auth_required"] and len(w["server_password"])==43' "$OBSCONF/plugin_config/obs-websocket/config.json" || fail "the WebSocket settings"
ok "the profile (a 20 s replay buffer into the clips folder), the scene (the screen), the WebSocket (port 4466, a 43-character password)"

say "OBS connects, hidden"
wait_for 60 "the app to connect to OBS" log_has "obs connected"
ok "connected: $(cat "$LOG"/reel-desktop.log* | grep -o 'obs_version=[^ ]*' | tail -1)"
PW=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["obs"]["setup"]["password"])' "$ROOT/config/settings.json")
sh "$HERE/check-obs-requests.sh" 4466 "$PW" || fail "the protocol"
wait_for 20 "the app to read the microphone's filters" log_has "the microphone's filters in OBS"
FILTERS=$(cat "$LOG"/reel-desktop.log* | grep "the microphone's filters in OBS" | tail -1)
echo "$FILTERS" | grep -q "Noise suppression (noise_suppress_filter_v2)" && echo "$FILTERS" | grep -q "Noise gate (noise_gate_filter)" || fail "the microphone's filters: $FILTERS"
ok "the microphone in OBS has the noise suppression (RNNoise) and the gate, as OBS reports them"
sleep 4
[ "$(visible_obs_windows)" = "0" ] || { shot obs-visible; obs_window_names; fail "OBS shows $(visible_obs_windows) window(s)"; }
ok "no OBS window shows (no main window, no wizard, no dialog)"
shot obs-hidden
grep -q -i "replay buffer" "$OBSCONF"/logs/*.txt 2>/dev/null && ok "OBS's own log: $(grep -h -i -m1 'replay buffer' "$OBSCONF"/logs/*.txt | sed 's/^[0-9:. ]*//')" || true

say "the replay buffer fills, the Save hotkey saves a clip"
ffmpeg -nostdin -loglevel error -re -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=40" -af volume=-12dB -f pulse -device null "a tone" &
TONE=$!
sleep 22
n=$(log_n "obs saved a replay")
own_frames
cues=$(log_n "cue captured played")
KEY=$(date +%s.%N)
xdotool key --clearmodifiers alt+F10
wait_for 30 "OBS to save the replay" log_has "obs saved a replay"
wait_for 10 "the save's sound" ms_after "$KEY" "cue captured played"
SOUND=$(ms_after "$KEY" "cue captured played")
WRITTEN=$(ms_after "$KEY" "obs saved a replay")
sleep 4
[ "$(log_n "cue captured played")" -eq $((cues + 1)) ] || fail "the save's sound played $(( $(log_n "cue captured played") - cues )) times"
[ "$SOUND" -lt 1000 ] && [ "$SOUND" -lt "$WRITTEN" ] || fail "the save's sound came $SOUND ms after the key, OBS's replay at $WRITTEN ms"
ok "the key to the save's sound: $SOUND ms, once, as OBS took the save; OBS had written the replay at $WRITTEN ms"
SAVED=$(cat "$LOG"/reel-desktop.log* | grep "obs saved a replay" | tail -1 | sed -n 's/.*path=\(.*\) obs saved a replay$/\1/p; s/.*obs saved a replay path=\(.*\)$/\1/p')
[ -n "$SAVED" ] || fail "no path in the log"
ok "saved: $SAVED"
TITLES=$(ffprobe -v error -select_streams a -show_entries stream_tags=title -of csv=p=0 "$SAVED" | paste -sd '|')
[ "$TITLES" = "Full mix|Microphone|Desktop" ] || fail "the replay's audio tracks: '$TITLES'"
ok "the replay carries each source on a track of its own, named: $TITLES"
kill "$TONE" 2>/dev/null || true
level() { ffmpeg -nostdin -i "$SAVED" -map "0:a:$1" -af volumedetect -f null - 2>&1 | sed -n 's/.*mean_volume: \([^ ]*\) dB.*/\1/p'; }
FULL_DB=$(level 0); MIC_DB=$(level 1); DESK_DB=$(level 2)
python3 -c 'import sys; f, m, d = map(float, sys.argv[1:]); assert -40 < d < -6 and abs(f - d) < 1.0 and m < -60, (f, m, d)' "$FULL_DB" "$MIC_DB" "$DESK_DB" || fail "a tone into desktop audio: full mix $FULL_DB dB, microphone $MIC_DB dB, desktop $DESK_DB dB"
ok "a tone played into desktop audio is in the full mix once, at its own level: full mix $FULL_DB dB, desktop $DESK_DB dB (doubled it would be 6 dB more), the microphone silent ($MIC_DB dB)"
if [ -n "$TOKEN" ]; then
  wait_for 180 "the upload to be confirmed" grep -qE '"state": "(confirmed|deleted)"' "$ROOT/data/ledger.json"
  CLIP_ID=$(sed -n 's/.*"clip_id": "\([^"]*\)".*/\1/p' "$ROOT/data/ledger.json" | head -1)
  ok "uploaded to $SERVER as clip $CLIP_ID"
  ready() { curl -fsS -H "Authorization: Bearer $TOKEN" "$SERVER/api/device/clips/$CLIP_ID" | grep -q '"ready":true'; }
  wait_for 120 "the server to publish the clip" ready
  ok "the server published it"
  curl -fsS -b "$JAR" "$SERVER/clips/$CLIP_ID" | grep -q '"tracks":\["Full mix","Microphone","Desktop"\]' || fail "the clip's tracks on the server"
  ok "the server keeps them by name: the editor's rows Full mix, Microphone, Desktop"
else
  wait_for 20 "the app to find the clip" log_has "clip found"
  python3 - "$ROOT/clips" <<'PY' || fail "the saved clip is not a 15 to 25 s video"
import json, subprocess, sys, glob, os
f = sorted(glob.glob(os.path.join(sys.argv[1], "Replay*")))[-1]
d = float(json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "json", f]))["format"]["duration"])
print("ok   the clip is %s, %.1f s" % (os.path.basename(f), d))
assert 15 <= d <= 25
PY
fi

say "OBS killed: started again, connected again, no safe mode question"
c=$(log_n "obs connected")
kill -9 "$FIRST"
wait_for 30 "OBS to be started again" reel_obs_not "$FIRST"
SECOND=$(obs_pid)
ok "started again, pid $SECOND"
wait_for 60 "the app to connect again" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'obs connected') -gt $c ]"
sleep 4
[ "$(visible_obs_windows)" = "0" ] || { shot obs-after-crash; fail "OBS shows a window after the crash (the safe mode question?)"; }
ok "connected again, nothing shows"

say "the app quits: OBS stops"
kill -TERM "$APP_PID"
wait_for 20 "the app to stop OBS" log_has "stopping the bundled OBS"
wait_for 20 "OBS to exit" no_reel_obs
wait_for 20 "the app to exit" sh -c "! kill -0 $APP_PID"
log_has "the bundled OBS exited" || fail "the app did not see OBS exit"
ok "OBS exited with the app"
trap - EXIT

say "an MP4 drops the tracks' names: the app gives them with the upload"
n=$(log_n "obs connected")
"$APP" --hidden >>"$ROOT/app.out" 2>&1 &
APP_PID=$!
trap 'kill -9 $APP_PID 2>/dev/null || true; pkill -9 -x obs 2>/dev/null || true; pkill -9 -x reel-capture 2>/dev/null || true' EXIT
wait_for 60 "the app to connect to OBS again" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'obs connected') -gt $n ]"
n=$(log_n "the bundled OBS started")
"$APP" --format=hybrid_mp4 >>"$ROOT/app.out" 2>&1 || true
wait_for 60 "OBS to start again with the format changed in the app" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'the bundled OBS started') -gt $n ]"
grep -q '^RecFormat2=hybrid_mp4$' "$OBSCONF/basic/profiles/reel/basic.ini" || fail "the format changed in the app was not written into OBS"
wait_for 60 "the app to connect to the OBS started again" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep 'obs connected' | tail -1 | cut -c1-19 | tr -d 'T:-') -gt \$(cat $LOG/reel-desktop.log* | grep 'the bundled OBS started' | tail -1 | cut -c1-19 | tr -d 'T:-') ]"
sleep 22
n=$(log_n "obs saved a replay")
own_frames
xdotool key --clearmodifiers alt+F10
wait_for 30 "OBS to save the MP4 replay" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'obs saved a replay') -gt $n ]"
MP4=$(cat "$LOG"/reel-desktop.log* | grep "obs saved a replay" | tail -1 | sed -n 's/.*path=\(.*\) obs saved a replay$/\1/p; s/.*obs saved a replay path=\(.*\)$/\1/p')
case "$MP4" in *.mp4) ;; *) fail "the replay is not an MP4: $MP4" ;; esac
STREAMS=$(ffprobe -v error -select_streams a -show_entries stream=index -of csv=p=0 "$MP4" | wc -l)
TITLES=$(ffprobe -v error -select_streams a -show_entries stream_tags=title -of csv=p=0 "$MP4" | tr -d '\n')
[ "$STREAMS" = 3 ] && [ -z "$TITLES" ] || fail "the MP4's audio: $STREAMS streams, titles '$TITLES'"
ok "the MP4 carries the three tracks, without their names (OBS's muxer writes them as titles, which MP4 drops)"
if [ -n "$TOKEN" ]; then
  wait_for 180 "the MP4's upload to be confirmed" sh -c "cat $LOG/reel-desktop.log* | grep 'upload confirmed' | grep -qF '$MP4'"
  MP4_CLIP=$(cat "$LOG"/reel-desktop.log* | grep 'upload confirmed' | grep -F "$MP4" | tail -1 | sed -n 's/.* clip=\([0-9A-Z]*\).*/\1/p')
  [ -n "$MP4_CLIP" ] || fail "no clip id for the MP4"
  ready_mp4() { curl -fsS -H "Authorization: Bearer $TOKEN" "$SERVER/api/device/clips/$MP4_CLIP" | grep -q '"ready":true'; }
  wait_for 120 "the server to publish the MP4" ready_mp4
  curl -fsS -b "$JAR" "$SERVER/clips/$MP4_CLIP" | grep -q '"tracks":\["Full mix","Microphone","Desktop"\]' || fail "the MP4's tracks on the server are not named"
  ok "the server names them as the app said: Full mix, Microphone, Desktop"
fi
kill -TERM "$APP_PID"
wait_for 20 "OBS to exit" no_reel_obs
wait_for 20 "the app to exit" sh -c "! kill -0 $APP_PID"
trap - EXIT

say "changed in OBS: shown in the app when OBS is hidden again, and kept over the next start"
PW=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["obs"]["setup"]["password"])' "$ROOT/config/settings.json")
ws() { python3 "$HERE/obs-ws.py" 4466 "$PW" "$@"; }
n=$(log_n "obs connected")
"$APP" --hidden >>"$ROOT/app.out" 2>&1 &
APP_PID=$!
trap 'kill -9 $APP_PID 2>/dev/null || true; pkill -9 -x obs 2>/dev/null || true; pkill -9 -x reel-capture 2>/dev/null || true' EXIT
wait_for 60 "the app to connect to OBS again" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'obs connected') -gt $n ]"
n=$(log_n "read from OBS")
ws SetProfileParameter '{"parameterCategory":"AdvOut","parameterName":"RecRBTime","parameterValue":"45"}' \
   SetProfileParameter '{"parameterCategory":"Video","parameterName":"FPSType","parameterValue":"2"}' \
   SetProfileParameter '{"parameterCategory":"Video","parameterName":"FPSNum","parameterValue":"30000"}' \
   SetProfileParameter '{"parameterCategory":"Video","parameterName":"FPSDen","parameterValue":"1001"}' \
   SetInputVolume '{"inputName":"Desktop audio","inputVolumeDb":-6}' \
   SetInputSettings '{"inputName":"Screen","inputSettings":{"show_cursor":false}}' >/dev/null || fail "OBS took no change"
ok "changed in OBS, as its Settings and its Audio Mixer would: the replay time 45 s, 29.97 fps, the desktop audio at -6 dB, the screen without the cursor"
"$APP" --hide-obs >>"$ROOT/app.out" 2>&1 || true
wait_for 20 "the app to read what was changed in OBS" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'read from OBS') -gt $n ]"
python3 - "$ROOT/config/settings.json" <<'PY' || fail "the app's settings do not have what was changed in OBS"
import json, sys
s = json.load(open(sys.argv[1]))["obs"]["setup"]
assert s["clip_seconds"] == 45, s["clip_seconds"]
assert s["quality"] == "custom" and s["custom_video"]["fps"] == [30000, 1001], (s["quality"], s["custom_video"])
assert s["volumes"].get("Desktop audio") == -6.0, s["volumes"]
PY
ok "Hide OBS read them into the app's settings, which its page shows: $(cat "$LOG"/reel-desktop.log* | grep 'read from OBS' | tail -1 | sed 's/.*read=//; s/ read from OBS$//')"
ws SetInputVolume '{"inputName":"Microphone","inputVolumeDb":-3}' >/dev/null || fail "OBS took no volume"
kill -TERM "$APP_PID"
wait_for 20 "OBS to exit" no_reel_obs
wait_for 20 "the app to exit" sh -c "! kill -0 $APP_PID"
ok "the microphone at -3 dB in OBS's mixer, then the app quits; OBS saves its mixer as it exits"
n=$(log_n "obs connected")
"$APP" --hidden >>"$ROOT/app.out" 2>&1 &
APP_PID=$!
wait_for 60 "the app to connect to OBS again" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'obs connected') -gt $n ]"
PROFILE="$OBSCONF/basic/profiles/reel/basic.ini"
grep -q '^RecRBTime=45$' "$PROFILE" && grep -q '^FPSNum=30000$' "$PROFILE" && grep -q '^FPSDen=1001$' "$PROFILE" || fail "reel wrote over what was changed in OBS: $(grep -E '^(RecRBTime|FPS)' "$PROFILE" | tr '\n' ' ')"
python3 - "$OBSCONF/basic/scenes/reel.json" <<'PY' || fail "the scene collection lost what was changed in OBS"
import json, math, sys
c = json.load(open(sys.argv[1]))
src = {s["name"]: s for s in c["sources"]}
for k in ("DesktopAudioDevice1", "AuxAudioDevice1"):
    if isinstance(c.get(k), dict):
        src.setdefault(c[k]["name"], c[k])
assert src["Screen"]["settings"]["show_cursor"] is False, src["Screen"]["settings"]
assert abs(20 * math.log10(src["Desktop audio"]["volume"]) + 6) < 0.1, src["Desktop audio"]["volume"]
assert abs(20 * math.log10(src["Microphone"]["volume"]) + 3) < 0.1, src["Microphone"]["volume"]
PY
wait_for 30 "OBS to log its video at 29.97" sh -c "ls -t '$OBSCONF'/logs/*.txt | head -1 | xargs -d '\n' grep -q '30000/1001'"
python3 - "$ROOT/config/settings.json" <<'PY' || fail "the app does not show the microphone's level from OBS"
import json, sys
s = json.load(open(sys.argv[1]))["obs"]["setup"]
assert s["clip_seconds"] == 45 and s["volumes"].get("Microphone") == -3.0, (s["clip_seconds"], s["volumes"])
PY
ok "the next start: OBS has 45 s and 29.97 fps (its own log: $(ls -t "$OBSCONF"/logs/*.txt | head -1 | xargs -d '\n' grep -h -m1 '30000/1001' | sed 's/^[0-9:. ]*//; s/  */ /g')), the cursor off, both levels; the app took the microphone's level at the start"

kill -TERM "$APP_PID"
wait_for 20 "OBS to exit" no_reel_obs
wait_for 20 "the app to exit" sh -c "! kill -0 $APP_PID"
trap - EXIT

say "OBS's simple output mode set in OBS: it stands, reel turns the replay buffer on, and its clip length reaches that mode"
PROFILE="$OBSCONF/basic/profiles/reel/basic.ini"
python3 - "$PROFILE" "$ROOT/clips" <<'PY'
import sys
p, clips = sys.argv[1], sys.argv[2]
lines = open(p, encoding="utf-8-sig").read().splitlines()
def put(section, key, value):
    sec = None
    for i, line in enumerate(lines):
        if line.startswith("["):
            if sec == section:
                lines.insert(i, f"{key}={value}")
                return
            sec = line.strip()[1:-1]
        elif sec == section and line.split("=", 1)[0] == key:
            lines[i] = f"{key}={value}"
            return
    if sec != section:
        lines.extend(["", f"[{section}]"])
    lines.append(f"{key}={value}")
put("Output", "Mode", "Simple")
for k, v in (("RecQuality", "Small"), ("RecFormat2", "mkv"), ("RecRBTime", "10"), ("RecRB", "false"), ("FilePath", clips)):
    put("SimpleOutput", k, v)
open(p, "w", encoding="utf-8").write("\n".join(lines) + "\n")
PY
simple() { python3 - "$PROFILE" "$1" <<'PY'
import sys
p, key = sys.argv[1], sys.argv[2]
sec, out = None, {}
for line in open(p, encoding="utf-8-sig"):
    line = line.strip()
    if line.startswith("["):
        sec = line[1:-1]
    elif "=" in line and sec == "SimpleOutput":
        k, v = line.split("=", 1)
        out[k] = v
print(out.get(key, ""))
PY
}
n=$(log_n "obs connected")
"$APP" --hidden >>"$ROOT/app.out" 2>&1 &
APP_PID=$!
trap 'kill -9 $APP_PID 2>/dev/null || true; pkill -9 -x obs 2>/dev/null || true; pkill -9 -x reel-capture 2>/dev/null || true' EXIT
wait_for 60 "the app to connect to OBS in its simple output mode" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'obs connected') -gt $n ]"
grep -q '^Mode=Simple$' "$PROFILE" && grep -q '^RecQuality=Small$' "$PROFILE" || fail "OBS's output settings were written over"
[ "$(simple RecRB)" = true ] && [ "$(simple RecRBTime)" = 10 ] || fail "the start did more or less than the replay buffer on: RecRB=$(simple RecRB) RecRBTime=$(simple RecRBTime)"
wait_for 20 "the app to show OBS's clip length" python3 -c "import json,sys; sys.exit(json.load(open('$ROOT/config/settings.json'))['obs']['setup']['clip_seconds'] != 10)"
ok "OBS's simple output mode and its quality stand, its clip length too (10 s, which the app shows); the start only turned the replay buffer on"
newest_obs_log() { ls -t "$OBSCONF"/logs/*.txt | head -1; }
wait_for 30 "OBS's replay buffer to start with its own settings" sh -c "ls -t '$OBSCONF'/logs/*.txt | head -1 | xargs -d '\n' grep -q 'Replay Buffer Start'"
ok "OBS's own log: $(newest_obs_log | xargs -d '\n' grep -h -m1 'simple_video_recording' | sed 's/^[0-9:. ]*//')"

say "what else reel reads and asks of OBS: a recording stopped, the setup's device lists, the microphone's filters"
n=$(log_n "obs saved a file")
ws StartRecord '{}' >/dev/null || fail "OBS did not start recording"
sleep 3
ws StopRecord '{}' >/dev/null || fail "OBS did not stop recording"
wait_for 20 "the app to hear the recording stop" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'obs saved a file') -gt $n ]"
REC=$(cat "$LOG"/reel-desktop.log* | grep "obs saved a file" | tail -1 | sed -n 's/.*obs saved a file path=\(.*\)$/\1/p')
[ -f "$REC" ] && [ "$(dirname "$REC")" = "$ROOT/clips" ] || fail "the recording's file: '$REC'"
wait_for 20 "the app to take the recording as a clip" sh -c "cat $LOG/reel-desktop.log* | grep 'clip found' | grep -qF '$REC'"
ok "a recording started and stopped in OBS: the app heard it stop, from OBS's RecordStateChanged, and took $(basename "$REC") as a clip"
ws GetInputPropertiesListPropertyItems '{"inputName":"Desktop audio","propertyName":"device_id"}' | grep -q '"itemValue"' || fail "OBS lists no device for Desktop audio"
ws GetInputPropertiesListPropertyItems '{"inputName":"Microphone","propertyName":"device_id"}' >/dev/null || fail "OBS has no device list for Microphone"
ok "OBS lists the devices of Desktop audio and answers for Microphone's (none here, with no microphone), as the setup asks for them"
filters() { # filters <noise> <gate|no-gate>: the requests the app builds for them, sent to OBS
  ws GetSourceFilterList '{"sourceName":"Microphone"}' | cut -d' ' -f2- | /target/debug/examples/mic_filters "$1" "$2" > "$ROOT/filters.req"
  set --
  while IFS= read -r kind && IFS= read -r data; do set -- "$@" "$kind" "$data"; done < "$ROOT/filters.req"
  ws "$@" >/dev/null
  ws GetSourceFilterList '{"sourceName":"Microphone"}' | cut -d' ' -f2- | python3 -c 'import json, sys; print(" ".join("%s:%s:%s" % (f["filterKind"], f["filterSettings"].get("method", ""), f["filterEnabled"]) for f in json.load(sys.stdin)["filters"]))'
}
[ "$(filters speex no-gate)" = "noise_suppress_filter_v2:speex:True" ] || fail "Speex without the gate, as the app asks for it: $(filters speex no-gate)"
[ "$(filters off no-gate)" = "" ] || fail "no filter, as the app asks for it: $(filters off no-gate)"
[ "$(filters rnnoise gate)" = "noise_suppress_filter_v2:rnnoise:True noise_gate_filter::True" ] || fail "RNNoise and the gate, as the app asks for them: $(filters rnnoise gate)"
ok "the microphone's filters as the app's requests set them in OBS: Speex alone, none, then RNNoise and the gate again"

n=$(log_n "the bundled OBS started")
"$APP" --clip-length=25 >>"$ROOT/app.out" 2>&1 || true
wait_for 60 "OBS to start again with the clip length changed in the app" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'the bundled OBS started') -gt $n ]"
[ "$(simple RecRBTime)" = 25 ] && grep -q '^Mode=Simple$' "$PROFILE" && grep -q '^RecQuality=Small$' "$PROFILE" || fail "the clip length changed in the app did not reach the simple output mode alone: RecRBTime=$(simple RecRBTime)"
ok "the clip length changed in the app, 25 s, written where the simple output mode reads it; nothing else of OBS's changed"

say "an installer asks the app to quit: OBS stopped for good, then the app"
"$APP" --quit-for-install >>"$ROOT/app.out" 2>&1 || true
wait_for 20 "the app to hear the installer" log_has "an installer asks the app to quit"
wait_for 20 "OBS to exit" no_reel_obs
wait_for 20 "the app to exit" sh -c "! kill -0 $APP_PID"
sleep 3
no_reel_obs || fail "OBS was started again after the app quit for an installer"
ok "the app stopped OBS, did not start it again as after a crash, and quit"
trap - EXIT

say "the user's OBS ran beside reel's all along, its settings the same to the byte"
main_alive || fail "the user's OBS stopped"
MAIN_AFTER=$(main_sum)
[ "$MAIN_BEFORE" = "$MAIN_AFTER" ] || { diff <(echo "$MAIN_BEFORE") <(echo "$MAIN_AFTER"); fail "the user's OBS settings changed"; }
ok "the user's OBS still runs, pid $MAIN_PID; its $(echo "$MAIN_AFTER" | wc -l) settings files are what they were"
kill -TERM "$MAIN_PID"
echo
echo "the app with the real OBS: all ok"
