#!/bin/sh
# Usage: installer-shots.sh <installer> <out folder> <its file before the version> [dpi, 96 is 100 %]
set -eu
EXE="$1" OUT="$2" NAME="$3" DPI="${4:-96}"
. /work/desktop/ci/packages.sh
S=/work/desktop/.wine-standins
W=$(mktemp -d)
# USER names Wine's profile folder, which the first page shows as the install folder
export HOME="$W/home" USER=sam WINEPREFIX="$W/wine" WINEDEBUG=-all DISPLAY=":$DPI" WINEDLLOVERRIDES="mscoree,mshtml="
mkdir -p "$HOME" "$OUT"
Xvfb ":$DPI" -screen 0 1280x1024x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1
wineboot -i >/dev/null 2>&1
wine winecfg -v win10 >/dev/null 2>&1
wine reg add 'HKCU\Control Panel\Desktop' /v LogPixels /t REG_DWORD /d "$DPI" /f >/dev/null 2>&1
wine reg add 'HKCU\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' /v pv /d 130.0.2849.80 /f >/dev/null 2>&1
wine reg add 'HKCU\Software\Wine\X11 Driver' /v Managed /d N /f >/dev/null 2>&1
wine reg add 'HKCU\Software\Wine\X11 Driver' /v Decorated /d Y /f >/dev/null 2>&1
cp /work/desktop/ci/selawik/*.ttf "$WINEPREFIX/drive_c/windows/Fonts/"
for f in "Segoe UI:Selawik" "Segoe UI Semibold:Selawik Semibold"; do
  wine reg add 'HKLM\Software\Microsoft\Windows NT\CurrentVersion\FontSubstitutes' /v "${f%%:*}" /d "${f#*:}" /f >/dev/null 2>&1
done
wineserver -w
fail() { printf 'FAIL %s\n' "$*"; wineserver -k || true; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
window() { # window <title> [not this id]: the id of the visible window of that title, waited for
  for _ in $(seq 1 60); do
    id=$(xdotool search --onlyvisible --name "^$1 ?\$" 2>/dev/null | grep -v "^${2:-none}\$" | tail -1 || true)
    [ -n "$id" ] && { echo "$id"; return 0; }
    sleep 1
  done
  return 1
}
# Waits on the installer's process, not its X window: X hands a closed window's id to the next
# program. This script's own shells name the installer too, and are left out.
ended() {
  for _ in $(seq 1 300); do
    pgrep -af "$(basename "$EXE")" 2>/dev/null | grep -vq '^[0-9]* sh ' || return 0
    sleep 1
  done
  return 1
}
shot() { # shot <window id> <file>: the window with its frame, once it has settled
  sleep 2
  import -window "$1" "$OUT/$NAME-$2.png"
  echo "shot $NAME-$2.png"
}
# Press and release sent apart: a window that closes on the press would otherwise leave the
# key held, and X would repeat it into the next window.
key() {
  timeout 5 xdotool windowfocus --sync "$1" 2>/dev/null || true
  for k in $(echo "$2" | tr '+' ' '); do xdotool keydown "$k"; done
  for k in $(echo "$2" | tr '+' ' ' | awk '{for (i = NF; i > 0; i--) printf "%s ", $i}'); do xdotool keyup "$k"; done
}
answers() { # answers <id> <shot before>
  xdotool windowminimize "$1"
  sleep 1
  xdotool windowactivate "$1" 2>/dev/null || xdotool windowmap "$1"
  sleep 2
  import -window "$1" "$W/after.png"
  changed=$(compare -metric AE "$OUT/$NAME-$2.png" "$W/after.png" null: 2>&1 | awk '{printf "%d", $1}')
  [ "$changed" -lt 200 ] || fail "minimized and restored while reel closed, the window did not paint itself again ($changed pixels differ)"
  ok "while reel closed, the window answered: minimized and restored, it painted itself again ($changed pixels differ)"
}
running() { pgrep -fi "$1" >/dev/null 2>&1; }
local_app() { find "$WINEPREFIX/drive_c/users" -maxdepth 4 -type d -path '*AppData/Local/reel' | head -1; }
installed() { for _ in $(seq 1 600); do d=$(local_app); [ -n "$d" ] && [ -f "$d/reel-elevate.exe" ] && return 0; sleep 1; done; return 1; }
# A stand-in reel that never quits when asked; without-obs: no OBS of its own in obs\
standin() {
  d=$(local_app)
  cp "$S/standin_app.exe" "$d/reel-desktop.exe"
  if [ "${1:-}" = without-obs ]; then
    rm -rf "$d/obs"
    (cd "$d" && wine "$d/reel-desktop.exe" >/dev/null 2>&1 &)
    for _ in $(seq 1 60); do running 'reel-desktop.exe' && break; sleep 0.5; done
    running 'reel-desktop.exe' || fail "the stand-in reel did not start"
    return
  fi
  mkdir -p "$d/obs/bin/64bit"
  cp "$S/standin_obs.exe" "$d/obs/bin/64bit/reel-capture.exe"
  (cd "$d" && wine "$d/reel-desktop.exe" >/dev/null 2>&1 &)
  for _ in $(seq 1 360); do running 'reel-capture.exe' && break; sleep 0.5; done
  running 'reel-capture.exe' || fail "the stand-in reel did not start its OBS"
}

TITLE=$([ "$NAME" = "$WITH_OBS_FILE" ] && echo "$WITH_OBS_NAME" || echo "$PLAIN_NAME")
run() { # run <prefix>: one install, a shot of each page; over a running reel as "over"
  p=$1
  d=$(local_app)
  [ -z "$d" ] || rm -f "$d/reel-elevate.exe"
  wine "$EXE" >/dev/null 2>&1 &
  id=$(window "$TITLE") || fail "no window \"$TITLE\""
  shot "$id" "$p-1-ready"
  if [ "$p" = install ]; then
    key "$id" alt+o
    shot "$id" "$p-1-options"
  fi
  if [ "$p" = over ] && [ "$TITLE" = "$PLAIN_NAME" ]; then
    # the package without OBS over the stand-in's OBS in obs\ takes it for the one with: its box first
    key "$id" space
    shot "$id" "$p-1-ready-ticked"
  fi
  key "$id" Return
  if [ "$p" = over ]; then
    shot "$id" "$p-2-closing-reel"
    running 'reel-stop' || fail "the stop helper had ended before the window was looked at; the proof needs it running"
    answers "$id" "$p-2-closing-reel"
  fi
  installed || fail "the install did not finish"
  shot "$id" "$p-3-done"
  wineserver -k || true
  wineserver -w || true
  sleep 1
}
run install
standin
run over
ok "the install over a running reel: Closing reel, then the install and the end"

if [ "$TITLE" = "$PLAIN_NAME" ]; then standin without-obs; else standin; fi
d=$(local_app)
rm -f "$d/reel-elevate.exe" "$d/logs/installer.log"
wine "$EXE" /P /UPDATE /R /ARGS --hidden >/dev/null 2>&1 &
id=$(window "$TITLE") || fail "no small window for an update"
shot "$id" "update-1-small"
h=$(xdotool getwindowgeometry "$id" | sed -n 's/.*Geometry: [0-9]*x\([0-9]*\).*/\1/p')
[ "$h" -lt $((260 * DPI / 96)) ] || fail "the update's window is not the small one ($h px high)"
pkill -f 'standin_obs|reel-capture|reel-desktop' || true
ended || fail "the update's window did not close by itself"
installed || fail "the update did not finish"
grep -q 'Closing reel' "$d/logs/installer.log" && grep -q 'reel starts again' "$d/logs/installer.log" || fail "logs\\installer.log has not the update's steps: $(cat "$d/logs/installer.log" 2>/dev/null)"
ok "the update's small window ($h px high), closed by itself, and reel started again with its arguments; the steps with their times in logs\\installer.log"
wineserver -k || true
wineserver -w || true
sleep 1

