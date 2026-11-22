# Session recording

A session recording keeps the whole session, from **Record** until **Stop**, where the replay buffer
keeps only the last minutes. You start and stop it with **Record** in the window's bar, **Record a
session** in the tray, or its key; or it starts by itself with a game. Connected, it goes up as any
clip does once it stops; in standalone it goes into the library on this PC. This page holds its key,
what games do with it, and its reminders. These are the desktop app's own settings, kept on this PC:
each PC with the app has its own. [Recording a whole
session](../desktop-app.md#recording-a-whole-session) tells the whole story, with what a recording
costs and when it stops by itself.

## Record a session {#record-a-session}

### Record a session {#hotkeys-record}

The key that starts a recording of the whole session, and stops the one that runs: Alt+F9 unless you
change it. More under [Hotkeys](hotkeys.md#hotkeys-record).

## With a game {#with-a-game}

What every game does with a session recording. A program's class and its own settings under [Games
and apps](games.md) change it for that program: **Games and apps**, at the top of the group, opens
that page. [Session recordings](../games-and-apps.md#session-recordings) tells the whole story.

### Every game records by itself {#session-record-by-itself}

Off by default: only the games you choose record by themselves, each with [Record by
itself](games.md#record-by-itself) set to **On** on its page under Games and apps. On, a session
recording starts when any program sorted as a Game starts, but one whose own setting is **Off**.

One recording covers every game: a game that starts while a recording runs starts no second one. A
game that runs already when the app starts starts none; one that starts after it does.

### Stop the session recording when its game closes {#session-stop-with-game}

On by default: a recording stops once its game has closed and the [grace
period](games.md#session-grace-seconds) under Games and apps has passed, so a game that restarts
within it does not cut the recording. That is also a recording you started by hand while the game
ran. With several such games running, it stops once the last of them has closed. A program's own
[Stop when it closes](games.md#stop-when-it-closes) changes it for that program.

Off, a recording runs until you stop it.

### Record by itself now {#record-by-itself-now}

Names the games that record by themselves, as Games and apps and the switch above set them. With
**Every game records by itself** on and no game in the list yet, it says so; with none chosen, it
says to set **Record by itself** on a game's page.

## Reminders {#reminders}

So a recording is not forgotten for hours. **Preview**, at the top of the group, plays the
reminder's sound as set below.

### Remind every {#session-remind-minutes}

While a session recording runs, a sound and a notification, "Still recording a session", every this
many minutes, counted from the recording's start: 30 unless you change it, up to 600. 0 is never.
The sound is what reaches you in a game in full screen, where Windows holds notifications back. A
change during a recording counts on from its last reminder, and a PC that slept through several
reminders gives one when it wakes.

### Play {#sounds-session-enabled}

On by default: the sound plays when a session recording starts, and at each reminder. Off, the
reminders come as notifications alone. Its bundled sound is `session.wav`, one short note twice.

### Volume {#sounds-session-volume}

The sound's volume, 0 to 100: 100, the default, plays the WAV as it is, and lower plays it quieter.
It cannot make a file louder than it is.

### Sound {#sounds-session-file}

A WAV of your own, typed in or picked with **Browse**. Empty plays the bundled `session.wav`. A file
that is not a WAV, or that cannot be read, plays nothing: the app's log says why, and **Preview**
says so at once. How the app plays its sounds on each system: [Sounds](clips.md#sounds).
