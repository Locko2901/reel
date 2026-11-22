# Essentials

Settings opens on **Essentials**. At its top, one sentence says what is set: the key that saves a
clip, how much a clip keeps, when the replay buffer runs, the quality, and where a clip goes. Below
it are the few settings most people change, each with what it costs. These are the desktop app's own
settings, kept on this PC (Settings, App, **About** names the folder): each PC with the app has its
own, and a change saves as you make it. The setup walks through the same choices ([All in
one](../setup/all-in-one.md#3-the-setup), [Your own OBS](../setup/own-obs.md#2-the-setup)), and [The
app's pages](../desktop-app.md#the-apps-pages) shows where everything else is.

Some rows depend on which OBS the app runs with. **reel's own OBS** is the OBS that came with reel
on Windows, or on Linux the installed OBS that reel runs a second time with settings of its own; an
**OBS of your own** is one you set up yourself, which the app connects to. Each entry says when that
matters. In standalone, where the library is on this PC, the **Uploads** group is not there.

## Clips {#clips}

### The replay buffer {#obs-replay-buffer}

OBS keeps the last seconds in memory, and a save makes them a clip. The two cards choose when that
buffer runs:

- **Always**, the default: a clip can be saved at any time, also on the desktop.
- **While a game runs**: OBS's replay buffer starts with a game and stops once the last game closed
  and the [grace period](games.md#session-grace-seconds) passed, 60 seconds unless you change it.
  While no game runs, the PC keeps the memory and the encoder time the buffer would take, and the
  Save key saves nothing: it plays the [No clip](clips.md#sounds-no-clip-enabled) sound and says
  why. **Record a session** works all along.

Each card's line says what a save keeps, with your key and your clip length. Which programs count as
games is the list under [Games and apps](games.md), and a game's own [Replay
buffer](games.md#replay-buffer) set to **Off** stops the buffer while that game runs, whatever this
says. [The replay buffer](../games-and-apps.md#the-replay-buffer) tells the whole story.

With an OBS of your own, reel starts and stops OBS's replay buffer only while [Start it in
OBS](clips.md#obs-start-replay) is on. Choosing **While a game runs** turns it on and says so; while
it is off, a line under the cards says reel cannot start the buffer with a game.

### Clip length {#obs-setup-clip-seconds}

How much of the past a clip keeps when you save it.

With reel's own OBS it is 1 minute unless you change it, from 5 seconds to 30 minutes. The slider
stops at 15 s, 30 s, 1, 2, 5 and 10 min, and the box beside it takes any number of seconds. The line
under it says what the length costs at the **Quality** below: the memory the replay buffer takes,
and about how large a clip of that length is. It warns when the buffer would take more than a
quarter of this PC's memory, and, connected, when a clip would be larger than your server's [Upload
size limit](../configuration.md#upload-size-limit).

A change is written into OBS's settings and OBS starts again with it, which takes a few seconds; the
replay buffer starts over then. With [Capture games that run as
administrator](capture.md#obs-setup-elevated) on, Windows asks once for the change.

With an OBS of your own, **Clip length** is OBS's own replay length, written into its profile, from
1 second to 60 minutes. It can be changed once OBS is connected. A running replay buffer starts
again so the change takes, and a change is refused while OBS records.

A [session recording](session.md) has no length: it records until you stop it.

### Quality {#obs-setup-quality}

With reel's own OBS: the picture every clip and session recording gets. Each of the four cards shows
its picture and the size of a minute of clip.

| Card | Picture | For |
|---|---|---|
| **Light** | 1080p at 30 frames a second | half the load, for a PC with little to spare; fast motion looks less fluid |
| **Smooth**, the default | 1080p at 60 | most games |
| **Sharp** | 1440p at 60 | small text that has to stay readable |
| **Native** | the screen's own size and refresh rate, up to 240 frames a second | the screen as it is |

A picture is never made larger than the screen: on a 1080p screen, Sharp records 1080p. The bitrate
follows the picture (about 25 Mbit/s at Smooth, 12.5 Mbit/s at Light), so a higher quality costs
more memory in the replay buffer, more room on the disk and longer uploads. A change starts OBS
again, as a change of **Clip length** does. [All in one](../setup/all-in-one.md#3-the-setup) says
when to pick which.

With OBS's own Video settings changed in OBS's window, the row says **as set in OBS**, with that
picture; a card sets one of reel's again.

### Quality {#quality}

With an OBS of your own, this row takes the place of the four cards: your OBS sets the picture, and
the row says what it is, its size, its frames a second and, where OBS says it, its bitrate. Its
button opens [Capture](capture.md#picture), which has your OBS's recording format and bitrate.

### Save a clip {#hotkeys-save}

The key that saves a clip, anywhere, also in a game. There is none until the setup sets Alt+F10. OBS
saves the replay, and the clip goes up as any clip does, or in standalone into the library on this
PC. When the key could not be taken, the line under it says why; on Linux a second line says how a
hotkey reaches the app there. Setting a key, and why one can be refused:
[Hotkeys](hotkeys.md#hotkeys-save).

## Sound {#sound}

### Microphone {#microphone}

With reel's own OBS: your voice, in every clip and session recording. The switch turns it on or off,
and the list beside it picks the device: **Default, as Windows sets it** follows Windows
(**Default** on Linux), or pick a microphone by its name. The setup turns it on with the default
device. A change starts OBS again, as a change of **Clip length** does.

The microphone is always in the full mix that links play. Its noise suppression, its noise gate and
its level are under [Capture](capture.md#sound), and show there while it is on.

With an OBS of your own this group is not there: the microphone is your OBS's own.

## Uploads {#uploads}

### Upload {#upload-mode}

Connected only: when a clip goes up to your server.

- **every clip**, the default: each clip goes up as soon as the app finds it.
- **the last, on its key**: clips wait on this PC, and [Upload the last
  clip](hotkeys.md#hotkeys-upload-last), Ctrl+Alt+U unless you change it, sends the newest one that
  waits.
- **by hand**: clips wait in the queue on Settings, [Status](status.md#queue), until you press
  **Upload now** on one.

Session recordings follow it too. **Save and share** sends its clip at once, whatever this says.
Clips that wait when you switch to **every clip** go up at the app's next start, or one by one with
**Upload now**. How a clip goes up, and the waits while you play, are under [Uploads](uploads.md).

## This PC {#this-pc}

### Start with Windows {#startup-autostart}

**Start at logon** on Linux. On by default: the app starts with your Windows or desktop session,
hidden in the tray and without a window, so no clip is missed. On Windows it is a Run entry for your
Windows user; on Linux an entry in your desktop's autostart. For the AppImage that entry points at
the AppImage's own path, so after moving the file, turn it off and on again. Off, the app runs only
once you start it. [What runs, and when](../desktop-app.md#what-runs-and-when).
