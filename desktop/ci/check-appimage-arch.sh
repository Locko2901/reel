#!/usr/bin/env bash
# The AppImage on Arch Linux under X11, with Arch's OBS beside the user's, and under Wayland.
#   check-appimage-arch.sh <the AppImage> <folder for the screenshot>
set -eu
IMG="$1" SHOTS="$2"
W=$(mktemp -d)
export HOME="$W/home" XDG_RUNTIME_DIR="$W/run" NO_AT_BRIDGE=1
mkdir -p "$HOME" "$XDG_RUNTIME_DIR" "$SHOTS"
chmod 700 "$XDG_RUNTIME_DIR"
fail() { printf 'FAIL %s\n' "$*"; tail -20 "$W"/*.out 2>/dev/null; cat "$W"/*/data/logs/reel-desktop.log* 2>/dev/null | tail -15; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
(cd "$W" && "$IMG" --appimage-extract >/dev/null)
R="$W/squashfs-root"
app_of() { # app_of <folder>: the app's own process, below dbus-run-session, by its REEL_DESKTOP_DIR
  for p in $(pgrep -x reel-desktop); do tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | grep -qx "REEL_DESKTOP_DIR=$1" && echo "$p"; done | head -1
}
home() { # home <name>: settings of its own, a server nobody answers at, so the app shows its settings
  mkdir -p "$W/$1/config"
  printf '{"server_url":"","device_name":"arch","obs":{"enabled":false},"startup":{"autostart":false},"log_level":"debug"}' > "$W/$1/config/settings.json"
}

Xvfb :95 -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &
XVFB=$!
sleep 1
home x11
DISPLAY=:95 XDG_SESSION_TYPE=x11 REEL_DESKTOP_DIR="$W/x11" APPIMAGE="$IMG" dbus-run-session -- "$R/AppRun" > "$W/x11.out" 2>&1 &
APP=$!
for _ in $(seq 1 60); do DISPLAY=:95 xdotool search --onlyvisible --name '^reel$' >/dev/null 2>&1 && break; sleep 1; done
DISPLAY=:95 xdotool search --onlyvisible --name '^reel$' >/dev/null 2>&1 || fail "X11: no window of reel's"
sleep 6
kill -0 "$APP" 2>/dev/null || fail "X11: the app ended"
DISPLAY=:95 import -window root "$SHOTS/arch-x11.png"
colours=$(convert "$SHOTS/arch-x11.png" -format '%k' info:)
[ "$colours" -gt 50 ] || fail "X11: the window is blank ($colours colours)"
kill "$(app_of "$W/x11")" "$APP" 2>/dev/null || true
ok "X11 on Arch: the AppImage starts, its window shows and WebKitGTK draws the page ($colours colours in arch-x11.png)"

reel_obs() { # reel's OBS: the one whose settings folder is reel's, under either name
  for p in $(pgrep -x reel-capture; pgrep -x obs); do tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | grep -qx "XDG_CONFIG_HOME=$W/obs/data/obs" && echo "$p"; done | head -1
}
seen() { # seen <pid>: what Discord reads of a process: its comm, argv[0] and exe
  printf 'comm %s, argv[0] %s, exe %s' "$(cat "/proc/$1/comm")" "$(tr '\0' '\n' < "/proc/$1/cmdline" | head -1)" "$(readlink "/proc/$1/exe")"
}
MAIN_CONF="$HOME/.config/obs-studio"
main_sum() { (cd "$MAIN_CONF" && find . -type f ! -path './logs/*' ! -path './profiler_data/*' ! -path './crashes/*' ! -path './.sentinel/*' -print0 | sort -z | xargs -0 sha256sum); }
mkdir -p "$MAIN_CONF" "$W/obs/config"
printf '[General]\nEnableAutoUpdates=false\nInfoIncrement=-1\n' > "$MAIN_CONF/global.ini"
printf '[General]\nFirstRun=true\nConfirmOnExit=false\n' > "$MAIN_CONF/user.ini"
DISPLAY=:95 dbus-run-session -- obs --disable-updater > "$W/main-obs.out" 2>&1 &
MAIN=$!
for _ in $(seq 1 60); do ls "$MAIN_CONF"/basic/profiles/*/basic.ini >/dev/null 2>&1 && break; sleep 1; done
ls "$MAIN_CONF"/basic/profiles/*/basic.ini >/dev/null 2>&1 || fail "the user's OBS did not start: $(tail -3 "$W/main-obs.out")"
sleep 6
BEFORE=$(main_sum)
cat > "$W/obs/config/settings.json" <<JSON
{"server_url":"","device_name":"arch","obs":{"bundled":true,"start_replay":true,
 "setup":{"done":true,"clip_seconds":20,"quality":"smooth","encoder":"obs_x264","game":false,"screen":{"id":"0","width":1280,"height":800,"hz":30},"format":"mkv"}},
 "clips_dir":"$W/obs/clips","startup":{"autostart":false},"log_level":"debug"}
JSON
DISPLAY=:95 XDG_SESSION_TYPE=x11 REEL_DESKTOP_DIR="$W/obs" APPIMAGE="$IMG" dbus-run-session -- "$R/AppRun" --hidden > "$W/obs.out" 2>&1 &
APP=$!
OLOG="$W/obs/data/logs"
for _ in $(seq 1 90); do grep -qs "obs connected" "$OLOG"/reel-desktop.log* && break; sleep 1; done
grep -qs "reel's own OBS is the one installed here.*/usr/bin/obs" "$OLOG"/reel-desktop.log* || fail "the app did not find Arch's OBS"
grep -qs "obs connected" "$OLOG"/reel-desktop.log* || fail "the app did not connect to its OBS"
OURS=$(reel_obs)
[ -n "$OURS" ] && [ "$OURS" != "$MAIN" ] || fail "no OBS of reel's beside the user's"
kill -0 "$MAIN" 2>/dev/null || fail "the user's OBS stopped"
grep -qs "your own OBS is running" "$OLOG"/reel-desktop.log* && fail "reel's OBS waited for the user's"
python3 -c 'import json,sys; c=json.load(open(sys.argv[1])); assert any(s["id"]=="xshm_input" for s in c["sources"])' "$W/obs/data/obs/obs-studio/basic/scenes/reel.json" || fail "reel's scene collection"
OBSV=$(grep -h -o 'obs_version="[^"]*"' "$OLOG"/reel-desktop.log* | tail -1)
THEIRS=$(for p in $(pgrep -x obs); do [ "$p" != "$OURS" ] && echo "$p"; done | head -1)
OURS_SEEN=$(seen "$OURS") MAIN_SEEN=$(seen "$THEIRS")
[ "$(cat "/proc/$OURS/comm")" = reel-capture ] && [ "$(basename "$(readlink "/proc/$OURS/exe")")" = reel-capture ] \
  && [ "$(basename "$(tr '\0' '\n' < "/proc/$OURS/cmdline" | head -1)")" = reel-capture ] || fail "reel's OBS still shows as obs: $OURS_SEEN"
[ "$(sha256sum < "$(readlink "/proc/$OURS/exe")")" = "$(sha256sum < /usr/bin/obs)" ] || fail "reel's OBS is not the installed program: $(readlink "/proc/$OURS/exe")"
kill -TERM "$(app_of "$W/obs")"
for _ in $(seq 1 30); do [ -z "$(reel_obs)" ] && [ -z "$(app_of "$W/obs")" ] && break; sleep 1; done
[ -z "$(reel_obs)" ] || fail "reel's OBS still runs after the app quit"
kill -0 "$MAIN" 2>/dev/null || fail "the user's OBS stopped with the app"
[ "$BEFORE" = "$(main_sum)" ] || fail "the user's OBS settings changed: $(diff <(echo "$BEFORE") <(main_sum) | head -5)"
kill "$MAIN" 2>/dev/null || true
kill "$XVFB" 2>/dev/null || true
ok "Arch's OBS ($OBSV) as reel's own: found at /usr/bin/obs, run a second time beside the user's OBS, connected, stopped with the app; the user's OBS ran on, its $(echo "$BEFORE" | wc -l) settings files the same to the byte"
ok "what Discord reads of a process: reel's OBS, a byte-for-byte copy of /usr/bin/obs, $OURS_SEEN; the user's OBS $MAIN_SEEN"

home wayland
cat > "$W/wayland/config/settings.json" <<JSON
{"server_url":"","device_name":"arch","obs":{"bundled":true,"start_replay":true,
 "setup":{"done":true,"clip_seconds":20,"quality":"smooth","encoder":"obs_x264","game":false,"screen":{"id":"pipewire","width":1280,"height":800,"hz":30},"format":"mkv"}},
 "clips_dir":"$W/wayland/clips","startup":{"autostart":false},"log_level":"debug"}
JSON
weston --backend=headless --renderer=pixman --socket=wayland-reel --idle-time=0 > "$W/weston.out" 2>&1 &
WESTON=$!
for _ in $(seq 1 20); do [ -S "$XDG_RUNTIME_DIR/wayland-reel" ] && break; sleep 0.5; done
[ -S "$XDG_RUNTIME_DIR/wayland-reel" ] || fail "Weston did not start"
WAYLAND_DISPLAY=wayland-reel XDG_SESSION_TYPE=wayland GDK_BACKEND=wayland REEL_DESKTOP_DIR="$W/wayland" APPIMAGE="$IMG" \
  dbus-run-session -- "$R/AppRun" > "$W/wayland.out" 2>&1 &
APP=$!
for _ in $(seq 1 60); do grep -qs "reel desktop starts" "$W/wayland/data/logs/"reel-desktop.log* && break; sleep 0.5; done
grep -qs "reel desktop starts" "$W/wayland/data/logs/"reel-desktop.log* || fail "Wayland: the app did not start"
sleep 10
kill -0 "$APP" 2>/dev/null || fail "Wayland: the app ended"
! grep -qi "protocol error\|EGL_BAD\|could not create default EGL display" "$W/wayland.out" || fail "Wayland: $(grep -i 'protocol error\|EGL' "$W/wayland.out" | head -2)"
python3 -c 'import json,sys; c=json.load(open(sys.argv[1])); s=[s for s in c["sources"] if s["name"]=="Screen"][0]; assert s["id"]=="pipewire-screen-capture-source", s["id"]' "$W/wayland/data/obs/obs-studio/basic/scenes/reel.json" || fail "Wayland: reel's OBS does not take the screen through PipeWire"
grep -q '^SysTrayEnabled=true$' "$W/wayland/data/obs/obs-studio/user.ini" || fail "Wayland: reel's OBS does not keep itself in its tray"
kill "$APP" 2>/dev/null || true
pkill -x obs 2>/dev/null || true
pkill -x reel-capture 2>/dev/null || true
kill "$WESTON" 2>/dev/null || true
ok "Wayland on Arch (Weston): the AppImage starts and stays up, no protocol error, no EGL error; reel's OBS takes the screen through PipeWire and keeps itself in its own tray"
