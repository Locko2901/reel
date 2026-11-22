# Status

What the app is doing: how it is connected, the clips on their way, the last clips, and what it
did. In standalone: the library on this PC, the clips going into it, and reel on this PC. Most of
the page is there to look at, and the first place to look when something goes wrong
([Where to look first](../troubleshooting.md#where-to-look-first)). Its one setting, **Log
level**, is kept on this PC, like every page under **This PC**.

## Connections {#connections}

One line for each part the app depends on, green while it is well, with what is wrong when it is
not:

- **Server**, connected: the server's name, your user and this PC's name on it, or "not paired
  yet", or "no server set" ([Connection](connection.md)).
- **Library**, in standalone: whether reel on this PC runs, and its folder. With a server paired,
  a **Server** line follows, for **Upload to the server**.
- **OBS**: whether the app is connected to OBS. With your own OBS, a line follows for each thing
  reel needs from it, as Capture lists them ([OBS](../troubleshooting.md#obs)).
- **Folder**: the clips folder the app watches.
- **Hotkey**: the main keys and what each does (connected, the ones that share; in standalone,
  the one that saves a clip), with the key of a session recording; or what went wrong with a key
  ([A hotkey is refused](../troubleshooting.md#a-hotkey-is-refused)).
- **Tray**, on Linux, only when the desktop shows no tray icons: reel runs all the same, and a
  second start opens its window.

## Standalone {#standalone}

Shows in standalone: reel on this PC, the library the app runs inside itself. The group's line
says how it runs: on its address, with the encoder it uses; or "starting reel on this PC", "the
library is moving; reel on this PC waits", or "reel did not start" with the reason.

- **Library**: the [library folder](library-folder.md#library-folder).
- **reel**: its address on this PC's loopback, on a port it picks at each start. Only the app
  opens it ([The library on this PC](../privacy.md#the-library-on-this-pc)).
- **Encoder**: the one in use, as the start's test found it
  ([GPU encoder](encoding.md#standalone-hwaccel)).
- **Doctor**: what that start noted, such as each encoder's test encode, or "nothing to note".
- **ffmpeg**: the version of the ffmpeg that came with the app. The line under the list names its
  build, its licence and where its source is, and the licence's file next to the app.

## Queue {#queue}

The clips on their way, one row each with its file and its state. The group's line says how many
wait; with none, the group says "Nothing waiting."

- **found**: on this PC, waiting for you. That is the case with **Upload** set to **the last, on
  its key** or **by hand** ([Upload](essentials.md#upload-mode)), and in standalone while reel on
  this PC cannot take a clip in, as while the library moves or its drive is away.
- **queued**: waiting its turn.
- **uploading**, with how far it is.
- **paused**: held by **Pause uploads** (in the tray and the menu of the window's bar), or by
  [the waits](uploads.md#upload-wait-for-games) while a game has the screen.
- **failed**, with the reason, such as a file over the server's
  [Upload size limit](imports.md#import-upload-max-gb).

**Upload now** sends a found clip, and **Retry** tries a failed one again. In standalone, **Add
now** takes a found clip into the library on this PC. **Open folder** shows the file in its
folder. A clip sent with **Upload to the server** comes through this queue too, with the bandwidth
limit and the chunk size of [Sending to it](connection.md#sending-to-it).

## Last clips {#last-clips}

Of the last eight clips this PC saved, the ones that are through: connected, uploaded to the
server; in standalone, in the library here. Each shows its title, when, and where it stands: "on the
server", "on the server, local copy removed" (after [Keep up to](clips.md#cache-gb) or
[Keep for](clips.md#cache-days)), "in the library", or the step the server is at with it.

- **Copy link**, connected: asks the server for the clip's link, waits until its preview is
  ready, and copies it.
- **Open**: the clip's page in the window.
- **Open folder**: the file in its folder, while it is still on this PC.
- **Upload to the server**: on a clip in the library on this PC that is not on the server yet,
  while a server is paired. It sends the clip's file
  ([Add a server later](../setup/standalone.md#add-a-server-later)).

With no clip yet, the group says "No clip yet. Save a replay in OBS."

## Log {#log}

What the app did, newest at the bottom: the last 300 lines. The box follows new lines every five
seconds while the page is open, and **Refresh** reads it at once; scrolled up to read, it stays
where you are. Tokens never appear in it. **Open the log folder** opens the folder with the files,
one a day (`reel-desktop.log.` and the date).
[The app's logs](../troubleshooting.md#the-app) says where the folder is on Windows and on Linux,
and where OBS keeps its own logs.

### Log level {#log-level}

How much the log says: **errors only**, **warnings**, **info**, the default, or **debug,
everything**, which writes every chunk of an upload too. A change takes effect at once. For a
problem that is hard to catch, set **debug, everything**, make it happen again, then set it back.
