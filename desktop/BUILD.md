# Building the desktop app

Everything about the desktop app builds and is checked in one container,
`desktop/ci/desktop.Dockerfile`, driven by the `desktop-*` targets of the root Makefile. The
host needs Docker, make and git: nothing is installed on the host and nothing needs sudo. The
Windows installers are cross compiled on Linux, and the checks run the app under Xvfb and the
installers under Wine.

## Quick start

```
make desktop-check       # fmt, clippy with warnings as errors, the tests
make desktop-windows     # both Windows installers, into ~/reel-desktop-out/
make desktop-appimage    # the AppImage and the .deb, into ~/reel-desktop-out/
```

The first run builds the image and compiles from cold, which takes a while (see
[What it takes](#what-it-takes)). `make` alone lists the targets, a line on each.

## Targets

### Builds

| Target | What | Where it lands |
|---|---|---|
| `make desktop-image` | the container image `reel-desktop:local` | Docker |
| `make desktop-windows` | both Windows installers: the app cross compiled with cargo-xwin, bundled with NSIS | `$DESKTOP_OUT`: `reel_<version>_x64-setup.exe` (reel, with OBS) and `reel-light_<version>_x64-setup.exe` (reel light), each with its `.sha256` |
| `make desktop-appimage` | the Linux app from one release build: the AppImage and the .deb | `$DESKTOP_OUT`: `reel_<version>_amd64.AppImage` and `reel_<version>_amd64.deb`, each with its `.sha256` |
| `make desktop-dev` | both Windows installers, the AppImage and the .deb as version `<version>-dev.<hash>`; the Status page shows the build as `dev.<hash>` | `$DESKTOP_OUT` |
| `make desktop-linux` | a Linux debug build of the app, and the fake OBS the checks under Xvfb use | the `reel-desktop-target` volume: `/target/debug/reel-desktop` in the container |
| `make desktop-stop-helper` | the installers' helpers `reel-stop.exe` and `reel-elevate.exe`, and the stand-ins their checks run, built for Windows | `desktop/app/nsis/` and `desktop/.wine-standins/` (both ignored by git) |
| `make desktop-obs-sources` | OBS's source archive of the pinned version, which a release that ships OBS carries | `$DESKTOP_OUT` |
| `make desktop-obs-image` | the image with the real OBS, `reel-obs-e2e:local` | Docker |
| `make desktop-shell` | a shell in the container, the worktree at `/work` | |

### Checks of the app

| Target | What | Needs a demo server |
|---|---|---|
| `make desktop-check` | `desktop/ci/check-commands.sh`, `desktop/ci/changes-test.sh`, `cargo fmt --check`, `cargo clippy -D warnings` and `cargo test` for the desktop workspace | no |
| `make desktop-check-windows` | clippy for Windows, cross compiled, so Windows-only code is checked without building an installer | no |
| `make desktop-appimage-check` | the AppImage and the .deb in `$DESKTOP_OUT`: what each holds, and the app under Xvfb as on X11 and as on Wayland | no |
| `make desktop-appimage-arch` | the AppImage on Arch Linux under X11 and Wayland, then its video and sound on an Arch without GStreamer | yes, for the second part |
| `make desktop-obs-e2e` | the debug build with the real OBS under Xvfb ([below](#the-run-with-the-real-obs)) | no |
| `make desktop-e2e` | the AppImage under Xvfb: uploads, hotkeys, Save and share, This app's pages, standalone, a server out of reach ([below](#the-end-to-end-run)) | yes |
| `make desktop-save-actions` | Save clip and Save and share from every way in: the tray, the header, the hotkey, the command line | no |
| `make desktop-quit` | the tray's Quit with each step timed, and a start right after it | no |
| `make desktop-standalone` | standalone mode: a new install, an app set up standalone with no server, a paired settings file with no mode | no |
| `make desktop-presses` | Save and share pressed three times in a row, and a double tap | yes |
| `make desktop-waited-upload` | an upload someone waits for goes past the pause and the queue | yes |
| `make desktop-notify-buttons` | Copy link and Open on a clip's notification, paired and standalone | yes |
| `make desktop-downloads` | a download in the window says so in the page | yes |
| `make desktop-zip-download` | the Select bar's Download saves its zip through the app's download hook | yes |
| `make desktop-back` | the way back in the window: the Library button, the mouse's back and forward buttons, Alt+Left and Alt+Right | yes |
| `make desktop-shots` | screenshots of the window at 1280x800 and 720x850, and the offline page | yes |

### Checks of the Windows installers

These run the installers and their helpers under Wine. They work on the installers in
`$DESKTOP_OUT`, so build them first.

| Target | What |
|---|---|
| `make desktop-installers-check` | what each installer holds, and both installed under Wine, silently, as the updater and a switch between them run them |
| `make desktop-obs-start-check` | reel installed under Wine starts its OBS: fresh, with Capture games that run as administrator on, and as an update over the newest older release, with the flags that release's app passes |
| `make desktop-previous` | that older release's installer with OBS, from GitHub into `$DESKTOP_PREVIOUS` as `with-obs_<version>_x64-setup.exe`, for the check above |
| `make desktop-obs-fresh` | the installers' hooks under Wine, in small installers made from them |
| `make desktop-elevate-check` | `reel-elevate.exe` under Wine, as far as Wine goes |
| `make desktop-obs-window-check` | Open OBS and Hide OBS on the real OBS under Wine |
| `make desktop-start-check` | the app of reel, with OBS, started under Wine over an older copy in Program Files |
| `make desktop-installer-shots` | every page of both installers as a PNG, into `shots/installer/` |
| `make desktop-installer-audit` | the same at 100, 125 and 150 %, into `shots/installer-audit/` |
| `make desktop-release-update` | a standalone app's update from a stand-in release: the app's test built for Windows finds reel light through reel on this PC, refuses it forged, and runs it signed as an update does; the installer installs and starts reel again |

`desktop-installer-shots` and `desktop-installer-audit` need `DESKTOP_VERSION` set to the
version in the installers' names. `desktop-release-update` needs reel light of `DESKTOP_VERSION`
signed: build it with `TAURI_SIGNING_PRIVATE_KEY` set (a key of your own made with `cargo tauri
signer generate` does), and name that key's `.pub`, put in `$DESKTOP_OUT`, in `RELEASE_PUBKEY`
(as `/out/<name>.pub`) unless it is the release key. With `UPDATE_FROM=github` it takes reel
light from GitHub's newest release instead, checked against the release key.

### Variables

| Variable | Default | What it sets |
|---|---|---|
| `DESKTOP_OUT` | `~/reel-desktop-out` | where the installers, the AppImage and the .deb land (`/out` in the container) |
| `DESKTOP_VERSION` | empty: the version in `desktop/app/tauri.conf.json` | the version the builds carry and the checks look for |
| `DESKTOP_INSTALLERS` | `plain with-obs` | which Windows installers `desktop-windows` builds, by kind: `with-obs` is reel, `plain` reel light |
| `CARGO_BUILD_JOBS` | `4` | cargo's parallel jobs in the container |
| `DESKTOP_TARGET_CAP_MB` | `8000` | the cap of the `reel-desktop-target` volume ([below](#the-volumes-and-their-caps)) |
| `DESKTOP_PREVIOUS` | `~/reel-desktop-previous` | where `desktop-previous` puts the older release |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | unset | when set, the builds are signed for the app's updater: a `.sig` beside each file |
| `REEL_E2E_SERVER`, `REEL_E2E_USER`, `REEL_E2E_PASSWORD` | `http://127.0.0.1:8746`, `sam`, `reel demo 8746` | the demo server the checks talk to, as `scripts/demo.sh serve` runs it; the user must be an admin |
| `REEL_E2E_OBS_PASSWORD` | empty | when set, `desktop-obs-e2e` sends its clip to the demo server |
| `DESKTOP_SHOTS_OUT`, `INSTALLER_SHOTS`, `INSTALLER_AUDIT` | `shots/desktop`, `shots/installer`, `shots/installer-audit` | where the screenshot targets write |
| `GITHUB_TOKEN` | unset | when set, `desktop-previous` calls GitHub's API with it |

`CARGO_BUILD_JOBS` matters on a small machine: with less than 8 GB of RAM, lower it (to 2 or
3) to keep the build out of swap; with more memory and cores, raise it.

## The image

`rust:1-bookworm` (Debian 12) plus:

- the Linux build dependencies of Tauri 2 (WebKitGTK 4.1, GTK 3, the Ayatana appindicator for
  the tray, librsvg, libsoup 3, patchelf), so the Linux app builds and runs under Xvfb and
  `cargo clippy` sees the real crates;
- `rustfmt`, `clippy` and the target `x86_64-pc-windows-msvc`;
- clang, lld and llvm (cargo-xwin compiles C with `clang-cl` and links with `lld-link`),
  `cargo-xwin` and the Tauri CLI (`cargo tauri`), each as the prebuilt binary of its release,
  pinned by version in the Dockerfile;
- NSIS from Debian for the installers, and 7-Zip to list what each holds;
- Wine 11.0 from WineHQ's repository for the installers' checks, pinned, its signing key
  checked by its fingerprint (Debian's Wine 8.0 lacks `bcryptprimitives.dll`, which a Rust
  program for Windows imports);
- Xvfb, xauth, x11-utils, xdotool, ImageMagick (`import` for screenshots), `trayer` as the tray
  host, D-Bus for the session bus the tray and WebKitGTK want, PulseAudio and alsa-utils for a
  null sink to play the cues into, and Python's D-Bus bindings for a stand-in desktop portal;
- the GStreamer plugins the AppImage carries for WebKitGTK's video and sound;
- ffmpeg and a font for synthetic clips.

`XWIN_ACCEPT_LICENSE=1` is set in the image. cargo-xwin downloads Microsoft's CRT and Windows
SDK, which come under Microsoft's licence: building the Windows app means accepting it. The
download lands once in the `reel-xwin-cache` volume (`/xwin`) and is reused.

## Running as the host user

Every target runs the container as the host user (`--user $(id -u):$(id -g)`), so what Tauri
writes in the worktree (`desktop/app/gen`) and the files copied to `$DESKTOP_OUT` belong to
you, not to root. Some things follow from that:

- The server's cargo volumes (`reel-cargo-registry`, `reel-cargo-git`) belong to root, as
  `make check` runs as root, so the desktop container has a volume of its own,
  `reel-desktop-cargo`, mounted at `/cargo-home` with `CARGO_HOME` pointing at it. The rust
  image's toolchain under `/usr/local/rustup` and `/usr/local/cargo/bin` is world readable
  and stays on `PATH`, so `cargo`, `cargo tauri` and `cargo xwin` work without a writable
  `/usr/local/cargo`.
- The mount points `/cargo-home`, `/target`, `/xwin`, `/out` and `/tmp/home` are made in the
  image with mode 1777. A new named volume takes the mode of its mount point on its first
  mount, so the host user can write into it.
- `HOME` is `/tmp/home` (PulseAudio, D-Bus and WebKitGTK want a writable home), and
  `XDG_CACHE_HOME` is in the cargo volume, so what the Tauri CLI downloads is kept between
  builds.
- The host's `/etc/passwd` and `/etc/group` are mounted read only: without an entry for your
  UID, `dbus-run-session` stops with "Could not get password database information for UID"
  and nothing under Xvfb starts.
- `make` creates `$DESKTOP_OUT` before the container mounts it. Docker would create a
  missing bind mount as root, and the container could not write into it.

## Under Xvfb

`desktop/ci/xvfb.sh run <command>` starts Xvfb on `:99` (1280x800 unless `XVFB_SCREEN` says
otherwise), a PulseAudio daemon with a null sink and `trayer` at the top right as the tray
host, then runs the command in `dbus-run-session` with `DISPLAY` set and the WebKitGTK
variables that keep it off the GPU. `desktop/ci/xvfb.sh shot <file.png>` takes a screenshot
of the display from another shell in the same container (`docker exec`).

To run the debug build of `make desktop-linux` by hand, from the top of the repository:

```
docker run --rm --name reel-xvfb --network host --user $(id -u):$(id -g) \
  -v reel-desktop-target:/target -v "$PWD":/work \
  -v /etc/passwd:/etc/passwd:ro -v /etc/group:/etc/group:ro \
  -e HOME=/tmp/home -e RUST_LOG=info -w /work/desktop reel-desktop:local \
  /work/desktop/ci/xvfb.sh run /target/debug/reel-desktop
```

and from another shell `mkdir -p shots && docker exec reel-xvfb /work/desktop/ci/xvfb.sh
shot /work/shots/xvfb.png`. Call the script by its absolute path: Docker does not resolve a
relative command against `-w`. The log says `reel desktop starts`. With no StatusNotifier
host on the bus, the tray icon falls back to an XEmbed icon, which is what trayer shows; on a
real desktop the tray is the system's own.

Harmless in the log: a warning about the missing accessibility bus (AT-SPI), and Gdk
assertions from WebKitGTK's first paint without a compositor.

## The Windows cross build

`make desktop-windows` fetches the pinned ffmpeg for Windows, then runs
`desktop/ci/build-windows.sh` in `desktop/app`:

```
cargo xwin build --release --target x86_64-pc-windows-msvc -p reel-desktop-core --bin reel-stop --bin reel-elevate
cargo tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
cargo tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis --config tauri.with-obs.conf.json
```

1. **cargo-xwin fetches the CRT and the Windows SDK** on its first run, about 1.2 GB into the
   `reel-xwin-cache` volume, then compiles with `clang-cl` and links with `lld-link`.
2. **`lld-link` warns LNK4099 for every CRT object** ("Cannot use debug info for
   libcmt.lib"): Microsoft's libraries refer to PDB files that are not in the SDK download.
   It is harmless, as the release build strips its symbols; rustc shows it once, as a
   `linker_messages` warning.
3. **The Tauri CLI downloads two files when it bundles**, kept in `XDG_CACHE_HOME`:
   `nsis_tauri_utils.dll`, Tauri's NSIS plugin, its hash checked, and
   `MicrosoftEdgeWebview2Setup.exe`, the WebView2 bootstrapper that
   `webviewInstallMode: embedBootstrapper` packs into the installer.
4. **Debian's NSIS 3.08 lacks `Win\RestartManager.nsh`**, which Tauri's installer script
   includes. The image adds that one header from NSIS 3.11, its SHA-256 checked in the
   Dockerfile; it only wraps the Restart Manager API with `System::Call`, so 3.08's
   `makensis` takes it.
5. **Two installers, one app.** The second build adds the pinned OBS
   (`desktop/ci/fetch-obs.sh`, its SHA-256 checked) as a resource, with the installer hooks of
   the package with OBS (`tauri.with-obs.conf.json`). Tauri names both after the product,
   `reel_<version>_x64-setup.exe`; `build-windows.sh` renames each as `desktop/packages.json`
   says: reel (with OBS) and reel light (without). The names people read and the files are
   in that one file, which the server, the app, the installers, the checks and the release job
   read. `DESKTOP_INSTALLERS` picks which of the two are built.
6. **The installer template follows the Tauri CLI.** `desktop/app/nsis/installer.nsi` is
   Tauri's NSIS template with reel's pages, and its first line names the tauri-cli it came
   from. `build-windows.sh` refuses another CLI version until the template is merged with
   that version's.
7. **Signed for the updater, not for Windows.** With `TAURI_SIGNING_PRIVATE_KEY` set, Tauri
   writes a `.sig` beside each installer. Nothing is Authenticode signed. The installers
   install per user (`installMode: currentUser`), with no UAC prompt.

If the build fails with no error beyond the linker warnings, run it once more before you dig:
that failure has been seen once and went away on the next run.

## The Linux build

`make desktop-appimage` fetches the pinned ffmpeg for Linux, then runs
`desktop/ci/build-linux.sh` in `desktop/app`: one `cargo tauri build --bundles appimage,deb`,
so the AppImage and the .deb hold the same app. The image is Debian 12, so both need glibc
2.36 or later: Arch, Fedora, Debian 12 and later, Ubuntu 24.04 and later, not Ubuntu 22.04.

- **The AppImage's tools are pinned.** Tauri's bundler takes AppRun, linuxdeploy and
  linuxdeploy's AppImage plugin from its cache (`XDG_CACHE_HOME/tauri`) and downloads what is
  missing. The script puts the three there first, each checked against the SHA-256 it pins,
  so nothing unpinned comes in. The bundler runs linuxdeploy with
  `--appimage-extract-and-run`, as the container has no FUSE.
- **What goes in:** both hold the app with the pinned ffmpeg and ffprobe beside it. The
  AppImage also carries WebKitGTK's processes and libraries, the tray's
  libayatana-appindicator, and GStreamer with its plugins; the .deb depends on the system's
  packages of those instead.
- **A dev version keeps its symbols.** With a `-dev.` version, as `make desktop-dev` gives
  it, the program keeps its line tables and linuxdeploy does not strip, so a panic run with
  `RUST_BACKTRACE=1` names its frames. A release is stripped.
- **Signed for the updater** as the Windows installers are: with
  `TAURI_SIGNING_PRIVATE_KEY` set, a `.sig` beside each.

## The ffmpeg sidecars

Standalone mode runs reel's own pipeline on the PC, so the app ships ffmpeg and ffprobe
beside it as Tauri sidecars. `desktop/ci/fetch-ffmpeg.sh` downloads the pinned build once
into the cargo volume, refuses it when its SHA-256 does not match, and puts ffmpeg, ffprobe
and ffmpeg's licence under `desktop/app/binaries/` (ignored by git) with the target triple in
the name, which is where `bundle.externalBin` in `tauri.conf.json` looks.

| Pin | Build |
|---|---|
| `desktop/ffmpeg/pinned.json` | Windows: gyan.dev's essentials build of an ffmpeg release, static, GPL v3, with libx264, NVENC, AMF and QSV; the checksum is the one gyan.dev publishes |
| `desktop/ffmpeg/pinned-linux.json` | Linux: BtbN's GPL build of the same release, static but for glibc, from a month's last build, which BtbN keeps; the checksum is the one GitHub publishes |

Debian's own ffmpeg would not do for the Linux app: it links Debian 12's libraries and would
break on any other distribution. To move to a newer ffmpeg, change the URL, the version and
the checksum in the pin.

## No console windows

The release app is a windowed program (`windows_subsystem = "windows"` in
`desktop/app/src/main.rs`), so a console program it starts (schtasks, ffmpeg, ffprobe) would
flash a console window unless started with `CREATE_NO_WINDOW`. Every process reel and the app
start goes through `reel_core::proc` (`crates/core/src/proc.rs`), which sets that flag on
Windows and leaves the pipes as the caller sets them; async callers wrap the result in tokio's
`Command`. `desktop/ci/check-commands.sh` fails on a `Command::new` or a `creation_flags`
anywhere else in the product's sources. It also fails when a Tauri command of the app has no
permission in `desktop/app/build.rs` or is granted to no page. It runs first in
`make desktop-check` and in `make check`.

## The end to end run

`make desktop-e2e` builds the AppImage and the Linux debug build (for the fake OBS), then
runs `desktop/ci/e2e.sh` under `xvfb.sh` against a reel demo server. It extracts the AppImage
and runs it from there, as a desktop without FUSE does. It signs in, makes a pairing code,
pairs as the app would and keeps the token as the Linux app keeps it (a file with mode 0600).
It starts the app hidden with `REEL_DESKTOP_DIR` pointing at a scratch folder, drops a
synthetic clip into the watched folder, waits for the upload to be confirmed and the local
copy to go (cache 0 GB), presses Share the last clip's hotkey (Ctrl+Alt+S) through xdotool
and checks that the link answers 200. Then it runs Save and share with the fake OBS, records a
hotkey, walks This app's pages with Back and Forward, checks the cues in the log (played
through `paplay` into the null sink), and covers standalone mode, a server out of reach, a
certificate for another name and a server answering 502. Its screenshots land in
`shots/desktop-e2e/`.

The checks that need a demo server talk to `REEL_E2E_SERVER` as `REEL_E2E_USER`, an admin.
The defaults are the demo `scripts/demo.sh serve` (or `make demo`) runs on 127.0.0.1:8746, so
with it running `make desktop-e2e` needs nothing more. Point them at another demo with the
three variables:

```
make desktop-e2e REEL_E2E_SERVER=http://127.0.0.1:8757 REEL_E2E_PASSWORD='reel demo 8757'
```

## The run with the real OBS

`make desktop-obs-e2e` runs the app against the real OBS, from OBS's own Linux package of
the pinned version, under Xvfb (`desktop/ci/obs-e2e.sh`). No server is needed: without
`REEL_E2E_OBS_PASSWORD` the clip is checked in the container, with it the clip goes to the
demo server.

Its image, `desktop/ci/obs.Dockerfile`, is not the build image. OBS packages only for its
newest Ubuntu, so the image is that Ubuntu: `linux_ubuntu` in `desktop/obs/pinned.json`,
which `make desktop-obs-image` passes in with the package's URL and SHA-256. A new OBS for a
newer Ubuntu moves the image with the pin. The app it runs is the debug build of
`make desktop-linux`, built on Debian 12 like the AppImage; the newer glibc runs it.

It and `make desktop-obs-start-check`, with the OBS inside reel's package under Wine, check the
protocol too: every request type reel's code sends must be one OBS lists in its `GetVersion`
answer (`desktop/ci/check-obs-requests.sh`). The list is read from the code
(`desktop/ci/obs-requests.sh`), and `make desktop-check` fails when a request is named in a
way that list would miss. `desktop/OBS.md` has what reel uses of OBS and what checks each,
the tests by hand included.

The scheduled workflow `.github/workflows/obs-bump.yml` opens a pull request that moves the
pin, and the version `THIRD_PARTY.md` names, when a newer stable OBS is out. It is a
`fix(desktop):`, so once it is merged release-please releases reel with that OBS. Its body lists
what to check by hand on Windows. `make check` fails
when `THIRD_PARTY.md` names another OBS than the pin (`ci/check-obs-version.sh`), so a bump by
hand changes both too.

## Screenshots

The screenshot targets write into `shots/` at the top of the repository, which git ignores:

| Target | Folder |
|---|---|
| `make desktop-shots` | `shots/desktop/` (`DESKTOP_SHOTS_OUT`) |
| `make desktop-e2e` | `shots/desktop-e2e/` |
| `make desktop-installer-shots` | `shots/installer/` (`INSTALLER_SHOTS`) |
| `make desktop-installer-audit` | `shots/installer-audit/` (`INSTALLER_AUDIT`) |
| `make desktop-appimage-arch` | `shots/linux/`, and the media check's in `$DESKTOP_OUT/arch-media/` |

`make desktop-standalone`, `desktop-downloads`, `desktop-zip-download` and `desktop-back` take
screenshots when `REEL_SHOTS_OUT` names a folder in the container, such as
`REEL_SHOTS_OUT=/work/shots/standalone`.

## The volumes and their caps

The containers keep their caches in named Docker volumes, so a rerun compiles only what
changed. The target directories grow with every build, so they are capped:
`ci/cap-volume.sh` measures a volume before a build and wipes it when it is over its cap.
The next build is then a cold one (minutes), which is better than a full disk.

| Volume | Holds | Cap | Used by |
|---|---|---|---|
| `reel-target` | the server's cargo target directory | 8 GB (`TARGET_CAP_MB`) | `make check` |
| `reel-cargo-registry`, `reel-cargo-git` | the server's cargo registry and git caches | none | `make check` |
| `reel-desktop-target` | the desktop workspace's target directory: the checks, the debug build, the Windows and Linux release builds, and the server's crates compiled into the app | 8 GB (`DESKTOP_TARGET_CAP_MB`) | every desktop target that compiles |
| `reel-desktop-cargo` | the desktop workspace's cargo home: the registry, git, the Tauri CLI's downloads, the pinned ffmpeg and OBS | none, about 2 GB | every desktop target |
| `reel-xwin-cache` | the Windows CRT and SDK that cargo-xwin unpacks | none, about 1.2 GB; keep it, or it is downloaded again | the Windows builds and clippy for Windows |

Three settings keep the target directories small: `CARGO_INCREMENTAL=0` in the containers,
`[profile.dev.package."*"] debug = false` in both workspaces (dependencies carry no debug
info in test builds; this repository's crates keep theirs), and the app crate built as an
`rlib` only (the `staticlib` and `cdylib` of Tauri's template are for mobile and would add
about 1.5 GB per build).

`docker volume rm reel-desktop-target reel-xwin-cache reel-desktop-cargo` frees the desktop
caches; the next build is cold again.

## What it takes

| Item | Size or time |
|---|---|
| the image `reel-desktop:local` | about 6.5 GB |
| the image `reel-obs-e2e:local` | about 2 GB |
| `reel-desktop-target` | up to its 8 GB cap |
| `reel-desktop-cargo` and `reel-xwin-cache` | about 2 GB and 1.2 GB |
| a cold `make desktop-windows` | 20 minutes or more: the SDK download and unpack, and a cold compile of the Tauri crates |
| a warm `make desktop-windows` | a few minutes |

## Continuous integration

### What a pull request runs

`.github/workflows/desktop.yml` runs on pull requests that touch `desktop/`, the `Makefile`
or the workflow itself, and by hand. It does not run on pushes to `main`. The jobs run the
make targets above, so a runner needs Docker, make and git.

1. **What changed** (`desktop/ci/changes.sh`, tested by `desktop/ci/changes-test.sh`)
   compares the pull request's head with the newest of its commits whose run passed. When
   only documentation changed since then (`docs/` but the generated `docs/configuration.md`,
   and the `*.md` files at the top of the repository), nothing else runs, and the run's
   summary says why. When no passed run is in the push's history (a new pull request, a
   force push, or only failed or cancelled runs before), everything runs.
2. **make desktop-check**, then `make desktop-check-windows`, so the Windows code is compiled
   on every pull request, installers or not.
3. **The app with the real OBS**: `make desktop-obs-e2e`.
4. **Linux AppImage and .deb**: `make desktop-appimage` and `make desktop-appimage-check`,
   with the files as a workflow artifact.
5. **Windows installer**, only when the pull request touches the installers' files (the
   patterns are in `changes.sh`) or is labelled `installers`: `make desktop-windows`,
   `make desktop-installers-check`, and `make desktop-obs-start-check` against the newest
   older release, with the installers as a workflow artifact. reel's installer, with OBS, is
   built alone unless reel light's own files changed; the label builds both. Adding the label
   to a pull request whose head already passed builds both installers and nothing else.

release-please's pull requests, which only bump the version, skip the checks and builds.
A newer push waits for the run in progress instead of cancelling it, so a documentation push
then finishes in a minute against the run that just passed. A manual run runs everything,
both installers included, and so does each push to a pull request that moves OBS's pin
(`desktop/obs/pinned.json`).

The server's CI (`.github/workflows/ci.yml`) runs on every pull request and takes the same
decision with `changes.sh`: documentation alone since its newest passed run skips
`make check` and the licence check.

### The release

The `desktop-release` job in `.github/workflows/ci.yml` runs after release-please made a
release. On the release commit it builds both Windows installers, the AppImage and the .deb,
each signed for the updater (the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), and runs the installer and AppImage checks. It uploads
the four files to the GitHub release, each with its `.sha256` and `.sig`, together with OBS's
source archive, and replaces assets of the same name on a rerun; it fails when a file or a
signature is missing. The Windows and Linux builds do not fit on the runner's disk side by
side, so the job wipes `reel-desktop-target` between them, and removes the volume and the
`reel-desktop:local` image at the end.

The same job runs by hand from the Actions page (CI, Run workflow) with a release tag and,
optionally, the commit to build. It uploads to that existing release, so a failed upload is
redone without a new release.

The version in the file names is the release's: release-please bumps `desktop/Cargo.toml` and
`desktop/app/tauri.conf.json` together with the server's `Cargo.toml`
(`.release-please-config.json`), so the app and the server share one version.
