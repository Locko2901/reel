#!/bin/sh
# The library on another drive and a copy of it, in the real app in standalone; under xvfb.sh with
# XVFB_SCREEN=1280x1200. The container's own disk and the mount at /out stand in for two drives.
set -eu
APP="${1:-/target/debug/reel-desktop}"
SHOTS="${REEL_SHOTS_OUT:-}"
FFMPEG=/work/desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu
[ -x /target/debug/examples/fake_obs ] || { echo "FAIL no stand-in OBS: make desktop-linux builds it"; exit 1; }
W=$(mktemp -d)
X="${REEL_OTHER_DRIVE:-/out}/reel-library-check-$$"
mkdir -p "$X"
PIDS=""
D="$W/pc"
L="$D/data/logs"
LIB="$D/data/library"
fail() {
  echo "FAIL $*"
  grep -m1 -A1 "panicked" "$W"/app*.out 2>/dev/null || tail -n 20 "$W"/app*.out 2>/dev/null
  tail -n 30 "$L"/reel-desktop.log* 2>/dev/null
  [ -z "$SHOTS" ] || cp "$L"/reel-desktop.log* "$SHOTS"/ 2>/dev/null || true
  exit 1
}
ok() { echo "ok   $*"; }
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; pkill -f fake_obs 2>/dev/null || true; rm -rf "$W" "$X"; }
trap cleanup EXIT
[ "$(stat -c %d "$W")" != "$(stat -c %d "$X")" ] || fail "$W and $X are on one drive"
logged() { cat "$L"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
waitlog() { # waitlog <words> <seconds> <what> [count before]
  for _ in $(seq 1 $(($2 * 2))); do [ "$(logged "$1")" -gt "${4:-0}" ] && return 0; sleep 0.5; done
  fail "$3"
}
start() { "$APP" "$@" > "$W/app$(date +%s%N).out" 2>&1 & PIDS="$PIDS $!"; APP_PID=$!; }
stop() { kill "$APP_PID" 2>/dev/null || true; for _ in $(seq 1 40); do kill -0 "$APP_PID" 2>/dev/null || break; sleep 0.25; done; }
window() { xdotool search --onlyvisible --name '^reel$' 2>/dev/null | head -1; }
shot() { # shot <name>: the window
  [ -n "$SHOTS" ] || return 0
  win=$(window)
  if [ -n "$win" ]; then import -display "$DISPLAY" -window "$win" "$SHOTS/$1.png"; else import -display "$DISPLAY" -window root "$SHOTS/$1.png"; fi
}
click() { # click <x y>
  # shellcheck disable=SC2086
  xdotool mousemove $1 click 1
}
# The system's folder picker (GTK): the path typed into its location field. Without a window
# manager nothing gives the new window the keyboard, so it is focused by hand.
PICKER='Select Folder|Open Folder|Choose|Folder'
picker() { xdotool search --onlyvisible --name "$PICKER" 2>/dev/null | grep -vx "$(window)" | head -1; }
pick() { # pick <folder>
  for _ in $(seq 1 20); do [ -n "$(picker)" ] && break; sleep 0.5; done
  dlg=$(picker)
  [ -n "$dlg" ] || fail "the folder picker did not open"
  echo "     the picker: $(xdotool getwindowname "$dlg")"
  sleep 1
  xdotool windowfocus --sync "$dlg" 2>/dev/null || xdotool windowfocus "$dlg"
  # out of GTK's Recent view, where a typed path is not taken: its Home in the sidebar
  # shellcheck disable=SC2086
  xdotool mousemove ${REEL_PICKER_HOME:-57 59} click 1
  sleep 0.5
  xdotool key ctrl+l
  sleep 0.5
  xdotool type --delay 20 "$1"
  sleep 0.5
  xdotool key Delete Return
  sleep 1
  [ -z "$SHOTS" ] || [ -z "$(picker)" ] || import -display "$DISPLAY" -window root "$SHOTS/picker.png"
  for _ in 1 2 3; do
    [ -n "$(picker)" ] || break
    xdotool windowfocus "$dlg" 2>/dev/null || true
    xdotool key Return
    sleep 1
  done
  [ -z "$(picker)" ] || fail "the folder picker did not close"
  xdotool windowfocus "$(window)" 2>/dev/null || true
}
# The picker on a folder with folders in it: GTK opens such a folder on Enter (its completion of
# the only child taken away first), and Open then picks the folder it is in.
pick_open() { # pick_open <folder>
  for _ in $(seq 1 20); do [ -n "$(picker)" ] && break; sleep 0.5; done
  dlg=$(picker)
  [ -n "$dlg" ] || fail "the folder picker did not open"
  sleep 1
  xdotool windowfocus --sync "$dlg" 2>/dev/null || xdotool windowfocus "$dlg"
  # shellcheck disable=SC2086
  xdotool mousemove ${REEL_PICKER_HOME:-57 59} click 1
  sleep 0.5
  xdotool key ctrl+l
  sleep 0.5
  xdotool type --delay 20 "$1/"
  sleep 0.5
  xdotool key Delete Return
  sleep 1.5
  if [ -n "$(picker)" ]; then
    # shellcheck disable=SC2086
    xdotool mousemove ${REEL_PICKER_OPEN:-1046 798} click 1
    sleep 1
  fi
  [ -z "$(picker)" ] || fail "the folder picker did not close"
  xdotool windowfocus "$(window)" 2>/dev/null || true
}
clips() { # the clips the library on this PC lists
  port=$(cat "$L"/reel-desktop.log* | sed -n 's/.*standalone: reel serves on the loopback.*addr=127.0.0.1:\([0-9]*\).*/\1/p' | tail -1)
  curl -fsS "http://127.0.0.1:$port/" | grep -o '/clips/[0-9A-Z]\{26\}' | sort -u | wc -l
}
server_page() { # This app, Server, the window at the size the coordinates are for
  b=$(logged "app page section=server")
  "$APP" --section=server > /dev/null 2>&1 || true
  # a section asked for while the window is made can land on the first page: asked again
  for _ in $(seq 1 20); do [ "$(logged "app page section=server")" -gt "$b" ] && break; sleep 0.5; done
  [ "$(logged "app page section=server")" -gt "$b" ] || "$APP" --section=server > /dev/null 2>&1 || true
  waitlog "app page section=server" 20 "This app, Server did not open" "$b"
  sleep 2
  WIN=$(window)
  [ -n "$WIN" ] || fail "no window"
  xdotool windowmove "$WIN" 0 0 windowsize "$WIN" 1280 1160 2>/dev/null || true
  sleep 2
}
[ -z "$SHOTS" ] || mkdir -p "$SHOTS"
for n in 1 2 3; do
  "$FFMPEG" -v error -f lavfi -i "testsrc2=duration=$((n + 1)):size=1280x720:rate=30" -f lavfi -i "sine=frequency=$((330 * n)):sample_rate=48000:duration=$((n + 1))" \
    -c:v libx264 -pix_fmt yuv420p -c:a aac -shortest "$W/clip$n.mp4"
done
save() { # save <n>: Save clip, OBS's replay being clip n
  echo "$W/clip$1.mp4" > "$W/rec/next-clip"
  b=$(logged "standalone: the clip is published")
  "$APP" --save > /dev/null 2>&1 || true
  waitlog "standalone: the clip is published" 90 "clip $1 did not go into the library" "$b"
}

mkdir -p "$D/config" "$W/rec" "$X/drive" "$W/backup"
cat > "$D/config/settings.json" <<JSON
{
  "mode": "standalone",
  "standalone": { "hwaccel": "none", "cap_gb": 0, "cap_keeps_unsent": true, "copies_on_hardware": false },
  "obs": { "enabled": true, "host": "127.0.0.1", "port": 4499, "password": "", "start_replay": false },
  "clips_follow_obs": true,
  "clips_dir": "$W/rec",
  "stable_seconds": 2,
  "startup": { "autostart": false, "elevated": false },
  "log_level": "debug"
}
JSON
/target/debug/examples/fake_obs --port 4499 --dir "$W/rec" --clip "$W/clip1.mp4" > "$W/fake-obs.out" 2>&1 &
PIDS="$PIDS $!"
export REEL_DESKTOP_DIR="$D" RUST_BACKTRACE=1
start --hidden
waitlog "standalone: reel serves on the loopback" 60 "reel on this PC did not start"
waitlog "obs connected" 30 "the app did not connect to the stand-in OBS"
save 1
save 2
[ "$(clips)" -eq 2 ] || fail "the library lists $(clips) clips, not 2"
# a filler in a clip's folder, so the move takes long enough to be seen
C1=$(ls "$LIB/library" | grep -v '^\.' | head -1)
head -c "${REEL_FILLER_MB:-800}M" /dev/urandom > "$LIB/library/$C1/filler.bin"
ok "standalone with two clips in $LIB"

server_page
shot "1-library-panel"

# the move, with a second folder for the other drive
click "${REEL_LIB_MOVE:-1222 738}"
pick "$X/drive"
sleep 2
shot "2-move-plan"
b=$(logged "standalone: reel stopped")
xdotool key Return
waitlog "standalone: reel stopped" 20 "the move did not stop reel on this PC" "$b"
for _ in $(seq 1 40); do [ "$(du -sm "$X/drive" | cut -f1)" -gt 100 ] && break; sleep 0.25; done
sleep 0.5
shot "3-move-copying"
waitlog "library: the settings name the new folder" 240 "the move did not switch"
waitlog "The library is in $X/drive now" 120 "the move did not end"
sleep 2
shot "4-move-done"
grep -q "\"library_dir\": \"$X/drive\"" "$D/config/settings.json" || fail "the settings do not name the new folder: $(grep library_dir "$D/config/settings.json")"
[ ! -e "$LIB" ] || fail "the old folder is still there: $(ls -A "$LIB")"
[ -f "$X/drive/reel.db" ] && [ -f "$X/drive/library/.reel-library" ] && [ -f "$X/drive/library/$C1/filler.bin" ] || fail "the new folder does not hold the library"
waitlog "standalone: reel serves on the loopback" 30 "reel on this PC did not start from the new folder" 1
[ "$(clips)" -eq 2 ] || fail "after the move the library lists $(clips) clips"
ok "the move across drives: copied, checked, switched, old folder removed; reel runs from $X/drive with both clips"

save 3
[ "$(clips)" -eq 3 ] || fail "a clip after the move: $(clips) clips"
ok "a clip saved after the move goes into the moved library"

# on the same drive: one rename
mkdir -p "$X/same"
click "${REEL_LIB_MOVE:-1222 738}"
pick "$X/same"
sleep 2
shot "5-move-same-drive-plan"
xdotool key Return
waitlog "The library is in $X/same now" 60 "the move on the same drive did not end"
[ ! -e "$X/drive" ] && [ -f "$X/same/reel.db" ] || fail "the rename: $(ls -A "$X")"
waitlog "standalone: reel serves on the loopback" 30 "reel on this PC did not start after the rename" 2
[ "$(clips)" -eq 3 ] || fail "after the rename: $(clips) clips"
ok "a move on the same drive: the folder renamed, reel runs from $X/same"

# the copy, while reel works
click "${REEL_LIB_COPY:-1202 829}"
pick "$W/backup"
sleep 2
shot "6-copy-plan"
xdotool key Return
for _ in $(seq 1 40); do [ -n "$(ls "$W/backup")" ] && [ "$(du -sm "$W/backup" | cut -f1)" -gt 100 ] && break; sleep 0.25; done
shot "7-copy-running"
waitlog "The copy is in $W/backup/reel copy" 240 "the copy did not end"
sleep 2
shot "8-copy-done"
COPY=$(ls -d "$W/backup"/reel\ copy\ * | head -1)
case "$COPY" in *unfinished*) fail "the copy kept its unfinished name";; esac
[ -f "$COPY/reel.db" ] && [ -f "$COPY/README.txt" ] && [ -f "$COPY/library/.reel-library" ] && [ -f "$COPY/library/$C1/filler.bin" ] || fail "the copy is not whole: $(ls -A "$COPY")"
ok "a copy while reel runs: $COPY, with its README"
stop

