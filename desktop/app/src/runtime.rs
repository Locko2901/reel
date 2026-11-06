//! The app's running state and its flows; the tray, the window and the notifications read
//! the one Status snapshot built here.
use crate::standalone::{self, Local, LocalSettings};
use crate::status::{
    BufferStep, LocalLine, ObsLine, OfflineLine, QueueItem, RecentItem, Status, WatcherLine,
};
use crate::{hotkey, notify, shell, startup, tray};
use reel_desktop_core::api::{Api, ShareAnswer, WhoAmI};
use reel_desktop_core::cues::{Cue, Player};
use reel_desktop_core::gaming::{self, Foreground};
use reel_desktop_core::ledger::{ClipState, Ledger};
use reel_desktop_core::links::Link;
use reel_desktop_core::obs::{ObsChange, ObsCommand, ObsConfig, ObsError, ObsEvent, ObsState};
use reel_desktop_core::secrets::TokenStore;
use reel_desktop_core::settings::{
    self, Mode, Paths, Settings, UpdateMode, UploadMode, WebviewAudio,
};
use reel_desktop_core::shape::{Shape, ShapeHealth, ShapeJobs, ShapeSection, ShapeWho};
use reel_desktop_core::uploader::{self, Control, Progress, UploadMeta};
use reel_desktop_core::watcher::{self, Found, WatchConfig};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

/// How long a save waits for OBS's word that the replay is written: long enough for OBS to
/// write minutes of video to a slow disk.
const SAVE_WAIT: Duration = Duration::from_secs(20);

/// A press this soon after a save is a double tap: it saves nothing and plays nothing.
const DOUBLE_TAP: Duration = Duration::from_secs(1);
/// How long OBS stays stopped after the last step in Program Files for another one: an OBS
/// asked to close while it still starts takes no close and is ended after the grace.
const LINGER: Duration = Duration::from_secs(3);

/// A clip the server has whole and is still working on.
pub struct Processing {
    pub clip_id: String,
    pub title: String,
    pub phase: String,
}

pub struct Inner {
    pub paths: Paths,
    pub settings: Settings,
    /// A new install: no settings file was there at this start.
    pub fresh: bool,
    /// Standalone: the window was asked for the library before the local reel was up; it goes
    /// there once it is.
    pub library_wanted: bool,
    pub ledger: Ledger,
    pub tokens: TokenStore,
    pub api: Option<Api>,
    pub who: Option<WhoAmI>,
    pub pair_line: Option<String>,
    pub player: Player,
    pub log: Option<reel_desktop_core::log::LogHandle>,
    pub obs: ObsLine,
    pub watcher: WatcherLine,
    pub hotkey_ok: bool,
    pub hotkey_line: String,
    pub hotkeys_paused: bool,
    pub hotkey_listens: u64,
    pub uploading: Option<(PathBuf, String, u64, u64)>,
    /// Uploads a Save and share waits for, with its token: they run first while the others
    /// hold, and an older one holds too once a newer one is pressed.
    pub waited: HashMap<PathBuf, CancellationToken>,
    /// The pause of each waited upload that runs.
    pub waited_running: HashMap<PathBuf, watch::Sender<bool>>,
    pub paused_by_user: bool,
    pub paused_fullscreen: bool,
    /// Pause recording, for this run only: a start of the app records again.
    pub recording_paused: bool,
    pub preparing: Option<String>,
    pub processing: Vec<Processing>,
    pub last_error: Option<String>,
    /// Why the last save gave no replay, until the next replay OBS saves.
    pub save_problem: Option<String>,
    /// Saves waiting for OBS's word that the replay is written, in the order OBS took them,
    /// a Save and share's with its token.
    pub save_waits: VecDeque<(oneshot::Sender<PathBuf>, Option<CancellationToken>)>,
    pub last_save: Option<std::time::Instant>,
    /// Saves whose sound played when OBS took them, so their files come without one.
    pub cued_saves: Vec<std::time::Instant>,
    pub watch_cancel: Option<CancellationToken>,
    pub obs_cancel: Option<CancellationToken>,
    pub obs_commands: Option<mpsc::Sender<ObsCommand>>,
    pub obs_state: Option<ObsState>,
    /// The game OBS's game capture source targets by window, which names the clips before the
    /// foreground does.
    pub obs_game: Option<String>,
    /// OBS's recording folder, kept from the last connection; the clips folder follows it.
    pub obs_dir: Option<PathBuf>,
    pub foreground: Arc<dyn Foreground>,
    pub share_running: bool,
    /// The newest Save and share's token, cancelled when a newer one is pressed: only the
    /// newest's link is copied.
    pub newest_share: Option<CancellationToken>,
    pub update: Option<tauri_plugin_updater::Update>,
    pub update_bytes: Option<Vec<u8>>,
    /// Empty in standalone while the line is what reel on this PC's read of the releases found.
    pub update_line: String,
    pub update_told: Option<String>,
    pub update_checked_at: Option<std::time::SystemTime>,
    /// The version this one replaced, at its first start after an update.
    pub updated_from: Option<String>,
    /// Standalone mode: reel in this process, once it is up.
    pub local: Option<Arc<Local>>,
    pub local_line: String,
    /// A move, a use or a copy of the library, under way or just ended.
    pub library_job: Option<crate::library::Job>,
    pub library_stop: Arc<std::sync::atomic::AtomicBool>,
    /// The library is moving: reel on this PC does not start until it is in its new place.
    pub moving: bool,
    /// The library's folder is elsewhere and not there: why reel on this PC waits.
    pub library_away: Option<String>,
    /// The server cannot be reached: the app shows its own page instead.
    pub offline: Option<Offline>,
    /// Counts the navigations of the library webview; a watchdog belongs to one of them.
    pub nav_nonce: u64,
    /// A reel page called the app since the last navigation: the sign of life.
    pub page_alive: bool,
    /// When the page last called the app, which it does every two seconds while it shows.
    pub last_call: Option<std::time::Instant>,
    /// The URL the app itself asked the webview to load, watched already; a navigation
    /// that is not this one came from a link or the history and is watched when seen.
    pub asked: Option<String>,
    /// The shape of reel's Settings pages, as last known, and where from.
    pub shape: Option<(std::time::Instant, Shape, &'static str)>,
    /// The OBS that came with the app, and its supervisor once the app runs it.
    pub bundled: Option<reel_desktop_core::bundled::Bundled>,
    pub obs_box: Option<crate::obsbox::ObsBox>,
    /// Why the task for games that run as administrator was not made or removed.
    pub obs_task_problem: Option<String>,
    /// Whether the bundled OBS offers NVIDIA's noise removal (its filter kind registered).
    pub nvidia_noise: Option<bool>,
    pub obs_app_audio: Option<bool>,
    pub obs_game_capture: Option<bool>,
    /// The copy in Program Files that OBS runs from for games run as administrator; None on
    /// Linux.
    pub copy: Option<reel_desktop_core::bundled::Bundled>,
    /// The copy is there without the second task: its task starts OBS in its own tray until
    /// the copy is made again.
    pub copy_in_tray: bool,
    /// The copy's task at sign-in is there.
    pub copy_task: bool,
    /// A copy or its task left in Program Files with the switch off.
    pub copy_left: bool,
    /// The bundled OBS runs without the rights the setup asks for, as the hotkeys were last
    /// taken for it: the app then takes the Save hotkey itself.
    pub obs_without_rights: bool,
    /// What was said in a notification about OBS's rights and its copy this run, said once.
    pub copy_told: std::collections::HashSet<String>,
    /// The app is updating the copy by itself, waiting for no game in front or for Windows.
    pub copy_updating: bool,
    /// Linux on Wayland: the portal's session to close for a new binding.
    #[cfg(target_os = "linux")]
    pub portal_cancel: Option<CancellationToken>,
    /// Linux: what to know when the desktop shows no tray; empty when it shows one.
    pub tray_line: String,
}

/// What went wrong with the server, in words and in code, and when the next try is.
#[derive(Debug, Clone)]
pub struct Offline {
    pub title: String,
    pub words: String,
    pub code: String,
    pub attempt: u32,
    pub next_try: std::time::Instant,
}

/// Why a server did not answer, for the fallback page and the settings row.
#[derive(Debug, Clone)]
pub struct Unreachable {
    pub title: String,
    pub words: String,
    pub code: String,
}

/// How long a page gets to announce itself after a navigation before the app probes.
const LIFE_WAIT: Duration = Duration::from_secs(8);

fn error_chain(e: &dyn std::error::Error) -> String {
    let mut parts = vec![e.to_string()];
    let mut cur = e.source();
    while let Some(s) = cur {
        parts.push(s.to_string());
        cur = s.source();
    }
    parts.dedup();
    parts.join(": ")
}

/// One request to the server's front page, six seconds, no redirect followed: reachable
/// when anything but a 5xx comes back. The words say what to do, the code is the chain.
pub async fn probe_server(base: &str) -> Result<(), Unreachable> {
    let base = base.trim_end_matches('/');
    let host = base
        .parse::<url::Url>()
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| base.to_string());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(6))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(reel_desktop_core::user_agent())
        .build()
        .map_err(|e| Unreachable {
            title: "The app could not make a request".into(),
            words: e.to_string(),
            code: String::new(),
        })?;
    match client.get(format!("{base}/")).send().await {
        Ok(r) if r.status().is_server_error() => Err(Unreachable {
            title: "The server is up but not serving".into(),
            words: format!(
                "{host} answered {}: a proxy without reel behind it, or reel restarting.",
                r.status()
            ),
            code: format!("HTTP {}", r.status()),
        }),
        Ok(_) => Ok(()),
        Err(e) => {
            let code = error_chain(&e);
            let lower = code.to_lowercase();
            let (title, words) = if e.is_timeout() {
                (
                    "No answer from the server",
                    format!("{host} did not answer in six seconds. Is it up, and does this network reach it?"),
                )
            } else if lower.contains("dns") || lower.contains("lookup") || lower.contains("resolve")
            {
                (
                    "The server's name is unknown",
                    format!("{host} does not resolve here. A typo in the URL, or no DNS on this network?"),
                )
            } else if lower.contains("certificate")
                || lower.contains("tls")
                || lower.contains("handshake")
                || lower.contains("ssl")
            {
                (
                    "The connection is not trusted",
                    format!("{host} answered with a certificate this app does not trust: made for another name, expired, or a proxy in between."),
                )
            } else if lower.contains("refused") {
                (
                    "Nothing listens at the server",
                    format!("{host} refused the connection. Is reel running there, and is the port right?"),
                )
            } else {
                (
                    "Cannot reach the server",
                    format!("{host} could not be reached."),
                )
            };
            Err(Unreachable {
                title: title.into(),
                words,
                code,
            })
        }
    }
}

#[derive(Clone)]
pub struct Runtime {
    pub app: AppHandle,
    pub inner: Arc<Mutex<Inner>>,
    paused_tx: Arc<watch::Sender<bool>>,
    queue_tx: mpsc::UnboundedSender<PathBuf>,
    /// One worker for saves, Update the copy, Remove it and what is read back from OBS.
    lane: Arc<reel_desktop_core::lane::Lane>,
}

fn relative(ts: jiff::Timestamp) -> String {
    let secs = (jiff::Timestamp::now().as_second() - ts.as_second()).max(0);
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86_400 {
        format!("{} h ago", secs / 3600)
    } else {
        let z = ts.to_zoned(jiff::tz::TimeZone::system());
        z.strftime("%b %-d, %H:%M").to_string()
    }
}

/// Standalone: the newest release the local reel's daily check read, and whether it is newer.
fn standalone_release(g: &Inner, current: &str) -> (Option<String>, bool) {
    let Some(state) = g
        .local
        .as_ref()
        .and_then(|l| l.ctx.updates.try_read().ok().map(|u| u.clone()))
    else {
        return (None, false);
    };
    let latest = state.latest.map(|r| r.version);
    let newer = latest
        .as_deref()
        .is_some_and(|v| reel_core::versions::older(current, v));
    (latest, newer)
}

fn standalone_update_line(g: &Inner, current: &str) -> String {
    let local = g.local.as_ref();
    let state = local.and_then(|l| l.ctx.updates.try_read().ok().map(|u| u.clone()));
    crate::status::release_line(
        local.is_some(),
        local.is_some_and(|l| l.ctx.config().advanced.update_check),
        state
            .as_ref()
            .and_then(|u| u.latest.as_ref().map(|r| r.version.as_str())),
        state.as_ref().and_then(|u| u.error.as_deref()),
        current,
    )
}

