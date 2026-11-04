# Fast and bulk storage

Keep the clips on a big, cheap disk such as a NAS, and the database on the
server's own fast disk. reel's library can live in a folder of its own,
so you choose a disk for each.

reel keeps two kinds of things, and they suit different disks:

- **The bulk:** the clips themselves. Every version of every clip (the
  master, the cuts, the preview videos) with its poster, loop and
  filmstrip, and the trash. This is almost all of the space, it grows with
  every clip, and it is read about as fast as a video plays. A NAS, an
  external drive or a big slow disk suits it.
- **The rest:** the database (`reel.db`), the caches (the copies under a
  size, the web copies the links' pages play) and the uploads in flight.
  Small, but touched on every page and every job, and SQLite wants a local
  disk it can lock. A fast local disk, such as an SSD or the machine's own
  disk, suits it.

The **Data folder** (`REEL_DATA_DIR`) holds the rest, and by default the
library too, in its `library` folder. The **Library folder**
(`REEL_LIBRARY_DIR`) puts the library somewhere else. The kept originals
have their own folder already (`REEL_ORIGINALS_DIR`), and so do the
incoming clips (`REEL_INCOMING_DIR`).

| Folder | Setting | What lives there | Where it goes best |
|---|---|---|---|
| Data | [Data folder](configuration.md#data-folder) | `reel.db`, `cache/`, `uploads/` | a fast local disk |
| Library | [Library folder](configuration.md#library-folder) | a folder per clip with every version, posters, loops and filmstrips; `.trash/`; `.reel-library` | a NAS or a big slow drive |
| Originals | [Originals folder](configuration.md#originals-folder) | the raw file of a re-encoded clip | a NAS or a big slow drive |
| Incoming | [Incoming folder](configuration.md#incoming-folder) | clips waiting to be taken in | wherever the recorder or the sync tool writes |

## A recommended layout

The database and the caches on the machine's own disk, the library and the
originals on the NAS:

```yaml
services:
  reel:
    image: ghcr.io/locko2901/reel:latest
    container_name: reel
    restart: unless-stopped
    ports:
      - "8746:8080"
    environment:
      TZ: Europe/Berlin
      REEL_PUBLIC_URL: https://clips.example.com
      REEL_LIBRARY_DIR: /library
    volumes:
      # the database and the caches: a fast local disk
      - ./data:/data
      # the clips and the trash: the NAS, mounted on the host
      - /mnt/nas/reel-library:/library
      - /mnt/nas/reel-originals:/originals
      - /mnt/nas/clips-incoming:/incoming
    devices:
      - /dev/dri:/dev/dri
```

Mount the share on the host (an NFS or SMB mount in `/etc/fstab`, or the
NAS's own tool) and hand the container the mounted folder, as above.

## See where everything is

Settings, **Library and storage**, **Folders** shows each folder with the
disk it is on, "on the data disk" or "on another disk than the data
folder", and how much is free there. The storage figures (the library
page's disk line, Settings, Status) name each disk once, for what is on
it:
`data disk 412.0 GB free of 485.2 GB; library and originals disk 9.8 TB free of 15.0 TB`.

The [Disk warning](configuration.md#disk-warning) watches each of them:
the data disk, the library's and the originals'.

## The trash lives in the library

A deleted clip moves to `.trash` in the library folder, beside the clips.
Deleting a clip and its **Undo** are a rename on the same disk, whatever
the layout, never a copy over the network.

An older reel kept the trash in the data folder (`trash/`). reel moves
what is there into the library's `.trash` by itself after it starts, a
clip at a time. You can restore or purge a clip while that runs.

## When the library is not there

A NAS that is switched off, or a mount that failed at boot, leaves an
empty folder where the library was, or none at all. reel does not take
that for an empty library. The library folder holds a small file,
`.reel-library`, with this reel's id. While the database holds clips and
that file is missing, reel writes nothing to the library folder:

- nothing is taken in from the incoming folder (the files wait there),
  and uploads are refused with "the library folder is not usable" (the
  app tries again later);
- no clip moves to or from the trash, and housekeeping leaves the library
  alone;
- the library page, Settings and Status say why, `/healthz` answers 503,
  and the log has the line.

Once the library is back, reel notices within ten seconds and goes on by
itself. Nothing needs a restart.

A new install, or one without clips yet, makes the file in an empty
folder at its first start. An install from before the file existed gets
it at its first start too, from the clips' folders it finds.

If the library really is gone (the NAS lost, no backup) and you want reel
to start over in that folder, make an empty file named `.reel-library` in
it: reel takes the folder and writes its id there. If a folder holds
another reel's `.reel-library`, delete that file to give the folder to
this one.

## Keep the database off the network

SQLite's locking is not reliable over NFS or SMB, and a database there can
be damaged. When the data folder is on a network share, Settings and the
log say so. reel refuses nothing, but keep the data folder on a local disk
and put only the library and the originals on the share.

## Move an existing library

1. Stop reel: `docker compose down`.
2. Copy the library to its new place, keeping the folder's contents as
   they are, the hidden `.reel-library` and `.trash` included:

    ```
    rsync -a ./data/library/ /mnt/nas/reel-library/
    ```

3. Set the Library folder (`REEL_LIBRARY_DIR: /library`) and mount the new
   place there, as in the layout above.
4. Start reel: `docker compose up -d`.
5. Once the clips play, delete the old copy in `./data/library`.

When it worked, Settings, **Library and storage**, **Folders** shows the
library "on another disk than the data folder", and the clips play as
before. The `.reel-library` file reel finds confirms the folder. The
database still names each file `library/...`, which now means the
Library folder.

A library with no `.reel-library` in it yet (one an older reel made) gets
the file at that first start too, as long as the clips' folders are in
it.

## The desktop app

All of this is for the server. The desktop app's own library, on the
gaming PC, lives in the app's data folder and has no Library folder
setting.
