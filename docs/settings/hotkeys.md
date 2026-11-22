# Hotkeys

Every key of the app in one place. Each one also sits with the settings it belongs to, on the page
named under it: a change in either place is the same key. These are the desktop app's own settings,
kept on this PC: each PC with the app has its own. [Hotkeys](../desktop-app.md#hotkeys) tells the
whole story, with the sounds and the timing.

In standalone, where the library is on this PC, only **Save a clip** and **Record a session** are
there: the other three make or copy a link, or upload, and need a server. Run from a desktop's
shortcut or the command line in standalone, they are refused with a notification that says so.

## Keys {#keys}

The keys work anywhere, also while a game has the focus.

**To set a key**, click its field and press the combination. Esc cancels, Backspace clears it, and a
key left empty does nothing. While a field listens, the app's keys are off, so the press is
recorded, not run. A combination is refused, and the field keeps what it had, when:

- **its last key types or moves the caret** (a letter, a digit, Space, an arrow, Home) and no Ctrl,
  Alt or Win is held with it; F1 to F24, Pause, Scroll Lock, Insert, Print Screen and the media and
  volume keys are fine alone;
- **another key of the app has it** ("Ctrl+Alt+S is already Share the last clip");
- **it is Ctrl+,**, which opens Settings;
- **Windows, the desktop or another app holds it** ("Ctrl+Alt+S is held by Windows or another app;
  ..."): pick another one ([A hotkey is refused](../troubleshooting.md#a-hotkey-is-refused)). When
  only the modifiers reached the field and nothing after them, a line under it says that Windows or
  another app holds that combination.

A game that runs as administrator does not pass its keys to the app ([Games that run as
administrator](../troubleshooting.md#games-that-run-as-administrator)).

**On Linux** the keys depend on the desktop. On X11 the app takes them for the whole desktop, as on
Windows. On Wayland it hands them to the desktop's own shortcuts, and the desktop decides which keys
they get: Settings, [Status](status.md#connections) says which ones it bound. Where neither works,
bind a key in the desktop's keyboard settings to the command this page shows, such as
`reel-desktop --save`; with the AppImage the command is the AppImage's path ([Hotkeys on
Linux](../setup/linux.md#5-hotkeys)).

### Save a clip {#hotkeys-save}

Also under [Essentials](essentials.md#hotkeys-save). OBS saves the replay, and the clip goes up as
any clip does ([Upload](essentials.md#upload-mode)), or in standalone into the library on this PC.
There is none until the setup sets Alt+F10.

- The [Clip captured](clips.md#sounds-captured-enabled) sound plays the moment OBS takes the save,
  before it has written the replay.
- A press within a second of the last save counts as a double tap and saves nothing.
- It saves nothing while clips are paused, while OBS is not connected, or while the rules for games
  keep the replay buffer off; then it says why, the last with the [No
  clip](clips.md#sounds-no-clip-enabled) sound.
- With reel's own OBS and [Capture games that run as administrator](capture.md#obs-setup-elevated)
  on, OBS takes this key itself, as only OBS's own key reaches a game that runs as administrator.
- OBS's own **Save Replay** key, in OBS's settings, saves a clip as well; with an OBS of your own,
  pick a key here that OBS does not use.

### Save and share {#hotkeys-save-share}

Also under [Clips](clips.md#hotkeys-save-share). Connected only; Ctrl+Alt+Shift+S unless you change
it. OBS saves the replay, and the app uploads it at once, ahead of any other upload and whatever
[Upload](essentials.md#upload-mode) says, with [Keep the last](clips.md#upload-keep-last) when that
is set. It waits until the server has the clip's link preview ready, so the link plays where you
paste it, then copies the link and plays the [Link copied](clips.md#sounds-link-copied-enabled)
sound.

Pressed again while one is under way, it saves another clip; only the newest clip's link is copied,
with one Link copied sound. When OBS's replay buffer is off, it saves nothing and says so; where
[Start it in OBS](clips.md#obs-start-replay) lets it, it starts the buffer first, and says to press
again once the moment happened.

### Share the last clip {#hotkeys-share-last}

Also under [Clips](clips.md#hotkeys-share-last). Connected only; Ctrl+Alt+S unless you change it.
The app asks the server for the link of the newest clip this PC sent that the server has, waits
until its link preview is ready, copies the link and plays the [Link
copied](clips.md#sounds-link-copied-enabled) sound. A session recording does not count here. With no
clip yet, a notification says so.

### Upload the last clip {#hotkeys-upload-last}

Also under [Uploads](uploads.md#hotkeys-upload-last). Connected only; Ctrl+Alt+U unless you change
it. It sends the newest clip or session recording that has not gone up yet, or whose upload failed.
It is meant for [Upload](essentials.md#upload-mode) set to **the last, on its key**, where clips
wait until you press it. With **every clip**, clips go up by themselves, so it only sends a failed
one again; with **by hand**, it sends the newest one that waits. With nothing to send, a
notification says "No clip is waiting."

### Record a session {#hotkeys-record}

Also under [Session recording](session.md#hotkeys-record). Alt+F9 unless you change it. It starts a
[session recording](../desktop-app.md#recording-a-whole-session), and stops the one that runs.
Connected, the recording goes up as any clip does once it stops; in standalone it goes into the
library on this PC.
