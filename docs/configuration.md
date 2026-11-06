<!-- Made by `make docs-config` from the settings catalogue; edit the catalogue, not this page. -->

# Configuration

Every setting of the server, where it is under Settings, its key in `config.toml`, its environment variable and what it accepts.

A value comes from four layers, each over the one before: reel's default, the value saved under Settings, `config.toml`, then the environment. A setting given in `config.toml` or the environment shows read only under Settings, with where it came from. A change under Settings takes effect as it is saved, except where a setting says it waits for the next start.

In the compose file the environment goes under `environment:`, as `REEL_PUBLIC_URL: https://clips.example.com`. `config.toml` lives where `REEL_CONFIG` says, `/data/config.toml` in the image:

```toml
public_url = "https://clips.example.com"

[ingest]
poll_seconds = 5
extensions = ["mkv", "mp4"]
```

## General

The name on the pages, the address links carry, the clock the dates follow, and the editor's quick trims.

### Site name

On the pages, in the share page's footer and in link previews as the site name.

- Under Settings: General, Site
- In `config.toml`: `site_name`, at the top
- Environment: `REEL_SITE_NAME`
- Accepts: text
- Default: `reel`

### Public URL

What every share link starts with; must be reachable from the internet for link previews to play.

- Under Settings: General, Site
- In `config.toml`: `public_url`, at the top
- Environment: `REEL_PUBLIC_URL`
- Accepts: a URL starting with http:// or https://
- Default: `http://localhost:8080`

### Listen address

The address and port the server binds; set when the process starts.

- Under Settings: General, Site
- In `config.toml`: `listen`, at the top
- Environment: `REEL_LISTEN`
- Accepts: text
- Default: `0.0.0.0:8080`
- In the image: `0.0.0.0:8080`, from its environment
- Read when the process starts, from `config.toml` or the environment only

### Time zone

An IANA name such as Europe/Berlin for the dates on the pages; empty follows the host's clock.

- Under Settings: General, Site
- In `config.toml`: `timezone`, at the top
- Environment: `REEL_TIMEZONE`
- Accepts: text
- Default: empty

### Quick trims

The editor's Keep the last buttons, in seconds: each keeps the end of the clip as a new version; 1 to 4 values from 5 to 600.

- Under Settings: General, Clip editor
- In `config.toml`: `quick_trims` under `[edit]`
- Environment: `REEL_QUICK_TRIMS`
- Accepts: a list: comma separated in the environment, an array in `config.toml`
- Default: `15, 30, 60`

## Library and storage

Where the library, the incoming clips and the kept originals live, how long the trash and the caches keep things, and what the disk is doing.

### Data folder

The database and the caches live here, and the library unless the Library folder says otherwise; a fast local disk; set when the process starts.

- Under Settings: Library and storage, Folders
- In `config.toml`: `data_dir`, at the top
- Environment: `REEL_DATA_DIR`
- Accepts: a path
- Default: `/data`
- In the image: `/data`, from its environment
- Read when the process starts, from `config.toml` or the environment only

### Library folder

The clips themselves, every version with its posters, loops and filmstrips, and the trash: the bulk, which may live on a NAS or a big slow drive; a .reel-library file in it tells reel it is the right folder; set when the process starts.

- Under Settings: Library and storage, Folders
- In `config.toml`: `library_dir`, at the top
- Environment: `REEL_LIBRARY_DIR`
- Accepts: a path
- Default: `/data/library`
- Read when the process starts, from `config.toml` or the environment only

### Incoming folder

Raw clips dropped here are picked up, published, then removed; a failed subfolder takes what ffmpeg cannot read.

- Under Settings: Library and storage, Folders
- In `config.toml`: `incoming_dir` under `[ingest]`
- Environment: `REEL_INCOMING_DIR`
- Accepts: a path
- Default: `/incoming`
- In the image: `/incoming`, from its environment

### Originals folder

Where the raw file of a re-encoded clip is kept.

- Under Settings: Library and storage, Folders
- In `config.toml`: `originals_dir` under `[ingest]`
- Environment: `REEL_ORIGINALS_DIR`
- Accepts: a path
- Default: `/originals`
- In the image: `/originals`, from its environment

### Keep originals

The raw file of re-encoded clips only, of every clip, or of none.

