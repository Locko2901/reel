#!/bin/sh
# Under xvfb.sh with XVFB_SCREEN=1280x900.
set -eu
APP="${1:-/target/debug/reel-desktop}"
SHOTS="${REEL_SHOTS_OUT:-}"
W=$(mktemp -d)
FFMPEG=/work/desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu
[ -x /target/debug/examples/fake_obs ] || { echo "FAIL no stand-in OBS: make desktop-linux builds it"; exit 1; }
PIDS=""
fail() {
  echo "FAIL $*"
  grep -m1 -A1 "panicked" "$W"/app*.out 2>/dev/null || tail -n 20 "$W"/app*.out 2>/dev/null
  tail -n 20 "$L"/reel-desktop.log* 2>/dev/null
  [ -z "$SHOTS" ] || cp "$L"/reel-desktop.log* "$SHOTS"/ 2>/dev/null || true
  exit 1
}
ok() { echo "ok   $*"; }
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; pkill -f fake_obs 2>/dev/null || true; }
trap cleanup EXIT
logged() { cat "$L"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
waitlog() { # waitlog <words> <seconds> <what>
  for _ in $(seq 1 $(($2 * 2))); do [ "$(logged "$1")" -gt 0 ] && return 0; sleep 0.5; done
  fail "$3"
}
waitmore() { # waitmore <words> <seconds> <what> <count before>
  for _ in $(seq 1 $(($2 * 2))); do [ "$(logged "$1")" -gt "$4" ] && return 0; sleep 0.5; done
  fail "$3"
}
said() { grep -c -- "$1" "$W/notifications.txt" || true; }
wait_said() { # wait_said <words> <seconds> <what> <count before>
  for _ in $(seq 1 $(($2 * 2))); do [ "$(said "$1")" -gt "$4" ] && return 0; sleep 0.5; done
  fail "$3"
}
start() { "$APP" "$@" > "$W/app$(date +%s%N).out" 2>&1 & PIDS="$PIDS $!"; APP_PID=$!; }
stop() { kill "$APP_PID" 2>/dev/null || true; for _ in $(seq 1 40); do kill -0 "$APP_PID" 2>/dev/null || break; sleep 0.25; done; }
window() { xdotool search --onlyvisible --name '^reel$' 2>/dev/null | head -1; }
shot() { # shot <name>: the window, or the screen
  [ -n "$SHOTS" ] || return 0
  win=$(window)
  if [ -n "$win" ]; then import -display "$DISPLAY" -window "$win" "$SHOTS/$1.png"; else import -display "$DISPLAY" -window root "$SHOTS/$1.png"; fi
}
[ -z "$SHOTS" ] || mkdir -p "$SHOTS"
"$FFMPEG" -v error -f lavfi -i "testsrc2=duration=3:size=1280x720:rate=30" -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=3" \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest "$W/clip.mp4"
FAKE_NOTIFY_ACTIONS=1 python3 /work/desktop/ci/fake-notifications.py > "$W/notifications.txt" 2>&1 &
PIDS="$PIDS $!"

D="$W/new"
L="$D/data/logs"
export REEL_DESKTOP_DIR="$D" RUST_BACKTRACE=1
export REEL_OBS_EXE=/work/desktop/ci/standin-obs-slow.sh STANDIN_DIR="$W/rec-new" STANDIN_CLIP="$W/clip.mp4" STANDIN_CLOSE_S=0
mkdir -p "$W/rec-new"
start
waitlog "a new install, no settings file yet: standalone" 30 "a new install did not start standalone"
for _ in $(seq 1 20); do [ -f "$D/config/settings.json" ] && break; sleep 0.5; done
grep -q '"mode": "standalone"' "$D/config/settings.json" || fail "the new install's settings do not say standalone"
waitlog 'window shows view="setup"' 30 "the setup did not open"
waitlog "standalone: reel serves on the loopback" 60 "reel on this PC did not start"
ok "a new install starts standalone: its settings say so, reel on this PC runs, the setup opens"
sleep 3
shot "1-new-install-choose"
# shellcheck disable=SC2086
xdotool mousemove ${REEL_SETUP_GO:-967 305} click 1
sleep 2
shot "2-new-install-setup"
grep -q '"bundled": true' "$D/config/settings.json" && grep -q '"mode": "standalone"' "$D/config/settings.json" || fail "Set it up did not keep standalone"
# shellcheck disable=SC2086
xdotool mousemove ${REEL_SETUP_WHERE:-233 273} click 1
sleep 2
shot "3-new-install-where"
stop
unset REEL_OBS_EXE
ok "Set it up keeps standalone, the default of Where clips go (shots: the choice, the first step, Where clips go)"

D="$W/here"
L="$D/data/logs"
mkdir -p "$D/config" "$W/rec"
cat > "$D/config/settings.json" <<JSON
{
  "mode": "standalone",
  "standalone": { "hwaccel": "none", "cap_gb": 0, "cap_keeps_unsent": true, "copies_on_hardware": false },
  "obs": { "enabled": true, "host": "127.0.0.1", "port": 4498, "password": "", "start_replay": false },
  "clips_follow_obs": true,
  "clips_dir": "$W/rec",
  "stable_seconds": 2,
  "hotkeys": { "share_last": "Ctrl+Alt+S", "save_share": "Ctrl+Alt+Shift+S", "upload_last": "Ctrl+Alt+U", "save": "Alt+F10" },
  "startup": { "autostart": false, "elevated": false },
  "log_level": "debug"
}
JSON
/target/debug/examples/fake_obs --port 4498 --dir "$W/rec" --clip "$W/clip.mp4" > "$W/fake-obs.out" 2>&1 &
PIDS="$PIDS $!"
python3 /work/desktop/ci/fake-sni.py > "$W/sni.txt" 2>&1 &
SNI=$!
PIDS="$PIDS $SNI"
export REEL_DESKTOP_DIR="$D"
start --hidden
waitlog "standalone: reel serves on the loopback" 60 "reel on this PC did not start"
waitlog "obs connected" 30 "the app did not connect to the stand-in OBS"
sleep 2
[ "$(logged 'window shows')" -eq 0 ] && [ -z "$(window)" ] || fail "a start at logon showed the window ($(logged 'window shows view'))"
ok "a start at logon stays in the tray: no window"
for _ in $(seq 1 20); do grep -q "^item " "$W/sni.txt" && break; sleep 0.5; done
python3 /work/desktop/ci/fake-sni.py menu > "$W/menu.txt" || fail "no tray menu"
echo "     the tray: $(tr '\n' '|' < "$W/menu.txt")"
grep -qx "Save clip" "$W/menu.txt" && grep -qx "Open library" "$W/menu.txt" && grep -qx "Open settings" "$W/menu.txt" || fail "the tray has no Save clip, Open library or Open settings"
! grep -q "Save and share\|Share the last clip\|Copy link\|Pause uploads" "$W/menu.txt" || fail "the tray has a link's or an upload's entry"
ok "the tray: Save clip, Open library (on), no Save and share, Share the last clip, Copy link or Pause uploads"
python3 /work/desktop/ci/fake-sni.py click "Save clip" | grep -q "enabled" || fail "Save clip is not clickable"
waitlog "standalone: the clip is published" 90 "the saved clip did not go into the library"
wait_said "Clip in the library" 20 "no Clip in the library notification" 0
grep "Clip in the library" "$W/notifications.txt" | tail -1 | grep -q "\[open=Open\]" || fail "Clip in the library's buttons: $(grep 'Clip in the library' "$W/notifications.txt" | tail -1)"
! grep "Clip in the library" "$W/notifications.txt" | grep -q "copy=" || fail "Clip in the library has Copy link"
LOCAL=$(cat "$L"/reel-desktop.log* | sed -n 's/.*standalone: the clip is published.* clip=\([0-9A-Z]*\).*/\1/p' | head -1)
[ -n "$LOCAL" ] || fail "no clip id in the log"
ok "the tray's Save clip: the clip went into the library; Clip in the library has Open alone"
python3 /work/desktop/ci/fake-sni.py menu > "$W/menu2.txt"
grep -q "^Open$" "$W/menu2.txt" && ! grep -q "Copy link" "$W/menu2.txt" || fail "the tray's last clips: $(tr '\n' '|' < "$W/menu2.txt")"
ok "the tray's Last clips: the library's clip with Open alone"
for how in "--save-and-share:Save and share" "--share-last:Share the last clip" "--upload-last:Upload the last clip" "reel-desktop://copy/$LOCAL:Copy link"; do
  arg=${how%:*}; what=${how##*:}
  b=$(said "$what is not part of standalone")
  "$APP" "$arg" > /dev/null 2>&1 || true
  wait_said "$what is not part of standalone" 20 "$arg was not refused with a word" "$b"
done
[ "$(logged 'save and share: the replay is saved')" -eq 0 ] || fail "Save and share saved something"
ok "Save and share, Share the last clip, Upload the last clip and an old notification's Copy link: refused with a word"
"$APP" > /dev/null 2>&1 || true
waitlog "the server's page reached the app" 30 "the window did not open on the library"
sleep 2
WIN=$(window)
[ -n "$WIN" ] || fail "no window"
xdotool windowmove "$WIN" 0 0 windowsize "$WIN" 1280 820 2>/dev/null || true
sleep 2
shot "4-library"
# shellcheck disable=SC2086
xdotool mousemove ${REEL_CHIP:-1190 24} click 1
sleep 1
shot "5-chip"
xdotool key Escape
python3 /work/desktop/ci/fake-sni.py click "Pause recording" > /dev/null || fail "the tray has no Pause recording"
waitlog "recording paused by the user paused=true" 10 "Pause recording did not pause"
sleep 3
# shellcheck disable=SC2086
xdotool mousemove ${REEL_CHIP:-1190 24} click 1
sleep 1
shot "5-chip-recording-paused"
xdotool key Escape
python3 /work/desktop/ci/fake-sni.py click "Resume recording" > /dev/null || fail "the tray has no Resume recording"
waitlog "recording paused by the user paused=false" 10 "Resume recording did not resume"
"$APP" "reel-desktop://open/$LOCAL" > /dev/null 2>&1 || true
waitlog "library loaded at=127.0.0.1/clips/$LOCAL" 20 "Open did not open the clip"
sleep 3
shot "6-clip"
"$APP" --section=status > /dev/null 2>&1 || true
sleep 2
shot "7-this-app-status"
ok "the window: the library on this PC, the chip, a clip's page, Settings, Status (shots)"
"$APP" --section=hotkeys > /dev/null 2>&1 || true
waitlog "app page section=hotkeys" 20 "the Hotkeys page did not open"
sleep 2
shot "9-hotkeys"
b=$(logged "hotkeys off while a field listens")
# Save a clip's field, the one row of the Hotkeys panel in standalone, under the Linux line
# shellcheck disable=SC2086
xdotool mousemove ${REEL_SAVE_FIELD:-1137 289} click 1
for _ in $(seq 1 20); do [ "$(logged 'hotkeys off while a field listens')" -gt "$b" ] && break; sleep 0.5; done
[ "$(logged 'hotkeys off while a field listens')" -gt "$b" ] || fail "Save a clip's field did not listen"
xdotool key --clearmodifiers alt+F9
waitlog "hotkey free.*Alt+F9" 10 "Alt+F9 was not taken for Save a clip: $(cat "$L"/reel-desktop.log* | grep -m1 'hotkey refused' || true)"
for _ in $(seq 1 20); do grep -q '"save": "Alt+F9"' "$D/config/settings.json" && break; sleep 0.5; done
grep -q '"save": "Alt+F9"' "$D/config/settings.json" || fail "Alt+F9 was not saved for Save a clip"
shot "10-hotkey-recorded"
sleep 1
# Another clip than the tray's: the library drops a file it has already
"$FFMPEG" -v error -f lavfi -i "testsrc2=duration=2:size=1280x720:rate=30" -f lavfi -i "sine=frequency=660:sample_rate=48000:duration=2" \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest "$W/clip2.mp4"
echo "$W/clip2.mp4" > "$W/rec/next-clip"
b=$(logged "standalone: the clip is published")
asked=$(logged "save: OBS saves the replay")
xdotool key --clearmodifiers alt+F9
for _ in $(seq 1 20); do [ "$(logged 'save: OBS saves the replay')" -gt "$asked" ] && break; sleep 0.5; done
[ "$(logged 'save: OBS saves the replay')" -gt "$asked" ] || fail "Alt+F9 did nothing: $(cat "$L"/reel-desktop.log* | grep -i 'hotkey' | tail -5)"
for _ in $(seq 1 180); do [ "$(logged 'standalone: the clip is published')" -gt "$b" ] && break; sleep 0.5; done
[ "$(logged 'standalone: the clip is published')" -gt "$b" ] || fail "Alt+F9 did not save a clip"
ok "Save a clip's hotkey changed on the Hotkeys page, from the setup's Alt+F10 to Alt+F9: saved, and it saves a clip"
# Too quiet to measure: a peak, and a loudness under the gate all through
"$FFMPEG" -v error -f lavfi -i "testsrc2=duration=3:size=1280x720:rate=30" -f lavfi -i "anoisesrc=color=pink:amplitude=0.0008:r=48000:duration=3" \
  -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 160k -ac 2 -shortest "$W/quiet.mp4"
echo "$W/quiet.mp4" > "$W/rec/next-clip"
b=$(logged "standalone: the clip is published")
xdotool key --clearmodifiers alt+F9
for _ in $(seq 1 180); do [ "$(logged 'standalone: the clip is published')" -gt "$b" ] && break; sleep 0.5; done
[ "$(logged 'standalone: the clip is published')" -gt "$b" ] || fail "the quiet clip was not saved"
QUIET=$(cat "$L"/reel-desktop.log* | sed -n 's/.*standalone: the clip is published.* clip=\([0-9A-Z]*\).*/\1/p' | sed -n "$((b + 1))p")
[ -n "$QUIET" ] || fail "no id of the quiet clip in the log"
lifeless=$(logged "no sign of life from the page")
"$APP" "reel-desktop://open/$QUIET" > /dev/null 2>&1 || true
waitlog "library loaded at=127.0.0.1/clips/$QUIET" 20 "Open did not open the quiet clip"
sleep 12
# the editor's audio line, under the player
xdotool mousemove 400 600 click 5 click 5 click 5 click 5 click 5 click 5
sleep 1
shot "6b-quiet-clip"
[ "$(logged 'no sign of life from the page')" -eq "$lifeless" ] || fail "the quiet clip's page never called the app"
[ "$(logged "page error path=/clips/$QUIET")" -eq 0 ] || fail "the quiet clip's page had an error: $(cat "$L"/reel-desktop.log* | grep -m1 "page error path=/clips/$QUIET")"
ok "a clip too quiet to measure: its page calls the app, no page error in the Log"
echo "     page errors in the Log of the standalone pages: $(logged 'page error')"
CLIP_AT="library loaded at=127.0.0.1/clips/$LOCAL"
b=$(logged "$CLIP_AT")
"$APP" "reel-desktop://open/$LOCAL" > /dev/null 2>&1 || true
waitmore "$CLIP_AT" 20 "Open did not open the clip again" "$b"
sleep 3
b=$(logged "app page section=status")
"$APP" --section=status > /dev/null 2>&1 || true
waitmore "app page section=status" 20 "Status did not open" "$b"
sleep 2
b=$(logged "$CLIP_AT")
stalls=$(logged "the page stopped answering")
lifeless=$(logged "no sign of life from the page")
xdotool mousemove 640 500 click 8
waitmore "$CLIP_AT" 15 "the mouse's back button did not go from Status to the clip" "$b"
sleep 16
[ "$(logged 'the page stopped answering')" -eq "$stalls" ] || fail "the clip page, back from Status, stopped calling the app and was taken for stalled"
[ "$(logged 'no sign of life from the page')" -eq "$lifeless" ] || fail "the clip page, back from Status, never called the app"
ok "back from Status to the clip page WebKit kept: it calls the app again, no stall in 16 s"
b=$(logged "$CLIP_AT")
stalls=$(logged "the page stopped answering")
pkill -STOP -f WebKitWebProcess || fail "no web process to stop"
for _ in $(seq 1 60); do [ "$(logged 'the page stopped answering')" -gt "$stalls" ] && break; sleep 0.5; done
pkill -CONT -f WebKitWebProcess || true
[ "$(logged 'the page stopped answering')" -gt "$stalls" ] || fail "a web process that stopped answering was not taken for stalled"
for _ in $(seq 1 40); do [ "$(logged "$CLIP_AT")" -gt "$b" ] && break; sleep 0.5; done
sleep 2
last=$(cat "$L"/reel-desktop.log* | grep "library loaded" | tail -1)
printf '%s\n' "$last" | grep -q -- "$CLIP_AT" || fail "the stalled clip page loaded again elsewhere: $(cat "$L"/reel-desktop.log* | grep 'the page loads again' | tail -1 | sed 's/.*INFO //'); $last"
ok "a clip page whose web process stopped answering loads again on the clip"
kill "$SNI" 2>/dev/null || true
stop

# No StatusNotifier host now: the tray as a desktop with an XEmbed tray draws it.
rm -f "$W/sni.txt"
start --hidden
waitlog "standalone: reel serves on the loopback" 60 "reel on this PC did not start again"
sleep 4
# shellcheck disable=SC2086
xdotool mousemove ${REEL_TRAY:-1266 14} click 3 2>/dev/null || true
sleep 1
[ -z "$SHOTS" ] || import -display "$DISPLAY" -window root "$SHOTS/8-tray-menu.png"
xdotool key Escape 2>/dev/null || true
stop
ok "the tray's menu drawn (shot)"

# A server's pages in the window: a part that throws, then a page whose script is missing.
D="$W/pages"
L="$D/data/logs"
mkdir -p "$D/config"
PAGES_PORT=4497
python3 /work/desktop/ci/standin-pages.py "$PAGES_PORT" /work/web/assets > "$W/pages.out" 2>&1 &
PIDS="$PIDS $!"
printf '{"mode":"connected","server_url":"http://127.0.0.1:%s","obs":{"enabled":false},"startup":{"autostart":false},"log_level":"debug"}' "$PAGES_PORT" > "$D/config/settings.json"
export REEL_DESKTOP_DIR="$D"
start --hidden
waitlog "reel desktop starts" 30 "the app set to the stand-in server did not start"
"$APP" > /dev/null 2>&1 || true
waitlog "the server's page reached the app" 30 "a page with a part that throws did not reach the app"
waitlog "page error path=/ error=.*a part that throws, on purpose" 20 "the part that threw is not in the Log"
for _ in $(seq 1 20); do grep -q "the-last-part-ran" "$W/pages.out" && break; sleep 0.5; done
grep -q "the-last-part-ran" "$W/pages.out" || fail "the parts after the one that threw did not start"
echo "     the Log: $(cat "$L"/reel-desktop.log* | grep -m1 'page error' | sed 's/.*WARN //')"
ok "a page with a part that throws: it reaches the app, the part's error is in the Log, the last part ran"
"$APP" --page=/broken > /dev/null 2>&1 || true
waitlog "page error path=/broken error=a script did not load: /assets/missing.js" 20 "the script that did not load is not in the Log"
waitlog "cannot reach the server title=The page stopped at an error code=a script did not load: /assets/missing.js" 30 \
  "the offline page does not say the page had an error: $(cat "$L"/reel-desktop.log* | grep "WARN cannot reach the server" | tail -1)"
sleep 2
shot "11-page-error"
stop
ok "a page whose script did not load: the Log has it, the offline page says the page had an error, with its line"

D="$W/old"
L="$D/data/logs"
mkdir -p "$D/config"
printf '{"server_url":"http://127.0.0.1:9","obs":{"enabled":false},"startup":{"autostart":false},"log_level":"debug"}' > "$D/config/settings.json"
export REEL_DESKTOP_DIR="$D"
start --hidden
waitlog "reel desktop starts" 30 "the old app did not start"
sleep 4
[ "$(logged 'a new install')" -eq 0 ] && [ "$(logged 'standalone: reel serves')" -eq 0 ] || fail "an app set to a server turned standalone"
! grep -q '"mode": "standalone"' "$D/config/settings.json" || fail "the old settings were made standalone"
stop
ok "a settings file without a mode, set to a server: connected as it was"
! grep -q "panicked" "$W"/app*.out || fail "a thread panicked"
echo "all ok"
