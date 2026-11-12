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
    /// The state's dot: ok, warn, err, active, rec or neutral.
    pub state_tone: String,
    /// The state in a word or two, as the window's bar shows it.
    pub state_word: String,
    /// A number where one helps: "last 60 s", "64%", "3 waiting".
    pub state_detail: String,
    /// The state in a sentence, as the bar's menu opens with it.
    pub state_line: String,
    pub state_fix: Option<Fix>,
    /// The bar's menu's OBS row.
    pub obs_row: Row,
    pub keys: Keys,
    pub paused_by_user: bool,
    pub paused_fullscreen: bool,
    /// Pause clips: the replay buffer is stopped on purpose until Resume clips.
    pub recording_paused: bool,
    /// A session recording that runs or starts.
    pub session: Option<crate::session::Line>,
    /// Whether a session recording can start now (or stop, while one runs), and why not.
    pub record: Action,
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
    /// The update's question while the window is open: Install now and Later.
    pub update_ask: Option<UpdateAsk>,
    /// An Install under way: it waits for what holds it, downloads, and reel closes.
    pub update_installing: bool,
    /// What the update brings: the notes of the versions it spans, from the server.
    pub update_notes: Option<String>,
    /// What installing the update does, said before it does (Settings, Startup).
    pub update_before: Option<String>,
    /// Standalone: Settings, Advanced, Check for updates of the library on this PC, once it is up.
    pub update_check: Option<bool>,
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

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UpdateAsk {
    pub version: String,
    pub title: String,
    pub words: String,
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
/// `six_hourly`: the app reads the releases again once a read is six hours old (New versions
/// not off), else reel on this PC reads them once a day.
pub fn release_line(
    running: bool,
    check_on: bool,
    six_hourly: bool,
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
        (None, None) if check_on => format!(
            "the newest release is read a minute after reel on this PC starts, then {}",
            if six_hourly { "every six hours" } else { "once a day" }
        ),
        (None, None) => {
            "not asked yet: Check now asks GitHub for the newest release, and nothing asks by itself".into()
        }
    }
}

impl Status {
    /// The state as the window's bar says it, without its fix and hint: the tray's first line.
    pub fn state_short(&self) -> String {
        if self.state_detail.is_empty() {
            self.state_word.clone()
        } else {
            format!("{}, {}", self.state_word, self.state_detail)
        }
    }

    /// The tray's tooltip.
    pub fn tooltip(&self) -> String {
        format!("reel: {}", self.state_short())
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

pub const RECORDING_PAUSED: &str = "clips are paused";

/// The app's state and its word while reel's own OBS is in trouble, by the OBS's state.
pub fn obs_trouble(box_state: &str) -> Option<(&'static str, &'static str)> {
    Some(match box_state {
        "gave_up" => ("error", "OBS stopped"),
        "restarting" => ("error", "OBS restarting"),
        "silent" => ("error", "OBS not answering"),
        "failed" => ("error", "OBS did not start"),
        "waiting" => ("waiting", "OBS waiting"),
        _ => return None,
    })
}

/// A button after the state's words that ends the state or shows why; `act` names what the
/// page does with it, `settings:<section>` opening that section of the app's settings.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Fix {
    pub word: &'static str,
    pub act: &'static str,
}

pub const RESUME_CLIPS: Fix = Fix {
    word: "Resume",
    act: "resume_clips",
};
pub const RESUME_UPLOADS: Fix = Fix {
    word: "Resume",
    act: "resume_uploads",
};
pub const START_OBS: Fix = Fix {
    word: "Start OBS",
    act: "obs_again",
};
pub const SETTINGS_OBS: Fix = Fix {
    word: "Settings",
    act: "settings:obs",
};
pub const SETTINGS_ENCODER: Fix = Fix {
    word: "Settings",
    act: "settings:encoder",
};
pub const SETTINGS_LIBRARY: Fix = Fix {
    word: "Settings",
    act: "settings:library",
};
pub const PAIR: Fix = Fix {
    word: "Pair",
    act: "settings:server",
};
pub const LOG: Fix = Fix {
    word: "Log",
    act: "settings:log",
};
pub const RETRY: Fix = Fix {
    word: "Retry",
    act: "retry",
};

/// What the app does, as the window's bar, the tray and its tooltip say it: the kind of state,
/// its dot, a short word, a number where one helps, the sentence its menu opens with, and the
/// fix it offers.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Said {
    pub state: &'static str,
    /// The dot's colour: ok, warn, err, active, rec or neutral.
    pub tone: &'static str,
    pub word: String,
    pub detail: String,
    pub line: String,
    pub fix: Option<Fix>,
}

