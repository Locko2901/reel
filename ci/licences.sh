#!/bin/sh
# Every crate of both workspaces under a licence deny.toml allows; runs in the check container.
set -eu
V=0.20.2
SUM=9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
curl -fsSL -o "$T/deny.tgz" "https://github.com/EmbarkStudios/cargo-deny/releases/download/$V/cargo-deny-$V-x86_64-unknown-linux-musl.tar.gz"
echo "$SUM  $T/deny.tgz" | sha256sum -c - >/dev/null
tar xzf "$T/deny.tgz" -C "$T" --strip-components=1
cd "$(dirname "$0")/.."
"$T/cargo-deny" --locked check licenses
(cd desktop && "$T/cargo-deny" --locked check licenses)
