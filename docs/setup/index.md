# Setting up reel

reel starts with the app on your gaming PC: your clips go into a library on that PC, with nothing
else to run. When you want share links that play right in the chat, add a server of your own.

## Just the app

Pick the guide for your PC. Each one ends with your first clip in the library.

| Your PC | Guide |
|---|---|
| Windows, without OBS, or with an OBS you would rather leave alone | [All in one](all-in-one.md): **reel** brings its own OBS and sets it up in eight short steps |
| Windows, with OBS set up the way you like it | [Your own OBS](own-obs.md): **reel light** connects to your OBS |
| Linux | [Linux](linux.md): the AppImage or the `.deb`, with the OBS you installed |

!!! note "Windows protected your PC"
    The Windows installers are not code signed yet, so SmartScreen warns the first time you run
    one. Click **More info**, then **Run anyway**.
    [Troubleshooting](../troubleshooting.md#windows-protected-your-pc) has the details.

[Standalone](standalone.md) explains the library on your PC: where it lives, how you send a clip,
and how it treats the PC while you play.

## A server, for links

A server is optional. It is what turns a clip into a link that plays in Discord and other chats.

| You want | Guide |
|---|---|
| Share links, and the library on a machine that is always on | [The server](server.md): one container, a reverse proxy, the first run; then you pair the app with it |
| Clips from a PC without the app | [No app](no-app.md): OBS or another recorder saving into a folder the server watches, and uploads in the browser |
| reel on your phone | [The phone](phone.md): reel on the home screen, and on Android, videos shared to it from the gallery |

Every guide has its steps in order, each with what you should see when it worked. The screenshots
come from a demo with made-up clips.
