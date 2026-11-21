# Games and apps

Settings, **Games and apps**, in the app's **Recording** group, is the list of the programs reel
has seen on this PC: which ones are games, the name each gives its clips, and what each does
while it runs: whether the replay buffer runs, whether a session recording starts by itself and
stops when it closes, and whether uploads and encoding wait. Steam's installed games come into it
by themselves. The list is this PC's alone and stays on it.

## How a program gets in

A program comes into the list as **Not sorted yet**, named after its file (`emberpass.exe`
becomes Emberpass), the first time

1. a clip or a session recording is saved while it is in front, or
2. it takes the whole screen, as a game in full screen does, or
3. you add it, with **Add a program**: one of the programs that run now, or its file (on
   Windows a program, `.exe`, or a shortcut to one, `.lnk`).

Steam's games come in by themselves, as [below](#steam). Browsers, chat and music programs,
editors, Explorer, OBS and reel itself never come in: reel knows they are no game.

!!! note "On Linux"

    The app cannot tell which program is in front on Linux, so there the list fills from Steam
    and by hand. It can tell which programs of the list run: the replay buffer, a session
    recording that starts by itself and one that stops with its game work as on Windows, and a
    clip takes the name of the game that runs, when one Game alone runs. Otherwise clips are
    named as before: by the game OBS captures by its window, when it does, and otherwise by
    their date. A program not sorted yet never counts as a game there, as reel cannot tell
    whether it has the whole screen; the waits are Windows' alone.

## Steam

**Add my Steam games to the list**, at the top of the page, is on by default. reel reads Steam's
own files on this PC: where Steam keeps its libraries, each installed app's manifest, and the
type of each app in Steam's cache (`appcache/appinfo.vdf`). Nothing is sent anywhere.

| Steam's type | Comes in as |
|---|---|
| a game, a demo or a mod | **Game**, with its name in Steam |
| an application, such as a wallpaper or a sound tool, or a type reel does not know | **Not sorted yet** |
| a tool (the runtimes, Proton, the redistributables), a soundtrack, a video, a setting | stays out |
| a program added to Steam by hand, outside its store | **Not sorted yet**, as its program |

An app comes in once Steam says it is fully installed. When `appinfo.vdf` is missing, or of a
version reel cannot read, the types come from simpler rules: Steam's own tools by their app
id and Proton by its folder stay out, a soundtrack stays out by where its files are (under
`steamapps/music`), and the rest comes in as a game.

Each Steam game counts as its whole folder: any program inside it, the game's own launcher
too, is that game, unless that program is in the list on its own.

When reel reads Steam: when the app starts, a few seconds after Steam installs or removes
something in one of its libraries, and once an hour, for Steam installed later or a library on
a drive that comes later. A new install leaves its first read until the setup is done, so the
setup's switch decides.

- A game that came in later says **new from Steam today** (then the day it came) until it is
  first seen; the line under the switch names the last one.
- A Steam game you take out with **Remove from the list** is not added again: it shows under
  **Not added again**, with **Add back**. Played, it can still come in as Not sorted yet, like any
  program.
- A game uninstalled in Steam stays in the list, with its name, so its clips keep it; its line
  says **not installed in Steam**. Installed again, it is the same entry.
- With the switch off, Steam adds nothing; the games it added stay, they are your list now.
- Without Steam, the switch keeps its value and its line says Steam was not found. Its games
  come in once Steam is installed.

Steam's own client, `steam.exe`, is no Steam app: it comes in as any program does.

Each read writes a line to the app's Log (Settings, **Log**) that says how the types were read
and what came of it, for example `Steam read in C:\Program Files (x86)\Steam: 2 libraries, the
types from appinfo.vdf version 41; 23 as Game, 2 as Not sorted yet, 9 stayed out, 0 still
installing`, or `the types from the fallback rules (...)` with why.

## The classes

A program's page shows its class as four cards; one click sorts it.

