#!/bin/sh
# The licence page of the server and the app (cargo-about), into $1. With --third-party, THIRD_PARTY.md's
# crate list from the same crates. Fails when a crate has no licence text cargo-about can find.
set -eu
V=0.9.3
SUM=3ad1e94ea1e63d69d1961f4fb15ccbbe9575d906111f96d2cb4dce6df48f71b4
LIST=
[ "${1:-}" = --third-party ] && { LIST=--third-party; shift; }
OUT="${1:?where the page goes}"
case "$OUT" in /*) ;; *) OUT="$PWD/$OUT" ;; esac
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
curl -fsSL --retry 3 -o "$T/about.tgz" "https://github.com/EmbarkStudios/cargo-about/releases/download/$V/cargo-about-$V-x86_64-unknown-linux-musl.tar.gz"
echo "$SUM  $T/about.tgz" | sha256sum -c - >/dev/null
tar xzf "$T/about.tgz" -C "$T" --strip-components=1
cd "$(dirname "$0")/.."
ROOT=$PWD
# about.toml serves both workspaces, so each run warns of the other's crates in it: those go unsaid.
about() { # about <workspace> <json>
  if ! (cd "$1" && "$T/cargo-about" -c never generate --locked --all-features -c "$ROOT/about.toml" --format json -o "$2") 2>"$T/log"; then
    cat "$T/log" >&2
    exit 1
  fi
  awk '/config found for a crate not present in the graph/ { skip = 1 } skip && /^$/ { skip = 0; next } !skip' "$T/log" >&2
}
about . "$T/server.json"
about desktop "$T/app.json"
python3 ci/licence-page.py $LIST --tool "cargo-about $V" "$OUT" "$T/server.json" "$T/app.json"
