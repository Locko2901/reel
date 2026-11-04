# Maintaining reel

What only the repository's owner needs, or someone who runs a fork of it: the settings the
workflows read, how a release goes, and the weekly OBS bump. Contributing is in
[CONTRIBUTING.md](CONTRIBUTING.md).

## Secrets and variables

In the repository's Settings, Secrets and variables, Actions:

| Name | Kind | What for |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | secret | signs the installers and the AppImage for the desktop app's updates |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | secret | that key's password |
| `DOCS_PRIVATE_WORDS` | secret | words the docs site's build refuses, comma separated: a deployment's addresses, host names, people's names (`ci/docs/hooks.py`) |
| `REEL_BOT_APP_ID` | variable | the repository's bot, below |
| `REEL_BOT_PRIVATE_KEY` | secret | the bot's private key |

Only the release jobs read the signing key: a push to main and a run by hand. No pull request
job does, and a fork's pull request gets no secret. Keep the key: an app checks each update
against the public key it was built with (`plugins.updater.pubkey` in
`desktop/app/tauri.conf.json`), so a new key cuts every installed app off its updates.

Every job runs on `ubuntu-latest` unless the variable `RUNS_ON` names another runner. Leave it
unset on a public repository: a self-hosted runner would run a fork's pull request on that
machine.

## The repository's bot

Two workflows make commits of their own: the weekly OBS bump (`obs-bump.yml`) opens a pull
request, and the release job (`ci.yml`) pushes the lock files to release-please's pull request.
With the workflows' own token, GitHub starts no checks for either. With a GitHub App of the
repository's, the checks start by themselves. To set it up:

1. Create a GitHub App (your account's Settings, Developer settings, GitHub Apps) with the
   webhook off and two repository permissions, Contents and Pull requests, both read and write.
2. Install it on this repository alone, and generate a private key on its page.
3. Set the variable `REEL_BOT_APP_ID` to the app's ID and the secret `REEL_BOT_PRIVATE_KEY` to
   the key.

Without them, both workflows run as before: close and reopen a pull request to start its
checks.

## A release

1. Each push to main updates release-please's pull request, "chore(main): release X.Y.Z": the
   version bumped everywhere and a new entry at the top of `CHANGELOG.md`, written from the
   commits' first lines.
2. Before merging it, read that entry as a user would. A squash merge's body is not split into
   lines, so write the entry out on the pull request's branch: one line for each change a user
   notices, and no links. The server shows the newest five entries on its Status page from the
   `CHANGELOG.md` built into its image, so the entry has to be right before the merge. The
   release's notes on GitHub come from the pull request's description: make it match, or edit
   the release afterwards.
3. Merging it makes CI tag `vX.Y.Z`, publish the release, give the image built from that commit
   the tags `X.Y.Z`, `X.Y`, `X` and `latest`, and build the installers onto the release: both
   Windows installers, the AppImage and the .deb, each with its `.sig` and `.sha256`, and the
   source of the OBS the Windows installer holds.

To build a release's installers again (a failed upload, a fix to the build), run the workflow
CI by hand (Actions, CI, Run workflow) with the release's tag, and a commit when it should not
be the tag's own. The run replaces the release's files of the same names.

## The OBS bump

On Mondays at 06:17 UTC, `obs-bump.yml` compares OBS's newest release with the pin in
`desktop/obs/pinned.json`. When OBS is newer, it opens the pull request "fix(desktop): bundle
OBS X.Y.Z", which moves the pin and the OBS version in `THIRD_PARTY.md`. A moved pin runs every
desktop check, the real OBS included. Being a fix, it opens the next release's pull request once
merged.
[desktop/OBS.md](desktop/OBS.md) lists what reel uses of OBS, what checks each part, and what to
try on Windows before merging.