/// What the state chain reads, apart from the app, so each state can be tried on its own.
#[derive(Debug, Default)]
pub struct Facts<'a> {
    pub standalone: bool,
    pub paired: bool,
    /// reel's own OBS while the app runs one: its state and line.
    pub obs_box: Option<(&'a str, &'a str)>,
    /// reel's own OBS, or one of the user's, is set up for reel.
    pub obs_set_up: bool,
    /// The OBS set up is reel's own.
    pub bundled: bool,
    pub obs_connected: bool,
    /// What the connection to OBS says: its versions, or why it is not there.
    pub obs_line: &'a str,
    /// The OBS was asked since the last start and did not answer.
    pub obs_refused: bool,
    /// Once OBS said: whether its replay buffer runs, and the seconds it keeps.
    pub replay: Option<(bool, u32)>,
    pub session: Option<&'a crate::session::Line>,
    pub recording_paused: bool,
    pub offline: Option<&'a OfflineLine>,
    pub encoder_back: Option<&'a str>,
    pub pair_line: Option<&'a str>,
    /// A copy or a move of the library: its kind, its phase and how far, in percent.
    pub library_job: Option<(&'a str, &'a str, u64)>,
    pub library_away: Option<&'a str>,
    /// The clip that goes up, its bytes sent and its size.
    pub uploading: Option<(&'a str, u64, u64)>,
    /// A clip the server works on: its phase, how far where it says, and its title.
    pub processing: Option<(&'a str, Option<u64>, &'a str)>,
    pub preparing: Option<&'a str>,
    pub save_problem: Option<&'a str>,
    pub paused_by_user: bool,
    pub paused_fullscreen: bool,
    pub last_error: Option<&'a str>,
    /// Clips that wait to go up.
    pub waiting: usize,
    /// Save a clip's hotkey; empty when it has none.
    pub save_key: &'a str,
}

/// A row of the bar's menu: its dot, word and line.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Row {
    pub tone: String,
    pub word: String,
    pub line: String,
}

/// The hotkeys the bar names beside what they do; empty where none is set or where the mode
/// has none.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Keys {
    pub save: String,
    pub save_share: String,
    pub share_last: String,
    pub record: String,
}

/// The OBS row of the bar's menu.
pub fn obs_row(f: &Facts) -> Row {
    let row = |tone: &str, word: &str, line: String| Row {
        tone: tone.into(),
        word: word.into(),
        line,
    };
    if let Some((box_state, box_line)) = f.obs_box {
        if let Some((state, word)) = obs_trouble(box_state) {
            return row(
                if state == "waiting" { "warn" } else { "err" },
                word.trim_start_matches("OBS "),
                box_line.to_string(),
            );
        }
    }
    if !f.obs_set_up {
        return row("neutral", "off", "no OBS is set up for reel".into());
    }
    if !f.obs_connected {
        return if f.obs_refused && !f.bundled {
            row("warn", "not connected", f.obs_line.to_string())
        } else {
            row("neutral", "connecting", f.obs_line.to_string())
        };
    }
    let whose = if f.bundled {
        "the OBS that came with reel".to_string()
    } else {
        f.obs_line.to_string()
    };
    match f.replay {
        Some((true, secs)) => row(
            "ok",
            "connected",
            format!("{whose}, the replay buffer keeps {}", replay_words(secs)),
        ),
        Some((false, _)) => row(
            "warn",
            "connected",
            format!("{whose}, its replay buffer is not running"),
        ),
        None => row("ok", "connected", whose),
    }
}

