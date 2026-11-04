# What reel uses of OBS, and what checks it

The list a new OBS is held against: every part of OBS reel relies on, and what checks it. A pull
request that moves the pin in `desktop/obs/pinned.json` runs every desktop job
(`desktop/ci/changes.sh`), and its description lists the hand tests at the end of this page.

"Linux" is `obs-e2e.sh` (`make
desktop-obs-e2e`, the pinned OBS's own Ubuntu package under Xvfb), "Wine" is
`check-obs-start.sh` (`make desktop-obs-start-check`, the OBS inside reel's installer, the
package with OBS), and "else" names the unit tests, the other checks, a test by hand, or why
nothing here can.

| What | Linux | Wine | Else |
|---|---|---|---|
| WebSocket: Hello, Identify with the password, `GetVersion` | connects with the password; the version logged | connects; the version is the pin's | the authentication string against the protocol's (`obs.rs`) |
| The 25 request types of the code | each in `availableRequests` | each in `availableRequests` | `obs-requests.sh --check` in `make desktop-check` |
| WebSocket settings: port 4466, a password, no alerts | `config.json` checked | the app connects with them | `bundled.rs` |
| `--multi`, its own settings folder | beside the user's OBS, whose files stay the same to the byte | | |
| `--portable`, `portable_mode.txt` | | the install's layout; the app finds its OBS | `check-installers.sh` |
| `--startreplaybuffer`, `StartReplayBuffer`, `RecRB` | the buffer fills; Simple mode has it turned on | `Replay Buffer Start` in OBS's log | |
| `--profile`, `--collection`, `--scene`, `--disable-updater`, `--disable-missing-files-check`, `--minimize-to-tray` | started with them, no window | started with them | `bundled.rs` (the arguments) |
| `--only-bundled-plugins` (the copy for games run as administrator) | | | `check-elevate.sh` (the arguments, a stand-in OBS); by hand |
| `global.ini` and `user.ini`: no wizard, no updater, no question on exit, the tray | no OBS window, wizard or dialog | | `bundled.rs`; `check-obs-window.sh` (the window hidden, shown, closed, the real OBS under Wine) |
| The profile: video, audio, the replay buffer's time, size and folder, the format, tracks and their names | written and read back; changed in OBS and kept; Simple mode stands | an update keeps it | `bundled.rs`, `readback.rs` |
| `SaveReplayBuffer`, `ReplayBufferSaved` | the Save hotkey saves, the path arrives, the clip is 15 to 25 s | | by hand, a game's clip |
| A setting changed in the app: the bundled OBS started again on its files; the user's own OBS told over the WebSocket (`SetProfileParameter`, `SetRecordDirectory`, `StopReplayBuffer`, `StartReplayBuffer`) | the bundled: `--format` and `--clip-length` reach the next replay, in Simple mode too | | the user's own: `obs.rs` (`apply` over a profile, the folder's fallback) |
| The user's own OBS (its WebSocket on 4455, its files never written) | | | `e2e.sh` and the other checks against the stand-in OBS; by hand |
| Formats: mkv, hybrid MP4 | both saved and read | | |
| x264 | the replays above | | |
| NVENC, AMF, Quick Sync, VAAPI | | | no GPU on a runner: by hand |
| The screen: `xshm_input`; PipeWire on Wayland; `monitor_capture` on Windows | X11: in the scene, frames recorded | | `check-appimage-arch.sh` (PipeWire's source written); Windows': by hand |
| Game capture (`game_capture`, `vkcapture-source`) and the game's name (`GetCurrentProgramScene`, `GetSceneItemList`, `GetInputSettings`) | | | no game here: `obs.rs` (the window's name), `e2e.sh` (the stand-in's game); by hand |
| Desktop audio and the microphone (PulseAudio, WASAPI), the device lists (`GetInputPropertiesListPropertyItems`) | a tone at its level in the full mix and on its track, the microphone silent; Desktop audio's devices listed, Microphone's list answered (the image has no microphone) | | WASAPI's: by hand |
| Application audio (Game audio, Discord) | | | no game or Discord here: `bundled.rs` (the kinds and the window); by hand |
| The tracks: mixers, `RecTracks`, the names | Full mix, Microphone, Desktop in the replay and on the server | | Game and Discord's: by hand |
| Volumes and mute (`SetInputVolume`, `GetInputVolume`, `GetInputMute`) | changed in OBS, read back, kept | | |
| The microphone's filters: RNNoise, Speex, the gate (`CreateSourceFilter`, `RemoveSourceFilter`, `GetSourceFilterList`) | from the scene file; then the app's own requests set Speex, none, RNNoise and the gate | | `bundled.rs` (`SetSourceFilterEnabled`, the order); NVIDIA's (an RTX card): by hand |
| The kinds probed (`GetSourceFilterKindList`, `GetInputKindList`) | the probe runs | | `runtime.rs` |
| A recording stopped (`RecordStateChanged` with its file) | `StartRecord` and `StopRecord` in OBS: the app takes the file | | `obs.rs` |
| A scene, an input or a scene item changed (the game read again) | | | `obs.rs` (the subscription) |
| The meters (`InputVolumeMeters`, while a page shows them) | | | `obs.rs` (the levels in dB); by hand, that they move |
| reel's hotkeys | Alt+F10 saves | | `check-standalone.sh` (a key changed); Windows': by hand |
| OBS's own Save hotkey, when OBS runs elevated | | | `bundled.rs` (the key written); by hand |
| Capture games that run as administrator: the copy, the task | | on without the copy: OBS runs without the rights, the app says why | `check-elevate.sh`; by hand |
| No safe mode question after a crash (`.sentinel`) | killed, started again, no question | the first start after an update ended to skip Wine's warning | by hand, after Quit and an update |
| How reel stops OBS: SIGTERM; WM_CLOSE, the task, `reel-stop` | SIGTERM, and an installer's quit stops it for good | | `check-obs-window.sh` (WM_CLOSE), `check-install-stop.sh` (a stand-in) |
| An update of reel, the package with OBS | | the newest older release, then this one: OBS's settings kept, the app and OBS back | |

## What to test on Windows for a bump

By hand, with reel (the package with OBS) from the pull
request's artifact: a game in fullscreen, saved with the hotkey (the clip, its game's name, its
audio on Game and Discord on its own track); the screen without a game, and the desktop audio
and the microphone from the setup's lists; the microphone's noise suppression (RNNoise, Speex,
and NVIDIA's on an RTX card) and the gate; each GPU encoder the setup offers on that PC, its
test and a saved clip; Capture games that run as administrator (the copy, the task, OBS's own
Save hotkey); OBS hidden, Show OBS and Hide OBS, and no question about safe mode after Quit, an
update or OBS ended.
