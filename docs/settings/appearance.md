# Appearance

The look of reel's pages, and what this browser remembers. **Theme** is kept on the server, the
same for every PC and browser; set in `config.toml` or the environment, it shows read only here,
with a padlock and where it came from. The group [This browser](#this-browser) is kept in each
browser on its own. In the app's standalone mode, **Theme** is a setting of the library on this
PC, and **This browser** is what the app's window remembers.

## Theme {#theme}

### Theme {#appearance-theme}

The pages' theme: **dark**, the only one there is. reel refuses any other value, here, in
`config.toml` or in the environment.

In `config.toml`: `theme` under `[appearance]`; environment: `REEL_THEME`; every value it takes:
[Configuration](../configuration.md#theme).

## This browser {#this-browser}

Kept in this browser only, never on the server: each browser, each private window and the app's
window has its own, and clearing the site's data in a browser puts them back to their defaults.
A change applies at once, and Settings says "Kept in this browser."

### Hover previews on cards {#hover-previews}

On (the default), a card in the library plays a short loop of its clip while the pointer rests
on it: up to four seconds from the poster, 480 pixels tall, without sound. Off, the card keeps
its poster and fetches no loop, which saves bandwidth on a laptop on a slow or metered
connection.

### Card size {#card-size}

How narrow a card in the library's grid may get, and so how many fit side by side: **small**
(about 220 pixels), **medium** (the default, about 280) or **large** (about 360). A wider window
fits more cards at any size. A collection's grid follows it too.

### Key hints {#key-hints}

On (the default), the pages show the keys that work there: the lines of keys under the library
and under a clip's player, and the key beside the search field and beside the menu's items. Off
hides them all. On a phone, a touch screen or a window under 600 pixels wide, the lines of keys
stay hidden either way.

### Accent {#accent}

reel's one pink, `#f282d8`, shown with a padlock: it is part of the design, not a setting, and
nothing changes it.
