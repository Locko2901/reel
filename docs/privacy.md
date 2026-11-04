# Privacy and security

Your clips stay on your own machines. Nobody without an account on your
server sees a clip until you share it, and reel sends no telemetry, no
analytics and no crash reports, and needs no account with anyone. This page says what anyone on the internet
can see, what stays behind the sign-in, how the sign-in and the desktop
app are protected, and everything that leaves your PC or your server.

## What is public

Only what you share. Two kinds of address work without an account:

- **A share link** (`https://clips.example.com/s/...`): the page of one
  clip, its video, its poster and, when the link allows it, the download.
  The page shows the clip's title, its description if it has one, its
  length, the height it plays at, the date it was recorded and, for a
  link that expires, when. It does not show who uploaded it, any other
  clip, or anything of the library.
- **A shared collection** (`https://clips.example.com/c/...`): the
  collection's name and its clips, each with its title, poster and
  length, each playable on a page like a share link's. A clip you add to
  the collection later shows there too; a clip you take out is gone from
  it.

Each link's address holds a random part of 128 bits (22 characters), made
from the operating system's randomness. It cannot be guessed, and links
are not listed anywhere. The pages carry `noindex`, so search engines that
respect it leave them out.

Each link counts how often a person opened its page, and keeps when a chat
app last fetched it to build the link's preview (which tells you the link
was posted somewhere). Both show on the Shares page. reel tells the two
apart by the request's browser name alone, and keeps no list of who
opened a link and no visitor addresses for it.

Three more paths answer without a sign-in, because the share pages and
the app need them:

- `/assets/`: reel's own stylesheet, fonts, icons and scripts, the same on
  every server, nothing of yours.
- `/healthz`: the health check. It answers with reel's version, how long
  it has run, whether the database and ffmpeg work, the hardware route,
  how many jobs wait and how many clips the library holds. Nothing about
  any clip.
- `/api/device/`: the desktop app's way in. Every request there has to
  carry a device token (below), and without one it is refused.

## End a link

- **Revoke** a link on the Shares page or on the clip's page. From that
  moment its page and its files answer "gone".
