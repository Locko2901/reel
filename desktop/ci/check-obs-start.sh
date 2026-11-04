#!/bin/sh
# The stand-in icuuc.dll goes in system32, where an update leaves it (see check-obs-window.sh).
set -eu
IN="$1" VERSION="${2:-}" PREVIOUS="${3:-}"
[ -n "$VERSION" ] || VERSION=$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' /work/desktop/app/tauri.conf.json | head -1)
. /work/desktop/ci/packages.sh
WITH="$IN/${WITH_OBS_FILE}_${VERSION}_x64-setup.exe"
OBS=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' /work/desktop/obs/pinned.json | head -1)
W=$(mktemp -d) I=""
export HOME="$W/home" WINEDEBUG=-all DISPLAY=:95 WINEDLLOVERRIDES="mscoree,mshtml="
mkdir -p "$HOME"
fail() {
  printf 'FAIL %s\n' "$*"
  log | cut -c1-300 | tail -15
  f=$(ls -t "$I"/obs/config/obs-studio/logs/*.txt 2>/dev/null | head -1)
  [ -n "$f" ] && tail -15 "$f" | cut -c1-200
  wineserver -k 2>/dev/null || true
  exit 1
}
ok() { printf 'ok   %s\n' "$*"; }
[ -f "$WITH" ] || fail "$(basename "$WITH") is not there"
Xvfb :95 -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1
clang --target=x86_64-pc-windows-msvc -O1 -nostdlib -ffreestanding -shared -fuse-ld=lld -Wl,/noentry \
  -o "$W/icuuc.dll" /work/desktop/ci/icuuc-stub.c
prefix() { # a Windows 10 of its own, WebView2 there as far as the installers look
  export WINEPREFIX="$W/$1"
  wineboot -i >/dev/null 2>&1
  wine winecfg -v win10 >/dev/null 2>&1
  wine reg add 'HKCU\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' /v pv /d 130.0.2849.80 /f >/dev/null 2>&1
  wineserver -w
  cp "$W/icuuc.dll" "$WINEPREFIX/drive_c/windows/system32/icuuc.dll"
  L=$(wine cmd /c "echo %LOCALAPPDATA%" 2>/dev/null | tr -d '\r')
  R=$(wine cmd /c "echo %APPDATA%" 2>/dev/null | tr -d '\r')
  I="$(winepath -u "$L" 2>/dev/null)/reel"
  SET="$(winepath -u "$R" 2>/dev/null)/reel"
  mkdir -p "$WINEPREFIX/drive_c/Clips" "$SET"
}
inst() { # inst <installer> [switches]: as the updater and a switch run it, waited for to its end
  timeout 900 wine "$@" >/dev/null 2>&1 || fail "$(basename "$1") ${2:-}: exit $?"
  for _ in $(seq 1 300); do pgrep -f "$(basename "$1")" >/dev/null || break; sleep 1; done
}
for_wine() { # each OBS an installer laid down, as check-obs-window.sh runs it
  perl -pi -e 's/wine_get_version/wine_get_versioX/g' "$I/obs/bin/64bit/reel-capture.exe"
  mkdir -p "$I/obs/config/obs-studio"
  grep -qs 'Renderer=OpenGL' "$I/obs/config/obs-studio/global.ini" || printf '[Video]\nRenderer=OpenGL\n' >> "$I/obs/config/obs-studio/global.ini"
}
settings() { # settings <true|false>: the setup done, Capture games that run as administrator as given
  printf '{"server_url":"http://127.0.0.1:9","clips_dir":"C:\\\\Clips","obs":{"bundled":true,"setup":{"done":true,"elevated":%s}},"startup":{"autostart":false},"log_level":"info"}' "$1" > "$SET/settings.json"
}
log() { cat "$I"/logs/* 2>/dev/null; }
start_app() { (cd "$I" && wine "$I/reel-desktop.exe" --hidden >/dev/null 2>&1 &); }
connected() { # connected <OBS version> <seconds>
  for _ in $(seq 1 "$2"); do
    log | grep -q "obs connected obs_version=\"$1\"" && return 0
    sleep 1
  done
  return 1
}
replay() { grep -qs 'Replay Buffer Start' "$I"/obs/config/obs-studio/logs/*.txt; }
quit() { wineserver -k 2>/dev/null || true; sleep 1; }

prefix fresh
inst "$WITH" /S
[ -f "$I/obs/bin/64bit/reel-capture.exe" ] && [ -f "$I/obs/core/obs-websocket/obs-websocket.dll" ] && [ -f "$I/obs/portable_mode.txt" ] || fail "$WITH_OBS_NAME laid down no OBS where the app looks: $(ls "$I/obs" 2>/dev/null | tr '\n' ' ')"
for_wine
settings false
start_app
connected "$OBS" 90 || fail "a fresh install: the app did not connect to its OBS $OBS"
log | grep -q 'the bundled OBS is here' || fail "the app did not say it found its OBS"
replay || fail "a fresh install: OBS's replay buffer did not start"
PW=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["server_password"])' "$I/obs/config/obs-studio/plugin_config/obs-websocket/config.json")
sh /work/desktop/ci/check-obs-requests.sh 4466 "$PW" || fail "the protocol of the bundled OBS"
ok "a fresh install of $WITH_OBS_NAME $VERSION: OBS $OBS in obs\\, found and started by the app, its WebSocket answering, the replay buffer on"
quit

rm -f "$I"/logs/*
settings true
start_app
connected "$OBS" 90 || fail "Capture games that run as administrator on, no copy: the app did not connect to its OBS"
log | grep -q 'line=running, hidden, without the highest rights' || fail "the app did not say OBS runs without the highest rights"
! log | grep -q 'OBS did not start' || fail "the app said OBS did not start: $(log | grep -m1 'OBS did not start' | cut -c1-200)"
ok "Capture games that run as administrator on, no copy in Program Files and no task: OBS started without the rights, the app saying why ($(log | grep -m1 -o 'the copy of OBS in Program Files is not there[^"]*' | head -1))"
quit
rm -rf "$W/fresh"

OLD=$(ls "${PREVIOUS:-/nonexistent}"/with-obs_*_x64-setup.exe 2>/dev/null | head -1 || true)
if [ -z "$OLD" ]; then
  ok "the update over the previous release: skipped, no previous installer given"
  exit 0
fi
FROM=$(basename "$OLD" | sed 's/^with-obs_\(.*\)_x64-setup\.exe$/\1/')
prefix update
inst "$OLD" /S
for_wine
settings false
start_app
for _ in $(seq 1 90); do log | grep -q 'obs connected' && break; sleep 1; done
WAS=$(log | sed -n 's/.*obs connected obs_version="\([^"]*\)".*/\1/p' | head -1)
[ -n "$WAS" ] || fail "the release $FROM with OBS did not connect to its OBS"
[ -f "$I/obs/config/obs-studio/basic/profiles/reel/basic.ini" ] || fail "the release $FROM with OBS wrote no profile for its OBS"
printf '\n[reel-check]\nKept=yes\n' >> "$I/obs/config/obs-studio/basic/profiles/reel/basic.ini"
inst "$WITH" /P /UPDATE /NCRC /R /ARGS --hidden
[ -f "$I/obs/bin/64bit/reel-capture.exe" ] && [ -d "$I/obs/core" ] || fail "the update laid down no OBS: $(ls "$I/obs" | tr '\n' ' ')"
grep -q 'Kept=yes' "$I/obs/config/obs-studio/basic/profiles/reel/basic.ini" || fail "the update did not keep OBS's settings"
[ "$(sha256sum "$I/reel-desktop.exe" | cut -d' ' -f1)" = "$(7z e -so "$WITH" reel-desktop.exe 2>/dev/null | sha256sum | cut -d' ' -f1)" ] || fail "the update did not put this version's app in place"
for _ in $(seq 1 30); do [ "$(log | grep -c 'reel desktop starts')" -ge 2 ] && break; sleep 1; done
[ "$(log | grep -c 'reel desktop starts')" -ge 2 ] || fail "the app did not start again after the update"
for_wine
pkill -f 'reel-capture.exe' 2>/dev/null || true
connected "$OBS" 120 || fail "after the update: the app did not connect to its OBS $OBS"
ok "the release $FROM with OBS (OBS $WAS) running, then $WITH_OBS_NAME $VERSION as its updater runs it (/P /UPDATE /NCRC /R /ARGS --hidden): the app back by itself at $VERSION, OBS's settings kept, OBS $OBS started and answering (OBS's first start after the update ended here to skip its Wine warning)"
quit
