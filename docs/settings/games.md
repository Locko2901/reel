# Games and apps

The list of the programs reel has seen on this PC: which ones are games, the name each gives its
clips, and what one program does apart from its class. Steam's installed games come into it by
themselves. What every game gets is set where its subject is: the replay buffer under
[Essentials](essentials.md#obs-replay-buffer), recording by itself under [Session
recording](session.md#with-a-game), and on Windows the waits under [Uploads](uploads.md) (under
[Encoding](encoding.md) in standalone). The page's first line links to each, and ends with **How the
list works**, which opens [Games and apps](../games-and-apps.md), the guide with the whole story.

**What happens while a program runs**, under that line, shows the order: every game, then its class,
then its own setting, each one after the first changing the one before ([Every game, its class, its
own setting](../games-and-apps.md#every-game-its-class-its-own-setting)).

These settings and the list are the desktop app's own, kept on this PC: the list is `programs.json`
in the app's settings folder (Settings, App, **About** names it), and nothing about it leaves the PC
but with **Show on Discord** on. The count beside the page's name in the nav is of the programs not
sorted yet.

On Linux reel cannot tell which program is in front: the list fills from Steam and by hand, and a
clip takes the name of the game that runs. The waits and **Clips are named** are Windows' alone.

## Steam {#steam}

The line beside the group's name says how many Steam libraries reel found on this PC, read here, or
that it found no Steam.

### Add my Steam games to the list {#games-steam}

On by default. Steam's installed games come into the list as Games, with their names in Steam, and
each new one as Steam finishes installing it. Steam's own tools (the runtimes, Proton, the
redistributables) and soundtracks stay out; an application, such as a wallpaper or a sound tool, and
a program added to Steam by hand come in as Not sorted yet. Each Steam game counts as its whole
folder: any program inside it, its launcher too, is that game, but one in the list on its own.

reel reads Steam's own files on this PC, and nothing is sent anywhere: when the app starts, a few
seconds after Steam installs or removes something, and once an hour. The line under the switch says
how many of the list came from Steam, and the last that came in.

Off, Steam adds nothing, and the games it added stay in the list. Without Steam the switch keeps its
value, and its games come in once Steam is installed. For a Steam that is not where it installs
itself, the app reads the folder in `REEL_STEAM_DIR` ([Other
variables](../configuration.md#other-variables)). [Steam](../games-and-apps.md#steam) has the types
and the rules.

### Not added again {#not-added-again}

Shows once you have removed a Steam game from the list: those games, which Steam does not add back.
**Add back** lets Steam add one again at its next read, in a few seconds. On Windows a removed game
you play can still come in as Not sorted yet, as any program does.

## Discord {#discord}

What your friends see on Discord while you play. Only a program sorted as a Game counts.
[Discord](../games-and-apps.md#discord) tells the whole story, and [What leaves your
PC](../privacy.md#what-leaves-your-pc) what it sends.

### Show on Discord {#discord-show}

Off unless you turn it on. On, while a Game runs and reel clips it or records it, your friends see
it on your Discord profile: **Clipping with reel**, or **Recording with reel** during a session
recording, the game, how many clips you saved while it runs, and a button to reel.

- It goes through the Discord app on this PC, and only while that runs; Discord in a browser cannot
  be told. reel tries again every 30 seconds, so it shows once Discord starts.
- The line under the switch says what reel sent, or why nothing went.
- Discord shows one game card at a time: another app that sends its own, such as an editor
  extension, can take its place.
- Whether Discord shares your activity at all is Discord's own setting, which reel cannot see.

### Name the game {#discord-name-game}

Shows while **Show on Discord** is on. On by default: the card names the game, with its picture from
Discord's own list of games, which reel reads from `discord.com` at most once a day. Off, Discord
says **A game**, with reel's mark, and reel does not read that list ([Discord's list of
games](../games-and-apps.md#discords-list-of-games)).

### Button to reel {#discord-button}

Shows while **Show on Discord** is on. On by default: a button, **reel on GitHub**, under the card
for your friends. Discord never shows it to you.

## When a game closes {#when-a-game-closes}

For every game.

### Grace period {#session-grace-seconds}

60 seconds unless you change it, from 0 to 600. A game that closes and starts again within it counts
as still running: a launcher that restarts the game, a crash and a quick start again, a change of
graphics settings that restarts it. A session recording goes on as one, and a replay buffer that
runs only while a game runs stays. Once it has passed, what the game started stops. With 0, that
happens as soon as the game closes.

While it runs, the window's bar counts down to what its end stops, and **Show on Discord** keeps the
card until then. [The grace period](../games-and-apps.md#the-grace-period).

## The list {#the-list}

Every program reel has seen, with a count of them beside the name. The ones not sorted yet come
first, marked, to be opened and sorted; then the rest by name, with Ignored ones at the end. The
line under each name says its class, its own settings, what Steam says of it, its clips and session
recordings, and whether it runs or when reel last saw it. A click opens its page,
[below](#a-programs-page). An empty list says how a program gets in ([How a program gets
in](../games-and-apps.md#how-a-program-gets-in)).

### Find a program {#find-a-program}

Shows with two programs or more: keeps the programs whose name or file holds what you type.

### Add a program {#add-a-program}

Puts a program into the list yourself, for one reel has not seen yet:

- **Running now**: one of the programs with a window (on Linux, the programs that run outside the
  system's own folders); start the game first.
- **A file**: its file, on Windows a program (`.exe`) or a shortcut to one.

A program in the list already opens its page instead. **Name** is its name in Steam when it sits in
a Steam library, or one guessed from its file; type another if you like. **Class** is Game unless
you pick another.

## A program's page {#a-programs-page}

Opens from the list, with **Games and apps** at its top to go back. Under its name, its class and
its own settings. **The program** says when and how it came in, with **Show in folder**; then come
its **Class**, what **This program** does apart from it, and **Remove from the list**. [A program's
page](../games-and-apps.md#a-programs-page) has the whole picture.

### Name {#name}

What its clips and session recordings take: its name in Steam, or one guessed from its file, until
you give it another, up to 80 characters. Enter keeps a new name, Esc puts the old one back. New
clips take the new name. When older clips of this PC carry the old name, reel asks once: **New clips
only**, or **Rename these too**; a title you changed by hand stays as it is. Connected, with a
server that can take a title from the app, **And the N on the server** changes the titles of the
clips this PC sent there too ([Renaming](../games-and-apps.md#renaming)).

### File {#file}

The program's file, for a program that is not a Steam game. For one added to Steam by hand, the row
says so.

### Folder {#folder}

For a Steam game: its folder in a Steam library, with its app id. Every program in that folder
counts as this game, but one in the list on its own. A game uninstalled in Steam stays in the list
with its name, so its clips keep it.

### In the library {#in-the-library}

How many clips and session recordings of this PC are named after it. **Show them** opens the
library, searched for its name.

### Clips are named {#clips-are-named}

Windows only. After its name, the default, or after its window's title, for an emulator or a
launcher that shows the game it runs: each clip then takes the name of the game on screen.

### Class {#class}

What the program is, as four cards; one click sorts it.

- **Game**: counts as a game. Clips take its name, a session recording stops when it closes, and it
  records by itself when that is on for it.
- **Not a game**: a full screen tool, a player, a VM. Clips saved in front of it still take its
  name, but nothing waits for it, and neither the replay buffer nor a recording starts or stops with
  it.
- **Not sorted yet**: new in the list, until sorted. On Windows, in full screen it counts as a game
  for the waits and the replay buffer. It never starts a recording.
- **Ignored**: as if it were not there. No clip takes its name, nothing starts or waits for it, and
  it does not come into the list again.

[The classes](../games-and-apps.md#the-classes) has the table.

### Replay buffer {#replay-buffer}

This program's own setting for the replay buffer while it runs: **As its class**, **On** or **Off**.
On, the buffer runs while it runs, whatever its class, when [The replay
buffer](essentials.md#obs-replay-buffer) runs only while a game runs. Off stops the buffer while it
runs, whatever else runs beside it, for a heavy game that needs every bit of the PC; the Save key
then plays the [No clip](clips.md#sounds-no-clip-enabled) sound.

Each row of this group shows the line of where its value comes from, the one that counts in white.
While the class is Ignored, the rows cannot be changed: an Ignored program does nothing.

### Record by itself {#record-by-itself}

**As its class**, **On** or **Off**: whether a session recording starts when it starts. On records
it whenever it runs, even with [Every game records by itself](session.md#session-record-by-itself)
off. Off never does.

### Stop when it closes {#stop-when-it-closes}

**As its class**, **On** or **Off**: whether the session recording stops when it closes, after the
[grace period](#session-grace-seconds). Off lets the recording go on after it closed.

### Uploads wait {#uploads-wait}

Windows only. **Uploads and encoding wait** in standalone with a server paired, **Encoding waits**
in standalone. **As its class**, **On** or **Off**: whether the uploads, or the library's heavy
work, wait while this program has the screen, whatever the switch for every game says. On, for one
heavy game, with the waits for every game off. [The waits](../games-and-apps.md#the-waits).

### Remove from the list {#remove-from-the-list}

Takes the program out of the list. On Windows it comes back as Not sorted yet when it is seen again;
on Linux, **Add a program** puts it back. Steam does not add a removed Steam game again: it shows
under [Not added again](#not-added-again), and on Windows comes back as Not sorted yet only when you
play it. To keep a program out for good, sort it as **Ignored** instead.
