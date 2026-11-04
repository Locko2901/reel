#!/bin/sh
# For Wine only: a stand-in icuuc.dll, wine_get_version renamed to skip OBS's Wine warning, and
# OpenGL, as Direct3D 11 stops OBS.
set -eu
S=/work/desktop/.wine-standins
W=$(mktemp -d)
export HOME="$W/home" WINEPREFIX="$W/pfx" WINEDEBUG=-all DISPLAY=:94 WINEDLLOVERRIDES="mscoree,mshtml="
mkdir -p "$HOME"
fail() { printf 'FAIL %s\n' "$*"; wineserver -k 2>/dev/null || true; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
# The pin's zip, not whichever version the cache lists first.
ZIP=$(/work/desktop/ci/fetch-obs.sh zip -) || fail "the pinned OBS zip (desktop/ci/fetch-obs.sh)"
Xvfb :94 -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1
wine wineboot --init >/dev/null 2>&1 || true
wine reg add 'HKCU\Software\Wine\Explorer' /v Desktop /d Default /f >/dev/null 2>&1
wine reg add 'HKCU\Software\Wine\Explorer\Desktops' /v Default /d 1280x800 /f >/dev/null 2>&1
PF="$WINEPREFIX/drive_c/Program Files/reel"
B="$PF/obs/bin/64bit"
mkdir -p "$PF/obs/config/obs-studio"
unzip -q "$ZIP" -d "$PF/obs"
perl -pi -e 's/wine_get_version/wine_get_versioX/g' "$B/obs64.exe"
mv "$B/obs64.exe" "$B/reel-capture.exe"
clang --target=x86_64-pc-windows-msvc -O1 -nostdlib -ffreestanding -shared -fuse-ld=lld -Wl,/noentry \
  -o "$B/icuuc.dll" /work/desktop/ci/icuuc-stub.c
: > "$PF/obs/portable_mode.txt"
printf '[General]\nFirstRun=true\n[Video]\nRenderer=OpenGL\n' > "$PF/obs/config/obs-studio/global.ini"
printf '[General]\nFirstRun=true\nConfirmOnExit=false\n[BasicWindow]\nSysTrayEnabled=false\nSysTrayWhenStarted=false\nSysTrayMinimizeToTray=false\n' \
  > "$PF/obs/config/obs-studio/user.ini"
cp "$S/reel-elevate.exe" "$PF/reel-elevate.exe"
look() { wine "$S/elevate_parts.exe" look 2>/dev/null | tr -d '\r' | cut -d' ' -f2-; }
helper() { wine "C:\\Program Files\\reel\\reel-elevate.exe" "$@" >/dev/null 2>&1; }
# The mean of the screen: OBS's dark UI over most of it, or Wine's blue desktop.
painted() { import -display :94 -window root "$W/$1.png"; convert "$W/$1.png" -resize 1x1 -format '%[fx:int(255*b)] %[fx:int(255*r)]' info:; }
wine "C:\\Program Files\\reel\\reel-elevate.exe" start >/dev/null 2>&1 &
# look says nothing until OBS has its main window, 15 s in a fresh prefix.
for _ in $(seq 1 120); do case "$(look)" in ""|none) sleep 1 ;; *) break ;; esac; done
sleep 5
[ "$(look)" = "hidden normal" ] || fail "OBS's main window at the start: $(look)"
EMPTY=$(painted empty)
ok "OBS $(basename "$ZIP" | cut -d- -f3) started by reel-elevate start, its main window hidden; the screen's mean blue and red: $EMPTY"
n=0
for step in show hide show; do
  [ "$step" = show ] && n=$((n + 1))
  helper window "$step"
  sleep 3
  case "$step" in
    show)
      [ "$(look)" = "shown normal" ] || fail "show $n: $(look)"
      set -- $(painted "show$n")
      [ "$1" -lt 80 ] && [ "$2" -lt 80 ] || fail "show $n: the screen is not OBS's dark UI (mean blue $1, red $2)"
      ok "show $n: on the screen, restored by OBS itself, painted (mean blue $1, red $2)" ;;
    hide)
      [ "$(look)" = "hidden minimized" ] || fail "hide: $(look)"
      ok "hide: minimized by OBS itself, then off the screen" ;;
  esac
done
helper window close
for _ in $(seq 1 30); do [ "$(look)" = none ] && break; sleep 0.5; done
[ "$(look)" = none ] || fail "close: $(look)"
ok "close: OBS closed as its close button closes it"
wineserver -k 2>/dev/null || true
