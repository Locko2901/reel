# reel desktop

reel's desktop app, for Windows and Linux. It sits in the tray beside OBS, picks up
every replay OBS saves and keeps it in a library: on the PC itself (standalone, which is
where a new install starts) or on a reel server it is paired with, which adds share links.
Paired, one hotkey saves the replay in OBS, uploads it and puts its link on the clipboard.

## What it does

- **Finds the clips.** It talks to OBS over OBS's WebSocket (version 5, built into OBS 28
  and later) and watches the recording folder too, so a clip is found even when OBS is not
  reachable.
- **Standalone.** reel's own server runs inside the app on the loopback, with the library
  under the app's data folder and the ffmpeg the app ships with. No server needed.
- **Paired with a server.** Clips upload in resumable pieces, and Save and share copies the
  link of the clip it just saved.
- **One window.** The window shows the library (the server's pages, or the standalone
  library), with the app's own pages under Settings, This app.
- **OBS included, or yours.** On Windows, the installer reel brings an unmodified OBS that
  the app sets up and runs for you; reel light connects to the OBS you have.

## Downloads

Every release on GitHub carries the four, each with its `.sha256`, and a reel server offers
them under Settings, Devices.

| File | For |
|---|---|
| `reel_<version>_x64-setup.exe` | reel: Windows 10 or 11, 64 bit, with OBS included |
| `reel-light_<version>_x64-setup.exe` | reel light: Windows 10 or 11, 64 bit, with your own OBS |
| `reel_<version>_amd64.AppImage` | Linux, any distribution with glibc 2.36 or later |
| `reel_<version>_amd64.deb` | Debian 12, Ubuntu 24.04 and later |

## Documentation

How to install, set up and use the app is in the user docs:

- [The desktop app](https://locko2901.github.io/reel/desktop-app/): the window, the tray,
  the hotkeys, the settings and the updates.
- [The setup guides](https://locko2901.github.io/reel/setup/), one per way to run it:
  [All in one](https://locko2901.github.io/reel/setup/all-in-one/) (Windows, OBS
  included), [Your own OBS](https://locko2901.github.io/reel/setup/own-obs/) (Windows),
  [Linux](https://locko2901.github.io/reel/setup/linux/) and
  [Standalone](https://locko2901.github.io/reel/setup/standalone/).

## The code

The app is Rust and [Tauri 2](https://tauri.app/). `desktop/` is a cargo workspace of its
own, so the server's build never compiles WebKitGTK or the Windows crates. It uses the
server's crates (`crates/core`, `store`, `media`, `pipeline` and `server`) as libraries:
standalone mode is reel's own server, compiled into the app.

| Folder | What it holds |
|---|---|
| `core/` | `reel-desktop-core`, everything without a window: the settings, pairing, the uploader, the folder watcher, the OBS client, the cache rules, the cues; and the installers' helpers `reel-stop` and `reel-elevate` |
| `app/` | `reel-desktop`, the Tauri app: the tray, the window, the hotkeys, the notifications, standalone mode, the updater; `app/nsis/` holds the Windows installers' script and pages |
| `ui/` | the app's own pages: This app's settings, the setup of the OBS that comes with reel, the page shown when the server cannot be reached |
| `assets/sounds/` | the cues |
| `obs/`, `ffmpeg/` | the pins, each with its SHA-256: the OBS that reel for Windows carries, and the ffmpeg every build carries |
| `packages.json` | the two Windows packages' names, reel and reel light, and their installers' files: the server, the app, the installers and the release read them from here |
| `ci/` | the container images and the scripts that build and check the app |

## Building

[BUILD.md](BUILD.md) has everything: one container builds the Windows installers (cross
compiled on Linux), the AppImage and the .deb, and runs the checks under Xvfb and Wine.
The host needs Docker, make and git. Before a pull request that touches `desktop/`, run
`make desktop-check`; [CONTRIBUTING.md](../CONTRIBUTING.md) has the rest.
