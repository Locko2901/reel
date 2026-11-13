# Games and apps

Settings, **Games and apps**, in the app's **Recording** group, is the list of the programs reel
has seen on this PC: which ones are games, and the name each gives its clips. Steam's installed
games come into it by themselves. The list is this PC's alone and stays on it.

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
    and by hand, and clips are named as before: by the game OBS captures by its window, when it
    does, and otherwise by their date.

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
- **This program** (Windows): the waits for this program alone, **As its class**, **On** or
  **Off**, with the line of where its value comes from ([The waits](#the-waits)).
- **Remove from the list**: it comes back as Not sorted yet when it is seen again; a Steam game
  is not added again. The class Ignored keeps a program out for good.

## Renaming

A new name is kept as typed, and new clips take it. When older clips and session recordings of
this PC carry the old name, reel asks once, for this rename: **New clips only**, or **Rename these
too**, with two of their titles before and after. Renaming them too gives them the new name as
their game, and changes the titles reel made them (the name followed by the moment, such as
`libvmcst, Nov 11, 18:30`); a title you changed by hand stays as it is. In standalone that is
the library on this PC, and the game's own collection follows.

On a connected PC the titles change in the app (Settings, Status, and a clip not sent yet goes
up with its new title), but not on the server: the app cannot change a clip's title there. Its
question says so, with how many the server has; change a title on the clip's page on the
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

What waits, while which program is in front, comes in three steps, each changing the one
before:

1. **Every game**: the switch above.
2. **Its class**: a Game takes it as it is, while its window is in front (a borderless game
   counts); Not sorted yet takes it while it has the whole screen; Not a game and Ignored never
   wait.
3. **Its own setting**, under This program on its page: **On** or **Off** for this program alone,
   whatever the switch and its class say; **As its class** leaves them.

A program's page shows the three as a line under the row, the one that counts in white:
`Every game, set under New clips: off › Starfall Tactics: on`.

The update of the OBS copy for games that run as administrator still waits until no full
screen program is in front, whatever these say: Windows' question would take the game's screen.

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
