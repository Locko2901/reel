# Connection

Where the clips go: **Mode**, standalone or connected, the server the app sends to, and the
pairing with it. Like every page under **This PC**, this one is kept on this PC: each PC has its
own. The pairing's token is not among the settings: on Windows it lives in the Windows Credential
Manager, on Linux in a file only your user can read
([The desktop app's token](../privacy.md#the-desktop-apps-token)).
[Standalone or connected](../desktop-app.md#standalone-or-connected) tells the whole story, and
[Add a server later](../setup/standalone.md#add-a-server-later) goes from standalone to a server
step by step.

## Mode {#mode-group}

The group's line says how the app runs: "connected to a server", or in standalone "reel runs on
this PC". While reel on this PC is not up, it says why, such as "starting reel on this PC",
"the library is moving; reel on this PC waits" or "reel did not start" with the reason.

### Mode {#mode}

- **connected**: every clip goes up to the server under **Server URL**, once the app is paired
  with it, and the window shows that server's library.
- **standalone**: reel runs inside the app, the library is on this PC, and the window shows it.
  Share links need a server, so only the keys **Save a clip** and **Record a session** are taken.

A new install starts standalone. An app set up before keeps its mode through an update.

A change takes effect at once: to standalone, reel on this PC starts; to connected, it stops and
the window shows the server's library. Clips in the library on this PC stay there: set **Mode**
back to standalone to see them. A server can also be paired while the app stays standalone, to
send it clips by hand from [Status](status.md#last-clips).

## Server {#server}

In standalone the group is called **A server, later**, and its line says how to add one: pair the
app with it here, send clips to it from Status, then switch **Mode** to connected.

### Server URL {#server-url}

The address of your reel server, such as `https://clips.example.com`. It has to be https: plain
http is taken only for a server on this same PC (`http://localhost:...` or
`http://127.0.0.1:...`), so the token never crosses a network in clear. An address with a query or
a `#` part is refused, and a `/` at its end is dropped. Empty is no server.

**Pair** sends its code to this address. Changing the address keeps the pairing: for the same
server under a new address, that is all it takes. Another server does not know this device:
connected, the window's bar says **not paired**, with "the server does not know this device any
more; pair again", and **Pairing code** shows again. The error words and what they mean:
[Pairing](../troubleshooting.md#pairing).

### Device name {#device-name}

How this PC shows on the server, under Settings, Devices. It starts as the PC's own name, up to 40
characters, and an empty field pairs with that name too.

The app sends the name when you press **Pair**, and after pairing the field holds the name the
server keeps. Set it before you pair: a change afterwards stays on this PC, and the server's list
keeps the name it got. To give the PC another name there, **Unpair**, pair again with the new
name, and revoke the old entry on the server.

## Pairing {#pairing}

The group's line says "paired" or "not paired". Pairing gives the app a token of its own,
for this PC and your user on the server: it uploads with it, and asks the server for links and
new versions with it. [Pair the desktop app](../setup/server.md#6-pair-the-desktop-app) shows the
server's side.

### Pairing code {#pairing-code}

Shows while the app is not paired. On the server, open Settings, Devices and press **Pair a
device**: the code it shows works once, for ten minutes. Enter it here and press **Pair**.

Pair takes the **Server URL** and the **Device name** above as they stand. When it works, the app
keeps the token and the server's address, and says "Paired with" the server; when OBS still lacks
something reel needs, it opens [Capture](capture.md) next. A code that is refused:
[Pairing](../troubleshooting.md#pairing).

### This device {#this-device}

Shows once the app is paired, in place of **Pairing code**: this PC's name on the server, your
user there, and the server's name.

**Unpair** forgets the token on this PC; connected, the window's bar then says **not paired**,
and clips wait on this PC until it is paired again. The server keeps the device in its list until
you press **Revoke** there, under Settings, Devices: revoke it there too when the PC is gone for
good. The other way round, a device revoked on the server shows as not paired here at the app's
next start, or at its next upload, which fails and says so: make a new code and pair again.

## Sending to it {#sending-to-it}

Shows in standalone once a server is paired. It holds the two settings of the uploads to that
server, which are the clips you send from [Status](status.md#last-clips) with **Upload to the
server**. They are the same settings that show under **Uploads** while the app is connected, and
each is explained there:

- [Bandwidth limit](uploads.md#upload-bandwidth-kbit): the most an upload sends, in kbit/s; 0 is
  no limit.
- [Chunk size](uploads.md#upload-chunk-mb): the size of each piece of an upload; 0 takes the
  server's.