impl Runtime {
    pub fn start(app: AppHandle) -> anyhow::Result<Runtime> {
        let paths = settings::paths();
        paths.ensure()?;
        let (settings, fresh) = Settings::load_or_fresh(&paths.settings_file())?;
        let log = match reel_desktop_core::log::init(&paths.logs_dir(), &settings.log_level) {
            Ok(h) => Some(h),
            Err(e) => {
                eprintln!("log: {e}");
                None
            }
        };
        tracing::info!(version = reel_desktop_core::VERSION, "reel desktop starts");
        if fresh {
            tracing::info!(
                "a new install, no settings file yet: standalone, the library on this PC"
            );
            if let Err(e) = settings.save(&paths.settings_file()) {
                tracing::warn!(error = %e, "the new install's settings were not written");
            }
        }
        let ledger = Ledger::load(&paths.ledger_file())?;
        let tokens = TokenStore::new(&paths.config_dir);
        let token = tokens.get().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "the token store did not answer");
            None
        });
        let api = match (settings.server_url.is_empty(), token) {
            (false, Some(t)) => Api::new(&settings.server_url, Some(t)).ok(),
            _ => None,
        };
        let player = if std::env::var("REEL_DESKTOP_SILENT").is_ok() {
            Player::silent(&paths.cues_dir())
        } else {
            Player::new(&paths.cues_dir())
        };
        tracing::info!(backend = %player.backend().name(), "cue player");
        let (paused_tx, _) = watch::channel(false);
        let (queue_tx, queue_rx) = mpsc::unbounded_channel::<PathBuf>();
        let first_line = match settings.mode {
            Mode::Standalone => String::new(),
            _ => "not checked yet".to_string(),
        };
        let inner = Inner {
            paths,
            settings,
            fresh,
            library_wanted: false,
            ledger,
            tokens,
            api,
            who: None,
            pair_line: None,
            player,
            log,
            obs: ObsLine {
                connected: false,
                line: "off".into(),
            },
            watcher: WatcherLine::default(),
            hotkey_ok: false,
            hotkey_line: String::new(),
            #[cfg(target_os = "linux")]
            #[cfg(target_os = "linux")]
            portal_cancel: None,
            tray_line: String::new(),
            hotkeys_paused: false,
            hotkey_listens: 0,
            uploading: None,
            paused_by_user: false,
            paused_fullscreen: false,
            recording_paused: false,
            preparing: None,
            processing: Vec::new(),
            last_error: None,
            save_problem: None,
            save_waits: VecDeque::new(),
            last_save: None,
            cued_saves: Vec::new(),
            waited: HashMap::new(),
            waited_running: HashMap::new(),
            watch_cancel: None,
            obs_cancel: None,
            obs_commands: None,
            obs_state: None,
            obs_game: None,
            obs_dir: None,
            foreground: Arc::new(gaming::System),
            share_running: false,
            newest_share: None,
            update: None,
            update_bytes: None,
            update_line: first_line,
            update_told: None,
            update_checked_at: None,
            updated_from: None,
            local: None,
            local_line: String::new(),
            library_job: None,
            library_stop: Default::default(),
            moving: false,
            library_away: None,
            offline: None,
            nav_nonce: 0,
            page_alive: false,
            last_call: None,
            asked: None,
            shape: None,
            bundled: detect_bundled(&app),
            obs_box: None,
            obs_task_problem: None,
            nvidia_noise: None,
            obs_app_audio: None,
            obs_game_capture: None,
            copy: None,
            copy_in_tray: false,
            copy_task: true,
            copy_left: false,
            obs_without_rights: false,
            copy_told: Default::default(),
            copy_updating: false,
        };
        let mut inner = inner;
        if inner
            .bundled
            .as_ref()
            .is_some_and(|b| b.platform == reel_desktop_core::bundled::Platform::Windows)
        {
            inner.copy = reel_desktop_core::obsproc::program_files()
                .map(|pf| reel_desktop_core::elevated::copy(&pf));
        }
        let rt = Runtime {
            app,
            inner: Arc::new(Mutex::new(inner)),
            paused_tx: Arc::new(paused_tx),
            queue_tx,
            lane: Arc::new(reel_desktop_core::lane::Lane::new(LINGER)),
        };
        rt.lane.start(Steps(rt.clone()));
        rt.spawn_upload_worker(queue_rx);
        rt.spawn_fullscreen_poll();
        rt.restart_watcher();
        rt.register_hotkeys();
        rt.startup_at_start();
        rt.spawn_whoami();
        rt.requeue_pending();
        rt.sweep_cache();
        rt.recover_library();
        if rt.is_standalone() {
            rt.start_local();
        }
        crate::updates::spawn_updated(rt.clone());
        crate::updates::spawn_checks(rt.clone());
        rt.push_status();
        Ok(rt)
    }

    /// Starts OBS and the app's connection to it, once the window and the tray are up.
    pub fn start_obs(&self) {
        self.apply_bundled(None);
        self.restart_obs();
        if self.lock().copy.is_some() {
            self.lane.post(reel_desktop_core::lane::Ask::Check);
        }
    }

    pub(crate) fn lane(&self) -> Arc<reel_desktop_core::lane::Lane> {
        self.lane.clone()
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn settings(&self) -> Settings {
        self.lock().settings.clone()
    }

    fn api(&self) -> Option<Api> {
        self.lock().api.clone()
    }

    fn cue(&self, cue: Cue) {
        let g = self.lock();
        let s = match cue {
            Cue::Captured => &g.settings.sounds.captured,
            Cue::Uploaded => &g.settings.sounds.uploaded,
            Cue::LinkCopied => &g.settings.sounds.link_copied,
        };
        g.player.play(cue, s);
    }

    /// OBS took a save: its sound plays now, not once the replay is written seconds later.
    fn save_taken(&self) {
        self.lock().cued_saves.push(std::time::Instant::now());
        self.cue(Cue::Captured);
    }

    /// Whether a clip just found is a save whose sound already played.
    fn was_cued(&self) -> bool {
        let mut g = self.lock();
        g.cued_saves.retain(|t| t.elapsed() < SAVE_WAIT);
        if g.cued_saves.is_empty() {
            return false;
        }
        g.cued_saves.remove(0);
        true
    }

    fn double_tap(&self) -> bool {
        let mut g = self.lock();
        let now = std::time::Instant::now();
        if g.last_save
            .is_some_and(|t| now.duration_since(t) < DOUBLE_TAP)
        {
            tracing::info!("a press within a second of the last save: a double tap, nothing saved");
            return true;
        }
        g.last_save = Some(now);
        false
    }

    pub fn status(&self) -> Status {
        let g = self.lock();
        let paired = g.api.is_some();
        let standalone = g.settings.mode == Mode::Standalone;
        let offline = g.offline.as_ref().map(|o| OfflineLine {
            title: o.title.clone(),
            words: o.words.clone(),
            code: o.code.clone(),
            retry_line: {
                let secs = o
                    .next_try
                    .saturating_duration_since(std::time::Instant::now())
                    .as_secs();
                if secs == 0 {
                    "trying again now".into()
                } else {
                    format!("trying again in about {secs} s (try {})", o.attempt + 1)
                }
            },
        });
        let (state, word, line) = if g.recording_paused {
            (
                "paused",
                "recording paused".to_string(),
                "the replay buffer is stopped until you resume recording".to_string(),
            )
        } else if let Some(o) = &g.offline {
            (
                "error",
                if standalone {
                    "reel on this PC does not answer"
                } else {
                    "cannot reach the server"
                }
                .to_string(),
                o.title.clone(),
            )
        } else if !paired && !standalone {
            (
                "unpaired",
                "not paired".to_string(),
                g.pair_line.clone().unwrap_or_else(|| {
                    "enter the server and a pairing code under Settings, Server".into()
                }),
            )
        } else if let Some(j) = g.library_job.as_ref().filter(|j| j.running) {
            let pct = (j.done * 100).checked_div(j.total).unwrap_or(0);
            (
                "working",
                if j.kind == "copy" {
                    "copying the library"
                } else {
                    "moving the library"
                }
                .to_string(),
                format!("{}, {pct}%", j.phase),
            )
        } else if let Some(why) = g.library_away.as_ref().filter(|_| standalone) {
            ("error", "library away".to_string(), why.clone())
        } else if let Some((_, name, sent, total)) = &g.uploading {
            let pct = if *total > 0 { sent * 100 / total } else { 0 };
            (
                "uploading",
                "uploading".to_string(),
                format!("{name}, {pct}%"),
            )
        } else if let Some(p) = g.processing.last() {
            ("working", p.phase.clone(), p.title.clone())
        } else if let Some(p) = &g.preparing {
            ("preparing", "preparing the link".to_string(), p.clone())
        } else if let Some(p) = &g.save_problem {
            ("error", "no replay saved".to_string(), p.clone())
        } else if g.paused_by_user && (!standalone || paired) {
            (
                "paused",
                "paused".to_string(),
                "uploads wait until you resume".into(),
            )
        } else if g.paused_fullscreen {
            (
                "paused",
                "paused".to_string(),
                if standalone {
                    "a game has the screen: the library's heavy work waits"
                } else {
                    "a game has the screen"
                }
                .into(),
            )
        } else if let Some(e) = &g.last_error {
            ("error", "error".to_string(), e.clone())
        } else {
            let n = g.ledger.pending().len();
            (
                "idle",
                "idle".to_string(),
                if n > 0 {
                    format!("{n} waiting")
                } else {
                    String::new()
                },
            )
        };
        let mut queue: Vec<QueueItem> = g
            .ledger
            .pending()
            .iter()
            .map(|c| {
                let (sent, total) = match &g.uploading {
                    Some((p, _, s, t)) if *p == c.path => (*s, *t),
                    _ => (0, c.size),
                };
                QueueItem {
                    path: c.path.display().to_string(),
                    name: c.name.clone(),
                    state: if g.uploading.as_ref().map(|u| &u.0) == Some(&c.path) {
                        "uploading".into()
                    } else if matches!(c.state, ClipState::Queued)
                        && (g.paused_by_user || g.paused_fullscreen)
                    {
                        "paused".into()
                    } else {
                        c.state.word().into()
                    },
                    sent,
                    total,
                    error: None,
                }
            })
            .collect();
        for c in g.ledger.recent(50) {
            if let ClipState::Failed { reason } = &c.state {
                queue.push(QueueItem {
                    path: c.path.display().to_string(),
                    name: c.name.clone(),
                    state: "failed".into(),
                    sent: 0,
                    total: c.size,
                    error: Some(reason.clone()),
                });
            }
        }
        let recent = g
            .ledger
            .recent(8)
            .iter()
            .filter(|c| {
                !matches!(
                    c.state,
                    ClipState::Found
                        | ClipState::Queued
                        | ClipState::Uploading
                        | ClipState::Failed { .. }
                )
            })
            .map(|c| RecentItem {
                clip_id: c.clip_id.clone(),
                name: c.name.clone(),
                title: c.title.clone(),
                when: relative(c.found_at),
                state: c.state.word().into(),
                local_clip_id: c.local_clip_id.clone(),
                phase: g
                    .processing
                    .iter()
                    .find(|p| Some(&p.clip_id) == c.clip_id.as_ref())
                    .map(|p| p.phase.clone()),
                share_url: c.share_url.clone(),
                path: if c.is_local() {
                    Some(c.path.display().to_string())
                } else {
                    None
                },
            })
            .collect();
        // A copy left is said once nothing runs or waits in the lane, against the setting the
        // last step ended with.
        let left = g.copy_left && self.lane.idle();
        let (save, save_share) = crate::status::save_actions(
            g.recording_paused,
            g.settings.obs.enabled || (g.bundled.is_some() && g.settings.obs.bundled),
            g.obs.connected,
            g.obs_state.as_ref(),
            paired,
            standalone,
        );
        let mut st = Status {
            paired,
            offline,
            mode: if standalone {
                "standalone"
            } else {
                "connected"
            }
            .into(),
            local: g.local.as_ref().map(|l| LocalLine {
                url: l.url.clone(),
                library_dir: g.settings.library_dir(&g.paths).display().to_string(),
                backend: l.backend.clone(),
                ffmpeg_version: l.ffmpeg_version.clone(),
                notes: l.notes.clone(),
                ffmpeg: standalone::ffmpeg_pin(),
            }),
            local_line: g.local_line.clone(),
            server_url: g.settings.server_url.clone(),
            server_name: g
                .who
                .as_ref()
                .map(|w| w.server_name.clone())
                .unwrap_or_default(),
            user: g.who.as_ref().map(|w| w.user.clone()).unwrap_or_default(),
            device_name: g
                .who
                .as_ref()
                .map(|w| w.name.clone())
                .unwrap_or_else(|| g.settings.device_name.clone()),
            state: state.into(),
            state_word: word,
            state_line: line,
            paused_by_user: g.paused_by_user,
            paused_fullscreen: g.paused_fullscreen,
            recording_paused: g.recording_paused,
            obs: g.obs.clone(),
            bundled: g.bundled.is_some(),
            obs_box: g.obs_box.as_ref().map(|b| b.line()),
            obs_task_problem: g.obs_task_problem.clone(),
            obs_step: self.lane.doing(),
            obs_elevated: g.settings.obs.setup.elevated,
            obs_copy: g
                .copy
                .as_ref()
                .filter(|_| {
                    g.bundled.is_some()
                        && ((g.settings.obs.bundled && g.settings.obs.setup.elevated) || left)
                })
                .map(|c| {
                    let version = reel_desktop_core::elevated::copy_version(c);
                    let wanted = reel_desktop_core::bundled::pinned().version;
                    let there = c.exe.is_file();
                    let named_before = there && reel_desktop_core::elevated::copy_named_before(c);
                    crate::status::CopyLine {
                        there,
                        stale: there
                            && (named_before || version.as_deref() != Some(wanted.as_str())),
                        tray: there && g.copy_in_tray,
                        task: g.copy_task,
                        left,
                        named_before,
                        version,
                        wanted,
                        folder: c
                            .config_root
                            .parent()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default(),
                    }
                }),
            nvidia_noise: g.nvidia_noise,
            obs_which: g.bundled.as_ref().map(|b| b.which()).unwrap_or_default(),
            obs_linux: g
                .bundled
                .as_ref()
                .is_some_and(|b| b.platform == reel_desktop_core::bundled::Platform::Linux),
            #[cfg(target_os = "linux")]
            wayland: crate::linux::session() == crate::linux::Session::Wayland,
            #[cfg(not(target_os = "linux"))]
            wayland: false,
            linux: cfg!(target_os = "linux"),
            obs_app_audio: g.obs_app_audio,
            obs_game_capture: g.obs_game_capture,
            server_upload_max_bytes: g.who.as_ref().and_then(|w| w.upload_max_bytes),
            obs_version: reel_desktop_core::bundled::pinned().version,
            obs_source: reel_desktop_core::bundled::pinned().sources_url,
            obs_state: g.obs_state.clone(),
            watcher: g.watcher.clone(),
            queue,
            recent,
            hotkey_ok: g.hotkey_ok,
            hotkey_line: g.hotkey_line.clone(),
            #[cfg(target_os = "linux")]
            hotkeys_how: crate::linux::hotkeys_how(),
            #[cfg(not(target_os = "linux"))]
            hotkeys_how: String::new(),
            tray_line: g.tray_line.clone(),
            save,
            save_share,
            discord_note: {
                let bundled = g.bundled.as_ref().filter(|_| g.settings.obs.bundled);
                let own = g.settings.obs.enabled && bundled.is_none();
                if own || bundled.is_some_and(|b| b.shows_as_obs()) {
                    crate::status::DISCORD_NOTE.to_string()
                } else {
                    String::new()
                }
            },
            obs_checks: if (g.bundled.is_some() && g.settings.obs.bundled)
                || !g.settings.obs.enabled
            {
                Vec::new()
            } else {
                crate::status::obs_checks(&g.obs, g.obs_state.as_ref(), &g.settings.obs)
            },
            server_admin: g.who.as_ref().map(|w| w.admin).unwrap_or(false),
            normalize_new: g.who.as_ref().and_then(|w| w.normalize_new),
            loudness_target: g.who.as_ref().and_then(|w| w.loudness_target),
            update_line: crate::updates::line_now(
                if standalone && g.update_line.is_empty() {
                    standalone_update_line(&g, &self.app.package_info().version.to_string())
                } else {
                    g.update_line.clone()
                },
                g.update
                    .as_ref()
                    .filter(|_| g.update_bytes.is_some() && !crate::updates::installing())
                    .map(|u| u.version.as_str()),
                g.library_job.as_ref(),
            ),
            update_version: g.update.as_ref().map(|u| u.version.clone()),
            update_installing: crate::updates::installing(),
            update_notes: g.update.as_ref().and_then(|u| u.body.clone()),
            update_before: g.update.as_ref().map(|u| {
                let kind = reel_desktop_core::update_steps::Kind::here(g.bundled.is_some());
                let on_quit = g.settings.updates == UpdateMode::OnQuit && g.update_bytes.is_some();
                reel_desktop_core::update_steps::before(&u.version, kind, on_quit)
            }),
            update_checked_at: g
                .update_checked_at
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64),
            updated_from: g.updated_from.clone(),
            versions: self.versions(&g),
            update_mode: match g.settings.updates {
                UpdateMode::Ask => "ask",
                UpdateMode::OnQuit => "on_quit",
                UpdateMode::Off => "off",
            }
            .into(),
            releases_url: crate::updates::releases_url(),
            uninstaller: crate::updates::uninstaller().is_some(),
            version: reel_desktop_core::version_line(),
            config_dir: g.paths.config_dir.display().to_string(),
            data_dir: g.paths.data_dir.display().to_string(),
            library: Default::default(),
            panel: Default::default(),
        };
        drop(g);
        st.library = self.library_line();
        st.panel = crate::status::panel(&st);
        st
    }

    /// Sends the snapshot to the tray and to the app's own page; a server page asks with
    /// `app_status` instead.
    pub fn push_status(&self) {
        let st = self.status();
        shell::emit_local(&self.app, "status", &st);
        tray::refresh(&self.app, &st);
    }

    /// What a reel page inside the app may know: no URL, no path, no setting value.
    pub fn app_status(&self) -> serde_json::Value {
        static FIRST: std::sync::Once = std::sync::Once::new();
        FIRST.call_once(|| tracing::info!("the server's page reached the app"));
        self.page_alive();
        let st = self.status();
        let can_share =
            st.mode != "standalone" && st.paired && self.lock().ledger.last_confirmed().is_some();
        serde_json::json!({
            "state": st.state,
            "state_word": st.state_word,
            "state_line": st.state_line,
            "paused": st.paused_by_user,
            "recording_paused": st.recording_paused,
            "obs_connected": st.obs.connected,
            "obs_line": st.obs.line,
            "can_share": can_share,
            "save": st.save,
            "save_share": st.save_share,
            "mode": st.mode,
            "paired": st.paired,
            "panel": st.panel,
            "version": st.version,
            "versions": st.versions,
            "update_line": st.update_line,
        })
    }

    fn versions(&self, g: &Inner) -> crate::status::Versions {
        let current = self.app.package_info().version.to_string();
        let shown = reel_desktop_core::version_line();
        let paired = g.settings.mode != Mode::Standalone;
        let server = g
            .who
            .as_ref()
            .map(|w| w.version.clone())
            .filter(|v| paired && !v.is_empty());
        let update = g.update.as_ref().map(|u| u.version.clone());
        let (mut standing, words) =
            crate::status::standing(&current, &shown, server.as_deref(), update.as_deref());
        let released = if paired {
            g.who.as_ref().and_then(|w| w.newest_release.clone())
        } else {
            let (latest, newer) = standalone_release(g, &current);
            if update.is_some() {
                standing = "update";
            } else if newer {
                standing = "out";
            }
            latest
        };
        #[cfg(target_os = "linux")]
        let kind = match crate::linux::appimage() {
            Some(_) => "the AppImage",
            None => "the .deb",
        };
        #[cfg(not(target_os = "linux"))]
        let kind = reel_core::packages::windows(g.bundled.is_some()).name;
        crate::status::Versions {
            app: shown,
            kind: kind.into(),
            obs: g
                .bundled
                .as_ref()
                .filter(|b| b.platform == reel_desktop_core::bundled::Platform::Windows)
                .map(|_| reel_desktop_core::bundled::pinned().version),
            server,
            newest: released,
            standing: standing.into(),
            words,
        }
    }

    pub async fn pair(&self, server_url: &str, code: &str, name: &str) -> Result<(), String> {
        let base = settings::server_base(server_url)?;
        let name = if name.trim().is_empty() {
            settings::default_device_name()
        } else {
            name.trim().to_string()
        };
        let paired = Api::pair(&base, code, &name)
            .await
            .map_err(|e| e.to_string())?;
        let api = Api::new(&base, Some(paired.token.clone())).map_err(|e| e.to_string())?;
        {
            let mut g = self.lock();
            g.tokens
                .set(&paired.token)
                .map_err(|e| format!("the token could not be kept: {e}"))?;
            g.settings.server_url = base.clone();
            g.settings.device_name = paired.name.clone();
            let file = g.paths.settings_file();
            g.settings.save(&file).map_err(|e| e.to_string())?;
            g.api = Some(api);
            g.pair_line = None;
            g.last_error = None;
        }
        tracing::info!(server = %base, device = %paired.name, user = %paired.user, "paired");
        self.spawn_whoami();
        let _ = base;
        self.library_changed();
        self.push_status();
        Ok(())
    }

    pub fn unpair(&self) {
        let mut g = self.lock();
        let _ = g.tokens.clear();
        g.api = None;
        g.who = None;
        g.pair_line = Some("unpaired here; revoke the device on the server too".into());
        drop(g);
        tracing::info!("unpaired");
        self.push_status();
    }

    fn spawn_whoami(&self) {
        let Some(api) = self.api() else {
            return;
        };
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            match api.whoami().await {
                Ok(w) => {
                    rt.lock().who = Some(w);
                    rt.lock().pair_line = None;
                }
                Err(reel_desktop_core::api::ApiError::Unauthorized) => {
                    let mut g = rt.lock();
                    g.api = None;
                    g.who = None;
                    g.pair_line =
                        Some("the server does not know this device any more; pair again".into());
                }
                Err(e) => {
                    rt.lock().last_error = Some(format!("server: {e}"));
                }
            }
            rt.push_status();
        });
    }

    pub fn save_settings(&self, new: Settings, base: Option<Settings>) -> Result<Settings, String> {
        if !new.server_url.trim().is_empty() {
            settings::check_server_url(&new.server_url)?;
        }
        if new.extensions.is_empty() {
            return Err("at least one extension".into());
        }
        if new.upload.keep_last != 0 && !(5..=600).contains(&new.upload.keep_last) {
            return Err("keep the last 5 to 600 seconds, or 0 for the whole clip".into());
        }
        if !(5..=1800).contains(&new.obs.setup.clip_seconds) {
            return Err("a clip of 5 seconds to 30 minutes".into());
        }
        let mut new = new;
        new.server_url = if new.server_url.trim().is_empty() {
            String::new()
        } else {
            settings::server_base(&new.server_url)?
        };
        // only what this request changed from its own base, applied over the requests before
        let base = base.unwrap_or_else(|| self.settings());
        // the library folder changes by a move alone
        new.standalone.library_dir = base.standalone.library_dir.clone();
        let changes = reel_desktop_core::lane::Changes::between(&base, &new);
        let lane = self.lane.clone();
        tokio::task::block_in_place(|| lane.ask(reel_desktop_core::lane::Ask::Save(changes)))
    }

    /// Keeps the settings a turn of the lane ends with, and applies them to the rest of the app.
    fn store(&self, old: &Settings, new: Settings) -> Result<Settings, String> {
        let old = old.clone();
        {
            let mut g = self.lock();
            let file = g.paths.settings_file();
            new.save(&file).map_err(|e| e.to_string())?;
            g.settings = new.clone();
            if new.server_url != old.server_url {
                g.api = match (new.server_url.is_empty(), g.tokens.get().ok().flatten()) {
                    (false, Some(t)) => Api::new(&new.server_url, Some(t)).ok(),
                    _ => None,
                };
                g.who = None;
            }
            if new.log_level != old.log_level {
                if let Some(l) = &g.log {
                    if let Err(e) = l.set_level(&new.log_level) {
                        tracing::warn!(error = %e, "log level");
                    }
                }
            }
        }
        if new.server_url != old.server_url {
            self.library_changed();
            self.spawn_whoami();
        }
        if new.clips_dir != old.clips_dir
            || new.clips_follow_obs != old.clips_follow_obs
            || new.extensions != old.extensions
            || new.stable_seconds != old.stable_seconds
        {
            self.restart_watcher();
        }
        let filters_only =
            new.obs.bundled && old.obs.bundled && new.obs.setup != old.obs.setup && {
                let mut was = old.obs.setup.clone();
                was.noise = new.obs.setup.noise;
                was.gate = new.obs.setup.gate;
                was.own_filters = new.obs.setup.own_filters.clone();
                was.own_filters_from = new.obs.setup.own_filters_from.clone();
                was.volumes = new.obs.setup.volumes.clone();
                was == new.obs.setup
            };
        if filters_only {
            // the microphone's filters and the volumes change in the running OBS, no restart
            if let Some(bx) = self.lock().obs_box.clone() {
                bx.update(
                    new.obs.setup.clone(),
                    new.clips_dir.clone(),
                    new.hotkeys.save.clone(),
                );
            }
            let rt = self.clone();
            let (setup, was) = (new.obs.setup.clone(), old.obs.setup.clone());
            tauri::async_runtime::spawn(async move {
                let filters_changed = (setup.noise, setup.gate, &setup.own_filters)
                    != (was.noise, was.gate, &was.own_filters);
                if filters_changed {
                    if let Err(e) = rt.apply_mic_filters(&setup).await {
                        tracing::warn!(error = %e, "the microphone's filters in OBS");
                    }
                }
                for (input, db) in &setup.volumes {
                    if was.volumes.get(input) != Some(db) {
                        if let Err(e) = rt.obs_volume(input, *db).await {
                            tracing::warn!(error = %e, input, "a volume in OBS");
                        }
                    }
                }
            });
        }
        let bundled_changed = !filters_only
            && (new.obs.bundled != old.obs.bundled
                || new.obs.setup != old.obs.setup
                || (new.obs.bundled && new.clips_dir != old.clips_dir)
                || (new.obs.bundled
                    && new.obs.setup.elevated
                    && new.hotkeys.save != old.hotkeys.save));
        if bundled_changed {
            self.apply_bundled(Some(&old));
        }
        if bundled_changed
            || new.obs.enabled != old.obs.enabled
            || new.obs.host != old.obs.host
            || new.obs.port != old.obs.port
            || new.obs.password != old.obs.password
        {
            self.restart_obs();
        } else if new.obs.start_replay && !old.obs.start_replay {
            let rt = self.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(Some(st)) = rt.obs_state(true).await {
                    rt.ensure_replay_buffer(&st).await;
                }
            });
        }
        if new.hotkeys != old.hotkeys || new.mode != old.mode {
            self.register_hotkeys();
        }
        if new.startup != old.startup {
            self.apply_startup();
        }
        if new.tray_icon != old.tray_icon {
            tray::set_icon(&self.app, new.tray_icon);
        }
        if new.mode != old.mode {
            {
                let mut g = self.lock();
                g.update = None;
                g.update_bytes = None;
                g.update_line = String::new();
            }
            match new.mode {
                Mode::Standalone => self.start_local(),
                Mode::Connected => {
                    self.stop_local();
                    self.library_changed();
                    let rt = self.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = crate::updates::check(&rt, false).await {
                            tracing::warn!(error = %e, "update check");
                        }
                    });
                }
            }
        } else if new.mode == Mode::Standalone && new.standalone != old.standalone {
            self.stop_local();
            self.start_local();
        }
        if new.upload.pause_in_fullscreen != old.upload.pause_in_fullscreen
            && !new.upload.pause_in_fullscreen
        {
            self.lock().paused_fullscreen = false;
            self.update_paused();
        }
        if new.cache != old.cache {
            self.sweep_cache();
        }
        tracing::info!("settings saved");
        self.push_status();
        {
            use tauri::Emitter;
            let _ = self.app.emit("settings", &new);
        }
        Ok(new)
    }

    fn apply_startup(&self) {
        let s = self.settings();
        if let Err(e) = startup::apply(&self.app, s.startup.autostart, s.startup.elevated) {
            tracing::warn!(error = %e, "startup");
            self.lock().last_error = Some(format!("startup: {e}"));
        }
    }

    /// Only the Run entry (a registry read), so no schtasks runs at start.
    fn startup_at_start(&self) {
        let s = self.settings();
        if let Err(e) = startup::run_entry(&self.app, s.startup.autostart && !s.startup.elevated) {
            tracing::warn!(error = %e, "startup");
        }
    }

    /// The hotkeys of this mode, each with whether the app registers it; with the highest rights
    /// OBS takes Save a clip itself, as only its hotkey reaches a game run as administrator.
    fn live_hotkeys<'a>(
        &self,
        s: &'a Settings,
    ) -> Vec<(reel_desktop_core::settings::Hotkey<'a>, bool)> {
        let without = self.lock().obs_without_rights;
        let obs_saves = self.bundled_in_use() && s.obs.setup.elevated && cfg!(windows) && !without;
        let here = s.mode == Mode::Standalone;
        s.hotkeys
            .all()
            .into_iter()
            .filter(|h| !here || h.action == hotkey::Action::Save)
            .map(|h| (h, !(obs_saves && h.action == hotkey::Action::Save)))
            .collect()
    }

    pub fn register_hotkeys(&self) {
        if !hotkey::grabs() {
            #[cfg(target_os = "linux")]
            self.register_portal();
            return;
        }
        let s = self.settings();
        let rt = self.clone();
        let live = self.live_hotkeys(&s);
        let obs_saves = live
            .iter()
            .any(|(h, by_app)| !by_app && h.action == hotkey::Action::Save);
        let here = s.mode == Mode::Standalone;
        let keys: Vec<(&str, hotkey::Action)> = live
            .iter()
            .filter(|(_, by_app)| *by_app)
            .map(|(h, _)| (h.combo, h.action))
            .collect();
        let problems = hotkey::register(&self.app, &keys, move |_app, action| match action {
            hotkey::Action::ShareLast => rt.share_last(None),
            hotkey::Action::SaveShare => rt.save_and_share(),
            hotkey::Action::UploadLast => rt.upload_last(),
            hotkey::Action::Save => rt.save_clip(),
        });
        let mut g = self.lock();
        g.hotkeys_paused = false;
        g.hotkey_ok = problems.is_empty();
        g.hotkey_line = if !problems.is_empty() {
            problems.join("; ")
        } else if here {
            match s.hotkeys.save.trim() {
                "" => "no hotkey for Save a clip yet: set one below".to_string(),
                k if obs_saves => format!("{k} saves a clip, through OBS"),
                k => format!("{k} saves a clip"),
            }
        } else {
            let mut line = format!("{} shares the last clip", s.hotkeys.share_last);
            if !s.hotkeys.save_share.trim().is_empty() {
                line.push_str(&format!(
                    ", {} saves the replay and shares it",
                    s.hotkeys.save_share
                ));
            }
            line
        };
        if !problems.is_empty() {
            tracing::warn!(problems = ?problems, "hotkeys");
        }
    }

    /// On Wayland: the shortcuts bound through the desktop's portal in a new session, the one
    /// before closed.
    #[cfg(target_os = "linux")]
    fn register_portal(&self) {
        let s = self.settings();
        let cancel = CancellationToken::new();
        if let Some(old) = self.lock().portal_cancel.replace(cancel.clone()) {
            old.cancel();
        }
        let mut shortcuts = vec![
            (
                "share-last",
                "Share the last clip",
                s.hotkeys.share_last.clone(),
            ),
            (
                "save-and-share",
                "Save the replay and share it",
                s.hotkeys.save_share.clone(),
            ),
            ("save", "Save a clip", s.hotkeys.save.clone()),
            (
                "upload-last",
                "Upload the last clip",
                s.hotkeys.upload_last.clone(),
            ),
        ];
        if s.mode == Mode::Standalone {
            shortcuts.retain(|(id, _, _)| *id == "save");
        }
        let rt = self.clone();
        let data_dir = self.lock().paths.data_dir.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = crate::linux::desktop_entry(&data_dir) {
                tracing::warn!(error = %e, "no desktop entry for the portals");
            }
            let on = {
                let rt = rt.clone();
                move |id: &str| rt.run_action(id)
            };
            let bound = crate::portal::bind(shortcuts, cancel, on).await;
            {
                let mut g = rt.lock();
                match bound {
                    Ok(bound) => {
                        tracing::info!(?bound, "hotkeys bound through the desktop's portal");
                        g.hotkey_ok = true;
                        g.hotkey_line = bound
                            .iter()
                            .filter(|(_, keys)| !keys.is_empty())
                            .map(|(id, keys)| format!("{keys} {}", crate::portal::what(id)))
                            .collect::<Vec<_>>()
                            .join(", ");
                        if g.hotkey_line.is_empty() {
                            g.hotkey_line =
                                "bound; the desktop has no keys on them yet: set them in its keyboard settings".into();
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "no hotkeys through a portal");
                        g.hotkey_ok = false;
                        g.hotkey_line = "the desktop took no hotkeys: bind a key to the command the Hotkeys page shows in its keyboard settings".into();
                    }
                }
            }
            rt.push_status();
        });
    }

    /// A hotkey's action by its name, from the command line (`--save-and-share`) or the portal.
    pub fn run_action(&self, name: &str) {
        tracing::info!(action = name, "an action from outside the app");
        match name.trim_start_matches("--") {
            "share-last" => self.share_last(None),
            "save-and-share" => self.save_and_share(),
            "upload-last" => self.upload_last(),
            "save" => self.save_clip(),
            "pause-recording" => self.set_recording_paused(true),
            "resume-recording" => self.set_recording_paused(false),
            other => tracing::warn!(action = other, "no such action"),
        }
    }

    /// An action the first start was given: once OBS answers when it needs OBS.
    pub fn run_action_when_ready(&self, name: &'static str) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            let needs_obs = matches!(name, "--save-and-share" | "--save");
            for _ in 0..30 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if !needs_obs || rt.lock().obs.connected {
                    break;
                }
            }
            rt.run_action(name);
        });
    }

    /// The hotkeys are off while a field listens; they come back when it stops, or after 30 s.
    pub fn hotkeys_listen(&self, on: bool) {
        if !on {
            let paused = self.lock().hotkeys_paused;
            if paused {
                self.register_hotkeys();
            }
            return;
        }
        let listen = {
            let mut g = self.lock();
            g.hotkeys_paused = true;
            g.hotkey_listens += 1;
            g.hotkey_listens
        };
        hotkey::pause(&self.app);
        tracing::info!("hotkeys off while a field listens");
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let stale = {
                let g = rt.lock();
                g.hotkeys_paused && g.hotkey_listens == listen
            };
            if stale {
                rt.register_hotkeys();
            }
        });
    }

    /// A combination recorded for one hotkey: refused when another hotkey of this mode has it,
    /// or when another app holds it and the app would register it.
    pub fn hotkey_try(&self, key: &str, combo: &str) -> Result<String, String> {
        let s = self.settings();
        let mine = s
            .hotkeys
            .all()
            .into_iter()
            .find(|h| h.key == key)
            .ok_or_else(|| format!("{key} is not a hotkey"))?;
        let live = self.live_hotkeys(&s);
        let others: Vec<(&str, &str)> = live
            .iter()
            .filter(|(h, _)| h.key != key)
            .map(|(h, _)| (h.label, h.combo))
            .collect();
        let trial = live.iter().any(|(h, by_app)| h.key == key && *by_app);
        let tried = hotkey::try_combo(&self.app, combo, mine.combo, &others, trial);
        match &tried {
            Ok(c) => tracing::info!(hotkey = key, combo = %c, "hotkey free"),
            Err(why) => tracing::info!(hotkey = key, combo, why = %why, "hotkey refused"),
        }
        tried
    }

    /// The folder watched: OBS's recording folder when the setting follows OBS and OBS told
    /// it, else the one from the settings.
    pub fn clips_dir(&self) -> (PathBuf, bool) {
        let g = self.lock();
        if g.settings.clips_follow_obs {
            if let Some(d) = &g.obs_dir {
                return (d.clone(), true);
            }
        }
        (g.settings.clips_dir.clone(), false)
    }

    pub fn restart_watcher(&self) {
        let s = self.settings();
        let (dir, from_obs) = self.clips_dir();
        let cancel = CancellationToken::new();
        {
            let mut g = self.lock();
            if let Some(old) = g.watch_cancel.take() {
                old.cancel();
            }
            g.watch_cancel = Some(cancel.clone());
            let ok = dir.is_dir();
            g.watcher = WatcherLine {
                dir: dir.display().to_string(),
                ok,
                line: if ok {
                    format!(
                        "watching {}{}",
                        dir.display(),
                        if from_obs {
                            " (OBS's recording folder)"
                        } else {
                            ""
                        }
                    )
                } else {
                    format!("{} does not exist", dir.display())
                },
                from_obs,
            };
        }
        if !dir.is_dir() {
            tracing::warn!(dir = %dir.display(), "the clips folder does not exist; nothing is watched");
            return;
        }
        tracing::info!(dir = %dir.display(), from_obs, stable_seconds = s.stable_seconds, "watching the clips folder");
        let (tx, mut rx) = mpsc::channel::<Found>(64);
        let cfg = WatchConfig {
            dir,
            extensions: s.extensions.clone(),
            stable: Duration::from_secs(s.stable_seconds.max(1)),
            take_backlog: true,
        };
        tauri::async_runtime::spawn(watcher::run(cfg, tx, cancel));
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(f) = rx.recv().await {
                rt.on_found(&f.path, f.size, f.backlog, "folder");
            }
        });
    }

    fn obs_runs_without_rights(&self) -> bool {
        self.lock()
            .obs_box
            .as_ref()
            .is_some_and(|b| b.line().without_rights)
    }

    /// The app came with OBS, and the setup chose that OBS.
    pub fn bundled_in_use(&self) -> bool {
        let g = self.lock();
        g.bundled.is_some() && g.settings.obs.bundled
    }

    /// Starts, sets up again or stops the bundled OBS as the settings say; only the change
    /// from `from` is written into OBS's settings.
    pub fn apply_bundled(&self, from: Option<&Settings>) {
        let (bundled, mut s, existing) = {
            let g = self.lock();
            (g.bundled.clone(), g.settings.clone(), g.obs_box.clone())
        };
        let Some(b) = bundled else {
            return;
        };
        if !s.obs.bundled {
            if let Some(bx) = existing {
                self.lock().obs_box = None;
                std::thread::spawn(move || bx.stop());
            }
            return;
        }
        if s.obs.setup.password.is_empty() {
            s.obs.setup.password = reel_desktop_core::bundled::new_password();
            let mut g = self.lock();
            let file = g.paths.settings_file();
            if let Err(e) = s.save(&file) {
                tracing::warn!(error = %e, "the OBS password was not saved");
            }
            g.settings = s.clone();
        }
        match existing {
            Some(bx) => bx.apply(
                s.obs.setup.clone(),
                s.clips_dir.clone(),
                s.hotkeys.save.clone(),
                from.filter(|o| o.obs.bundled).map(|o| {
                    (
                        o.obs.setup.clone(),
                        o.clips_dir.clone(),
                        o.hotkeys.save.clone(),
                    )
                }),
            ),
            None => {
                let rt = self.clone();
                let told = self.app.clone();
                let copy = self.lock().copy.clone();
                let bx = crate::obsbox::ObsBox::start(
                    (b, copy),
                    s.obs.setup.clone(),
                    s.clips_dir.clone(),
                    s.hotkeys.save.clone(),
                    move || rt.obs_box_changed(),
                    move |n| {
                        notify::show(
                            &told,
                            "OBS keeps stopping",
                            &format!(
                                "{} stopped {n} times in a row; reel starts it again each time. Settings, Log has the details.",
                                if cfg!(target_os = "linux") { "reel's own OBS" } else { "The OBS that came with reel" }
                            ),
                        )
                    },
                );
                self.lock().obs_box = Some(bx);
            }
        }
    }

    /// The bundled OBS's line changed; when OBS starts or stops running without the rights the
    /// setup asks for, the hotkeys are taken again and a notification says why.
    fn obs_box_changed(&self) {
        self.push_status();
        let Some(line) = self.lock().obs_box.as_ref().map(|b| b.line()) else {
            return;
        };
        let was = std::mem::replace(&mut self.lock().obs_without_rights, line.without_rights);
        if was == line.without_rights {
            return;
        }
        self.register_hotkeys();
        if line.without_rights {
            self.tell_once(
                "OBS runs without the highest rights",
                "Games that run as administrator are not captured until the copy of OBS for them is in Program Files again: Settings, OBS. Your other games are.",
            );
        }
    }

    fn tell_once(&self, title: &str, body: &str) {
        if self.lock().copy_told.insert(format!("{title}\n{body}")) {
            notify::show(&self.app, title, body);
        }
    }

    /// An input's choices in OBS, for the setup's lists (the audio devices).
    pub async fn obs_items(
        &self,
        input: String,
        property: String,
    ) -> Result<Vec<reel_desktop_core::obs::Item>, String> {
        self.obs_command(|reply| ObsCommand::Items {
            input,
            property,
            reply,
        })
        .await
    }

    /// Sets an input's settings in OBS now, so the setup's meter follows the device picked.
    pub async fn obs_input_set(
        &self,
        input: String,
        settings: serde_json::Value,
    ) -> Result<(), String> {
        self.obs_command(|reply| ObsCommand::SetInput {
            input,
            settings,
            reply,
        })
        .await
    }

    pub async fn obs_meters(&self, on: bool) -> Result<(), String> {
        self.obs_command(|reply| ObsCommand::Meters { on, reply })
            .await
    }

    /// Any request of obs-websocket, its response data.
    pub async fn obs_request(
        &self,
        kind: &str,
        data: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let kind = kind.to_string();
        self.obs_command(|reply| ObsCommand::Request { kind, data, reply })
            .await
    }

    /// A source's volume in the running OBS, in dB.
    pub async fn obs_volume(&self, input: &str, db: f64) -> Result<(), String> {
        self.obs_request(
            "SetInputVolume",
            serde_json::json!({ "inputName": input, "inputVolumeDb": db.clamp(-60.0, 26.0) }),
        )
        .await
        .map(|_| ())
    }

    /// Sets the Microphone's filters in the running OBS to what the setup wants, Strong standing
    /// in for NVIDIA where OBS does not offer it.
    pub async fn apply_mic_filters(
        &self,
        setup: &reel_desktop_core::bundled::ObsSetup,
    ) -> Result<(), String> {
        use reel_desktop_core::bundled::{filter_requests, Noise};
        let mut s = setup.clone();
        if s.noise == Noise::Nvidia && self.lock().nvidia_noise == Some(false) {
            s.noise = Noise::Rnnoise;
        }
        let current = self
            .obs_request(
                "GetSourceFilterList",
                serde_json::json!({ "sourceName": "Microphone" }),
            )
            .await?;
        let current = current["filters"].as_array().cloned().unwrap_or_default();
        for (kind, data) in filter_requests("Microphone", &current, &s.mic_filters()) {
            self.obs_request(&kind, data).await?;
        }
        tracing::info!(noise = ?s.noise, gate = s.gate, "the microphone's filters set in OBS");
        Ok(())
    }

    fn copy_there(&self) -> bool {
        self.lock().copy.as_ref().is_some_and(|c| c.exe.is_file())
    }

    /// One elevated step with the settings the copy gets; on Remove the copy's settings go back
    /// into the bundled OBS's, so nothing set there is lost.
    fn copy_step(
        &self,
        step: reel_desktop_core::elevated::Step,
        old: &Settings,
        new: &Settings,
        answered: &dyn Fn(),
    ) -> Result<(), String> {
        use reel_desktop_core::elevated::Step;
        let (b, copy) = {
            let g = self.lock();
            (g.bundled.clone(), g.copy.clone())
        };
        let b = b.ok_or("this app came without OBS")?;
        let copy = copy.ok_or("Windows did not say where Program Files is")?;
        let (mut from, mut to) = (old.obs.setup.clone(), new.obs.setup.clone());
        (from.elevated, to.elevated) = match step {
            Step::Install => (false, true),
            Step::Write => (true, true),
            Step::Remove => (true, false),
        };
        let from = (&from, old.clips_dir.as_path(), old.hotkeys.save.as_str());
        let to = (&to, new.clips_dir.as_path(), new.hotkeys.save.as_str());
        if step == Step::Remove {
            let back = copy.read_files().map(|_| b.change(&copy, from, to));
            reel_desktop_core::obsproc::task_end();
            reel_desktop_core::obsproc::elevated_step(step, &b, &[], answered)?;
            self.read_copy();
            if let Some(files) = back {
                if let Err(e) = b.put(&files) {
                    tracing::warn!(error = %e, "the copy's settings did not go back into the bundled OBS");
                }
            }
            tracing::info!("the OBS for games that run as administrator removed, its task too");
            return Ok(());
        }
        let theirs = if step == Step::Install { &b } else { &copy };
        let files = copy.change(theirs, from, to);
        reel_desktop_core::obsproc::task_end();
        reel_desktop_core::obsproc::elevated_step(step, &b, &files, answered)?;
        self.read_copy();
        tracing::info!(step = ?step, files = files.len(), "the OBS for games that run as administrator, in Program Files");
        Ok(())
    }

    /// Finds the copy's program, tray and task again once the helper made or removed it; read
    /// only, it is set against the setting once nothing waits.
    fn read_copy(&self) {
        let found = reel_desktop_core::obsproc::program_files()
            .map(|pf| reel_desktop_core::elevated::copy(&pf));
        let in_tray = in_tray(found.as_ref());
        let task = reel_desktop_core::obsproc::task_exists();
        let mut g = self.lock();
        if g.copy.is_none() {
            return;
        }
        g.copy = found;
        g.copy_in_tray = in_tray;
        g.copy_task = task;
    }

    /// Checks the copy against the saved setting, at the start and whenever the lane is idle;
    /// what is off is said with the step that sets it right, never prompted for.
    fn check_copy(&self) {
        if self.lock().copy.is_none() {
            return;
        }
        self.read_copy();
        let mut g = self.lock();
        let on = g.bundled.is_some() && reel_desktop_core::lane::on(&g.settings);
        let there = g.copy.as_ref().is_some_and(|c| c.exe.is_file());
        let (task, in_tray) = (g.copy_task, g.copy_in_tray);
        g.copy_left = reel_desktop_core::lane::left(on, there, task);
        tracing::info!(
            on,
            there,
            task,
            in_tray,
            "the copy of OBS in Program Files read"
        );
        if g.copy_left {
            tracing::warn!(there, task, "a copy of OBS with the rights is left in Program Files though Capture games that run as administrator is off: Settings, OBS offers to remove it");
        } else if on && there && !task {
            tracing::warn!("Capture games that run as administrator is on, the copy is there, its task at sign-in is not: Update the copy puts it back");
        }
        let wanted = reel_desktop_core::bundled::pinned().version;
        let older = match (on && there, g.copy.as_ref()) {
            (true, Some(c)) => match reel_desktop_core::elevated::copy_version(c) {
                Some(v) if v == wanted && !reel_desktop_core::elevated::copy_named_before(c) => {
                    None
                }
                Some(v) => Some(format!("OBS {v}")),
                None => Some("an OBS of an earlier version".to_string()),
            },
            _ => None,
        };
        let version = self.app.package_info().version.to_string();
        let updating = g.copy_updating;
        let by_itself = reel_desktop_core::elevated::update_at_start(
            on,
            there,
            older.is_some(),
            &g.settings.copy_update_asked,
            &version,
        ) && !updating;
        drop(g);
        if updating {
            return;
        }
        // An update brings the bundled OBS, never the copy, which only an administrator
        // changes: the app makes it again itself, once a version.
        if let (true, Some(was)) = (by_itself, older.as_ref()) {
            self.update_copy_by_itself(was.clone(), version);
            return;
        }
        if let Some(was) = older {
            tracing::warn!(copy = %was, wanted, "the copy of OBS for games that run as administrator is not the OBS this reel brought: Update the copy");
            self.tell_once(
                &format!("The OBS for games that run as administrator is {was}"),
                &format!("This version of reel brought OBS {wanted}. Update the copy under Settings, OBS; Windows asks once."),
            );
        }
    }

    /// Makes the copy again once no game is in front; this version is noted first, so a No, a
    /// failure or a quit asks again only after the next update.
    fn update_copy_by_itself(&self, was: String, version: String) {
        let wanted = reel_desktop_core::bundled::pinned().version;
        let note = |rt: &Runtime, words: Option<String>| {
            if let Some(b) = rt.lock().obs_box.as_ref() {
                b.note(words);
            }
            rt.push_status();
        };
        self.lock().copy_updating = true;
        tracing::info!(copy = %was, wanted, "the copy for games that run as administrator is updated by the app itself, Windows asking once");
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            let fg = rt.lock().foreground.clone();
            let told = rt.clone();
            let waited = tokio::task::spawn_blocking(move || {
                reel_desktop_core::gaming::wait_while_fullscreen(&*fg, Duration::from_secs(5), || {
                    tracing::info!("a fullscreen app is in front: the copy is updated once it is gone");
                    note(&told, Some(format!("reel updates the copy to OBS {} once no game is in front; Windows asks once", reel_desktop_core::bundled::pinned().version)));
                })
            })
            .await
            .unwrap_or(false);
            let lane = rt.lane.clone();
            let mark = rt.clone();
            let at = version.clone();
            let _ = tokio::task::spawn_blocking(move || {
                lane.ask(reel_desktop_core::lane::Ask::Run(Box::new(move || {
                    let mut g = mark.lock();
                    g.settings.copy_update_asked = at;
                    let file = g.paths.settings_file();
                    if let Err(e) = g.settings.save(&file) {
                        tracing::warn!(error = %e, "the version that asked was not kept");
                    }
                })))
            })
            .await;
            note(
                &rt,
                Some(format!(
                    "reel updates the copy to OBS {wanted}; Windows asks once"
                )),
            );
            tracing::info!(waited, "the copy: Windows is asked now");
            let lane = rt.lane.clone();
            let done =
                tokio::task::spawn_blocking(move || lane.ask(reel_desktop_core::lane::Ask::Again))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map(|_| ()));
            rt.lock().copy_updating = false;
            match done {
                Ok(()) => {
                    tracing::info!(from = %was, to = %wanted, "the copy for games that run as administrator is updated; OBS starts again from it");
                    note(&rt, None);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "the copy for games that run as administrator was not updated; reel asks again after its next update");
                    note(&rt, Some(format!("the copy was not updated ({e}): Settings, OBS, Update the copy, or reel asks again after its next update")));
                    rt.tell_once(
                        &format!("The OBS for games that run as administrator is {was}"),
                        &format!("This version of reel brought OBS {wanted}. Update the copy under Settings, OBS; Windows asks once."),
                    );
                }
            }
        });
    }

    /// Removes a copy, or its task, left in Program Files with the switch off.
    pub fn copy_remove(&self) -> Result<(), String> {
        let lane = self.lane.clone();
        tokio::task::block_in_place(|| lane.ask(reel_desktop_core::lane::Ask::Remove)).map(|_| ())
    }

    /// Makes the copy again from the OBS this version brought.
    pub fn copy_again(&self) -> Result<(), String> {
        let lane = self.lane.clone();
        tokio::task::block_in_place(|| lane.ask(reel_desktop_core::lane::Ask::Again)).map(|_| ())
    }

    fn restart_bundled(&self) {
        let s = self.settings();
        if let Some(bx) = self.lock().obs_box.clone() {
            bx.apply(
                s.obs.setup.clone(),
                s.clips_dir.clone(),
                s.hotkeys.save.clone(),
                None,
            );
        }
    }

    /// Takes what OBS has into the settings in the lane's turn, so a step never runs on
    /// settings a reading changes under it.
    async fn take_from_obs(&self, a: reel_desktop_core::readback::Taken, read_with: Settings) {
        let rt = self.clone();
        let lane = self.lane.clone();
        let take = tokio::task::spawn_blocking(move || {
            lane.ask(reel_desktop_core::lane::Ask::Run(Box::new(move || {
                rt.take_now(a, &read_with)
            })))
        });
        let _ = tokio::time::timeout(Duration::from_secs(2), take).await;
    }

    fn take_now(&self, a: reel_desktop_core::readback::Taken, read_with: &Settings) {
        let before = self.settings();
        if &before != read_with {
            tracing::debug!("a reading of OBS dropped: the settings changed while it ran");
            return;
        }
        let mut s = before.clone();
        s.obs.setup = a.setup;
        if let Some(d) = &a.clips_dir {
            s.clips_dir = PathBuf::from(d);
        }
        if let Some(k) = &a.save_key {
            s.hotkeys.save = k.clone();
        }
        if s == before {
            return;
        }
        tracing::info!(read = %a.words.join("; "), "read from OBS");
        self.keep_setup(&s);
        {
            use tauri::Emitter;
            let _ = self.app.emit("settings", &s);
        }
        if s.clips_dir != before.clips_dir {
            self.restart_watcher();
        }
        if s.hotkeys != before.hotkeys {
            self.register_hotkeys();
        }
    }

    /// Reads what OBS has, the running OBS's answers over the files it saved; returns what
    /// differed from the settings, in words.
    pub async fn read_obs(&self) -> Vec<String> {
        let (b, copy, s, restarting) = {
            let g = self.lock();
            let restarting = g
                .obs_box
                .as_ref()
                .is_some_and(|b| matches!(b.line().state.as_str(), "starting" | "restarting"));
            (
                g.bundled.clone(),
                g.copy.clone(),
                g.settings.clone(),
                restarting,
            )
        };
        // An OBS about to start again with a change of reel's still has the old value, and one
        // a step in Program Files stops is not read.
        let Some(b) = b.filter(|_| s.obs.bundled && !restarting && self.lane.idle()) else {
            return Vec::new();
        };
        let obs = match copy {
            Some(c)
                if s.obs.setup.elevated && c.exe.is_file() && !self.obs_runs_without_rights() =>
            {
                c
            }
            _ => b,
        };
        let (profile, live) = if self.lock().obs.connected {
            (self.profile_now().await, Some(self.live_sources().await))
        } else {
            (None, None)
        };
        let (setup, clips, key) = (
            s.obs.setup.clone(),
            s.clips_dir.clone(),
            s.hotkeys.save.clone(),
        );
        let got = tokio::task::spawn_blocking(move || {
            obs.read(&setup, &clips, &key, profile.as_deref(), live.as_ref())
        })
        .await
        .ok()
        .flatten();
        match got {
            Some(a) => {
                let words = a.words.clone();
                self.take_from_obs(a, s).await;
                words
            }
            None => Vec::new(),
        }
    }

    /// The profile's keys reel reads, as the running OBS answers them; None when it does not.
    async fn profile_now(&self) -> Option<String> {
        use reel_desktop_core::readback::{profile_text, PROFILE_KEYS};
        let mut got = Vec::new();
        for (section, key) in PROFILE_KEYS {
            let v = self
                .obs_request(
                    "GetProfileParameter",
                    serde_json::json!({ "parameterCategory": section, "parameterName": key }),
                )
                .await
                .ok()?;
            if let Some(value) = v["parameterValue"].as_str() {
                got.push((*section, *key, value.to_string()));
            }
        }
        Some(profile_text(&got))
    }

    async fn live_sources(&self) -> reel_desktop_core::readback::Live {
        use reel_desktop_core::readback::{Live, LiveSource, SOURCES};
        let mut live = Live::default();
        for name in SOURCES {
            let input = serde_json::json!({ "inputName": name });
            let Ok(v) = self.obs_request("GetInputSettings", input.clone()).await else {
                continue;
            };
            let mut l = LiveSource {
                settings: Some(v["inputSettings"].clone()),
                ..Default::default()
            };
            if let Ok(v) = self
                .obs_request(
                    "GetSourceFilterList",
                    serde_json::json!({ "sourceName": name }),
                )
                .await
            {
                l.filters = v["filters"].as_array().cloned();
            }
            if let Ok(v) = self.obs_request("GetInputVolume", input.clone()).await {
                l.volume_db = v["inputVolumeDb"].as_f64();
            }
            if let Ok(v) = self.obs_request("GetInputMute", input).await {
                l.muted = v["inputMuted"].as_bool();
            }
            live.sources.insert(name.to_string(), l);
        }
        live
    }

    /// Hides OBS's window if the app showed it, and reads what OBS has.
    pub async fn obs_done(&self) -> Vec<String> {
        self.show_bundled(false);
        self.read_obs().await
    }

    /// The setup changed by what OBS already has: kept for the next start, OBS left running.
    fn keep_setup(&self, s: &Settings) {
        {
            let mut g = self.lock();
            let file = g.paths.settings_file();
            if let Err(e) = s.save(&file) {
                tracing::warn!(error = %e, "the settings were not saved");
            }
            g.settings = s.clone();
        }
        if let Some(bx) = self.lock().obs_box.clone() {
            bx.update(
                s.obs.setup.clone(),
                s.clips_dir.clone(),
                s.hotkeys.save.clone(),
            );
        }
        self.push_status();
    }

    /// An OBS with the highest rights is ended by its task without saving, so the volumes and
    /// the microphone's filters go to it again once it answers.
    async fn live_again(&self) {
        let s = self.settings().obs.setup;
        if !(s.elevated && cfg!(windows)) {
            return;
        }
        for (input, db) in &s.volumes {
            if let Err(e) = self.obs_volume(input, *db).await {
                tracing::warn!(error = %e, input, "a volume in OBS");
            }
        }
        if s.mic.is_some() {
            if let Err(e) = self.apply_mic_filters(&s).await {
                tracing::warn!(error = %e, "the microphone's filters in OBS");
            }
        }
    }

    /// What the bundled OBS offers: NVIDIA's noise removal registers only where its effects
    /// are installed on an RTX card.
    async fn read_filter_kinds(&self) {
        let Ok(v) = self
            .obs_request("GetSourceFilterKindList", serde_json::json!({}))
            .await
        else {
            return;
        };
        let nvidia = v["sourceFilterKinds"]
            .as_array()
            .map(|a| a.iter().any(|k| k == "nvidia_audiofx_filter"))
            .unwrap_or(false);
        self.lock().nvidia_noise = Some(nvidia);
        tracing::info!(nvidia, "the bundled OBS's filters");
        if let Ok(v) = self
            .obs_request(
                "GetInputKindList",
                serde_json::json!({ "unversioned": true }),
            )
            .await
        {
            let has = |k: &str| {
                v["inputKinds"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|x| x == k))
            };
            let audio = has("pipewire_audio_application_capture");
            let game = has("vkcapture-source");
            {
                let mut g = self.lock();
                g.obs_app_audio = Some(audio);
                g.obs_game_capture = Some(game);
            }
            tracing::info!(
                app_audio = audio,
                game_capture = game,
                "the bundled OBS's sources"
            );
            self.push_status();
        }
        if let Ok(v) = self
            .obs_request(
                "GetSourceFilterList",
                serde_json::json!({ "sourceName": "Microphone" }),
            )
            .await
        {
            let on: Vec<String> = v["filters"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|f| {
                            format!(
                                "{} ({})",
                                f["filterName"].as_str().unwrap_or(""),
                                f["filterKind"].as_str().unwrap_or("")
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            tracing::info!(filters = %on.join(", "), "the microphone's filters in OBS");
        }
        let s = self.settings().obs.setup;
        if !nvidia && s.noise == reel_desktop_core::bundled::Noise::Nvidia && s.mic.is_some() {
            if let Err(e) = self.apply_mic_filters(&s).await {
                tracing::warn!(error = %e, "Strong in place of NVIDIA's noise removal");
            }
        }
        self.push_status();
    }

    /// Stops the bundled OBS and waits for it; false when there was none.
    pub fn stop_bundled(&self) -> bool {
        let bx = self.lock().obs_box.take();
        let Some(bx) = bx else {
            return false;
        };
        tracing::info!("stopping the bundled OBS");
        // From a task (SIGTERM, an update), waiting would hold the worker thread the
        // supervisor is woken on, until the wait gives up and OBS is left running.
        match tokio::runtime::Handle::try_current() {
            Ok(h) if h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
                tokio::task::block_in_place(|| bx.stop())
            }
            _ => bx.stop(),
        }
        true
    }

    pub fn show_bundled(&self, shown: bool) {
        if let Some(bx) = self.lock().obs_box.clone() {
            bx.show(shown);
        }
    }

    /// Open OBS or Hide OBS; once hidden again, what OBS has is read.
    pub fn show_bundled_and_read(&self, shown: bool) {
        self.show_bundled(shown);
        if !shown {
            let rt = self.clone();
            tauri::async_runtime::spawn(async move {
                rt.read_obs().await;
            });
        }
    }

    pub fn restart_obs(&self) {
        let s = self.settings();
        let bundled = self.bundled_in_use();
        let mut g = self.lock();
        if let Some(old) = g.obs_cancel.take() {
            old.cancel();
        }
        g.obs_commands = None;
        if !s.obs.enabled && !bundled {
            g.obs = ObsLine {
                connected: false,
                line: "off".into(),
            };
            drop(g);
            return;
        }
        let cancel = CancellationToken::new();
        g.obs_cancel = Some(cancel.clone());
        let cfg = if bundled {
            ObsConfig {
                host: "127.0.0.1".into(),
                port: reel_desktop_core::bundled::PORT,
                password: s.obs.setup.password.clone(),
            }
        } else {
            ObsConfig::from(&s.obs)
        };
        g.obs = ObsLine {
            connected: false,
            line: if bundled {
                "connecting to the OBS that came with reel".into()
            } else {
                format!("connecting to {}:{}", s.obs.host, s.obs.port)
            },
        };
        let (tx, mut rx) = mpsc::channel::<ObsEvent>(64);
        let (ctx, crx) = mpsc::channel::<ObsCommand>(8);
        g.obs_commands = Some(ctx);
        let ready = g.obs_box.as_ref().filter(|_| bundled).map(|b| b.ready());
        drop(g);
        tauri::async_runtime::spawn(reel_desktop_core::obs::run(cfg, tx, crx, cancel, ready));
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(ev) = rx.recv().await {
                match ev {
                    ObsEvent::Connected {
                        obs_version,
                        ws_version,
                    } => {
                        rt.lock().obs = ObsLine {
                            connected: true,
                            line: format!("OBS {obs_version}, websocket {ws_version}"),
                        };
                        tracing::info!(obs_version, ws_version, "obs connected");
                        if bundled {
                            if let Some(bx) = rt.lock().obs_box.clone() {
                                bx.answered(true);
                            }
                        }
                        rt.push_status();
                        rt.obs_connected();
                    }
                    ObsEvent::Disconnected { reason } => {
                        {
                            let mut g = rt.lock();
                            g.obs = ObsLine {
                                connected: false,
                                line: format!("not connected: {reason}"),
                            };
                            g.obs_state = None;
                            g.obs_game = None;
                            if bundled {
                                if let Some(bx) = g.obs_box.clone() {
                                    bx.answered(false);
                                }
                            }
                        }
                        rt.push_status();
                    }
                    ObsEvent::Meters { levels } => {
                        use tauri::Emitter;
                        let _ = rt.app.emit("obs-meters", levels);
                    }
                    ObsEvent::Game { game } => {
                        tracing::info!(game = ?game, "obs game capture");
                        rt.lock().obs_game = game;
                    }
                    ObsEvent::SceneChanged => {}
                    ObsEvent::ReplayBufferState { active } => {
                        let st = {
                            let mut g = rt.lock();
                            if let Some(st) = g.obs_state.as_mut() {
                                st.replay_active = active;
                            }
                            g.obs_state.clone()
                        };
                        tracing::info!(active, "obs replay buffer");
                        rt.push_status();
                        if let Some(st) = st.filter(|_| active && rt.lock().recording_paused) {
                            let rt = rt.clone();
                            tauri::async_runtime::spawn(async move {
                                rt.ensure_replay_buffer(&st).await;
                            });
                        }
                    }
                    ObsEvent::RecordState { active, saved } => {
                        if let Some(st) = rt.lock().obs_state.as_mut() {
                            st.recording = active;
                        }
                        tracing::info!(active, "obs recording");
                        if let Some(path) = saved {
                            let p = PathBuf::from(&path);
                            tracing::info!(path = %path, "obs saved a file");
                            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                            rt.on_found(&p, size, false, "obs");
                        }
                        rt.push_status();
                    }
                    ObsEvent::ReplayBufferSaved { path } => {
                        let p = PathBuf::from(&path);
                        tracing::info!(path = %path, "obs saved a replay");
                        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                        let waiting = {
                            let mut g = rt.lock();
                            g.save_problem = None;
                            // OBS writes one replay a save, in turn: this one is the oldest
                            // save's still waiting
                            while g.save_waits.front().is_some_and(|(tx, _)| tx.is_closed()) {
                                g.save_waits.pop_front();
                            }
                            let waiting = g.save_waits.pop_front();
                            if let Some((_, Some(share))) = &waiting {
                                g.waited.insert(p.clone(), share.clone());
                            }
                            waiting
                        };
                        rt.on_found(&p, size, false, "obs");
                        if let Some((tx, _)) = waiting {
                            let _ = tx.send(p);
                        }
                        rt.push_status();
                    }
                }
            }
        });
    }

    /// The Test button's answer: the versions, or plain words with the OS line under them.
    pub async fn obs_test(
        host: &str,
        port: u16,
        password: &str,
    ) -> Result<String, (String, Option<String>)> {
        let cfg = ObsConfig {
            host: host.to_string(),
            port,
            password: password.to_string(),
        };
        match reel_desktop_core::obs::test_connection(&cfg).await {
            Ok(v) => Ok(format!(
                "OBS {} with obs-websocket {}",
                v.obs_version, v.ws_version
            )),
            Err(e) => Err(e.explain(host, port)),
        }
    }

    /// Sends one command to the OBS loop and waits for its answer; the words of an error
    /// are the Test button's, with the detail after them.
    async fn obs_command<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<T, ObsError>>) -> ObsCommand,
    ) -> Result<T, String> {
        let (tx, host, port) = {
            let g = self.lock();
            (
                g.obs_commands.clone(),
                g.settings.obs.host.clone(),
                g.settings.obs.port,
            )
        };
        let not_connected = || "OBS is not connected".to_string();
        let tx = tx.ok_or_else(not_connected)?;
        if !self.lock().obs.connected {
            return Err(not_connected());
        }
        let (reply, rx) = oneshot::channel();
        tx.send(make(reply)).await.map_err(|_| not_connected())?;
        let answer = tokio::time::timeout(Duration::from_secs(20), rx)
            .await
            .map_err(|_| "OBS did not answer in time".to_string())?
            .map_err(|_| not_connected())?;
        answer.map_err(|e| {
            let (words, detail) = e.explain(&host, port);
            match detail {
                Some(d) => format!("{words} {d}"),
                None => words,
            }
        })
    }

    fn obs_connected(&self) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            if rt.bundled_in_use() {
                rt.read_filter_kinds().await;
                rt.live_again().await;
                rt.read_obs().await;
            }
            match rt.obs_state(true).await {
                Ok(Some(st)) => {
                    rt.ensure_replay_buffer(&st).await;
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(error = %e, "OBS state not read"),
            }
        });
    }

    /// What OBS is set to: the last reading, or a fresh one when asked and connected.
    pub async fn obs_state(&self, refresh: bool) -> Result<Option<ObsState>, String> {
        if !self.lock().obs.connected {
            return Ok(None);
        }
        if !refresh {
            if let Some(st) = self.lock().obs_state.clone() {
                return Ok(Some(st));
            }
        }
        let st = self.obs_command(ObsCommand::State).await?;
        self.after_obs_state(&st);
        Ok(Some(st))
    }

    fn after_obs_state(&self, st: &ObsState) {
        let follow = {
            let mut g = self.lock();
            g.obs_state = Some(st.clone());
            let dir = (!st.record_dir.is_empty()).then(|| PathBuf::from(&st.record_dir));
            let changed = dir.is_some() && g.obs_dir != dir;
            if dir.is_some() {
                g.obs_dir = dir;
            }
            changed && g.settings.clips_follow_obs
        };
        if follow {
            tracing::info!(dir = %st.record_dir, "the clips folder follows OBS");
            self.restart_watcher();
        }
        self.push_status();
    }

    /// Writes a change into OBS and reads the state again.
    pub async fn obs_apply(&self, change: ObsChange) -> Result<Vec<String>, String> {
        let lines = self
            .obs_command(|reply| ObsCommand::Apply(change, reply))
            .await?;
        for l in &lines {
            tracing::info!(what = %l, "OBS changed");
        }
        let _ = self.obs_state(true).await;
        Ok(lines)
    }

    /// The replay buffer as Pause recording and the setting "start the replay buffer" want it:
    /// stopped while recording is paused, else started when it is off and OBS is not recording.
    pub async fn ensure_replay_buffer(&self, st: &ObsState) {
        let paused = self.lock().recording_paused;
        match crate::status::buffer_step(paused, self.settings().obs.start_replay, st) {
            BufferStep::Start => self.start_replay_buffer(st).await,
            BufferStep::Stop => self.stop_replay_buffer().await,
            BufferStep::Leave => {}
        }
    }

    async fn stop_replay_buffer(&self) {
        match self.obs_command(ObsCommand::StopReplayBuffer).await {
            Ok(()) => tracing::info!("the replay buffer was stopped: recording is paused"),
            Err(e) => tracing::warn!(error = %e, "the replay buffer did not stop"),
        }
        let _ = self.obs_state(true).await;
    }

    /// Enables the replay buffer in OBS's profile if needed, and starts it.
    async fn start_replay_buffer(&self, st: &ObsState) {
        if !st.replay_enabled {
            let change = ObsChange {
                replay_enabled: Some(true),
                ..ObsChange::default()
            };
            if let Err(e) = self
                .obs_command(|reply| ObsCommand::Apply(change, reply))
                .await
            {
                tracing::warn!(error = %e, "the replay buffer could not be enabled");
                return;
            }
        }
        match self.obs_command(ObsCommand::StartReplayBuffer).await {
            Ok(()) => tracing::info!("the replay buffer was started by the app"),
            Err(e) => tracing::warn!(error = %e, "the replay buffer did not start"),
        }
        let _ = self.obs_state(true).await;
    }

    pub fn on_found(&self, path: &Path, size: u64, backlog: bool, via: &str) {
        let mode;
        let is_new;
        {
            let mut g = self.lock();
            let known = g.ledger.get(path).is_some();
            is_new = !known;
            if !known {
                let game = g.obs_game.clone().or_else(|| {
                    g.foreground
                        .foreground_process()
                        .and_then(|p| gaming::game_name(&p.exe_name, &p.title))
                });
                let now = jiff::Timestamp::now();
                let title = if backlog {
                    None
                } else {
                    Some(gaming::clip_title(game.as_deref(), now))
                };
                if let Err(e) = g.ledger.found(path, size) {
                    tracing::warn!(error = %e, "ledger");
                }
                let recorded = if backlog { None } else { Some(now) };
                let _ = g
                    .ledger
                    .set_meta(path, game.clone(), title.clone(), recorded);
                tracing::info!(path = %path.display(), size, via, backlog, game = ?game, "clip found");
            }
            mode = g.settings.upload.mode;
        }
        if !is_new {
            return;
        }
        if !backlog && !self.was_cued() {
            self.cue(Cue::Captured);
        }
        if self.is_standalone() {
            self.ingest_locally(path);
        } else if mode == UploadMode::Auto {
            self.enqueue(path);
        }
        self.push_status();
    }

    pub fn enqueue(&self, path: &Path) {
        let now = {
            let mut g = self.lock();
            match g.ledger.get(path).map(|c| c.state.clone()) {
                Some(ClipState::Confirmed) | Some(ClipState::Deleted) => return,
                Some(ClipState::Queued) | Some(ClipState::Uploading) => return,
                None => return,
                _ => {}
            }
            let _ = g.ledger.mark_queued(path);
            g.waited.contains_key(path)
        };
        if now {
            let (rt, p) = (self.clone(), path.to_path_buf());
            tauri::async_runtime::spawn(async move { rt.upload_one(&p).await });
        } else {
            let _ = self.queue_tx.send(path.to_path_buf());
        }
        self.push_status();
    }

    fn requeue_pending(&self) {
        let pending: Vec<(PathBuf, ClipState)> = self
            .lock()
            .ledger
            .pending()
            .iter()
            .map(|c| (c.path.clone(), c.state.clone()))
            .collect();
        let mode = self.settings().upload.mode;
        let standalone = self.is_standalone();
        for (p, state) in pending {
            if !p.exists() {
                continue;
            }
            match state {
                // taken into the library once the local reel is up (start_local)
                ClipState::Found if standalone => {}
                ClipState::Found if mode == UploadMode::Auto => self.enqueue(&p),
                ClipState::Queued | ClipState::Uploading => {
                    self.back_to_found(&p);
                    self.enqueue(&p);
                }
                _ => {}
            }
        }
        let _ = self.lock().ledger.forget_missing();
    }

    pub fn back_to_found(&self, path: &Path) {
        let mut g = self.lock();
        if let Some(c) = g.ledger.get_mut(path) {
            c.state = ClipState::Found;
        }
        if let Err(e) = g.ledger.save() {
            tracing::warn!(error = %e, "ledger");
        }
    }

    pub fn upload_last(&self) {
        if self.not_here("Upload the last clip") {
            return;
        }
        let last = self
            .lock()
            .ledger
            .recent(50)
            .iter()
            .find(|c| matches!(c.state, ClipState::Found | ClipState::Failed { .. }))
            .map(|c| c.path.clone());
        match last {
            Some(p) => {
                self.back_to_found(&p);
                self.enqueue(&p);
            }
            None => notify::show(&self.app, "Nothing to upload", "No clip is waiting."),
        }
    }

    fn spawn_upload_worker(&self, mut rx: mpsc::UnboundedReceiver<PathBuf>) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(path) = rx.recv().await {
                rt.upload_one(&path).await;
            }
        });
    }

    async fn upload_one(&self, path: &Path) {
        let api = loop {
            if let Some(a) = self.api() {
                break a;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
            if self.lock().ledger.get(path).is_none() {
                return;
            }
        };
        let (meta, bandwidth, chunk, waited) = {
            let mut g = self.lock();
            let Some(rec) = g.ledger.get(path).cloned() else {
                return;
            };
            if !matches!(rec.state, ClipState::Queued) {
                return;
            }
            let _ = g.ledger.mark_uploading(path, None);
            let waited = g.waited.contains_key(path).then(|| {
                let (tx, rx) = watch::channel(false);
                g.waited_running.insert(path.to_path_buf(), tx);
                rx
            });
            g.uploading = Some((path.to_path_buf(), rec.name.clone(), 0, rec.size));
            g.last_error = None;
            (
                UploadMeta {
                    file_name: None,
                    recorded_at: rec.recorded_at.map(|t| t.to_string()),
                    game: rec.game.clone(),
                    title: rec.title.clone(),
                    keep_last: Some(g.settings.upload.keep_last).filter(|s| *s > 0),
                    audio_names: track_names(g.bundled.as_ref(), &g.settings),
                },
                g.settings.upload.bandwidth_kbit,
                g.settings.upload.chunk_mb,
                waited,
            )
        };
        self.update_paused();
        self.push_status();
        let shared = waited.is_some();
        let mut ctl = match waited {
            Some(own) => {
                tracing::info!(path = %path.display(), "a waited upload: past the queue and the pause");
                Control::new(own)
            }
            None => Control::new(self.paused_tx.subscribe()),
        };
        ctl.bandwidth_kbit = bandwidth;
        ctl.chunk_bytes = if chunk > 0 {
            Some(chunk.min(90) * 1024 * 1024)
        } else {
            None
        };
        let (tx, mut prx) = mpsc::unbounded_channel::<Progress>();
        let rt = self.clone();
        let p = path.to_path_buf();
        let progress_task = tauri::async_runtime::spawn(async move {
            let mut last = std::time::Instant::now() - Duration::from_secs(1);
            while let Some(pr) = prx.recv().await {
                {
                    let mut g = rt.lock();
                    if let Some(u) = g.uploading.as_mut().filter(|u| u.0 == p) {
                        match &pr {
                            Progress::Uploading { sent, total }
                            | Progress::Paused { sent, total } => {
                                u.2 = *sent;
                                u.3 = *total;
                            }
                            _ => {}
                        }
                    }
                    if let Progress::Retrying { reason, in_secs } = &pr {
                        g.last_error = Some(format!("{reason}; again in {in_secs} s"));
                    }
                }
                if last.elapsed() > Duration::from_millis(400) {
                    last = std::time::Instant::now();
                    rt.push_status();
                }
            }
        });
        let result = uploader::upload_file(&api, path, meta, &ctl, tx).await;
        let _ = progress_task.await;
        if shared {
            let mut g = self.lock();
            g.waited.remove(path);
            g.waited_running.remove(path);
        }
        self.update_paused();
        let title;
        match result {
            Ok(out) => {
                {
                    let mut g = self.lock();
                    let _ = g.ledger.mark_confirmed(path, &out.clip_id, out.deduped);
                    if g.uploading.as_ref().is_some_and(|u| u.0 == path) {
                        g.uploading = None;
                    }
                    g.last_error = None;
                    title = g
                        .ledger
                        .get(path)
                        .and_then(|c| c.title.clone())
                        .unwrap_or(out.title.clone());
                }
                tracing::info!(path = %path.display(), clip = %out.clip_id, deduped = out.deduped, "upload confirmed");
                // a Save and share's clip has its link copied next, with that sound
                if !shared {
                    self.cue(Cue::Uploaded);
                    if self.is_standalone() {
                        // Upload to the server from the local library: no link here
                        notify::show(
                            &self.app,
                            "Clip uploaded",
                            &format!("{title} is on the server too."),
                        );
                    } else {
                        let hk = self.settings().hotkeys.share_last;
                        notify::show_clip(
                            &self.app,
                            "Clip uploaded",
                            &format!("{title}. {hk} copies the link; the tray menu has it too."),
                            &out.clip_id,
                            true,
                        );
                    }
                }
                self.sweep_cache();
                let local_id = self
                    .lock()
                    .ledger
                    .get(path)
                    .and_then(|c| c.local_clip_id.clone());
                if let (Some(local), Some(lid)) = (self.local(), local_id) {
                    let server = self.settings().server_url;
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = local.mark_uploaded(&lid, &server).await {
                            tracing::warn!(error = %e, "standalone: the upload was not noted in the library");
                        }
                    });
                }
                self.watch_server(out.clip_id.clone(), title, path.to_path_buf());
            }
            Err(e) => {
                let reason = e.to_string();
                {
                    let mut g = self.lock();
                    let _ = g.ledger.mark_failed(path, &reason);
                    if g.uploading.as_ref().is_some_and(|u| u.0 == path) {
                        g.uploading = None;
                    }
                    if matches!(e, uploader::UploadError::Unauthorized) {
                        g.api = None;
                        g.who = None;
                        g.pair_line = Some(reason.clone());
                    } else {
                        g.last_error = Some(reason.clone());
                    }
                }
                tracing::warn!(path = %path.display(), error = %reason, "upload failed");
                notify::show(&self.app, "Upload failed", &reason);
            }
        }
        self.push_status();
    }

    /// After the server has the whole file: follow it through publishing and the preview
    /// video, so the tray and the Status page show the phase instead of a stuck percentage.
    fn watch_server(&self, clip_id: String, title: String, path: PathBuf) {
        let Some(api) = self.api() else {
            return;
        };
        {
            let mut g = self.lock();
            g.processing.retain(|p| p.clip_id != clip_id);
            g.processing.push(Processing {
                clip_id: clip_id.clone(),
                title: title.clone(),
                phase: "on the server".into(),
            });
        }
        self.push_status();
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            let (ctl, _tx) = Control::never_paused();
            let rt2 = rt.clone();
            let id = clip_id.clone();
            let r = uploader::watch_clip(&api, &clip_id, &ctl, move |phase| {
                let line = match phase {
                    uploader::ServerPhase::Publishing { .. } => "on the server".to_string(),
                    uploader::ServerPhase::Preview { progress: Some(p) } => format!(
                        "making the preview video, {}%",
                        (p.clamp(0.0, 1.0) * 100.0).round() as i64
                    ),
                    uploader::ServerPhase::Preview { progress: None } => {
                        "making the preview video".to_string()
                    }
                };
                let changed = {
                    let mut g = rt2.lock();
                    match g.processing.iter_mut().find(|p| p.clip_id == id) {
                        Some(p) if p.phase != line => {
                            p.phase = line;
                            true
                        }
                        _ => false,
                    }
                };
                if changed {
                    rt2.push_status();
                }
            })
            .await;
            {
                let mut g = rt.lock();
                g.processing.retain(|p| p.clip_id != clip_id);
                if let Err(uploader::UploadError::Failed(reason)) = &r {
                    let line = format!("{title}: the server could not read it: {reason}");
                    if g.ledger.get(&path).map(|c| c.is_local()).unwrap_or(false) {
                        let _ = g.ledger.mark_failed(&path, &line);
                    }
                    g.last_error = Some(line);
                }
            }
            match &r {
                Ok(_) => tracing::info!(clip = %clip_id, "the clip is ready to share"),
                Err(e) => {
                    tracing::warn!(clip = %clip_id, error = %e, "the server did not finish the clip")
                }
            }
            rt.push_status();
        });
    }

    pub fn is_standalone(&self) -> bool {
        self.lock().settings.mode == Mode::Standalone
    }

    fn local(&self) -> Option<Arc<Local>> {
        self.lock().local.clone()
    }

    /// The library cap's line for the standalone library at `gb`; none while the local reel is
    /// not running.
    pub async fn cap_line(&self, gb: u64) -> Option<String> {
        let local = self.local()?;
        let keeps = self.lock().settings.standalone.cap_keeps_unsent;
        let rows = match local.ctx.store.cap_rows().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "the library cap's line");
                return None;
            }
        };
        let plan = reel_pipeline::cap::plan(&rows, gb.min(100_000) * 1_000_000_000, keeps);
        Some(reel_pipeline::cap::line(
            &plan,
            local.ctx.config().share.trash_days,
        ))
    }

    fn local_settings(&self) -> LocalSettings {
        let g = self.lock();
        LocalSettings {
            data_dir: g.settings.library_dir(&g.paths),
            hwaccel: g.settings.standalone.hwaccel.clone(),
            cap_gb: g.settings.standalone.cap_gb,
            cap_keeps_unsent: g.settings.standalone.cap_keeps_unsent,
            copies_on_hardware: g.settings.standalone.copies_on_hardware,
        }
    }

    /// Standalone: reel in this process; the library webview goes to it once it is up.
    pub fn start_local(&self) {
        if self.lock().local.is_some() {
            return;
        }
        if self.lock().moving {
            self.lock().local_line = "the library is moving; reel on this PC waits".into();
            self.push_status();
            return;
        }
        if let Some(why) = self.library_away_now() {
            let waiting = self.lock().library_away.replace(why.clone()).is_some();
            self.lock().local_line = why.clone();
            tracing::warn!(why, "standalone: reel on this PC waits for its library");
            self.push_status();
            if !waiting {
                self.wait_for_library();
            }
            return;
        }
        let s = self.local_settings();
        self.lock().local_line = "starting reel on this PC".into();
        self.push_status();
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            match Local::start(&s).await {
                Ok(local) => {
                    let local = Arc::new(local);
                    let url = local.url.clone();
                    {
                        let mut g = rt.lock();
                        g.local_line = format!("reel on {} with {}", local.addr, local.backend);
                        local.hold(g.paused_fullscreen);
                        g.local = Some(local.clone());
                    }
                    rt.watch_library(local);
                    let _ = url;
                    if rt.is_standalone() {
                        if std::mem::take(&mut rt.lock().library_wanted) {
                            rt.go_library("/");
                        } else {
                            rt.library_changed();
                        }
                        rt.take_in_found();
                    }
                    rt.push_status();
                }
                Err(e) => {
                    tracing::error!(error = %e, "standalone: reel did not start");
                    {
                        let mut g = rt.lock();
                        g.local_line = format!("reel did not start: {e}");
                        g.last_error = Some(format!("standalone: {e}"));
                    }
                    rt.push_status();
                }
            }
        });
    }

    fn take_in_found(&self) {
        let found: Vec<PathBuf> = self
            .lock()
            .ledger
            .pending()
            .iter()
            .filter(|c| matches!(c.state, ClipState::Found) && c.local_clip_id.is_none() && !c.keep)
            .map(|c| c.path.clone())
            .collect();
        for p in found.into_iter().filter(|p| p.exists()) {
            tracing::info!(path = %p.display(), "standalone: a clip found before reel on this PC was up goes in now");
            self.ingest_locally(&p);
        }
    }

    /// Retry and Upload now: in standalone a clip goes into the local library again, and only a
    /// master for Upload to the server goes to the server's queue.
    pub fn take_again(&self, path: &Path) {
        let bridge = self.lock().ledger.get(path).is_some_and(|c| c.keep);
        self.back_to_found(path);
        if self.is_standalone() && !bridge {
            self.ingest_locally(path);
            self.push_status();
        } else {
            self.enqueue(path);
        }
    }

    /// Standalone has no link to make or copy, nor a queue for a server: such an action is
    /// refused with a word.
    pub fn not_here(&self, what: &str) -> bool {
        if !self.is_standalone() {
            return false;
        }
        tracing::info!(what, "standalone: not part of it, refused");
        notify::show(
            &self.app,
            &format!("{what} is not part of standalone"),
            "reel keeps the clips in the library on this PC, and makes no links; Save clip saves one there. A server can be added under Settings, Server.",
        );
        true
    }

    pub fn stop_local(&self) {
        let local = self.lock().local.take();
        if let Some(l) = local {
            l.stop();
        }
        self.lock().local_line = String::new();
    }

    /// Standalone: the clip goes into the library on this PC; "uploaded" is the moment it
    /// is published there.
    fn ingest_locally(&self, path: &Path) {
        let Some(local) = self.local() else {
            let mut g = self.lock();
            if g.moving || g.library_away.is_some() {
                tracing::info!(path = %path.display(), "standalone: the clip waits in the clips folder until reel on this PC is back");
            } else {
                g.last_error = Some(
                    "standalone: reel is not running yet; the clip waits in the folder".into(),
                );
            }
            return;
        };
        let (title, recorded, game, keep_last, names) = {
            let g = self.lock();
            let keep_last = Some(g.settings.upload.keep_last as f64).filter(|s| *s > 0.0);
            let names = track_names(g.bundled.as_ref(), &g.settings);
            match g.ledger.get(path) {
                Some(c) => (
                    c.title.clone(),
                    c.recorded_at.map(|t| t.to_string()),
                    c.game.clone(),
                    keep_last,
                    names,
                ),
                None => (None, None, None, keep_last, names),
            }
        };
        let rt = self.clone();
        let p = path.to_path_buf();
        tauri::async_runtime::spawn(async move {
            let id = match local
                .ingest(&p, title, recorded, game, keep_last, names)
                .await
            {
                Ok(id) => id,
                Err(e) => {
                    let _ = rt
                        .lock()
                        .ledger
                        .mark_failed(&p, &format!("standalone: {e}"));
                    rt.lock().last_error = Some(format!("standalone: {e}"));
                    rt.push_status();
                    return;
                }
            };
            let _ = rt.lock().ledger.mark_local(&p, &id);
            tracing::info!(path = %p.display(), clip = %id, "standalone: the clip is in the library's queue");
            rt.push_status();
            let start = std::time::Instant::now();
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                match local.ctx.store.clip(&id).await {
                    Ok(Some(c)) if c.state == "ready" => {
                        rt.cue(Cue::Uploaded);
                        notify::show_clip(&rt.app, "Clip in the library", &c.title, &id, false);
                        tracing::info!(clip = %id, "standalone: the clip is published");
                        shell::reload_library_if_home(&rt.app);
                        break;
                    }
                    Ok(Some(c)) if c.state == "failed" => {
                        let why = c
                            .error
                            .clone()
                            .unwrap_or_else(|| "reel could not read it".into());
                        let _ = rt.lock().ledger.mark_failed(&p, &why);
                        rt.lock().last_error = Some(format!("{}: {why}", c.title));
                        break;
                    }
                    Ok(Some(_)) if start.elapsed() < Duration::from_secs(600) => {}
                    _ => break,
                }
            }
            rt.push_status();
        });
    }

    /// Standalone with a server paired: the clip's master goes up as any clip would, and the
    /// library remembers the server once the upload is confirmed.
    pub fn upload_local(&self, local_clip_id: &str) -> Result<(), String> {
        let Some(local) = self.local() else {
            return Err("standalone: reel on this PC is not running".into());
        };
        if self.api().is_none() {
            return Err("pair the app with a server first (Server page)".into());
        }
        let rt = self.clone();
        let id = local_clip_id.to_string();
        tauri::async_runtime::spawn(async move {
            match local.master_of(&id).await {
                Ok(Some((master, title))) => {
                    {
                        let mut g = rt.lock();
                        let raw = g
                            .ledger
                            .clips
                            .iter()
                            .find(|c| c.local_clip_id.as_deref() == Some(id.as_str()))
                            .map(|c| c.path.clone());
                        let relinked = match &raw {
                            Some(r) => g.ledger.relink(r, &master).unwrap_or(false),
                            None => false,
                        };
                        if !relinked {
                            let size = std::fs::metadata(&master).map(|m| m.len()).unwrap_or(0);
                            let _ = g.ledger.found(&master, size);
                            if let Some(c) = g.ledger.get_mut(&master) {
                                c.keep = true;
                                c.local_clip_id = Some(id.clone());
                                c.title = Some(title.clone());
                            }
                            let _ = g.ledger.save();
                        }
                    }
                    tracing::info!(clip = %id, file = %master.display(), "standalone: the master goes up to the server");
                    rt.enqueue(&master);
                }
                Ok(None) => {
                    rt.lock().last_error = Some("that clip has no master file yet".into());
                    rt.push_status();
                }
                Err(e) => {
                    rt.lock().last_error = Some(format!("standalone: {e}"));
                    rt.push_status();
                }
            }
        });
        Ok(())
    }

    /// The Save hotkey: OBS saves the replay, and it goes up as every clip OBS saves does.
    pub fn save_clip(&self) {
        if self.double_tap() || self.recording_is_paused() {
            return;
        }
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            if !rt.lock().obs.connected {
                return rt.save_failed("OBS is not connected; Settings, OBS says why.");
            }
            let (tx, rx) = oneshot::channel();
            rt.lock().save_waits.push_back((tx, None));
            if let Err(e) = rt.obs_command(ObsCommand::SaveReplay).await {
                return rt.save_failed(&e);
            }
            rt.save_taken();
            tracing::info!("save: OBS saves the replay");
            match tokio::time::timeout(SAVE_WAIT, rx).await {
                Ok(Ok(path)) => {
                    tracing::info!(path = %path.display(), "save: the replay is saved")
                }
                _ => rt.save_failed(&rt.no_file_words()),
            }
        });
    }

    /// A save while recording is paused saves nothing and says so.
    fn recording_is_paused(&self) -> bool {
        if !self.lock().recording_paused {
            return false;
        }
        tracing::info!("no replay saved: recording is paused");
        notify::show(
            &self.app,
            "No replay saved",
            "Recording is paused; Resume recording in the tray or in the app's window records again.",
        );
        true
    }

    fn save_failed(&self, why: &str) {
        tracing::warn!(why, "no replay saved");
        self.lock().save_problem = Some(why.to_string());
        self.push_status();
        notify::show(&self.app, "No replay saved", why);
    }

    /// Why a save OBS took gave no file, and where OBS's own log says more.
    fn no_file_words(&self) -> String {
        let logs = {
            let g = self.lock();
            g.bundled
                .as_ref()
                .filter(|_| g.settings.obs.bundled)
                .map(|b| b.logs_dir().display().to_string())
        };
        let secs = SAVE_WAIT.as_secs();
        match logs {
            Some(dir) => format!("OBS took the save but wrote no replay within {secs} s; its log in {dir} says why."),
            None => format!("OBS took the save but wrote no replay within {secs} s; OBS's log (Help, Log Files) says why."),
        }
    }

    /// A press while one runs is a Save and share too, with its own sound: each saves its
    /// replay, and the newest's link is the one copied.
    pub fn save_and_share(&self) {
        if self.not_here("Save and share") || self.double_tap() || self.recording_is_paused() {
            return;
        }
        let token = CancellationToken::new();
        if let Some(older) = self.lock().newest_share.replace(token.clone()) {
            older.cancel();
        }
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            let phase = |rt: &Runtime, line: Option<&str>| {
                if !token.is_cancelled() {
                    rt.lock().preparing = line.map(String::from);
                    rt.push_status();
                }
            };
            let stop = |rt: &Runtime, words: &str| {
                tracing::warn!(why = words, "save and share stopped");
                phase(rt, None);
                rt.save_failed(words);
            };
            phase(&rt, Some("saving the replay in OBS"));
            let s = rt.settings();
            if !(s.obs.enabled || rt.bundled_in_use()) || !rt.lock().obs.connected {
                return stop(
                    &rt,
                    "OBS is not connected; the app reaches it under Settings, OBS.",
                );
            }
            match rt.obs_command(ObsCommand::ReplayBufferActive).await {
                Ok(true) => {}
                Ok(false) => {
                    let started = match rt.obs_state(true).await {
                        Ok(Some(st)) if s.obs.start_replay => {
                            rt.ensure_replay_buffer(&st).await;
                            rt.obs_command(ObsCommand::ReplayBufferActive)
                                .await
                                .unwrap_or(false)
                        }
                        _ => false,
                    };
                    let words = if started {
                        "The replay buffer was off and runs now; press again once the moment happened."
                    } else if s.obs.start_replay {
                        "The replay buffer is off and did not start (OBS may be recording); start it in OBS."
                    } else {
                        "The replay buffer is off; start it in OBS, or turn on Start the replay buffer under Settings, OBS."
                    };
                    return stop(&rt, words);
                }
                Err(e) => return stop(&rt, &e),
            }
            let (tx, rx) = oneshot::channel();
            rt.lock().save_waits.push_back((tx, Some(token.clone())));
            if let Err(e) = rt.obs_command(ObsCommand::SaveReplay).await {
                return stop(&rt, &e);
            }
            rt.save_taken();
            let path = match tokio::time::timeout(SAVE_WAIT, rx).await {
                Ok(Ok(p)) => p,
                _ => return stop(&rt, &rt.no_file_words()),
            };
            tracing::info!(path = %path.display(), "save and share: the replay is saved");
            notify::show(&rt.app, "Replay saved", "Uploading it; the link follows.");
            rt.enqueue(&path);
            phase(&rt, Some("uploading the replay"));
            let until = std::time::Instant::now() + Duration::from_secs(3600);
            let clip_id = loop {
                let rec = rt.lock().ledger.get(&path).cloned();
                match rec {
                    Some(c)
                        if matches!(c.state, ClipState::Confirmed | ClipState::Deleted)
                            && c.clip_id.is_some() =>
                    {
                        break c.clip_id.unwrap_or_default();
                    }
                    Some(c) => {
                        if let ClipState::Failed { reason } = &c.state {
                            phase(&rt, None);
                            notify::show(&rt.app, "The replay did not upload", reason);
                            return;
                        }
                    }
                    None => {}
                }
                if std::time::Instant::now() > until {
                    phase(&rt, None);
                    notify::show(
                        &rt.app,
                        "The replay did not upload",
                        "It took over an hour.",
                    );
                    return;
                }
                tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!(path = %path.display(), "save and share: a newer one was pressed, so this clip's link is not asked for");
                        return;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                }
            };
            tracing::info!(clip = %clip_id, "save and share: uploaded, the link next");
            let Some(api) = rt.api() else {
                return phase(&rt, None);
            };
            rt.copy_link(api, Some(clip_id), Some(token.clone())).await;
        });
    }

    pub fn share_last(&self, clip_id: Option<String>) {
        let what = if clip_id.is_some() {
            "Copy link"
        } else {
            "Share the last clip"
        };
        if self.not_here(what) {
            return;
        }
        let Some(api) = self.api() else {
            notify::show(
                &self.app,
                "Not paired",
                "Pair the app with the server first.",
            );
            return;
        };
        {
            let mut g = self.lock();
            if g.share_running {
                return;
            }
            if clip_id.is_none()
                && g.ledger.last_confirmed().is_none()
                && g.ledger.pending().is_empty()
            {
                drop(g);
                notify::show(&self.app, "No clip yet", "Save a replay in OBS first.");
                return;
            }
            g.share_running = true;
            g.preparing = Some("asking the server".into());
        }
        self.push_status();
        let rt = self.clone();
        let wanted = clip_id.clone().or_else(|| {
            let g = rt.lock();
            g.ledger.last_confirmed().and_then(|c| c.clip_id.clone())
        });
        tauri::async_runtime::spawn(async move {
            rt.copy_link(api, wanted, None).await;
            rt.lock().share_running = false;
            rt.push_status();
        });
    }

    /// One of the app's links from a notification's button, for a clip of this app's own only.
    pub fn run_link(&self, link: Link) {
        let found = self
            .lock()
            .ledger
            .by_either_id(link.clip())
            .map(|c| (c.clip_id.clone(), c.local_clip_id.clone()));
        let Some((server, local)) = found else {
            tracing::info!(link = %link.url(), "a link to a clip this app does not know: ignored");
            return;
        };
        tracing::info!(link = %link.url(), "a notification's button");
        let standalone = self.is_standalone();
        match link {
            Link::Copy(_) => match server {
                Some(id) => self.share_last(Some(id)),
                None if self.not_here("Copy link") => {}
                None => notify::show(
                    &self.app,
                    "No link",
                    "The clip is in the library on this PC, and the app is connected to a server now.",
                ),
            },
            Link::Open(_) => {
                let page = if standalone { local } else { server };
                let opened = match page {
                    Some(id) => self.open_library(&format!("/clips/{id}")),
                    None if standalone => {
                        Err("it is on the server, and the app runs standalone now".to_string())
                    }
                    None => Err(
                        "it is in the library on this PC, and the app is connected to a server now"
                            .to_string(),
                    ),
                };
                if let Err(e) = opened {
                    notify::show(&self.app, "The clip did not open", &e);
                }
            }
        }
    }

    /// A link the first start was given: once the library is known, at most 30 s after start.
    pub fn run_link_when_ready(&self, link: Link) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            for _ in 0..30 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if !rt.library_base().is_empty() {
                    break;
                }
            }
            rt.run_link(link);
        });
    }

    /// Waits until the clip's link embeds and copies it; a Save and share's stops waiting once a
    /// newer one is pressed, so the clipboard holds the newest clip's link.
    async fn copy_link(&self, api: Api, wanted: Option<String>, share: Option<CancellationToken>) {
        let newest = || share.as_ref().is_none_or(|t| !t.is_cancelled());
        let (mut ctl, _tx) = Control::never_paused();
        if let Some(t) = &share {
            ctl.cancel = t.clone();
            if !t.is_cancelled() {
                self.lock().preparing = Some("asking the server".into());
                self.push_status();
            }
        }
        let mut told = false;
        let rt2 = self.clone();
        let answer = uploader::share_when_ready(&api, wanted.as_deref(), &ctl, |step, progress| {
            if !newest() {
                return;
            }
            let pct = (progress * 100.0).round() as i64;
            let line = match step {
                "ingest" => format!("the server is still publishing the clip, {pct}%"),
                "preview" => format!("the link preview's video is rendering, {pct}%"),
                "upload" => "the link preview's video goes up to the storage".into(),
                other => format!("{other}, {pct}%"),
            };
            rt2.lock().preparing = Some(line.clone());
            rt2.push_status();
            if !told {
                told = true;
                notify::show(&rt2.app, "Preparing the link", &line);
            }
        })
        .await;
        if newest() {
            self.lock().preparing = None;
        }
        match answer {
            Ok(ShareAnswer::Ready { url, clip_id, .. }) if !newest() => {
                let _ = self.lock().ledger.set_share_url(&clip_id, &url);
                tracing::info!(clip = %clip_id, "link ready after a newer save and share was pressed: not copied");
            }
            Ok(ShareAnswer::Ready {
                url,
                clip_id,
                title,
            }) => match notify::copy(&self.app, &url) {
                Ok(()) => {
                    let _ = self.lock().ledger.set_share_url(&clip_id, &url);
                    tracing::info!(clip = %clip_id, "link copied");
                    self.cue(Cue::LinkCopied);
                    notify::show(&self.app, "Link copied. Go paste it.", &title);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "clipboard");
                    notify::show(
                        &self.app,
                        "The link is ready, but the clipboard refused",
                        &url,
                    );
                }
            },
            Ok(ShareAnswer::Preparing { .. }) => {}
            Err(uploader::UploadError::Cancelled) => {
                tracing::info!(
                    "save and share: a newer one was pressed, so this link is not waited for"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "share");
                notify::show(&self.app, "No link", &e.to_string());
            }
        }
        self.push_status();
    }

    /// Pause recording stops OBS's replay buffer and keeps it stopped, which gives back its
    /// encoder and its memory; Resume recording starts it again.
    pub fn set_recording_paused(&self, paused: bool) {
        tracing::info!(paused, "recording paused by the user");
        self.lock().recording_paused = paused;
        self.push_status();
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            match rt.obs_state(true).await {
                Ok(Some(st)) if paused => rt.ensure_replay_buffer(&st).await,
                Ok(Some(st)) if !st.replay_active => rt.start_replay_buffer(&st).await,
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "OBS state not read"),
            }
        });
    }

    pub fn set_paused(&self, paused: bool) {
        tracing::info!(paused, "uploads paused by the user");
        self.lock().paused_by_user = paused;
        self.update_paused();
        self.push_status();
    }

    /// Holds the uploads nobody waits for; `send_replace` keeps the value while no upload listens.
    fn update_paused(&self) {
        let g = self.lock();
        let newest = |p: &PathBuf| g.waited.get(p).is_some_and(|t| !t.is_cancelled());
        let held = g.paused_by_user || g.paused_fullscreen || g.waited_running.keys().any(newest);
        self.paused_tx.send_replace(held);
        for (p, tx) in &g.waited_running {
            tx.send_replace(held && !newest(p));
        }
    }

    fn spawn_fullscreen_poll(&self) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut lan: Option<(std::time::Instant, bool)> = None;
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let (want, only_wan, server, fg) = {
                    let g = rt.lock();
                    (
                        g.settings.upload.pause_in_fullscreen,
                        g.settings.upload.pause_only_on_wan,
                        g.settings.server_url.clone(),
                        g.foreground.clone(),
                    )
                };
                let mut fullscreen = want && fg.fullscreen_app();
                if fullscreen && only_wan {
                    let fresh = match lan {
                        Some((at, v)) if at.elapsed() < Duration::from_secs(60) => v,
                        _ => {
                            let v = server_on_lan(&server).await;
                            lan = Some((std::time::Instant::now(), v));
                            v
                        }
                    };
                    if fresh {
                        fullscreen = false;
                    }
                }
                let changed = {
                    let mut g = rt.lock();
                    let changed = g.paused_fullscreen != fullscreen;
                    g.paused_fullscreen = fullscreen;
                    changed
                };
                if changed {
                    tracing::info!(fullscreen, "fullscreen app");
                    rt.update_paused();
                    if let Some(l) = rt.local() {
                        l.hold(fullscreen);
                    }
                    rt.push_status();
                }
            }
        });
    }

    pub fn sweep_cache(&self) {
        let mut g = self.lock();
        let entries = g.ledger.cache_entries();
        let gone = reel_desktop_core::cache::to_delete(
            &entries,
            g.settings.cache.gb,
            g.settings.cache.days,
            jiff::Timestamp::now(),
        );
        for p in gone {
            match std::fs::remove_file(&p) {
                Ok(()) => {
                    tracing::info!(path = %p.display(), "local copy removed, the server has it");
                    let _ = g.ledger.mark_deleted(&p);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let _ = g.ledger.mark_deleted(&p);
                }
                Err(e) => tracing::warn!(path = %p.display(), error = %e, "could not remove"),
            }
        }
    }

    /// The server's page in the library webview, on top of the window.
    pub fn open_library(&self, path: &str) -> Result<(), String> {
        let (url, standalone) = {
            let g = self.lock();
            if g.settings.mode == Mode::Standalone {
                (
                    g.local.as_ref().map(|l| l.url.clone()).unwrap_or_default(),
                    true,
                )
            } else {
                (g.settings.server_url.clone(), false)
            }
        };
        if url.is_empty() {
            return Err(if standalone {
                "reel on this PC is not running yet".into()
            } else {
                "no server set yet; enter the server URL under Settings, Server".into()
            });
        }
        let _ = url;
        shell::ensure(&self.app);
        self.go_library(path);
        shell::present(&self.app);
        Ok(())
    }

    /// The library webview's base: the local reel in standalone, the server otherwise.
    pub fn library_base(&self) -> String {
        let g = self.lock();
        if g.settings.mode == Mode::Standalone {
            g.local.as_ref().map(|l| l.url.clone()).unwrap_or_default()
        } else {
            g.settings.server_url.clone()
        }
    }

    /// Takes the webview to a page of the server and watches for its first `app_status` call;
    /// without one the app probes the server and shows its own page.
    pub fn go_library(&self, path: &str) {
        let base = self.library_base();
        if base.is_empty() {
            if self.is_standalone() {
                self.lock().library_wanted = true;
            }
            return;
        }
        // no window yet: it goes to the library when it is made
        if self.app.get_webview(shell::LIBRARY).is_none() {
            return;
        }
        let nonce = {
            let mut g = self.lock();
            g.nav_nonce += 1;
            g.page_alive = false;
            g.asked = Some(shell::library_url(&base, path).to_string());
            g.nav_nonce
        };
        shell::navigate(&self.app, &base, path);
        self.watch(nonce, base);
    }

    /// The library is another one now: its front page loads.
    fn library_changed(&self) {
        if shell::owner_busy(&self.app) {
            self.lock().offline = None;
            self.push_status();
        } else {
            self.go_library("/");
        }
    }

    /// The webview starts a navigation: a server page reached from a link or the history gets
    /// a watch here, one of the app's own pages ends the watch of the page left behind.
    pub fn navigating(&self, url: &url::Url) {
        let mut g = self.lock();
        if g.asked.as_deref() == Some(url.as_str()) {
            g.asked = None;
            return;
        }
        if shell::is_local(url) {
            g.nav_nonce += 1;
            return;
        }
        if !matches!(url.scheme(), "http" | "https") {
            return;
        }
        g.nav_nonce += 1;
        g.page_alive = false;
        let nonce = g.nav_nonce;
        drop(g);
        let base = self.library_base();
        if !base.is_empty() {
            self.watch(nonce, base);
        }
    }

    fn watch(&self, nonce: u64, base: String) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            let origin = shell::origin_of(&base);
            let mut waited = Duration::ZERO;
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                waited += Duration::from_secs(2);
                {
                    let g = rt.lock();
                    if g.nav_nonce != nonce || g.page_alive {
                        return;
                    }
                }
                if rt.app.get_webview(shell::LIBRARY).is_none() {
                    return;
                }
                let at = rt
                    .app
                    .get_webview(shell::LIBRARY)
                    .and_then(|w| w.url().ok());
                if at.as_ref().map(shell::is_local).unwrap_or(false) {
                    return;
                }
                let foreign = at
                    .as_ref()
                    .map(|u| {
                        matches!(u.scheme(), "http" | "https")
                            && shell::origin_of(u.as_str()) != origin
                    })
                    .unwrap_or(false);
                if foreign && waited < Duration::from_secs(600) {
                    continue;
                }
                if waited >= LIFE_WAIT {
                    tracing::info!(at = ?at.map(|u| u.to_string()), "no sign of life from the page");
                    break;
                }
            }
            let why = match probe_server(&base).await {
                Err(why) => why,
                Ok(()) => Unreachable {
                    title: "The server answers, but its page did not load here".into(),
                    words: "The app reached the server, but the window did not get its page. This can be a name or certificate problem on the path the window takes, or a reel older than 1.0.1 whose pages do not know the app.".into(),
                    code: "no sign of life from the page after the server answered".into(),
                },
            };
            rt.go_offline(nonce, why);
        });
    }

    fn page_alive(&self) {
        let back = {
            let mut g = self.lock();
            g.page_alive = true;
            g.last_call = Some(std::time::Instant::now());
            g.offline.take().is_some()
        };
        if back {
            tracing::info!("the server is back; the library returns");
            self.push_status();
        }
    }

    /// The page's web process was ended: the page it showed loads again, or the library's
    /// front page when it was not one of the library's own.
    #[cfg(target_os = "linux")]
    pub fn reload_page(&self) {
        let base = self.library_base();
        let path = self
            .app
            .get_webview(shell::LIBRARY)
            .and_then(|w| w.url().ok())
            .filter(|u| {
                matches!(u.scheme(), "http" | "https")
                    && shell::origin_of(u.as_str()) == shell::origin_of(&base)
            })
            .map(|u| match u.query() {
                Some(q) => format!("{}?{q}", u.path()),
                None => u.path().to_string(),
            })
            .unwrap_or_else(|| "/".into());
        tracing::info!(path = %path, "the page loads again");
        self.go_library(&path);
    }

    /// Whether the server's page, alive before, has not called the app for `quiet`; once said,
    /// the count starts again at the page's next call.
    #[cfg(target_os = "linux")]
    pub fn page_silent(&self, quiet: Duration) -> bool {
        let mut g = self.lock();
        let silent = g.page_alive && g.last_call.is_some_and(|t| t.elapsed() > quiet);
        if silent {
            g.last_call = None;
        }
        silent
    }

    fn go_offline(&self, nonce: u64, why: Unreachable) {
        {
            let mut g = self.lock();
            if g.nav_nonce != nonce || g.page_alive {
                return;
            }
            let attempt = g.offline.as_ref().map(|o| o.attempt + 1).unwrap_or(0);
            let wait = Duration::from_secs((10u64 << attempt.min(3)).min(60));
            let (title, words) = if g.settings.mode == Mode::Standalone {
                (
                    "reel on this PC does not answer".to_string(),
                    "The library's pages did not open in the window. The app tries again by itself; Settings, Log says why.".to_string(),
                )
            } else {
                (why.title.clone(), why.words.clone())
            };
            g.offline = Some(Offline {
                title,
                words,
                code: why.code.clone(),
                attempt,
                next_try: std::time::Instant::now() + wait,
            });
            tracing::warn!(title = %why.title, code = %why.code, attempt, "cannot reach the server");
        }
        if !shell::owner_busy(&self.app) {
            shell::go(&self.app, shell::View::Offline, None);
        }
        self.push_status();
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            let next = rt.lock().offline.as_ref().map(|o| o.next_try);
            let Some(next) = next else {
                return;
            };
            tokio::time::sleep(next.saturating_duration_since(std::time::Instant::now())).await;
            if rt.lock().offline.is_none() {
                return;
            }
            rt.try_again().await;
        });
    }

    async fn try_again(&self) {
        let base = self.library_base();
        if base.is_empty() {
            return;
        }
        match probe_server(&base).await {
            Ok(()) => {
                if shell::owner_busy(&self.app) {
                    self.lock().offline = None;
                    tracing::info!("the server answers again; the library loads when opened");
                    self.push_status();
                } else {
                    tracing::info!("the server answers again; loading its page");
                    self.go_library("/");
                }
            }
            Err(why) => {
                let nonce = self.lock().nav_nonce;
                self.go_offline(nonce, why);
            }
        }
    }

    pub async fn retry_now(&self) {
        tracing::info!("retry asked");
        self.try_again().await;
    }

    /// A server page handed the shape of its Settings pages over, for the app's own pages.
    pub fn take_shape(&self, shape: Shape) {
        tracing::info!(
            sections = shape.sections.len(),
            user = %shape.who.name,
            "the server's page handed over the shape of its settings"
        );
        self.lock().shape = Some((std::time::Instant::now(), shape, "page"));
    }

    /// What the app's own Settings section is drawn with: from the server's page (fresh for two
    /// minutes), the local reel, the device API, the last known, or the built in shape.
    pub async fn page_shape(&self) -> serde_json::Value {
        let (standalone, cached) = {
            let g = self.lock();
            (g.settings.mode == Mode::Standalone, g.shape.clone())
        };
        let base = self.library_base();
        let fresh = cached
            .as_ref()
            .filter(|(at, _, from)| *from == "page" && at.elapsed() < Duration::from_secs(120))
            .map(|(_, s, from)| (s.clone(), *from));
        let known = |rt: &Runtime| {
            cached
                .clone()
                .map(|(_, s, from)| (s, from))
                .unwrap_or_else(|| (rt.built_in_shape(), "built-in"))
        };
        let (mut shape, from) = if let Some(f) = fresh {
            f
        } else if standalone {
            match self.local_shape().await {
                Some(s) => (s, "standalone"),
                None => (self.built_in_shape(), "built-in"),
            }
        } else if let Some(api) = self.api() {
            match tokio::time::timeout(Duration::from_secs(3), api.sections()).await {
                Ok(Ok(s)) => {
                    self.lock().shape = Some((std::time::Instant::now(), s.clone(), "api"));
                    (s, "api")
                }
                Ok(Err(e)) => {
                    tracing::debug!(error = %e, "no shape from the device API");
                    known(self)
                }
                Err(_) => known(self),
            }
        } else {
            known(self)
        };
        if let Some(o) = self.lock().offline.as_ref() {
            shape.health = ShapeHealth {
                state: "err".into(),
                line: o.title.clone(),
            };
        }
        serde_json::json!({ "shape": shape, "from": from, "base": base })
    }

    /// The sections of the reel this app was built with, and what the app knows itself.
    fn built_in_shape(&self) -> Shape {
        let g = self.lock();
        let standalone = g.settings.mode == Mode::Standalone;
        Shape {
            sections: reel_core::settings::Section::ALL
                .iter()
                .filter(|s| !standalone || s.in_standalone())
                .map(|s| ShapeSection {
                    id: s.id().into(),
                    label: s.label_in(standalone).into(),
                    line: s.line_in(standalone.then(|| g.paths.library_dir()).as_deref()),
                    changed: 0,
                })
                .collect(),
            who: ShapeWho {
                name: g.who.as_ref().map(|w| w.user.clone()).unwrap_or_default(),
                line: String::new(),
                admin: true,
            },
            health: ShapeHealth {
                state: "ok".into(),
                line: String::new(),
            },
            jobs: ShapeJobs::default(),
        }
    }

    async fn local_shape(&self) -> Option<Shape> {
        let local = self.local()?;
        let state = local.state.clone();
        let cfg = state.ctx.config();
        let user = reel_server::auth::User {
            id: String::new(),
            name: cfg.auth.dev_user.clone(),
            admin: true,
        };
        let who = reel_server::views::Who::at(&state, &user).await;
        let eff = state.ctx.effective();
        let sections = reel_core::settings::Section::ALL
            .iter()
            .filter(|s| s.in_standalone())
            .map(|s| ShapeSection {
                id: s.id().into(),
                label: s.label_in(true).into(),
                line: s.line_in(Some(&cfg.library_dir())),
                changed: reel_core::settings::DEFS
                    .iter()
                    .filter(|d| {
                        d.section == *s && eff.source(d.key) == reel_core::settings::Source::Table
                    })
                    .count() as u64,
            })
            .collect();
        let (queued, running, failed) = state.ctx.store.job_counts_full().await.ok()?;
        Some(Shape {
            sections,
            who: ShapeWho {
                name: who.name,
                line: who.line,
                admin: who.admin,
            },
            health: ShapeHealth {
                state: who.health.into(),
                line: who.health_line,
            },
            jobs: ShapeJobs {
                queued: queued.max(0) as u64,
                running: running.max(0) as u64,
                failed: failed.max(0) as u64,
            },
        })
    }

    /// The flags of a second start; says whether anything was applied. Saved on a thread of
    /// its own: a step in Program Files may hold the lane, and this runs on the main thread.
    pub fn apply_args(&self, args: &[String]) -> bool {
        self.apply_args_then(args, false)
    }

    /// At a first start, before the window, which reads what is saved; nothing holds the lane.
    pub fn apply_args_now(&self, args: &[String]) -> bool {
        self.apply_args_then(args, true)
    }

    fn apply_args_then(&self, args: &[String], wait: bool) -> bool {
        let base = self.settings();
        let mut s = base.clone();
        let mut changed = false;
        let mut obs = false;
        for a in args {
            if let Some(m) = a.strip_prefix("--mode=") {
                match m {
                    "standalone" => s.mode = Mode::Standalone,
                    "connected" => s.mode = Mode::Connected,
                    _ => continue,
                }
                changed = true;
            } else if let Some(u) = a.strip_prefix("--server=") {
                s.server_url = u.to_string();
                changed = true;
            } else if let Some(n) = a
                .strip_prefix("--clip-length=")
                .and_then(|n| n.parse::<u32>().ok())
            {
                s.obs.setup.clip_seconds = n;
                changed = true;
            } else if let Some(f) = a.strip_prefix("--format=") {
                s.obs.setup.format = f.to_string();
                changed = true;
            } else if a == "--show-obs" || a == "--hide-obs" {
                self.show_bundled_and_read(a == "--show-obs");
                obs = true;
            } else if let Some(w) = WebviewAudio::from_args(std::slice::from_ref(a)) {
                if s.webview_audio() != w {
                    s.webview_audio_fallback = w == WebviewAudio::Service;
                    changed = true;
                    tracing::info!(
                        to = w.word(),
                        "where WebView2 plays sound, saved; it holds from a run's first window"
                    );
                }
            }
        }
        if changed {
            let rt = self.clone();
            let save = move || match rt.save_settings(s, Some(base)) {
                Ok(_) => tracing::info!("settings changed from the command line"),
                Err(e) => tracing::warn!(error = %e, "settings from the command line refused"),
            };
            if wait {
                save();
            } else {
                std::thread::spawn(save);
            }
        }
        changed || obs
    }
}

