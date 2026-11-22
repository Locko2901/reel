# Encoding

Standalone only: the encoder reel on this PC makes new versions and copies under a size with.
What is encoded again at all is under [Ingest](ingest.md). This page is one of the app's own,
kept on this PC with its other settings, and it sets the library on this PC. Every ffmpeg reel
on this PC runs at a low priority: Windows' idle class, `nice 10` on Linux
([How it treats the PC while you play](../setup/standalone.md#how-it-treats-the-pc-while-you-play)).

## Encoder {#encoder}

The group's line names the encoder in use, as the test at reel's start found it, such as "in use:
NVIDIA NVENC" or "in use: x264 on the CPU".

On Windows the group also holds the waits. They are explained with the uploads, where they show
while the app is connected:
[Encoding waits while a game has the screen](uploads.md#upload-wait-for-games), and with a server
paired [Only on the internet, not on the LAN](uploads.md#upload-pause-only-on-wan).

### GPU encoder {#standalone-hwaccel}

Which encoder reel on this PC uses for its heavy work: the re-encode of a new clip that cannot go
into the library as it is, an edit or a montage that is encoded again, and with
[Copies on the GPU](#standalone-copies-on-hardware) on, the copies under a size. Each time reel on
this PC starts, it tests the encoder with a short encode:

- **automatic**, the default: on Windows NVIDIA NVENC, then AMD AMF, then Intel Quick Sync; on
  Linux Intel Quick Sync, then VAAPI. The first that passes is used. On Linux, NVENC and AMF are
  tried only when you pick them.
- **NVIDIA NVENC**, **AMD AMF**, **Intel Quick Sync**, and on Linux **VAAPI**: that one alone.
- **none, the CPU**: x264 on the CPU, with no test.

An encoder that fails its test leaves the work to x264 on the CPU, and **Doctor** under
[Status, Standalone](status.md#standalone) says why. A job the GPU fails on is done again on the
CPU. A change starts reel on this PC again, and its test with it.

### Copies on the GPU {#standalone-copies-on-hardware}

On by default: the copies under a size, the files made to fit a chat's upload limit, are encoded
on the GPU encoder in use, in one pass, instead of two-pass x264 on the CPU: faster, and a little
larger. Off, or while no GPU encoder is in use (**none, the CPU**, or none passed its test), they
are two-pass x264, with the preset under [Ingest](ingest.md#media-x264-preset). A change starts
reel on this PC again. [Send the file itself](../sharing.md#send-the-file-itself) has the copies.
