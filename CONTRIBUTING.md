# Contributing to reel

Thanks for wanting to help. Bug reports, ideas and pull requests are all welcome. For anything
bigger than a fix, open an issue first so we can agree on the shape before you write it.

## What you need

Docker and `make`. Every build and test runs in a container, so nothing else goes on your
machine. A local Rust toolchain (stable) and ffmpeg also work for the server with `make
check-host`.

## The layout

| Where | What |
|---|---|
| `crates/core` | settings, config, names, the edit recipes and plans |
| `crates/media` | ffmpeg and ffprobe: the arguments, probing, hardware encoders |
| `crates/store` | SQLite through sqlx, with its migrations |
| `crates/pipeline` | the jobs: ingest, cuts, previews, montages, external storage |
| `crates/server` | the web server: pages, the API, accounts, share links |
| `crates/reel` | the `reel` binary and the end to end tests |
| `web/` | the templates, the stylesheet and the scripts of every page |
| `desktop/` | the desktop app, its own cargo workspace: `core` (no GUI), `app` (Tauri), `ui` (its pages), `ci` (its builds and checks) |
| `docs/` | the user site, built with MkDocs Material |
| `deploy/` | the example Docker Compose file and config |

## Build and test

```
make check            # fmt, clippy with warnings as errors, every test, in the check container
make desktop-check    # the same for the desktop app
make docs             # the user site, strictly (a broken link fails it)
scripts/demo.sh serve # a demo server with synthetic clips on 127.0.0.1:8746
```

`make` alone lists every target. [`desktop/BUILD.md`](desktop/BUILD.md) has the installers'
builds and the app's checks under Xvfb and Wine.

`docs/configuration.md` is made from the settings catalogue in `crates/core/src/settings.rs`:
after changing a setting, run `make docs-config`, or `make check` fails.

Migrations under `crates/store/migrations` are never edited once released (sqlx checks their
checksums): a change to the schema is a new numbered file.

## How the code is written

- `cargo fmt` and `cargo clippy --all-targets -- -D warnings` clean.
- Few comments. A short docstring where a name does not already say what something is for, and
  a short comment only where the code cannot say why (a platform quirk, an order that matters,
  a number that looks arbitrary). No commented out code, no history, no banners.
- Plain ASCII text: no em or en dashes or other dashes, no special spaces, no typographic quotes
  or ellipsis characters, no emoji or pictographic symbols, in code, UI strings, docs and commit
  messages. `make check` runs `ci/check-text.sh`, which fails on them. Letters with accents are
  fine where they belong, such as a name or a test's unicode input.
- Tests for what you change. The end to end tests in `crates/reel/tests/e2e.rs` run a real
  server against synthetic clips made with ffmpeg.
- UI: one accent, 1 px rules, no gradients, every control the same height, usable on a phone
  (44 px targets). Look at a few existing pages before adding one.

## Commits and pull requests

- [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`,
  `refactor:`, `test:`, `ci:`, `chore:`, with a scope where it helps (`fix(desktop): ...`).
  release-please writes the changelog and the releases from them, so a `feat:` or `fix:` line
  is what users read.
- One change per pull request, with what it does and how you checked it. CI runs `make check`
  on every pull request (and `make desktop-check` when `desktop/` changed).
- By contributing you agree that your work is released under the [MIT licence](LICENSE).

## Security

Please do not open a public issue for a vulnerability: see [SECURITY.md](SECURITY.md).
