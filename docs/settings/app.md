# App

The app itself: its tray icon, its new versions, and what it is. Whether it starts with your
session is **Start with Windows** (on Linux **Start at logon**), under
[Essentials](essentials.md#startup-autostart). Like every page under **This PC**, this one is kept
on this PC: each PC has its own. [What runs, and when](../desktop-app.md#what-runs-and-when) and
[Updates](../desktop-app.md#updates) tell the whole story.

## The tray {#the-tray}

### Tray icon {#tray-icon}

**in colour**, the default, shows reel's mark in colour on its dark square. **white** and
**black** show the mark alone in one colour, for a tray whose icons are all white or all black.
The icon changes at once.

### Start elevated at logon {#startup-elevated}

Windows only, and retired: the row shows only on a PC where the setting is on, so that you can
turn it off. While it is on, a scheduled task starts the app at logon with the highest rights, in
place of **Start with Windows**, so that its hotkeys reach a game that runs as administrator. But
the app shows the server's pages, and should not run with the highest rights.

Turn it off: Windows asks for administrator rights to remove the task, and from then on **Start with
Windows**, while it is on, starts the app at logon as on any other PC. For games that run as
administrator, turn on **Capture games that run as administrator** under
[Capture](capture.md#obs-setup-elevated) instead: with the OBS that came with reel, it gives the
highest rights to OBS alone. With your own OBS, see [Games that run as
administrator](../troubleshooting.md#games-that-run-as-administrator).

## Updates {#updates-group}

The group's line says where new versions come from. Connected, the paired server offers the app
of its own version, so updating the server brings the new app to every paired PC. In standalone,
the newest release comes from GitHub, asked for only by **Check now** or by **Look for new
versions on GitHub**. Either way the installer is signed with reel's release key, and the app
checks the signature before it runs anything. While a new version is ready, its release notes
show under the rows.

### New versions {#updates}

What the app does with a newer version it finds by itself:

- **ask, then install**, the default: a notification asks, with **Install now** and **Later**, and
  so does the top of the window's pages. **Later** asks again a day later, or at the next start.
- **install when the app quits**: it downloads the new version at once and installs it when you
  quit from the tray. reel then stays closed until you open it.
- **off**: nothing is offered by itself. **Check now** still offers a newer version.

Connected, the app asks its server 45 seconds after it starts, then every six hours, unless this
is off. In standalone, this is what **Look for new versions on GitHub** does with a release it
finds. After **Install**, reel closes, its installer shows its progress, and reel opens again by
itself. [Updating the desktop app](../updating.md#the-desktop-app) covers both cases.

### This version {#this-version}

Its line says where the update stands: a version ready to install, the version its last update
came from, or why none is offered. Connected, it also says when it last asked the server.

- **Check now** asks at once: the server while connected, GitHub's releases in standalone. It
  offers a newer version whatever **New versions** says.
- **Install reel** with the version's number shows while one is ready. Before you press it, a line
  says what happens next and about how long it takes. While a session recording runs, or the
  library moves, the install waits for it.
- **Open the releases** shows in standalone while nothing is ready to install: it opens reel's
  releases page on GitHub, for an install by hand.

The `.deb` never updates itself: install the next version's `.deb` the same way as the first.
When an update does not install, the reel you had starts again and says why
([An update does not install](../troubleshooting.md#an-update-does-not-install)).

### Look for new versions on GitHub {#look-for-new-versions-on-github}

Standalone only; connected, the app takes its updates from its server. On, the app asks GitHub
for the newest release of reel by itself: one request, with reel's version and nothing else, a
minute after reel on this PC starts, then every six hours by the clock, or once a day with **New
versions** off. Off, it asks only when you press **Check now**.

It is a setting of the library on this PC, kept in its database rather than with the app's
settings, so the switch is greyed out while reel on this PC is not running. A library on this PC
starts with it off; the setup's **New versions** step turns it on unless you switch it off there.
[What leaves your PC](../privacy.md#what-leaves-your-pc) and
[New versions](../setup/standalone.md#new-versions).

## Versions {#versions}

This app's version and kind (reel or reel light, the AppImage or the `.deb`), and with reel on
Windows the version of the OBS that came with it; connected, the server's version; the newest
release; and a line on the updates, such as the version its last update came from.

## About {#about}

The app's version, and with reel on Windows the OBS that came with it; the folders the app keeps its
settings and its data in (the log, the list of clips sent, the library on this PC); and the
licences: reel's, MIT, and for that OBS, OBS Studio's, GPL 2.0, with a link to its source.

On Windows, **Uninstall reel...** starts reel's uninstaller, where you choose what goes with it,
and quits. Windows' own Apps settings do the same.
[Uninstalling](../desktop-app.md#uninstalling) says what goes and what stays.
