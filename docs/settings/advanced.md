# Advanced

The server's log and its level, the check for new releases, the exports of the database and the
settings, the first run again, the doctor, a restart, and the history of every change made under
Settings. These settings are kept on the server, the same for every PC and browser. A setting
given in `config.toml` or the environment shows read only here, with a padlock and where it came
from. The guides: [The logs](../troubleshooting.md#the-server) and
[Where to look first](../troubleshooting.md#where-to-look-first) for a problem,
[Know when a release is out](../updating.md#know-when-a-release-is-out),
[Copy the database safely](../backup.md#copy-the-database-safely).

In the app's standalone mode this page holds the settings of the library on this PC, kept on this
PC: [Repository](#advanced-update-repo), the export of [The database](#export-database),
[Doctor](#doctor) and the [Settings history](#settings-history). The rest is a server's. The
app's log is under Settings, **Status**, and the library's check for new versions is **Look for
new versions on GitHub** on the app's **App** page.

## Logs {#logs}

The page has two groups named Logs: the first holds **Log level**, and the one further down,
below **The process**, shows the log itself. It shows the last 200 lines this process wrote,
oldest at the top, with share tokens shortened.

- **Refresh** reads the last lines again.
- **Download the last hour** saves the lines of the last hour as a text file,
  `reel-log-<date>-<time>.txt`, for a bug report. reel keeps the newest 2000 lines in memory, so
  a busy hour can come back shorter, and a restart starts the lines afresh.

The container's own log has everything the server did, without secrets:
`docker compose logs reel` ([The server](../troubleshooting.md#the-server)). The app's standalone
mode has neither group.

### Log level {#advanced-log-level}

How much the log says: **errors only**, **warnings**, **info** (the default) or **debug,
everything**. A change applies at once, to the container's log and to the lines on this page.
Set **debug** to find a problem: make it happen again, download the last hour, then set the level
back. When the environment sets `RUST_LOG`, it wins, and this setting changes nothing while it
is set.

In `config.toml`: `log_level` under `[advanced]`; environment: `REEL_LOG_LEVEL`; every value it
takes: [Configuration](../configuration.md#log-level).

## Updates {#updates}

### Check for updates {#advanced-update-check}

On (the default), the server asks GitHub for the newest public release of the
[Repository](#advanced-update-repo) a minute after it starts and then once a day. The request
carries reel's version and nothing else. When a newer one is out:

- the Status page's update line says so, with a link to the release notes;
- the **Version** line of the status chip at the top of the pages says so;
- [An update is available](notifications.md#notify-events) sends a note, if it is on under
  Notifications.

**Check now** on the Status page asks at once while this is on. Off, reel asks GitHub nothing,
and the Status page has no **Check now**. Updating the server:
[Update the server](../updating.md#update-the-server).

The app's standalone mode does not show it here: the library's check is **Look for new versions
on GitHub** on the app's **App** page ([Updates](../desktop-app.md#updates)).

In `config.toml`: `update_check` under `[advanced]`; environment: `REEL_UPDATE_CHECK`; every
value it takes: [Configuration](../configuration.md#check-for-updates).

### Repository {#advanced-update-repo}

The GitHub repository reel's releases come from, as `owner/name`: `Locko2901/reel` by default.
**Check for updates** reads its newest public release, and while
[Offer app updates](devices.md#device-app-updates) is on, the server fetches the desktop app's
installers from its releases. Change it only for a fork of your own; a private fork's
installers also need the [Release token](devices.md#device-updates-token). In the app's
standalone mode it is the repository the library's check for new versions asks.

In `config.toml`: `update_repo` under `[advanced]`; environment: `REEL_UPDATE_REPO`; every value
it takes: [Configuration](../configuration.md#repository).

## Export {#export}

Both downloads leave every secret blank: the external storage's **Secret key**, the **ntfy
token** and the **Release token**.

### The database {#export-database}

**Download** gives `reel-<date>-<time>.db`, a consistent copy of the database taken while reel
runs: every clip, version, link, collection, user, paired device and setting, with the secrets
blank. It holds no clip files: keep the library folder beside it
([What to keep on a server](../backup.md#what-to-keep-on-a-server)). After a restore from it,
enter the secrets again where you used them ([Restore a server](../backup.md#restore-a-server)).

In the app's standalone mode it is the database of the library on this PC. A copy of the
library with its clips' files is **Make a copy** on the app's **Library folder** page
([Back up a standalone library](../backup.md#back-up-a-standalone-library)).

### The settings as config.toml {#export-config}

**Download** gives `config.toml` with the settings in force, from every layer (reel's defaults,
Settings, `config.toml` and the environment), the secrets blank, and a first line with the time of
the export. Use it to see what a server runs with, or to take its settings to another server.
Every key in a `config.toml` shows read only under Settings, the blank secrets included, so keep
only the lines you want fixed before you put it in place
([Configuration](../configuration.md)). The app's standalone mode does not have it.

## The process {#the-process}

### The first run {#setup-again}

**Run it again** opens the first run's six steps (Welcome, Folders, Hardware, Access, Sharing,
Done) over the current values. Nothing is reset: each step saves as you change it, as a setting
here does, and **Skip the rest, use the defaults** only ends the run. Use it to check the public
URL and the folders again, or for the app's installers on its last step
([The first run](../setup/server.md#4-the-first-run)). The app's standalone mode does not have
it.

### Doctor {#doctor}

**Open Status** opens the Status page. Its **Doctor** panel shows what `reel doctor` sees:
ffmpeg, the hardware route and the decoders, the folders, the disk, the database, the jobs, the
sign-in and the app's installers. **Run again** there probes the hardware and the disks again
([Where to look first](../troubleshooting.md#where-to-look-first)). In the app's standalone mode
it is the Status page of the library on this PC.

### Restart reel {#restart}

**Restart**, after **Sure?**, stops reel cleanly a moment later; whatever runs it starts it
again, and the page reloads by itself. In Docker that is the container's restart policy: the
example compose file's `restart: unless-stopped` does it. With no restart policy, or one that
restarts only after a failure, reel stays stopped, as it exits without an error. A job running at
the time starts again after the restart.

Use it for a setting marked **after a restart**, or after you edited `config.toml`. A change to
the compose file's environment needs `docker compose up -d` instead, which makes the container
again. The app's standalone mode does not have it: a restart would stop the app's library for
good.

## Settings history {#settings-history}

The last 500 changes made under Settings and in the first run, newest first: the setting, its
value before and after the change, who made it and when. **the value under the table** is the
value without one saved under Settings, as after a **Reset**: reel's default, or what
`config.toml` or the environment gives. A secret's values never show. A token cleared with its ntfy server
has a line of its own. Changes made in `config.toml` or the environment are not listed.

**Revert**, after **Sure?**, sets the setting back to its value before that change. The revert is
a change of its own, so it shows in the history too. A setting given in `config.toml` or the
environment since that change cannot be reverted here, and reverting a secret's line clears what Settings
holds for it. A setting marked **after a restart** waits for the next start, as usual.

In the app's standalone mode it is the history of the library's settings on this PC.

### This PC's changes {#this-pcs-changes}

In the app's window the table lists the changes of the app's own settings too, among the
server's by time: every setting of the pages under This PC, Library folder and Encoding, and
of the setup. Who says **this PC** (its device name on the pointer), then where the change came
from: **Settings**, **the setup**, **Revert**, **the command line**, or **reel** when the app
changed one itself, as when it went back from an encoder that did not record. A program's own
settings under Games and apps show with its name, such as **Dust Runner, Class**. A password says
**changed**, never its values. The app keeps its last 500 changes in `settings-history.jsonl`
beside its settings, kept by an update and gone with the settings.

**Revert** on a line of this PC, after **Sure?**, puts the old value back through the same save as
its page, with the same checks, and adds a line from **Revert**. Some lines do not go back from
here and say where to change the setting instead: **Mode** and **Server URL** (Connection),
**Library folder** (a move), the **Screen** and the **Encoder** (Capture), the obs-websocket
**Password**, and a program's **Name** (its page under Games and apps).

A browser shows the server's lines alone: this PC's changes stay on this PC.
