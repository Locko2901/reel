#!/bin/sh
set -eu
W=$(mktemp -d)
export HOME="$W/home" WINEPREFIX="$W/wine" WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
mkdir -p "$HOME"
fail() { echo "FAIL $*"; exit 1; }
for v in 1 2; do
  t="$W/v$v/obs"
  mkdir -p "$t/bin/64bit" "$t/obs-plugins/64bit" "$t/data/obs-studio"
  if [ "$v" = 1 ]; then exe=obs64.exe; else exe=reel-capture.exe; fi
  echo "OBS $v" > "$t/bin/64bit/$exe"
  echo "a plugin of OBS $v" > "$t/obs-plugins/64bit/plugin-of-$v.dll"
  echo "data of OBS $v" > "$t/data/obs-studio/data-of-$v.txt"
  : > "$t/portable_mode.txt"
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
  File /r "$W/v$v/obs"
SectionEnd
NSI
  makensis -V2 "$W/v$v.nsi" >/dev/null
done
wine wineboot --init >/dev/null 2>&1 || true
wine "$W/v1.exe" /S
L=$(wine cmd /c "echo %LOCALAPPDATA%" 2>/dev/null | tr -d '\r')
I="$(winepath -u "$L")/reel/obs"
[ -f "$I/bin/64bit/obs64.exe" ] || fail "the first install put no OBS in $I"
echo "ok   the first install: OBS 1 in $I"
mkdir -p "$I/config/obs-studio/basic/profiles/reel" "$I/config/obs-studio/logs"
echo "[General]" > "$I/config/obs-studio/basic/profiles/reel/basic.ini"
echo "a log" > "$I/config/obs-studio/logs/2026-10-11.txt"
wine "$W/v2.exe" /S
grep -q "OBS 2" "$I/bin/64bit/reel-capture.exe" || fail "OBS 2 is not in place"
[ ! -e "$I/bin/64bit/obs64.exe" ] || fail "obs64.exe of an older version's install stayed"
[ -f "$I/obs-plugins/64bit/plugin-of-2.dll" ] && [ -f "$I/data/obs-studio/data-of-2.txt" ] && [ -f "$I/portable_mode.txt" ] || fail "OBS 2's files are not all there"
[ ! -e "$I/obs-plugins/64bit/plugin-of-1.dll" ] || fail "OBS 1's plugin stayed"
[ ! -e "$I/data/obs-studio/data-of-1.txt" ] || fail "OBS 1's data stayed"
echo "ok   the update: OBS 2 in place as reel-capture.exe, nothing of OBS 1 left (its obs64.exe, its plugin and its data gone)"
[ -f "$I/config/obs-studio/basic/profiles/reel/basic.ini" ] && [ -f "$I/config/obs-studio/logs/2026-10-11.txt" ] || fail 'obs\config did not survive'
printf '%s\n' 'ok   obs\config survived, OBS'"'"'s settings and logs as they were'