# a move stopped by the app's end halfway, then the next start
b=$(logged "standalone: reel serves on the loopback")
start --hidden
waitlog "standalone: reel serves on the loopback" 60 "reel on this PC did not start again" "$b"
server_page
mkdir -p "$W/other"
click "${REEL_LIB_MOVE:-1222 738}"
pick "$W/other"
sleep 2
xdotool key Return
for _ in $(seq 1 80); do [ "$(du -sm "$W/other" | cut -f1)" -gt 100 ] && break; sleep 0.1; done
kill -9 "$APP_PID"
sleep 1
[ -f "$D/data/library-move.json" ] || fail "no journal for the move cut short"
grep -q '"phase": "copying"\|"phase": "checking"' "$D/data/library-move.json" || fail "the journal: $(cat "$D/data/library-move.json")"
b=$(logged "standalone: reel serves on the loopback")
start --hidden
waitlog "library: a move the app did not finish" 30 "the next start did not see the move cut short"
waitlog "standalone: reel serves on the loopback" 60 "reel on this PC did not start after the move cut short" "$b"
[ ! -f "$D/data/library-move.json" ] || fail "the journal is still there"
[ -z "$(ls -A "$W/other")" ] || fail "the target still holds: $(ls -A "$W/other")"
grep -q "\"library_dir\": \"$X/same\"" "$D/config/settings.json" || fail "the settings moved on"
[ "$(clips)" -eq 3 ] || fail "after the roll back: $(clips) clips"
server_page
shot "9-move-cut-short"
stop
ok "a move cut short by the app's end: the next start rolled it back, the library stayed whole in $X/same"

