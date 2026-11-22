# Library folder

Standalone only: where the library is on this PC, a copy of it to keep, and how big it may grow.
What the library keeps, and for how long, is under [Library and storage](storage.md). This page
is one of the app's own, kept on this PC with its other settings, and it sets the library on
this PC. [Standalone](../setup/standalone.md) tells the whole story, and
[Back up a standalone library](../backup.md#back-up-a-standalone-library) what to keep.

## Library {#library}

Where reel keeps the library on this PC, and a copy to keep.

### Library folder {#library-folder}

The folder that holds the clips with every version, their database, the trash and the raw files
reel kept. Its line says where it is, how much it takes and how much is free on its drive, and
whether that drive can be unplugged; or, while it is not there, why. It starts in
`%LOCALAPPDATA%\reel\library` on Windows and `~/.local/share/reel/library` on Linux.

- **Open folder** opens it.
- **Move** puts the library in a folder you pick, after a line that says how much moves and how
  much is free there. An empty folder takes the library itself; a folder with other things in it,
  such as `D:\`, gets a new folder `reel library` inside it. On the same drive the folder is just
  renamed, at once. To another drive, reel on this PC stops while every file is copied, read back
  and checked; the clips you save meanwhile wait in the clips folder and go in after, and the old
  folder is removed once the copy is checked. **Stop** leaves the library where it is.
- A folder you pick that holds a reel library, such as a copy, is taken up as it is: reel on this
  PC works from it from then on, and the library it used before stays where it is.

A folder on the network is refused: reel's database needs a drive of this PC. A drive you unplug
works: while it is away, reel on this PC waits, the clips wait in the clips folder, and reel
starts again within ten seconds of the drive coming back.
[The library on another drive](../setup/standalone.md#the-library-on-another-drive) has every
case, and [Is not there: is its drive plugged in?](../troubleshooting.md#is-not-there-is-its-drive-plugged-in)
what to do when the line says it is missing.

### A copy of the library {#a-copy-of-the-library}

**Make a copy** puts the whole library (the database, every clip with its versions, the trash and
the raw files reel kept) into a new folder named like `reel copy 2026-11-04 2130`, inside a folder
you pick: a backup drive, or a USB drive to take to another PC. reel goes on while it copies, and
every file is read back and checked before the folder gets its name. The copies under a size stay
out; reel makes them again when they are asked for. **Stop** ends it.

The copy is made through reel on this PC, so the button is greyed out while that is not running.
To use a copy, here or on another PC, pick its folder with **Move**
([A copy of the library](../setup/standalone.md#a-copy-of-the-library)).

## Size {#size}

### Library cap {#standalone-cap-gb}

The size the library is kept under, in GB: 100 by default, up to 100000, and 0 is no cap.
Housekeeping runs every 10 minutes and moves clips to the trash until the library is under it:

- The oldest go first, by their recording time. Older session recordings go before clips, so
  one long recording does not push out a hundred clips.
- Never moved, whatever its age: a starred clip, a clip in a collection, and, while
  [The cap spares unsent clips](#standalone-cap-keeps-unsent) is on, a clip that is on no server.
- A clip it moves is like a deleted one: the trash keeps it for its days
  ([Trash](storage.md#share-trash-days)), **Restore** there brings it back, and the disk has its
  room back once the trash lets it go.

The line under the field says what the library holds and, as you type, what the cap would move at
its next run: how many clips, how much, recorded when, and what it spares. Saving a new size
starts reel on this PC again with it. [The library cap](../sharing.md#the-library-cap) says what
it counts. On a server, the same cap is [Library cap](storage.md#storage-cap-gb) under Library and
storage; in standalone this row takes its place.

### The cap spares unsent clips {#standalone-cap-keeps-unsent}

On by default: a clip that is not on a server too is never moved by the cap. A clip counts as
sent once it is on a server, such as after **Upload to the server** on
[Status](status.md#last-clips).

So in a library whose clips never go to a server, the cap moves nothing while this is on: the line
under **Library cap** counts those clips as "on no server yet". Turn it off for the cap to keep a
library that lives on this PC alone under its size. Saving it starts reel on this PC again.
