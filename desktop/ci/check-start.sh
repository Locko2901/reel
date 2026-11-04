#!/bin/sh
# The app of the package with OBS under Wine with Capture games as administrator on and a copy
# without a second task. Wine has no WebView2, so the start is checked up to the window being made.
set -eu
. /work/desktop/ci/packages.sh
INST="$1/${WITH_OBS_FILE}_$2_x64-setup.exe"
S=/work/desktop/.wine-standins
W=$(mktemp -d)
export HOME="$W/home" WINEPREFIX="$W/pfx" WINEDEBUG=-all DISPLAY=:96 WINEDLLOVERRIDES="mscoree,mshtml="
mkdir -p "$HOME"
fail() { printf 'FAIL %s\n' "$*"; cat "$W"/d/data/logs/* 2>/dev/null | cut -c1-300 | tail -15; tail -5 "$W/app.out" 2>/dev/null; wineserver -k 2>/dev/null || true; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
[ -f "$INST" ] || fail "$(basename "$INST") is not there"
Xvfb :96 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1
wine wineboot --init >/dev/null 2>&1 || true
A="$WINEPREFIX/drive_c/reel-app"
PF="$WINEPREFIX/drive_c/Program Files/reel"
mkdir -p "$A/obs/bin/64bit" "$W/x" "$PF/obs/bin/64bit" "$PF/obs/config" "$W/d/config" "$WINEPREFIX/drive_c/Clips"
7z x -y -o"$W/x" "$INST" reel-desktop.exe >/dev/null || fail "no reel-desktop.exe in $(basename "$INST")"
cp "$(find "$W/x" -name reel-desktop.exe | head -1)" "$A/"
cp "$S/standin_obs.exe" "$A/obs/bin/64bit/reel-capture.exe"
cp "$S/standin_obs.exe" "$PF/obs/bin/64bit/reel-capture.exe"
cp "$S/reel-elevate.exe" "$PF/reel-elevate.exe"
printf '{"server_url":"","clips_dir":"C:\\\\Clips","obs":{"bundled":true,"setup":{"done":true,"elevated":true}},"startup":{"autostart":false},"log_level":"info"}' \
  > "$W/d/config/settings.json"
REEL_DESKTOP_DIR="Z:$(echo "$W/d" | tr / '\\')" wine 'C:\reel-app\reel-desktop.exe' > "$W/app.out" 2>&1 &
log() { cat "$W"/d/data/logs/* 2>/dev/null; }
made() { xwininfo -root -tree 2>/dev/null | grep -q '"reel": ("reel-desktop.exe"'; }
for i in $(seq 1 60); do made && break; [ "$i" -gt 10 ] && ! pgrep -f 'reel-desktop.exe' >/dev/null && break; sleep 1; done
! grep -q 'RPC_E_CHANGED_MODE' "$W/app.out" || fail "the window's OleInitialize answered RPC_E_CHANGED_MODE: $(grep -m1 'panicked' "$W/app.out")"
log | grep -q 'start: the tray icon and the app are up' || fail "the start did not reach the tray icon and the app"
made || fail "the app made no window"
pgrep -f 'reel-desktop.exe' >/dev/null || fail "the app is not running"
ok "with an older version's copy, its rights on: the tray icon and the app up at $(log | grep -m1 'start: the tray icon and the app are up' | sed -n 's/.* at=\([0-9.]* s\).*/\1/p'), the window made past its OleInitialize, the app running (the app of 772b1a3 died there: RPC_E_CHANGED_MODE); its webview is Windows' to show, Wine has no WebView2"
wineserver -k 2>/dev/null || true
