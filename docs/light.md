# How light it is

What reel costs a machine, measured, never estimated. Closed to the
tray, the app on Windows is one process of 13 MB, with no browser engine
running. Below: what costs what, why the app stays that small, the
downloads, the app on Windows and on Linux, and the server.

## What costs what

- **The app** is a Rust program that draws its window with the WebView
  Windows already has (WebView2, the engine of Edge). It carries no
  browser of its own. Its own work is a folder watch, an upload, a
  connection to OBS and a few hotkeys.
- **Recording** is OBS's: the replay buffer, the capture and the encoder
  are OBS's, on the graphics card when it has an encoder. reel adds
  nothing to a frame. With your own OBS, reel's cost is the app's alone.
- **The server** is one Rust program with SQLite. ffmpeg runs only while
  there is work (a new clip, a cut, a copy under a size), as a separate
  process at low priority, one at a time by default.
- **Not there at all**: no feed, no social layer, no Windows service (the
  app runs while you are logged in and stops when you quit it), no
  telemetry. The app talks to your server and to OBS on the same PC,
  nothing else; standalone, it asks GitHub for a newer release only when
  you press **Check now** or turn on the daily check. The server asks
  GitHub once a day whether a newer release is out (Settings, Advanced,
  **Check for updates** turns that off), and fetches the app's installers
  of its own version from the release while they are missing (Settings,
  Devices, **Offer app updates**).

## Why the app stays light

The window is the only heavy part of the app, and it exists only while
it is open. Close the window and its WebView ends with it: on Windows no
`msedgewebview2.exe` of reel's is left, and the app is one process of
13 MB that uses no measurable CPU. That small Rust program is what keeps
working while you play: it watches the clips folder, talks to OBS,
listens for the hotkeys and uploads. Open the window again and the
WebView starts again for it.

Even with the window open on the library, the app takes 262 MB on
Windows, most of it the WebView's.

## The downloads

| Download | Size |
|---|---|
| reel, for Windows | 204.1 MB, OBS included |
| reel light, for Windows | 69.8 MB, for the OBS you have |
| reel for Linux, the AppImage | 284.7 MB, everything inside: ffmpeg, WebKitGTK and GStreamer for the window's video and sound |
| reel for Debian and Ubuntu, the .deb | 153.6 MB, ffmpeg included; WebKitGTK and GStreamer come from the distribution |

## The app on Windows

Measured on a gaming PC (Ryzen 9 9950X3D, 62 GB, RTX 4090, Windows 11
23H2) on an idle desktop: 60 samples a second apart, memory as the
private working set, CPU as a share of the whole machine.

| | Processes | Memory | CPU | GPU memory |
|---|---|---|---|---|
| reel, closed to the tray | 1 (`reel-desktop.exe`) | 13 MB | 0.00 % | none |
| reel, its window open on the library | 7: `reel-desktop.exe` 13 MB and six WebView2 processes of 171, 34, 30, 9, 3 and 2 MB | 262 MB | about 0.01 % | 96 MB dedicated, 104 MB shared |

Closed to the tray, no `msedgewebview2.exe` of reel's is left: the
window's WebView ends with the window.

### The recorder

reel's OBS holds the replay buffer in memory and writes nothing until you
save a clip. On the same PC, on an idle desktop, with a replay buffer of
180 s of 1440p at 60 frames, recorded at a constant quality (CQP 20) on
the graphics card's encoder, it held 440 MB of memory and kept the video
encoder 30 % busy. Those numbers follow from OBS's settings rather than
from the program: a lower quality, a lower frame rate or a shorter
buffer costs less, and you can tune every one of them in OBS.

## The app on Linux

The app is the same code on Windows and Linux apart from the WebView:
WebView2 (Edge's engine) on Windows, WebKitGTK on Linux. Measured in a
virtual machine under a virtual display, memory as PSS, CPU as a share of
one core, so these numbers do not compare with the Windows table.

| | Measured |
|---|---|
| The program itself | 31.5 MB |
| Started hidden, in the tray, window never opened | 101 MiB in 1 process |
| Window open on the library | 237 MiB in 3 processes |
| Window closed to the tray | 153 MiB in 2 processes: WebKitGTK keeps its network process for the program's lifetime |
| CPU, idle in the tray | 0.23 % of one core |
| CPU, idle with the window open | 0.67 % of one core |
| Uploading a 190 MB clip | 4.2 s over the loopback, 0.86 CPU seconds (20 % of one core on average), at most 95 MiB |
| reel's own OBS, replay buffer running (60 s) | 671 MiB, 286 % of one core: software rendering and x264 at 1280x800, 60 frames, no GPU in the VM |
| The app beside that OBS | 84 MiB, 0.13 % of one core |

The OBS line is not what a gaming PC sees: in the virtual machine OBS
draws and encodes on the CPU. On a PC with a graphics card, OBS composes
on the GPU and encodes with NVENC, AMF, Quick Sync or VAAPI.

## The server

Measured on a demo server: a virtual machine with 12 cores of a QEMU
CPU, no GPU, so every encode on the CPU; reel 1.1.0 with about 160 clips
in its library. Where a home server running the release image under
Docker was measured too, its number comes first.

| | Measured |
|---|---|
| Image to download | 153 MB (the release image) |
| Image unpacked | 451 MB: ffmpeg (jellyfin-ffmpeg with the Intel media driver) 356 MB, Debian 85 MB, reel 10 MB (layers of a build of 2026-10-07; the reel binary is 17.2 MB today) |
| The reel binary | 17.2 MB |
| Idle memory | 14 MiB on the home server (`docker stats`). On the demo, two minutes after a start: 22 MiB resident, of which 8 MiB its own data and 14 MiB the program's mapped code; after hours of ingests and cuts, 90 MiB (PSS), since the memory allocator keeps what the busy moments took for the next one |
| Idle CPU | 0.06 % of one core on the home server, 0.075 % on the demo (two minutes, from `/proc`) |
| Processes and threads | 9 on the home server (`docker stats` PIDs); one process with 20 threads on the demo, and no ffmpeg while idle |
| A new clip: a minute of 1080p at 60 frames, 25 Mbit/s, 190.5 MB | 23 s from the file landing in the incoming folder to the clip ready with its preview video, poster, hover loop and filmstrip; 94.5 CPU seconds in all; at most 380 MiB for reel and its ffmpeg together. The clip itself was only repackaged (1.6 s): H.264 under the copy rules is never encoded again. The rest is the preview video that link previews play (9.7 s) and the poster, loop and filmstrip (2 s). |
| Disk for that clip | 209.7 MB: the clip 190.5 MB, the preview video 18.9 MB, poster, loop and filmstrip 0.3 MB |
| A cut, the last 30 s, starting between keyframes | 28 s, 207.7 CPU seconds, at most 1004 MiB: the cut is encoded again (38.4 MB) and gets its own preview video |
| A cut starting on a keyframe | 17 s, 113.4 CPU seconds, at most 621 MiB: the cut is copied (96.6 MB), most of the time is its preview video |

With a GPU (Intel Quick Sync or VAAPI) the encodes move to the GPU and
the CPU seconds of a cut fall accordingly; the Jobs page shows how long
each job took on yours.

How it was measured: memory as the sum of PSS over reel and its ffmpeg
children from `/proc/<pid>/smaps_rollup`, polled every second; CPU from
`/proc/<pid>/stat` (reel's own time plus that of its finished children);
the clip made with ffmpeg's test pattern (`testsrc2`) at 25 Mbit/s, so
it compresses no better than a game would.
