#!/bin/sh
# $1 may be an extracted AppImage's AppRun.
set -eu
APP="${1:-/target/debug/reel-desktop}"
W=$(mktemp -d)
[ -x /target/debug/examples/fake_obs ] || { echo "FAIL no stand-in OBS: make desktop-linux builds it"; exit 1; }
fail() { echo "FAIL $*"; grep -m1 -A1 "panicked" "$W/app.out" || tail -20 "$W/app.out"; tail -5 "$L"/reel-desktop.log* 2>/dev/null; exit 1; }
ok() { echo "ok   $*"; }
/work/desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu -v error -f lavfi -i testsrc=duration=2:size=320x240:rate=30 \
  -c:v libx264 -pix_fmt yuv420p "$W/clip.mp4"
mkdir -p "$W/d/config" "$W/rec"
python3 /work/desktop/ci/fake-notifications.py > "$W/notifications.txt" 2>&1 &
python3 /work/desktop/ci/fake-sni.py > "$W/sni.txt" 2>&1 &
/target/debug/examples/fake_obs --port 4499 --dir "$W/rec" --clip "$W/clip.mp4" > "$W/fake-obs.out" 2>&1 &
printf '{"server_url":"","obs":{"enabled":true,"host":"127.0.0.1","port":4499,"password":"","start_replay":false},"clips_follow_obs":true,"clips_dir":"%s","startup":{"autostart":false},"log_level":"debug"}' "$W/rec" \
  > "$W/d/config/settings.json"
export REEL_DESKTOP_DIR="$W/d" RUST_BACKTRACE=1
L="$W/d/data/logs"
"$APP" > "$W/app.out" 2>&1 &
for _ in $(seq 1 60); do grep -qs "obs connected" "$L"/reel-desktop.log* && break; sleep 1; done
grep -qs "obs connected" "$L"/reel-desktop.log* || fail "the app did not connect to the stand-in OBS"
grep -q "notifications up" "$W/notifications.txt" || fail "no notification server"
for _ in $(seq 1 20); do grep -q "^item " "$W/sni.txt" && break; sleep 0.5; done
grep -q "^item " "$W/sni.txt" || fail "the app's tray did not register with the StatusNotifier host"
sleep 3
saves() { grep -c "save: OBS saves the replay\|save and share: the replay is saved" "$L"/reel-desktop.log* | tail -1; }
saved() { # saved <before> <what>: one more save reached OBS
  for _ in $(seq 1 20); do [ "$(saves)" -gt "$1" ] && return 0; sleep 0.5; done
  fail "$2 did not reach OBS"
}

# The stand-in writes the replay in 3 s: the save's sound must come before it.
echo 3000 > "$W/rec/write-ms"
cues() { grep -c "cue captured played" "$L"/reel-desktop.log* | tail -1; }
written() { grep -c "obs saved a replay" "$L"/reel-desktop.log* | tail -1; }
ms() { python3 /work/desktop/ci/ms-after.py "$1" "$2" "$L"; }
sound_after() { # sound_after <moment>: ms from it to the next save's sound
  for _ in $(seq 1 20); do ms "$1" "cue captured played" && return 0; sleep 0.25; done
  return 1
}
c=$(cues); w=$(written)
KEY=$(date +%s.%N)
xdotool key --clearmodifiers ctrl+alt+shift+s
FIRST=$(sound_after "$KEY") || fail "Save and share played no sound"
sleep 1.5
AGAIN=$(date +%s.%N)
xdotool key --clearmodifiers ctrl+alt+shift+s
SECOND=$(sound_after "$AGAIN") || fail "a second press while Save and share ran played no sound"
for _ in $(seq 1 40); do [ "$(written)" -ge $((w + 2)) ] && break; sleep 0.25; done
[ "$(written)" -ge $((w + 2)) ] || fail "the second press saved no other clip"
WRITTEN=$(ms "$KEY" "obs saved a replay")
sleep 1
grep -q "save and share: a newer one was pressed" "$L"/reel-desktop.log* || fail "the second press was not a Save and share of its own"
[ "$FIRST" -lt 1000 ] && [ "$FIRST" -lt "$WRITTEN" ] || fail "Save and share's sound came $FIRST ms after the key, the replay at $WRITTEN ms"
[ "$SECOND" -lt 1000 ] || fail "the second press's sound came $SECOND ms after it"
[ "$(cues)" -eq $((c + 2)) ] || fail "two saves sounded $(( $(cues) - c )) times"
rm -f "$W/rec/write-ms"
ok "Save and share sounded $FIRST ms after the key, as OBS took the save (the replay written at $WRITTEN ms); a press while it ran saved another clip, sounding $SECOND ms after it; each save sounded once"

apart() { sleep 1.2; } # a press within a second of the last save is a double tap
apart
n=$(saves)
# The bar's "..." from the window's size, as reel.css places it: 26 px wide, 20 px from the right
# edge, in the 30 px at the foot. Esc closes its menu and gives "..." the focus; Enter opens it
# from the keyboard, on its first item, Save clip.
eval "$(xdotool getwindowgeometry --shell "$(xdotool search --onlyvisible --name '^reel$' | head -1)")"
# shellcheck disable=SC2086
xdotool mousemove ${REEL_BAR_MORE:-$((X + WIDTH - 33)) $((Y + HEIGHT - 15))} click 1
sleep 1
xdotool key Escape
sleep 0.3
xdotool key Return
sleep 0.5
xdotool key Return
saved "$n" "the bar's Save clip"
ok "the bar of the app's own pages: its menu opens from the keyboard on Save clip, which reached OBS"

apart
n=$(saves)
python3 /work/desktop/ci/fake-sni.py click "Save clip" || fail "no Save clip in the tray's menu"
saved "$n" "the tray's Save clip"
ok "the tray's Save clip, clicked through its DBusMenu as a bar does, reached OBS"
apart
: > "$W/rec/fail-next"
python3 /work/desktop/ci/fake-sni.py click "Save clip" > /dev/null
for _ in $(seq 1 20); do grep -q "^notification reel: No replay saved" "$W/notifications.txt" && break; sleep 0.5; done
grep -q "^notification reel: No replay saved" "$W/notifications.txt" || fail "a save OBS refused gave no notification: $(cat "$W/notifications.txt")"
ok "the tray's Save clip refused by OBS: the notification \"No replay saved\" reached the server"

apart
n=$(saves)
xdotool key --clearmodifiers ctrl+alt+shift+s
saved "$n" "the hotkey Ctrl+Alt+Shift+S (Save and share)"
ok "the hotkey on X11, Ctrl+Alt+Shift+S, reached OBS (with a Save and share running, as another clip)"

for flag in --save --save-and-share; do
  apart
  n=$(saves)
  "$APP" "$flag" > /dev/null 2>&1 || true
  saved "$n" "$flag"
done
ok "the command line, as a desktop's own shortcut and the portal's run it: --save and --save-and-share reached OBS"
sleep 3
! grep -q "panicked" "$W/app.out" || fail "a thread panicked: $(grep -m1 -A1 panicked "$W/app.out")"
sent=$(grep -c "^notification reel: Replay saved" "$W/notifications.txt" || true)
[ "$sent" -ge 1 ] || fail "the notifications did not arrive: $(cat "$W/notifications.txt")"
ok "no thread panicked; $sent notification \"Replay saved\" reached the notification server, from Save and share (the presses while it ran saved clips of their own)"
