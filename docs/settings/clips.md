# Clips

Saving a clip, what each one keeps, its keys and its sounds, and how much stays on this PC after the
server has it. In standalone, where the library is on this PC, the page holds saving a clip, what
goes into the library of each, its sounds, and where OBS writes it; the keys and the rows about the
server are not there. These are the desktop app's own settings, kept on this PC: each PC with the
app has its own. [The desktop app](../desktop-app.md) tells the whole story.

**reel's own OBS** below is the OBS that came with reel on Windows, or on Linux the installed OBS
that reel runs a second time; an **OBS of your own** is one you set up, which the app connects to.

## Save a clip now {#save-a-clip-now}

What the hotkeys **Save a clip** and **Save and share** do, as two buttons, for when no key is at
hand. **Save clip** has OBS save the replay. **Save and share**, connected only, also uploads the
clip at once, waits until its link preview is ready and copies the link. A button that cannot run is
greyed, and the line under them says why ("Not now: ..."); otherwise it says how much OBS's replay
buffer keeps. The keys are on [Hotkeys](hotkeys.md).

## The replay buffer {#the-replay-buffer}

This group shows with an OBS of your own.

### Start it in OBS {#obs-start-replay}

On by default. When OBS is open and its replay buffer is off, the app turns the replay buffer on in
OBS's profile and starts it: as you turn this on, when the app connects to OBS, and when **Save and
share** finds the buffer off. It never does so while OBS records. Off, you start the replay buffer
in OBS yourself.

