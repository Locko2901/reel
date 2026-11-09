//! One snapshot of the app, for the tray, the app's pages and the notifications.
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct ObsLine {
    pub connected: bool,
    pub line: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct WatcherLine {
    pub dir: String,
    pub ok: bool,
    pub line: String,
    /// The folder came from OBS over the WebSocket, not from the setting.
    pub from_obs: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueItem {
    pub path: String,
    pub name: String,
    pub state: String,
    pub sent: u64,
    pub total: u64,
    pub error: Option<String>,
}

/// Standalone mode: the local reel, its hardware probe and its ffmpeg.
#[derive(Debug, Clone, Serialize, Default)]
pub struct LocalLine {
    pub url: String,
    pub library_dir: String,
    pub backend: String,
    pub ffmpeg_version: Option<String>,
    pub notes: Vec<String>,
    pub ffmpeg: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentItem {
    pub clip_id: Option<String>,
    /// Standalone: the clip's id in the library on this PC.
    pub local_clip_id: Option<String>,
    pub name: String,
    pub title: Option<String>,
    pub when: String,
    pub state: String,
    pub phase: Option<String>,
    pub share_url: Option<String>,
    pub path: Option<String>,
}

/// Why the server cannot be reached, and when the next try is.
#[derive(Debug, Clone, Serialize, Default)]
pub struct OfflineLine {
    pub title: String,
    pub words: String,
    pub code: String,
    pub retry_line: String,
    /// The server answered and its page stopped at an error: no advice about the connection.
    pub page_error: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Status {
    pub paired: bool,
    /// Set while the app shows its own page instead of the server's.
    pub offline: Option<OfflineLine>,
    /// connected or standalone.
    pub mode: String,
    /// Standalone: the local reel once it runs; the line says what it is doing otherwise.
    pub local: Option<LocalLine>,
    pub local_line: String,
    pub server_url: String,
    pub server_name: String,
    pub user: String,
    pub device_name: String,
    pub state: String,
    pub state_word: String,
    pub state_line: String,
    pub paused_by_user: bool,
    pub paused_fullscreen: bool,
    /// Pause recording: the replay buffer is stopped on purpose until Resume recording.
    pub recording_paused: bool,
    pub obs: ObsLine,
    /// The OBS that came with the app is there.
    pub bundled: bool,
    /// What the bundled OBS is doing, while the app runs it.
    pub obs_box: Option<crate::obsbox::BoxLine>,
    /// Why Capture games that run as administrator did not take.
    pub obs_task_problem: Option<String>,
    /// The copy in Program Files that OBS runs from for games run as administrator.
    pub obs_copy: Option<CopyLine>,
    /// The step in Program Files that runs now, and whether Windows' prompt is open.
    pub obs_step: Option<reel_desktop_core::lane::Doing>,
    /// The switch for games that run as administrator, as saved.
    pub obs_elevated: bool,
    /// Whether the bundled OBS offers NVIDIA's noise removal; None until it answered.
    pub nvidia_noise: Option<bool>,
    /// Which OBS reel runs as its own: the one that came with it, or on Linux the installed
    /// one run a second time; empty without one.
    pub obs_which: String,
    /// Linux: the bundled OBS is the installed one; the screen comes through PipeWire on
    /// Wayland, through X11 otherwise.
    pub obs_linux: bool,
    /// Each encoder's last test on this PC: Settings offers only those that recorded in OBS.
    pub obs_encoders: Vec<reel_desktop_core::encoders::Verdict>,
    pub wayland: bool,
    pub linux: bool,
    /// Linux, once reel's OBS answered: whether it has the PipeWire audio capture plugin
    /// (a program's audio alone) and obs-vkcapture (a game's picture).
    pub obs_app_audio: Option<bool>,
    pub obs_game_capture: Option<bool>,
    /// The paired server's upload limit in bytes, once it said.
    pub server_upload_max_bytes: Option<u64>,
    /// The bundled OBS's version and where its source is, for the GPL's naming.
    pub obs_version: String,
    pub obs_source: String,
    /// What OBS is set to, once read over the WebSocket; None while not connected.
    pub obs_state: Option<reel_desktop_core::obs::ObsState>,
    pub watcher: WatcherLine,
    pub queue: Vec<QueueItem>,
    pub recent: Vec<RecentItem>,
    pub hotkey_ok: bool,
    pub hotkey_line: String,
    /// Linux: how the hotkeys reach the app on this desktop; empty elsewhere.
    pub hotkeys_how: String,
    /// Linux: what to know when the desktop shows no tray; empty when it shows one.
    pub tray_line: String,
    /// What to switch off in Discord where it sees reel's OBS as OBS and turns Streamer Mode
    /// on; empty where reel's OBS runs under a name of its own.
    pub discord_note: String,
    /// Whether Save clip and Save and share can run now, and why not.
    pub save: Action,
    pub save_share: Action,
    /// An OBS of the user's own: its WebSocket and its replay buffer, each with what to do.
    pub obs_checks: Vec<CheckLine>,
    pub server_admin: bool,
    /// The server's Normalise new clips, with its target; None until the server said.
    pub normalize_new: Option<bool>,
    pub loudness_target: Option<i32>,
    pub update_line: String,
    pub update_version: Option<String>,
    /// An Install under way: it waits for what holds it, downloads, and reel closes.
    pub update_installing: bool,
    /// What the update brings: the notes of the versions it spans, from the server.
    pub update_notes: Option<String>,
    /// What installing the update does, said before it does (Settings, Startup).
    pub update_before: Option<String>,
    /// When the server was last asked, in milliseconds since 1970, for the page's clock.
    pub update_checked_at: Option<u64>,
    /// The version this one replaced at its first start after an update.
    pub updated_from: Option<String>,
    pub versions: Versions,
    pub update_mode: String,
    pub releases_url: String,
    /// Windows: the uninstaller the installer left beside the app, for About's Uninstall reel.
    pub uninstaller: bool,
    pub version: String,
    pub config_dir: String,
    pub data_dir: String,
    pub library: crate::library::LibraryLine,
    pub panel: Panel,
}

/// The status chip's popover by mode: its third row (Server or Library) and which buttons
/// are there.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Panel {
    pub place: String,
    /// The row's dot, word and line when the app has them; None takes the health the page has.
    pub place_state: Option<(String, String, String)>,
    pub pause: bool,
    pub save_share: bool,
    pub share_last: bool,
    pub shares: bool,
}

pub fn panel(st: &Status) -> Panel {
    if st.mode != "standalone" {
        return Panel {
            place: "Server".into(),
            place_state: None,
            pause: true,
            save_share: true,
            share_last: true,
            shares: true,
        };
    }
    let place_state = match &st.local {
        Some(_) => None,
        None => {
            let line = if st.local_line.is_empty() {
                "reel on this PC is not running".to_string()
            } else {
                st.local_line.clone()
            };
            let dot = if line.contains("did not start")
                || st.local_line.is_empty()
                || st.library.away.is_some()
            {
                "err"
            } else {
                "active"
            };
            Some((dot.into(), "on this PC".into(), line))
        }
    };
    Panel {
        place: "Library".into(),
        place_state,
        pause: st.paired,
        save_share: false,
        share_last: false,
        shares: false,
    }
}

/// Standalone's line about new versions while no update is offered: what reel on this PC's
/// read of the releases found.
pub fn release_line(
    running: bool,
    check_on: bool,
    latest: Option<&str>,
    error: Option<&str>,
    current: &str,
) -> String {
    if !running {
        return "reel on this PC is not running to read the releases: new versions are on the releases page".into();
    }
    match (latest, error) {
        (_, Some(e)) => format!("the releases could not be read: {e}"),
        (Some(v), None) if reel_core::versions::older(current, v) => {
            format!("reel {v} is out; Check now offers it to install")
        }
        (Some(v), None) => format!("up to date: {v} is the newest release"),
        (None, None) if check_on => {
            "the newest release is read a minute after reel on this PC starts, then once a day".into()
        }
        (None, None) => {
            "not asked yet: Check now asks GitHub for the newest release, and nothing asks by itself".into()
        }
    }
}

impl Status {
    /// The one line the tray tooltip and the top strip show.
    pub fn tooltip(&self) -> String {
        if self.state_line.is_empty() {
            format!("reel: {}", self.state_word)
        } else {
            format!("reel: {}, {}", self.state_word, self.state_line)
        }
    }
}

/// Discord's own words for its switch, in its Streamer Mode settings.
pub const DISCORD_NOTE: &str = "Discord turns on its Streamer Mode by itself while it sees OBS running, and this OBS runs as long as reel does. To keep Streamer Mode off: in Discord, User Settings, Streamer Mode (in newer versions under Appearance), turn off Automatically Enable/Disable.";

/// An action the tray and the app's pages offer: whether it can run now, and why not.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Action {
    pub ok: bool,
    pub why: String,
}

impl Action {
    fn from(why: Option<&str>) -> Action {
        Action {
            ok: why.is_none(),
            why: why.unwrap_or_default().to_string(),
        }
    }
}

/// Save clip needs recording on and OBS connected with its replay buffer running; Save and
/// share also needs a server, or standalone.
pub fn save_actions(
    recording_paused: bool,
    obs_set_up: bool,
    connected: bool,
    state: Option<&reel_desktop_core::obs::ObsState>,
    paired: bool,
    standalone: bool,
) -> (Action, Action) {
    let save = if recording_paused {
        Some(RECORDING_PAUSED)
    } else if !obs_set_up {
        Some("no OBS is set up for reel")
    } else if !connected {
        Some("OBS is not connected")
    } else {
        match state {
            None => Some("OBS has not said yet whether its replay buffer runs"),
            Some(st) if !st.replay_active => Some("OBS's replay buffer is not running"),
            Some(_) => None,
        }
    };
    let share = save.or(if !paired && !standalone {
        Some("the app is not paired with a server")
    } else {
        None
    });
    (Action::from(save), Action::from(share))
}

pub const RECORDING_PAUSED: &str = "recording is paused";

/// The app's state and its word while reel's own OBS is in trouble, by the OBS's state.
pub fn obs_trouble(box_state: &str) -> Option<(&'static str, &'static str)> {
    Some(match box_state {
        "gave_up" => ("error", "OBS stopped"),
        "restarting" => ("error", "OBS restarting"),
        "silent" => ("error", "OBS does not answer"),
        "failed" => ("error", "OBS did not start"),
        "waiting" => ("waiting", "OBS waits"),
        _ => return None,
    })
}

/// What the app does with OBS's replay buffer, once it knows its state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferStep {
    Start,
    Stop,
    Leave,
}

/// Paused, a buffer that runs is stopped and a stopped one is left alone. Otherwise a stopped
/// buffer is started when Start the replay buffer is on and OBS is not recording.
pub fn buffer_step(
    recording_paused: bool,
    start_replay: bool,
    st: &reel_desktop_core::obs::ObsState,
) -> BufferStep {
    match (recording_paused, st.replay_active) {
        (true, true) => BufferStep::Stop,
        (false, false) if start_replay && !st.recording => BufferStep::Start,
        _ => BufferStep::Leave,
    }
}

/// The copy of OBS in Program Files: there or not, and from which OBS.
#[derive(Debug, Clone, Serialize)]
pub struct CopyLine {
    pub there: bool,
    pub version: Option<String>,
    /// The OBS this version of reel brought.
    pub wanted: String,
    /// Made from another OBS than this version brought, or still running OBS as obs64.exe.
    pub stale: bool,
    /// Its task starts OBS in its own tray, which Open OBS does not reach.
    pub tray: bool,
    /// The task that starts OBS at sign-in is there.
    pub task: bool,
    /// Left with the switch off: its task would start OBS with the rights at sign-in.
    pub left: bool,
    pub named_before: bool,
    pub folder: String,
}

/// One thing the app checks, whether it is right, and what it is or what to do.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CheckLine {
    pub name: &'static str,
    pub ok: bool,
    pub line: String,
}

