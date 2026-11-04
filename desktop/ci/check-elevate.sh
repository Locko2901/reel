#!/bin/sh
# reel-elevate.exe under Wine, as far as Wine goes; what Wine cannot do is said.
set -eu
S=/work/desktop/.wine-standins
W=$(mktemp -d)
export HOME="$W/home" WINEPREFIX="$W/wine" WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
mkdir -p "$HOME"
fail() { printf 'FAIL %s\n' "$*"; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
wine wineboot --init >/dev/null 2>&1 || true
winvar() { wine cmd /c "echo %$1%" 2>/dev/null | tr -d '\r'; }
LW="$(winvar LOCALAPPDATA)\\reel"
L=$(winepath -u "$LW" 2>/dev/null | tr -d '\r')
PFW="$(winvar ProgramW6432)"
[ -n "$PFW" ] && [ "$PFW" != "%ProgramW6432%" ] || PFW="$(winvar ProgramFiles)"
PF=$(winepath -u "$PFW" 2>/dev/null | tr -d '\r')
mkdir -p "$L/obs/bin/64bit" "$L/obs/data/obs-studio" "$L/obs/core/obs-websocket"
cp "$S/standin_obs.exe" "$L/obs/bin/64bit/reel-capture.exe"
echo "Qt of the bundled OBS" > "$L/obs/bin/64bit/Qt6Core.dll"
echo "data" > "$L/obs/data/obs-studio/a.txt"
echo "plugin" > "$L/obs/core/obs-websocket/obs-websocket.dll"
: > "$L/obs/portable_mode.txt"
cp "$S/reel-elevate.exe" "$L/reel-elevate.exe"
st="$L/obs/copy-settings/obs-studio"
mkdir -p "$st/basic/profiles/reel" "$st/basic/scenes" "$st/plugin_config/obs-websocket"
printf '[General]\nLastVersion=1\n' > "$st/global.ini"
printf '[General]\nFirstRun=true\n' > "$st/user.ini"
printf '[AdvOut]\nRecRB=true\nRecRBTime=60\n' > "$st/basic/profiles/reel/basic.ini"
printf '{"rate_control":"CBR","bitrate":12500}' > "$st/basic/profiles/reel/recordEncoder.json"
printf '{"name":"reel","sources":[],"modules":{"scripts-tool":[{"path":"C:/evil.lua"}],"output-timer":{}}}' > "$st/basic/scenes/reel.json"
printf '{"server_port":4466}' > "$st/plugin_config/obs-websocket/config.json"
printf '{"not":"reel'"'"'s"}' > "$st/basic/scenes/other.json"
mkdir -p "$PF/reel/obs/obs-plugins/64bit"
echo "an older OBS's plugin" > "$PF/reel/obs/obs-plugins/64bit/old.dll"
ok "the install: a bundled OBS, reel-elevate.exe beside the app, the settings staged; an older copy in $PFW\\reel"

ANSW="$(winvar TEMP)\\reel-admin-test.txt"
ANS=$(winepath -u "$ANSW" | tr -d '\r')
set +e
wine "$L/reel-elevate.exe" install "$LW" "$ANSW" "WINE\\tester" > "$W/out.txt" 2>&1
CODE=$?
set -e
OUT=$(tr -d '\r' < "$W/out.txt")
printf '%s\n' "$OUT" | sed 's/^/     /'
ANSWER=$(cat "$ANS" 2>/dev/null | tr -d '\r' || true)
[ -n "$ANSWER" ] || fail "no answer in the answer file"
ok "the answer file: $ANSWER (exit code $CODE)"
case "$ANSWER" in
  ok) ;;
  *"rights of"*"not the administrators"*)
    [ ! -e "$PF/reel" ] || fail "the copy stayed after the rights check refused it"
    ok "Wine keeps no owner but its user's, so the rights check refuses the copy and removes it, its task too, as the helper does on Windows when the rights are not Program Files' own"
    PARTS=1 ;;
  *) fail "the helper: $ANSWER" ;;
