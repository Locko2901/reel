#!/usr/bin/env bash
# Both Windows installers, the one with OBS (the pinned OBS as a resource) and the one without,
# named as desktop/packages.json says, into /out. REEL_INSTALLERS names the ones to build
# (with-obs, plain), both when empty. Runs from /work/desktop/app.
set -euo pipefail
. ../ci/packages.sh
NSIS=/target/x86_64-pc-windows-msvc/release/bundle/nsis
# nsis/installer.nsi is Tauri's template with reel's pages: another tauri-cli's is merged in first.
cli=$(cargo tauri --version | awk '{print $2}')
grep -q "^; Tauri's installer.nsi of tauri-cli $cli " nsis/installer.nsi \
  || { echo "nsis/installer.nsi is from another tauri-cli than $cli: merge its template first"; exit 1; }
args=(--runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis)
[ -n "${DESKTOP_VERSION:-}" ] && args+=(--config "{\"version\":\"$DESKTOP_VERSION\"}")
[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ] && args+=(--config '{"bundle":{"createUpdaterArtifacts":true}}')
build() {
  rm -f "$NSIS"/*-setup.exe*
  cargo tauri build "${args[@]}" "$@"
}
# Tauri names both reel_<version>_x64-setup.exe, after the product
put() {
  for f in "$NSIS"/reel_*-setup.exe*; do cp "$f" "/out/$(basename "$f" | sed "s/^reel_/${1}_/")"; done
}
# reel-stop.exe runs before an installer touches the install folder; reel-elevate.exe goes beside the app.
cargo xwin build --release --target x86_64-pc-windows-msvc -p reel-desktop-core --bin reel-stop --bin reel-elevate
cp /target/x86_64-pc-windows-msvc/release/reel-stop.exe nsis/reel-stop.exe
cp /target/x86_64-pc-windows-msvc/release/reel-elevate.exe nsis/reel-elevate.exe
want=" ${REEL_INSTALLERS:-plain with-obs} "
if [[ $want == *" plain "* ]]; then
  build
  put "$PLAIN_FILE"
fi
if [[ $want == *" with-obs "* ]]; then
  ../ci/fetch-obs.sh windows obs
  build --config tauri.with-obs.conf.json
  put "$WITH_OBS_FILE"
  rm -rf obs /target/x86_64-pc-windows-msvc/release/obs
fi
cd /out
for f in *-setup.exe; do sha256sum "$f" > "$f.sha256"; done
ls -l /out
