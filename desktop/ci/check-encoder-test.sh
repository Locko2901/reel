#!/bin/sh
# Under xvfb.sh with XVFB_SCREEN=1280x900: the setup's encoder test in the app, its rows as each
# test ends. A stand-in ffmpeg beside a copy of the app answers the test encodes: NVENC fails
# after a while as on a PC without the card, VAAPI hangs until a mark file is there.
set -eu
APP=/target/debug/reel-desktop
SHOTS="${REEL_SHOTS_OUT:-}"
REAL=/work/desktop/app/binaries/ffmpeg-x86_64-unknown-linux-gnu
[ -x /target/debug/examples/fake_obs ] || { echo "FAIL no stand-in OBS: make desktop-linux builds it"; exit 1; }
W=$(mktemp -d)
D="$W/new"
L="$D/data/logs"
PIDS=""
fail() {
  echo "FAIL $*"
  grep -m1 -A1 "panicked" "$W"/app.out 2>/dev/null || tail -n 20 "$W"/app.out 2>/dev/null
  grep "setup: " "$L"/reel-desktop.log* 2>/dev/null | tail -n 20
  exit 1
}
ok() { echo "ok   $*"; }
cleanup() { for p in $PIDS; do kill "$p" 2>/dev/null || true; done; pkill -f fake_obs 2>/dev/null || true; }
trap cleanup EXIT
logged() { cat "$L"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
waitlog() { # waitlog <words> <count> <seconds> <what>
  for _ in $(seq 1 $(($3 * 4))); do [ "$(logged "$1")" -ge "$2" ] && return 0; sleep 0.25; done
  fail "$4"
}
window() { xdotool search --onlyvisible --name '^reel$' 2>/dev/null | head -1; }
shot() {
  [ -n "$SHOTS" ] || return 0
  import -display "$DISPLAY" -window "$(window)" "$SHOTS/$1.png"
}
click() { xdotool mousemove "$1" "$2" click 1; }
[ -z "$SHOTS" ] || mkdir -p "$SHOTS"

# The app finds its ffmpeg beside its own file.
mkdir -p "$W/app" "$W/rec"
cp "$APP" "$W/app/reel-desktop"
ln -s "${REAL%/*}/ffprobe-x86_64-unknown-linux-gnu" "$W/app/ffprobe"
cat > "$W/app/ffmpeg" <<SH
#!/bin/sh
case "\$*" in
  *h264_nvenc*)
    sleep 6
    echo '[h264_nvenc @ 0x5581c0] Cannot load libcuda.so.1' >&2
    echo '[vost#0:0/h264_nvenc @ 0x5581a0] Error while opening encoder - maybe incorrect parameters such as bit_rate, rate, width or height.' >&2
    echo '[vost#0:0/h264_nvenc @ 0x5581a0] [enc:h264_nvenc @ 0x5581e0] Could not open encoder before EOF' >&2
    echo '[vost#0:0/h264_nvenc @ 0x5581a0] Task finished with error code: -22 (Invalid argument)' >&2
    exit 234 ;;
  *h264_vaapi*)
    [ -e "$W/vaapi-ok" ] || exec sleep 60
    sleep 3; exit 0 ;;
esac
exec "$REAL" "\$@"
SH
chmod 755 "$W/app/ffmpeg"
"$REAL" -v error -f lavfi -i "testsrc2=duration=3:size=1280x720:rate=30" -c:v libx264 -pix_fmt yuv420p "$W/clip.mp4"

export REEL_DESKTOP_DIR="$D" RUST_BACKTRACE=1
export REEL_OBS_EXE=/work/desktop/ci/standin-obs-slow.sh STANDIN_DIR="$W/rec" STANDIN_CLIP="$W/clip.mp4" STANDIN_CLOSE_S=0
"$W/app/reel-desktop" > "$W/app.out" 2>&1 &
PIDS="$PIDS $!"
waitlog 'window shows view="setup"' 1 30 "the setup did not open"
waitlog "setup: encoder test asked" 1 30 "the setup page did not start the encoder test"
WIN=$(window)
[ -n "$WIN" ] || fail "no window"
xdotool windowmove "$WIN" 0 0 windowsize "$WIN" 1280 800 2>/dev/null || true
sleep 1
# shellcheck disable=SC2086
click ${REEL_SETUP_GO:-967 305}
sleep 1
# shellcheck disable=SC2086
click ${REEL_SETUP_ENCODER:-233 145}
sleep 1
[ "$(logged 'setup: encoder tested')" -eq 0 ] || fail "NVENC's test ended before its row could be seen testing"
shot "1-waiting"
ok "the rows from the start: NVENC testing, VAAPI waiting (shot 1)"
waitlog "setup: encoder tested encoder=\"obs_nvenc_h264_tex\" state=NotHere" 1 20 "NVENC's row did not end as not here"
sleep 3
shot "2-testing"
ok "NVENC not here in plain words, VAAPI testing with its seconds (shot 2)"
waitlog "setup: encoder tested encoder=\"ffmpeg_vaapi_tex\" state=NoAnswer" 1 30 "VAAPI's hung test did not end at the time limit"
waitlog "setup: encoder test done" 1 10 "the test did not end"
waitlog "setup: the page has the encoder test's answer" 1 10 "the page did not log when it had the answer"
sleep 1
shot "3-done-no-answer"
pgrep -f "sleep 60" > /dev/null && fail "the hung stand-in still runs after the time limit"
ms=$(cat "$L"/reel-desktop.log* | sed -n 's/.*encoder="ffmpeg_vaapi_tex" state=NoAnswer ms=\([0-9]*\).*/\1/p' | head -1)
[ -n "$ms" ] && [ "$ms" -ge 10000 ] && [ "$ms" -lt 13000 ] || fail "VAAPI's time in the log: '$ms' ms, not the 10 s limit"
ok "a test that hangs: did not answer in 10 s ($ms ms), its ffmpeg stopped (shot 3)"
touch "$W/vaapi-ok"
# shellcheck disable=SC2086
click ${REEL_ENCODERS_AGAIN:-1078 222}
waitlog "setup: encoder test asked" 2 10 "Test again did not start a test"
waitlog "setup: encoder test done" 2 30 "the second test did not end"
grep -q 'encoder="ffmpeg_vaapi_tex" state=Works' "$L"/reel-desktop.log* || fail "VAAPI did not work the second time"
sleep 1
# shellcheck disable=SC2086
click ${REEL_ENCODER_DETAILS:-760 313}
sleep 1
shot "4-done-details"
ok "Test again: VAAPI works, NVENC's details open on ffmpeg's lines (shot 4)"
grep "setup: " "$L"/reel-desktop.log* | sed 's/^[^ ]* *//' | cut -c1-220
