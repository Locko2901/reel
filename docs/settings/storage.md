# Library and storage

**Library and storage** holds where reel keeps its files (the database, the library, the
incoming clips and the kept originals), how long the trash and the copies under a size keep
things, the library cap, and what the disks are doing, with buttons that tidy up at once. These
settings are kept on the server, the same for every PC and browser, and only an admin sees
them. The **Data folder** and the **Library folder** are read when the process starts, so they
are always read only here; any other setting given in `config.toml` or the environment shows
read only too, with where it came from ([Configuration](../configuration.md)).
[Fast and bulk storage](../bulk-storage.md) tells the story of the folders,
[Delete and restore](../sharing.md#deleting-a-clip) the trash, and
[The library cap](../sharing.md#the-library-cap) the cap.

In the desktop app's standalone library this page holds the settings of the library on this PC:
**Keep originals**, **Trash**, **Copies under a size** and **Disk warning**, and the group
**Now** without **Clear the web copies** and **Remake the preview videos**. Its line says where
the library is; the library's folder and its cap are on the app's **Library folder** page.

## Folders {#folders}

### Data folder {#data-dir}

The folder for the database (`reel.db`), the caches (the copies under a size and the animated
pictures in `cache/discord`, the web copies in `cache/web`), the uploads on their way in
(`uploads/`) and the installers paired apps update from (`app-updates/`). The library lives in
it too, as `library/`, unless the **Library folder** puts it elsewhere.

The database is touched on every page and every job, and SQLite needs a disk it can lock: keep
the data folder on a fast local disk, never on a network share. The line under the row says
which disk it is on and how much is free there, and warns when it is on a network share such as
NFS or SMB ([Keep the database off the network](../bulk-storage.md#keep-the-database-off-the-network)).

reel reads it when the process starts, so it is read only here. The image sets `/data`; to keep
it elsewhere, mount another folder of the host at `/data` in the compose file
([8. Folders and backups](../setup/server.md#8-folders-and-backups)).

In `config.toml`: `data_dir`; environment: `REEL_DATA_DIR`; every value it takes:
[Configuration](../configuration.md#data-folder).

### Library folder {#library-dir}

The clips themselves: a folder per clip with every version (the original, the cuts, the preview
videos), its poster, hover loop and filmstrip, and the trash in `.trash`. This is almost all of
reel's space and it grows with every clip, so it may live on a NAS or a big slow drive. Unset,
it is the `library` folder in the data folder.

A file `.reel-library` in it, with this reel's id, tells reel it is the right folder. While the
database holds clips and that file is missing (a NAS switched off, a mount that failed), reel
writes nothing there, and the line under the row says the folder is not usable and why. reel
goes on by itself once the folder is back
([When the library is not there](../bulk-storage.md#when-the-library-is-not-there)). Otherwise
the line says whether it is on the data disk or another one, and how much is free.

The library cannot hold the data folder, nor sit inside the incoming folder: reel does not start
with such a folder. reel reads it when the process starts, so it is read only here: set
`REEL_LIBRARY_DIR` in the compose file and mount the folder there
([A recommended layout](../bulk-storage.md#a-recommended-layout)). To move a library that holds
clips, follow [Move an existing library](../bulk-storage.md#move-an-existing-library).

In `config.toml`: `library_dir`; environment: `REEL_LIBRARY_DIR`; every value it takes:
[Configuration](../configuration.md#library-folder).

### Incoming folder {#ingest-incoming-dir}

The folder reel watches for raw clips. A recorder or a sync tool drops files there; reel takes
each one in and publishes it, and the file leaves the folder, its raw file kept only when
[Keep originals](#ingest-keep-originals) says so. A file ffmpeg cannot read moves to `failed/`
inside it, with a note that says why
([Troubleshooting](../troubleshooting.md#a-clip-is-in-the-incoming-folders-failed-folder)).
Which files it takes, and when, is under Settings, **Ingest**, **Incoming folder**
([Poll every](ingest.md#ingest-poll-seconds), [Stable polls](ingest.md#ingest-stable-polls),
[Extensions](ingest.md#ingest-extensions)). [No app](../setup/no-app.md#1-obs-into-the-incoming-folder)
shows how to point OBS at it.

A folder set here has to exist; the first run's **Create it** makes a missing one. reel reads
the new folder from its next look, a few seconds later. A folder that is not there is not read,
and nothing is taken while the library folder is not usable: the files wait.

In the image it is `/incoming`, set by the image's own environment, so the row is read only:
mount the host folder you want at `/incoming` in the compose file. The standalone library has no
incoming folder: the app hands its clips over itself.

In `config.toml`: `incoming_dir` under `[ingest]`; environment: `REEL_INCOMING_DIR`; every value
it takes: [Configuration](../configuration.md#incoming-folder).

### Originals folder {#ingest-originals-dir}

Where reel keeps the raw file of a clip, as [Keep originals](#ingest-keep-originals) says: the
file as it came, named by the clip's id with its own ending. **Details** on the clip's page then
offers **Download the original**. reel never plays from this folder, and it does not delete a
kept original, not even when its clip is deleted for good, so the folder only grows: a NAS or a
big slow drive suits it.

A folder set here has to exist; the first run's **Create it** makes a missing one. A change
applies to the next raw file kept; the ones kept before stay where they are. The **Disk** line
under **Now** counts what the folder holds, or says it is not reachable.

In the image it is `/originals`, set by the image's own environment, so the row is read only:
mount the host folder you want at `/originals` in the compose file.

In `config.toml`: `originals_dir` under `[ingest]`; environment: `REEL_ORIGINALS_DIR`; every
value it takes: [Configuration](../configuration.md#originals-folder).

### Keep originals {#ingest-keep-originals}

Which raw files reel keeps in the [Originals folder](#ingest-originals-dir) once it has taken a
clip in, whether the file came from the incoming folder, an upload, a paired app or a scanned
folder:

- **re-encoded clips only**, the default: the raw file of a clip whose video reel encoded again,
  because it is not H.264, is taller or faster than
  [Copy video up to](ingest.md#media-copy-max-height) and
  [Copy video under](ingest.md#media-copy-max-mbit) allow, or is not 8-bit 4:2:0. A clip whose
  video is copied as it is keeps the recording's own picture in the library, so its raw file goes.
- **every clip**: every raw file, copied or not. Choose it to keep the recorded sound too, when
  the audio is re-encoded or [Normalise loudness](ingest.md#media-normalize-audio) is on.
- **none**: no raw file; each is deleted once its clip is in.

A change applies from the next clip on. In the standalone library the raw files are kept on this
PC with the library, and **Details** has **Open** and **Show in folder** for them.

In `config.toml`: `keep_originals` under `[ingest]`; environment: `REEL_KEEP_ORIGINALS`; every
value it takes: [Configuration](../configuration.md#keep-originals).

## Trash and caches {#trash-and-caches}

### Trash {#share-trash-days}

How many days a deleted clip waits in the trash before reel deletes it for good: 7 by default,
1 to 365. Until then, **Undo** in the message after a delete, or **Restore** in the trash, brings
it back with its links ([Delete and restore](../sharing.md#deleting-a-clip)). A clip the library
cap moved waits the same days.

Housekeeping runs every 10 minutes and deletes the clips whose days are over, counted from
their delete. A shorter value therefore applies to the clips already in the trash at its next
run. The trash is `.trash` in the library folder, so a delete and its **Undo** are a rename on
the same disk ([The trash lives in the library](../bulk-storage.md#the-trash-lives-in-the-library)).
The disk has a clip's room back only once it goes for good; [The trash](#open-trash) under
**Now** empties it at once.

In `config.toml`: `trash_days` under `[share]`; environment: `REEL_TRASH_DAYS`; every value it
takes: [Configuration](../configuration.md#trash).

### Copies under a size {#share-discord-cache-gb}

How much disk the copies under a size and the animated pictures may take together: 5 GB by
default, 1 to 500 GB. A copy under a size is the file the clip page's **Send** makes to fit a
chat's upload limit, and the animated GIFs and WebPs are made there too
([Send the file itself](../sharing.md#send-the-file-itself)). Each is made once and kept, so the
next download of it is ready at once.

Housekeeping, every 10 minutes, deletes the ones made longest ago until the rest fit. A copy
under a size is made again when someone asks for it; an animated picture only when someone makes
it again. They live in the data folder's `cache/discord`, never in the library, and the library
cap does not count them.

In `config.toml`: `discord_cache_gb` under `[share]`; environment: `REEL_DISCORD_CACHE_GB`;
every value it takes: [Configuration](../configuration.md#copies-under-a-size).

### Disk warning {#storage-disk-warn-gb}

The free space under which a disk counts as low: 20 GB by default; 0 turns the warning off.
reel watches each disk that holds its files once: the data folder's, the library's and the kept
originals' ([See where everything is](../bulk-storage.md#see-where-everything-is)). A disk under
it shows amber on the Status page, named in its **disk** line. With Notifications set up, the
event "the disk is low" sends a note each time a disk falls under it
([Events](notifications.md#notify-events)); housekeeping checks every 10 minutes.

In the standalone library it watches the disk that holds the library, and sends nothing.

In `config.toml`: `disk_warn_gb` under `[storage]`; environment: `REEL_DISK_WARN_GB`; every
value it takes: [Configuration](../configuration.md#disk-warning).

### Library cap {#storage-cap-gb}

The size reel keeps the library under by itself, so old clips never fill the disk: 0, the
default, is no cap; at most 100000 GB. It counts each clip's versions and its preview video, not
the copies under a size, the web copies, the kept originals or the trash. At each housekeeping
run, every 10 minutes, it moves clips to the trash until the library is under it:

- the oldest first, by when they were recorded, and a clip with a live link only after every
  clip without one;
- the desktop app's older session recordings before the clips;
- never a clip someone starred, nor one in a collection someone made.

A clip it moves is like a deleted one: its links stop, and **Restore** in the trash brings it
back with them for [Trash](#share-trash-days) days.

The line under the row says what the library holds and, as you type a size, what the cap would
move at its next run and what it spares. If the line says the cap waits, nothing moves until you
save the cap or change it. With Notifications set up, the event "the library cap moved clips"
tells you once a run. [The library cap](../sharing.md#the-library-cap) has the whole story. In
the standalone library the cap is on the app's **Library folder** page instead.

In `config.toml`: `cap_gb` under `[storage]`; environment: `REEL_LIBRARY_CAP_GB`; every value it
takes: [Configuration](../configuration.md#library-cap).

### The cap spares unsent clips {#storage-cap-keeps-unsent}

A setting of the desktop app's standalone library, where a clip can be on this PC and on no
server: on, the default, the library cap never moves a clip that is on no server yet. There it
shows on the app's **Library folder** page, under **Library cap**
([How it treats the PC while you play](../setup/standalone.md#how-it-treats-the-pc-while-you-play)).

A server leaves it out: every clip it holds is on a server, so its Settings never show the row,
and a value in `config.toml` or the environment changes nothing there.

In `config.toml`: `cap_keeps_unsent` under `[storage]`; environment:
`REEL_LIBRARY_CAP_KEEPS_UNSENT`; every value it takes:
[Configuration](../configuration.md#the-cap-spares-unsent-clips).

## Now {#now}

The trash is purged on its own after its days, and the copies under a size are trimmed to their
size; these buttons do it, and a few other chores, at once. Each acts the moment you press it
and says what it did beside it. The jobs a button queues show on the Jobs page.

### The trash {#open-trash}

The line beside the row says what the trash holds: "empty", or how many clips and their size on
disk ("3 clips, 1.2 GB on disk"), the same figure as the trash in the **Disk** line.

- **Open** goes to the trash page. It lists every deleted clip with who deleted it and when, its
  size and how long it has left; there **Restore** brings one back with its links, and
  **Delete now** deletes one for good ([Delete and restore](../sharing.md#deleting-a-clip)).
- **Purge now** (it asks "Sure?") deletes every clip in the trash for good, at once, with its
  copies under a size and animated pictures. Nothing comes back: their **Undo** is over. A kept
  original stays in the Originals folder. Use it when the disk needs the room before the clips'
  [Trash](#share-trash-days) days are up.

### Clear the copies under a size {#clear-discord}

Deletes every copy under a size and every animated GIF and WebP at once, whatever
[Copies under a size](#share-discord-cache-gb) allows; the answer says how many went and how much
room came back. A copy under a size is made again when someone asks for it on a clip's **Send**,
which takes its encoding time again. An animated picture comes back only when someone makes it
again with **Make and download**.

Use it to get the room back at once, or after changing
[Audio for copies](ingest.md#media-copy-audio) or the
[x264 preset for copies](ingest.md#media-x264-preset): the copies made under the old setting are
not offered any more, yet keep their room until the oldest go.

### Clear the web copies {#clear-web}

Deletes every web copy at once: the sharper copies a link's page plays in a browser
([What the chat and the page play](../sharing.md#what-plays-where)). Each page plays its preview
video until its copy is made again, which starts on its next view.

Housekeeping already removes, every 10 minutes, the web copies no live link plays and those made
for another [Web copy bitrate](sharing.md#share-web-mbit) or
[Web copy height](sharing.md#share-web-max-height); this button removes them all. Not in the
standalone library, which has no links.

### Rebuild posters, loops and filmstrips {#rebuild-derived}

Queues one job per clip that makes the files beside each of its versions again: the poster, the
hover loop and the filmstrip. The current version's poster comes from the time the clip keeps
for it, so a frame you picked stays, and so does an image you uploaded. The jobs run at a low
priority, after ingests, cuts and copies; the answer says how many.

Use it when some of these files are missing, after a restore or a move that left them behind
([Restore a server](../backup.md#restore-a-server)), or to make them all again
([8. Folders and backups](../setup/server.md#8-folders-and-backups) lists them).

### Remake the default posters {#remake-posters}

Gives every clip whose poster reel chose by itself that poster again, by reel's rule: a quarter
of the way into the clip, the first frame that is not black within the next three seconds. A
poster you picked stays: a clip with a frame you chose or an image you uploaded is left alone.

One job per clip, at a low priority after the other work, makes the poster again, with the
clip's hover loop and filmstrip; the answer says how many. Use it when default posters show a
black frame.

### Remake the preview videos {#remake-embeds}

Makes the preview videos that do not fit the current rules. It looks at each clip's current
version that needs a preview video of its own (one over
[Preview video size from the server](sharing.md#share-embed-mb), or not H.264 with AAC) and
queues one when it has none yet, or has one made for another size or another
[Audio for copies](ingest.md#media-copy-audio). Nothing is remade on its own after such a change:
until you press this, a link serves what it served. The answer says how many were queued and
why. A session recording that has no preview yet gets one when a link asks for it, not from
here.

Pressing it deletes each outdated preview at once. With link previews from external storage off,
a link to a version it queued answers "This clip is still being prepared." until the new preview
is made, so press it while your links are quiet. Not in the standalone library, which has no links.

### Disk {#disk-refresh}

The line beside the row says where reel's room goes: every version of every clip (the originals,
the cuts and the preview videos); the library on disk with its posters, loops and filmstrips; the
caches (the copies under a size, the web copies); the trash; the uploads on their way in; the
kept originals, or that the Originals folder is not reachable; and each disk with its free space
("data disk 412.0 GB free of 485.2 GB").

reel counts the folders at most once a minute, and the trash afresh each time. **Refresh**
counts everything again at once, for after you deleted or moved files yourself. In the
standalone library the line leaves out the preview videos and the web copies, which it does not
have.
