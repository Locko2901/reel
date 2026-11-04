#!/bin/sh
# Runs as root only to join the render device's group, whatever gid the host gives it, so no
# gid is written into the compose file; then runs reel as reel.
set -eu
if [ "$(id -u)" != "0" ]; then
  exec reel "$@"
fi
for dev in /dev/dri/*; do
  [ -c "$dev" ] || continue
  gid="$(stat -c %g "$dev")"
  [ "$gid" = "0" ] && continue
  name="$(getent group "$gid" | cut -d: -f1 || true)"
  if [ -z "$name" ]; then
    name="dri$gid"
    groupadd -g "$gid" "$name"
  fi
  usermod -aG "$name" reel
done
chown reel:reel /data 2>/dev/null || true
exec gosu reel reel "$@"
