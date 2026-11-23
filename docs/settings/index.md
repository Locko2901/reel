# Settings

Every setting of reel, page by page, as Settings shows them: what each one does, when to change
it and what it costs. Settings itself keeps to the names and the controls: the small question
mark beside a name shows what the setting does, and its **More in the docs** link lands on the
setting's entry here.

## Where Settings is

- **In the desktop app:** Settings in the window's top bar, or Ctrl+, anywhere in the window. It
  opens on **Essentials**. On a window 1600 pixels wide or more it opens beside the page you were
  on, along the window's left edge, and that page stays usable; its **×** or Esc closes it.
- **In a browser:** Settings in the server's top bar, for an admin. A browser shows the server's
  pages only.

## The pages

The nav on the left lists every page; under 960 pixels wide a list at the top takes its place, as
it does in the app's Settings beside a page on a window under 2200 pixels wide. In a browser
window 1600 pixels wide or more the nav sits at its left edge and a page's groups flow side by
side.

- **Essentials** and the pages under **This PC** are the desktop app's own. They are kept on this
  PC: each PC with the app has its own.
- The pages under **Library**, **Server** and **System** are the server's. They are kept on the
  server: the same for every PC and every browser. In the app's window they show beside the
  app's pages.
- In the app's standalone mode, where reel runs inside the app and the library is on this PC,
  the pages under **Library** and **System** are the settings of that library, kept on this PC.

In the app's window, a small tag beside a page's name says where it is kept: **this PC only** or
**every PC and browser**.

| Group | Page | What it holds |
|---|---|---|
| | [Essentials](essentials.md) | the replay buffer, the clip length, the quality, the key that saves a clip, the microphone, uploads, starting with Windows |
| This PC | [Clips](clips.md) | saving a clip, what each one keeps, its keys and sounds, what stays on this PC |
| | [Session recording](session.md) | recording a whole session, by hand or with a game, and its reminders |
| | [Capture](capture.md) | what OBS captures and how it encodes |
| | [Hotkeys](hotkeys.md) | every key of the app in one place |
| | [Games and apps](games.md) | which programs are games, and what each does apart from its class |
| | [Uploads](uploads.md) | how clips go up to the server |
| | [Connection](connection.md) | the server the app sends to, pairing, standalone or connected |
| | [App](app.md) | the tray icon, new versions, the versions in use, about the app |
| | [Status](status.md) | what the app is doing now, its queue and its log |
| | [Library folder](library-folder.md) | standalone: where the library is on this PC, a copy of it, its size |
| | [Encoding](encoding.md) | standalone: the encoder for the library's new versions and copies |
| Library | [Library and storage](storage.md) | the folders, the trash and the caches, the disk |
| | [Ingest](ingest.md) | how new clips are taken in, copied or re-encoded |
| | [Imports](imports.md) | bringing in a folder of old clips |
| Server | [Sharing](sharing.md) | links, their previews, copies under a size |
| | [Devices](devices.md) | the app's downloads and the paired PCs |
| | [Access](access.md) | users and sign-in |
| | [Notifications](notifications.md) | where reel tells about events, and which |
| System | [General](general.md) | the site's name and address, its clock, the editor's quick trims |
| | [Appearance](appearance.md) | the pages' theme, and this browser's own choices |
| | [Advanced](advanced.md) | logs, exports, the settings' history, the first run, a restart |

## A change saves as you make it

There is no Save button: a setting is saved the moment it changes, and Settings says so if a
value is refused. On the server's pages a setting that differs from its default shows
**changed, was** with the old value and a **Reset** button, and a page's name in the nav counts
its changed settings. [Settings history](advanced.md#settings-history) lists the last changes
with **Revert**; in the app's window it lists this PC's own changes too, tagged **this PC**.

A server setting given in `config.toml` or the environment shows read only, with a padlock and
a badge that says where it came from; it changes there, not under Settings
([Configuration](../configuration.md) lists every key and variable). **after a restart** marks a
setting that takes effect at the next start.

## The question mark

Hover the question mark beside a name, or reach it with Tab, to see what the setting does. A
click, or a tap on a phone, keeps it open until you click elsewhere or press Esc. A page's name
and some groups have one too, for what the whole page or group is about.

**Filter settings**, at the top right, finds a setting by its name or by the words of its
explanation: a setting found by those words alone shows them under its name while the filter is
on.
