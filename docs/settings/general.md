# General

**General** holds the name on reel's pages, the address its links carry, the address the server
listens on, the clock the dates follow, and the editor's quick trims. These settings are kept on
the server, the same for every PC and browser, and only an admin sees them. A setting given in
`config.toml` or the environment shows read only here, with where it came from
([Configuration](../configuration.md)). [The server](../setup/server.md) sets most of them up in
its first run.

In the desktop app's standalone library, General holds **Time zone** and **Quick trims** alone:
there they are the settings of the library on this PC. The name, the public address and the
listen address are a server's.

## Site {#site}

### Site name {#site-name}

The name reel goes by: on the sign-in page and the first run, in the footer of a link's page and
of a collection's page, in a link's preview in a chat as the site's name, and at the start of
every notification's title ("reel: a job failed"). A paired desktop app names the server by it.
It is 1 to 40 characters, and `reel` until you change it.

In `config.toml`: `site_name`; environment: `REEL_SITE_NAME`; every value it takes:
[Configuration](../configuration.md#site-name).

### Public URL {#public-url}

The address every link starts with: a share link is this, then `/s/` and its token
([What a link gives you](../sharing.md#what-a-link-is)). Chat apps fetch a link's preview from
it, so it has to be reachable from the internet, usually through your reverse proxy with
HTTPS, for the video to play in a chat. It starts with `http://` or `https://`; a slash at the
end is dropped.

reel also builds from it a collection's link, the links in its notifications, the server
address the **Pair a device** dialog shows, and the address an identity provider sends you back
to (`/oidc/callback`). The first run's **Check the public URL** asks it for `/healthz` and says
whether it answers. Links you already sent keep the address they were made with, so after a
change they work only while the old address still reaches this server.

The compose file sets it as `REEL_PUBLIC_URL`, which makes the row read only; change it there
([1. Start the container](../setup/server.md#1-start-the-container)). If a chat shows only the
link and no video, see [Troubleshooting](../troubleshooting.md#a-link-shows-no-video-in-a-chat).

In `config.toml`: `public_url`; environment: `REEL_PUBLIC_URL`; every value it takes:
[Configuration](../configuration.md#public-url).

### Listen address {#listen}

The address and port the server answers on, inside its container. reel reads it when the
process starts, so it is always read only here. The image sets `0.0.0.0:8080`: every address,
port 8080, which the compose file publishes on the host as port 8746 (`"8746:8080"`). Change it
only to run reel on another port or address, and change the compose file's `ports` line to
match.

No sign-in ([single](../setup/server.md#no-sign-in-single)) needs a loopback listen address
such as `127.0.0.1:8080`: on any other, reel refuses to start in that mode.

In `config.toml`: `listen`; environment: `REEL_LISTEN`; every value it takes:
[Configuration](../configuration.md#listen-address).

### Time zone {#timezone}

The clock the pages' dates follow, and the one reel uses for the titles it makes from a clip's
recording time. It takes an IANA name such as `Europe/Berlin` or `America/New_York`; a name reel
does not know is refused. Empty, the default, follows the host's clock: in the container that
is its `TZ`, `UTC` unless the compose file sets another. A change shows on the next page you
open.

`TZ` sets the container's clock, and this setting what the pages follow
([Other variables](../configuration.md#other-variables)). In the standalone library, empty
follows this PC's clock.

In `config.toml`: `timezone`; environment: `REEL_TIMEZONE`; every value it takes:
[Configuration](../configuration.md#time-zone).

## Clip editor {#clip-editor}

### Quick trims {#edit-quick-trims}

The editor's **keep the last** buttons, in seconds: `15, 30, 60` by default. Each button keeps
the end of the clip, that many seconds, as a new version and makes it current, in one click
([Keep the last seconds](../editor.md#keep-the-last)). A button longer than the clip is greyed
out.

Give 1 to 4 values, each from 5 to 600 seconds, separated by commas: `10, 20, 45`. They are
sorted, and a value given twice shows once. The desktop app's own **Keep the last**, for every
new clip it saves, is a setting of the app, not this one.

In `config.toml`: `quick_trims` under `[edit]`; environment: `REEL_QUICK_TRIMS`; every value it
takes: [Configuration](../configuration.md#quick-trims).
