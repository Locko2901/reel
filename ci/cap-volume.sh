#!/usr/bin/env bash
# Wipes a cargo target volume over a cap before a build adds to it: a full disk takes down
# every job on the runner. Usage: ci/cap-volume.sh <volume> <cap in MB> <image with du and rm>
set -euo pipefail
VOLUME="${1:?volume name}"
CAP_MB="${2:?cap in MB}"
IMAGE="${3:?image}"
if ! docker volume inspect "$VOLUME" >/dev/null 2>&1; then
  echo "cap: $VOLUME does not exist yet"
  exit 0
fi
USED_MB=$(docker run --rm -v "$VOLUME":/v "$IMAGE" sh -c 'du -sm /v 2>/dev/null | cut -f1' | tr -d '[:space:]')
USED_MB="${USED_MB:-0}"
if [ "$USED_MB" -gt "$CAP_MB" ]; then
  echo "cap: $VOLUME holds ${USED_MB} MB, over the cap of ${CAP_MB} MB; wiping it"
  docker run --rm -v "$VOLUME":/v "$IMAGE" sh -c 'rm -rf /v/* /v/.[!.]* 2>/dev/null || true'
else
  echo "cap: $VOLUME holds ${USED_MB} MB of ${CAP_MB} MB"
fi
