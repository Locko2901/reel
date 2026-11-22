# Capture

What OBS captures and how it encodes it, what a clip hears, and the OBS behind it all. These are the
desktop app's own settings, kept on this PC: each PC with the app has its own. The clip length, the
quality and the microphone are on [Essentials](essentials.md).

The page follows the OBS the app runs with:

- **reel's own OBS**, the OBS that came with reel on Windows, or on Linux the installed OBS that
  reel runs a second time: every group below. Its first line says to change anything else in OBS
  itself; reel keeps working as long as OBS's WebSocket and its replay buffer stay on. The setup
  walks through the same choices ([All in one](../setup/all-in-one.md#3-the-setup), [Set it up for
  me on Linux](../setup/linux.md#set-it-up-for-me)), and its values are OBS's own: a change made in
  OBS's window shows here too.
- **An OBS of your own**: the format and the bitrate under **Picture**, and the **obs-websocket**
  connection. Its first line says that OBS tells the app the moment a replay is saved, with the
  folder watch as a safety net ([OBS's settings, later](../setup/own-obs.md#3-obss-settings-later)).

When Discord can see the OBS that runs, and would turn on its Streamer Mode by itself, a notice at
the top says how to keep that off ([Discord turns on its Streamer
Mode](../troubleshooting.md#discord-turns-on-its-streamer-mode)).

## Picture {#picture}

The line beside the group's name says how a change takes. With reel's own OBS, each change is
written into OBS's settings and OBS starts again with it, which takes a few seconds; with [Capture
games that run as administrator](#obs-setup-elevated) on, it is written into the copy of OBS in
Program Files, and Windows asks once for each change. With an OBS of your own, a change is written
into your OBS's profile, refused while OBS records, and a running replay buffer starts again so the
change takes.

### Encoder {#obs-setup-encoder}

With reel's own OBS: the encoder OBS records with. NVIDIA NVENC, AMD AMF and Intel Quick Sync encode
on the graphics card and leave the game more room than **x264 on the CPU**; on Linux the choices are
NVIDIA NVENC, VAAPI (AMD and Intel) and x264.

The setup tests each encoder this PC has with a short encode, then in OBS itself, and picks the
first that records. Only an encoder whose last test here worked can be picked; x264 always can, and
until the setup has tested, it is the one in use. The row lists the others with why they are not
offered ("not tested on this PC yet", or what the test found), and [Run the setup
again](#the-obs-that-came-with-reel) tests them again.

If the replay buffer does not run after a change, or a save fails before any replay has been saved
with the new encoder, reel goes back to the encoder of the last replay and says so
([Afterwards](../setup/all-in-one.md#afterwards)).

### Format {#obs-setup-format}

With reel's own OBS: the file OBS records into. **hybrid MP4**, the default, plays everywhere and
survives a crash of OBS; **MKV** survives one too. Both are among the [Extensions](#extensions) the
folder watch takes. A change starts OBS again.

### Format {#format}

With an OBS of your own: its recording format, as its profile has it: MKV, hybrid MP4, MP4,
fragmented MP4, MOV, fragmented MOV, MPEG-TS or FLV. **MKV** survives a crash of OBS, and **hybrid
MP4**, from OBS 30 on, plays everywhere and survives one too: pick one of those two ([OBS's
settings, later](../setup/own-obs.md#3-obss-settings-later)). The row can be changed once OBS is
connected; a change is written into OBS's profile.

### Video bitrate {#video-bitrate}

With an OBS of your own: the video bitrate OBS records at, in kbit/s, from 500 to 300000. It can be
set here only in OBS's simple output mode with the recording quality "same as stream", where the
recording takes OBS's stream bitrate. In advanced output mode the bitrate sits in the encoder's own
settings in OBS (Settings, Output, Recording), and with another recording quality a bitrate does not
apply: the row then says which, and cannot be changed.

A higher bitrate makes larger clips and a replay buffer that takes more memory. With the library's
defaults, a clip in H.264 up to 1440p and 50 Mbit/s, with AAC audio, goes into the library as it is,
never encoded again; one above that is encoded again ([Copy video
under](ingest.md#media-copy-max-mbit)).

## What it captures {#what-it-captures}

With reel's own OBS only. The screen is the one the setup picked.

### The game in front {#obs-setup-game}

On by default. On Windows: OBS's game capture of any fullscreen application, over the screen, so a
clip shows the game while one runs and the screen otherwise. Off, OBS captures the screen alone.

On Linux it is obs-vkcapture's capture of a Vulkan or OpenGL game started with `obs-gamecapture` (on
Steam, the launch option `obs-gamecapture %command%`), over the screen. Without obs-vkcapture in
that OBS the row says so, and the screen holds the game. obs-vkcapture serves one OBS at a time:
when your own OBS captures games through it too, turn this off ([Set it up for
me](../setup/linux.md#set-it-up-for-me)).

A change starts OBS again.

### Capture games that run as administrator {#obs-setup-elevated}

Windows only, off by default. A game that runs with administrator rights can only be captured by an
OBS with those rights too. On, a scheduled task starts OBS, never reel, with the highest rights,
from a copy of OBS in Program Files that only an administrator can change.

- **Windows asks once** when you turn it on or off, and once for each change of the setup while it
  is on, such as the clip length, the quality or the encoder. The levels and the microphone's
  filters change without asking.
- **OBS takes the Save a clip key itself**, as Windows does not pass keys pressed in such a game to
  reel. Save and share and Share the last clip work again once that game is not in front.
- **Off** removes the task and the copy; Windows asks once for that.
- **While a change of the copy runs**, the switch waits, and the line under it says what runs or
  what went wrong with the task.

Leave it off unless a game of yours needs it. [Games that run as
administrator](../setup/all-in-one.md#games-that-run-as-administrator) tells the whole story, with
updates of the copy.

### Its copy in Program Files {#its-copy-in-program-files}

Shows while the copy of OBS for games that run as administrator is there, or should be. All well, it
names the copy's OBS version and folder. Otherwise it says what is wrong, with one button, and
Windows asks once:

- **Put it in place**: the copy is not there, so OBS runs without the highest rights and games that
  run as administrator are not captured.
- **Update the copy**: the copy holds another OBS than this version of reel brought, still runs OBS
  under OBS's own name, misses its task, or still shows OBS's own tray icon.
- **Remove it**: a copy or its task is left in Program Files with the switch off, and would start
  OBS with the rights at sign-in.

## Sound {#sound}

With reel's own OBS only: what a clip hears. A change of the sources or the tracks starts OBS again;
the noise suppression, the noise gate and the levels change in the running OBS, without a restart.
The [Microphone](essentials.md#microphone) itself is on Essentials.

### Desktop audio {#obs-setup-desktop}

On by default: everything the PC plays on the output the setup picked, the game and Discord
included. It is in the full mix, track 1, which links play. Off, a clip hears only the sources you
choose: the microphone, the game alone, Discord alone.

### The game alone {#the-game-alone}

Off unless you turn it on: only the game's sound. On Windows that is the game in front, with [The
game in front](#obs-setup-game) on, or a program you pick from the ones with a window open; to turn
it on, open the game first, or have The game in front on. On Linux it needs the PipeWire audio
capture plugin for OBS (obs-pipewire-audio-capture), and the row shows only when that OBS has it;
the game is picked by name from the programs playing sound, so start it first.

With **Desktop audio** on, the game is in the full mix through it already, so it goes on a track of
its own and never into the full mix twice. With Desktop audio off, it makes the full mix with the
microphone.

### Discord alone {#obs-setup-discord}

Off unless you turn it on: only Discord's voices, on the same rule for the tracks as **The game
alone**. On Linux it needs the same plugin.

### Noise suppression {#obs-setup-noise}

Shows while the microphone is on: OBS's filter on the microphone.

| Choice | What it does |
|---|---|
| **Off** | the microphone as it comes, at no cost |
| **Light (Speex)** | takes steady hiss and hum, with very little CPU |
| **Strong (RNNoise)**, the default | a small neural network that also takes keyboard clicks and the noise between words; more CPU than Light, still a small share of one core |
| **NVIDIA noise removal** | on the graphics card, an RTX card with NVIDIA's audio effects installed: the strongest, at some of the card's time; offered only when OBS offers it |
| **My own OBS's filters** | the filters copied from your own OBS's microphone, as they are; offered once **Use these filters** below copied them |

When the NVIDIA choice is set and OBS here does not offer it, Strong is used, and the row says so.

### Your own OBS's microphone {#your-own-obss-microphone}

Shows while the microphone is on, when your own OBS, beside reel's, has filters on its microphone
and they are not in use here. The row names the source, its scene collection and its filters. **Use
these filters** copies them as they are, without changing your OBS, and sets **Noise suppression**
to **My own OBS's filters**.

### Noise gate {#obs-setup-gate}

Off by default; shows while the microphone is on. After the noise suppression, OBS's noise gate
silences the microphone while its sound stays below a level. With OBS's own values, the gate opens
at -26 dB and closes at -32 dB. It suits a microphone that picks up the room between your words.
With **My own OBS's filters** it is not added: those filters are used as they are.

### Each source on a track of its own {#obs-setup-own-tracks}

Off unless turned on; the setup turns it on by itself with more than one source. Beside the full mix
on track 1, which links play, each source chosen above goes on a track of its own, so the editor can
turn each one up or down later ([Mix the audio tracks](../editor.md#audio-tracks)). Each track of
its own adds 160 kbit/s of audio to a clip.

### Where each goes {#where-each-goes}

A line for each source chosen: what makes the full mix, which links play, and which source also has
a track of its own. With nothing chosen, it says a clip has no sound.

### Each source's level {#obs-setup-volumes}

One row for each source chosen above, named after it: **Desktop audio level**, **Microphone level**,
**Game level** (**Game audio level** for a program you picked) and **Discord level**. Each is the
source's volume in OBS, from -30 to +10 dB, 0 unless you change it, with a meter that shows its
sound after the volume. The meters move while this page is open, a few seconds after OBS starts. A
level changes in the running OBS as you move it, without a restart, and is kept when you let go.

## The OBS behind it {#the-obs-behind-it}

Which OBS the app runs with, and how it reaches it: **The OBS that came with reel** while reel's own
OBS is in use, **obs-websocket** for an OBS of your own. Folded under them, **What reel reads from
OBS, and how it finds the clips** holds the three groups after them.

## The OBS that came with reel {#the-obs-that-came-with-reel}

**reel's own OBS** on Linux. It shows when the app has an OBS to run itself: reel, the Windows
installer that brings OBS, or on Linux an OBS installed on the PC. Its line says how that OBS runs,
or that it is not in use. Its buttons:

- **Run the setup again** opens the setup, with its tests of the encoders.
- **Set it up** shows until that setup is done.
- **Start OBS again** shows once reel stopped starting an OBS that kept stopping, and tries once
  more.
- **Open OBS** and **Hide OBS**, on Windows, show and hide OBS's own window, for anything reel's
  choices do not name. While it is up, a bar at the top of the app's pages says where reel's
  settings are in OBS. On Linux OBS's window opens from its own tray icon.

### Use the OBS that came with reel {#obs-bundled}

**Run a second OBS of reel's** on Linux. On, reel starts that OBS hidden with the app, sets it up,
starts it again after a crash, and stops it when you quit. Off, the app connects to an OBS of your
own, as under **obs-websocket** below. The setup's **Set it up for me** turns it on, and **Use my
own OBS** off.

On Linux it is the OBS you installed, run a second time with settings of its own (its profile,
scenes, WebSocket and replay buffer) and its own tray icon; your own OBS and its settings stay as
they are, and both can run at once. On Windows, while your own OBS is open, reel's waits until it
closes, so two captures never run at once ([Afterwards](../setup/all-in-one.md#afterwards)).

## obs-websocket {#obs-websocket}

With an OBS of your own: how the app reaches OBS over OBS's WebSocket server, built into OBS 28 and
later. In OBS, open Tools, **WebSocket Server Settings** and tick **Enable WebSocket server**. The
line beside the group's name says whether the app is connected, or what it waits for. A change of
any row connects again at once. reel never starts or ends your OBS; once OBS is back after it closed
or crashed, the app connects again within a few seconds. **Run the setup** opens the setup for your
own OBS ([Your own OBS](../setup/own-obs.md#2-the-setup)).

### Connect to OBS {#obs-enabled}

On by default. Off, the app does not connect to OBS at all: OBS does not tell it when a replay is
saved, the app's own Save keys save nothing and say that OBS is not connected, and Settings, Status
says OBS is off. A clip saved with OBS's own Save Replay key still comes in through the folder
watch, from the [Clips folder](clips.md#clips-dir).

### Host {#obs-host}

Where OBS runs: `127.0.0.1`, the default, when OBS runs on this PC.

### Port {#obs-port}

OBS's **Server Port**, in its WebSocket Server Settings: 4455 by default, in OBS and here.

### Password {#obs-password}

OBS's **Server Password**, when **Enable Authentication** is on in its WebSocket Server Settings:
**Show Connect Info** there shows it. Leave it empty when authentication is off. A wrong one says
"OBS refused the password".

### Test {#test}

Connects once with the values in the rows above, saved or not, and asks OBS its version. When it
works, it says OBS's and obs-websocket's versions and what it found; when not, why in plain words,
with the system's own line under them. [OBS](../troubleshooting.md#obs) says what each answer means.

## What reel needs from OBS {#what-reel-needs-from-obs}

With an OBS of your own and **Connect to OBS** on. Two lines, **OBS's WebSocket** and **The replay
buffer**, checked as long as the app runs, each saying what to do when something is missing. The
fold opens by itself while one of them is not right. [OBS](../troubleshooting.md#obs) has every
line.

## What OBS is set to {#what-obs-is-set-to}

What OBS says over its WebSocket once it is connected, with either OBS. The line beside the group's
name has OBS's and obs-websocket's versions; then the list:

- the **Profile** in use, whose settings these are, and the **Output mode**, simple or advanced;
- the **Recording folder**, the **Format** and the **Encoder**;
- **Video**: the bitrate, or the recording quality, or in advanced mode that it sits in the
  encoder's settings;
- **Audio**: its bitrate where OBS says it, and the tracks a recording takes;
- the **Replay buffer**: enabled or not, how many seconds it keeps, running or stopped;
- **Recording**: whether OBS records;
- the **Video size**: the canvas, the output and the frames a second.

It is read when the app connects and after each change the app makes in OBS. **Refresh** reads it
again, after a change made in OBS itself. Without OBS connected, it says so.

## The clips folder {#the-clips-folder}

How reel finds a new clip. With OBS connected, OBS tells the app the moment it saved a replay, and
the app takes that file at once. Beside it, the app watches the clips folder as a safety net: a file
that lands there counts once its size has held still and it opens. These rows set that watch.

### Follow OBS {#clips-follow-obs}

On by default: once OBS is connected and says its recording folder, the app watches that folder, and
follows it when it changes in OBS. Until then, and with it off, the [Clips
folder](clips.md#clips-dir) is watched, whatever OBS says. With reel's own OBS both are the same
folder.

### Extensions {#extensions}

The file endings the folder watch takes, comma separated: `mkv, mp4, mov` unless you change them.
Case does not matter, a dot in front is dropped, and at least one is needed. Files whose names end
in `.part`, `.tmp` or `.crdownload`, still being written, and names that start with a dot are always
skipped. A replay OBS says it saved is taken whatever its ending: the list counts for the files the
folder watch finds.

### Still for {#stable-seconds}

The seconds a file's size must hold still before the folder watch takes it: 5 unless you change it,
from 1 to 120. The file must also open: on Windows the app opens it for itself alone, which fails
while OBS still writes it. The folder is looked at every 2 seconds, and at once when the system says
something in it changed. A longer wait suits a folder that fills slowly, such as one on a network
drive.