- Under Settings: Library and storage, Folders
- In `config.toml`: `keep_originals` under `[ingest]`
- Environment: `REEL_KEEP_ORIGINALS`
- Accepts: one of `reencoded` (re-encoded clips only), `all` (every clip), `none`
- Default: `reencoded` (re-encoded clips only)

### Trash

A deleted clip stays in the trash this long, with Undo, before it is purged; the trash is .trash in the library folder, so a delete is a rename on any disk.

- Under Settings: Library and storage, Trash and caches
- In `config.toml`: `trash_days` under `[share]`
- Environment: `REEL_TRASH_DAYS`
- Accepts: a whole number, 1 to 365 days
- Default: `7`

### Copies under a size

The copies made for downloads under a size, and the animated pictures, are kept up to this much; the oldest go first.

- Under Settings: Library and storage, Trash and caches
- In `config.toml`: `discord_cache_gb` under `[share]`
- Environment: `REEL_DISCORD_CACHE_GB`
- Accepts: a whole number, 1 to 500 GB
- Default: `5`

### Disk warning

Each disk that holds reel's data (the data folder's, the library's, the kept originals') turns amber under this much free space, and Notifications may send; 0 turns it off.

- Under Settings: Library and storage, Trash and caches
- In `config.toml`: `disk_warn_gb` under `[storage]`
- Environment: `REEL_DISK_WARN_GB`
- Accepts: a whole number, 0 to 10000 GB free
- Default: `20`

### Library cap

