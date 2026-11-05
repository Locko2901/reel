# Backing up

Everything reel knows lives in a few folders. Keep a copy of the right
ones and you can bring your whole library back, titles, versions, links
and settings included, on the same machine or a new one. This page says
what to keep on a server, how to copy its database safely, how to
restore it, and where the desktop app keeps its own things.

## What to keep on a server

The folders are the ones your `docker-compose.yml` mounts. With the
example compose file:

| What | Where | Back it up? |
|---|---|---|
| The database: every clip, version, link, collection, user, paired device and setting | `./data/reel.db` | Yes, always. Copy it as [below](#copy-the-database-safely). |
| The clips: a folder per clip with every version (the original, the cuts, the preview videos), its poster, hover loop and filmstrip; the trash in `.trash/`; the `.reel-library` file | `./data/library/` | Yes. With a [Library folder](configuration.md#library-folder) set, back up that folder instead. |
| Settings written in a file, if you made one | `./data/config.toml` | Yes, when it is there. Settings changed in reel's own Settings are in the database. |
| The raw file of each clip reel had to encode again | `./originals` | If you want the raw recordings back. reel plays every clip without them. |
| Your compose file, with the environment it sets | `docker-compose.yml` | Yes. |
| The copies under a size, the web copies, the GIFs and WebPs | `./data/cache/` | No: each is made again when it is next asked for. |
| Uploads on their way in | `./data/uploads/` | No. |
| The app's installers of the server's own version, for paired apps; those of other versions are removed | `./data/app-updates/` | No: the server fetches them again from the release. |
| Raw clips waiting to be taken in | `./incoming` | No: it is a drop folder, emptied as reel takes each file. |

The clips are almost all of the space; the database and the rest are
small. [Fast and bulk storage](bulk-storage.md) has the folders in
detail, and how to put the library on a NAS.

## Copy the database safely

reel's database is SQLite, and while reel runs it keeps recent changes in
two files beside it (`reel.db-wal` and `reel.db-shm`). A plain copy of
`reel.db` taken while reel writes can be broken. Take one of these
instead:

=== "With reel stopped"

    The simplest and safest: stop reel, copy, start it again.

    ```
    docker compose stop
    cp -a ./data ./originals docker-compose.yml /mnt/backup/reel/
    docker compose start
    ```

    Stopped, everything in `./data` is consistent, so any copy tool
    works.

=== "From reel's Settings"

    As an admin, open Settings, **Advanced**, **Export**, and press
    **Download** beside **The database**. You get `reel-<date>.db`, a
    consistent copy taken while reel runs.

    The copy has its secrets blank: the external storage's **Secret
    key**, the **ntfy token** and the **Release token**. After a restore
    from it, enter those again where you used them.

=== "With sqlite3 on the host"

    The `sqlite3` tool (package `sqlite3` on Debian and Ubuntu) takes a
    consistent copy while reel runs, secrets included:

    ```
    sqlite3 ./data/reel.db "VACUUM INTO '/mnt/backup/reel/reel.db'"
    ```

    The target file must not exist yet. Run it as a user that may write
    the data folder: reel writes it as user 1000, so use `sudo` if your
    user is another.

With the database copied, copy the library and the originals; those are
plain files and can be copied while reel runs. Copy the database first,
then the folders, so every clip the copy knows has its files in the
backup.

## A nightly backup

A short script that copies the database while reel runs, then the
folders, skipping what reel makes again:

```sh
#!/bin/sh
set -eu
cd /srv/reel
to=/mnt/backup/reel
mkdir -p "$to"
rm -f "$to/reel.db.new"
sqlite3 ./data/reel.db "VACUUM INTO '$to/reel.db.new'"
mv "$to/reel.db.new" "$to/reel.db"
rsync -a --exclude 'reel.db*' --exclude /cache/ --exclude /uploads/ --exclude /app-updates/ ./data/ "$to/data/"
rsync -a ./originals/ "$to/originals/"
cp docker-compose.yml "$to/"
```

Change `/srv/reel` to the folder with your compose file and
`/mnt/backup/reel` to your backup disk. Save it as
`/usr/local/bin/reel-backup`, make it executable, and run it every night
at 4 from root's crontab (`sudo crontab -e`):

```
0 4 * * * /usr/local/bin/reel-backup
```

This keeps one copy that each run brings up to date. For older versions
too, back up `/mnt/backup/reel` with a tool that keeps snapshots, such
as restic or borg.

## Restore a server

1. Stop reel if it runs: `docker compose down`.
2. Put the folders back where the compose file mounts them: the data
   folder (with `config.toml` and `library`, the hidden `.reel-library`
   and `.trash` included) and the originals, with an empty `incoming`
   folder beside them. Put your compose file back too.
3. Put the database copy in place as `./data/reel.db`. Delete any
   `reel.db-wal` and `reel.db-shm` beside it: they belong to another
   database.
4. Give the folders to the user reel runs as in the container:

    ```
    sudo chown -R 1000:1000 ./data ./incoming ./originals
    ```

5. Start reel with the same version as the backup, or a newer one:
   `docker compose up -d`. A newer reel brings the database up to date
   at start; an older one refuses a database moved past it
   ([Updating](updating.md#going-back-to-an-older-version)).
6. Open the Status page. **Doctor** checks the folders and the database,
   and the library shows your clips.

After a restore from Settings' **Export**, enter the secrets again: the
external storage's **Secret key** (Settings, **Sharing**), the **ntfy
token** (Settings, **Notifications**) and the **Release token**
(Settings, **Devices**), where you used them.

If a poster, a hover loop or a filmstrip is missing afterwards, Settings,
**Library and storage**, **Rebuild posters, loops and filmstrips** makes
them again, and **Remake the preview videos** does the same for the link
previews.

Paired apps stay paired: their devices are in the database.

## The desktop app

The app keeps its settings in one folder and everything else, a
standalone library included, in another:

| What | Windows | Linux |
|---|---|---|
| The settings (`settings.json`) | `%APPDATA%\reel` | `~/.config/reel` |
| The pairing with your server | the Windows Credential Manager | `~/.config/reel/token` |
| A standalone library: its clips, its database, its copies | `%LOCALAPPDATA%\reel\library`, or the folder you moved it to | `~/.local/share/reel/library`, or the folder you moved it to |
| The logs | `%LOCALAPPDATA%\reel\logs` | `~/.local/share/reel/logs` |
| The settings of reel's own OBS | `%LOCALAPPDATA%\reel\obs\config` | `~/.local/share/reel/obs` (OBS from Flathub: `~/.var/app/com.obsproject.Studio/reel`) |

Started with `REEL_DESKTOP_DIR` set, the app keeps all of it in that one
folder instead: the settings under `config`, the rest under `data`
([Configuration](configuration.md)).

The clips folder, where OBS writes its replays, is a folder of yours:
Settings, **Clips folder and cache** (**Clips folder** in standalone)
names it.

### Back up a standalone library

The standalone library is a whole reel inside the app, with its own
database, so the same rule holds: copy it with the app stopped, or take
a consistent copy of the database while it runs.

The app does it for you: Settings, **Library folder**, **Make a copy**
puts a complete copy, every file checked, into a new
folder on the drive you pick, while reel runs. See
[A copy of the library](setup/standalone.md#a-copy-of-the-library).

By hand:

1. Quit reel from its tray icon.
2. Copy the library folder to your backup drive, and the settings folder
   with it. Settings, **Library folder** says where the library is:
   `%LOCALAPPDATA%\reel\library` on Windows and
   `~/.local/share/reel/library` on Linux, unless you moved it.
3. Start reel again.

To use a copy the app made, on this PC or a new one: Settings, **Library
folder**, **Move**, and pick the copy's folder. To
restore a copy made by hand, quit reel, put the folders back, and start
it. On a new Windows PC the pairing is not in those folders: pair the
app again under Settings, **Server**.