esac
if [ "${PARTS:-}" = 1 ]; then
  mkdir -p "$PF/reel/obs/obs-plugins/64bit"
  echo "an older OBS's plugin" > "$PF/reel/obs/obs-plugins/64bit/old.dll"
  cp "$S/reel-elevate.exe" "$PF/reel/reel-elevate.exe"
  wine "$S/elevate_parts.exe" "$LW" "$PFW\\reel" "WINE\\tester" 2>/dev/null | tr -d '\r' > "$W/parts.txt" || true
  sed 's/^/     /' "$W/parts.txt"
  ok "the helper's steps after the check, one by one, the same functions (elevate_parts):"
fi
[ "$(cat "$PF/reel/obs/bin/64bit/Qt6Core.dll")" = "Qt of the bundled OBS" ] && [ -f "$PF/reel/obs/data/obs-studio/a.txt" ] && [ -f "$PF/reel/obs/portable_mode.txt" ] || fail "the copy is not the bundled OBS"
[ -f "$PF/reel/obs/core/obs-websocket/obs-websocket.dll" ] || fail "the bundled OBS's plugins (core) are not in the copy"
[ ! -e "$PF/reel/obs/obs-plugins" ] || fail "an older OBS's plugins (obs-plugins) stayed in the copy"
ok "the copy in $PFW\\reel\\obs: the bundled OBS's program, data and plugins (core, as OBS 33 has them), nothing of an older OBS (no obs-plugins)"
[ -f "$PF/reel/reel-elevate.exe" ] && grep -q . "$PF/reel/obs/reel-version.txt" || fail "no helper or version in the copy"
ok "reel-elevate.exe beside the copy (for the task and later steps), and the version: $(cat "$PF/reel/obs/reel-version.txt")"
C="$PF/reel/obs/config/obs-studio"
[ -f "$C/basic/profiles/reel/basic.ini" ] && [ -f "$C/plugin_config/obs-websocket/config.json" ] && [ ! -e "$C/basic/scenes/other.json" ] || fail "the settings are not the six of reel's"
grep -q '"output-timer"' "$C/basic/scenes/reel.json" && ! grep -q 'scripts-tool\|evil.lua' "$C/basic/scenes/reel.json" || fail "the scene collection's scripts were not dropped"
ok "the settings: the six files of reel's, nothing else; the scene collection without its scripts"
NOSCHED=""
if [ "${PARTS:-}" = 1 ]; then
  if grep -q '^task FAILED: .*0x80041315' "$W/parts.txt"; then
    NOSCHED=1
    ok "the task: Task Scheduler's interface answers that its service does not run (0x80041315); Wine's Schedule service does not start (sc start: 1053), so the task is Windows' to show"
  else
    grep -q '^task: registered' "$W/parts.txt" || fail "the task, through Task Scheduler's interface: $(grep '^task' "$W/parts.txt")"
  fi
fi
T=$(find "$WINEPREFIX/drive_c/windows/system32/tasks" -iname 'reel OBS*' 2>/dev/null | head -1)
if [ -n "$T" ]; then
  iconv -f utf-16 -t utf-8 "$T" 2>/dev/null > "$W/task.xml" || cp "$T" "$W/task.xml"
  grep -q 'HighestAvailable' "$W/task.xml" && grep -q 'LogonTrigger' "$W/task.xml" && grep -q '<Arguments>start</Arguments>' "$W/task.xml" || { cat "$W/task.xml"; fail "the task is not as it should be"; }
  ok "the task, through Task Scheduler's interface: at sign-in, the highest rights, the copy's helper's start"
elif [ -z "$NOSCHED" ]; then
  ok "the task registered through Task Scheduler's interface (Wine keeps no file of it to read back)"
fi
echo "[AdvOut]
RecRBTime=90" > "$st/basic/profiles/reel/basic.ini"
wine "$L/reel-elevate.exe" write "$LW" "$ANSW" "WINE\\tester" >/dev/null 2>&1 || true
if [ "${PARTS:-}" = 1 ]; then
  case "$(cat "$ANS" | tr -d '\r')" in *"not the administrators"*) ;; *) fail "write: $(cat "$ANS")" ;; esac
  ok "write: the settings taken, then the same rights check refuses under Wine and removes the copy"
  mkdir -p "$PF/reel/obs/bin/64bit" && cp "$S/reel-elevate.exe" "$PF/reel/reel-elevate.exe" && : > "$PF/reel/obs/bin/64bit/reel-capture.exe"
  wine "$S/elevate_parts.exe" "$LW" "$PFW\\reel" "WINE\\tester" >/dev/null 2>&1 || true
