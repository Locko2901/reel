#!/usr/bin/env bash
# A local demo: built-in accounts, CPU encoding, synthetic clips.
# Usage: scripts/demo.sh [serve|seed|doctor|stop|status]
set -euo pipefail
cd "$(dirname "$0")/.."
export REEL_LISTEN="${REEL_LISTEN:-127.0.0.1:8746}"
export REEL_PUBLIC_URL="${REEL_PUBLIC_URL:-http://127.0.0.1:8746}"
export REEL_DATA_DIR="${REEL_DATA_DIR:-$PWD/data}"
export REEL_INCOMING_DIR="${REEL_INCOMING_DIR:-$PWD/data/incoming}"
export REEL_ORIGINALS_DIR="${REEL_ORIGINALS_DIR:-$PWD/data/originals}"
export REEL_AUTH_MODE=local
export REEL_DEMO_USER="${REEL_DEMO_USER:-sam}"
export REEL_DEMO_PASSWORD="${REEL_DEMO_PASSWORD:-reel demo 8746}"
export REEL_FFMPEG="${REEL_FFMPEG:-$HOME/.local/ffmpeg/ffmpeg}"
export REEL_FFPROBE="${REEL_FFPROBE:-$HOME/.local/ffmpeg/ffprobe}"
export REEL_HWACCEL="${REEL_HWACCEL:-none}"
export REEL_POLL_SECONDS="${REEL_POLL_SECONDS:-3}"
export RUST_LOG="${RUST_LOG:-info,sqlx=warn,tower_http=warn}"
mkdir -p "$REEL_DATA_DIR" "$REEL_INCOMING_DIR" "$REEL_ORIGINALS_DIR"
BIN="${REEL_BIN:-target/release/reel}"
PIDFILE="$REEL_DATA_DIR/reel.pid"
LOG="$REEL_DATA_DIR/reel.log"
PORT="${REEL_LISTEN##*:}"
# Set at every start, so a changed REEL_DEMO_PASSWORD takes.
demo_admin() {
  REEL_PASSWORD="$REEL_DEMO_PASSWORD" "$BIN" passwd --make-admin "$REEL_DEMO_USER" | tail -1
}
case "${1:-serve}" in
  build) cargo build --release -p reel --features style-page ;;
  bin) echo "$BIN" ;;
  serve)
    [ -x "$BIN" ] || cargo build --release -p reel --features style-page
    if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then echo "already running, pid $(cat "$PIDFILE")"; exit 0; fi
    demo_admin
    setsid nohup "$BIN" serve >>"$LOG" 2>&1 &
    echo $! > "$PIDFILE"
    sleep 1
    echo "reel serving on $REEL_LISTEN (pid $(cat "$PIDFILE")), log $LOG"
    ;;
  stop) [ -f "$PIDFILE" ] && kill "$(cat "$PIDFILE")" 2>/dev/null && rm -f "$PIDFILE" && echo stopped || echo "not running" ;;
  status) curl -fsS "http://127.0.0.1:${REEL_LISTEN##*:}/healthz" && echo ;;
  seed) [ -x "$BIN" ] || cargo build --release -p reel --features style-page; "$BIN" seed --count "${2:-8}" ;;
  doctor) [ -x "$BIN" ] || cargo build --release -p reel --features style-page; "$BIN" doctor ;;
  passwd) demo_admin ;;
  *) echo "usage: $0 [build|serve|stop|status|seed [n]|doctor|passwd]"; exit 1 ;;
esac
