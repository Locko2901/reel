#!/usr/bin/env bash
# After obs-e2e.sh, in the image with the real OBS: this OBS has none of the GPU's encoders (no
# GPU here), whose test encodes a stand-in ffmpeg passes. The setup's test refuses them in OBS
# without starting OBS with one; a setup with one is taken back to x264 before OBS starts, and
# one OBS's log has not listed yet is taken back once OBS holds on its "Missing Encoders" dialog.
set -euo pipefail
APP="${REEL_E2E_APP:-/target/debug/reel-desktop}"
SHOTS="${REEL_E2E_SHOTS:-}"
E="${REEL_E2E_DIR:-/tmp/reel-obs-e2e}-encoders"
D="$E/dir"
LOG="$D/data/logs"
OBSCONF="$D/data/obs/obs-studio"
PROFILE="$OBSCONF/basic/profiles/reel/basic.ini"
SETTINGS="$D/config/settings.json"
rm -rf "$E"
mkdir -p "$E/app" "$D"
[ -n "$SHOTS" ] && mkdir -p "$SHOTS"
say() { printf '\n== %s\n' "$*"; }
ok() { printf 'ok   %s\n' "$*"; }
fail() { printf '\nFAILED: %s\n' "$*"; cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -E "encoder|OBS|obs" | tail -40; exit 1; }
wait_for() { # wait_for <seconds> <description> <command...>
  local secs=$1 what=$2; shift 2
  for _ in $(seq 1 "$secs"); do
    if "$@" >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  fail "timed out waiting for $what"
}
log_has() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -q -- "$1"; }
log_n() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
obs_pid() { local p; for p in $(pgrep -x reel-capture; pgrep -x obs); do tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | grep -qx "XDG_CONFIG_HOME=$D/data/obs" && { echo "$p"; return; }; done; true; }
no_reel_obs() { [ -z "$(obs_pid)" ]; }
missing_dialog() { xdotool search --onlyvisible --name '^Missing Encoders$' >/dev/null 2>&1; }
window() { xdotool search --onlyvisible --name '^reel$' 2>/dev/null | head -1; }
shot() { [ -n "$SHOTS" ] && import -display "$DISPLAY" -window "$(window)" "$SHOTS/$1.png" || true; }
click() { xdotool mousemove "$1" "$2" click 1; }
setting() { python3 -c 'import json,sys; s=json.load(open(sys.argv[1])); s["obs"]["setup"].update(json.loads(sys.argv[2])); json.dump(s, open(sys.argv[1], "w"))' "$SETTINGS" "$1"; }
encoder_now() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["obs"]["setup"]["encoder"])' "$SETTINGS"; }
HERE=$(cd "$(dirname "$0")" && pwd)
buffer_runs() { python3 "$HERE/obs-ws.py" 4466 "$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["obs"]["setup"]["password"])' "$SETTINGS")" GetReplayBufferStatus '{}' 2>/dev/null | grep -q '"outputActive": true'; }
refused() { python3 -c 'import json,sys; k=json.load(open(sys.argv[1])); sys.exit(0 if any(v["id"]==sys.argv[2] and not v["ok"] for v in k["tested"]) else 1)' "$D/data/encoders.json" "$1"; }
start() { "$E/app/reel-desktop" "$@" >>"$E/app.out" 2>&1 & APP_PID=$!; }
quit() {
  "$E/app/reel-desktop" --quit-for-install >>"$E/app.out" 2>&1 || true
  wait_for 30 "the app to exit" sh -c "! kill -0 $APP_PID"
  wait_for 20 "OBS to exit" no_reel_obs
}
trap 'kill -9 ${APP_PID:-0} 2>/dev/null || true; p=$(obs_pid); [ -n "$p" ] && kill -9 "$p" 2>/dev/null || true' EXIT

# The app finds its ffmpeg beside its own file.
cp "$APP" "$E/app/reel-desktop"
ln -s "$(command -v ffprobe)" "$E/app/ffprobe"
cat > "$E/app/ffmpeg" <<'SH'
#!/bin/sh
# The GPU's encoders listed and their test encodes passed, as on a PC with the cards.
case "$*" in
  *-encoders*)
    /usr/bin/ffmpeg "$@"
    echo ' V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)'
    echo ' V....D h264_vaapi           H.264/AVC (VAAPI) (codec h264)'
    exit 0 ;;
  *h264_nvenc*|*h264_vaapi*) exit 0 ;;
esac
exec /usr/bin/ffmpeg "$@"
SH
chmod 755 "$E/app/ffmpeg"
export REEL_DESKTOP_DIR="$D"