Housekeeping keeps the library (each clip's versions and its preview video) under this size: the oldest clips go to the trash first, the ones without a live link before the others; a starred clip or one in a collection never goes; 0 is no cap.

- Under Settings: Library and storage, Trash and caches
- In `config.toml`: `cap_gb` under `[storage]`
- Environment: `REEL_LIBRARY_CAP_GB`
- Accepts: a whole number, 0 to 100000 GB
- Default: `0`

### The cap spares unsent clips

The app's standalone library: a clip that is not on a server too is never trimmed by the cap; a server leaves this out, every clip being on it.

- Under Settings: Library and storage, Trash and caches
- In `config.toml`: `cap_keeps_unsent` under `[storage]`
- Environment: `REEL_LIBRARY_CAP_KEEPS_UNSENT`
- Accepts: `true` or `false`
- Default: `true`

## Ingest

How the incoming folder is read, what is copied and what is re-encoded, and the workers that do it.

### Poll every

How often the folder is read; a file is taken once its size holds still for the stable polls.

- Under Settings: Ingest, Incoming folder
- In `config.toml`: `poll_seconds` under `[ingest]`
- Environment: `REEL_POLL_SECONDS`
- Accepts: a whole number, 1 to 3600 s
- Default: `5`

### Stable polls

Polls a file must keep the same size before it is read, for slow copies over the network.

- Under Settings: Ingest, Incoming folder
- In `config.toml`: `stable_polls` under `[ingest]`
- Environment: `REEL_STABLE_POLLS`
- Accepts: a whole number, 1 to 20 polls
- Default: `2`

### Extensions

File endings the watcher and the upload take; anything else is ignored.

- Under Settings: Ingest, Incoming folder
- In `config.toml`: `extensions` under `[ingest]`
- Environment: `REEL_EXTENSIONS`
- Accepts: a list: comma separated in the environment, an array in `config.toml`
- Default: `mkv, mp4, mov, webm, ts, avi`

### Copy video up to

H.264 at most this tall is copied untouched, never re-encoded.

- Under Settings: Ingest, Copy or re-encode
- In `config.toml`: `copy_max_height` under `[media]`
- Environment: `REEL_COPY_MAX_HEIGHT`
- Accepts: a whole number, 480 to 4320 p
- Default: `1440`

### Copy video under

And at most this fast; anything above is re-encoded.

- Under Settings: Ingest, Copy or re-encode
- In `config.toml`: `copy_max_mbit` under `[media]`
- Environment: `REEL_COPY_MAX_MBIT`
- Accepts: a whole number, 1 to 200 Mbit/s
- Default: `50`

### Re-encode at

The video bitrate for everything that is not copied.

- Under Settings: Ingest, Copy or re-encode
- In `config.toml`: `encode_mbit` under `[media]`
- Environment: `REEL_ENCODE_MBIT`
- Accepts: a whole number, 1 to 100 Mbit/s
- Default: `10`

### Audio

AAC at this rate when the audio is re-encoded, or the source rate when lower; AAC is copied as it is.

- Under Settings: Ingest, Copy or re-encode
- In `config.toml`: `audio_kbit` under `[media]`
- Environment: `REEL_AUDIO_KBIT`
- Accepts: a whole number, 64 to 512 kbit/s
- Default: `320`

### Normalise loudness

Forces an audio re-encode for every clip.

- Under Settings: Ingest, Copy or re-encode
- In `config.toml`: `normalize_audio` under `[media]`
- Environment: `REEL_NORMALIZE_AUDIO`
- Accepts: `true` or `false`
- Default: `false`

### Normalise new clips

Every new clip, from the app, the incoming folder or an upload, gets a version levelled to the loudness target right after the ingest, made current, the video copied and only the audio encoded again; the original stays as it is.

- Under Settings: Ingest, Copy or re-encode
- In `config.toml`: `normalize_new` under `[media]`
- Environment: `REEL_NORMALIZE_NEW`
- Accepts: `true` or `false`
- Default: `false`

### Loudness target

The integrated loudness Normalise loudness brings a clip to, here at ingest, in the clip editor and for each clip of a montage; -16 is the usual level for phones and streaming, -23 for broadcast.

- Under Settings: Ingest, Copy or re-encode
- In `config.toml`: `loudness_target` under `[media]`
- Environment: `REEL_LOUDNESS_TARGET`
- Accepts: a whole number, -30 to -5 LUFS
- Default: `-16`

### Copies on the GPU

Copies under a size on the hardware encoder in one pass instead of two-pass x264: faster, a little larger.

- Under Settings: Ingest, Hardware and workers
- In `config.toml`: `copies_on_hardware` under `[media]`
- Environment: `REEL_COPIES_ON_HARDWARE`
- Accepts: `true` or `false`
- Default: `false`

### Audio for copies

The copies under a size and the preview videos keep the source audio untouched when it is AAC (copy), else AAC at the source rate up to 320; a number forces that AAC rate; only when the video would fall below a watchable floor is the audio stepped down, and the copy says so.

- Under Settings: Ingest, Copy or re-encode
- In `config.toml`: `copy_audio` under `[media]`
- Environment: `REEL_COPY_AUDIO`
- Accepts: one of `copy` (as it is), `320` (AAC at 320 kbit/s), `256` (AAC at 256 kbit/s), `192` (AAC at 192 kbit/s), `160` (AAC at 160 kbit/s)
- Default: `copy` (as it is)

### x264 preset for copies

The two-pass libx264 of the copies under a size; slow is about half the speed of medium on an N100 and worth it at these sizes; faster presets trade quality for time.

- Under Settings: Ingest, Copy or re-encode
- In `config.toml`: `x264_preset` under `[media]`
- Environment: `REEL_X264_PRESET`
- Accepts: one of `faster`, `fast`, `medium`, `slow`, `slower`
- Default: `slow`

### Hardware encoding

The GPU route for re-encodes, probed at startup: QSV and VAAPI on Linux, NVENC, AMF and QSV on Windows; the copies under a size use the CPU unless Copies on the GPU is on.

- Under Settings: Ingest, Hardware and workers
- In `config.toml`: `hwaccel` under `[media]`
- Environment: `REEL_HWACCEL`
- Accepts: one of `auto` (automatic), `qsv` (Intel Quick Sync), `vaapi` (VAAPI), `nvenc` (NVIDIA NVENC), `amf` (AMD AMF), `none` (none, the CPU)
- Default: `auto` (automatic)
- Takes effect at the next start

### Render device

The DRI device the container was given.

- Under Settings: Ingest, Hardware and workers
- In `config.toml`: `render_device` under `[media]`
- Environment: `REEL_RENDER_DEVICE`
- Accepts: a path
- Default: `/dev/dri/renderD128`
- Takes effect at the next start

### ffmpeg

The binary reel runs; the image sets it.

- Under Settings: Ingest, Hardware and workers
- In `config.toml`: `ffmpeg` under `[media]`
- Environment: `REEL_FFMPEG`
- Accepts: text
- Default: `ffmpeg`
- In the image: `/usr/lib/jellyfin-ffmpeg/ffmpeg`, from its environment
- Read when the process starts, from `config.toml` or the environment only

### ffprobe

The probe reel runs; the image sets it.

- Under Settings: Ingest, Hardware and workers
- In `config.toml`: `ffprobe` under `[media]`
- Environment: `REEL_FFPROBE`
- Accepts: text
- Default: `ffprobe`
- In the image: `/usr/lib/jellyfin-ffmpeg/ffprobe`, from its environment
- Read when the process starts, from `config.toml` or the environment only

### Workers

Ffmpeg runs at a time; more than one competes with a media server for the GPU. A job someone waits for (a link's preview, a cut, a copy under a size, an upload's ingest) does not wait for them: it runs at once and pauses the others meanwhile.

- Under Settings: Ingest, Hardware and workers
- In `config.toml`: `workers` under `[pipeline]`
- Environment: `REEL_WORKERS`
- Accepts: a whole number, 1 to 8
- Default: `1`
- Takes effect at the next start

### Niceness

CPU priority of every ffmpeg, 0 to 19; higher gives way sooner.

- Under Settings: Ingest, Hardware and workers
- In `config.toml`: `nice` under `[pipeline]`
- Environment: `REEL_NICE`
- Accepts: a whole number, 0 to 19
- Default: `10`
- Takes effect at the next start

### Job timeout

A job running longer is killed and marked failed.

- Under Settings: Ingest, Hardware and workers
- In `config.toml`: `job_timeout_minutes` under `[pipeline]`
- Environment: `REEL_JOB_TIMEOUT_MINUTES`
- Accepts: a whole number, 5 to 1440 min
- Default: `60`
- Takes effect at the next start

## Sharing

The defaults a new link gets, the copies under a size, and where link previews load from.

### Preview video size from the server

The video file link previews (og:video) play inline, in Discord and other apps, when this server serves it; a version over this size, or one that is not H.264 with AAC, gets a smaller copy for it. Previewers give up fetching after a few seconds, so the file has to arrive within about five seconds of the server's upload: 20 MB suits 30 to 35 Mbit/s.

- Under Settings: Sharing, Link preview
- In `config.toml`: `embed_mb` under `[share]`
- Environment: `REEL_EMBED_MB`
- Accepts: a whole number, 5 to 95 MB
- Default: `20`

### Link previews from external storage

Discord and other apps give up on a link's preview video after about nine seconds, and from a home upload a large one often comes too late. With this on, reel puts each link's preview video in an S3-compatible storage (Cloudflare R2 is recommended; Backblaze B2, Wasabi, MinIO and AWS work) and the previews load from there in a moment, whatever the upload is doing. The preview videos are then stored with that service; the pages, the clips and the web copies stay here. On R2's free tier (10 GB, no fee for downloads) the default limit costs nothing. A new link waits for its preview's upload, about as long as the file takes at the upload's speed, unless it went up ahead. Discord keeps a link's preview for a while, so a message already posted keeps the embed it had.

- Under Settings: Sharing, External storage
- In `config.toml`: `on` under `[external]`
- Environment: `REEL_EXTERNAL`
- Accepts: `true` or `false`
- Default: `false`

### S3 endpoint

The service's S3 address; for R2 https://ACCOUNT_ID.r2.cloudflarestorage.com, on the bucket's settings page.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `endpoint` under `[external]`
- Environment: `REEL_EXTERNAL_ENDPOINT`
- Accepts: a URL starting with http:// or https://
- Default: empty

### Region

The bucket's region: auto for R2 and MinIO, the bucket's own on the others.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `region` under `[external]`
- Environment: `REEL_EXTERNAL_REGION`
- Accepts: text
- Default: `auto`

### Bucket

A bucket of reel's own is best; reel writes under reel/ in it and never touches what it did not write.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `bucket` under `[external]`
- Environment: `REEL_EXTERNAL_BUCKET`
- Accepts: text
- Default: empty

### Access key

The id of a token that may read and write this bucket only.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `access_key` under `[external]`
- Environment: `REEL_EXTERNAL_ACCESS_KEY`
- Accepts: text
- Default: empty

### Secret key

The token's secret; written, never shown again.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `secret_key` under `[external]`
- Environment: `REEL_EXTERNAL_SECRET_KEY`
- Accepts: a secret, never shown again once saved

### Public address

Where the bucket's files are served from, such as https://media.example.com on a custom domain; R2's r2.dev address works for a test, but Cloudflare limits its rate.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `public_url` under `[external]`
- Environment: `REEL_EXTERNAL_PUBLIC_URL`
- Accepts: a URL starting with http:// or https://
- Default: empty

### Preview video size from storage

The preview video of a link served from the storage; the storage has no upload limit, so it can be sharper than the server's, and chat apps still fetch it in time.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `embed_mb` under `[external]`
- Environment: `REEL_EXTERNAL_EMBED_MB`
- Accepts: a whole number, 5 to 200 MB
- Default: `40`

### Limit

The most reel keeps there, counted from what it wrote. R2's free 10 GB are shared by every bucket of the account, so with others there keep this under what they leave. At the limit the previews made ahead go first, then the links' previews, the least used first; a link inside the protected window keeps its own.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `limit_gb` under `[external]`
- Environment: `REEL_EXTERNAL_LIMIT_GB`
- Accepts: a whole number, 1 to 10000 GB
- Default: `5`

### Made ahead

Previews that go up before a link exists, so a later link is instant; they make room first.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `ahead` under `[external]`
- Environment: `REEL_EXTERNAL_AHEAD`
- Accepts: one of `off` (none), `edited` (for edited clips), `edited_new` (for edited and new clips)
- Default: `off` (none)

### New for

How long a clip counts as new after it came in, for Made ahead.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `new_days` under `[external]`
- Environment: `REEL_EXTERNAL_NEW_DAYS`
- Accepts: a whole number, 1 to 365 days
- Default: `7`

### Protected window

A link's preview stays in the storage this long after the link is made, whatever the order; when these alone fill the limit, a new link's preview comes from this server and Status says so; 0 protects none.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `protect_days` under `[external]`
- Environment: `REEL_EXTERNAL_PROTECT_DAYS`
- Accepts: a whole number, 0 to 365 days
- Default: `14`

### Revoke links never shared

Revokes a link no chat app ever fetched and no one ever opened, once it is as old as the days below, and its preview leaves the storage. A link posted or opened even once is never revoked, however quiet it goes, so an old chat never shows a broken link; nor is one with an expiry of its own. A link pasted where no preview is built and never opened looks never shared.

- Under Settings: Sharing, External storage
- Shows while [Link previews from external storage](#link-previews-from-external-storage) is on; its value stays when that is off
- In `config.toml`: `cleanup` under `[share]`
- Environment: `REEL_LINK_CLEANUP`
- Accepts: `true` or `false`
- Default: `false`

### Revoke after

How many days after it was made a link never shared is revoked.

- Under Settings: Sharing, External storage
- Shows while [Revoke links never shared](#revoke-links-never-shared) is on; its value stays when that is off
- In `config.toml`: `cleanup_days` under `[share]`
- Environment: `REEL_LINK_CLEANUP_DAYS`
- Accepts: a whole number, 1 to 365 days
- Default: `30`

### Web copy bitrate

A link's page plays a copy of its version at this rate, video and audio together, once the copy is made, and the preview video until then. Everyone watching shares the home upload: at about 30 Mbit/s up, 10 Mbit/s carries two or three viewers at once. A version already under it, H.264 with AAC and no taller than the height below, plays as it is.

- Under Settings: Sharing, Link page
- In `config.toml`: `web_mbit` under `[share]`
- Environment: `REEL_WEB_MBIT`
- Accepts: a whole number, 2 to 50 Mbit/s
- Default: `10`

### Web copy height

The tallest a web copy is; a shorter version keeps its own height.

- Under Settings: Sharing, Link page
- In `config.toml`: `web_max_height` under `[share]`
- Environment: `REEL_WEB_MAX_HEIGHT`
- Accepts: one of `720` (720p), `1080` (1080p), `1440` (1440p), `2160` (2160p)
- Default: `1440` (1440p)

### Default limit

The limit of a copy under a size asked for without naming one, such as a bare discord.mp4 address; one of the list. Unless set, it is 20, Discord's without Nitro, or the largest limit under 20 when the list has no 20.

- Under Settings: Sharing, Copies under a size
- In `config.toml`: `discord_limit_mb` under `[share]`
- Environment: `REEL_DISCORD_LIMIT_MB`
- Accepts: a whole number, 1 to 500 MB
- Default: `20`

### Limit list

The limits the picker offers, in MB; chat apps' upload limits are the point of them: Discord takes 20 MB without Nitro, 50 with Nitro Basic and 500 with Nitro, and some apps still stop at 10.

- Under Settings: Sharing, Copies under a size
- In `config.toml`: `discord_limits_mb` under `[share]`
- Environment: `REEL_DISCORD_LIMITS_MB`
- Accepts: a list: comma separated in the environment, an array in `config.toml`
- Default: `10, 20, 50, 100, 500`

### Allow the download

Whether a new link's page offers the full file, until changed in the link dialog.

- Under Settings: Sharing, New links
- In `config.toml`: `download_default` under `[share]`
- Environment: `REEL_SHARE_DOWNLOAD_DEFAULT`
- Accepts: `true` or `false`
- Default: `true`

### Expire

What a new link's expiry starts on in the dialog.

- Under Settings: Sharing, New links
- In `config.toml`: `expiry_default` under `[share]`
- Environment: `REEL_SHARE_EXPIRY_DEFAULT`
- Accepts: one of `never`, `1d` (after 1 day), `7d` (after 7 days), `30d` (after 30 days)
- Default: `never`

## Imports

A folder to scan and how big an upload may be.

### Upload size limit

An upload bigger than this is refused; the proxy in front has a limit of its own.

- Under Settings: Imports, Upload
- In `config.toml`: `upload_max_gb` under `[import]`
- Environment: `REEL_UPLOAD_MAX_GB`
- Accepts: a whole number, 1 to 64 GB
- Default: `8`

## Notifications

Where reel tells you about failures, a full disk and new versions: ntfy or a webhook.

### Send to

Off, an ntfy topic, or a webhook that takes JSON.

- Under Settings: Notifications, Where
- In `config.toml`: `kind` under `[notify]`
- Environment: `REEL_NOTIFY_KIND`
- Accepts: one of `off`, `ntfy` (an ntfy topic), `webhook` (a webhook)
- Default: `off`

### ntfy server

The server's address, such as https://ntfy.example.org; a new address clears the token below, which is then entered again for that server.

- Under Settings: Notifications, Where
- In `config.toml`: `ntfy_url` under `[notify]`
- Environment: `REEL_NTFY_URL`
- Accepts: a URL, https (plain http only for a host on the private network)
- Default: empty

### ntfy topic

The topic reel posts to.

- Under Settings: Notifications, Where
- In `config.toml`: `ntfy_topic` under `[notify]`
- Environment: `REEL_NTFY_TOPIC`
- Accepts: text
- Default: empty

### ntfy token

An access token when the topic is protected; written, never shown again.

- Under Settings: Notifications, Where
- In `config.toml`: `ntfy_token` under `[notify]`
- Environment: `REEL_NTFY_TOKEN`
- Accepts: a secret, never shown again once saved

### Webhook URL

Receives a JSON body with the event, the clip, the reason and a link.

- Under Settings: Notifications, Where
- In `config.toml`: `webhook_url` under `[notify]`
- Environment: `REEL_WEBHOOK_URL`
- Accepts: a URL, https (plain http only for a host on the private network)
- Default: empty

### Events

What reel tells about; the first five are on for a new install.

- Under Settings: Notifications, Events
- In `config.toml`: `events` under `[notify]`
- Environment: `REEL_NOTIFY_EVENTS`
- Accepts: any of `job_failed` (a job failed), `file_unreadable` (a file could not be read), `disk_low` (the disk is low), `storage` (the external storage needs a look), `library_cap` (the library cap moved clips), `update_available` (an update is available), `link_made` (a link was made), comma separated
- Default: `job_failed, file_unreadable, disk_low, storage, library_cap`

## Access

Who gets in and how reel knows who they are. Share links stay public either way.

### Mode

Local: reel's own accounts; proxy: a proxy in front names the user; oidc: an identity provider signs in; single: no sign-in, only on a loopback address; set when the process starts.

- Under Settings: Access, Identity
- In `config.toml`: `mode` under `[auth]`
- Environment: `REEL_AUTH_MODE`
- Accepts: one of `local` (local accounts), `proxy` (a proxy in front), `oidc` (an identity provider), `single` (single user)
- Default: `local` (local accounts)
- Read when the process starts, from `config.toml` or the environment only
- Changing it on a server with users: [Switching the sign-in method](switching-sign-in.md)

### Trusted proxies

Only these addresses may name the user; anything else answers 401.

- Under Settings: Access, Identity
- In `config.toml`: `trusted_proxies` under `[auth]`
- Environment: `REEL_TRUSTED_PROXIES`
- Accepts: a list: comma separated in the environment, an array in `config.toml`
- Default: empty
- Read when the process starts, from `config.toml` or the environment only

### Single user

The name everyone is in single mode.

- Under Settings: Access, Identity
- In `config.toml`: `dev_user` under `[auth]`
- Environment: `REEL_DEV_USER`
- Accepts: text
- Default: `you`
- Read when the process starts, from `config.toml` or the environment only

### Admins

Users with these names are admins, each exactly as reel gets it, case included; the first user ever seen is one too, except behind an identity provider, where the first admin is listed here, carries the admin value or types the setup code; with an identity provider, the subject the provider sends instead of a name (Authentik's is the username unless its client says otherwise), counted at the first sign-in, and at every sign-in with a role claim.

- Under Settings: Access, Identity
- In `config.toml`: `admins` under `[auth]`
- Environment: `REEL_ADMINS`
- Accepts: a list: comma separated in the environment, an array in `config.toml`
- Default: empty

### Idle timeout

A session ends this long after its last use; behind an identity provider the default is 7 days, elsewhere 30.

- Under Settings: Access, Sessions
- In `config.toml`: `session_days` under `[auth]`
- Environment: `REEL_SESSION_DAYS`
- Accepts: a whole number, 1 to 365 days
- Default: `30`
- Takes effect at the next start

### Session lifetime

A session ends this long after the sign-in, however active; 0 is no limit; behind an identity provider the default is 30 days, elsewhere 90.

- Under Settings: Access, Sessions
- In `config.toml`: `session_max_days` under `[auth]`
- Environment: `REEL_SESSION_MAX_DAYS`
- Accepts: a whole number, 0 to 3650 days
- Default: `90`

### Issuer

The OpenID Connect issuer URL exactly as the provider gives it, its last slash included, such as https://auth.example.org/application/o/reel/; reel asks it for its metadata at start, again by itself until it answers, and every hour.

- Under Settings: Access, Identity provider
- In `config.toml`: `oidc_issuer` under `[auth]`
- Environment: `REEL_OIDC_ISSUER`
- Accepts: a URL starting with http:// or https://
- Default: empty
- Takes effect at the next start

### Provider name

What the sign-in page calls the provider, as in Sign in with Authentik; empty shows the issuer's host; shown as typed, at most 40 characters.

- Under Settings: Access, Identity provider
- In `config.toml`: `oidc_name` under `[auth]`
- Environment: `REEL_OIDC_NAME`
- Accepts: text
- Default: empty

### Client id

The client registered at the issuer; its secret comes from REEL_OIDC_CLIENT_SECRET or the file REEL_OIDC_CLIENT_SECRET_FILE names, never from here.

- Under Settings: Access, Identity provider
- In `config.toml`: `oidc_client_id` under `[auth]`
- Environment: `REEL_OIDC_CLIENT_ID`
- Accepts: text
- Default: empty
- Takes effect at the next start

### Scopes

What is asked of the issuer, space separated.

- Under Settings: Access, Identity provider
- In `config.toml`: `oidc_scopes` under `[auth]`
- Environment: `REEL_OIDC_SCOPES`
- Accepts: text
- Default: `openid profile email`
- Takes effect at the next start

### Name claim

The claim a user is named by when first seen (email only when the provider marks it verified); the subject when it is missing; a known user keeps their name: who the user is comes from the issuer and subject, never from the name.

- Under Settings: Access, Identity provider
- In `config.toml`: `oidc_username_claim` under `[auth]`
- Environment: `REEL_OIDC_USERNAME_CLAIM`
- Accepts: text
- Default: `preferred_username`

### Role claim

A claim (a string or a list) that makes an admin when it holds the admin value, followed at every sign-in both ways; the Admins list and the last admin stay admins; empty means the Admins list alone, at first sight.

- Under Settings: Access, Identity provider
- In `config.toml`: `oidc_role_claim` under `[auth]`
- Environment: `REEL_OIDC_ROLE_CLAIM`
- Accepts: text
- Default: empty

### Admin value

The value in the role claim that means admin; it cannot be empty while a role claim is set.

- Under Settings: Access, Identity provider
- In `config.toml`: `oidc_admin_value` under `[auth]`
- Environment: `REEL_OIDC_ADMIN_VALUE`
- Accepts: text
- Default: `admin`

## Devices

The paired apps: a code to pair one, their tokens, what they upload, and the chunks they send.

### Chunk size

A paired app sends a clip in chunks of this size and resumes at the last one after a drop; 90 is the most, so a chunk passes Cloudflare's 100 MB request limit; on the LAN larger is faster.

- Under Settings: Devices, Uploads from devices
- In `config.toml`: `chunk_mb` under `[device]`
- Environment: `REEL_DEVICE_CHUNK_MB`
- Accepts: a whole number, 1 to 90 MB
- Default: `32`

### Unfinished uploads expire

An upload in pieces, the app's or the browser's, that nothing has touched for this long is removed with its file; it starts again from the beginning.

- Under Settings: Devices, Uploads from devices
- In `config.toml`: `upload_expiry_hours` under `[device]`
- Environment: `REEL_DEVICE_UPLOAD_EXPIRY_HOURS`
- Accepts: a whole number, 1 to 720 h
- Default: `24`

### Offer app updates

A paired app older than this server, on Windows or the Linux AppImage, is offered the app of the server's version, signed, from the app-updates folder in the data folder; the .deb does not update itself.

- Under Settings: Devices, App updates
- In `config.toml`: `app_updates` under `[device]`
- Environment: `REEL_APP_UPDATES`
- Accepts: `true` or `false`
- Default: `true`

### Release token

For a private fork only: a GitHub token that reads its releases; the server fetches its own version's installers and their signatures from the release at start and once a day while they are missing, and a public repository's releases need none; written, never shown again.

- Under Settings: Devices, App updates
- In `config.toml`: `updates_token` under `[device]`
- Environment: `REEL_UPDATES_TOKEN`
- Accepts: a secret, never shown again once saved

## Appearance

The look of the pages, and what this browser remembers.

### Theme

Dark is the only theme so far; the light one is on the roadmap.

- Under Settings: Appearance, Theme
- In `config.toml`: `theme` under `[appearance]`
- Environment: `REEL_THEME`
- Accepts: one of `dark`
- Default: `dark`

## Advanced

Logs, the update check, the history of every change, exports and the restart.

### Log level

How much the log says; RUST_LOG wins when it is set.

- Under Settings: Advanced, Logs
- In `config.toml`: `log_level` under `[advanced]`
- Environment: `REEL_LOG_LEVEL`
- Accepts: one of `error` (errors only), `warn` (warnings), `info`, `debug` (debug, everything)
- Default: `info`

### Check for updates

Once a day, against the public releases of the repository, nothing else.

- Under Settings: Advanced, Updates
- In `config.toml`: `update_check` under `[advanced]`
- Environment: `REEL_UPDATE_CHECK`
- Accepts: `true` or `false`
- Default: `true`

### Repository

The GitHub repository whose releases are checked, as owner/name.

- Under Settings: Advanced, Updates
- In `config.toml`: `update_repo` under `[advanced]`
- Environment: `REEL_UPDATE_REPO`
- Accepts: text
- Default: `Locko2901/reel`

## Other variables

| Variable | For | What it does |
|---|---|---|
| `REEL_CONFIG` | The server | The path of `config.toml`, the file layer under the environment. The image sets `/data/config.toml`; without it reel reads no file. |
| `REEL_OIDC_CLIENT_SECRET` | The server | The identity provider's client secret, with `auth.mode` set to `oidc`. Never stored in the database or shown under Settings. |
| `REEL_OIDC_CLIENT_SECRET_FILE` | The server | A file holding that secret instead, such as a Docker secret under `/run/secrets/`. |
| `REEL_PASSWORD` | The server | Read by `reel passwd <name>` only: the new password of that user, twelve characters at least. `docker exec -e REEL_PASSWORD='...' reel reel passwd --make-admin admin` is the way back in when the admin is locked out. |
| `REEL_DESKTOP_DIR` | The desktop app | Puts the app's settings (under `config`) and its data, the logs and a standalone library included (under `data`), in this one folder instead of the user's own folders. |

`RUST_LOG`, when set, wins over [Log level](#log-level). `TZ` sets the container's clock; [Time zone](#time-zone) is what the pages follow.