/// Whether the server's host resolves to a private address: then an upload during a game
/// hurts nobody's ping on the internet.
async fn server_on_lan(server: &str) -> bool {
    let Ok(url) = url::Url::parse(server) else {
        return false;
    };
    let Some(host) = url.host_str().map(|h| h.to_string()) else {
        return false;
    };
    let port = url.port_or_known_default().unwrap_or(443);
    let found = tokio::net::lookup_host((host.as_str(), port)).await;
    match found {
        Ok(addrs) => addrs.into_iter().any(|a| match a.ip() {
            std::net::IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
            std::net::IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00,
        }),
        Err(_) => false,
    }
}

/// The steps in Program Files as the lane runs them.
struct Steps(Runtime);

impl reel_desktop_core::lane::Program for Steps {
    type Hold = crate::obsbox::Hold;

    fn saved(&self) -> Settings {
        self.0.settings()
    }

    fn with_obs(&self) -> bool {
        self.0.lock().bundled.is_some()
    }

    fn copy_there(&self) -> bool {
        self.0.copy_there()
    }

    fn hold(&self) -> Option<crate::obsbox::Hold> {
        let bx = self.0.lock().obs_box.clone()?;
        tracing::info!("OBS stops for the steps in Program Files");
        Some(bx.hold())
    }