| | Game | Not a game | Not sorted yet | Ignored |
|---|---|---|---|---|
| Clips saved in front of it take its name | yes | yes | yes | no |
| The replay buffer, when it runs only while a game runs | runs while it runs | not started by it | runs while it has the whole screen | as if it were not there |
| Records by itself, when every game does | yes | no | no | no |
| Stops the session recording when it closes | yes | no | no | no |
| The waits, when they are on | while it is in front | never | while it has the whole screen | never |
| In the list | by name | by name | first, marked | at the end |

- **Game**: counts as a game.
- **Not a game**: a full screen tool, a player, a VM.
- **Not sorted yet**: new in the list, and acting as before there was a list: in full screen it
  counts as a game for the waits, when they are on.
- **Ignored**: as if it were not there: no clip takes its name, nothing waits for it, and it does
  not come back as Not sorted yet.

## A program's page

- **The program**: when and how it came in, and **Show in folder**.
- **Name**: what its clips and session recordings take; its name in Steam, or the one guessed
  from its file, until you give it another ([Renaming](#renaming)).
- **File**, or for a Steam game **Folder**, with its app id.
- **In the library**: the clips and session recordings of this PC named after it, with **Show
  them**.
- **Clips are named** (Windows): after its name, or after its window's title, for an emulator
  or a launcher that shows the game it runs.
- **Class**: the four cards above.
- **This program**: what it does while it runs, each **As its class**, **On** or **Off** for this
  program alone, with the line of where its value comes from ([Every game, its class, its own
  setting](#every-game-its-class-its-own-setting)): **Replay buffer**, **Record by itself**,
  **Stop when it closes**, and on Windows the waits.
- **Remove from the list**: it comes back as Not sorted yet when it is seen again; a Steam game
  is not added again. The class Ignored keeps a program out for good.

The list says **running now** on the line of a program that runs.

## While a game runs

reel looks at the programs that run every 3 seconds: on Windows each process and the file it
runs from, on Linux each program's file under `/proc`. A program counts when its file is in the
list, or lies in the folder of a Steam game of the list. Nothing of it leaves the PC; the Log
says each start and close of a program of the list, for example `game: Ember Pass started
(C:\Games\Ember Pass\EmberPass-Win64-Shipping.exe), a Game`.

### The replay buffer

Settings, **OBS**, **The replay buffer**, **Runs**:

- **always**, the default: as before, a clip can be saved at any time.
- **only while a game runs**: OBS's replay buffer starts with a game and stops once the last
  one closed and the [grace period](#the-grace-period) passed. It leaves the PC its memory and
  its encoder while no game runs.

A game can also stop it while it runs, with its own **Replay buffer** set to **Off**: a heavy
game that needs every bit of the PC. Such an Off wins over every other program that runs: the
buffer stays stopped until that game closes.

With an OBS of your own, reel starts and stops OBS's replay buffer only with **Start the
replay buffer** on (OBS, obs-websocket); choosing **only while a game runs** turns it on, and
says so. The setup's step **Games** asks the same, **Always** pressed.

### Session recordings

Settings, **Session recording**, **With a game**:

- **Every game records by itself**, off by default: on, a session recording starts when a Game
  starts. Off, only the games you choose do, with **Record by itself** on their page.
- **Stop the session recording when its game closes**, on by default: a recording stops once its
  game closed and the grace period passed. That is also one you started by hand while the game
  ran. With several such games running, it stops once the last of them closed.
- **Grace period**, 60 s by default (0 to 600).
- **Record by itself now** names the games that do, as the list sets them.

One recording covers every game: a second game that starts while one runs does not start
another. A game that runs when reel starts starts none; one that starts after does. The
recording takes the game's name, as a clip does. Its notification, **Session recording started**,
says `Ember Pass started. It stops when Ember Pass closes; Alt+F9 stops it sooner.`, and its end
**Session recording stopped: Ember Pass closed**. **Record** and Alt+F9 stop it sooner, as
any recording.

### The grace period

A game that closes and starts again within the grace period counts as still running: a
launcher that restarts the game, a crash and a quick start again, a change of graphics
settings that restarts it. The recording goes on as one, and a buffer that runs only while a
game runs stays. The Log says `game: Ember Pass closed; the session recording stops in 60 s
unless it starts again`.

While it runs, the window's bar counts down to what its end changes, in place of the game's
name: **ready**, then **buffer stops in 0:45** (a buffer that runs only while a game runs);
**recording**, then **stops in 0:45** (a recording that stops with its game, and with it the
buffer when both end together); **no replay buffer**, then **back in 0:45** (a game whose own Off
kept the buffer off). Its menu's sentence says which game closed: `Ember Pass closed: the
session recording stops in 0:45; if Ember Pass starts again before then, it goes on.` The tray
has a line with the time it ends, `Ember Pass closed: the recording stops at 21:14:05`. A game
that starts again within it takes the countdown away, and at its end the recording or the
buffer stops right then.

### When a clip cannot be saved

While the replay buffer is stopped by these rules, the Save hotkey saves nothing. reel says so
at once, as a game in full screen may hold notifications back:

- the sound **No clip** (Settings, **Sounds**), a short falling tone, which can be turned off or
  changed there;
- the notification **No clip saved**: `The replay buffer is off while Starfall Tactics runs, as
  set for it under Games and apps. Alt+F9 records a session instead.`, with **Open Games and
  apps**;
- in the window, a toast that says why, and the bar's state;
- in the tray, **Save clip** greyed, with why on the line under it.

**Record** (Alt+F9) works all along.

### In the bar and the tray

While a Game of the list runs, the window's bar names it after its state's words, quietly; a
long name is cut short, and gives way first on a narrow window, before Record or the state's
fix. Its menu's sentence says what the game gets: `Ember Pass runs for 14 min, a Game: the
replay buffer runs.` The tray names it on a line of its own, `Ember Pass runs: the recording
stops when it closes`, and its tooltip ends with it.

With the buffer stopped, the bar says **waiting for a game** (only while a game runs, and none
runs) or **no replay buffer** (a game's own Off), the latter with **Games and apps** to change
it. A program not sorted yet that takes the whole screen shows in the tray as `Quarry Kings is
new: sort it`, with **Game**, **Not a game**, **Ignore** and **Open in settings**.

## Renaming

A new name is kept as typed, and new clips take it. When older clips and session recordings of
this PC carry the old name, reel asks once, for this rename: **New clips only**, or **Rename these
too**, with two of their titles before and after. Renaming them too gives them the new name as
their game, and changes the titles reel made them (the name followed by the moment, such as
`libvmcst, Nov 11, 18:30`); a title you changed by hand stays as it is. In standalone that is
the library on this PC, and the game's own collection follows.

On a connected PC the titles change in the app (Settings, Status, and a clip not sent yet goes
up with its new title). The question also offers **And the N on the server**, on to begin with:
the titles of the clips this PC sent, changed on the server too. reel sends each one's title
before and after; the server changes it only while it still reads as before, so a title someone
changed on the server since stays, and the toast says how many did. A clip whose title on the
PC was changed by hand is not sent at all.

A server of a version before this cannot take a title from the app: the question then says the
server's titles stay, with how many the server has; change a title on the clip's page on the
server.

## The waits

**Uploads and the library's heavy work can wait while a game has the screen**, for a PC a game
keeps busy or a slow connection. They are off unless you turn them on (Windows):

| On this PC | Where | Its words | What waits |
|---|---|---|---|
| standalone | Settings, **New clips** | **Encoding waits while a game has the screen** | the library's heavy work: a re-encode, the copies under a size, the hover loop and the filmstrip |
| standalone, with a server paired | Settings, **New clips** | **Uploads and encoding wait while a game has the screen** | that, and the uploads to the server |
| connected | Settings, **Upload** | **Uploads wait while a game has the screen** | the uploads; a Save and share's goes at once all the same |

Connected, **Only on the internet, not on the LAN** shows under it while it is on: then an
upload to a server on the local network goes on during a game. An install updated from a
version before Games and apps starts with the waits off too.

What waits, while which program is in front, comes from the [three
layers](#every-game-its-class-its-own-setting): a Game waits while its window is in front (a
borderless game counts), a program not sorted yet while it has the whole screen.

The update of the OBS copy for games that run as administrator still waits until no full
screen program is in front, whatever these say: Windows' question would take the game's screen.

## Every game, its class, its own setting

What a program does while it runs comes in three layers, each changing the one before:

1. **Every game**: set where its subject is.
2. **Its class**: what the class makes of it ([The classes](#the-classes)).
3. **Its own setting**, under This program on its page: **On** or **Off** for this program
   alone, whatever the other two say; **As its class** leaves them.

| | Every game, set under | Off by its class | Its own On | Its own Off |
|---|---|---|---|---|
| **Replay buffer** | OBS, The replay buffer: always, or only while a game runs | Not a game and Ignored never start it; Ignored is as if it were not there | it runs while it runs | stopped while it runs, whatever runs beside it |
| **Record by itself** | Session recording, Every game records by itself | every class but Game | it records when it starts | never |
| **Stop when it closes** | Session recording, Stop the session recording when its game closes | every class but Game | the recording stops when it closes | the recording goes on after it |
| **The waits** (Windows) | New clips (Upload when connected) | Not a game and Ignored | they wait while it is in front | never |

Ignored overrides its own settings: an Ignored program does nothing. A program's page shows the
layers as a line under each row, the one that counts in white: `Every game, set under New
clips: off › Starfall Tactics: on`. At the top of the list, **What happens while a program
runs** shows the order.

## What stays on the PC

The list lives in `programs.json` in the app's settings folder (Settings, Startup, **About**
names it). Steam's files are read on this PC, and nothing about them, or about the programs
you run, leaves it. See [Privacy and security](privacy.md).

## Examples

**A program with a file name for a name.** A VM tool, `de-libvm-c_str_onkl_sto.exe`, names its
clips "De Libvm C Str Onkl Sto". On its page, type `libvmcst` as its Name and press Enter; to
the question, **Rename these 13 too**: its older clips become `libvmcst, Nov 11, 18:30` and so
on. Sorted as **Not a game**, its full screen holds no upload, also with the waits on.

**An emulator.** Its window's title says the game it runs. **Clips are named** after its
window's title: each clip takes the name of the game on screen, not the emulator's.

**Steam and a game's own launcher.** Ember Pass came in from Steam as a game: every program in
its folder counts as Ember Pass, its launcher too. To keep the launcher from naming clips, add
it on its own (**Add a program**, its file) as **Ignored**. Steam's client in Big Picture, which
takes the whole screen, comes in as Not sorted yet: sort it as **Not a game**.

**A heavy game.** One game keeps the PC so busy that a re-encode beside it stutters, while the
others are fine. Leave the waits off under New clips, and on that game's page set **Encoding
waits** to **On**: the line reads `Every game, set under New clips: off › Starfall Tactics: on`.

**A heavy game with its replay buffer off.** Starfall Tactics needs all of the PC's memory, and
clips of it are not wanted. On its page set **Replay buffer** to **Off**. While it runs, OBS's
replay buffer is stopped, also with another game running beside it; the bar says **no replay
buffer**, and the Save hotkey plays **No clip** and says why. Alt+F9 still records a session.
Once it closed and the grace period passed, the buffer runs again.

**Two games that record by themselves.** You want every evening of Ember Pass and of Cinder
Road on record, and clips of the rest. Leave **Every game records by itself** off, and on both
games' pages set **Record by itself** to **On**; **Record by itself now** under Session recording
names them. Ember Pass starts a recording; Cinder Road, started an hour later beside it, starts
none, and the one recording goes on. Ember Pass closes first: the recording goes on while
Cinder Road runs, and stops once it closed too and the grace period passed. A crash of Cinder
Road and a start again within the minute does not cut it.
