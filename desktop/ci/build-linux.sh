#!/bin/sh
set -eu
VERSION="${DESKTOP_VERSION:-$(sed -n 's/^  "version": "\(.*\)",/\1/p' tauri.conf.json)}"
TOOLS="${XDG_CACHE_HOME:-$HOME/.cache}/tauri"
mkdir -p "$TOOLS"
# The bundler's AppImage tools, cached and checked first so it downloads nothing.
tool() { # tool <name in the cache> <url> <sha256>
  if [ -f "$TOOLS/$1" ] && echo "$3  $TOOLS/$1" | sha256sum -c - >/dev/null 2>&1; then
    return 0
  fi
  curl -fsSL --retry 3 -o "$TOOLS/$1.part" "$2"
  echo "$3  $TOOLS/$1.part" | sha256sum -c - >/dev/null || { echo "$1 is not the pinned build"; rm -f "$TOOLS/$1.part"; exit 1; }
  chmod 755 "$TOOLS/$1.part"
  mv "$TOOLS/$1.part" "$TOOLS/$1"
}
tool AppRun-x86_64 \
  https://github.com/tauri-apps/binary-releases/releases/download/apprun-old/AppRun-x86_64 \
  f30140a43a0a59e46db21bdefdf749b9e9f2c6946e92afabbacf98b8ae73fb4f
tool linuxdeploy-07333c6-x86_64.AppImage \
  https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy-07333c6/linuxdeploy-x86_64.AppImage \
  36a2d7e274d12e1050d0e9ecfe11d339ed54720b2bec464c286d53f8b07f5c62
tool linuxdeploy-plugin-appimage.AppImage \
  https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/1-alpha-20250213-1/linuxdeploy-plugin-appimage-x86_64.AppImage \
  992d502a248e14ab185448ddf6f6e7d25558cb84d4623c354c3af350c25fccb3

# A dev version keeps line tables so a panic's backtrace names each frame; linuxdeploy strips
# what it bundles unless NO_STRIP is set.
case "$VERSION" in
  *-dev.*) export CARGO_PROFILE_RELEASE_DEBUG=line-tables-only CARGO_PROFILE_RELEASE_STRIP=none NO_STRIP=1 ;;
esac

cargo tauri build --bundles appimage,deb --config "{\"version\":\"$VERSION\"}"
B="${CARGO_TARGET_DIR:-../target}/release/bundle"
cp "$B/appimage/reel_${VERSION}_amd64.AppImage" "$B/deb/reel_${VERSION}_amd64.deb" /out/
cd /out
for f in "reel_${VERSION}_amd64.AppImage" "reel_${VERSION}_amd64.deb"; do
  if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    cargo tauri signer sign --app-version "$VERSION" "$f"
  fi
  sha256sum "$f" > "$f.sha256"
done
ls -l "reel_${VERSION}_amd64."*