else
  [ "$(cat "$ANS" | tr -d '\r')" = "ok" ] && grep -q 'RecRBTime=90' "$C/basic/profiles/reel/basic.ini" || fail "write: $(cat "$ANS")"
  ok "write: the settings written again, the copy as it was"
fi
B="$PF/reel/obs/bin/64bit"
mkdir -p "$B" "$PF/reel/obs/config/obs-studio/.sentinel"
cp "$S/standin_obs.exe" "$B/reel-capture.exe"
cp "$S/reel-elevate.exe" "$PF/reel/reel-elevate.exe"
: > "$PF/reel/obs/config/obs-studio/.sentinel/run_1"
if [ -z "${NOSCHED:-}" ]; then
  OLD=$(wine "$S/elevate_parts.exe" old "WINE\\tester" 2>/dev/null | tr -d '\r')
  [ "$OLD" = "old tasks: registered" ] || fail "the tasks of an older version's copy: $OLD"
  T=$(find "$WINEPREFIX/drive_c/windows/system32/tasks" -iname 'reel OBS (games*' 2>/dev/null | head -1)
  if [ -n "$T" ]; then
    iconv -f utf-16 -t utf-8 "$T" 2>/dev/null > "$W/old.xml" || cp "$T" "$W/old.xml"
    grep -q '<Arguments>clear-marks</Arguments>' "$W/old.xml" && grep -q 'minimize-to-tray' "$W/old.xml" || { cat "$W/old.xml"; fail "the older version's task is not as it was"; }
    [ -z "$(find "$WINEPREFIX/drive_c/windows/system32/tasks" -iname 'reel OBS window*')" ] || fail "a second task is left"
  fi
fi
AP=$(wine "$S/elevate_parts.exe" apartment 2>/dev/null | tr -d '\r')
says() { printf '%s\n' "$AP" | grep -qx "$1" || { echo "$AP"; fail "with an older version's copy, on the app's main thread: $1"; }; }
says 'second task: false'
says 'ole: S_OK'
[ -n "${NOSCHED:-}" ] || says 'task: true'
if [ -z "${NOSCHED:-}" ]; then
  ok "an older version's copy and its task (at sign-in: clear-marks, then OBS in its tray; no second task): the app's calls to Task Scheduler on its main thread see the task and no second one, and leave the thread as it was: OleInitialize there, as the window does: S_OK"
else
  ok "an older version's copy, no second task: the app's calls to Task Scheduler on its main thread (which Wine's answers without its service) leave the thread as it was: OleInitialize there, as the window does: S_OK"
fi
rm -f "$B/args.txt" "$B/closed.txt"
look() { wine "$S/elevate_parts.exe" look 2>/dev/null | tr -d '\r'; }
helper() { wine "$PF/reel/reel-elevate.exe" "$@" >/dev/null 2>&1; }
wine "$PF/reel/reel-elevate.exe" start >/dev/null 2>&1 &
START=$!
for _ in $(seq 1 60); do [ -f "$B/args.txt" ] && look | grep -q 'hidden\|shown' && break; sleep 0.5; done
[ -f "$B/args.txt" ] || fail "start: the copy's OBS did not start"
ARGS=$(cat "$B/args.txt")
case "$ARGS" in *--portable*--only-bundled-plugins*) ;; *) fail "start: OBS's line: $ARGS" ;; esac
case "$ARGS" in *--minimize-to-tray*) fail "start: OBS still sent to its tray: $ARGS" ;; esac
[ ! -e "$PF/reel/obs/config/obs-studio/.sentinel/run_1" ] || fail "start: OBS's crash marks stayed"
FIRST=$(look | cut -d' ' -f2)
ok "start, from the copy's folder: its OBS with --portable ... --only-bundled-plugins, no tray, its crash marks cleared; its main window $FIRST at first (the start's show state, as Wine applies it)"
helper window hide; [ "$(look | cut -d' ' -f2)" = hidden ] || fail "window hide: $(look)"
helper window show; [ "$(look | cut -d' ' -f2-)" = "shown normal" ] || fail "window show: $(look)"
helper window hide; [ "$(look | cut -d' ' -f2-)" = "hidden minimized" ] || fail "window hide, again: $(look)"
ok "window hide, show, hide: OBS's main window gone, back restored by itself, then minimized by itself and gone"
for bad in "show now" "Show" "run" ""; do
  set +e; wine "$PF/reel/reel-elevate.exe" window $bad >/dev/null 2>&1; CODE=$?; set -e
  [ "$CODE" = 2 ] && [ "$(look | cut -d' ' -f2)" = hidden ] || fail "window \"$bad\": exit code $CODE, $(look)"
