# Uploads

How the app sends clips up to the server it is paired with: the key that uploads the last clip,
whether uploads wait while you play, and the pace and the size of each upload. A clip goes up in
chunks, an upload cut off by a drop goes on where it stopped, and the server checks the whole file
against its hash; a file the server has already is not sent again. Whether each clip goes up by
itself is **Upload**, under [Essentials](essentials.md#upload-mode). Like every page under
**This PC**, this one is kept on this PC: each PC has its own.

The page shows while the app is connected. In standalone it leaves the nav: the waits show under
[Encoding](encoding.md#encoder) on Windows, and once a server is paired, **Bandwidth limit** and
**Chunk size** show under [Connection, Sending to it](connection.md#sending-to-it). Each entry below
says where it shows. [The desktop app](../desktop-app.md#the-apps-pages) has the whole picture.

## Uploads {#uploads}

The page's one group: the key, the waits, and the pace of an upload.

### Upload the last clip {#hotkeys-upload-last}

A key, anywhere, also in a game, that sends the newest clip waiting on this PC, or the newest one
whose upload failed. It goes with **Upload** set to **the last, on its key** under
[Essentials](essentials.md#upload-mode): each new clip then waits on this PC until you press it.
With **by hand** it sends the newest waiting clip, as **Upload now** on
[Status](status.md#queue) does. With **every clip** nothing waits, so the key only tries a failed
upload again; with nothing to send, a notification says "Nothing to upload".

Ctrl+Alt+U by default. To change it, click the field and press the keys: a combination that
Windows, the desktop or another app holds is refused, and the field keeps what it had. It is the
same key as **Upload the last clip** under [Hotkeys](hotkeys.md#hotkeys-upload-last): a change in
either place changes both. On Linux, where the desktop may not pass a key to the app, bind one to
`reel-desktop --upload-last` in the desktop's own keyboard settings
([Hotkeys](../desktop-app.md#hotkeys)).

In standalone the row hides, and the key is not taken: pressed from a shortcut or the command
line, it is refused with a notification.

### Uploads wait while a game has the screen {#upload-wait-for-games}

Windows only; on Linux the row does not show. Off by default: each clip goes up as it is found,
also while you play. On, uploads wait while a game has the screen and go on after it lets go: for
a slow connection, or a PC a game keeps busy. The app looks at the program in front every two
seconds. Turning the switch off lets a waiting upload go at once.

A game, here, is what [Games and apps](games.md) calls a Game, and a program not sorted yet while
it has the whole screen. A program's own page there can turn the waits on or off for that program
alone, whatever this switch says
([Every game, its class, its own setting](../games-and-apps.md#every-game-its-class-its-own-setting)).

The upload of a **Save and share** never waits: it goes at once, ahead of the others, so its link
comes while you play ([Save and share](clips.md#hotkeys-save-share)).

In standalone the row shows under [Encoding](encoding.md#encoder), with other words, since the
library on this PC does heavy work of its own:

- **Encoding waits while a game has the screen**: a re-encode, the copies under a size, the hover
  loop and the filmstrip wait.
- With a server paired, **Uploads and encoding wait while a game has the screen**: that, and the
  uploads to the server.

[The waits](../games-and-apps.md#the-waits) has the table for every mode.

### Only on the internet, not on the LAN {#upload-pause-only-on-wan}

Windows only, and shows under the waits while they are on. Off by default. On, an upload to a
server on the local network goes on during a game: only an upload out to the internet waits.

The server counts as on the local network when its name leads to a private IPv4 address (10,
172.16 to 172.31, 192.168), a link-local or loopback one, or an IPv6 unique local or loopback one.
The app looks it up at most once a minute while a game has the screen. Leave the switch off when
even an upload on the LAN costs the game too much: it still takes disk and CPU time.

In standalone it shows under [Encoding](encoding.md#encoder) once a server is paired. There,
with the server on the local network, nothing waits while it applies: the library's own work goes
on during the game too.

### Bandwidth limit {#upload-bandwidth-kbit}

The most an upload sends, in kbit/s (1 kbit/s is 1000 bits a second), up to 10000000. 0, the
default, is no limit. The pace holds over the whole upload: after a pause it does not burst to
catch up. Set it below your connection's upload speed when an upload slows a game or a call on the
same line; 8000, for example, keeps an upload to about 1 MB a second.

It applies from the next upload that starts; an upload under way keeps the pace it began with. It
shows while the app is connected, and in standalone with a server paired under
[Connection, Sending to it](connection.md#sending-to-it), where it paces **Upload to the server**.

### Chunk size {#upload-chunk-mb}

The size of each piece an upload is sent in, in MB, at most 90. 0, the default, takes the server's
own **Chunk size** ([Devices](devices.md#device-chunk-mb) on the server, 32 by default). A size set
here wins over the server's, larger or smaller, for this PC alone.

- **Larger** is faster on the local network: fewer requests for the same file.
- **Smaller** loses less on a connection that drops often: after a drop the upload goes on from
  the last chunk the server took.
- **A proxy** in front of the server has to accept a request of a whole chunk. The MB here are
  MiB: 90 is 94,371,840 bytes, under Cloudflare's 100 MB a request
  ([Uploads fail behind a proxy](../troubleshooting.md#uploads-fail-behind-a-proxy)).

It applies from the next upload that starts. It shows while the app is connected, and in
standalone with a server paired under [Connection, Sending to it](connection.md#sending-to-it).
