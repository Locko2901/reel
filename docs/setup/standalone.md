# Standalone: the library on your PC

A new install of the app is reel by itself. It keeps your library on the gaming PC and shows it in
its window, with the same pages a server has: the editor, montages, collections, favourites, and
copies under a size for chats with an upload limit. Nothing leaves the PC, and there is no server
to run and no account to make.

To send a clip, you drag its file into the chat: **Show in folder** has it selected, or a copy under
the chat's upload limit downloads. Share links need a server, which you can
[add at any time](#add-a-server-later).

## 1. Install the app

Follow the guide for your PC: [All in one](all-in-one.md) or [Your own OBS](own-obs.md) on
Windows, [Linux](linux.md) for the AppImage or the `.deb`. Every installer carries what standalone
needs, ffmpeg included.

## 2. Check that it is standalone

A new install starts standalone, so there is usually nothing to do:

- **reel** on Windows, and **Set it up for me** on Linux: the setup's last step, **Where clips
  go**, has **This PC only** chosen already. Press **Next**, then **Finish**.
- **reel light**, or your own OBS on Linux: the window opens on Settings, **OBS**. Connect your OBS as
  in [Your own OBS](own-obs.md), steps 2 to 4 (on Linux, [Linux](linux.md), step 4).
- An app that was paired with a server keeps that through an update. To switch it, open Settings,
  **Server** and set **Mode** to **standalone**.

When it worked, **Mode** says "reel runs on this PC", and the window shows the library.

![The library in standalone mode](img/app-standalone-library.png)

The library lives in `%LOCALAPPDATA%\reel\library` on Windows and `~/.local/share/reel/library` on
Linux, until you [move it to another drive](#the-library-on-another-drive). The status chip at the
top of the window has a **Library** row: on this PC, where it lives, and whether reel on this PC
runs.

The library opens in the app's window alone, not in a browser: reel on this PC lets in only the
app, with a key that changes at each start ([Privacy](../privacy.md#the-library-on-this-pc)).

## 3. The first clip

1. Play a game and press **Save a clip**: Alt+F10 after the setup of reel's own OBS. With your own
   OBS, set a key under Settings, **Hotkeys**, or use OBS's own **Save Replay** hotkey. The "clip
   captured" sound plays.
2. A second or two later the notification "Clip in the library" appears. Click **Open**.
3. On the clip's page, under **Send**, press **Show in folder**: the folder opens with the file
   selected. Drag it into the chat.
4. For a chat with a lower upload limit, pick a size under it in the list instead and press
   **Download**. A note at the bottom of the page says where the copy went, with **Show in folder**.

A clip OBS saves in H.264 with AAC sound, up to 1440p and 50 Mbit/s, goes into the library as it
is, never encoded again.

In standalone, the tray and the window offer **Save clip**, and the hotkeys **Save a clip**, alone.
Save and share, Share the last clip and Copy link make links, and links need a server.

### The files are on this PC

A clip's files are in the library already, so the window opens them where they are instead of
downloading a copy:

- **Open** plays the file in the PC's own player. **Show in folder** opens its folder with the file
  selected: Explorer on Windows, your file manager on Linux (or the folder alone, where the file
  manager cannot select a file).
- They are under **Send** for the version as it is, on each row under **Versions**, in **Details**
  for the raw file reel kept, and in a clip's menu in the library (**Open the file**). `d` on a
  clip's page opens its current version.
- A copy under a size, an animated GIF or WebP and the zip of picked clips are made to be sent:
  they download to your Downloads folder, as on a server.
- A file moved or deleted outside reel does not open, and the page says so. Move it back, or delete
  the clip.

## How it treats the PC while you play

- **Low priority.** Every ffmpeg runs at the lowest priority: Windows' idle class, `nice 10` on
  Linux.
- **Heavy work waits for the game.** On Windows, a re-encode, the hover loop and the filmstrip,
  and the copies under a size wait while a game is fullscreen, and run when it ends (Settings,
  New clips, **Wait while a game is fullscreen**).
- **The graphics card where it can.** When it starts, reel tests the encoders with a short encode
  and takes the first that works: on Windows NVIDIA NVENC, AMD AMF, then Intel Quick Sync; on
  Linux Intel Quick Sync, then VAAPI; else the CPU. Settings, **Encoder** shows which one, and its
  **GPU encoder** picks another; Settings, **Status** names the ffmpeg that came with the app, with
  its licence.
- **A cap on its size.** **Library cap** (Settings, Library folder, 100 GB to start with) moves
  the oldest clips to the trash when the library grows past it, as on a server
  ([The library cap](../sharing.md#the-library-cap)). A starred clip or one in a collection never
  goes, and neither does a clip that is on no server, unless you turn **The cap spares unsent
  clips** off. The line under the cap says what it would move at the size you type.

## The library on another drive

The library holds every clip with its versions, their database and the raw files reel kept, and it
grows with your clips. To put it on a bigger drive:

1. Open Settings, **Library folder**. Under **Library**, **Library folder** says where the library
   is, how much it takes, and how much is free on its drive.
2. Press **Move** and pick a folder on the other drive. An empty folder takes the library itself; a
   folder with other things in it, such as `D:\`, gets a new folder `reel library` inside it.
3. The panel says how much moves and how much is free there. Press **Move**.

While it moves, reel on this PC stops and the panel shows how far it is. Every file is copied, then
read back and checked; only then does reel start again from the new folder, and the old one is
removed. Clips you save meanwhile wait in the clips folder and go into the library once it is back.
**Stop** leaves the library where it was. On the same drive the folder is just renamed, at once.

If the app or the PC stops halfway, the next start rolls the move back, or finishes it once every
file was checked. The library is always whole in one of the two places, and the panel says which.

A drive you unplug, such as a USB drive, works too. While it is away, reel on this PC waits: **Save
clip** still saves, the clips wait in the clips folder, and reel starts again by itself within ten
seconds of the drive coming back. If Windows gives the drive another letter, press **Move** and pick
the library's folder at its new letter: a folder that holds a reel library is taken up as it is.

A folder on the network is refused, because reel's database needs a drive of this PC. For a library
on a NAS, [run reel as a server](server.md).

## A copy of the library

Settings, **Library folder**, **Make a copy** puts the whole library (the database,
every clip with its versions, the trash and the raw files reel kept) into a new folder named like
`reel copy 2026-11-04 2130`, in a folder you pick: a backup drive, or a USB drive to take to another
PC. reel goes on while it copies, and every file is read back and checked before the folder gets
its name. The copies under a size are left out; reel makes them again when they are asked for.

A copy is good for two things:

- **Keeping it.** If the drive with the library dies, the copy has every clip as it was when you
  made it. Each copy is a folder of its own; delete the old ones you no longer need.
- **Another PC.** Take the copy along, install reel there, and use it.

To use a copy, on this PC or another one: Settings, **Library folder**,
**Move**, and pick the copy's folder. reel takes up the library in it as it is and works in that
folder from then on; the library it used before stays where it was, and picking that folder again
goes back. To keep the copy itself as it is, copy its folder to where the library should live
first, and pick that one. The copy's `README.txt` says the same.

## New versions

Under Settings, **Startup**, **Updates**, **Check now** asks GitHub for the newest release.
Settings, **Advanced**, **Check for updates** asks by itself while it is on: a minute after the
start, then every six hours by the clock (once a day with **New versions** off). The setup's
**Look for new versions on GitHub**, on its **New versions** step, is the same switch, on unless you
switch it off there; without the setup, as in reel light, it is off until you turn it on. When a
newer reel is out, **Install reel** and the release's notes appear there and in the tray menu. The
app takes the installer of its own package from the release (reel, reel light or the AppImage),
checks its signature, and puts it in place; your settings and your library stay. On Windows, reel
closes, its installer shows its progress in a small window, and reel opens again by itself
([Updates](../desktop-app.md#updates) has every step).

A standalone app of 1.11 or earlier cannot install an update itself: install the new version by
hand once, from the releases page (**Open the releases** opens it), the same way as the first.
The `.deb` never updates itself.

## Add a server later

A server of your own adds share links that play in the chat
([The server](server.md) sets one up).

1. On the server, open Settings, Devices and press **Pair a device**. A code appears, good for ten
   minutes and one use.
2. In the app, open Settings, **Server**. Under **A server, later**, enter the **Server
   URL** (`https://clips.example.com`).
3. Under **Pairing**, enter the **Pairing code** and press **Pair**. When it worked, **Pairing**
   says "paired".
4. To send clips you already have, open Settings, **Status**: each of the last clips has
   **Upload to the server**. An older one you can download here and upload on the server's
   **Upload** page.
5. To have new clips go to the server, set **Mode** to **connected**. The window now shows the
   server's library, and **Save and share** puts a link on your clipboard.

Clips you did not send stay in the library on this PC. Set **Mode** back to **standalone** to see
them again.
