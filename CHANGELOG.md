# Changelog

## [1.11.2](https://github.com/Locko2901/reel/compare/v1.11.1...v1.11.2) (2026-11-04)


### Bug Fixes

* **deps:** rustls 0.23.46, and zip 9 under the app's updater ([#14](https://github.com/Locko2901/reel/issues/14)) ([8b79941](https://github.com/Locko2901/reel/commit/8b7994107ddb3db91ea172889a8f44bb562b1833))

## 1.11.1 (2026-11-04)

### Bug Fixes

* an app paired again gets the clip the server already has, and its link, instead of "not found"
* an app is never handed another user's clip of the same file; the upload is refused
* the image's Status says the date it was built from
* a chosen poster stays when a cut or another version becomes current, as long as its frame is in that version
* a card's line on a phone shows whole items, the time first
* the changelog on Status shows code as code, and no marks

## 1.11.0 (2026-11-03)

### Features

* **desktop:** the Windows downloads are reel, with OBS, and reel light, for a PC with OBS already; either one updates or replaces the other, and its installer says Switch to reel or Switch to reel light

### Bug Fixes

* **desktop:** a recording stopped in OBS is taken as a clip, and a scene item turned on or off reaches the app (OBS 5's own events)
* **desktop:** Save a clip's hotkey can be changed
* **desktop:** reel's notifications make no sound of their own; reel's cues are the sound
* **desktop:** the app on Linux leaves out what only Windows does, and offers VAAPI
* **desktop:** a new OBS from the weekly bump is released by itself
* a link whose preview is still being made says so, not that it was retired
* `c` and Send to an app take the clip's newest live link
* Make a montage takes the picked clips in the order picked
* the live updates ask a proxy not to buffer them
* the standalone Storage line says where the library is
* the server's version and the newest release show in the header's menu in a browser too
* THIRD_PARTY.md names the OBS the pin names

## 1.10.0 (2026-11-03)

### Features

* **desktop:** the Windows installers in a light window: one page with Install, the progress, and Open reel at the end
* **desktop:** an update you can see: the app says what happens before it does, the installer shows a small window, and reel opens again by itself where you left it
* **desktop:** an update that does not install starts the reel you had again and says why

### Changes

* the server image's clock is UTC unless you set `TZ`

## 1.9.1 (2026-11-02)

### Bug Fixes

* standalone has nothing of a server's, and is a new install's default

## 1.9.0 (2026-11-02)

### Features

* Download in the Select bar, the picked clips as one zip
* keyframe snapping in the montage editor's trimmer
* the animated pictures made of a clip, under Send

### Bug Fixes

* Make and download never downloads when the job's event is missed

## 1.8.0 (2026-11-01)

### Features

* a cut fades in from black and out to it, picture and sound
* a cut framed tall (9:16) or square at a window of the picture you place, for a phone's short
* montages with crossfades or fades through black, in a tall or square frame, filled or with bars, at one loudness or each clip's own
* the montage editor: each clip's part and volume, a preview of the whole montage, and a montage opened again to change or replace it

### Bug Fixes

* **web:** Render the montage opens the new clip, also in the app (no reload of the montage page)

## 1.7.0 (2026-11-01)

### Features

* a clip or a part of it as a GIF or WebP
* a montage of several clips in one new clip
* **desktop:** Copy link and Open on the "Clip uploaded" notification
* favourites: star clips and filter the library to the starred ones
* select several clips and delete, restore, star, copy links or add them to a collection at once
* the Stats page: clips by game and by month
* upload an image as a clip's poster

### Bug Fixes

* a way back from the clip page
* limits from the round's security review (poster image size, picks, montage length, the Stats months)
* the clip page's back button names the Stats page
* the library cap acts on a server's clips, spares starred and collected ones, and waits after the update until an admin saves it

## 1.6.0 (2026-10-31)

### Features

* **desktop:** Open next to Copy link in the tray's last clips

## 1.5.0 (2026-10-30)

### Features

* **desktop:** a version pill, and an updater that says what it found

### Bug Fixes

* **desktop:** an update refreshes the copy of OBS for games that run as administrator by itself
* **desktop:** the app's own pages follow the server's stylesheet

## 1.4.2 (2026-10-30)

### Bug Fixes

* **desktop:** reel with OBS starts its OBS without its admin copy; plain reel asks before removing OBS

## 1.4.1 (2026-10-29)

### Bug Fixes

* **web:** one width for every page

## 1.4.0 (2026-10-29)

### Features

* **library:** a menu on each clip, and a page for the trash
* **storage:** the library on a storage of its own (REEL_LIBRARY_DIR)
* **web:** the pages of clips use the window's width

## 1.3.3 (2026-10-29)

### Bug Fixes

* **pipeline:** a clip moves to the trash and back across filesystems

## 1.3.2 (2026-10-27)

### Bug Fixes

* **desktop:** ship OBS 33 to the desktop's users

## 1.3.1 (2026-10-19)

### Bug Fixes

* **desktop:** no WebSocket to an OBS reel asked to close; a starting OBS is asked again
* **desktop:** the copy's OBS starts without its tray icon whatever wrote it before
* **desktop:** the lane takes waiting requests as one, decides in their turn, checks once idle

## 1.3.0 (2026-10-18)

### Features

* **desktop:** a panic, on any thread, written into the day's log before the app ends
* **desktop:** a switch for where WebView2 plays the window's sound
* **desktop:** OBS with the rights without its tray icon, opened and hidden through a second task
* **desktop:** quit at once, OBS stopped behind the window, each step timed in the log
* **desktop:** the window's sound plays from WebView2's browser process
* **web:** one set of keys for every player

### Bug Fixes

* **ci:** the installers' check reads each file list whole
* **desktop:** a test build never downloads or installs the release by itself
* **desktop:** OBS held stopped while a step in Program Files runs
* **desktop:** one step in Program Files at a time, the admin capture switch survives hammering
* **desktop:** Open OBS and Hide OBS through OBS's own minimize and restore
* **desktop:** Task Scheduler asked on a thread of its own, the window before OBS

## 1.2.0 (2026-10-15)

### Features

* **audio:** each source on a track of its own, named, and the editor's rows by those names
* **collections:** a game is a collection by itself, and a Collections tab
* **desktop:** a clip hears the sources chosen, each once in the full mix
* **desktop:** a download in the window says so in the page
* **desktop:** a volume per source before recording
* **desktop:** clips up to 30 minutes, with what a long one costs said
* **desktop:** Edit in OBS on every OBS setting, and what is changed in OBS read back
* **desktop:** hotkeys are recorded, not typed
* **desktop:** Inno Setup installers in place of NSIS
* **desktop:** keep the last seconds of each clip
* **desktop:** Linux, the AppImage and the .deb beside it
* **desktop:** noise suppression for the microphone, with OBS's own filters
* **desktop:** presses in a row copy the newest clip's link
* **desktop:** reel's own OBS on Linux, the installed OBS run a second time
* **desktop:** Save clip and Save and share as buttons, in the tray and the app's pages
* **desktop:** Save clip and Save and share on the server's pages too, at every width
* **desktop:** save the replay and share it with one key
* **desktop:** set it up myself in OBS, and Edit in OBS
* **desktop:** the bundled OBS runs with the app, hidden, and comes back after a crash
* **desktop:** the installers in reel's look, with as few pages as there can be
* **desktop:** the OBS for games that run as administrator runs from Program Files
* **desktop:** the setup of the OBS that came with the app
* **desktop:** the styled NSIS installers again, with the uninstaller's choices
* **desktop:** updates through the paired server, signed with the release key
* **device:** the paired app's updates come from its server
* **edit:** a volume for cuts, previewed live and rendered with the video copied
* **edit:** per-track audio, mixed into the one track a version plays
* **edit:** quick trims, and a replay published with its last seconds current
* Fold, reel's mark, everywhere at the sizes drawn for it
* **ingest:** normalise new clips, in the same cut as the last seconds
* **library:** game picks and collections with a page and a link
* **library:** the top is one toolbar line with a Filter
* **phone:** a tab bar at the bottom under 600 px
* **phone:** reel installs as a web app and takes videos shared to it
* **player:** the volume and the mute are kept per browser
* **settings:** link previews from external storage as settings under one switch, with a Test
* **share:** a link's page plays a sharper web copy
* **sharing:** a copy under Discord's 20 MB by default
* **sharing:** a link knows when it was posted and how often it was opened
* **sharing:** a link posted before an edit offers its link for this version
* **sharing:** an S3 client of reel's own for link previews in external storage
* **sharing:** link previews go to the external storage before a link is handed out
* **sharing:** previews made ahead, what the storage holds, and its alarms
* **sharing:** Shares keeps a row to one line at desktop widths
* **sharing:** the cleanup revokes links never shared, N days after they were made
* **style:** one on and off control, the switch, on every page
* **updates:** each install keeps its installer, and the server offers both
* **upload:** the browser uploads in pieces, resumable, under the session

### Bug Fixes

* **desktop:** pairing in the setup leaves you in the setup
* **assets:** a page never runs modules of another build
* **build:** a dev version whose short hash is all digits
* **cards:** each thing once, two collections to a row, no app in a title
* **ci:** the installer reaches the release through a pinned action, not gh on the runner
* **copies:** both passes of a copy under a size take the same frames
* **desktop:** a key that types nothing is a hotkey alone
* **desktop:** a Save and share upload goes at once, past the queue and the pause
* **desktop:** a silent install no longer starts reel
* **desktop:** a window closed to the tray lets its WebView go
* **desktop:** an update that brings another OBS installs it fresh
* **desktop:** Edit in OBS is a labeled link that opens OBS's dialog in front
* **desktop:** links into OBS only for its dialogs, in a slot; a bar while OBS is open
* **desktop:** Linux children leave the AppImage's environment; OBS stops on a SIGTERM
* **desktop:** no console window from any child process, and no schtasks at start
* **desktop:** no links into OBS's settings, one Open OBS with Hide OBS beside it
* **desktop:** no verbatim path reaches OBS, and an OBS that does not answer is said
* **desktop:** nothing of reel's runs when an installer touches its folder
* **desktop:** notifications on Linux from a thread of their own, no longer a panic
* **desktop:** OBS gets the rights for games run as administrator, never the app
* **desktop:** reel's OBS on Linux finds its muxer again, and a save that gives no replay says so
* **desktop:** reel's OBS runs under a name of its own, so Discord keeps out of Streamer Mode
* **desktop:** Show OBS and Edit in OBS show OBS's main window, and nothing else
* **desktop:** the AppImage carries GStreamer's GL plugin, so video goes to the screen without a copy
* **desktop:** the desktop entry the shortcuts portal knows reel by
* **desktop:** the elevated step is reel-elevate.exe, no PowerShell
* **desktop:** the elevated step takes no file of the user's
* **desktop:** the installers audited at 100, 125 and 150 %
* **desktop:** the Linux window plays clips, and never freezes on media
* **desktop:** the page watch's two methods are Linux's only, so Windows builds without a warning
* **desktop:** the save's sound as OBS takes the save
* **desktop:** the screen's restore token survives the setup written whole
* **desktop:** the setup lists the audio devices from Windows
* **desktop:** the uninstaller offers the clips folder only when it is a plain folder of its own
* **desktop:** Uninstall reel and This app, OBS's first read have their permissions
* **desktop:** what does not apply stays hidden when a section opens
* **editor:** with a cut current, the editor opens on that cut's range and sound
* **events:** a tab signed out backs off and says so
* **focus:** no ring on a pick when the Filter opens by a tap
* **games:** an app that is no game names no game
* **header:** the right group keeps its place on every page
* **library:** the Filter on an iPhone
* **pipeline:** a background ffmpeg's timeout counts only the time it ran
* **player:** one play button where the browser draws its own
* **server:** the storage line names kept originals as the setting does
* **settings:** a saved secret's field says it is set without a reload
* **settings:** a setting of several options is a row with a switch for each
* **settings:** the cleanup revokes links never shared, N days after they were made
* **sharing:** a link waiting for a preview copy raises it, and the web waits for the server's
* **sharing:** a preview deleted with its link stays at most a day in a CDN's cache
* **sharing:** a revoked or expired link's preview leaves the storage, even when made ahead
* **sharing:** an app's own browser and a phone named CUBOT count as people opening a link
* **sharing:** keep last makes no storage copy of the original
* **sharing:** og:video is reel's own address again, sending the chat app on to the storage
* **sharing:** one press makes one link, and the buttons say what they wait for
* **sharing:** the storage line names its one reason plainly, and says "for n" only among several
* **status:** the app update line names both installers
* **style:** one vertical rhythm on every page, measured by the style guard
* **style:** the responsive audit, every page on phones, tablets and Safari
* **style:** the switch takes a 44 px target on a phone
* the share pages show the site name, and words that were wrong
* titles made from a recording time say the pages' wall clock, the UTC ones made again once
* **web:** hover loops come round whole in WebKitGTK, starting again on their own end

### Performance

* **pipeline:** a lane for the jobs someone waits for
* **pipeline:** with keep last, the link's work first and the whole clip's after

## 1.1.0 (2026-10-09)

### Features

* **desktop:** one header, reel's pages carry the app's parts
* **desktop:** the app's settings are nine pages under a This app group in reel's Settings nav
* **desktop:** This app is a Settings section of reel's shape, one webview, Back across the switch
* **library:** the open library shows a new clip without a reload
* **polish:** titles from the recording time, posters that skip black frames, one name per thing
* **style:** jobs, shares, status, upload, the sign-in and the first run on the system
* **style:** the app's pages on the system, the after screenshots, the guard on every page
* **style:** the library, the clip page and the share page on the system
* **style:** the Settings page's action panels as rows of the grid, badges and danger buttons
* **style:** the visual system in one stylesheet, the header's status chip and avatar, the settings row grid

### Bug Fixes

* **ci:** the installer's out directory exists before Docker mounts it, and a manual way into the release job
* **desktop:** every This app entry opens its page from the app's own pages, and Back walks them
* **desktop:** the app's settings always reachable, its own page when the server is out of reach
* **pipeline:** the update test takes the next major as newer, not a fixed tag
* **polish:** the sweep reads a ShadowPlay name without its recorder tail
* **polish:** the title sweep keeps the game, names the year, and only renames what the name can date
* **style:** controls that sit on a line together share one height, and the guard measures it
* **style:** plain text never reacts to the pointer, and the guard checks hover and the pointer cursor
* **style:** the guard reads a token inline as the system's own, the disk line says originals

## 1.0.0 (2026-10-09)

### Features

* ingest, edits, share links with Discord embeds, file sending, the demo and the tests
* **accounts:** built-in accounts next to the proxy and single modes over one user table: sign-in with argon2id passwords and hashed session cookies, the first admin made at the sign-in page, members who see everything and edit what they uploaded, admin-only settings and users, sessions and sign-in attempts under Access, a lockout after five failures, cross-site changes refused
* **clip:** one Download row with a quality select
* **clip:** the editor previews the cut: with a trim selection the player starts at the in point, stops or loops at the out point, pulls seeks back inside, and a readout runs from zero to the cut length; a toggle shows the whole clip, clearing or rendering ends the preview
* **desktop-core:** the GUI-free core: settings, the token store, the device API client, the uploader, the watcher, the OBS client, the cache rules, the ledger, the cues and the log
* **desktop:** OBS's recording settings on the OBS page, the clips folder follows OBS
* **desktop:** one window on the library, the strip and the gear
* **desktop:** the window keeps its section nav at its own width, a --section argument opens a page
* **desktop:** standalone mode, reel inside the app on the loopback
* **desktop:** the settings window page, its script and styles on reel's tokens, the end to end driver and the README
* **desktop:** the shell: the runtime (found, upload, confirm, cache, the link), the tray, the hotkeys, the settings window's commands, autostart and the elevated task, the library window
* **desktop:** the Tauri 2 shell scaffold with the tray, the plugins and the icons
* every version plays, downloads and shares on its own; links follow or pin; cuts as new clips; one download on the share page; no drag handle
* **imports:** a folder scan under Imports
* **ingest:** every raw file is hashed before the probe, a file the library has is one clip, unfinished device uploads expire
* **jobs:** a Jobs tab with the queue, running jobs with progress and the route taken, failed jobs with their reason, Retry and Dismiss, the last 100 finished; Cancel kills the encode and leaves the raw file for the next poll; the tab carries a live count
* **jobs:** discard a failed ingest with its parked file and clip
* **library:** recording time from more file names, the container stamp, then the file time marked as such, shown apart from the arrival
* **library:** day dividers for date sorts and a disk line with refresh
* **library:** the search filters live in the browser
* **media:** downscales keep the best audio and the best video the size allows, audio first
* **media:** the embed copy takes a rung from 0.06 bits per pixel
* **notify:** notifications to an ntfy topic or a webhook
* **server:** the device API, bearer token only, scoped to the device's owner and its own clips
* **settings:** a Settings tab with every setting as a row: the catalog of keys with their environment names, the config built in layers (defaults, the table, config.toml, the environment) and swapped live, per-key saves with validation, history and undo, locked rows for what the environment sets
* **settings:** Devices next to Access, Pair a device with a ten-minute code, the list with Revoke
* **settings:** storage actions, the browser's own appearance choices, and Advanced
* **setup:** the first run, six steps over the settings, gated for admins
* share page player with loop, half speed, frame steps, timestamp links, keys, expiry notice, viewport stage with a flat poster tint
* **status:** a health dot in the header opens Status
* **status:** a Status page with the build (commit, date, compiler), the daily update check against the repository's releases with Check now, the changelog embedded at build time, and doctor's checks with a re-probe; the build script stamps the commit, the image takes it as a build argument
* **store:** devices, pairing codes, device uploads and the source hash of a clip (migration 0013)

### Bug Fixes

* **accounts:** bind identities by subject, sync the role, cap the pending session, keep the first admin's write in a transaction
* **assets:** every stylesheet and script URL carries the build's mark and a bare URL is never cached, so no proxy or browser serves an older build's stylesheet to a new page; a guard fails on any class a built page uses that the product stylesheet lacks; screenshots of every Settings section and Jobs view come from the running demo
* cover the native video overlay until playback starts
* **clip:** the player starts on a click or tap anywhere, the cover carries the play ring and never blocks the native controls once playback has begun
* **desktop:** a backlog clip carries no recording time of its own; the server reads it from the name
* **desktop:** build the library window off the main thread
* **desktop:** one tray icon, built in code, not a second one from the config
* **desktop:** the e2e driver names the container format, accepts the deleted state as confirmed; the watcher logs its folder
* **desktop:** the OBS test result under its row, in plain words
* **desktop:** the strip keeps 44 px on Linux; the library reloads on a local publish
* **imports:** the folder scan copies by default, skips what it took before, and moves only on request
* **ingest:** keep quality: remux H.264 up to 1440p and 50 Mbit/s with AAC copied, normalisation off by default, AAC 320 kbit/s or the source rate when re-encoding
* **library:** the disk line leads with what the clips take, summed from the library's versions; free space, caches and originals move into the tooltip
* **media:** the Windows constants above the derive, not between it and the struct
* **pipeline:** cancellation is cooperative and a state is written after its file work: a running job's ffmpeg watches the job's token and the job unwinds through its own cleanup before the worker marks it cancelled; a failing ingest parks the raw file before the clip reads failed
* **pipeline:** queue the embed copy before the publish; hold heavy jobs
* **player:** hovering the play ring no longer paints the player grey; the poster stays until playback starts
* publish a clip after its poster, and show the server's phases in the app
* **send:** Send to an app shares the link; Send the file is its own button that names the copy under the limit before making it, shows the progress and sends on the next tap
* **server:** the device middleware and the pair limiter run on their routes only, not the fallback; the log test reads the whole ring
* **settings:** the action panels pass the style guard and the filter
* **share:** keep the play overlay hidden while stepping frames or seeking by keys; it returns when paused by the play control or at the end
* **share:** the Discord preview size is 20 MB and the tags point at the copy whenever one exists
* **ui:** every page on a phone: 44 px controls on a coarse pointer or under 600 px, a wrapping top bar with the search on the library only, share cards under 960 px, 44 px trim handles, the disk detail on a tap, landscape players sized by the height; the hidden attribute wins over the button rule
* **ui:** hide the browser's spin buttons on number inputs
* **wording:** the file the link previews play is the preview video, not Discord's

### Performance

* full hardware route for re-encodes (GPU decode, vpp scale, GPU encode) with a per-clip fallback ladder to software decode and libx264; doctor probes hardware decode per codec
