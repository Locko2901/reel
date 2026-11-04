#!/bin/sh
# The proofs and the app's checks that talk to a demo default to the one scripts/demo.sh serves.
set -eu
cd "$(dirname "$0")/.."
port=$(sed -n 's/^export REEL_LISTEN="${REEL_LISTEN:-127\.0\.0\.1:\([0-9]*\)}"$/\1/p' scripts/demo.sh)
[ -n "$port" ] || { echo "scripts/demo.sh names no port to listen on"; exit 1; }
other=$(grep -noE '127\.0\.0\.1:[0-9]{4,}|reel demo [0-9]{4,}|demo on :[0-9]{4,}' Makefile ci/*.sh desktop/ci/*.sh | grep -vE "[^0-9]$port\$" || true)
if [ -n "$other" ]; then
  echo "the demo serves on $port; these default to another port:"
  echo "$other"
  exit 1
fi
echo "every default demo address is the demo's, port $port"
