# Building from source

reel is open source under the MIT licence, and everything it ships
builds from its repository. Every build and check runs in a container,
so Docker and `make` are all they need. This page gives the short way to
each build;
[CONTRIBUTING.md](https://github.com/Locko2901/reel/blob/main/CONTRIBUTING.md)
and [desktop/BUILD.md](https://github.com/Locko2901/reel/blob/main/desktop/BUILD.md)
have the details.

```
git clone https://github.com/Locko2901/reel.git
cd reel
make
```

`make` alone lists every target.

## The server's image

```
make image
```

builds the image from the repository's `Dockerfile` and tags it
`reel:local`. To run your build, put `image: reel:local` in your
`docker-compose.yml` in place of `ghcr.io/locko2901/reel:latest`, then
`docker compose up -d`. The image is built for x86_64 (amd64).

## The checks

```
make check
```

runs what CI runs, in a container: the formatting, clippy with warnings
as errors, and every test, the end to end tests among them, which run a
real server against synthetic clips. `make desktop-check` does the same
for the desktop app.

## A demo server

```
scripts/demo.sh serve
```

builds reel with a local Rust toolchain and starts a demo at
`http://127.0.0.1:8746`, with its data in `./data`. Sign in as `sam`
with the password `reel demo 8746`. `scripts/demo.sh seed` drops a few
synthetic clips into its incoming folder, and `scripts/demo.sh stop`
stops it. The demo runs on your machine rather than in a container, so
it needs ffmpeg too: it looks in `~/.local/ffmpeg`, or set `REEL_FFMPEG`
and `REEL_FFPROBE` to yours.

## The desktop app

| Command | What you get |
|---|---|
| `make desktop-windows` | both Windows installers, **reel** (with OBS) and **reel light**, cross compiled on Linux |
| `make desktop-appimage` | the Linux AppImage and the `.deb` |
| `make desktop-check` | the desktop app's formatting, clippy and tests |

The files land in `~/reel-desktop-out`, each with its `.sha256`. With
`TAURI_SIGNING_PRIVATE_KEY` set, each also gets the `.sig` the app's
updater checks.

## Contributing

Bug reports, ideas and pull requests are welcome. For anything bigger
than a fix, open an issue first. CONTRIBUTING.md has the layout of the
code, how it is written and how commits are named.
