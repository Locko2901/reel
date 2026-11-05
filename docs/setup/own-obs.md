# Your own OBS: reel light

For a Windows PC with OBS set up already, your scenes and all. **reel light** is reel without an
OBS of its own: it connects to yours, OBS keeps recording the way you set it up, and every replay
you save lands in your library a moment later. On Linux, see [Linux](linux.md).

What you need:

- Windows 10 or 11, 64 bit.
- OBS Studio 28 or later, which has the WebSocket server built in.
- Nothing else to start with. A reel server is optional: it adds share links
  ([Add a server for links](#add-a-server-for-links)).

## 1. Download and install

!!! note "Windows protected your PC"
    The installer is not code signed yet, so Windows SmartScreen warns about it. Click
    **More info**, then **Run anyway**.
    [Troubleshooting](../troubleshooting.md#windows-protected-your-pc) has the details.

1. Download reel light, `reel-light_<version>_x64-setup.exe` (about 70 MB), from
   [reel's releases](https://github.com/Locko2901/reel/releases), with its `.sha256` if you want to
   check the file. A reel server you run offers it under Settings, Devices, **reel light, for
   Windows**.
2. Run it. Its one page, **Install reel light**, says what it installs, that it is for you alone with no admin rights
   needed, and the folder, `%LOCALAPPDATA%\reel`. **Options** has **Add a shortcut to the
   desktop**, on unless you untick it. Press **Install**. Without Microsoft's WebView2 runtime,
   which Windows 10 and 11 usually have, the installer brings it along.

    ![The installer's page: reel light, for you, no admin rights needed](img/installer-reel-light-first.png)

3. At the end, press **Open reel**. The app, called reel in the Start menu and in Apps like the
   one with OBS, starts in the tray (the corner by the clock) and opens
   its window on Settings, **OBS**. **Close** leaves it for later: reel is in the Start
   menu.

A new install keeps the library on this PC ([Standalone](standalone.md)), so OBS is all there is
left to connect. An app that was set up before keeps what it had.

## 2. Turn on OBS's WebSocket

OBS tells the app over its WebSocket the moment it saves a replay.

1. In OBS, open **Tools**, **WebSocket Server Settings**.
2. Tick **Enable WebSocket server**. Leave **Server Port** at 4455 and **Enable Authentication** on.
3. Press **Show Connect Info**, copy the **Server Password**, and press **OK**.
4. In the app, under Settings, **OBS**, find the **obs-websocket** panel. Leave **Connect to OBS**
   on and **Host** `127.0.0.1`, **Port** `4455`, paste the **Password**, and press **Test**.
5. When it worked, Test says "OBS 32.x.x with obs-websocket 5.x.x" (your versions), and **What OBS
   is set to** fills in: the output mode, the recording folder, the format, the encoder and the
   replay buffer.

![Settings, OBS, connected to OBS](img/app-settings-obs.png)

The panel **What reel needs from OBS**, at the top of the page, keeps checking the WebSocket and
the replay buffer for as long as the app runs, and says in words what to do about either.

## 3. The replay buffer

reel saves clips from OBS's replay buffer: the last minute or so, kept in memory and written to a
file when you save.

1. In OBS, open **Settings**, **Output**. In the Simple output mode, under **Replay Buffer**, tick
   **Enable Replay Buffer** and set **Maximum Replay Time** (60 s is a good start). In the Advanced
   mode it is the **Replay Buffer** tab.
2. For the recording format, pick **Hybrid MP4** or **MKV**: both survive a crash of OBS. For the
   encoder, any **H.264** one works (NVENC, AMF, Quick Sync or x264). Clips in H.264 up to 1440p
   and 50 Mbit/s, with AAC audio (OBS's default), go into the library as they are, never encoded
   again.

With **Start the replay buffer** on (the default), the app turns OBS's replay buffer on and starts
it whenever it finds it off. It can also change the recording folder, the replay length, the
format and the bitrate for you: Settings, OBS, **Change in OBS**.

The clips folder follows OBS's recording folder unless you turn **Follow OBS** off under Settings,
**Clips folder** (**Clips folder and cache** when connected). The app watches that folder as a
safety net too, so a clip saved while OBS and the app were not talking is still found.

!!! tip "Discord's Streamer Mode"
    Discord turns on its Streamer Mode by itself while it sees OBS running, and with reel your OBS
    runs whenever you play. To keep Streamer Mode off: in Discord, **User Settings**, **Streamer
    Mode** (in newer versions under **Appearance**), turn off **Automatically Enable/Disable**.

## 4. Hotkeys

You can save a clip two ways, and use both:

- **OBS's own**: in OBS, **Settings**, **Hotkeys**, **Save Replay**. OBS saves, and the app takes
  the clip in.
- **The app's**: Settings, **Hotkeys**, **Save a clip** (no key until you set one). Click the field
  and press the keys. A combination another program holds is refused with a reason, and the field
  keeps what it had.

![Settings, Hotkeys](img/app-settings-hotkeys.png)

With a server, the page has three more keys: **Save and share**, **Share the last clip** and
**Upload the last clip** ([Hotkeys](../desktop-app.md#hotkeys)).

## 5. The first clip

1. Play a game and press your **Save a clip** key, or OBS's **Save Replay**. The "clip captured"
   sound plays.
2. A moment later the notification "Clip in the library" appears. Click **Open** to see the clip.
3. To send it, open **Send** on the clip's page, download the original or a copy under your chat's
   upload limit, and drag the file into the chat ([Standalone](standalone.md) has the details).

To open the window later, double-click the tray icon.

## Add a server for links

With a server of your own ([The server](server.md)), every clip goes up as you save it, and
**Save and share** puts its link on your clipboard.

1. On the server, open Settings, Devices and press **Pair a device**. A code appears, good for ten
   minutes and one use.

    ![Pair a device on the server](img/server-pair-a-device.png)

2. In the app, open Settings, **Server**. Under **A server, later**, enter the **Server URL**
   (`https://clips.example.com`) and a **Device name** for this PC (how it shows on the server).
3. Under **Pairing**, enter the **Pairing code** and press **Pair**. When it worked, **Pairing**
   says "paired", and **This device** names this PC, your user and your server.

    ![Paired](img/app-settings-server.png)

4. Set **Mode** to **connected**, so new clips go to the server. Clips already in the library on
   this PC can go up from Settings, **Status**, each with **Upload to the server**.

The window now shows your server's library; with reel's own accounts it asks you to sign in once,
inside the window. The device's token is kept in the Windows Credential Manager, never in a file.
Settings, Devices on the server lists the PC, and **Revoke** there signs it out.

Then make your first link:

1. In a game, press **Ctrl+Alt+Shift+S** (Save and share). The "clip captured" sound plays.
2. The app uploads the clip and waits until its link preview is ready, so the link plays where you
   paste it. Then the "link copied" sound plays.
3. Paste the link into a chat. In Discord, and in other apps that show a link's video, it plays
   right there.

## Games that run as administrator

Windows does not pass keys pressed in a game that runs as administrator to a program without
those rights, and that includes the app. For such a game, run OBS as administrator yourself (its
shortcut's Properties, Compatibility, **Run this program as an administrator**, or a scheduled task
at logon) and use OBS's own **Save Replay** key. OBS saves, and the app still takes the clip. The
app's own keys work again once that game is not in front.

Would you rather have reel run and set up its own OBS, with this handled for you? Install
[reel](all-in-one.md), the package with OBS, over this one: its page says **Switch to reel**, and
your settings, your library and the pairing stay.