/// What an OBS of the user's own needs for reel: its WebSocket server and its replay buffer.
pub fn obs_checks(
    obs: &ObsLine,
    state: Option<&reel_desktop_core::obs::ObsState>,
    s: &reel_desktop_core::settings::ObsSettings,
) -> Vec<CheckLine> {
    let at = format!("{}:{}", s.host, s.port);
    let reason = obs.line.to_lowercase();
    let websocket = if obs.connected {
        format!("on, {}", obs.line)
    } else if reason.contains("refused the password") {
        "OBS refused the password: in OBS, Tools, WebSocket Server Settings, Show Connect Info has it; copy it into Password below".into()
    } else if reason.contains("needs a password") {
        "OBS asks for a password: in OBS, Tools, WebSocket Server Settings, Show Connect Info has it; copy it into Password below".into()
    } else if reason.starts_with("connecting") {
        format!("trying {at}")
    } else {
        format!(
            "nothing answers at {at}: start OBS; in OBS, Tools, WebSocket Server Settings, tick Enable WebSocket server and check that its port is {}",
            s.port
        )
    };
    let (replay_ok, replay) = match (obs.connected, state) {
        (true, Some(st)) if st.replay_active => (
            true,
            format!("running, it keeps the last {} s", st.replay_seconds),
        ),
        (true, Some(st)) if st.replay_enabled && s.start_replay => {
            (false, "on in OBS, not running yet: reel starts it".into())
        }
        (true, Some(st)) if st.replay_enabled => (
            false,
            "on in OBS, not running: turn on Start the replay buffer below, or press Start Replay Buffer in OBS".into(),
        ),
        (true, Some(_)) if s.start_replay => (
            false,
            "off in OBS: reel turns it on and starts it (not while OBS records)".into(),
        ),
        (true, Some(_)) => (
            false,
            "off in OBS: in OBS, Settings, Output, Replay Buffer, tick Enable Replay Buffer, then Start Replay Buffer; or turn on Start the replay buffer below and reel does both".into(),
        ),
        _ => (false, "known once OBS answers".into()),
    };
    vec![
        CheckLine {
            name: "OBS's WebSocket",
            ok: obs.connected,
            line: websocket,
        },
        CheckLine {
            name: "The replay buffer",
            ok: replay_ok,
            line: replay,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use reel_desktop_core::obs::ObsState;
    use reel_desktop_core::settings::ObsSettings;

    fn line(connected: bool, line: &str) -> ObsLine {
        ObsLine {
            connected,
            line: line.into(),
        }
    }

    #[test]
    fn an_own_obs_is_told_how_to_turn_on_what_reel_needs() {
        let mut s = ObsSettings {
            start_replay: false,
            ..ObsSettings::default()
        };
        let refused = "not connected: cannot reach OBS at ws://127.0.0.1:4455: Connection refused (os error 111)";
        let off = obs_checks(&line(false, refused), None, &s);
        assert!(!off[0].ok);
        assert!(off[0]
            .line
            .contains("Tools, WebSocket Server Settings, tick Enable WebSocket server"));
        assert_eq!(off[1].line, "known once OBS answers");
        let pw = obs_checks(
            &line(false, "not connected: OBS needs a password and none is set"),
            None,
            &s,
        );
        assert!(pw[0].line.contains("Show Connect Info"));
        let mut st = ObsState::default();
        let on = line(true, "OBS 32.0.1, websocket 5.6.2");
        let c = obs_checks(&on, Some(&st), &s);
        assert!(c[0].ok && c[0].line == "on, OBS 32.0.1, websocket 5.6.2");
        assert!(!c[1].ok);
        assert!(c[1]
            .line
            .contains("Settings, Output, Replay Buffer, tick Enable Replay Buffer"));
        s.start_replay = true;
        assert!(obs_checks(&on, Some(&st), &s)[1]
            .line
            .contains("reel turns it on"));
        st.replay_enabled = true;
        st.replay_active = true;
        st.replay_seconds = 60;
        let c = obs_checks(&on, Some(&st), &s);
        assert!(c[1].ok && c[1].line == "running, it keeps the last 60 s");
    }

    #[test]
    fn the_save_actions_run_only_while_the_replay_buffer_runs_and_say_why_not() {
        let mut st = ObsState::default();
        let why = |a: &Action| (a.ok, a.why.clone());
        let (save, share) = save_actions(false, false, false, None, true, false);
        assert_eq!(why(&save), (false, "no OBS is set up for reel".into()));
        assert_eq!(why(&share), why(&save));
        let (save, _) = save_actions(false, true, false, None, true, false);
        assert_eq!(why(&save), (false, "OBS is not connected".into()));
        let (save, _) = save_actions(false, true, true, None, true, false);
        assert!(!save.ok && save.why.contains("has not said yet"));
        let (save, share) = save_actions(false, true, true, Some(&st), true, false);
        assert_eq!(
            why(&save),
            (false, "OBS's replay buffer is not running".into())
        );
        assert_eq!(why(&share), why(&save));
        st.replay_active = true;
        let (save, share) = save_actions(false, true, true, Some(&st), true, false);
        assert_eq!(
            (why(&save), why(&share)),
            ((true, String::new()), (true, String::new()))
        );
        let (save, share) = save_actions(false, true, true, Some(&st), false, false);
        assert!(save.ok);
        assert_eq!(
            why(&share),
            (false, "the app is not paired with a server".into())
        );
        assert!(
            save_actions(false, true, true, Some(&st), false, true).1.ok,
            "standalone saves into its library"
        );
    }

    #[test]
    fn paused_recording_greys_both_saves_and_says_why_first() {
        let running = ObsState {
            replay_active: true,
            ..ObsState::default()
        };
        let (save, share) = save_actions(true, true, true, Some(&running), true, false);
        assert_eq!((save.ok, save.why.as_str()), (false, "recording is paused"));
        assert_eq!(share, save);
        let (save, _) = save_actions(true, false, false, None, false, true);
        assert_eq!(
            save.why, "recording is paused",
            "before what else is missing"
        );
    }

    #[test]
    fn an_obs_in_trouble_comes_first_in_the_state_line() {
        assert_eq!(obs_trouble("gave_up"), Some(("error", "OBS stopped")));
        assert_eq!(obs_trouble("restarting"), Some(("error", "OBS restarting")));
        assert_eq!(obs_trouble("waiting"), Some(("waiting", "OBS waits")));
        assert_eq!(obs_trouble("running"), None);
        assert_eq!(obs_trouble("starting"), None);
    }

    #[test]
    fn the_restart_check_leaves_a_paused_buffer_alone_and_stops_one_that_runs() {
        let stopped = ObsState::default();
        let running = ObsState {
            replay_active: true,
            ..ObsState::default()
        };
        let recording = ObsState {
            recording: true,
            ..ObsState::default()
        };
        assert_eq!(buffer_step(false, true, &stopped), BufferStep::Start);
        assert_eq!(buffer_step(true, true, &stopped), BufferStep::Leave);
        assert_eq!(
            buffer_step(true, true, &running),
            BufferStep::Stop,
            "a buffer that starts while paused, as the app's OBS starts its own, is stopped"
        );
        assert_eq!(buffer_step(true, false, &running), BufferStep::Stop);
        assert_eq!(buffer_step(false, true, &running), BufferStep::Leave);
        assert_eq!(buffer_step(false, false, &stopped), BufferStep::Leave);
        assert_eq!(buffer_step(false, true, &recording), BufferStep::Leave);
    }

    #[test]
    fn the_status_panel_has_a_servers_row_and_buttons_connected_and_the_librarys_standalone() {
        let connected = Status {
            mode: "connected".into(),
            ..Status::default()
        };
        let p = panel(&connected);
        assert_eq!(p.place, "Server");
        assert!(p.pause && p.save_share && p.share_last && p.shares);
        assert_eq!(p.place_state, None, "the server's health from its page");
        let mut here = Status {
            mode: "standalone".into(),
            local: Some(LocalLine {
                library_dir: r"C:\Users\owner\AppData\Local\reel\library".into(),
                ..LocalLine::default()
            }),
            ..Status::default()
        };
        let p = panel(&here);
        assert_eq!(p.place, "Library");
        assert!(!p.pause && !p.save_share && !p.share_last && !p.shares);
        assert_eq!(
            p.place_state, None,
            "reel on this PC runs: its health from its page"
        );
        here.paired = true;
        assert!(
            panel(&here).pause,
            "Upload to the server, once one is paired"
        );
        assert!(!panel(&here).share_last && !panel(&here).save_share);
        here.local = None;
        here.local_line = "starting reel on this PC".into();
        assert_eq!(
            panel(&here).place_state,
            Some((
                "active".into(),
                "on this PC".into(),
                "starting reel on this PC".into()
            ))
        );
        here.local_line = "reel did not start: the library is not there".into();
        assert_eq!(panel(&here).place_state.unwrap().0, "err");
        here.local_line = r"D:\reel library is not there: is its drive plugged in?".into();
        here.library.away = Some(here.local_line.clone());
        assert_eq!(
            panel(&here).place_state.unwrap().0,
            "err",
            "a library whose drive is away"
        );
        here.library.away = None;
        here.local_line = "the library is moving; reel on this PC waits".into();
        assert_eq!(panel(&here).place_state.unwrap().0, "active");
        here.local_line = String::new();
        assert_eq!(
            panel(&here).place_state.unwrap(),
            (
                "err".into(),
                "on this PC".into(),
                "reel on this PC is not running".into()
            )
        );
    }

    #[test]
    fn standalone_says_what_the_release_check_read_and_asks_no_server() {
        assert_eq!(
            release_line(true, true, Some("1.10.0"), None, "1.9.0"),
            "reel 1.10.0 is out; Check now offers it to install"
        );
        assert_eq!(
            release_line(true, true, Some("1.9.0"), None, "1.9.0"),
            "up to date: 1.9.0 is the newest release"
        );
        assert_eq!(
            release_line(true, true, Some("1.9.0"), Some("timed out"), "1.9.0"),
            "the releases could not be read: timed out",
            "a read that failed says so, not the read before it"
        );
        assert_eq!(
            release_line(true, true, None, Some("GitHub answered 503"), "1.9.0"),
            "the releases could not be read: GitHub answered 503"
        );
        assert!(release_line(true, true, None, None, "1.9.0").contains("once a day"));
        assert_eq!(
            release_line(true, false, None, None, "1.9.0"),
            "not asked yet: Check now asks GitHub for the newest release, and nothing asks by itself",
            "the default: nothing asked by itself"
        );
        assert!(release_line(false, true, None, None, "1.9.0").ends_with("on the releases page"));
        for line in [
            release_line(true, true, Some("1.10.0"), None, "1.9.0"),
            release_line(false, true, None, None, "1.9.0"),
        ] {
            assert!(
                !line.contains("server") && !line.contains("paired"),
                "{line}"
            );
        }
    }

    #[test]
    fn the_pill_says_one_number_while_the_two_match_and_a_colour_only_when_it_matters() {
        use super::standing;
        assert_eq!(
            standing("1.4.1", "1.4.1", None, None),
            ("alone", "1.4.1".into())
        );
        assert_eq!(
            standing("1.4.1", "1.4.1", Some("1.4.1"), None),
            ("same", "1.4.1".into())
        );
        assert_eq!(
            standing("1.4.1", "1.4.1", Some("1.4.0"), None),
            ("ahead", "app 1.4.1, server 1.4.0".into()),
            "a newer app than its server: the updater cannot help"
        );
        assert_eq!(
            standing("1.3.1", "1.3.1", Some("1.4.0"), Some("1.4.0")),
            ("update", "app 1.3.1, server 1.4.0".into())
        );
        assert_eq!(standing("1.3.1", "1.3.1", Some("1.4.0"), None).0, "behind");
        assert_eq!(
            standing(
                "1.4.1-dev.e58080a",
                "1.4.1 (dev.e58080a)",
                Some("1.4.1"),
                None
            ),
            ("behind", "app 1.4.1 (dev.e58080a), server 1.4.1".into()),
            "a test build is older than the release of its number"
        );
    }
}
/// The app's version beside its server's, for the pill in the header and its popover.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Versions {
    /// The app's version, a test build's build after it.
    pub app: String,
    /// reel or reel light on Windows (`reel_core::packages`), the AppImage or the .deb.
    pub kind: String,
    /// The OBS that came with the app; None without one.
    pub obs: Option<String>,
    /// The paired server's version; None before it said, and standalone.
    pub server: Option<String>,
    /// The newest release, as the server last read it.
    pub newest: Option<String>,
    /// alone (no server), same, update (one is ready), ahead (the app is newer than its server,
    /// which offers only its own version), behind (the server is newer, nothing offered yet).
    pub standing: String,
    /// What the pill says: one number while the two match, both while they differ.
    pub words: String,
}

/// Where the app stands against its server: `current` as the updater compares it (a test
/// build's whole version), `shown` as the app names itself.
pub fn standing(
    current: &str,
    shown: &str,
    server: Option<&str>,
    update: Option<&str>,
) -> (&'static str, String) {
    use reel_core::versions::older;
    let Some(server) = server.filter(|s| !s.is_empty()) else {
        return ("alone", shown.to_string());
    };
    let both = format!("app {shown}, server {server}");
    if update.is_some() {
        ("update", both)
    } else if older(server, current) {
        ("ahead", both)
    } else if older(current, server) {
        ("behind", both)
    } else {
        ("same", shown.to_string())
    }
}
