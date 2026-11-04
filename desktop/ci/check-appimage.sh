#!/usr/bin/env bash
# The .deb's contents, and the AppImage on X11 and, with a stand-in portal, as on Wayland.
#   check-appimage.sh <folder> <version>
set -euo pipefail
IN="$1" VERSION="$2"
IMG="$IN/reel_${VERSION}_amd64.AppImage"
DEB="$IN/reel_${VERSION}_amd64.deb"
W=$(mktemp -d)
fail() { printf 'FAIL %s\n' "$*"; cat "$W"/*/data/logs/reel-desktop.log* 2>/dev/null | tail -25; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
for f in "$IMG" "$DEB"; do [ -f "$f" ] || fail "$(basename "$f") is not there"; done
FFV=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' /work/desktop/ffmpeg/pinned-linux.json | head -1)

dpkg-deb -c "$DEB" > "$W/deb.txt"
for p in usr/bin/reel-desktop usr/bin/ffmpeg usr/bin/ffprobe usr/share/applications/reel.desktop; do
  grep -Eq " (\./)?$p\$" "$W/deb.txt" || fail "the .deb lacks /$p"
done
DEPS=$(dpkg-deb -f "$DEB" Depends)
case "$DEPS" in *libwebkit2gtk-4.1*) ;; *) fail "the .deb does not depend on WebKitGTK: $DEPS" ;; esac
case "$DEPS" in *gstreamer1.0-plugins-good*gstreamer1.0-libav*|*gstreamer1.0-libav*gstreamer1.0-plugins-good*) ;; *) fail "the .deb does not depend on GStreamer's plugins for the clips' video: $DEPS" ;; esac
ok "the .deb: reel-desktop, ffmpeg, ffprobe, the menu entry; depends on $DEPS"

(cd "$W" && "$IMG" --appimage-extract >/dev/null)
R="$W/squashfs-root"
for p in AppRun usr/bin/reel-desktop usr/bin/ffmpeg usr/bin/ffprobe reel.desktop; do
  [ -e "$R/$p" ] || fail "the AppImage lacks $p"
done
"$R/usr/bin/ffmpeg" -hide_banner -version | head -1 | grep -q "$FFV" || fail "the AppImage's ffmpeg is not $FFV"
! ldd "$R/usr/bin/ffmpeg" 2>/dev/null | grep -q libav || fail "the AppImage's ffmpeg wants libav* of the system"
ok "the AppImage: AppRun, reel-desktop, the menu entry, ffmpeg $FFV built in (no libav* of the system)"
G="$R/usr/lib/gstreamer-1.0"
for p in libgstcoreelements.so libgstplayback.so libgstautodetect.so libgstpulseaudio.so libgstalsa.so libgstisomp4.so libgstlibav.so; do
  [ -f "$G/$p" ] || fail "the AppImage lacks GStreamer's $p"
done
[ -x "$R/usr/lib/gstreamer1.0/gstreamer-1.0/gst-plugin-scanner" ] || fail "the AppImage lacks GStreamer's plugin scanner"
grep -q 'GST_PLUGIN_SYSTEM_PATH_1_0="${APPDIR}/usr/lib/gstreamer-1.0"' "$R/apprun-hooks/linuxdeploy-plugin-gstreamer.sh" || fail "the AppImage does not point GStreamer at its own plugins"
ok "the AppImage carries GStreamer for the window's video and sound: $(ls "$G" | wc -l) plugins (base, good, libav's decoders, ALSA), the scanner, its hook"

start() { # start <name> [env...]: the AppImage's app with a home of its own, hidden
  local name=$1; shift
  mkdir -p "$W/$name/config" "$W/$name/data"
  printf '{"server_url":"http://127.0.0.1:9","device_name":"check","obs":{"enabled":false},"startup":{"autostart":false},"log_level":"debug"}' > "$W/$name/config/settings.json"
  env REEL_DESKTOP_DIR="$W/$name" APPIMAGE="$IMG" "$@" "$R/AppRun" --hidden > "$W/$name/out.txt" 2>&1 &
  PID=$!
  LOG="$W/$name/data/logs"
  for _ in $(seq 1 60); do grep -qs "reel desktop starts" "$LOG"/reel-desktop.log* && return 0; sleep 0.5; done
  fail "$name: the app did not start: $(tail -5 "$W/$name/out.txt")"
}
logged() { grep -qs -- "$1" "$LOG"/reel-desktop.log*; }
waitlog() { for _ in $(seq 1 "$2"); do logged "$1" && return 0; sleep 1; done; fail "$3"; }
stop() { kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true; }

start x11 XDG_SESSION_TYPE=x11
sleep 4
! logged "no tray on this desktop" || fail "on X11 with a tray the app says there is none"
env REEL_DESKTOP_DIR="$W/x11" APPIMAGE="$IMG" "$R/AppRun" --share-last >/dev/null 2>&1 || true
waitlog "an action from outside the app" 15 "--share-last did not reach the running app"
logged '"--share-last"\|action=--share-last\|action="--share-last"' || fail "the action reached is not --share-last"
"$R/AppRun" --section=status >/dev/null 2>&1 &
for _ in $(seq 1 30); do xdotool search --onlyvisible --name '^reel$' >/dev/null 2>&1 && break; sleep 1; done
xdotool search --onlyvisible --name '^reel$' >/dev/null 2>&1 || fail "no window of reel's on the screen"
stop
ok "on X11: the app starts from the AppImage, a tray is there, --share-last from a second start reaches the running app, its window opens"

python3 /work/desktop/ci/fake-portal.py > "$W/portal.txt" 2>&1 &
PORTAL=$!
for _ in $(seq 1 20); do grep -q "portal up" "$W/portal.txt" && break; sleep 0.5; done
start wayland XDG_SESSION_TYPE=wayland XDG_CURRENT_DESKTOP=KDE
for _ in $(seq 1 30); do [ "$(grep -c '^bound ' "$W/portal.txt")" = 4 ] && break; sleep 1; done
grep -q "^registered dev.lockoo.reel" "$W/portal.txt" || fail "the app did not register with the portal"
grep -q "^bound share-last CTRL+ALT+s$" "$W/portal.txt" && grep -q "^bound save-and-share CTRL+ALT+SHIFT+s$" "$W/portal.txt" \
  || fail "the shortcuts bound are not the settings' keys: $(grep '^bound' "$W/portal.txt" | tr '\n' ';')"
waitlog "hotkeys bound through the desktop's portal" 15 "the app did not take the portal's answer"
python3 /work/desktop/ci/fake-portal.py press share-last
waitlog "action=share-last\|action=\"share-last\"" 15 "the shortcut the desktop fired ran no action"
waitlog "no tray on this desktop" 15 "on Wayland without a StatusNotifier host the app does not say there is no tray"
stop
kill "$PORTAL" 2>/dev/null || true
ok "as on Wayland (KDE): registered with the portal, the four shortcuts bound with the settings' keys ($(grep '^bound' "$W/portal.txt" | cut -d' ' -f2,3 | tr '\n' ',' | sed 's/,$//')), a shortcut fired by the desktop runs its action; no tray, and the app says so"