say "the setup's test: both GPU encoders pass the test encode, and OBS has neither"
start
wait_for 30 "the setup to open" log_has 'window shows view="setup"'
WIN=$(window)
[ -n "$WIN" ] || fail "no window"
xdotool windowmove "$WIN" 0 0 windowsize "$WIN" 1280 800 2>/dev/null || true
sleep 2
# shellcheck disable=SC2086
click ${REEL_SETUP_GO:-967 305}
sleep 1
# shellcheck disable=SC2086
click ${REEL_SETUP_ENCODER:-233 145}
wait_for 60 "OBS to start for the setup" log_has "the bundled OBS started"
# The test the page started before OBS ran could not try them; the Encoder step tests again.
wait_for 90 "NVENC refused from OBS's list" log_has 'encoder tried in OBS encoder=obs_nvenc_h264_tex ok=false .*line=OBS here has no NVIDIA NVENC H.264'
wait_for 30 "VAAPI refused from OBS's list" log_has 'encoder tried in OBS encoder=ffmpeg_vaapi_tex ok=false .*line=OBS here has no VAAPI H.264'
wait_for 30 "the test to end" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c \"setup: the page has the encoder test's answer\") -ge 2 ]"
[ "$(log_n 'the bundled OBS started')" = 1 ] || fail "OBS was started again for an encoder it does not have"
missing_dialog && fail "OBS shows its Missing Encoders dialog"
refused obs_nvenc_h264_tex && refused ffmpeg_vaapi_tex || fail "the verdicts kept: $(cat "$D/data/encoders.json")"
took=$(grep -h "setup: the encoders tried in OBS" "$LOG"/reel-desktop.log* | sed -n 's/.* ms=\([0-9]*\).*/\1/p' | tail -1 || true)
sleep 1
shot "obs-encoders-setup"
ok "both refused from OBS's own list of encoders, in ${took:-?} ms, OBS never started with one: no Missing Encoders dialog"
"$E/app/reel-desktop" --section=obs >>"$E/app.out" 2>&1 || true
wait_for 20 "Settings to open" log_has 'window shows view="settings"'
sleep 2
for _ in $(seq 1 "${REEL_SETTINGS_SCROLL:-5}"); do xdotool click 5; done
sleep 1
shot "obs-encoders-settings"
quit
ok "Settings, OBS shows the two not offered and why"

say "a setup with an encoder OBS's last start did not list: taken back to x264 before OBS starts"
setting '{"done": true, "encoder": "ffmpeg_vaapi_tex"}'
n=$(log_n "the bundled OBS started")
start --hidden
wait_for 60 "OBS to start" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'the bundled OBS started') -gt $n ]"
log_has 'the change of encoder is taken back failed="ffmpeg_vaapi_tex" back=obs_x264' || fail "the change was not taken back before OBS started"
grep -q '^RecEncoder=obs_x264$' "$PROFILE" || fail "OBS's profile: $(grep RecEncoder "$PROFILE")"
[ "$(encoder_now)" = obs_x264 ] || fail "the saved encoder is $(encoder_now)"
wait_for 30 "the replay buffer" buffer_runs
missing_dialog && fail "OBS shows its Missing Encoders dialog"
quit
ok "x264 saved and written before OBS's start, its replay buffer runs, no dialog"

say "an encoder OBS has not listed yet: OBS holds on its dialog, the change is taken back"
rm -rf "$OBSCONF/logs"
setting '{"encoder": "obs_nvenc_h264_tex"}'
# In OBS's profile too, as when it was picked in OBS itself: reel writes nothing into OBS as it starts.
sed -i 's/^RecEncoder=.*/RecEncoder=obs_nvenc_h264_tex/' "$PROFILE"
python3 -c 'import json,sys; k=json.load(open(sys.argv[1])); k["tested"]=[v for v in k["tested"] if v["id"]!="obs_nvenc_h264_tex"]; json.dump(k, open(sys.argv[1], "w"))' "$D/data/encoders.json"
n=$(log_n "the bundled OBS started")
t0=$(date +%s)
start --hidden
wait_for 40 "OBS's Missing Encoders dialog" missing_dialog
wait_for 60 "the change to be taken back" log_has 'the change of encoder is taken back failed="obs_nvenc_h264_tex" back=obs_x264'
wait_for 60 "OBS to start again with x264" sh -c "[ \$(cat $LOG/reel-desktop.log* | grep -c 'the bundled OBS started') -ge $((n + 2)) ]"
wait_for 40 "the replay buffer with x264" buffer_runs
t1=$(date +%s)
grep -q '^RecEncoder=obs_x264$' "$PROFILE" || fail "OBS's profile: $(grep RecEncoder "$PROFILE")"
missing_dialog && fail "the Missing Encoders dialog stayed"
refused obs_nvenc_h264_tex || fail "NVENC is not refused now"
quit
ok "taken back to x264 and recording $((t1 - t0)) s after the start; NVENC refused"
trap - EXIT
echo
echo "the encoders with the real OBS: all ok"
