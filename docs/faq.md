# FAQ

Short answers to what people ask first, each with the page that says
more.

## The basics

### What is reel?

reel keeps your game clips and makes links to them that play right in
the chat. OBS records; the reel app on your gaming PC takes each replay
OBS saves and puts it in your library, where you can cut it, mix its
audio or make a montage. Add a server of your own and **Save and share**
puts a link to the clip on your clipboard with one key. See the
[home page](index.md) for the tour.

### Does it cost anything?

No. reel is free and open source: no account, no subscription, no paid
tier. What you run it on is yours: the gaming PC, and if you want share
links, a machine for the server and a domain that points to it.
[Link previews from external storage](storage.md) is optional; Cloudflare
R2's free tier covers what reel puts there with the default limit.

### Do I need a server?

No, but you need one for links. Out of the box the app runs reel by
itself, with the library on your PC ([Standalone](setup/standalone.md)):
to send a clip, download it, or a copy under a chat's upload limit, and
drag the file into the chat. A share link is a page on the internet, and
that takes a server the internet can reach
([The server](setup/server.md)). You can add one any time later, and the
clips you have can go up to it then.

### What does it record with?

OBS. reel has no recorder of its own: it takes each replay OBS saves.
On Windows, the **reel** installer brings OBS along, sets it up and
runs it hidden ([All in one](setup/all-in-one.md)); **reel light**
connects to the OBS you have set up already
([Your own OBS](setup/own-obs.md)). On Linux, reel uses the OBS you
installed from your distribution or Flathub ([Linux](setup/linux.md)).
A server also takes clips from any recorder that saves files, through
its incoming folder ([No app](setup/no-app.md)).

### Is my clip public?

No, not until you share it. Your library is seen only by the people
signed in to your reel; in standalone, only from your own PC. A share
link opens one clip's page to whoever has the link. Its address holds
128 random bits, so nobody can guess it, links are listed nowhere, and
the pages ask search engines to leave them out. A link can expire after
1, 7 or 30 days, and **Revoke** ends it at once. See
[Privacy and security](privacy.md).

### Is there a feed, likes or followers?

No. reel has no feed, no profiles, no likes, no comments and no
followers. It keeps your clips and makes links to them; whoever you send
a link to sees that clip, and nobody else finds it. See
[Sharing and links](sharing.md).

### Does reel put a watermark on my clips?

No. A clip that is already H.264 within the copy rules is only
repackaged, its picture and sound untouched; anything else is encoded
again, with nothing added. The share page shows the clip with a small
footer below it, never on the video.

### Where are my clips stored?

On your own machines, never in a cloud of reel's (there is none). Out of
the box they stay on your gaming PC, in the app's library:
`%LOCALAPPDATA%\reel\library` on Windows, `~/.local/share/reel/library`
on Linux. With a server, they live on the machine that runs it: a folder
of video files plus a SQLite database. See
[Privacy and security](privacy.md) and [Backing up](backup.md).

### What happens to my clips if I stop using reel?

