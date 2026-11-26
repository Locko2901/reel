# About

What this reel is, where its code and its docs are, its licence, and the work of others it ships
with. Nothing on this page is a setting: it only says what is there. It is the last page of
Settings, under **System**, and anyone signed in can read it: an admin among the other pages, a
member from the menu behind their initial, **About**, and in the app's window under **System**.

## reel {#reel}

- **Version**: the server's version. In the app's standalone mode, the version of reel on this PC,
  which is the app's own.
- **Built**: the day of the commit it was built from, and that commit, linked to it on GitHub.
  A build made without git says so.
- **Licence**: reel's own, the MIT licence, copyright (c) 2026 Lockoo. **Read it** opens its
  whole text, as it is in this version.
- **This app**, in the app's window only: the app's version and kind (reel or reel light on
  Windows, the AppImage or the `.deb` on Linux), with reel on Windows the OBS that came with it,
  and its updater's line: whether a new version is ready to install, or the version its last
  update came from. **Updates** opens the app's [App](app.md#updates-group) page. Connected, the
  app takes its updates from this server, so the two versions are the same once it has updated.

## Links {#links}

Each opens in the browser; in the app's window too, which stays on reel.

- **Docs**: this site.
- **Source code**: reel's repository on GitHub.
- **Release notes**: the release page of this version, with what it changed. The Status page
  shows the newest five releases too.
- **Report a bug**: reel's issues on GitHub. Say what you did, what happened and the version on
  this page; the server's log helps ([Logs](advanced.md#logs)).

## Credits {#credits}

The work of others that reel's downloads carry, each under its own licence, with where its source
or its terms are. **Every one** opens `THIRD_PARTY.md` of this version, which lists them all with
their versions and where each one's source is.

- **OBS Studio**, in reel for Windows (the installer with OBS), as its makers publish it,
  unmodified: GNU GPL 2.0 or later. Its licence text is in the install folder, and **Source**
  opens the source of that version.
- **FFmpeg**, in every download of the app (gyan.dev's build on Windows, BtbN's on Linux): GNU GPL
  3.0. In the app's standalone mode, the row also names the version that encodes the library on
  this PC.
- **jellyfin-ffmpeg**, on a server running reel's image: the FFmpeg that encodes there, with Intel's
  media driver, GNU GPL 3.0. A server given another FFmpeg names its version instead.
- **Debian 12**, which the server's image is built on: each package's licence is in its copyright
  file inside the image.
- **WebKitGTK, GTK and GStreamer**, the app's window on Linux, and **Microsoft Edge WebView2**,
  the app's window on Windows. In the app's standalone mode the page lists the one of this PC's
  system.
- **Geist and Geist Mono**, the pages' type: SIL Open Font License 1.1.
- **Rust crates**, the libraries the server and the app are built from, each under its own
  licence: **The list** names every one with its licence.