[The replay buffer](essentials.md#obs-replay-buffer) set to **While a game runs** needs it, as reel
then starts and stops OBS's replay buffer with your games: choosing that turns it on. With reel's
own OBS the app runs the buffer itself and this group is not there.

## What a clip keeps {#what-a-clip-keeps}

### Keep the last {#upload-keep-last}

Each clip goes into the library with only its last seconds as its current version, the one the
library and the links show, and the whole file is kept as its original. 0, the default, keeps the
whole clip. Otherwise 5 to 600 seconds.

- **Why**: a replay often holds more than the play. With a long **Clip length** for safety and
  **Keep the last** at 20 s, say, every clip shows the last 20 seconds, and the rest is still there.
- **The cut** starts on a keyframe up to 2.5 s earlier when there is one, so it may be a little
  longer than asked. A clip not longer than the seconds asked for is kept whole.
- **Going back**: the clip's **Versions**, in the editor, has the original; **Make current** shows
  the whole clip again ([Versions](../editor.md#versions)).
- **Never cut**: a session recording.
- **Connected**, the server makes the cut as it takes the clip in, before any link exists, and
  **Save and share** uploads with it too. On a server with link previews in external storage, a clip
  cut this way counts as edited ([Made ahead](../storage.md#made-ahead)).
- **In standalone**, the library on this PC makes the cut the same way.

A change counts for each clip that goes up, or into the library, after it; a clip already there
stays as it is. The editor's own **keep the last** buttons do the same for one clip at a time ([Keep
the last seconds](../editor.md#keep-the-last)).

## Keys {#keys}

Connected only. These keys work anywhere, also in a game. **Save a clip** is under
[Essentials](essentials.md#hotkeys-save), and every key, with how to set one, is under
[Hotkeys](hotkeys.md).

### Save and share {#hotkeys-save-share}

Ctrl+Alt+Shift+S unless you change it: OBS saves the replay, the app uploads it (with [Keep the
last](#upload-keep-last), when set), waits until it embeds, copies the link and plays the [Link
copied](#sounds-link-copied-enabled) sound. More under [Hotkeys](hotkeys.md#hotkeys-save-share).

### Share the last clip {#hotkeys-share-last}

Ctrl+Alt+S unless you change it: asks the server for the link of the last clip this PC sent, waits
until it embeds, copies it and plays the sound. More under [Hotkeys](hotkeys.md#hotkeys-share-last).

## Sounds {#sounds}

Short sounds tell you what the app did without a look at the screen, also in a game in full screen,
where Windows holds notifications back. Each cue has a switch with **Preview**, and while it is on,
a **Volume** and a **Sound**. The session recording's sound is under [Session
recording](session.md#reminders).

Only WAV files play. On Windows the app plays them itself; on Linux it uses `paplay`, `aplay` or
`ffplay`, the first it finds, and plays nothing without one of them. **Preview** plays the cue with
its volume and file as set, even while its switch is off, and says why when it cannot.

### Clip captured {#sounds-captured-enabled}

On by default. Plays the moment OBS takes a save from the app (a key, or **Save clip** here, in the
tray or in the window's bar), before the replay is written, which for a long replay takes a few
seconds. A clip that comes another way, such as OBS's own Save Replay key, plays it once the app
finds the file. Files already in the folder when the app starts play nothing. Its bundled sound is
`save.wav`, two rising notes.

### Volume {#sounds-volume}

Each cue's volume, 0 to 100: 100, the default, plays the WAV as it is, and lower plays it quieter.
It cannot make a file louder than it is. **Link copied** starts at 50, as it plays the same sound as
**Clip uploaded**.

### Sound {#sounds-file}

Each cue's file: a WAV of your own, typed in or picked with **Browse**. Empty plays the cue's
bundled sound, named in the box. A file that is not a WAV, or that cannot be read, plays nothing:
the app's log says why, and **Preview** says so at once.

### No clip {#sounds-no-clip-enabled}

On by default. Plays when the Save key finds no replay buffer because the rules for games keep it
off: a game's own setting under [Games and apps](games.md#replay-buffer) keeps it off while that
game runs, or [The replay buffer](essentials.md#obs-replay-buffer) runs only while a game runs and
none runs. A notification says the same, but the sound reaches you in a game in full screen, where
Windows holds notifications back. Its bundled sound is `no_clip.wav`, a short falling tone. [When a
clip cannot be saved](../games-and-apps.md#when-a-clip-cannot-be-saved).

### Clip uploaded {#sounds-uploaded-enabled}

**Clip in the library** in standalone. On by default. Connected, it plays when the server confirms
the clip is in the library; for a **Save and share** clip, **Link copied** plays instead. In
standalone it plays when the clip is ready in the library on this PC, and when a clip sent with
**Upload to the server** is on the server too. Its bundled sound is `uploaded.wav`, one soft note.

### Link copied {#sounds-link-copied-enabled}

Connected only. On by default. Plays when a link lands on the clipboard: from **Save and share** or
**Share the last clip**, and from **Copy link** in the tray, in a notification or on Settings,
Status. Several presses of Save and share in a row copy only the newest clip's link, with one sound.
Its bundled sound is `uploaded.wav` at half the volume.

## On this PC {#on-this-pc}

The line beside the group's name says which folder the app watches, or that it does not exist.

### Clips folder {#clips-dir}

Where clips land. `C:\Clips` on Windows and `~/Clips` on Linux unless the setup or you set another.

- **With reel's own OBS** it is OBS's recording folder too: reel writes it into OBS's settings, and
  OBS starts again with it.
- **With an OBS of your own**, the app takes OBS's own recording folder while OBS is connected and
  [Follow OBS](capture.md#clips-follow-obs) is on, and the row names the folder OBS says. This
  folder is watched until OBS connects and says its own, and always with Follow OBS off.

The app watches the folder as a safety net, so a clip saved while OBS and the app are not connected
is still found: a file counts once its size has held still and it can be opened ([Still
for](capture.md#stable-seconds), [Extensions](capture.md#extensions)). Only the folder itself is
watched, not its subfolders. Connected, clips stay in it after they go up, as long as **Keep up to**
and **Keep for** say. In standalone each clip is taken into the library on this PC ([Library
folder](library-folder.md)).

### Recording folder {#recording-folder}

With an OBS of your own: your OBS's recording folder, where it writes recordings and replays, as OBS
says it. A folder typed or picked with **Browse** is set in OBS, in the profile it uses. It can be
changed once OBS is connected, never while OBS records, and a running replay buffer starts again so
the change takes. With Follow OBS on, the clips folder is this one.

### Keep up to {#cache-gb}

Connected only. How much of the clips the server has already confirmed stays in the clips folder on
this PC: 5 GB unless you change it, up to 10000. Over it, confirmed clips are deleted from this PC,
the oldest first, until the rest fits. 0 deletes each clip as soon as the server confirms it.

Only clips the server confirmed are ever deleted: a clip that waits, goes up or failed stays, and so
does any other file in the folder. Deleting happens when the app starts, after each upload the
server confirms, and when you change this or **Keep for**. The clip stays on the server.

### Keep for {#cache-days}

Connected only. Confirmed clips older than this many days, counted from when the server confirmed
them, are deleted from this PC: 14 days unless you change it, up to 3650. 0 means no age limit, and
only **Keep up to** counts. The age goes first, then the size.
