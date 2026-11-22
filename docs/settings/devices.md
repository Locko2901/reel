# Devices

The desktop app as the server sees it: its installers, the PCs paired with this server, how
their clips travel up, and the app updates the server hands them. These settings are kept on the
server, the same for every PC and browser. A setting given in `config.toml` or the environment
shows read only here, with a padlock and where it came from. Pairing a PC step by step is in
[Pair the desktop app](../setup/server.md#6-pair-the-desktop-app), the app itself in
[The desktop app](../desktop-app.md), and what a paired app may do in
[The desktop app's token](../privacy.md#the-desktop-apps-token).

The app's standalone mode has no Devices page: reel on that PC is the app's own library and
pairs no devices, and the page's address answers "Not in standalone". To pair that app with a
server, see [Add a server later](../setup/standalone.md#add-a-server-later).

## Uploads from devices {#uploads-from-devices}

### Chunk size {#device-chunk-mb}

A paired app sends a clip in chunks of this size, one request each. After a dropped connection
it goes on from the last chunk the server has, so a drop costs one chunk at most, never the
whole clip. An upload in pieces from the browser takes the same size. Each upload takes the
size in force when it starts, or when it goes on after a drop.

From 1 to 90 MB, 32 by default. On the LAN a larger chunk is faster. Behind a proxy, each chunk
is one request, so the proxy has to let a request of that size through: 90 is the most, so a
chunk passes Cloudflare's limit of 100 MB per request
([Put a reverse proxy in front](../setup/server.md#2-put-a-reverse-proxy-in-front),
[Behind Cloudflare](../setup/server.md#behind-cloudflare),
[Uploads fail behind a proxy](../troubleshooting.md#uploads-fail-behind-a-proxy)).

In `config.toml`: `chunk_mb` under `[device]`; environment: `REEL_DEVICE_CHUNK_MB`; every value
it takes: [Configuration](../configuration.md#chunk-size).

### Unfinished uploads expire {#device-upload-expiry-hours}

An upload in pieces, from the app or the browser, that nothing has touched for this long is
removed with what it had received; reel looks every ten minutes. The next try starts that clip
again from the beginning. From 1 to 720 hours, 24 by default. Make it longer when a PC can be off
for a day or more in the middle of a large upload; the cost is the room a stalled upload keeps
in the data folder until it goes.

In `config.toml`: `upload_expiry_hours` under `[device]`; environment:
`REEL_DEVICE_UPLOAD_EXPIRY_HOURS`; every value it takes:
[Configuration](../configuration.md#unfinished-uploads-expire).

## App updates {#app-updates}

### Offer app updates {#device-app-updates}

On (the default), a paired app older than this server, on Windows or the Linux AppImage, is
offered the app of the server's own version, signed, from the `app-updates` folder in the data
folder. The app checks the signature before it installs anything. The `.deb` never updates
itself: install each new version's `.deb` from [The app](#the-app) the same way as the first.

While it is on, the server fetches its own version's installers and their signatures from the
release of the [Repository](advanced.md#advanced-update-repo) (Advanced) at start and once a day
while one is missing, and removes those of other versions. Off, paired apps are offered nothing
and the server fetches nothing: the rows under
[The app](#the-app) say "not on this server yet" unless you put the installers, each with its
`.sig` file, into `app-updates` yourself. The app's side: [Updates](../desktop-app.md#updates)
and [Paired with a server](../updating.md#paired-with-a-server).

In `config.toml`: `app_updates` under `[device]`; environment: `REEL_APP_UPDATES`; every value it
takes: [Configuration](../configuration.md#offer-app-updates).

### Release token {#device-updates-token}

For a private fork only: a GitHub token that can read the fork's releases, so the server can
fetch the app's installers from them. The public repository's releases need none. The token goes
to GitHub only with those requests. It is written, never shown again: the row says only whether
one is set. The exports of the database and of the settings leave it blank, so after a restore
from one, enter it again ([Restore a server](../backup.md#restore-a-server)).

In `config.toml`: `updates_token` under `[device]`; environment: `REEL_UPDATES_TOKEN`; every
value it takes: [Configuration](../configuration.md#release-token).

## The app {#the-app}

The installers of this server's own version, for the PC games are played on: the app uploads
what OBS saves, and **Save and share** copies a link with one key. Each row says who it is for,
with **Download** and its size:

| Row | For |
|---|---|
| **reel, for Windows** | everything in one install, OBS included and set up by the app: a PC without OBS, or one where reel should run its own |
| **reel light, for Windows** | the app alone, for a PC with OBS set up already: it connects to that OBS |
| **reel for Linux, the AppImage** | one file for any distribution, with OBS from the distribution or Flathub; it updates itself from this server |
| **reel for Debian and Ubuntu, the .deb** | the same app as a package, with OBS from the distribution or Flathub; it does not update itself |

A row says "not on this server yet" until the server has that installer. It fetches them from
the release at start and once a day while [Offer app updates](#device-app-updates) is on. Which
one to take, and what each needs: [Which download](../desktop-app.md#which-download).

## Paired devices {#paired-devices}

The desktop app on each PC paired with this server. A paired app uploads what OBS saves, makes
links to the clips it sent, and looks for a new version of itself. It signs in with a token of
its own, never with a password or a browser's session.

- **Pair a device** makes a pairing code and shows it with this server's address, its
  [Public URL](general.md#public-url). In the app, open Settings, **Connection**, enter the
  address and the code there, and pair. The code works once and for ten minutes; the dialog
  counts down the time left, and **Copy the code** copies it. **Done** closes it and lists the
  new device. The device belongs to the admin who made the code, and so do the clips it sends.
  When pairing fails: [Pairing](../troubleshooting.md#pairing).
- The list shows each device's name (the one the app gave it), whose it is, when it paired, its
  **Last seen** time, how many of its uploads reached the library, and whether it is paired or
  revoked.
- **Revoke**, after **Sure?**, signs that app out at once: its token stops working, and the app
  asks to be paired again. The row stays, marked revoked. Revoke a PC you no longer use, or one
  that is lost.

A device whose user is disabled under [Access](access.md#users) stops uploading until that user
is enabled again.
