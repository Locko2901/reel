# Ingest

**Ingest** holds how reel reads the incoming folder, what it keeps as it is and what it encodes
again when a clip comes in, the loudness of new clips, how it makes the copies it sends, and the
GPU and the workers that do the work. These settings are kept on the server, the same for every
PC and browser, and only an admin sees them. A change to how clips come in applies to the clips
that come in afterwards: a clip already in the library keeps its files.

**ffmpeg** and **ffprobe** are read when the process starts, so they are always read only here;
a row marked **after a restart** takes effect at reel's next start
([Restart reel](advanced.md#restart)). Any other setting given in `config.toml` or the
environment shows read only too, with where it came from ([Configuration](../configuration.md)).
[No app](../setup/no-app.md#what-reel-takes-and-when) tells how the incoming folder is read,
and [The editor](../editor.md) what the cuts do with these settings.

In the desktop app's standalone library this page holds the settings of the library on this PC:
the group **Copy or re-encode**, and **Job timeout** alone under **Hardware and workers**. Its
line points to the app's **Encoder** page, which has the GPU encoder and **Copies on the GPU**.
The library runs one worker, at a low priority, and has no incoming folder.

## Incoming folder {#incoming-folder}

### Poll every {#ingest-poll-seconds}

How often reel reads the [incoming folder](storage.md#ingest-incoming-dir), in seconds: every 5
by default, 1 to 3600. Each read notes the size of every file there. A file is taken once its
size held still for [Stable polls](#ingest-stable-polls) reads in a row and it can be opened. A
shorter time takes a clip in sooner and reads the folder more often.

reel reads this value when it starts watching the folder, so a change takes effect at its next
start.

In `config.toml`: `poll_seconds` under `[ingest]`; environment: `REEL_POLL_SECONDS`; every value
it takes: [Configuration](../configuration.md#poll-every).

### Stable polls {#ingest-stable-polls}

How many reads in a row a file's size has to stay the same before reel takes it in: 2 by
default, 1 to 20. With the defaults, a file is taken about ten seconds after it stops growing.
Raise it when files arrive over a slow network, or from a sync tool that pauses in the middle of
a file, so reel never takes half a file. A file that drops out of sight before it is taken is
counted again from its return.

In `config.toml`: `stable_polls` under `[ingest]`; environment: `REEL_STABLE_POLLS`; every value
it takes: [Configuration](../configuration.md#stable-polls).

### Extensions {#ingest-extensions}

The file endings reel takes, without the dot: `mkv, mp4, mov, webm, ts, avi` by default. They
count for the incoming folder, for every upload (the browser's, ShareX's and the paired apps')
and for [Scan a folder](imports.md#scan-folder). A file with another ending is left alone in a
folder, and refused as an upload. Each ending is 1 to 8 letters or digits, the list needs at
least one, and the case does not matter.

Names that start with a dot or end in `.part`, `.tmp`, `.crdownload` or `.txt` are never taken,
whatever the list: copies and sync tools use such names while they write. In the standalone
library, the app's own **Extensions** decide which files of its clips folder it takes.

In `config.toml`: `extensions` under `[ingest]`; environment: `REEL_EXTENSIONS`; every value it
takes: [Configuration](../configuration.md#extensions).

## Copy or re-encode {#copy-or-re-encode}

### Copy video up to {#media-copy-max-height}

The tallest video reel keeps as it is: 1440p by default, 480 to 4320. When a clip comes in, its
video is copied untouched, which loses nothing and takes about as long as copying the file, when
it is H.264, at most this tall, at most [Copy video under](#media-copy-max-mbit) fast, and 8-bit
4:2:0. Any other video is encoded again at [Re-encode at](#media-encode-mbit), and scaled down
to this height when it is taller: with the default, a 2160p clip comes out at 1440p. The clip
page's line says which, and why ("video re-encoded: 2160p is over 1440p").

Raise it to keep taller recordings as they are; lower it to bring them down to a smaller size,
at the cost of an encode for every taller clip. With **Keep originals** on its default, the raw
file of a clip encoded again is kept ([Keep originals](storage.md#ingest-keep-originals)).

In `config.toml`: `copy_max_height` under `[media]`; environment: `REEL_COPY_MAX_HEIGHT`; every
value it takes: [Configuration](../configuration.md#copy-video-up-to).

### Copy video under {#media-copy-max-mbit}

The fastest video reel keeps as it is, as the video's own bitrate: 50 Mbit/s by default, 1 to
200. H.264 within [Copy video up to](#media-copy-max-height) but at a higher bitrate is encoded
again at [Re-encode at](#media-encode-mbit). Lower it to bring heavy recordings down to a size
that is lighter to store and to play; raise it to keep them as they are.

In `config.toml`: `copy_max_mbit` under `[media]`; environment: `REEL_COPY_MAX_MBIT`; every
value it takes: [Configuration](../configuration.md#copy-video-under).

### Re-encode at {#media-encode-mbit}

The video bitrate of everything reel encodes again: 10 Mbit/s by default, 1 to 100. It counts
for a clip whose video is not copied when it comes in, for a cut of the editor that is not a
stream copy ([Fast cuts on keyframes](../editor.md#stream-copy-or-re-encode)), and for a
montage. The encode runs on the GPU when reel has one ([Hardware encoding](#media-hwaccel)),
else on the CPU. A higher rate keeps more detail in a busy game and makes larger files. The
copies under a size, the preview videos and the web copies have sizes and rates of their own.

In `config.toml`: `encode_mbit` under `[media]`; environment: `REEL_ENCODE_MBIT`; every value it
takes: [Configuration](../configuration.md#re-encode-at).

### Audio {#media-audio-kbit}

The AAC bitrate reel encodes sound at: 320 kbit/s by default, 64 to 512. AAC sound is copied as
it is when a clip comes in, so this counts for other sound (Opus, MP3, PCM and the like), and
for every clip while [Normalise loudness](#media-normalize-audio) is on. A source at a lower
rate keeps its own. The editor's cuts and montages encode their sound at this rate too.

In `config.toml`: `audio_kbit` under `[media]`; environment: `REEL_AUDIO_KBIT`; every value it
takes: [Configuration](../configuration.md#audio).

### Normalise loudness {#media-normalize-audio}

On, every new clip's sound is levelled to the [Loudness target](#media-loudness-target) as it
comes in: its audio is encoded again as AAC at the [Audio](#media-audio-kbit) rate, even when it
is AAC already, while the video follows its own rule. The library's original then holds the
levelled sound, and the sound as recorded survives only in a raw file reel kept
([Keep originals](storage.md#ingest-keep-originals) set to **every clip**). Off, the default,
the sound keeps its level.

[Normalise new clips](#media-normalize-new) levels new clips in a version of their own and
leaves the original alone; while this is on, it does nothing more. The editor's own
**normalise loudness** switch levels one cut ([Fix the sound](../editor.md#audio)).

In `config.toml`: `normalize_audio` under `[media]`; environment: `REEL_NORMALIZE_AUDIO`; every
value it takes: [Configuration](../configuration.md#normalise-loudness).

### Normalise new clips {#media-normalize-new}

On, every new clip with sound, from the desktop app, the incoming folder, an upload or a scanned
folder, gets a version levelled to the [Loudness target](#media-loudness-target) right after it
comes in, before it shows in the library, and that version is made current. Only the sound is
encoded again; the video is copied, and the original stays as it came, under **Versions**. When
the desktop app's **Keep the last** trims a new clip, the trim and the levelling are one cut. A
sound too quiet to be measured keeps its level, and its version says **too quiet to normalise**
([Fix the sound](../editor.md#audio)). Off by default.

The desktop app's **Upload** page (**New clips** in standalone) shows whether it is on. With
[Normalise loudness](#media-normalize-audio) on, this does nothing: the original is levelled
already. In the standalone library it counts for the clips from OBS and the uploads.

In `config.toml`: `normalize_new` under `[media]`; environment: `REEL_NORMALIZE_NEW`; every value
it takes: [Configuration](../configuration.md#normalise-new-clips).

### Loudness target {#media-loudness-target}

The integrated loudness, in LUFS, that levelling brings a clip to: -16 by default, -30 to -5.
-16 is the usual level for phones and streaming, -23 the one for broadcast. It counts for
[Normalise loudness](#media-normalize-audio) and [Normalise new clips](#media-normalize-new) as
clips come in, for the editor's **normalise loudness** switch, and for each clip of a montage
that evens out the loudness
([Choose how it looks and sounds](../editor.md#choose-how-it-looks-and-sounds)). A version
already levelled keeps its level.

In `config.toml`: `loudness_target` under `[media]`; environment: `REEL_LOUDNESS_TARGET`; every
value it takes: [Configuration](../configuration.md#loudness-target).

### Audio for copies {#media-copy-audio}

The sound of the copies reel makes to send: the copies under a size, the preview videos chat
apps play, and the web copies a link's page plays.

- **As it is** (`copy`), the default: AAC is kept untouched; other sound becomes AAC at its own
  rate, up to the [Audio](#media-audio-kbit) rate.
- **AAC at 320**, **256**, **192** or **160 kbit/s**: that rate, always encoded again.

In a copy under a size the sound comes first and the picture gets the rest of the room, so a
lower rate leaves more room for the picture. Only when the picture would fall below a watchable
quality does reel step the sound down (256, 192, 160, then 128 kbit/s), and the copy's line
says so.

After a change, a copy under a size made under the old rule is made again when it is next asked
for; the web copies go at the next housekeeping, within 10 minutes, and are made again on their
next view; the preview videos wait for
[Remake the preview videos](storage.md#remake-embeds). In the standalone library it counts for
the copies under a size alone.

In `config.toml`: `copy_audio` under `[media]`; environment: `REEL_COPY_AUDIO`; every value it
takes: [Configuration](../configuration.md#audio-for-copies).

### x264 preset for copies {#media-x264-preset}

How hard x264 works on the copies under a size, which it makes in two passes on the CPU:
**slow** by default; **faster**, **fast**, **medium**, **slow** or **slower**. A slower preset
gives a sharper picture at the same size and takes longer: slow runs at about half the speed of
medium on an Intel N100, which is worth it at these small sizes. Choose a faster one when the
copies take too long on your server.

It does nothing while [Copies on the GPU](#media-copies-on-hardware) is on and reel has a GPU,
and the preview videos and the web copies do not use it. After a change, a copy made with
another preset is made again when it is next asked for
([Send the file itself](../sharing.md#send-the-file-itself)). In the standalone library it works
the same, with **Copies on the GPU** on the app's **Encoder** page.

In `config.toml`: `x264_preset` under `[media]`; environment: `REEL_X264_PRESET`; every value it
takes: [Configuration](../configuration.md#x264-preset-for-copies).

## Hardware and workers {#hardware-and-workers}

### Copies on the GPU {#media-copies-on-hardware}

On, the copies under a size are made on the GPU's encoder in one pass instead of two passes of
x264 on the CPU: faster, and a little larger. A copy that comes out over its limit is made again
at a lower rate, up to three tries. It counts only while reel encodes on a GPU
([Hardware encoding](#media-hwaccel)); without one the copies stay on x264. Off by default. In
the standalone library it is on the app's **Encoder** page.

In `config.toml`: `copies_on_hardware` under `[media]`; environment: `REEL_COPIES_ON_HARDWARE`;
every value it takes: [Configuration](../configuration.md#copies-on-the-gpu).

### Hardware encoding {#media-hwaccel}

Which GPU route reel encodes on: **automatic** by default, or **Intel Quick Sync**, **VAAPI**,
**NVIDIA NVENC**, **AMD AMF**, or **none, the CPU**. When reel starts, it runs a short test
encode on each route asked for and takes the first that works: automatic tries Quick Sync, then
VAAPI on Linux, and NVENC, AMF, then Quick Sync on Windows. Quick Sync and VAAPI need the
[Render device](#media-render-device). When nothing works, reel encodes on the CPU with x264,
which is not an error. The Status page's **hardware** line says which route it took and what the
test found, and so does the first run's **Hardware** step
([4. The first run](../setup/server.md#4-the-first-run)).

The GPU does the re-encodes of new clips, the cuts, the montages, the preview videos and the web
copies; the copies under a size stay on the CPU unless
[Copies on the GPU](#media-copies-on-hardware) is on. An encode the GPU cannot do moves to the
CPU by itself. Choose a route by hand when automatic takes the wrong one, or **none, the CPU**
to leave the GPU to something else. It takes effect after a restart.

The compose file on [The server](../setup/server.md#1-start-the-container) sets
`REEL_HWACCEL: auto`, which makes the row read only; delete that line to choose here. See also
[Which graphics cards does it use?](../faq.md#which-graphics-cards-does-it-use)

In `config.toml`: `hwaccel` under `[media]`; environment: `REEL_HWACCEL`; every value it takes:
[Configuration](../configuration.md#hardware-encoding).

### Render device {#media-render-device}

The GPU's render node that Quick Sync and VAAPI open: `/dev/dri/renderD128` by default. The
compose file hands the container the host's `/dev/dri`; on a machine with more than one GPU,
give the node of the one reel should use, such as `/dev/dri/renderD129`. When the node is not
there, reel leaves Quick Sync and VAAPI out, and the Status page's **hardware** line says it is
not present. NVENC and AMF do not use it. It takes effect after a restart.

In `config.toml`: `render_device` under `[media]`; environment: `REEL_RENDER_DEVICE`; every value
it takes: [Configuration](../configuration.md#render-device).

### ffmpeg {#media-ffmpeg}

The ffmpeg reel runs for every encode, cut and copy. The image sets it to its own
(`/usr/lib/jellyfin-ffmpeg/ffmpeg`); leave it so there. Run outside the image, give the path of
an ffmpeg of your own, or leave `ffmpeg` for the one on the `PATH`
([A demo server](../building.md#a-demo-server)). The Status page's **ffmpeg** line names its
version, or says it is not found. reel reads it when the process starts, so it is always read
only here.

In `config.toml`: `ffmpeg` under `[media]`; environment: `REEL_FFMPEG`; every value it takes:
[Configuration](../configuration.md#ffmpeg).

### ffprobe {#media-ffprobe}

The ffprobe reel reads a file with (its streams, its length, its tags) before it decides what to
do with it. The image sets it to its own (`/usr/lib/jellyfin-ffmpeg/ffprobe`); leave it so there.
Run outside the image, give its path, or leave `ffprobe` for the one on the `PATH`. reel reads it
when the process starts, so it is always read only here.

In `config.toml`: `ffprobe` under `[media]`; environment: `REEL_FFPROBE`; every value it takes:
[Configuration](../configuration.md#ffprobe).

### Workers {#pipeline-workers}

How many background jobs run at once, each one ffmpeg: 1 by default, up to 8. Background jobs
are the ingests, the web copies, the posters, loops and filmstrips, and the like. A job someone
waits for (a link's preview video, a cut, a copy under a size, an upload's ingest) does not wait
for them: it runs at once, and pauses the others meanwhile.

More than one worker gets through a backlog sooner, but the jobs then share the CPU and the GPU,
with each other and with a media server on the same machine
([How light it is](../light.md#what-costs-what)). It takes effect after a restart. The
standalone library always runs one.

In `config.toml`: `workers` under `[pipeline]`; environment: `REEL_WORKERS`; every value it takes:
[Configuration](../configuration.md#workers).

### Niceness {#pipeline-nice}

The CPU priority every ffmpeg runs at, 0 to 19: 10 by default. reel starts each ffmpeg under
`nice` at this value, so a higher one gives way sooner to everything else on the machine, such
as a media server; 0 runs it at the normal priority. It decides who goes first when the CPU is
busy, not how fast ffmpeg runs on an idle machine. It takes effect after a restart. The
standalone library always runs ffmpeg at a low priority: Windows' idle class, `nice 10` on Linux.

In `config.toml`: `nice` under `[pipeline]`; environment: `REEL_NICE`; every value it takes:
[Configuration](../configuration.md#niceness).

### Job timeout {#pipeline-job-timeout-minutes}

How long one ffmpeg run of a job may take before reel stops it and the job fails, saying it
"timed out": 60 minutes by default, 5 to 1440. A run on a long video gets longer, four times
the video's length, so a session recording of hours is not cut off. The time a background job
spends paused for a job someone waits for does not count. Raise it on a slow machine whose long
encodes fail this way. It takes effect after a restart; in the standalone library, at the next
start of the app.

In `config.toml`: `job_timeout_minutes` under `[pipeline]`; environment:
`REEL_JOB_TIMEOUT_MINUTES`; every value it takes:
[Configuration](../configuration.md#job-timeout).
