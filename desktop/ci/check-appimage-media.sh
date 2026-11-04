#!/usr/bin/env bash
#   check-appimage-media.sh <the AppImage> <server> <user> <password> <folder for the screenshots>
set -u
IMG="$1" SERVER="$2" USER_NAME="$3" PASSWORD="$4" SHOTS="$5"
W=$(mktemp -d)
export HOME="$W/home" XDG_RUNTIME_DIR="$W/run" NO_AT_BRIDGE=1 DISPLAY=:97
mkdir -p "$HOME" "$XDG_RUNTIME_DIR" "$SHOTS"
chmod 700 "$XDG_RUNTIME_DIR"
HOST=$(echo "$SERVER" | sed -E 's#https?://([^/:]+).*#\1#')
fail() { printf 'FAIL %s\n' "$*"; tail -15 "$W"/*.out 2>/dev/null; cat "$W"/*/data/logs/reel-desktop.log* 2>/dev/null | tail -15; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
! ls /var/lib/pacman/local | grep -qE '^(gstreamer|gst-)' || fail "this Arch has GStreamer installed"
[ ! -d /usr/lib/gstreamer-1.0 ] || fail "this Arch has GStreamer's plugins"
(cd "$W" && "$IMG" --appimage-extract >/dev/null)
R="$W/squashfs-root"
P="$R/usr/lib/gstreamer-1.0"
[ -f "$P/libgstautodetect.so" ] && [ -f "$P/libgstlibav.so" ] && [ -x "$R/usr/lib/gstreamer1.0/gstreamer-1.0/gst-plugin-scanner" ] \
  || fail "the AppImage carries no GStreamer plugins"
[ -f "$P/libgstopengl.so" ] || fail "the AppImage carries no GL plugin (glimagesink, glsinkbin)"
ok "the AppImage carries GStreamer: $(ls "$P" | wc -l) plugins, libav's decoders, the GL plugin and the plugin scanner among them"

JAR="$W/jar"
curl -fsS -c "$JAR" -o /dev/null "$SERVER/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode "remember=on" "$SERVER/signin"
CODE=$(curl -fsS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -X POST "$SERVER/api/devices/pair-code" | sed -n 's/.*"code":"\([^"]*\)".*/\1/p')
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$CODE\",\"name\":\"arch-media\"}" "$SERVER/api/device/pair" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
[ ${#TOKEN} -eq 43 ] || fail "no pairing with $SERVER"

Xvfb :97 -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &
XVFB=$!
sleep 1
shot() { import -window root "$SHOTS/media-$1.png"; }
changed() { # changed <a> <b> <crop>: how many pixels of the crop differ between two shots
  magick compare -metric AE <(magick "$SHOTS/media-$1.png" -crop "$3" +repage png:-) <(magick "$SHOTS/media-$2.png" -crop "$3" +repage png:-) null: 2>&1 | awk '{printf "%d", $1}'
}
CARD=280x150+28+160 PLAYER=560x300+140+160
start() { # start <log name>: paired, on the library, signed in through the window the first time
  D="$W/app"
  mkdir -p "$D/config"
  printf '%s' "$TOKEN" > "$D/config/token"
  chmod 600 "$D/config/token"
  printf '{"server_url":"%s","device_name":"arch-media","obs":{"enabled":false,"setup_later":true},"startup":{"autostart":false},"log_level":"debug"}' "$SERVER" > "$D/config/settings.json"
  LOG="$D/data/logs"
  local front signin
  front=$(loads "library loaded at=$HOST/$") signin=$(loads "library loaded at=$HOST/signin")
  # APPDIR as the AppImage's runtime sets it before AppRun, whose GStreamer hook reads it
  APPDIR="$R" REEL_DESKTOP_DIR="$D" APPIMAGE="$IMG" GST_DEBUG=webkitmediaplayer:4 GST_DEBUG_NO_COLOR=1 GST_DEBUG_FILE="$W/$1.gst" \
    dbus-run-session -- "$R/AppRun" > "$W/$1.out" 2>&1 &
  APP=$!
  for _ in $(seq 1 60); do
    [ "$(loads "library loaded at=$HOST/$")" -gt "$front" ] && { sleep 3; return 0; }
    [ "$(loads "library loaded at=$HOST/signin")" -gt "$signin" ] && break
    sleep 1
  done
  [ "$(loads "library loaded at=$HOST/signin")" -gt "$signin" ] || fail "$1: neither the library nor the sign-in page in the window"
  sleep 2
  xdotool mousemove 640 328 click 1
  xdotool type --delay 20 "$USER_NAME"
  xdotool key Tab
  xdotool type --delay 20 "$PASSWORD"
  xdotool key Return
  wait_log 30 "library loaded at=$HOST/$" "$front" || { shot "$1-signin"; fail "$1: the library did not load after the sign-in"; }
  sleep 3
}
loads() { cat "$LOG"/reel-desktop.log* 2>/dev/null | grep -c -- "$1" || true; }
wait_log() { # wait_log <seconds> <pattern> [count before]
  local before=${3:-0}
  for _ in $(seq 1 "$1"); do [ "$(loads "$2")" -gt "$before" ] && return 0; sleep 1; done
  return 1
}
quit() { # the app itself, below dbus-run-session, and its web process
  pkill -x reel-desktop
  for _ in $(seq 1 20); do pgrep -x reel-desktop >/dev/null || break; sleep 0.5; done
  pkill -f WebKitWebProcess
  kill "$APP" 2>/dev/null
  sleep 1
}
hover() { xdotool mousemove 700 120 click 1; sleep 1; xdotool mousemove 160 230; sleep 4; shot "$1-a"; sleep 1; shot "$1-b"; }

start plays
hover hover
n=$(changed hover-a hover-b "$CARD")
[ "$n" -gt 300 ] || fail "the card's hover loop does not play ($n pixels changed)"
ok "the library's hover loop plays ($n pixels of the card changed in a second)"
# WebKit's player logs each pass's end; WebKitGTK 2.50 ended a looped video's later passes early
ends() { grep -c 'didEnd:.*Playback ended' "$W/plays.gst" 2>/dev/null || true; }
for _ in $(seq 1 16); do [ "$(ends)" -ge 3 ] && break; sleep 1; done
url=$(grep -m1 -o 'Load http[^ ]*\.loop\.mp4' "$W/plays.gst" | cut -c6-)
[ -n "$url" ] || fail "WebKit loaded no hover loop"
curl -fsS -b "$JAR" -o "$W/loop.mp4" "$url" || fail "the hover loop $url does not download"
len=$(python3 -c 'import struct,sys; d=open(sys.argv[1],"rb").read(); i=d.find(b"mvhd"); t,n=struct.unpack(">IQ",d[i+24:i+36]) if d[i+4]==1 else struct.unpack(">II",d[i+16:i+24]); print(n/t)' "$W/loop.mp4")
passes=$(grep 'didEnd:.*Playback ended' "$W/plays.gst" | awk '{split($1,t,":"); s=t[1]*3600+t[2]*60+t[3]; if (NR>1) {printf "%s%.2f", sep, s-p; sep=", "} p=s}')
[ "$(ends)" -ge 2 ] || fail "the hover loop did not come round twice in 16 s ($(ends) ends)"
echo "$passes" | awk -F', ' -v len="$len" '{for (i=1;i<=NF;i++) if ($i < len-0.25) exit 1}' \
  || fail "a pass of the hover loop ended early: the loop is $len s, the passes after the first $passes s"
ok "the hover loop comes round whole: the loop $len s, the passes after the first $passes s"
xdotool click 1
wait_log 20 "library loaded at=$HOST/clips/" || fail "the card's click did not open the clip"
sleep 3
xdotool mousemove 440 330 click 1
sleep 3
shot play-a
sleep 1
shot play-b
n=$(changed play-a play-b "$PLAYER")
[ "$n" -gt 1000 ] || fail "the clip does not play ($n pixels changed)"
ok "the clip's page opens and its video plays ($n pixels of the player changed in a second)"
before=$(loads "library loaded at=$HOST/$")
xdotool mousemove 112 24 click 1
wait_log 20 "library loaded at=$HOST/$" "$before" || fail "the header's Library did not answer"
! grep -q "not found. Please install it\|CRITICAL" "$W/plays.out" || fail "$(grep -m2 "not found\|CRITICAL" "$W/plays.out")"
! grep -q "video and sound in the window are off" "$LOG"/reel-desktop.log* || fail "the app turned the window's media off"
! grep -q "GL video sink" "$W/plays.out" || fail "WebKit found no GL video sink: $(grep -m1 'GL video sink' "$W/plays.out")"
ok "the window answers after playing; no GStreamer element missing, no GObject critical, and WebKit has its GL video sink (no \"GL video sink dependencies\" warning)"
quit

: > "$P/libgstautodetect.so"
rm -rf "$HOME/.cache/gstreamer-1.0"
start broken
pages=$(loads "library loaded at=$HOST/")
hover broken
xdotool click 1
wait_log 30 "the page stopped answering" || fail "a page with a broken audio sink froze the window and nothing ended it"
grep -q "autoaudiosink not found" "$W/broken.out" || fail "the audio sink was not missing after all"
ok "with the audio sink's plugin broken the page stops answering, as was seen on a real desktop; the app ends its web process"
wait_log 30 "library loaded at=$HOST/" "$pages" || fail "the page did not load again"
sleep 3
clips=$(loads "library loaded at=$HOST/clips/")
xdotool mousemove 900 760; sleep 1; xdotool mousemove 160 230; sleep 2
xdotool click 1
wait_log 20 "library loaded at=$HOST/clips/" "$clips" || fail "after the reload the card's click did not open the clip"
sleep 3
shot broken-clip
before=$(loads "library loaded at=$HOST/$")
xdotool mousemove 440 330 click 1; sleep 2
xdotool mousemove 112 24 click 1
wait_log 20 "library loaded at=$HOST/$" "$before" || fail "after the reload the header's Library did not answer"
ok "the page loads again without video and sound and the window answers: the clip opens, the header's Library works"
quit
kill "$XVFB" 2>/dev/null || true