    fn step(
        &self,
        job: reel_desktop_core::lane::Job,
        step: reel_desktop_core::elevated::Step,
        old: &Settings,
        new: &Settings,
        answered: &dyn Fn(),
    ) -> Result<(), String> {
        use reel_desktop_core::lane::Job;
        let done = match job {
            Job::Save => self.0.copy_step(step, old, new, answered),
            Job::Again => {
                self.0.copy_step(step, old, old, answered)?;
                self.0.restart_bundled();
                Ok(())
            }
            Job::Remove => {
                let b = self
                    .0
                    .lock()
                    .bundled
                    .clone()
                    .ok_or("this app came without OBS")?;
                reel_desktop_core::obsproc::task_end();
                reel_desktop_core::obsproc::elevated_step(step, &b, &[], answered)?;
                self.0.read_copy();
                tracing::info!("the copy left in Program Files removed, its tasks too");
                Ok(())
            }
        };
        if done.is_err() {
            self.0.read_copy();
        }
        done
    }

    fn save(&self, old: &Settings, new: Settings) -> Result<Settings, String> {
        self.0.store(old, new)
    }

    fn said(&self, problem: Option<String>) {
        if let Some(p) = &problem {
            tracing::warn!(problem = %p, "the OBS for games that run as administrator");
        }
        self.0.lock().obs_task_problem = problem;
    }