/// "60 s", "90 s", "3 min": the length OBS's replay buffer keeps.
pub fn replay_words(secs: u32) -> String {
    if secs >= 120 && secs.is_multiple_of(60) {
        format!("{} min", secs / 60)
    } else {
        format!("{secs} s")
    }
}

/// A line of the app's own words as the start of a sentence, with its full stop.
fn sentence(s: &str) -> String {
    let s = s.trim().trim_end_matches('.');
    if s.is_empty() {
        return String::new();
    }
    format!("{}.", crate::tray::capital(s))
}

/// The first state that holds is the one said: what wants a hand first, then what runs, then
/// ready.
pub fn said(f: &Facts) -> Said {
    let say = |state, tone, word: &str, detail: String, line: String, fix| Said {
        state,
        tone,
        word: word.to_string(),
        detail,
        line,
        fix,
    };
    let none = String::new;
    if let Some((box_state, box_line)) = f.obs_box {
        if let Some((state, word)) = obs_trouble(box_state) {
            let tone = if state == "waiting" { "warn" } else { "err" };
            return match box_state {
                "gave_up" => say(
                    state,
                    tone,
                    word,
                    none(),
                    format!(
                        "{} Start OBS tries again; Settings, OBS says why.",
                        sentence(box_line)
                    ),
                    Some(START_OBS),
                ),
                "failed" => say(
                    state,
                    tone,
                    word,
                    none(),
                    sentence(box_line),
                    Some(SETTINGS_OBS),
                ),
                _ => say(state, tone, word, none(), sentence(box_line), None),
            };
        }
    }
    if let Some(s) = f.session {
        return if s.starting {
            say(
                "recording",
                "active",
                "starting recording",
                none(),
                format!("Starting a session recording, {}.", s.line),
                None,
            )
        } else {
            say(
                "recording",
                "rec",
                if s.stopping {
                    "stopping recording"
                } else {
                    "recording"
                },
                s.length.clone(),
                format!(
                    "Recording a session: {}. Stop ends it, and the recording goes into the library.",
                    s.line
                ),
                None,
            )
        };
    }
    if f.recording_paused {
        return say(
            "paused",
            "warn",
            "clips paused",
            none(),
            "Clips paused: OBS keeps no replay until you resume. A session recording still works."
                .into(),
            Some(RESUME_CLIPS),
        );
    }
    if f.obs_set_up && !f.obs_connected {
        return if f.obs_refused && !f.bundled {
            say(
                "obs_off",
                "warn",
                "OBS not connected",
                none(),
                "Your OBS is not running, or its WebSocket is off. reel connects once it answers."
                    .into(),
                Some(SETTINGS_OBS),
            )
        } else {
            say(
                "connecting",
                "neutral",
                "connecting to OBS",
                none(),
                "OBS has not answered yet.".into(),
                None,
            )
        };
    }
    if f.obs_set_up {
        match f.replay {
            None => {
                return say(
                    "connecting",
                    "neutral",
                    "connecting to OBS",
                    none(),
                    "OBS is connected and has not said yet whether its replay buffer runs.".into(),
                    None,
                )
            }
            Some((false, _)) => {
                return say(
                    "buffer_off",
                    "neutral",
                    "replay buffer off",
                    none(),
                    "OBS is connected, its replay buffer is not running: no clip can be saved."
                        .into(),
                    Some(SETTINGS_OBS),
                )
            }
            Some((true, _)) => {}
        }
    }
    if let Some(o) = f.offline {
        if o.page_error {
            return say(
                "error",
                "err",
                "page error",
                none(),
                if o.code.is_empty() {
                    sentence(&o.title)
                } else {
                    format!("{} {}", sentence(&o.title), sentence(&o.code))
                },
                Some(RETRY),
            );
        }
        return say(
            "error",
            "err",
            if f.standalone {
                "library not answering"
            } else {
                "server unreachable"
            },
            none(),
            format!("{} {}", sentence(&o.title), sentence(&o.retry_line)),
            Some(RETRY),
        );
    }
    if let Some(words) = f.encoder_back {
        return say(
            "error",
            "err",
            "encoder changed back",
            none(),
            sentence(words),
            Some(SETTINGS_ENCODER),
        );
    }
    if !f.paired && !f.standalone {
        return say(
            "unpaired",
            "err",
            "not paired",
            none(),
            sentence(
                f.pair_line
                    .unwrap_or("enter the server and a pairing code under Settings, Server"),
            ),
            Some(PAIR),
        );
    }
    if let Some((kind, phase, pct)) = f.library_job {
        let (word, what) = if kind == "copy" {
            ("copying library", "Copying the library")
        } else {
            ("moving library", "Moving the library")
        };
        return say(
            "working",
            "active",
            word,
            format!("{pct}%"),
            format!("{what}: {phase}, {pct}%."),
            None,
        );
    }
    if let Some(why) = f.library_away.filter(|_| f.standalone) {
        return say(
            "error",
            "err",
            "library away",
            none(),
            sentence(why),
            Some(SETTINGS_LIBRARY),
        );
    }
    if let Some((name, sent, total)) = f.uploading {
        let pct = (sent * 100).checked_div(total).unwrap_or(0);
        return say(
            "uploading",
            "active",
            "uploading",
            format!("{pct}%"),
            format!(
                "Uploading {name}, {pct}% of {}.",
                reel_desktop_core::session::size_words(total)
            ),
            None,
        );
    }
    if let Some((phase, pct, title)) = f.processing {
        let preview = phase.starts_with("making the preview");
        return say(
            "working",
            "active",
            if preview { "making preview" } else { phase },
            pct.map(|p| format!("{p}%")).unwrap_or_default(),
            format!(
                "{title}: {}.",
                if preview {
                    "the server makes its preview video"
                } else {
                    "the server takes it in"
                }
            ),
            None,
        );
    }
    if let Some(p) = f.preparing {
        return say(
            "preparing",
            "active",
            "preparing link",
            none(),
            format!("Preparing the link to {p}."),
            None,
        );
    }
    if let Some(p) = f.save_problem {
        return say(
            "error",
            "err",
            "no clip saved",
            none(),
            sentence(p),
            Some(LOG),
        );
    }
    if f.paused_by_user && (!f.standalone || f.paired) {
        return say(
            "paused",
            "warn",
            "uploads paused",
            if f.waiting > 0 {
                format!("{} waiting", f.waiting)
            } else {
                none()
            },
            match f.waiting {
                0 => "Uploads paused until you resume.".to_string(),
                1 => "Uploads paused: 1 clip waits until you resume.".to_string(),
                n => format!("Uploads paused: {n} clips wait until you resume."),
            },
            Some(RESUME_UPLOADS),
        );
    }
    if f.paused_fullscreen {
        return say(
            "paused",
            "warn",
            "uploads waiting",
            none(),
            if f.standalone {
                "A game has the screen: the library's heavy work waits.".to_string()
            } else {
                "A game has the screen: uploads wait until it lets go.".to_string()
            },
            None,
        );
    }
    if let Some(e) = f.last_error {
        return say("error", "err", "error", none(), sentence(e), Some(LOG));
    }
    let wait = match f.waiting {
        0 => String::new(),
        1 => " 1 clip waits to upload.".into(),
        n => format!(" {n} clips wait to upload."),
    };
    match f.replay.filter(|_| f.obs_set_up) {
        Some((_, secs)) => {
            let kept = replay_words(secs);
            let how = if f.save_key.is_empty() {
                "Save clip below saves them".to_string()
            } else {
                format!("{} or Save clip below saves them", f.save_key)
            };
            say(
                "idle",
                "ok",
                "ready",
                format!("last {kept}"),
                format!("OBS keeps the last {kept}; {how}.{wait}"),
                None,
            )
        }
        None => say(
            "idle",
            "ok",
            "ready",
            none(),
            format!("No OBS is set up for reel: it takes in what lands in the clips folder.{wait}"),
            None,
        ),
    }
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

/// One thing a test of OBS read; `ok` only on what reel needs, whether it is right.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Found {
    pub name: &'static str,
    pub line: String,
    pub ok: Option<bool>,
}

