#!/usr/bin/env bash
set -euo pipefail
export DISPLAY="${DISPLAY:-:99}"
export HOME="${HOME:-/tmp/home}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/xdg-$(id -u)}"
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export GDK_BACKEND=x11
export PULSE_SERVER="unix:$XDG_RUNTIME_DIR/pulse/native"
mkdir -p "$HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
case "${1:-}" in
  shot)
    import -display "$DISPLAY" -window root "${2:?a file}"
    ;;
  run)
    shift
    Xvfb "$DISPLAY" -screen 0 "${XVFB_SCREEN:-1280x800}x24" -nolisten tcp >/tmp/xvfb.log 2>&1 &
    XVFB=$!
    for _ in $(seq 1 50); do xdpyinfo -display "$DISPLAY" >/dev/null 2>&1 && break; sleep 0.1; done
    pulseaudio --daemonize=yes --exit-idle-time=-1 --disallow-exit --load="module-null-sink sink_name=null" >/tmp/pulse.log 2>&1 || true
    trayer --edge top --align right --width 20 --widthtype percent --height 32 --transparent false --tint 0x15121a --SetDockType false >/tmp/trayer.log 2>&1 &
    TRAYER=$!
    set +e
    dbus-run-session -- "$@"
    CODE=$?
    set -e
    kill $TRAYER 2>/dev/null || true
    pulseaudio --kill 2>/dev/null || true
    kill $XVFB 2>/dev/null || true
    exit $CODE
    ;;
  *)
    echo "usage: $0 run <command...> | shot <file.png>"; exit 1 ;;
esac