    fn settle(&self) {
        self.0.check_copy();
    }

    fn changed(&self) {
        self.0.push_status();
    }
}

/// A copy in Program Files without the second task, whose OBS keeps its own tray icon.
fn in_tray(copy: Option<&reel_desktop_core::bundled::Bundled>) -> bool {
    copy.is_some_and(|c| {
        c.exe.is_file()
            && (!reel_desktop_core::obsproc::window_task_exists()
                || reel_desktop_core::elevated::tray_on(c) == Some(true))
    })
}

/// The OBS that came with the app; on Linux the installed one, run as a second OBS with
/// settings of its own (REEL_OBS_EXE names one for tests).
fn detect_bundled(app: &AppHandle) -> Option<reel_desktop_core::bundled::Bundled> {
    use reel_desktop_core::bundled::{self, Bundled};
    if cfg!(not(windows)) {
        let data = settings::paths().data_dir;
        if let Some(exe) = std::env::var_os("REEL_OBS_EXE") {
            return Some(Bundled::linux(PathBuf::from(exe), &data));
        }
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let found = bundled::installed(
            std::env::var_os("PATH").as_deref(),
            &home,
            &data,
            &bundled::flatpak_dirs(&home),
        );
        match &found {
            Some(b) => {
                tracing::info!(obs = %b.which(), "reel's own OBS is the one installed here, run a second time")
            }
            None => tracing::info!("no OBS installed here: no OBS on the PATH, none from Flathub"),
        }
        return found;
    }
    let dir = app.path().resource_dir().ok()?;
    let b = Bundled::in_resources(&dir);
    if let Some(b) = &b {
        tracing::info!(exe = %b.exe.display(), "the bundled OBS is here");
    }
    b
}

/// The names of the bundled OBS's tracks, told to the server with each clip since an MP4
/// drops them.
fn track_names(
    bundled: Option<&reel_desktop_core::bundled::Bundled>,
    s: &Settings,
) -> Option<Vec<String>> {
    let o = &s.obs;
    bundled
        .filter(|_| o.bundled && o.setup.own_tracks)
        .map(|b| o.setup.track_names(b.platform))
}
