# Getting started

Start with the app on your gaming PC: in a few minutes you have a library of your clips right
there. When you want share links that play in chats, add a server of your own; that part takes
about an hour the first time, most of it the reverse proxy.

## 1. The app on your gaming PC

Pick the guide that fits your PC:

| Your PC | The guide |
|---|---|
| Windows, without OBS, or with an OBS you would rather leave alone | [All in one](setup/all-in-one.md): **reel** brings its own OBS and sets it up in a few steps |
| Windows, with OBS set up the way you like it | [Your own OBS](setup/own-obs.md): **reel light** connects to your OBS |
| Linux | [Linux](setup/linux.md): the AppImage or the .deb, with your installed OBS |

!!! note "Windows protected your PC"
    The installers are not code signed, so Windows SmartScreen may warn the first time. Choose
    **More info**, then **Run anyway**. [Troubleshooting](troubleshooting.md#windows-protected-your-pc)
    has the details.

A new install keeps the library on your PC: no server, no account. Play a game and press
**Save a clip** (Alt+F10 after reel's setup of its OBS). The clip shows up in the library a second or two
later, ready to cut, or to download as a copy under your chat's upload limit and drag into the
chat. [Standalone](setup/standalone.md) has the whole picture.

## 2. Your server, for links

The server is one container with three folders. On a Linux machine with Docker:

1. Make a folder for it, such as `/srv/reel`, and save this as `docker-compose.yml` in it:

    ```yaml
    --8<-- "deploy/docker-compose.yml"
    ```

2. Change `REEL_PUBLIC_URL` to the address your links will carry and `TZ` to your time zone.
   Without an Intel or AMD GPU, delete the two `devices` lines.
3. Start it and read the setup code from its log:

    ```
    docker compose up -d
    docker compose logs reel
    ```

4. Put a reverse proxy with HTTPS in front, for the address in `REEL_PUBLIC_URL`. Share links
   have to be reachable from the internet, for the people you send them to and for the chat
   apps that build their previews.
5. Open the address, make the first admin with the setup code, and walk through the six short
   steps of the first run.

[The server](setup/server.md) has each step in full: the proxy settings for nginx, Nginx Proxy
Manager and Caddy, signing in through Authentik or another identity provider, and what each
step should show.

## 3. Pair the app with your server

1. On the server, open Settings, Devices and press **Pair a device**. You get a code.
2. In the app, open Settings, **Server**, and under **A server, later** enter the server's
   address and the code, then press **Pair**.
3. Set **Mode** to **connected**, so new clips go to the server. Clips already in the PC's
   library can go up too, one by one, from Settings, Status.

## 4. The first link

1. Play a game, then press **Save and share** (Ctrl+Alt+Shift+S unless you changed it).
2. The "clip captured" sound plays, the upload shows in the tray, and a few seconds later the
   "link copied" sound.
3. Paste it into a chat. Discord, and other chat apps that show video previews, play the clip
   right there; anywhere else the link opens its page, which plays it in the browser.

## Other ways in

- [No app](setup/no-app.md): OBS or any recorder saving into a folder the server watches, or
  uploads in the browser.
- [The phone](setup/phone.md): reel on the home screen, and videos shared to it from the
  gallery.

From here: [Sharing and links](sharing.md) for what a link can do, [The editor](editor.md) for
cuts, sound and montages, and [The desktop app](desktop-app.md) for everything the app does. If
something does not happen, [Troubleshooting](troubleshooting.md) says where to look.