# the copy taken up as the library
b=$(logged "standalone: reel serves on the loopback")
start --hidden
waitlog "standalone: reel serves on the loopback" 60 "reel on this PC did not start" "$b"
server_page
click "${REEL_LIB_MOVE:-1222 738}"
pick_open "$COPY"
sleep 2
shot "10-use-plan"
b=$(logged "standalone: reel serves on the loopback")
xdotool key Return
waitlog "standalone: reel serves on the loopback" 60 "reel did not start from the copy" "$b"
grep -q "\"library_dir\": \"$COPY\"" "$D/config/settings.json" || fail "the settings do not name the copy"
[ "$(clips)" -eq 3 ] || fail "the copy lists $(clips) clips"
[ -f "$X/same/reel.db" ] || fail "the library used before is not where it was"
sleep 2
shot "11-use-done"
stop
ok "the copy taken up as the library: three clips, the library before left where it was"

# the drive away, and back
mv "$COPY" "$W/away"
b=$(logged "standalone: reel on this PC waits for its library")
start --hidden
waitlog "standalone: reel on this PC waits for its library" 30 "reel on this PC did not wait for its library" "$b"
server_page
shot "12-library-away"
b=$(logged "standalone: reel serves on the loopback")
mv "$W/away" "$COPY"
waitlog "library: its folder is back" 30 "the library's return was not seen"
waitlog "standalone: reel serves on the loopback" 60 "reel did not start once the library was back" "$b"
stop
ok "a library whose drive is away: reel on this PC waits, and starts by itself once it is back"