- A link made with an expiry (1 day, 7 days or 30 days; see
  [Expire](configuration.md#expire)) ends by itself.
- A clip in the trash, or deleted, takes every link to it down with it.
- A collection's link ends with **Revoke** on the collection's page, or
  when you make a new link for it, which replaces the old one.

!!! note "Copies a chat app already made"
    Once a link was posted, Discord and other apps that showed a preview
    may keep their own copy of the poster and the preview video. Revoking
    stops reel from serving the clip; it cannot reach into those copies.

## What stays behind the sign-in

Everything else: the library, every clip's page and versions, the
collections you have not shared, the Shares, Jobs and Stats pages,
Settings and Status. A request without a valid session goes to the
sign-in page, or gets a 401 in proxy mode.

Everyone with an account sees the whole library. A member uploads, and
edits and shares what they uploaded. An admin does everything. No clip is
private to one user.

## Who can sign in

You set the mode when the container starts (see
[Mode](configuration.md#mode)):

- **local** (the default): reel's own accounts. The first admin needs the
  setup code from the server's log, and admins add everyone else.
- **proxy**: a sign-in in front of reel (Authentik, Authelia,
  oauth2-proxy) names the user in a header. reel believes it only from
  the addresses in [Trusted proxies](configuration.md#trusted-proxies),
  and refuses to start in this mode without that list.
- **oidc**: an identity provider signs people in over OpenID Connect. A
  user is bound to the provider's issuer and subject, never to a display
  name. The client secret comes only from the environment
  (`REEL_OIDC_CLIENT_SECRET`) or a file it names
  (`REEL_OIDC_CLIENT_SECRET_FILE`), never from Settings or the database.
- **single**: no sign-in at all. reel allows it only on a loopback
  address and refuses to start with it on any other.

### Passwords and sessions

- A password has at least 12 characters, and reel stores only its
  argon2id hash.
- Sign-in tries are limited per address and per name: five, then one more
  every two minutes. Admins see the failed tries (the address, the name
  and when), and reel removes them after a week.
- The session cookie is `HttpOnly`, `SameSite=Lax`, and `Secure` when the
  public URL is https. The database keeps only a hash of each session's
  id, so a copy of the database signs nobody in.
- A session ends after [Idle timeout](configuration.md#idle-timeout)
  without use (30 days by default) and, however active, after
  [Session lifetime](configuration.md#session-lifetime) (90 days).
- **Sign out everywhere** (Settings, **Access**, with reel's own
  accounts) ends every session of your account. A changed password does
  the same, and so does `reel passwd` from the server's shell.
- reel refuses a change (an upload, a delete, a setting) that does not
  come from its own pages, so a page on another site cannot act in your
  name through your browser.

## The desktop app's token

When you pair the app with a server, it gets a device token of 256 bits.
An admin makes the pairing code under Settings, **Devices**. The code
works once and for ten minutes, the server keeps only its hash, and
pairing tries are limited per address.

- On Windows the token lives in the Windows Credential Manager, never in
  a file.
- On Linux it is a file named `token` in the app's settings folder
  (`~/.config/reel/`), readable by your user only.
- It never goes into the app's settings file or its log.
- The app takes a server address with https only, and plain http only for
  a server on the same PC, so the token never crosses the network
  unencrypted.

**Revoke** next to the device under Settings, **Devices** signs that app
out at once, and it asks to be paired again. The list shows each device's
name, whose it is, when it was last seen and how many clips it sent.

## What leaves your PC

A new install of the desktop app is standalone: the library lives on your
PC, and the app talks only to OBS on the same PC, over OBS's WebSocket on
`127.0.0.1`. Nothing leaves the PC, and no update check runs by itself.
**Check now** under This app, Startup asks GitHub's public releases for
the newest version when you press it, and Settings, **Advanced**, **Check
for updates** turns a daily check on.

When that check finds a newer reel, the app reads the signature of its
installer from the same release, and downloads the installer itself when
you press **Install** (or right away, when **New versions** is set to
install when the app quits). It checks the signature against the key
built into it before it installs anything. Each of these requests goes to
GitHub with reel's version in its user agent and nothing else: no token,
no account, nothing of yours.

Paired with a server, the app also talks to **your server**, at the
address you paired it with: the clips it uploads, the library its window
shows, the links it makes, and once a day the question whether a newer
app is there. The app's updates then come from your server, signed, and
the app checks the signature against the key built into it before it
installs anything.

Nothing else: no telemetry, no analytics, no crash reports, no account
with anyone.

On Windows, the app draws its window with Microsoft's WebView2, which
Windows keeps up to date itself. When Windows lacks it, the installer runs
Microsoft's WebView2 setup, which downloads it from Microsoft.

### The OBS that comes with reel

reel runs the OBS that comes with it as `reel-capture.exe`, so Discord
does not switch to Streamer Mode while it records in the background. The
files are OBS's own, unchanged, and its source ships with every release.
On Linux, reel runs its second OBS, a copy of the one you installed, as
`reel-capture` for the same reason.

With **reel**, the package with OBS, the installer adds a Windows Firewall rule that
blocks every inbound connection to that OBS (`reel-capture.exe`). Its
WebSocket listens on every network interface and cannot be told
otherwise; the rule keeps other machines away from it. The WebSocket uses
a port of its own (4466, not OBS's usual 4455) and a password the app
made, which is never shown. Uninstalling reel, or switching to reel
light, removes the rule along with that OBS.

On Linux, reel's own OBS is the one you installed, run a second time with
settings of its own in reel's folder. Its WebSocket has the same port of
its own and a password the app made. reel adds no firewall rule there
(that takes root): the WebSocket answers on the network as OBS's always
does, and only to that 43-character password. A firewall such as ufw or
firewalld can keep port 4466 to this PC.

With your own OBS, the app connects to the address and password you give
it (`127.0.0.1` and OBS's own password by default).

## What your server contacts

Only these, and only while they are set:

- **GitHub's releases**, once a day, to see whether a newer reel is out
  ([Check for updates](configuration.md#check-for-updates), on by
  default; [Repository](configuration.md#repository) names where). The
  request carries reel's version in its user agent and nothing else.
- **The desktop app's installers**, from the public release of the
  server's own version, while
  [Offer app updates](configuration.md#offer-app-updates) is on (by
  default): at start and once a day while one is missing, with no token
  and nothing of yours in the request. A
  [Release token](configuration.md#release-token) is sent only when you
  set one, for a private fork's releases.
- **Your notifications**, when [Send to](configuration.md#send-to) is an
  ntfy topic or a webhook: the event, the clip, the reason and a link.
  They go over https unless the address is on your own network.
- **Your identity provider**, in oidc mode.
- **Your external storage**, while [Link previews from external
  storage](storage.md) is on: the preview videos of your links, put
  there, listed and deleted with the key you gave it. Anyone with a
  preview's address can fetch it, just as anyone with the link can open
  the link. The address holds 128 random bits, and the preview leaves the
  storage when its link ends (a CDN in front may keep a copy for up to ten
  minutes). The share pages, the clips and everything else stay on your
  server.
- **Its own public URL**, once, when you press the first run's **Check the
  public URL**.

## Behind a reverse proxy

reel speaks plain HTTP, and the reverse proxy in front adds HTTPS. reel
reads the visitor's address from `X-Forwarded-For`, and in proxy mode the
user's name from `X-authentik-username`, only when the request comes from
an address in [Trusted proxies](configuration.md#trusted-proxies). Anyone
else is taken at their own address, and in proxy mode gets a 401, whatever
the headers say. List the proxy's exact address (as `10.0.0.2/32`), never
a whole network.

## What an admin can see

An admin sees what every member sees (the whole library), and also:

- Settings, with every value (secrets such as tokens show only that they
  are set, never their value);
- the users, their roles and when each was last seen;
- the paired devices, as above;
- the failed sign-in tries of the past week, with address and name;
- the server's log, and the exports of the database and the settings,
  both with the secrets blank.

An admin can also cancel or retry any job on the Jobs page; a member acts
only on the jobs of their own clips.

Whoever runs the server can of course read its folders: the clips, the
originals and the database are files on that machine.
