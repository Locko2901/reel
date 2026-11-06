#!/bin/sh
# A standalone app's update under Wine, the app's test built for Windows: found through reel on
# this PC, run as an update runs it; the installer then installs and starts reel again.
#   check-release-update.sh <folder> <version> [the .pub of the key reel light was signed with]
#     reel light of that version from a stand-in release, refused once forged
#   check-release-update.sh github
#     reel light of GitHub's newest release, against the release key
set -eu
. /work/desktop/ci/packages.sh
RELEASE_KEY=$(sed -n 's/^ *"pubkey": *"\([^"]*\)".*/\1/p' /work/desktop/app/tauri.conf.json)
if [ "$1" = github ]; then
  EXE="" KEY="$RELEASE_KEY"
else
  EXE="$1/${PLAIN_FILE}_$2_x64-setup.exe" PUB="${3:-}"
  [ -f "$EXE" ] || { echo "FAIL $EXE is not there"; exit 1; }
  [ -f "$EXE.sig" ] || { echo "FAIL $EXE.sig is not there: desktop-windows signs only with TAURI_SIGNING_PRIVATE_KEY"; exit 1; }
  if [ -n "$PUB" ]; then KEY=$(cat "$PUB"); else KEY="$RELEASE_KEY"; fi
fi
TEST=$(ls -t /target/x86_64-pc-windows-msvc/debug/deps/reel_desktop_lib-*.exe 2>/dev/null | head -1)
[ -n "$TEST" ] || { echo "FAIL no test executable: cargo xwin test --no-run -p reel-desktop --lib builds it"; exit 1; }
W=$(mktemp -d)
export HOME="$W/home" WINEDEBUG=-all DISPLAY=:96 WINEDLLOVERRIDES="mscoree,mshtml="
mkdir -p "$HOME"
fail() { printf 'FAIL %s\n' "$*"; wineserver -k 2>/dev/null || true; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
Xvfb :96 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1
export WINEPREFIX="$W/prefix"
wineboot -i >/dev/null 2>&1
wine winecfg -v win10 >/dev/null 2>&1
wine reg add 'HKCU\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' /v pv /d 130.0.2849.80 /f >/dev/null 2>&1
wineserver -w
L=$(wine cmd /c "echo %LOCALAPPDATA%" 2>/dev/null | tr -d '\r')
T=$(wine cmd /c "echo %TEMP%" 2>/dev/null | tr -d '\r')
I="$(winepath -u "$L" 2>/dev/null)/reel"
TMP="$(winepath -u "$T" 2>/dev/null)"
UPDATE_PUBKEY="$KEY"
export UPDATE_PUBKEY
if [ -n "$EXE" ]; then
  cp "$EXE" "$EXE.sig" "$W/"
  STANDIN_INSTALLER=$(winepath -w "$W/$(basename "$EXE")" 2>/dev/null | tr -d '\r')
  STANDIN_VERSION="$2"
  export STANDIN_INSTALLER STANDIN_VERSION
fi

timeout 600 wine "$TEST" --ignored --exact --nocapture \
  updates::tests::a_standalone_app_on_windows_runs_the_releases_installer > "$W/test.out" 2>&1 \
  || fail "the test: $(tail -30 "$W/test.out")"
grep '^ok   ' "$W/test.out" || true
grep -q 'its signature checked' "$W/test.out" || fail "the test ended before the install: $(tail -30 "$W/test.out")"
VERSION=$(sed -n 's/^ok   reel \([^ ]*\) found on the release.*/\1/p' "$W/test.out" | head -1)
ok "the test ended with the updater, as it does once the installer starts"

seen=""
for _ in $(seq 1 120); do
  if xdotool search --onlyvisible --name '^reel' >/dev/null 2>&1; then seen=1; break; fi
  grep -q 'the installer ends' "$I/logs/installer.log" 2>/dev/null && break
  sleep 0.5
done
for _ in $(seq 1 600); do grep -q 'the installer ends' "$I/logs/installer.log" 2>/dev/null && break; sleep 1; done
grep -q 'the installer ends' "$I/logs/installer.log" 2>/dev/null || fail "the installer did not finish: $(cat "$I/logs/installer.log" 2>/dev/null)"
[ -n "$seen" ] && ok "the installer's small window was shown" || printf 'note the installer ended before its window was seen\n'
grep -q -- '/P /UPDATE /NCRC /R /ARGS --hidden' "$I/logs/installer.log" || fail "the installer did not run with the update's flags: $(head -1 "$I/logs/installer.log")"
grep -q 'reel starts again' "$I/logs/installer.log" || fail "the installer did not start reel again: $(cat "$I/logs/installer.log")"
[ -f "$I/reel-desktop.exe" ] || fail "no reel-desktop.exe in $I"
shown=$(wine reg query 'HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall' /s 2>/dev/null | tr -d '\r' | sed -n 's/.*DisplayVersion *REG_SZ *//p' | head -1)
[ "$shown" = "$VERSION" ] || fail "Apps shows reel $shown, not $VERSION"
ok "reel $VERSION installed by the update (/P /UPDATE /NCRC /R /ARGS --hidden), Apps says $shown, and reel started again: $(sed -n '1p' "$I/logs/installer.log" | cut -c1-60)..."
left=$(ls -d "$TMP"/test-*-updater-* 2>/dev/null | head -1)
[ -n "$left" ] && ok "the updater left the installer in $(basename "$left") in the temporary folder, which the next start of reel removes" || true
wineserver -k 2>/dev/null || true
wineserver -w 2>/dev/null || true
echo "the standalone update from the release: all checks passed"