They stay yours, as ordinary MP4 files in folders on your disk. To take
them out with their titles, pick them in the library (**Select**, then
**Ctrl+A** for every clip shown) and press **Download** in the bar: you
get one zip with a folder per clip, named by its title, holding its
videos and a `clip.json` with its details
([Several clips at once](sharing.md#several-clips-at-once)). Uninstalling
the app leaves your clips where they are unless you choose otherwise
([Uninstalling](uninstalling.md)). Share links work as long as your
server runs.

### Is reel open source?

Yes. reel's code is under the MIT licence, in the repository
[`Locko2901/reel`](https://github.com/Locko2901/reel) on GitHub, with
the installers on its releases and the server's image on GitHub's
container registry (`ghcr.io/locko2901/reel`). The OBS that
comes with **reel** for Windows is OBS Studio, unmodified, under its own
licence, the GNU GPL, with its licence text in the install folder and
OBS's source archive on the release that ships it.
[THIRD_PARTY.md](https://github.com/Locko2901/reel/blob/main/THIRD_PARTY.md)
lists everything reel ships from others, each with its licence and where
its source is. To build reel yourself, see
[Building from source](building.md).

## Setting it up

### What does the server need?

A Linux machine that stays on, with Docker, room for the clips, and a
domain with a reverse proxy and HTTPS in front, so the share pages can be
reached from the internet. reel idles at a few tens of MiB of memory.
The image is built for x86_64 (amd64) only. See
[The server](setup/server.md) and [How light it is](light.md).

### Does it work with my OBS?

Yes, with OBS Studio 28 or later, which has the WebSocket built in. Turn
on OBS's WebSocket server and the replay buffer, give the app the
password, and the app hears every replay OBS saves. See
[Your own OBS](setup/own-obs.md).

### Which graphics cards does it use?

On the gaming PC, OBS encodes with what your card has: on Windows NVIDIA
NVENC, AMD AMF or Intel Quick Sync, on Linux NVENC or VAAPI, or x264 on
the CPU. The setup of reel's own OBS tests each with a short encode and
picks the first that works. On the server, reel uses an Intel or AMD
chip given to the container (Intel Quick Sync, else VAAPI) for the
encodes it has to make, and the CPU without one. A clip OBS saved in
H.264 is mostly only repackaged, so a server without a GPU copes.

### Windows warns about the installer. Is it safe?

The installers are not code signed yet, so Windows SmartScreen may say
"Windows protected your PC" the first time you run one. Click **More
info**, then **Run anyway**. Each download on the release has a `.sha256` beside it,
so you can check you got the file the release published. See
[Troubleshooting](troubleshooting.md#windows-protected-your-pc).

### Does the app run on Linux?

Yes, as an AppImage and as a `.deb` for Debian and Ubuntu. Paired with a
server, the AppImage updates itself from it; the `.deb` does not. Both
need an x86_64 system with glibc 2.36 or later: Arch, Fedora, Debian 12,
Ubuntu 24.04 and later. reel runs the OBS you installed from your
distribution or Flathub a second time, with settings of its own, so your
own OBS and its settings stay as they are and both can run at once; or
it connects to your OBS. The AppImage carries what its window plays
video with (GStreamer); the `.deb` uses your system's. See
[Linux](setup/linux.md).

### And on a Mac?

There is no app for macOS. reel's pages work in Safari on a Mac like in
any browser, uploads included ([No app](setup/no-app.md)).

### Can I use it on my phone?

Yes, with a server. reel installs to the home screen from the browser
and opens full screen. On Android you can share a video to reel from the
gallery; on an iPhone you upload it from reel's upload page. See
[The phone](setup/phone.md).

### Does it work behind Cloudflare?

Yes. Cloudflare refuses any single request over 100 MB, so the desktop
app, the browser and the phone all send a clip in pieces of at most
90 MiB, and a clip of any size gets through. A dropped connection
resumes where it stopped.

### Can friends upload too?

Yes, with an account on your server. An admin adds users under
Settings, Access (or the sign-in in front of reel lets them in). A
member sees the whole library, uploads in the browser or from the phone,
and edits and shares what they uploaded; an admin does everything.
People who only watch need no account: a link is enough. Pairing a
desktop app takes a code an admin makes under Settings, Devices.

## Clips and links

### Does the video play in the chat?

Yes, in apps that show link previews with video, Discord among them. A
share link's page carries the tags (`og:video`) that tell such an app
where its preview video is, and the app plays it inline. Previewers give
up fetching after a few seconds, so that preview is a copy small enough
to arrive in time: 20 MB by default
([Preview video size from the server](configuration.md#preview-video-size-from-the-server)).
Anywhere else the link opens its page, which plays the clip in better
quality and can offer the full file for download. See
[Sharing and links](sharing.md).

### Do viewers hear reel when I share its window?

On Windows, yes: a capture of reel's window takes its sound, the clips'
included (OBS's Application Audio Capture does). In Discord, if viewers
still hear nothing, turn on **Use an experimental method to capture
audio from applications** (Settings, Voice & Video; some versions show
it only with Developer Mode on). If that does not help either, share
the whole screen with its sound (viewers then hear everything your PC
plays), or post the clip's link in the chat, where it plays for everyone
with its sound.

### Can I send the file itself instead of a link?

Yes. Under **Send**, a clip's page makes a copy under a size to download
and drop into a chat: 10, 20, 50, 100 or 500 MB by default, the upload
limits of apps such as Discord. A copy is made once and kept for the
next time, up to a set amount
([Copies under a size](configuration.md#copies-under-a-size)). **Send**
also makes an animated GIF or WebP of up to 15 seconds, for places that
show pictures but no video. See
[Send the file itself](sharing.md#send-the-file-itself).

### Can I cut a clip?

Yes, in the browser or the app's window. Every cut is a new version of
the clip, and the original stays as it was. You can also fade a cut in
and out, frame it tall or square for a phone, and join clips into a
montage. See [The editor](editor.md).

### Can I keep the raw files?

Yes. By default reel keeps the raw file of every clip it had to encode
again, in the originals folder; it can keep every one, or none
([Keep originals](configuration.md#keep-originals)).

## Running it

### How much does it cost my PC?

Little. Closed to the tray, reel's app on Windows is one process of
about 13 MB that uses no measurable CPU. The recording is OBS's, and its
cost follows OBS's settings. The numbers and how they were taken are on
[How light it is](light.md).

### How do I update reel?

On the server, pull the new image and start it again; the database
updates itself at start, and the Status page says when a newer release
is out. A paired app is then offered the server's version, signed and
checked before it installs. The Linux `.deb` and a standalone app do not
update this way: their new version is on the
[releases page](https://github.com/Locko2901/reel/releases). See
[Updating](updating.md).

### How do I back up my clips?

On a server, keep the data folder (the database and the library) and the
originals folder; copy the database with reel stopped, or with a
consistent copy while it runs. The app keeps its settings and a
standalone library in two folders of your user. See
[Backing up](backup.md).

### How do I uninstall it?

On Windows, from Windows' Settings, **Apps**, or the app's own
**Uninstall reel...**; you choose whether your settings and your clips
folder go too. On Linux, delete the AppImage or `sudo apt remove reel`.
On the server, `docker compose down`. See [Uninstalling](uninstalling.md).

### Something does not work. Where do I look?

The app's status chip and its Status page say what the app, OBS and the
server are doing; the server's Status page and `reel doctor` say the
same for the server. [Troubleshooting](troubleshooting.md) goes through
the logs and the common errors.
