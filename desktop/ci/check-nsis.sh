#!/bin/sh
# The real NSIS installers under Wine, silently as the updater and a switch run them.
#   check-nsis.sh <folder with the installers> [version, the app's own when empty]
set -eu
IN="$1" VERSION="${2:-}"
[ -n "$VERSION" ] || VERSION=$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' /work/desktop/app/tauri.conf.json | head -1)
. /work/desktop/ci/packages.sh
PLAIN="$IN/${PLAIN_FILE}_${VERSION}_x64-setup.exe"
WITH="$IN/${WITH_OBS_FILE}_${VERSION}_x64-setup.exe"
P=$PLAIN_NAME F=$WITH_OBS_NAME
S=/work/desktop/.wine-standins
W=$(mktemp -d)
export HOME="$W/home" WINEDEBUG=-all DISPLAY=:97 WINEDLLOVERRIDES="mscoree,mshtml="
mkdir -p "$HOME"
fail() { printf 'FAIL %s\n' "$*"; wineserver -k 2>/dev/null || true; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
running() { pgrep -fi "$1" >/dev/null 2>&1; }
[ -f "$WITH" ] || fail "$(basename "$WITH") is not there"
skip() { printf 'skip %s\n' "$*"; }
HAVE_PLAIN=""
[ -f "$PLAIN" ] && HAVE_PLAIN=1
Xvfb :97 -screen 0 1024x768x24 -nolisten tcp >/dev/null 2>&1 &
sleep 1
prefix() { # a Windows 10 of its own, WebView2 there as far as the installers look
  export WINEPREFIX="$W/$1"
  wineboot -i >/dev/null 2>&1
  wine winecfg -v win10 >/dev/null 2>&1
  wine reg add 'HKCU\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' /v pv /d 130.0.2849.80 /f >/dev/null 2>&1
  wineserver -w
  L=$(wine cmd /c "echo %LOCALAPPDATA%" 2>/dev/null | tr -d '\r')
  R=$(wine cmd /c "echo %APPDATA%" 2>/dev/null | tr -d '\r')
  I="$(winepath -u "$L" 2>/dev/null)/reel"
  WI=$(winepath -w "$I" 2>/dev/null | tr -d '\r')
  SET="$(winepath -u "$R" 2>/dev/null)/reel"
  MENU="$(winepath -u "$R" 2>/dev/null)/Microsoft/Windows/Start Menu/Programs/reel.lnk"
}
inst() { # inst <installer> [switches]: silently, as the updater and a switch run it
  f=$1
  shift
  timeout 600 wine "$f" /S "$@" || fail "$(basename "$f") $*: exit $?"
  wineserver -w
  [ -f "$I/reel-desktop.exe" ] || fail "$(basename "$f") $*: no reel-desktop.exe in $I"
}
key() { wine reg query "$1" >/dev/null 2>&1; }
UNINST='HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall'
entries() { wine reg query "$UNINST" /s 2>/dev/null | tr -d '\r' | grep -c 'DisplayName.*REG_SZ *reel$' || true; }
uninstall() { # the uninstaller, silently, in place, waited for to its end
  timeout 600 wine "$I/uninstall.exe" /S "_?=$WI" >/dev/null 2>&1 || true
  wineserver -w
}
SCHEME='HKCU\Software\Classes\reel-desktop'
scheme() { # the app's links go to this install's reel-desktop.exe
  wine reg query "$SCHEME" /v "URL Protocol" >/dev/null 2>&1 || return 1
  cmd=$(wine reg query "$SCHEME\\shell\\open\\command" /ve 2>/dev/null | tr -d '\r' | sed -n 's/.*REG_SZ *//p')
  [ "$cmd" = "\"$WI\\reel-desktop.exe\" \"%1\"" ]
}
app_files() { for f in reel-desktop.exe ffmpeg.exe ffprobe.exe binaries/ffmpeg-LICENSE.txt reel-elevate.exe uninstall.exe; do [ -f "$I/$f" ] || return 1; done; }
# update <installer> <title> <what> <flags...>: as an app runs its update, over the running
# stand-in: the small window of that title shown, reel started again, the flags in the log
update() {
  exe=$1 title=$2 what=$3
  shift 3
  rm -f "$I/logs/installer.log"
  timeout 600 wine "$exe" "$@" >/dev/null 2>&1 &
  seen=""
  for _ in $(seq 1 120); do
    if xdotool search --onlyvisible --name "^$title ?\$" >/dev/null 2>&1; then seen=1; break; fi
    sleep 0.5
  done
  [ -n "$seen" ] || fail "$what ($*) showed no window \"$title\""
  for _ in $(seq 1 600); do grep -q 'the installer ends' "$I/logs/installer.log" 2>/dev/null && break; sleep 1; done
  grep -q 'reel starts again' "$I/logs/installer.log" || fail "the installer of $what did not start reel again: $(cat "$I/logs/installer.log")"
  grep -q -- "$*" "$I/logs/installer.log" || fail "logs\\installer.log does not name the flags it ran with ($*)"
  wineserver -k 2>/dev/null || true
  wineserver -w 2>/dev/null || true
}

prefix one
if [ -n "$HAVE_PLAIN" ]; then
inst "$PLAIN"
key "$UNINST\\reel" && app_files && [ -f "$MENU" ] && [ ! -e "$I/obs" ] && [ "$(entries)" = 1 ] || fail "$P is not all there: $(ls "$I")"
! running 'reel-desktop.exe' || fail "a silent install started reel"
scheme || fail "$P did not register its links: $(wine reg query "$SCHEME" /s 2>/dev/null | tr -d '\r' | head -8)"
ok "$P: the app, ffmpeg and ffprobe with ffmpeg's licence, reel-elevate.exe, the uninstaller, the Start menu's shortcut reel, one reel in Apps, its links (reel-desktop://) to its reel-desktop.exe; no OBS; reel not started"

# Wine's HKCR leaves out the user's own classes, so ShellExecute finds no handler: the check
# runs the registered command as Windows does, %1 the link.
WD="$W/app-dir"
mkdir -p "$WD/config"
printf '{"server_url":"http://127.0.0.1:9","obs":{"enabled":false},"startup":{"autostart":false},"log_level":"info"}' > "$WD/config/settings.json"
export REEL_DESKTOP_DIR="$(winepath -w "$WD" 2>/dev/null | tr -d '\r')"
applog() { cat "$WD"/data/logs/reel-desktop.log* 2>/dev/null; }
open_link() {
  exe=$(wine reg query "$SCHEME\\shell\\open\\command" /ve 2>/dev/null | tr -d '\r' | sed -n 's/.*REG_SZ *"\([^"]*\)" "%1"$/\1/p')
  [ -n "$exe" ] || fail "no command registered for the links"
  (cd "$I" && wine "$exe" "$1" >/dev/null 2>&1 &)
}
open_link 'reel-desktop://copy/01HZZZZZZZZZZZZZZZZZZZZZZZ'
for _ in $(seq 1 60); do applog | grep -q "a link to a clip this app does not know" && break; sleep 0.5; done
applog | grep -q "a link to a clip this app does not know: ignored link=reel-desktop://copy/01HZZZZZZZZZZZZZZZZZZZZZZZ" || fail "a link opened through Windows did not reach a first start of the app: $(applog | tail -5)"
running 'reel-desktop.exe' || fail "the app did not stay running after a link"
open_link 'reel-desktop://delete/01HZZZZZZZZZZZZZZZZZZZZZZZ'
for _ in $(seq 1 40); do applog | grep -q "not one of the app's: ignored" && break; sleep 0.5; done
applog | grep -q "a second start handed its arguments over" && applog | grep -q "not one of the app's: ignored arg=reel-desktop://delete/" || fail "a second link did not reach the running app: $(applog | tail -5)"
[ "$(pgrep -fic 'reel-desktop.exe')" -le 2 ] || fail "a second link left a second app running"
ok "a link through the registered command: the app started with it, hidden, and weighed it (a clip it does not know); a second one went to the running app, which ignored a verb it does not have"
wineserver -k 2>/dev/null || true
sleep 1
unset REEL_DESKTOP_DIR

# an app of 1.10 without OBS updating, with the flags it passes: the window of the package
# without OBS, reel started again
cp "$S/standin_app.exe" "$I/reel-desktop.exe"
(cd "$I" && wine "$I/reel-desktop.exe" >/dev/null 2>&1 &)
for _ in $(seq 1 60); do running 'reel-desktop.exe' && break; sleep 0.5; done
update "$PLAIN" "$P" "an update as an app of 1.10 without OBS runs it" /P /UPDATE /NCRC /R /ARGS --hidden
[ ! -e "$I/obs" ] && app_files && [ "$(entries)" = 1 ] && ! cmp -s "$I/reel-desktop.exe" "$S/standin_app.exe" || fail "the update of $P did not put its app in place, or brought an OBS"
ok "an update as an app of 1.10 without OBS runs it (/P /UPDATE /NCRC /R /ARGS --hidden): the small window \"$P\", the app replaced, still no OBS, reel started again with --hidden"
else
skip "$P: its installer was not built in this run (the one with OBS alone)"
fi

inst "$WITH"
ZIP=$(/work/desktop/ci/fetch-obs.sh zip - 2>/dev/null) || fail "the pinned OBS zip is not in the cache (make desktop-installers-check fetches it)"
want=$(unzip -Z1 "$ZIP" | grep -v '/$' | sed 's|^bin/64bit/obs64\.exe$|bin/64bit/reel-capture.exe|' | sort)
have=$(cd "$I/obs" && find . -type f | sed 's|^\./||' | grep -v '^portable_mode.txt$' | grep -v '^reel-obs.txt$' | sort)
printf '%s\n' "$want" > "$W/want"
printf '%s\n' "$have" > "$W/have"
cmp -s "$W/want" "$W/have" || fail "obs\\ is not the zip's files, portable_mode.txt and reel-obs.txt: $(diff "$W/want" "$W/have" | head -5)"
grep -q "^OBS Studio $(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' /work/desktop/obs/pinned.json | head -1) " "$I/obs/reel-obs.txt" || fail "obs\\reel-obs.txt does not name the OBS it holds: $(cat "$I/obs/reel-obs.txt" 2>/dev/null)"
a=$(unzip -p "$ZIP" bin/64bit/obs64.exe | sha256sum | cut -d' ' -f1)
b=$(sha256sum "$I/obs/bin/64bit/reel-capture.exe" | cut -d' ' -f1)
[ "$a" = "$b" ] && [ -f "$I/obs/portable_mode.txt" ] && [ -f "$I/obs/data/obs-studio/license/gplv2.txt" ] && app_files || fail "$F is not all there"
[ "$(entries)" = 1 ] || fail "Apps shows $(entries) reel"
scheme || fail "$F did not register its links"
ok "$F, with OBS${HAVE_PLAIN:+, over $P}: its links registered; every one of the zip's $(printf '%s\n' "$want" | wc -l) files under obs\\, portable_mode.txt, nothing else; obs64.exe as reel-capture.exe, byte for byte ($a); the licence; one reel in Apps"

start_reel() { # the stand-in app, as reel with its OBS holding obs\bin\64bit\Qt6Core.dll
  (cd "$I" && wine "$I/reel-desktop.exe" >/dev/null 2>&1 &)
  for _ in $(seq 1 60); do running 'reel-capture.exe' && break; sleep 0.5; done
  running 'reel-capture.exe' || fail "the stand-in app did not start its OBS"
  sleep 1
}
cp "$S/standin_app.exe" "$I/reel-desktop.exe"
cp "$S/standin_obs.exe" "$I/obs/bin/64bit/reel-capture.exe"
# a plugin of an older OBS, in its folder (OBS 33 has none: its plugins are under core\)
mkdir -p "$I/obs/obs-plugins/64bit"
printf 'a plugin of an older OBS' > "$I/obs/obs-plugins/64bit/old-plugin.dll"
mkdir -p "$I/obs/config/obs-studio"
printf 'set in OBS' > "$I/obs/config/obs-studio/kept.ini"
wine reg delete "$SCHEME" /f >/dev/null 2>&1 || true
start_reel
inst "$WITH" /UPDATE
sleep 2
! running 'reel-desktop.exe' && ! running 'reel-capture.exe' || fail "the stand-in app or its OBS runs after the update"
! cmp -s "$I/reel-desktop.exe" "$S/standin_app.exe" || fail "the app was not replaced"
cmp -s "$I/obs/bin/64bit/reel-capture.exe" "$S/standin_obs.exe" && [ -e "$I/obs/obs-plugins/64bit/old-plugin.dll" ] || fail "an update with the OBS the folder holds touched obs\\"
grep -q 'stays as it is' "$I/logs/installer.log" || fail "logs\\installer.log does not say OBS stayed: $(cat "$I/logs/installer.log")"
[ "$(entries)" = 1 ] && [ ! -e "$I/reel-installing" ] || fail "Apps shows $(entries) reel after the update, or the mark stayed"
scheme || fail "the update did not register the links"
ok "an update over a running reel without links (as 1.6.0 and before), run as the updater runs it (/S /UPDATE), with the OBS obs\\reel-obs.txt names: the app and its OBS ended first, the app replaced, OBS's files left as they are, the mark gone, the links registered"

# the same update over an OBS of another version, as reel-obs.txt says: obs\ fresh
printf 'OBS Studio 32.0.0 0000' > "$I/obs/reel-obs.txt"
cp "$S/standin_app.exe" "$I/reel-desktop.exe"
start_reel
inst "$WITH" /UPDATE
sleep 2
! running 'reel-desktop.exe' && ! running 'reel-capture.exe' || fail "the stand-in app or its OBS runs after the update"
! cmp -s "$I/reel-desktop.exe" "$S/standin_app.exe" && [ "$(sha256sum "$I/obs/bin/64bit/reel-capture.exe" | cut -d' ' -f1)" = "$b" ] || fail "the app or OBS was not replaced"
[ ! -e "$I/obs/obs-plugins/64bit/old-plugin.dll" ] && grep -q 'set in OBS' "$I/obs/config/obs-studio/kept.ini" && [ ! -e "$I/reel-installing" ] || fail "obs\\ is not fresh with its settings kept, or the mark stayed"
grep -q '^OBS Studio' "$I/obs/reel-obs.txt" && ! grep -q '32.0.0' "$I/obs/reel-obs.txt" || fail "obs\\reel-obs.txt was not written again"
ok "the same update over another OBS: both replaced, the older OBS's plugin gone, obs\\config kept, reel-obs.txt naming the new OBS"

# an app of 1.9 updating, with the flags Tauri's updater passed in quiet mode: the small
# window all the same, and reel started again with its arguments
cp "$S/standin_app.exe" "$I/reel-desktop.exe"
cp "$S/standin_obs.exe" "$I/obs/bin/64bit/reel-capture.exe"
start_reel
update "$WITH" "$F" "an update as an app of 1.9 runs it" /S /UPDATE /R /ARGS --hidden
ok "an update as an app of 1.9 with OBS runs it (/S /UPDATE /R /ARGS --hidden): its small window \"$F\" shown, reel started again with --hidden, every step in logs\\installer.log"

# an app of 1.10 with OBS, with the flags it passes
cp "$S/standin_app.exe" "$I/reel-desktop.exe"
cp "$S/standin_obs.exe" "$I/obs/bin/64bit/reel-capture.exe"
start_reel
update "$WITH" "$F" "an update as an app of 1.10 with OBS runs it" /P /UPDATE /NCRC /R /ARGS --hidden
[ -f "$I/obs/bin/64bit/reel-capture.exe" ] && grep -q 'set in OBS' "$I/obs/config/obs-studio/kept.ini" && [ "$(entries)" = 1 ] || fail "the update of $F lost its OBS or OBS's settings"
ok "an update as an app of 1.10 with OBS runs it (/P /UPDATE /NCRC /R /ARGS --hidden): the small window \"$F\", OBS and its settings kept, reel started again with --hidden"

if [ -n "$HAVE_PLAIN" ]; then
cp "$S/standin_app.exe" "$I/reel-desktop.exe"
cp "$S/standin_obs.exe" "$I/obs/bin/64bit/reel-capture.exe"
start_reel
set +e
timeout 600 wine "$PLAIN" /S >/dev/null 2>&1
code=$?
set -e
for _ in $(seq 1 60); do running "$(basename "$PLAIN")" || break; sleep 1; done
[ "$code" != 0 ] && [ -f "$I/obs/bin/64bit/reel-capture.exe" ] && grep -q 'set in OBS' "$I/obs/config/obs-studio/kept.ini" || fail "$P silently over $F without /WITHOUTOBS: exit $code, $(ls "$I/obs" 2>/dev/null | tr '\n' ' ')"
running 'reel-desktop.exe' && running 'reel-capture.exe' && cmp -s "$I/reel-desktop.exe" "$S/standin_app.exe" || fail "the refused install stopped or replaced reel"
ok "$P silently over $F without /WITHOUTOBS: refused (exit $code) before anything was touched; reel and its OBS still running, OBS and its settings kept"
wineserver -k 2>/dev/null || true
sleep 1

inst "$PLAIN" /WITHOUTOBS
[ ! -e "$I/obs" ] && app_files && [ "$(entries)" = 1 ] || fail "$P over $F left obs\\: $(ls "$I/obs" 2>/dev/null | head -3)"
scheme || fail "$P over $F left no links"
ok "$P over $F with /WITHOUTOBS: the bundled OBS gone, its settings with it; one reel in Apps; its links registered"
else
skip "$P over $F, refused silently and with /WITHOUTOBS: its installer was not built in this run"
fi

wine reg add 'HKCU\Software\Microsoft\Windows\CurrentVersion\Run' /v reel /d "$WI\\reel-desktop.exe" /f >/dev/null 2>&1
C="$(winepath -u 'C:\' 2>/dev/null)Clips"
mkdir -p "$SET" "$C" "$I/logs"
printf '{\n  "clips_dir": "C:\\\\Clips",\n  "device_name": "kept"\n}' > "$SET/settings.json"
printf 'a clip' > "$C/Replay 2026-10-13 21-14-03.mp4"
printf 'a line' > "$I/logs/reel.log"
uninstall
! key "$UNINST\\reel" && [ ! -e "$I/reel-desktop.exe" ] && [ ! -e "$MENU" ] || fail "the uninstall left $(ls "$I" 2>/dev/null)"
! wine reg query 'HKCU\Software\Microsoft\Windows\CurrentVersion\Run' /v reel >/dev/null 2>&1 || fail "reel still starts at sign-in"
grep -q kept "$SET/settings.json" && [ -f "$I/logs/reel.log" ] || fail "the silent uninstall took the settings or the logs"
[ -f "$C/Replay 2026-10-13 21-14-03.mp4" ] || fail "the silent uninstall took the clips folder"
! wine reg query "$SCHEME" >/dev/null 2>&1 || fail "the uninstall left the links registered"
ok "the uninstaller, silently: the app, its entry in Apps, its Start menu shortcut, its start at sign-in and its links gone; the settings, the logs and the clips folder stay"
wineserver -k 2>/dev/null || true
rm -rf "$W/one"

prefix two
inst "$WITH"
scheme || fail "$F on a PC of its own did not register its links"
mkdir -p "$I/obs/config/obs-studio"
printf 'set in OBS' > "$I/obs/config/obs-studio/kept.ini"
uninstall
[ ! -e "$I/obs" ] && [ ! -e "$I/reel-desktop.exe" ] || fail "the uninstall of $F left $(ls "$I" 2>/dev/null)"
! wine reg query "$SCHEME" >/dev/null 2>&1 || fail "the uninstall of $F left the links registered"
ok "$F on a PC of its own, its links registered, then uninstalled: OBS, its settings and the links gone with it"
wineserver -k 2>/dev/null || true
