# Third party software and media

reel itself is under the [MIT licence](LICENSE). What it ships, or what the downloads carry,
comes from others under their own licences, listed here with where each one's source is.

## What each download carries

| Download | Others' work inside |
|---|---|
| The server's image, `ghcr.io/locko2901/reel` | Debian 12, jellyfin-ffmpeg 7, Intel's media driver, the Rust crates |
| **reel** for Windows | OBS Studio, ffmpeg and ffprobe, the WebView2 bootstrapper, the Rust crates |
| **reel light** for Windows | ffmpeg and ffprobe, the WebView2 bootstrapper, the Rust crates |
| The AppImage for Linux | ffmpeg and ffprobe, WebKitGTK, GTK, GStreamer and their libraries from Debian 12, the Rust crates |
| The .deb for Linux | ffmpeg and ffprobe, the Rust crates (WebKitGTK and GStreamer come from your distribution) |
| Every page reel serves, and the docs site | the Geist and Geist Mono fonts |

## OBS Studio

In **reel** for Windows: OBS Studio 33.0.1, the official build, unmodified, under the
GNU General Public License version 2 or later. Its licence text is in the install folder
beside it. Its complete source is the release's own archive,
`OBS-Studio-33.0.1-Sources.tar.gz`, which every reel release that ships OBS carries as an
asset, and the upstream tag
[obsproject/obs-studio 33.0.1](https://github.com/obsproject/obs-studio/tree/33.0.1). The
version reel pins is in [`desktop/obs/pinned.json`](desktop/obs/pinned.json). On Linux, reel uses
the OBS your distribution installed and ships none.

## ffmpeg and ffprobe

Under the GNU General Public License version 3 (these builds include GPL libraries such as
x264). Their licence text ships beside them as `ffmpeg-LICENSE.txt`.

- **Windows installers**: ffmpeg 9.0.2, the "essentials" build by
  [gyan.dev](https://www.gyan.dev/ffmpeg/builds/), pinned in
  [`desktop/ffmpeg/pinned.json`](desktop/ffmpeg/pinned.json). The build page lists every
  library in it with its version.
- **AppImage and .deb**: ffmpeg 9.0.2, the GPL build `n9.0.2-17-g2a571b6068` by
  [BtbN](https://github.com/BtbN/FFmpeg-Builds) of 2026-09-30
  (`autobuild-2026-09-30-13-08`), pinned in
  [`desktop/ffmpeg/pinned-linux.json`](desktop/ffmpeg/pinned-linux.json). Its build scripts at
  that tag name every library and its version.
- **Source**: FFmpeg's own at the matching release,
  [FFmpeg n9.0.2](https://github.com/FFmpeg/FFmpeg/tree/n9.0.2) (BtbN's build is commit
  `2a571b6068` on the `release/9.0` branch), and each library's from the build pages above.
- **The server's image**: [jellyfin-ffmpeg](https://github.com/jellyfin/jellyfin-ffmpeg) 7
  (GPL 3), installed from Jellyfin's Debian repository, with the Intel media driver
  (`intel-media-va-driver-non-free`, MIT, from Debian's non-free section) and libva (MIT).

## Debian's libraries

The server's image is built on `debian:bookworm-slim`, and the AppImage carries Debian 12's
builds of WebKitGTK 2.50.6 (`webkit2gtk`), GTK 3.24.38 (`gtk+3.0`), GStreamer 1.22.0 with
its base, good and libav plugins (`gstreamer1.0`, `gst-plugins-base1.0`, `gst-plugins-good1.0`,
`gst-libav1.0`), libsoup 3.2.3, librsvg 2.54.7, GLib 2.74.6, libayatana-appindicator 0.5.92 and
the FFmpeg 5.1 libraries the libav plugin uses (`ffmpeg`), with what they depend on. Most are
under the GNU LGPL 2.1 or later; some plugins and the FFmpeg libraries as Debian builds them
are under the GPL. Each package's licence is in its copyright file, and its exact source is
Debian's source package of that version, from any Debian mirror or
[snapshot.debian.org](https://snapshot.debian.org/) (`apt-get source <package>=<version>` in
Debian 12). The versions the image carries are those of Debian 12's updates on the day of the
release.

## Microsoft Edge WebView2

The Windows installers carry Microsoft's WebView2 bootstrapper, which installs the WebView2
runtime when Windows does not have it, under Microsoft's
[distribution terms](https://developer.microsoft.com/microsoft-edge/webview2/). reel's
window runs on it; nothing of it is changed.

## Fonts

[Geist and Geist Mono](https://github.com/vercel/geist-font), Copyright 2024 The Geist Project
Authors, under the SIL Open Font License 1.1, whose text is beside them in
[`web/assets/fonts/LICENSE-geist`](web/assets/fonts/LICENSE-geist).

[Selawik](https://github.com/microsoft/Selawik), Copyright 2015 Microsoft Corporation, with
Reserved Font Name Selawik, under the SIL Open Font License 1.1, whose text is beside it in
[`desktop/ci/selawik/OFL.txt`](desktop/ci/selawik/OFL.txt). Only CI uses it, unchanged, in
place of Segoe UI for the installers' screenshots under Wine; no download carries it.

## The docs site

Built with [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/) (MIT), whose
styles and scripts the site serves.

## The pictures in the docs

The clips in the screenshots of the README and the docs were recorded from three open source
games, played by their own bots and AI:

- **Xonotic** 0.8.6, by Team Xonotic, [xonotic.org](https://xonotic.org): its code, maps,
  models, textures, sound and music under the
  [GPL 3 or later](https://www.gnu.org/licenses/gpl-3.0.html). The maps in the pictures are
  Solarium, Catharsis and Afterslime.
- **SuperTuxKart** 1.4, by the SuperTuxKart Team, [supertuxkart.net](https://supertuxkart.net):
  its code under the GPL 3 or later; its art, sound and music under licences per file, mostly
  [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/) and
  [4.0](https://creativecommons.org/licenses/by-sa/4.0/), also CC BY 3.0 and CC0 (the game's
  copyright file names each author). The track in the pictures is *Black Forest* by Sven
  Andreas Belting, with art by others; the kart is *Tux* by Julian Schönbächler (CC BY-SA 3.0).
- **0 A.D.** Alpha 26, by Wildfire Games, [play0ad.com](https://play0ad.com): its code under the
  GPL 2 or later, its art and sound under
  [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/).

The screenshots that show a frame of one of these games are under that game's licence, as its
art is: those of SuperTuxKart and 0 A.D. under CC BY-SA (4.0 and 3.0), those of Xonotic under
the GPL 3 or later. The players' names in them are the games' own bot names and `sam`; the
titles, collections and links around them are a made up player's.

## The sound cues

The app's two cues are reel's own, made with ffmpeg
([`desktop/assets/sounds/SOUNDS.md`](desktop/assets/sounds/SOUNDS.md)).

## Rust crates

Every crate the server and the desktop app build on, checked with
[cargo-deny](https://github.com/EmbarkStudios/cargo-deny) against the licences
[`deny.toml`](deny.toml) allows (`make licences`, which CI runs): all of them may ship inside an
MIT program. Five crates of the app's window (`cssparser`, `cssparser-macros`, `dtoa-short`,
`option-ext`, `selectors`) are under the Mozilla Public License 2.0, a file level copyleft:
reel uses them unmodified, and their source is on [crates.io](https://crates.io/). `make
third-party` writes this list again.

<!-- crates:begin -->

616 crates, by the licence each declares:

- **MIT OR Apache-2.0** (370): allocator-api2 0.2.21, anstream 1.0.0, anstyle 1.0.14, anstyle-parse 1.0.0, anstyle-query 1.1.5, anstyle-wincon 3.0.11, anyhow 1.0.104, arboard 3.6.1, arc-swap 1.9.2, argon2 0.5.3, askama 0.14.0, askama_derive 0.14.0, askama_parser 0.14.0, async-broadcast 0.7.2, async-channel 2.5.0, async-executor 1.14.0, async-io 2.6.0, async-lock 3.4.2, async-process 2.5.0, async-recursion 1.2.0, async-signal 0.2.14, async-task 4.7.1, async-trait 0.1.92, atomic-waker 1.1.2, autocfg 1.5.1, base16ct 0.2.0, base64 0.21.7, base64 0.22.1, base64 0.23.1, base64ct 1.8.3, basic-toml 0.1.10, bit-set 0.8.0, bit-vec 0.8.0, bitflags 1.3.2, bitflags 2.13.2, blake2 0.10.6, block-buffer 0.10.4, block-buffer 0.12.1, blocking 1.7.0, bstr 1.13.1, camino 1.2.6, cargo-platform 0.1.9, cargo_toml 1.0.1, cc 1.6.0, cfg-expr 0.15.8, cfg-if 1.0.5, chacha20 0.10.2, chrono 0.4.45, clap 4.6.7, clap_builder 4.6.7, clap_derive 4.6.7, clap_lex 1.1.1, colorchoice 1.0.5, concurrent-queue 2.5.0, const-oid 0.10.2, const-oid 0.9.6, cookie 0.18.2, core_detect 1.0.0, cpufeatures 0.2.17, cpufeatures 0.3.1, crc 3.4.0, crc-catalog 2.5.0, crc32fast 1.5.2, crc32fast 1.5.4, crossbeam-channel 0.5.17, crossbeam-queue 0.3.14, crossbeam-utils 0.8.23, crypto-bigint 0.5.5, crypto-common 0.1.7, crypto-common 0.2.2, ctor 1.0.13, curve25519-dalek-derive 0.1.1, dbus 0.9.12, der 0.7.10, deranged 0.5.8, digest 0.10.7, digest 0.11.3, dirs 6.0.0, dirs 7.0.0, dirs-sys 0.5.0, displaydoc 0.2.7, downcast-rs 1.2.1, dtoa 1.0.11, dyn-clone 1.0.20, ecdsa 0.16.9, ed25519 2.2.3, either 1.19.0, elliptic-curve 0.13.8, enumflags2 0.7.12, enumflags2_derive 0.7.12, equivalent 1.0.2, erased-serde 0.4.10, errno 0.3.14, event-listener 5.4.2, event-listener-strategy 0.5.4, fastrand 2.5.0, fdeflate 0.3.7, ff 0.13.1, field-offset 0.3.6, filetime 0.2.29, find-msvc-tools 0.1.14, fixedbitset 0.5.7, flate2 1.1.10, flume 0.11.1, fnv 1.0.7, form_urlencoded 1.2.2, futures 0.3.34, futures-channel 0.3.34, futures-core 0.3.34, futures-executor 0.3.34, futures-intrusive 0.5.0, futures-io 0.3.34, futures-lite 2.6.1, futures-macro 0.3.34, futures-sink 0.3.34, futures-task 0.3.34, futures-timer 3.0.4, futures-util 0.3.34, getrandom 0.2.17, getrandom 0.3.4, getrandom 0.4.3, glob 0.3.4, global-hotkey 0.8.0, group 0.13.0, hashbrown 0.12.3, hashbrown 0.14.5, hashbrown 0.15.5, hashbrown 0.16.1, hashbrown 0.17.1, hashlink 0.10.0, heck 0.4.1, heck 0.5.0, hex 0.4.3, hkdf 0.12.4, hmac 0.12.1, html5ever 0.39.0, http 1.5.0, httparse 1.10.1, httpdate 1.0.3, hybrid-array 0.4.15, iana-time-zone 0.1.65, ident_case 1.0.1, idna 1.1.0, idna_adapter 1.2.2, image 0.25.10, indexmap 1.9.3, indexmap 2.14.2, ipnet 2.12.2, is_terminal_polyfill 1.70.2, itertools 0.10.5, itoa 1.0.18, json-patch 4.2.0, jsonptr 0.7.1, keyboard-types 0.7.0, keyboard-types 0.8.3, keyring 3.6.3, lazy_static 1.5.1, libappindicator 0.9.0, libappindicator-sys 0.9.0, libc 0.2.190, libdbus-sys 0.2.7, lock_api 0.4.14, log 0.4.34, markup5ever 0.39.0, mime 0.3.17, muda 0.20.0, multiversion_no_op 1.0.0, notify-rust 4.18.2, notify-types 2.1.0, num-bigint-dig 0.8.6, num-conv 0.2.2, num-integer 0.1.47, num-iter 0.1.46, num-traits 0.2.19, oauth2 5.0.0, once_cell 1.21.4, once_cell_polyfill 1.70.2, openssl-probe 0.2.1, ordered-stream 0.2.0, p256 0.13.2, p384 0.13.1, parking 2.2.1, parking_lot 0.12.5, parking_lot_core 0.9.12, password-auth 1.0.0, password-hash 0.5.0, pem-rfc7468 0.7.0, percent-encoding 2.3.2, petgraph 0.8.3, pin-project 1.1.13, pin-project-internal 1.1.13, pin-project-lite 0.2.17, piper 0.2.5, pkcs1 0.7.5, pkcs8 0.10.2, pkg-config 0.3.34, png 0.17.16, png 0.18.1, polling 3.11.0, portable-atomic 1.15.0, powerfmt 0.2.1, ppv-lite86 0.2.21, primeorder 0.13.6, proc-macro-crate 1.3.1, proc-macro-crate 2.0.2, proc-macro-crate 3.5.0, proc-macro-error 1.0.4, proc-macro-error-attr 1.0.4, proc-macro2 1.0.107, quote 1.0.47, rand 0.10.3, rand 0.8.8, rand 0.9.5, rand_chacha 0.3.1, rand_chacha 0.9.0, rand_core 0.10.1, rand_core 0.6.4, rand_core 0.9.5, regex 1.13.1, regex-automata 0.4.18, regex-syntax 0.8.11, reqwest 0.12.28, reqwest 0.13.5, rfc6979 0.4.0, rsa 0.9.10, rustc-hash 2.1.3, rustc_version 0.4.1, rustls-pki-types 1.15.1, rustls-platform-verifier 0.7.1, rustversion 1.0.23, scopeguard 1.2.0, sec1 0.7.3, semver 1.0.28, serde 1.0.229, serde-untagged 0.1.9, serde_core 1.0.229, serde_derive 1.0.229, serde_derive_internals 0.29.1, serde_json 1.0.151, serde_path_to_error 0.1.20, serde_plain 1.0.2, serde_repr 0.1.21, serde_spanned 0.6.9, serde_spanned 1.1.2, serde_urlencoded 0.7.1, serde_with 3.24.0, serde_with_macros 3.24.0, serialize-to-javascript 0.1.2, serialize-to-javascript-impl 0.1.2, servo_arc 0.4.3, sha1 0.11.0, sha2 0.10.9, sha2 0.11.0, shlex 2.0.1, signal-hook-registry 1.4.8, signature 2.2.0, simdutf8 0.1.5, siphasher 1.0.4, smallvec 1.16.2, socket2 0.6.5, softbuffer 0.4.8, spinning_top 0.3.0, spki 0.7.3, sqlx 0.8.6, sqlx-core 0.8.6, sqlx-macros 0.8.6, sqlx-macros-core 0.8.6, sqlx-sqlite 0.8.6, stable_deref_trait 1.2.1, string_cache 0.9.0, string_cache_codegen 0.6.1, symlink 0.1.0, syn 1.0.109, syn 2.0.119, syn 3.0.6, system-deps 6.2.2, tar 0.4.46, tauri 2.12.1, tauri-build 2.7.1, tauri-codegen 2.7.1, tauri-macros 2.7.1, tauri-plugin 2.7.1, tauri-plugin-autostart 2.7.0, tauri-plugin-clipboard-manager 2.4.1, tauri-plugin-dialog 2.8.1, tauri-plugin-fs 2.6.0, tauri-plugin-global-shortcut 2.4.0, tauri-plugin-notification 2.5.1, tauri-plugin-opener 2.7.0, tauri-plugin-single-instance 2.5.2, tauri-plugin-updater 2.13.2, tauri-runtime 2.12.1, tauri-runtime-wry 2.12.1, tauri-utils 2.10.1, tauri-winrt-notification 0.8.1, tempfile 3.27.0, tendril 0.5.1, thiserror 1.0.69, thiserror 2.0.21, thiserror-impl 1.0.69, thiserror-impl 2.0.21, thread_local 1.1.10, time 0.3.55, time-core 0.1.9, time-macros 0.2.32, tokio-rustls 0.26.6, toml 0.8.2, toml 0.8.23, toml 1.1.7+spec-1.1.0, toml_datetime 0.6.11, toml_datetime 0.6.3, toml_datetime 1.1.2+spec-1.1.0, toml_edit 0.19.15, toml_edit 0.20.2, toml_edit 0.22.27, toml_edit 0.25.16+spec-1.1.0, toml_parser 1.1.4+spec-1.1.0, toml_write 0.1.2, toml_writer 1.1.3+spec-1.1.0, tower_governor 0.8.0, tray-icon 0.25.1, tungstenite 0.30.0, typeid 1.0.3, typenum 1.20.1, unicase 2.10.0, unicode-segmentation 1.13.3, url 2.5.8, utf8_iter 1.0.4, utf8parse 0.2.2, uuid 1.27.0, vcpkg 0.2.15, version_check 0.9.5, web-time 1.1.0, web_atoms 0.2.6, winapi 0.3.9, window-vibrancy 0.8.1, windows 0.58.0, windows 0.62.2, windows-collections 0.3.2, windows-core 0.58.0, windows-core 0.62.2, windows-future 0.3.2, windows-implement 0.58.0, windows-implement 0.60.2, windows-interface 0.58.0, windows-interface 0.59.3, windows-link 0.2.1, windows-numerics 0.3.1, windows-registry 0.6.1, windows-result 0.2.0, windows-result 0.4.1, windows-strings 0.1.0, windows-strings 0.5.1, windows-sys 0.59.0, windows-sys 0.60.2, windows-sys 0.61.2, windows-targets 0.52.6, windows-targets 0.53.5, windows-threading 0.2.1, windows-version 0.1.7, windows_x86_64_gnu 0.52.6, windows_x86_64_gnu 0.53.1, windows_x86_64_msvc 0.52.6, windows_x86_64_msvc 0.53.1, wl-clipboard-rs 0.9.4, wry 0.57.0, x11rb 0.13.2, x11rb-protocol 0.13.2, xattr 1.6.1, zeroize 1.9.1
- **MIT** (164): ashpd 0.13.13, atk 0.18.2, atk-sys 0.18.2, atoi 2.0.0, auto-launch 0.6.0, axum 0.8.9, axum-core 0.5.6, axum-login 0.18.0, axum-macros 0.5.1, bytes 1.12.1, cairo-rs 0.18.5, cairo-sys-rs 0.18.2, cargo_metadata 0.19.2, cfb 0.14.0, cfg_aliases 0.2.2, darling 0.24.1, darling_core 0.24.1, darling_macro 0.24.1, dashmap 6.2.1, data-encoding 2.11.1, derive_more 2.1.1, derive_more-impl 2.1.1, dlopen2 0.8.2, dlopen2_derive 0.4.3, dom_query 0.28.0, dotenvy 0.15.7, embed-resource 3.0.12, endi 1.1.1, gdk 0.18.2, gdk-pixbuf 0.18.5, gdk-pixbuf-sys 0.18.0, gdk-sys 0.18.2, gdkwayland-sys 0.18.2, gdkx11 0.18.2, gdkx11-sys 0.18.2, generic-array 0.14.7, gio 0.18.4, gio-sys 0.18.1, glib 0.18.5, glib-macros 0.18.5, glib-sys 0.18.1, gobject-sys 0.18.0, governor 0.10.4, gtk 0.18.2, gtk-sys 0.18.2, gtk3-macros 0.18.2, http-body 1.1.0, http-body-util 0.1.5, http-range-header 0.4.2, hyper 1.12.0, hyper-util 0.1.21, ico 0.5.0, infer 0.22.0, is-docker 0.2.0, is-wsl 0.4.0, javascriptcore-rs 1.1.2, javascriptcore-rs-sys 1.1.1, libm 0.2.16, libsqlite3-sys 0.30.1, matchers 0.2.0, memoffset 0.9.1, mime_guess 2.0.5, minisign-verify 0.2.5, mio 1.2.4, multer 3.1.0, new_debug_unreachable 1.0.6, nix 0.29.0, nix 0.31.3, nom 8.0.0, nonempty 0.7.0, nu-ansi-term 0.50.3, open 5.4.4, openidconnect 4.0.1, ordered-float 2.10.1, os_info 3.15.0, os_pipe 1.2.3, pango 0.18.3, pango-sys 0.18.0, phf 0.13.1, phf_codegen 0.13.1, phf_generator 0.13.1, phf_macros 0.13.1, phf_shared 0.13.1, plist 1.10.1, precomputed-hash 0.1.1, quanta 0.12.6, quick-xml 0.41.0, quick-xml 0.42.0, raw-cpuid 11.6.0, rfd 0.16.0, rmp 0.8.15, rmp-serde 1.3.1, rust-embed 8.13.0, rust-embed-impl 8.13.0, rust-embed-utils 8.13.0, schannel 0.1.29, schemars 0.8.22, schemars_derive 0.8.22, serde-value 0.7.0, sharded-slab 0.1.7, simd-adler32 0.3.10, slab 0.4.12, soup3 0.5.0, soup3-sys 0.5.0, spin 0.9.9, strsim 0.11.1, synstructure 0.14.0, tauri-winres 0.3.6, tokio 1.53.2, tokio-macros 2.7.2, tokio-stream 0.1.19, tokio-tungstenite 0.30.0, tokio-util 0.7.19, tower 0.5.3, tower-cookies 0.11.0, tower-http 0.6.11, tower-layer 0.3.3, tower-service 0.3.3, tower-sessions 0.14.0, tower-sessions-core 0.14.0, tower-sessions-memory-store 0.14.0, tower-sessions-sqlx-store 0.15.0, tracing 0.1.44, tracing-appender 0.2.5, tracing-attributes 0.1.31, tracing-core 0.1.36, tracing-log 0.2.0, tracing-subscriber 0.3.23, tree_magic_mini 3.2.2, try-lock 0.2.5, uds_windows 1.2.1, ulid 1.2.1, urlencoding 2.1.3, urlpattern 0.6.0, version-compare 0.2.1, vswhom 0.1.0, vswhom-sys 0.1.3, want 0.3.2, wayland-backend 0.3.17, wayland-client 0.31.15, wayland-protocols 0.32.13, wayland-protocols-wlr 0.3.12, wayland-scanner 0.31.11, wayland-sys 0.31.11, webkit2gtk 2.0.2, webkit2gtk-sys 2.0.2, webview2-com 0.39.1, webview2-com-macros 0.8.1, webview2-com-sys 0.39.1, winnow 0.5.40, winnow 0.7.15, winnow 1.0.4, winreg 0.56.0, x11 2.21.0, x11-dl 2.21.0, zbus 5.19.0, zbus_macros 5.19.0, zbus_names 4.3.4, zcheapstr 1.1.0, zip 4.6.1, zmij 1.0.23, zvariant 5.15.0, zvariant_derive 5.15.0, zvariant_utils 4.2.0
- **Unicode-3.0** (18): icu_collections 2.3.0, icu_locale_core 2.3.0, icu_normalizer 2.3.0, icu_normalizer_data 2.3.0, icu_properties 2.3.0, icu_properties_data 2.3.0, icu_provider 2.3.1, litemap 0.8.3, potential_utf 0.1.6, tinystr 0.8.4, writeable 0.6.4, yoke 0.8.3, yoke-derive 0.8.4, zerofrom 0.1.8, zerofrom-derive 0.1.8, zerotrie 0.2.5, zerovec 0.11.8, zerovec-derive 0.11.6
- **MIT OR Unlicense** (12): aho-corasick 1.1.5, byteorder 1.5.0, byteorder-lite 0.1.0, globset 0.4.20, jiff 0.2.38, jiff-core 0.1.1, jiff-tzdb 0.1.9, jiff-tzdb-platform 0.1.3, memchr 2.8.3, same-file 1.0.6, walkdir 2.5.0, winapi-util 0.1.11
- **ISC** (6): forwarded-header-value 0.1.1, inotify 0.11.5, inotify-sys 0.1.8, libloading 0.7.4, rustls-webpki 0.103.15, untrusted 0.9.0
- **Apache-2.0** (5): gethostname 1.1.0, hound 3.5.1, nonzero_ext 0.3.0, sync_wrapper 1.0.2, tao 0.37.1
- **BSD-3-Clause** (5): alloc-no-stdlib 3.0.0, alloc-stdlib 0.3.0, curve25519-dalek 4.1.3, ed25519-dalek 2.2.0, subtle 2.6.1
- **MIT OR Apache-2.0 OR Zlib** (5): bytemuck 1.25.2, miniz_oxide 0.8.9, miniz_oxide 0.9.1, raw-window-handle 0.6.2, xkeysym 0.2.1
- **MPL-2.0** (5): cssparser 0.37.0, cssparser-macros 0.7.1, dtoa-short 0.3.5, option-ext 0.2.0, selectors 0.38.0
- **MIT OR Apache-2.0 OR ISC** (3): hyper-rustls 0.27.10, rustls 0.23.45, rustls-native-certs 0.8.4
- **Apache-2.0 OR BSD-3-Clause** (2): moxcms 0.8.1, pxfm 0.1.30
- **BSL-1.0** (2): clipboard-win 5.4.1, error-code 3.4.0
- **MIT OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception** (2): linux-raw-sys 0.12.1, rustix 1.1.5
- **MIT OR Apache-2.0 OR BSD-2-Clause** (2): zerocopy 0.8.61, zerocopy 0.8.62
- **Zlib** (2): foldhash 0.1.5, foldhash 0.2.0
- **(Apache-2.0 OR MIT) AND BSD-3-Clause** (1): encoding_rs 0.8.42
- **(MIT OR Apache-2.0) AND Unicode-3.0** (1): unicode-ident 1.0.26
- **Apache-2.0 AND ISC** (1): ring 0.17.14
- **Apache-2.0 AND MIT** (1): dpi 0.1.2
- **Apache-2.0 OR BSL-1.0** (1): ryu 1.0.23
- **Apache-2.0 OR CC0-1.0 OR MIT-0** (1): dunce 1.0.5
- **Apache-2.0 WITH LLVM-exception** (1): target-lexicon 0.12.16
- **BSD-3-Clause AND MIT** (1): brotli 9.0.0
- **CC0-1.0** (1): notify 8.2.0
- **CDLA-Permissive-2.0** (1): webpki-roots 1.0.9
- **MIT AND BSD-3-Clause** (1): matchit 0.8.4
- **MIT OR Apache-2.0 OR 0BSD** (1): adler2 2.0.1
- **MIT OR BSD-3-Clause** (1): brotli-decompressor 6.0.1

<!-- crates:end -->
