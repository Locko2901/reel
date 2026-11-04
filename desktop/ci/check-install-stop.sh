#!/bin/sh
# Before an installer touches the install folder, the stop helper ends the running app, then its
# OBS, and waits until OBS's library can be written; the uninstaller does the same.
set -eu
S=/work/desktop/.wine-standins
W=$(mktemp -d)
export HOME="$W/home" WINEPREFIX="$W/wine" WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
mkdir -p "$HOME"
fail() { printf 'FAIL %s\n' "$*"; pkill -fi reel-desktop.exe || true; pkill -fi reel-capture.exe || true; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
running() { pgrep -fi "$1" >/dev/null 2>&1; }
for v in 1 2; do
  t="$W/v$v"
  mkdir -p "$t/obs/bin/64bit" "$t/obs/data"
  cp "$S/standin_obs.exe" "$t/obs/bin/64bit/reel-capture.exe"
  echo "Qt of OBS $v" > "$t/obs/bin/64bit/Qt6Core.dll"
  echo "data of OBS $v" > "$t/obs/data/data-of-$v.txt"
  cp "$S/standin_app.exe" "$t/reel-desktop.exe"
  cat > "$W/v$v.nsi" <<NSI
Unicode true
!include LogicLib.nsh
Name "reel $v, the hooks with OBS"
OutFile "$W/v$v.exe"
RequestExecutionLevel user
InstallDir "\$LOCALAPPDATA\\reel"
SilentInstall silent
Var UpdateMode
!include "/work/desktop/app/nsis/with-obs.nsh"
Section
  !insertmacro NSIS_HOOK_PREINSTALL
  SetOutPath "\$INSTDIR"
  File /r "$t/obs"
  File "$t/reel-desktop.exe"
  WriteUninstaller "\$INSTDIR\\uninstall.exe"
  !insertmacro REEL_INSTALL_DONE
SectionEnd
Section Uninstall
  !insertmacro NSIS_HOOK_PREUNINSTALL
  Delete "\$INSTDIR\\reel-desktop.exe"
  RMDir /r "\$INSTDIR\\obs"
  !insertmacro REEL_INSTALL_DONE
SectionEnd
NSI
  makensis -V2 "$W/v$v.nsi" >/dev/null
done
ok "two installers from the hook files, the stop helper in both, the uninstaller's too"
for flavour in plain with-obs; do
  cat > "$W/$flavour.nsi" <<NSI
Unicode true
!include LogicLib.nsh
Name "reel, $flavour's hooks whole"
OutFile "$W/$flavour.exe"
RequestExecutionLevel user
InstallDir "\$LOCALAPPDATA\\reel"
Var UpdateMode
!include "/work/desktop/app/nsis/$flavour.nsh"
Section
  !insertmacro NSIS_HOOK_PREINSTALL
  !ifmacrodef NSIS_HOOK_POSTINSTALL
    !insertmacro NSIS_HOOK_POSTINSTALL
  !endif
  WriteUninstaller "\$INSTDIR\\uninstall.exe"
SectionEnd
Section Uninstall
  !insertmacro NSIS_HOOK_PREUNINSTALL
  !insertmacro NSIS_HOOK_POSTUNINSTALL
SectionEnd
NSI
  makensis -V2 "$W/$flavour.nsi" >/dev/null || fail "$flavour's hooks do not compile"
done
ok "both installers' hooks compile whole, the removal of the copy in Program Files among them"
wine wineboot --init >/dev/null 2>&1 || true
wine "$W/v1.exe" /S
L=$(wine cmd /c "echo %LOCALAPPDATA%" 2>/dev/null | tr -d '\r')
I="$(winepath -u "$L")/reel"
WI=$(winepath -w "$I" 2>/dev/null | tr -d '\r')
[ -f "$I/obs/bin/64bit/reel-capture.exe" ] && [ -f "$I/reel-desktop.exe" ] || fail "the first install is not in $I"
ok "the first install, in $WI"

start_reel() {
  (cd "$I" && wine "$I/reel-desktop.exe" >/dev/null 2>&1 &)
  for _ in $(seq 1 60); do running 'reel-capture.exe' && break; sleep 0.5; done
  running 'reel-capture.exe' || fail "the stand-in app did not start its OBS"
  sleep 1
  if wine cmd /c "echo x>> \"$WI\\obs\\bin\\64bit\\Qt6Core.dll\"" >/dev/null 2>&1 && grep -q '^x' "$I/obs/bin/64bit/Qt6Core.dll"; then
    fail "the stand-in OBS does not hold Qt6Core.dll"
  fi
}

start_reel
kill -9 $(pgrep -fi 'reel-capture.exe' | head -1)
for _ in $(seq 1 20); do running 'reel-capture.exe' && break; sleep 0.25; done
running 'reel-capture.exe' || fail "the stand-in app did not start its OBS again"
ok "reel runs with its OBS holding obs\\bin\\64bit\\Qt6Core.dll; OBS ended under it is started again, as the app does after a crash"

OUT=$(wine /work/desktop/app/nsis/reel-stop.exe "$WI" --app-wait 3 2>/dev/null | tr -d '\r') || fail "the helper: $OUT"
printf '%s\n' "$OUT" | sed 's/^/     /'
printf '%s\n' "$OUT" | grep -q "reel runs (pid .*): asked to quit" && printf '%s\n' "$OUT" | grep -q "reel did not quit in 3 s: ended" || fail "the app was not ended first"
sleep 3
! running 'reel-desktop.exe' && ! running 'reel-capture.exe' || fail "something of reel's runs again after the helper"
ok "the helper alone: the app ended first (it did not quit when asked), then its OBS by its verbatim path in capitals, and nothing started OBS again"

start_reel
wine "$W/v2.exe" /S
sleep 3
! running 'reel-desktop.exe' && ! running 'reel-capture.exe' || fail "the old app or its OBS runs after the install"
grep -q "Qt of OBS 2" "$I/obs/bin/64bit/Qt6Core.dll" || fail "Qt6Core.dll was not replaced: $(cat "$I/obs/bin/64bit/Qt6Core.dll")"
[ -f "$I/obs/data/data-of-2.txt" ] && [ ! -e "$I/obs/data/data-of-1.txt" ] || fail "obs\\ is not the new OBS's alone"
[ ! -e "$I/reel-installing" ] || fail "the installer's mark stayed"
ok "an install over a running reel: the app and its OBS ended in that order, Qt6Core.dll free and replaced, obs\\ the new OBS's alone, the mark gone"

start_reel
wine "$I/uninstall.exe" /S "_?=$WI"
sleep 3
! running 'reel-desktop.exe' && ! running 'reel-capture.exe' || fail "the app or its OBS runs after the uninstall"
[ ! -e "$I/obs" ] && [ ! -e "$I/reel-desktop.exe" ] && [ ! -e "$I/reel-installing" ] || fail "the uninstall left $(ls "$I")"
ok "the uninstaller over a running reel: the same, then obs\\ and the app gone"