done
ok "any other word (\"show now\", \"Show\", \"run\", none): nothing done, exit code 2"
SEEN=$(wine "$S/elevate_parts.exe" wait 2>/dev/null | tr -d '\r')
set +e; wait $START; CODE=$?; set -e
[ -f "$B/closed.txt" ] || fail "window close: OBS got no close ($SEEN)"
[ "$(look)" = none ] && [ "$CODE" = 0 ] || fail "window close: $(look), the start's exit code $CODE"
ok "window close: OBS closed as its close button closes it and exited; the app's wait: $SEEN; the helper's start out with OBS's exit code, 0"
rm -f "$B/args.txt"
wine "$PF/reel/reel-elevate.exe" start >/dev/null 2>&1 &
for _ in $(seq 1 60); do [ -f "$B/args.txt" ] && [ "$(look)" != none ] && break; sleep 0.5; done
wine taskkill /f /im reel-elevate.exe >/dev/null 2>&1 || true
for _ in $(seq 1 20); do [ "$(look)" = none ] && break; sleep 0.5; done
if [ "$(look)" = none ]; then
  ok "the helper's start ended (as Task Scheduler ends the task): its OBS ended with it, held in its job"
else
  wine taskkill /f /im reel-capture.exe >/dev/null 2>&1 || true
  ok "the helper's start ended: Wine left its OBS running, so the job's end of it is Windows' to show"
fi
wine "$L/reel-elevate.exe" remove "$LW" "$ANSW" >/dev/null 2>&1 || true
[ ! -e "$PF/reel" ] || fail "remove: $(cat "$ANS"); left $(ls "$PF/reel" 2>/dev/null)"
if [ -n "$NOSCHED" ]; then
  grep -q '0x80041315' "$ANS" || fail "remove: $(cat "$ANS")"
  ok "remove: the copy gone, and the answer says what Task Scheduler said: $(tr -d '\r' < "$ANS")"
else
  [ "$(cat "$ANS" | tr -d '\r')" = "ok" ] || fail "remove: $(cat "$ANS")"
  [ -z "$(find "$WINEPREFIX/drive_c/windows/system32/tasks" -iname 'reel OBS*' 2>/dev/null)" ] || fail "remove: the task stayed"
  ok "remove, the helper beside the app: the copy and the task gone"
fi
mkdir -p "$PF/reel/obs/bin/64bit"
cp "$S/standin_obs.exe" "$PF/reel/obs/bin/64bit/reel-capture.exe"
cp "$S/reel-elevate.exe" "$PF/reel/reel-elevate.exe"
H=$(wine "$S/elevate_parts.exe" hammer "$LW" 2>/dev/null | tr -d '\r' | grep '^requests ')
field() { printf '%s\n' "$H" | sed -n "s/.* $1 \([^ ]*\).*/\1/p"; }
[ -n "$H" ] || fail "the hammer said nothing"
[ "$(field helpers-at-once)" = 1 ] && [ "$(field steps)" -ge 1 ] || fail "the hammer: $H"
[ "$(field saved)/$(field copy)" != "on/none" ] || fail "the hammer left the switch on without a copy: $H"
[ "$(field saved)/$(field copy)/$(field left)" != "off/there/no" ] || fail "the hammer left a copy with the switch off, not said: $H"
ok "the switch hammered: 20 requests from four threads at random moments through the app's lane and the real step, the helper run as administrator as the app runs it (Wine then refuses the rights): $(field turns) turns, $(field steps) steps, never more than one helper at once; at the end the switch $(field saved), the copy $(field copy), a copy left said: $(field left)"
