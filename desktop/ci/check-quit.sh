#!/bin/sh
# $1 may be an extracted AppImage's AppRun.
set -eu
APP="${1:-/target/debug/reel-desktop}"
W=$(mktemp -d)
[ -x /target/debug/examples/fake_obs ] || { echo "FAIL no stand-in OBS: make desktop-linux builds it"; exit 1; }
fail() { echo "FAIL $*"; grep -m1 -A1 "panicked" "$W/app.out" || tail -20 "$W/app.out"; tail -8 "$L"/reel-desktop.log* 2>/dev/null; exit 1; }
ok() { echo "ok   $*"; }
mkdir -p "$W/d/config" "$W/rec"
python3 /work/desktop/ci/fake-sni.py > "$W/sni.txt" 2>&1 &
/work/desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu -v error -f lavfi -i testsrc=duration=1:size=160x120:rate=30 \
  -c:v libx264 -pix_fmt yuv420p "$W/clip.mp4"
printf '{"server_url":"","obs":{"bundled":true,"setup":{"done":true}},"startup":{"autostart":false},"log_level":"info"}' \
  > "$W/d/config/settings.json"
export REEL_OBS_EXE=/work/desktop/ci/standin-obs-slow.sh STANDIN_DIR="$W/rec" STANDIN_CLIP="$W/clip.mp4" STANDIN_CLOSE_S=3
export REEL_DESKTOP_DIR="$W/d" RUST_BACKTRACE=1
L="$W/d/data/logs"
now() { date +%s.%N; }
ms_since() { python3 -c "import sys,time; print(round((time.time()-float(sys.argv[1]))*1000))" "$1"; }
running() { pgrep -f "^$APP" >/dev/null 2>&1; }
shown() { xdotool search --onlyvisible --name '^reel$' >/dev/null 2>&1; }
says() { cat "$L"/reel-desktop.log* 2>/dev/null | grep -c "$1" || true; }
up() { # up <n>: the app's nth start up, its tray registered, its window on the screen, OBS answered
  for _ in $(seq 1 90); do [ "$(says "obs connected")" -ge "$1" ] && shown && break; sleep 1; done
  [ "$(says "obs connected")" -ge "$1" ] || fail "the app did not connect to the stand-in OBS"
  shown || fail "the window did not show"
  for _ in $(seq 1 20); do [ "$(grep -c '^item ' "$W/sni.txt")" -ge "$1" ] && break; sleep 0.5; done
  [ "$(grep -c '^item ' "$W/sni.txt")" -ge "$1" ] || fail "the app's tray did not register with the StatusNotifier host"
  sleep 2
}
status() {
  item=$(grep '^item ' "$W/sni.txt" | tail -1)
  dbus-send --session --print-reply --dest="$(echo "$item" | cut -d' ' -f2)" "$(echo "$item" | cut -d' ' -f3)" \
    org.freedesktop.DBus.Properties.Get string:org.kde.StatusNotifierItem string:Status 2>/dev/null \
    | sed -n 's/.*string "\(.*\)".*/\1/p'
}

"$APP" > "$W/app.out" 2>&1 &
up 1
n=0
for step in "start: the tray icon and the app are up" "start: the window is made" "start: OBS starts behind the window" "the bundled OBS started"; do
  m=$(cat "$L"/reel-desktop.log* | grep -n -m1 "$step" | cut -d: -f1)
  [ -n "$m" ] && [ "$m" -gt "$n" ] || fail "the start's steps are not in order at \"$step\""
  n=$m
done
ok "the start: the tray icon and the app, the window at $(grep -h -m1 'start: the window is made' "$L"/reel-desktop.log* | sed -n 's/.* at=\([0-9.]* s\).*/\1/p'), then OBS behind it"
[ "$(status)" = "Active" ] || fail "the tray's item is $(status), not Active, before the quit"
T0=$(now)
python3 /work/desktop/ci/fake-sni.py click "Quit" > /dev/null || fail "no Quit in the tray's menu"
WIN="" TRAY="" OUT=""
for _ in $(seq 1 400); do
  [ -z "$WIN" ] && ! shown && WIN=$(ms_since "$T0")
  [ -z "$TRAY" ] && running && [ "$(status)" != "Active" ] && TRAY=$(ms_since "$T0")
  [ -z "$OUT" ] && ! running && OUT=$(ms_since "$T0") && break
  sleep 0.02
done
[ -n "$OUT" ] || fail "the app did not exit within 8 s of Quit"
[ -n "$WIN" ] && [ "$WIN" -lt 1000 ] || fail "the window stayed on the screen ${WIN:-until the app was out} ms"
[ -n "$TRAY" ] && [ "$TRAY" -lt 1000 ] || fail "the tray's item stayed Active ${TRAY:-until the app was out} ms"
[ "$OUT" -ge 3000 ] || fail "the app was out after $OUT ms, before its OBS had closed (3 s)"
ok "Quit in the tray: the window gone after $WIN ms, the tray's item Passive after $TRAY ms; OBS took its 3 s behind them, the app out after $OUT ms"
for step in "quit: asked" "quit: the window and the tray icon are gone" "OBS asked to close" "the bundled OBS exited, after it was asked to close" "quit: OBS is stopped" "quit: done; the app exits"; do
  grep -q "$step" "$L"/reel-desktop.log* || fail "the log does not say \"$step\""
done
last=$(cat "$L"/reel-desktop.log* | tail -1)
case "$last" in *"quit: done; the app exits"*) ;; *) fail "the log's last line is not the quit's: $last" ;; esac
at() { grep -h "$1" "$L"/reel-desktop.log* | tail -1 | sed -n 's/.* \(after\|at\)=\([0-9.]* s\).*/\2/p'; }
ok "the log: the window and the tray icon gone at $(at "the tray icon are gone"), OBS's exit seen $(at "the bundled OBS exited") after it was asked, OBS stopped at $(at "quit: OBS is stopped"), written out to its last line"

"$APP" > "$W/app2.out" 2>&1 &
up 2
python3 /work/desktop/ci/fake-sni.py click "Quit" > /dev/null || fail "no Quit in the tray's menu, the second time"
"$APP" --hidden > "$W/app3.out" 2>&1 &
for _ in $(seq 1 60); do running && [ "$(says "reel desktop starts")" -ge 3 ] && break; sleep 0.5; done
sleep 2
n=$(pgrep -fc "^$APP" || true)
[ "$n" = 1 ] || fail "$n apps run after a Quit and a start right after it, not one"
grep -q "it starts again once out" "$L"/reel-desktop.log* || fail "the start during the quit was not handed to the app on its way out"
ok "a start while the quit stops OBS is not lost: handed to the app on its way out, reel started again once out, one app runs"
python3 /work/desktop/ci/fake-sni.py click "Quit" > /dev/null || true
for _ in $(seq 1 60); do running || break; sleep 0.2; done
! running || fail "the app did not quit at the end"
! grep -q "panicked" "$W"/app*.out || fail "a thread panicked: $(grep -m1 -A1 panicked "$W"/app*.out)"
ok "no thread panicked"
