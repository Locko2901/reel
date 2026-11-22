# Imports

**Imports** holds how big an upload may be, and a folder of clips to take into the library in
one go. These settings are kept on the server, the same for every PC and browser, and only an
admin sees them. A setting given in `config.toml` or the environment shows read only here, with
where it came from ([Configuration](../configuration.md)). [No app](../setup/no-app.md) tells
the other ways clips get in: the incoming folder, the browser and ShareX.

In the desktop app's standalone library this page holds the same two things for the library on
this PC: how big an upload may be, and a folder of clips on this PC to take in.

## Upload {#upload}

### Upload size limit {#import-upload-max-gb}

The largest file reel takes as an upload: 8 GB by default, 1 to 1000. A bigger one is refused,
and the answer says where the limit is. It counts for an upload in the browser, whole or in
pieces, one from ShareX, and a clip a paired desktop app sends: the app marks such a clip as
failed without sending it. A long session recording of the desktop app can pass 8 GB, so raise
it for those ([Recording a whole session](../desktop-app.md#recording-a-whole-session)). The
incoming folder and [Scan a folder](#scan-folder) have no such limit.

The reverse proxy in front has a request limit of its own. The browser and the desktop app send
a file in pieces of the [Chunk size](devices.md#device-chunk-mb), so the proxy only has to take
a piece; ShareX sends the whole file in one request, so for it the proxy has to allow up to this
limit ([2. Put a reverse proxy in front](../setup/server.md#2-put-a-reverse-proxy-in-front)). An
upload in pieces that nothing touches for a while
([Unfinished uploads expire](devices.md#device-upload-expiry-hours) under Devices, 24 hours by
default) is removed with its file, and starts again from the beginning.

In the standalone library no proxy stands in front: this limit is the only one.

In `config.toml`: `upload_max_gb` under `[import]`; environment: `REEL_UPLOAD_MAX_GB`; every
value it takes: [Configuration](../configuration.md#upload-size-limit).

## Scan a folder {#scan-a-folder}

Takes a folder of clips you already have, such as old recordings, into the library in one go.
Every clip file in it gets an ingest of its own, copied or encoded again as Settings, **Ingest**
says, as if it had come in through the incoming folder, and is published. **Scan now** copies the
files and leaves the folder untouched, and a second scan skips what it imported before;
**Move into reel** takes the files themselves, as the incoming folder does, and they leave the
folder.

A file is taken when it sits directly in the folder and its ending is one of the
[Extensions](ingest.md#ingest-extensions). Subfolders are left alone, and so are the names reel
never takes: a leading dot, or an ending of `.part`, `.tmp`, `.crdownload` or `.txt`. The same
video twice is one clip, whatever its name. The clips belong to the admin who scanned.

### Folder {#scan-folder}

The folder to scan, as a path reel can read. On a server that is a path inside its container,
such as `/mnt/clips/old`: mount the host's folder there in the compose file first. It cannot be
reel's own incoming, originals, data or library folder, nor a folder inside one of them.

- **Scan now** copies each file into the data folder first, so the data disk needs room for them
  while they go in, and leaves your folder untouched. A second scan of the same folder skips each
  file it imported from there before, so you can scan it again after adding files.
- **Move into reel** (it asks "Sure?") takes the files themselves: each is removed from your
  folder once its clip is in, its raw file kept or not as
  [Keep originals](storage.md#ingest-keep-originals) says. reel has to be allowed to delete files
  in that folder.

The answer says how many files were queued, how many were imported before and skipped, and how
many were skipped for other reasons. The clips show on the Jobs page and in the library as they
land.

In the standalone library it is a folder on this PC, such as `D:\Videos\old clips` on Windows or
`/home/you/Videos/old clips` on Linux.