R=$(wine cmd /c "echo %APPDATA%" 2>/dev/null | tr -d '\r')
P=$(wine cmd /c "echo %USERPROFILE%" 2>/dev/null | tr -d '\r')
SET="$(winepath -u "$R")/reel"
C="$(winepath -u 'C:\')Clips"
d=$(local_app)
mkdir -p "$SET"
printf '{\n  "clips_dir": "%s",\n  "device_name": "kept"\n}' "$(printf '%s' "$P" | sed 's/\\/\\\\/g')" > "$SET/settings.json"
wine "$d/uninstall.exe" >/dev/null 2>&1 &
id=$(window "Remove reel") || fail "no window \"Remove reel\""
shot "$id" "kept-1-choices"
key "$id" Escape
for _ in $(seq 1 30); do xdotool search --onlyvisible --name '^Remove reel ?$' >/dev/null 2>&1 || break; sleep 1; done
sleep 2
[ -f "$d/reel-desktop.exe" ] && [ -d "$(winepath -u "$P")" ] || fail "closing the uninstaller took reel or the profile"
ok "the profile as the clips folder: kept, the page says why, nothing removed"

mkdir -p "$SET" "$C/Valorant"
printf '{\n  "clips_dir": "C:\\\\Clips",\n  "device_name": "kept"\n}' > "$SET/settings.json"
head -c 3000000 /dev/urandom > "$C/Valorant/Replay 2026-10-13 21-14-03.mp4"
head -c 2000000 /dev/urandom > "$C/Replay 2026-10-12 18-20-31.mkv"
d=$(local_app)
wine "$d/uninstall.exe" >/dev/null 2>&1 &
id=$(window "Remove reel") || fail "no window \"Remove reel\""
shot "$id" "remove-1-choices"
# the first box has the focus; Tab goes to the clips folder's, Space ticks it
key "$id" Tab
key "$id" space
ask=$(window "Remove reel" "$id") || fail "ticking the clips folder asked nothing"
shot "$ask" "remove-2-clips-asked"
key "$ask" y
shot "$id" "remove-3-choices-ticked"
key "$id" Return
# OBS's thousands of files take a while to go
for _ in $(seq 1 300); do [ ! -f "$d/reel-desktop.exe" ] && [ ! -e "$d/obs" ] && [ ! -e "$C" ] && break; sleep 1; done
shot "$id" "remove-4-removed"
[ ! -e "$C" ] || fail "the clips folder was ticked and stays"
grep -q kept "$SET/settings.json" || fail "the settings were not ticked and went"
ok "the uninstaller: its choices, the clips folder asked about once more and deleted, the settings kept"
wineserver -k || true