/// What a test of an OBS found, for the setup to show: its versions, its profile, the replay
/// buffer and what a replay is written as.
pub fn found(st: &reel_desktop_core::obs::ObsState) -> Vec<Found> {
    let line = |name, line: String| Found {
        name,
        line,
        ok: None,
    };
    let keeps = format!("keeps the last {} s", st.replay_seconds);
    let replay = match (st.replay_enabled, st.replay_active) {
        (_, true) => format!("on and running, it {keeps}"),
        (true, false) => format!("on in this profile, not running; it {keeps}"),
        (false, false) => "off in this profile".to_string(),
    };
    vec![
        line(
            "OBS",
            format!("{} with obs-websocket {}", st.obs_version, st.ws_version),
        ),
        line(
            "Profile",
            if st.profile.is_empty() {
                "not named".into()
            } else {
                st.profile.clone()
            },
        ),
        Found {
            name: "The replay buffer",
            line: replay,
            ok: Some(st.replay_active),
        },
        line(
            "Recording folder",
            if st.record_dir.is_empty() {
                "not set".into()
            } else {
                st.record_dir.clone()
            },
        ),
        line(
            "Recorded as",
            format!(
                "{}, {}, {}x{} at {} fps",
                st.format_name,
                st.encoder_name,
                st.output.0,
                st.output.1,
                (st.fps * 100.0).round() / 100.0
            ),
        ),
    ]
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
    fn a_test_says_what_it_found_in_the_users_obs() {
        let mut st = ObsState {
            obs_version: "32.0.1".into(),
            ws_version: "5.6.2".into(),
            profile: "Gaming".into(),
            record_dir: "C:\\Users\\sam\\Videos".into(),
            format_name: "hybrid MP4".into(),
            encoder_name: "NVIDIA NVENC H.264".into(),
            replay_enabled: true,
            replay_seconds: 120,
            replay_active: true,
            output: (1920, 1080),
            fps: 60.0,
            ..ObsState::default()
        };
        let lines = |st: &ObsState| {
            found(st)
                .into_iter()
                .map(|f| format!("{}: {}", f.name, f.line))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            lines(&st),
            [
                "OBS: 32.0.1 with obs-websocket 5.6.2",
                "Profile: Gaming",
                "The replay buffer: on and running, it keeps the last 120 s",
                "Recording folder: C:\\Users\\sam\\Videos",
                "Recorded as: hybrid MP4, NVIDIA NVENC H.264, 1920x1080 at 60 fps",
            ]
        );
        assert_eq!(found(&st)[2].ok, Some(true));
        assert!(found(&st).iter().filter(|f| f.ok.is_some()).count() == 1);
        st.replay_active = false;
        assert_eq!(
            lines(&st)[2],
            "The replay buffer: on in this profile, not running; it keeps the last 120 s"
        );
        assert_eq!(found(&st)[2].ok, Some(false));
        st.replay_enabled = false;
        st.profile.clear();
        st.record_dir.clear();
        st.fps = 29.97;
        let l = lines(&st);
        assert_eq!(l[1], "Profile: not named");
        assert_eq!(l[2], "The replay buffer: off in this profile");
        assert_eq!(l[3], "Recording folder: not set");
        assert!(l[4].ends_with("at 29.97 fps"), "{}", l[4]);
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
    fn paused_clips_grey_both_saves_and_say_why_first() {
        let running = ObsState {
            replay_active: true,
            ..ObsState::default()
        };
        let (save, share) = save_actions(true, true, true, Some(&running), true, false);
        assert_eq!((save.ok, save.why.as_str()), (false, "clips are paused"));
        assert_eq!(share, save);
        let (save, _) = save_actions(true, false, false, None, false, true);
        assert_eq!(save.why, "clips are paused", "before what else is missing");
    }

    #[test]
    fn an_obs_in_trouble_comes_first_in_the_state_line() {
        assert_eq!(obs_trouble("gave_up"), Some(("error", "OBS stopped")));
        assert_eq!(obs_trouble("restarting"), Some(("error", "OBS restarting")));
        assert_eq!(obs_trouble("waiting"), Some(("waiting", "OBS waiting")));
        assert_eq!(obs_trouble("running"), None);
        assert_eq!(obs_trouble("starting"), None);
    }

    /// A connected app on an OBS whose replay buffer keeps a minute, Save a clip on Alt+F10.
    fn ready(standalone: bool) -> Facts<'static> {
        Facts {
            standalone,
            paired: !standalone,
            obs_set_up: true,
            obs_connected: true,
            obs_line: "OBS 31.0.0, websocket 5.5.2",
            replay: Some((true, 60)),
            save_key: "Alt+F10",
            ..Facts::default()
        }
    }

    fn short(f: &Facts) -> (String, String, Option<&'static str>) {
        let s = said(f);
        (s.word, s.detail, s.fix.map(|f| f.act))
    }

    #[test]
    fn ready_says_what_obs_keeps_and_how_it_is_saved() {
        let s = said(&ready(false));
        assert_eq!((s.state, s.tone), ("idle", "ok"));
        assert_eq!((s.word.as_str(), s.detail.as_str()), ("ready", "last 60 s"));
        assert_eq!(
            s.line,
            "OBS keeps the last 60 s; Alt+F10 or Save clip below saves them."
        );
        assert_eq!(s.fix, None);
        let s = said(&Facts {
            replay: Some((true, 180)),
            save_key: "",
            waiting: 3,
            ..ready(true)
        });
        assert_eq!(s.detail, "last 3 min");
        assert_eq!(
            s.line,
            "OBS keeps the last 3 min; Save clip below saves them. 3 clips wait to upload."
        );
        assert_eq!(
            said(&Facts {
                replay: Some((true, 90)),
                ..ready(false)
            })
            .detail,
            "last 90 s"
        );
        let s = said(&Facts {
            obs_set_up: false,
            obs_connected: false,
            replay: None,
            ..ready(false)
        });
        assert_eq!(
            (s.word.as_str(), s.detail.as_str()),
            ("ready", ""),
            "no OBS, nothing kept"
        );
    }

    #[test]
    fn an_obs_of_ones_own_that_does_not_answer_is_not_connected_in_amber_with_settings() {
        let closed = Facts {
            obs_connected: false,
            obs_refused: true,
            obs_line: "nothing answers at 127.0.0.1:4455; is OBS running?",
            replay: None,
            ..ready(true)
        };
        let s = said(&closed);
        assert_eq!(
            (s.state, s.tone, s.word.as_str()),
            ("obs_off", "warn", "OBS not connected")
        );
        assert_eq!(s.fix, Some(SETTINGS_OBS));
        assert_eq!(
            s.line,
            "Your OBS is not running, or its WebSocket is off. reel connects once it answers."
        );
        assert_eq!(
            short(&Facts {
                obs_refused: false,
                ..closed
            })
            .0,
            "connecting to OBS",
            "before its first answer"
        );
        assert_eq!(
            short(&Facts {
                bundled: true,
                ..closed
            })
            .0,
            "connecting to OBS",
            "reel's own OBS has its own words when it is in trouble"
        );
        assert_eq!(
            short(&Facts {
                replay: Some((false, 60)),
                ..ready(false)
            }),
            (
                "replay buffer off".into(),
                String::new(),
                Some("settings:obs")
            )
        );
        assert_eq!(
            short(&Facts {
                replay: None,
                ..ready(false)
            })
            .0,
            "connecting to OBS"
        );
        let row = obs_row(&closed);
        assert_eq!(
            (row.tone.as_str(), row.word.as_str()),
            ("warn", "not connected")
        );
        assert_eq!(
            row.line,
            "nothing answers at 127.0.0.1:4455; is OBS running?"
        );
        let row = obs_row(&ready(false));
        assert_eq!(
            (row.tone.as_str(), row.line.as_str()),
            (
                "ok",
                "OBS 31.0.0, websocket 5.5.2, the replay buffer keeps 60 s"
            )
        );
        let row = obs_row(&Facts {
            bundled: true,
            ..ready(false)
        });
        assert_eq!(
            row.line,
            "the OBS that came with reel, the replay buffer keeps 60 s"
        );
    }

    #[test]
    fn the_first_state_that_holds_is_said_with_its_fix() {
        let gave_up = Facts {
            bundled: true,
            obs_box: Some(("gave_up", "OBS stopped 3 times in a minute")),
            obs_connected: false,
            ..ready(false)
        };
        let s = said(&gave_up);
        assert_eq!((s.word.as_str(), s.tone), ("OBS stopped", "err"));
        assert_eq!(s.fix, Some(START_OBS));
        assert_eq!(
            s.line,
            "OBS stopped 3 times in a minute. Start OBS tries again; Settings, OBS says why."
        );
        let rec = crate::session::Line {
            since_ms: 1,
            line: "12 min, started from the window".into(),
            length: "12 min".into(),
            ..Default::default()
        };
        assert_eq!(
            said(&Facts {
                session: Some(&rec),
                ..gave_up
            })
            .word,
            "OBS stopped",
            "OBS in trouble before the recording"
        );
        let s = said(&Facts {
            session: Some(&rec),
            recording_paused: true,
            ..ready(false)
        });
        assert_eq!(
            (s.word.as_str(), s.detail.as_str(), s.tone),
            ("recording", "12 min", "rec")
        );
        assert_eq!(
            s.line,
            "Recording a session: 12 min, started from the window. Stop ends it, and the recording goes into the library."
        );
        let starting = crate::session::Line {
            starting: true,
            line: "asked from the window".into(),
            ..Default::default()
        };
        assert_eq!(
            short(&Facts {
                session: Some(&starting),
                ..ready(false)
            })
            .0,
            "starting recording"
        );
        let stopping = crate::session::Line {
            stopping: true,
            ..rec.clone()
        };
        assert_eq!(
            short(&Facts {
                session: Some(&stopping),
                ..ready(false)
            })
            .0,
            "stopping recording"
        );
        assert_eq!(
            short(&Facts {
                recording_paused: true,
                obs_connected: false,
                obs_refused: true,
                ..ready(true)
            }),
            ("clips paused".into(), String::new(), Some("resume_clips")),
            "clips paused before OBS not connected"
        );
        let away = OfflineLine {
            title: "No answer from the server".into(),
            retry_line: "trying again in about 20 s (try 2)".into(),
            ..Default::default()
        };
        let s = said(&Facts {
            offline: Some(&away),
            ..ready(false)
        });
        assert_eq!(
            (s.word.as_str(), s.fix),
            ("server unreachable", Some(RETRY))
        );
        assert_eq!(
            s.line,
            "No answer from the server. Trying again in about 20 s (try 2)."
        );
        assert_eq!(
            short(&Facts {
                offline: Some(&away),
                ..ready(true)
            })
            .0,
            "library not answering"
        );
        let broken = OfflineLine {
            page_error: true,
            title: "The server answers, but its page did not load here".into(),
            code: "TypeError: x is undefined".into(),
            ..Default::default()
        };
        assert_eq!(
            short(&Facts {
                offline: Some(&broken),
                ..ready(false)
            })
            .0,
            "page error"
        );
        assert_eq!(
            short(&Facts {
                paired: false,
                ..ready(false)
            }),
            ("not paired".into(), String::new(), Some("settings:server"))
        );
        assert_eq!(
            short(&Facts {
                library_job: Some(("copy", "copying", 40)),
                ..ready(true)
            }),
            ("copying library".into(), "40%".into(), None)
        );
        assert_eq!(
            short(&Facts {
                library_away: Some("the drive E: is not there"),
                ..ready(true)
            }),
            (
                "library away".into(),
                String::new(),
                Some("settings:library")
            )
        );
        let s = said(&Facts {
            uploading: Some((
                "Ridgeback Rally 2026.11.12 - 21.02.44.mp4",
                136_000_000,
                212_000_000,
            )),
            ..ready(false)
        });
        assert_eq!((s.word.as_str(), s.detail.as_str()), ("uploading", "64%"));
        assert_eq!(
            s.line,
            "Uploading Ridgeback Rally 2026.11.12 - 21.02.44.mp4, 64% of 212 MB."
        );
        assert_eq!(
            short(&Facts {
                processing: Some(("making the preview video, 40%", Some(40), "the bridge hold")),
                ..ready(false)
            }),
            ("making preview".into(), "40%".into(), None)
        );
        assert_eq!(
            short(&Facts {
                processing: Some(("on the server", None, "the bridge hold")),
                ..ready(false)
            })
            .0,
            "on the server"
        );
        assert_eq!(
            short(&Facts {
                preparing: Some("the bridge hold"),
                ..ready(false)
            })
            .0,
            "preparing link"
        );
        assert_eq!(
            short(&Facts {
                save_problem: Some("OBS did not save the replay"),
                ..ready(false)
            }),
            ("no clip saved".into(), String::new(), Some("settings:log"))
        );
        let s = said(&Facts {
            paused_by_user: true,
            waiting: 3,
            ..ready(false)
        });
        assert_eq!(
            (s.word.as_str(), s.detail.as_str()),
            ("uploads paused", "3 waiting")
        );
        assert_eq!(s.fix, Some(RESUME_UPLOADS));
        assert_eq!(s.line, "Uploads paused: 3 clips wait until you resume.");
        assert_eq!(
            short(&Facts {
                paused_by_user: true,
                ..ready(true)
            })
            .0,
            "ready",
            "standalone without a server has no uploads to pause"
        );
        assert_eq!(
            short(&Facts {
                paused_fullscreen: true,
                ..ready(false)
            })
            .0,
            "uploads waiting"
        );
        assert_eq!(
            short(&Facts {
                last_error: Some("server: 502 Bad Gateway"),
                ..ready(false)
            }),
            ("error".into(), String::new(), Some("settings:log"))
        );
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
            release_line(true, true, false, Some("1.10.0"), None, "1.9.0"),
            "reel 1.10.0 is out; Check now offers it to install"
        );
        assert_eq!(
            release_line(true, true, false, Some("1.9.0"), None, "1.9.0"),
            "up to date: 1.9.0 is the newest release"
        );
        assert_eq!(
            release_line(true, true, false, Some("1.9.0"), Some("timed out"), "1.9.0"),
            "the releases could not be read: timed out",
            "a read that failed says so, not the read before it"
        );
        assert_eq!(
            release_line(
                true,
                true,
                false,
                None,
                Some("GitHub answered 503"),
                "1.9.0"
            ),
            "the releases could not be read: GitHub answered 503"
        );
        assert!(release_line(true, true, false, None, None, "1.9.0").ends_with("then once a day"));
        assert!(
            release_line(true, true, true, None, None, "1.9.0").ends_with("then every six hours"),
            "New versions not off: the app reads again once a read is six hours old"
        );
        assert_eq!(
            release_line(true, false, false, None, None, "1.9.0"),
            "not asked yet: Check now asks GitHub for the newest release, and nothing asks by itself",
            "the default: nothing asked by itself"
        );
        assert!(
            release_line(false, true, false, None, None, "1.9.0").ends_with("on the releases page")
        );
        for line in [
            release_line(true, true, false, Some("1.10.0"), None, "1.9.0"),
            release_line(false, true, false, None, None, "1.9.0"),
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
