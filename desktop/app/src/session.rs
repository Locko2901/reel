//! A session recording: OBS records until it is stopped, and the file goes into the library as a
//! recording. `record_now` is the one place it starts and stops, with what asked.
use crate::runtime::Runtime;
use crate::{notify, status};
use reel_desktop_core::cues::Cue;
use reel_desktop_core::obs::ObsCommand;
use reel_desktop_core::session::{self, By, Facts, Recording};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    Start,
    Stop,
    /// Starts one, or stops the one that runs: the hotkey and `--record-session`.
    Toggle,
}

/// How often a running recording is looked at: its reminders, the disk, the tray's minutes.
const TICK: Duration = Duration::from_secs(5);
/// How often the disk's room is read while it records.
const DISK_EVERY: Duration = Duration::from_secs(15);
/// How long OBS has to say that a recording it was asked to start runs.
const START_WAIT: Duration = Duration::from_secs(15);
/// How long a file OBS left behind must hold its size before it counts as whole.
const STILL: Duration = Duration::from_secs(10);
/// How long Quit waits for OBS to close the file.
const QUIT_WAIT: Duration = Duration::from_secs(12);

/// A start asked of OBS that OBS has not confirmed yet.
#[derive(Debug, Clone, Copy)]
pub struct Asked {
    pub by: By,
}

/// What the window and the tray show of a session recording.
#[derive(Debug, Clone, serde::Serialize, Default, PartialEq)]
pub struct Line {
    /// When it started, in milliseconds since 1970, for the window's clock.
    pub since_ms: u64,
    pub line: String,
    /// The length in words: "1 h 12 min".
    pub length: String,
    pub starting: bool,
    pub stopping: bool,
    /// OBS does not answer: the recording may go on in OBS.
    pub lost: bool,
}

pub fn line(rec: Option<&Recording>, asked: Option<&Asked>, lost: bool) -> Option<Line> {
    let now = SystemTime::now();
    match (rec, asked) {
        (Some(r), _) => Some(Line {
            since_ms: r
                .started
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or_default(),
            line: if lost {
                format!(
                    "{}; OBS does not answer, it may go on recording",
                    session::minutes_words(r.length(now))
                )
            } else {
                r.line(now)
            },
            length: session::minutes_words(r.length(now)),
            starting: false,
            stopping: r.stopping.is_some(),
            lost,
        }),
        (None, Some(a)) => Some(Line {
            line: format!("asked {}", a.by.words()),
            starting: true,
            ..Line::default()
        }),
        (None, None) => None,
    }
}

impl Runtime {
    /// Starts or stops it with what asked, and says in a notification when that did not work.
    pub fn record(&self, ask: Ask, by: By) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(why) = rt.record_now(ask, by).await {
                let title = if rt.lock().session.is_some() {
                    "The session recording did not stop"
                } else {
                    "No session recording"
                };
                notify::show(&rt.app, title, &crate::tray::capital(&why));
            }
        });
    }

    /// The one place a session recording starts and stops; the window waits for its answer.
    pub async fn record_now(&self, ask: Ask, by: By) -> Result<(), String> {
        let (running, starting) = {
            let g = self.lock();
            (g.session.is_some(), g.session_asked.is_some())
        };
        let start = match ask {
            Ask::Start => true,
            Ask::Stop => false,
            Ask::Toggle if starting => {
                tracing::info!(
                    "session recording: asked {} while one starts; nothing more",
                    by.words()
                );
                return Ok(());
            }
            Ask::Toggle => !running,
        };
        if start {
            self.start_session(by).await
        } else if running {
            self.stop_session(by).await
        } else {
            tracing::info!(
                "session recording: a stop asked {}, and none runs",
                by.words()
            );
            Ok(())
        }
    }

    fn session_facts(&self) -> Facts {
        let dir = self.record_dir();
        let g = self.lock();
        Facts {
            leaving: crate::updates::leaving(),
            recording: g.session.is_some(),
            starting: g.session_asked.is_some(),
            obs_set_up: g.settings.obs.enabled || (g.bundled.is_some() && g.settings.obs.bundled),
            connected: g.obs.connected,
            trial: g.encoder_trial.is_some(),
            free: dir.and_then(|d| free_bytes(&d)),
        }
    }

    /// Whether a session recording can start now, or stop: the tray and the window grey it.
    pub(crate) fn session_action(&self) -> status::Action {
        if self.lock().session.is_some() {
            return status::Action {
                ok: true,
                why: String::new(),
            };
        }
        let why = session::start_refused(&self.session_facts());
        status::Action {
            ok: why.is_none(),
            why: why.unwrap_or_default(),
        }
    }

    async fn start_session(&self, by: By) -> Result<(), String> {
        if let Some(why) = session::start_refused(&self.session_facts()) {
            tracing::warn!(
                "session recording did not start (asked {}): {why}",
                by.words()
            );
            return Err(why);
        }
        self.lock().session_asked = Some(Asked { by });
        self.push_status();
        tracing::info!("session recording: OBS is asked to start, {}", by.words());
        if let Err(e) = self.obs_command(ObsCommand::StartRecord).await {
            self.lock().session_asked = None;
            self.push_status();
            let why = format!("OBS did not start recording: {e}");
            tracing::warn!(
                "session recording did not start (asked {}): {why}",
                by.words()
            );
            return Err(why);
        }
        let deadline = Instant::now() + START_WAIT;
        while Instant::now() < deadline {
            if self.lock().session.is_some() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // an OBS that started without a word: its status says
        match self.obs_command(ObsCommand::RecordStatus).await {
            Ok(st) if st.active => {
                if self.lock().session.is_none() {
                    self.session_began(None, false);
                }
                Ok(())
            }
            other => {
                self.lock().session_asked = None;
                self.push_status();
                let why = match other {
                    Err(e) => format!("OBS did not say whether it records: {e}"),
                    Ok(_) => "OBS took the start, but does not record".to_string(),
                };
                tracing::warn!(
                    "session recording did not start (asked {}): {why}",
                    by.words()
                );
                Err(why)
            }
        }
    }

    async fn stop_session(&self, by: By) -> Result<(), String> {
        {
            let mut g = self.lock();
            let Some(r) = g.session.as_mut() else {
                return Ok(());
            };
            if r.stopping.is_some() {
                return Ok(());
            }
            r.stopping = Some(by);
        }
        self.push_status();
        tracing::info!("session recording: OBS is asked to stop, {}", by.words());
        match self.obs_command(ObsCommand::StopRecord).await {
            Ok(path) => {
                // OBS's word that it stopped names the file; this answer may come first
                if let Some(p) = path {
                    if let Some(r) = self.lock().session.as_mut() {
                        r.file.get_or_insert_with(|| PathBuf::from(&p));
                    }
                }
                Ok(())
            }
            Err(e) if e.contains("not active") => {
                tracing::info!("session recording: OBS had stopped already");
                self.session_ended(None, By::Obs);
                Ok(())
            }
            Err(e) => {
                if let Some(r) = self.lock().session.as_mut() {
                    r.stopping = None;
                }
                self.push_status();
                let why = format!("OBS did not stop recording: {e}");
                tracing::warn!(
                    "session recording did not stop (asked {}): {why}",
                    by.words()
                );
                Err(why)
            }
        }
    }

    /// OBS's word that its recording started, stopped, or runs at a connection.
    pub(crate) fn session_state(
        &self,
        active: bool,
        started: Option<String>,
        saved: Option<String>,
    ) {
        if active {
            let known = self.lock().session.is_some();
            if !known {
                self.session_began(started.map(PathBuf::from), true);
            } else if let Some(path) = started {
                if let Some(r) = self.lock().session.as_mut() {
                    r.file = Some(PathBuf::from(&path));
                }
            }
            return;
        }
        if let Some(path) = saved {
            let by = self
                .lock()
                .session
                .as_ref()
                .and_then(|r| r.stopping)
                .unwrap_or(By::Obs);
            self.session_ended(Some(PathBuf::from(path)), by);
        }
    }

    /// A recording runs: reel's own start, one started in OBS, or one found at a connection.
    fn session_began(&self, file: Option<PathBuf>, cue: bool) {
        let (by, started) = {
            let mut g = self.lock();
            let asked = g.session_asked.take();
            let by = asked.map(|a| a.by).unwrap_or(By::Obs);
            let mut r = Recording::new(SystemTime::now(), by);
            r.file = file.clone();
            r.game = g.name_in_front(reel_desktop_core::programs::Came::Recording);
            let started = r.started;
            g.session = Some(r);
            g.session_lost = None;
            (by, started)
        };
        let file = file
            .map(|f| f.display().to_string())
            .unwrap_or_else(|| "the file OBS names when it stops".into());
        tracing::info!("session recording started {}: {file}", by.words());
        if cue {
            self.cue(Cue::Session);
        }
        self.push_status();
        self.spawn_session_watch(started);
    }

    /// A recording that ran when the app connected to OBS: it goes on as one started in OBS,
    /// counted from when OBS says it started. One the app lost while OBS did not answer
    /// goes on as it was, or ends if OBS has stopped it.
    pub(crate) async fn session_on_connect(&self) {
        let st = match self.obs_command(ObsCommand::RecordStatus).await {
            Ok(st) => st,
            Err(e) => {
                tracing::warn!(error = %e, "session recording: OBS's recording status not read");
                return;
            }
        };
        let (known, lost) = {
            let g = self.lock();
            (g.session.is_some(), g.session_lost.is_some())
        };
        match (st.active, known) {
            (true, true) => {
                if lost {
                    tracing::info!("session recording: OBS answers again and still records");
                }
                self.lock().session_lost = None;
                self.push_status();
            }
            (true, false) => {
                let started = SystemTime::now()
                    .checked_sub(Duration::from_millis(st.duration_ms))
                    .unwrap_or_else(SystemTime::now);
                {
                    let mut g = self.lock();
                    let mut r = Recording::new(started, By::Obs);
                    r.game = g.obs_game.clone();
                    g.session = Some(r);
                }
                tracing::info!(
                    "session recording: OBS records already, for {}; it goes on as one started in OBS",
                    session::length_words(Duration::from_millis(st.duration_ms))
                );
                self.push_status();
                self.spawn_session_watch(started);
            }
            (false, true) if lost => self.session_ended(None, By::ObsEnded),
            _ => {}
        }
    }

    /// OBS went away while it recorded: what it wrote is taken once the file holds still.
    pub(crate) fn session_obs_gone(&self) {
        let mut g = self.lock();
        if g.session.is_some() && g.session_lost.is_none() {
            g.session_lost = Some(Instant::now());
            drop(g);
            tracing::warn!("session recording: OBS does not answer; the file is taken once OBS has let go of it");
            self.push_status();
        }
    }

    /// OBS split the recording: the file before is whole and goes in; the session goes on.
    pub(crate) fn session_split(&self, path: String) {
        let done = {
            let mut g = self.lock();
            let Some(r) = g.session.as_mut() else {
                return;
            };
            r.parts += 1;
            let done = r.file.replace(PathBuf::from(&path));
            done.map(|f| (f, r.clone()))
        };
        tracing::info!(next = %path, "session recording: OBS goes on in a new file");
        if let Some((file, r)) = done {
            self.take_session_file(&file, Some(&r));
        }
    }

    /// The recording ended: its file goes in as a recording, the log and the line say so.
    pub(crate) fn session_ended(&self, saved: Option<PathBuf>, by: By) {
        let taken = self.lock().session.take();
        let Some(r) = taken else {
            if let Some(f) = saved {
                tracing::info!(file = %f.display(), "a recording OBS made stopped");
                self.take_session_file(&f, None);
            }
            return;
        };
        {
            let mut g = self.lock();
            g.session_lost = None;
            g.session_asked = None;
        }
        let length = session::length_words(r.length(SystemTime::now()));
        let named = saved.or_else(|| r.file.clone());
        let file = named.clone().filter(|f| f.is_file());
        match (&file, &named) {
            (Some(f), _) => tracing::info!(
                "session recording stopped {} after {length}: {}",
                by.words(),
                f.display()
            ),
            (None, Some(f)) => tracing::warn!(
                "session recording stopped {} after {length}; OBS left no file at {}",
                by.words(),
                f.display()
            ),
            (None, None) => tracing::warn!(
                "session recording stopped {} after {length}; OBS named no file",
                by.words()
            ),
        }
        if by == By::ObsEnded {
            notify::show(
                &self.app,
                "OBS stopped during a session recording",
                &match &file {
                    Some(_) => {
                        format!("What it recorded is kept ({length}) and goes into the library.")
                    }
                    None => format!("It had recorded {length}; OBS left no file to keep."),
                },
            );
        }
        if let Some(f) = file {
            self.take_session_file(&f, Some(&r));
        }
        self.push_status();
    }

    /// Takes a recording's file as a clip marked as a recording, titled by its start and game.
    fn take_session_file(&self, path: &Path, rec: Option<&Recording>) {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let meta = rec.map(|r| crate::runtime::SessionFile {
            recorded: jiff::Timestamp::try_from(r.started)
                .unwrap_or_else(|_| jiff::Timestamp::now()),
            game: r.game.clone(),
        });
        self.on_found_recording(path, size, meta);
    }

    /// Reminders, the disk, the tray's minutes and the game, while this recording runs.
    fn spawn_session_watch(&self, started: SystemTime) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(TICK);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut minute = u64::MAX;
            let mut disk_at: Option<Instant> = None;
            let mut still: Option<(u64, Instant)> = None;
            loop {
                tick.tick().await;
                let Some(r) = rt.lock().session.clone().filter(|r| r.started == started) else {
                    return;
                };
                let length = r.length(SystemTime::now());
                if rt.lock().session_lost.is_some() {
                    if rt.whole_after_loss(&r, &mut still) {
                        rt.session_ended(None, By::ObsEnded);
                        return;
                    }
                    continue;
                }
                let every = rt.lock().settings.session.remind_minutes;
                if r.stopping.is_none() && session::reminder_due(length, every, r.reminded_at) {
                    if let Some(s) = rt.lock().session.as_mut() {
                        s.reminded_at = length;
                    }
                    rt.remind(length);
                }
                if length.as_secs() / 60 != minute {
                    minute = length.as_secs() / 60;
                    rt.session_game();
                    rt.push_status();
                }
                if r.stopping.is_none() && disk_at.is_none_or(|t| t.elapsed() >= DISK_EVERY) {
                    disk_at = Some(Instant::now());
                    if let Some(why) = rt.short_of_room(&r) {
                        tracing::warn!("session recording stops by itself: {why}");
                        notify::show(
                            &rt.app,
                            "Session recording stopped: the disk is almost full",
                            &format!(
                                "{} recorded; {why}. It goes into the library.",
                                session::length_words(length)
                            ),
                        );
                        if let Err(e) = rt.record_now(Ask::Stop, By::Disk).await {
                            notify::show(&rt.app, "The session recording did not stop", &e);
                        }
                    }
                }
            }
        });
    }

    fn remind(&self, length: Duration) {
        let words = session::length_words(length);
        tracing::info!("session recording reminder: {words} so far");
        self.cue(Cue::Session);
        let hotkey = self.settings().hotkeys.record.trim().to_string();
        let stop = if hotkey.is_empty() {
            "The tray or the window stops it.".to_string()
        } else {
            format!("{hotkey}, the tray or the window stops it.")
        };
        notify::show(
            &self.app,
            "Still recording a session",
            &format!("{words} so far. {stop}"),
        );
    }

    /// The game a recording is named after: the one seen last while it ran.
    fn session_game(&self) {
        let mut g = self.lock();
        let seen = g.name_in_front(reel_desktop_core::programs::Came::Recording);
        if let (Some(game), Some(r)) = (seen, g.session.as_mut()) {
            r.game = Some(game);
        }
    }

    /// Why the recording must stop for room, if it must: the disk OBS records to, and in
    /// standalone the library's, which still makes its copy of the recording.
    fn short_of_room(&self, r: &Recording) -> Option<String> {
        let dir = r
            .file
            .as_ref()
            .and_then(|f| f.parent().map(Path::to_path_buf))
            .or_else(|| self.record_dir())?;
        let written = r
            .file
            .as_ref()
            .and_then(|f| std::fs::metadata(f).ok())
            .map(|m| m.len())
            .unwrap_or(0);
        let library = {
            let g = self.lock();
            (g.settings.mode == reel_desktop_core::settings::Mode::Standalone)
                .then(|| g.settings.library_dir(&g.paths))
        };
        let same = library.as_ref().is_some_and(|l| same_disk(&dir, l));
        let free = free_bytes(&dir)?;
        if session::short_of_room(free, if same { written } else { 0 }) {
            return Some(if same {
                format!(
                    "the disk has {} left, and the library needs as much room again as the recording's {} to take it in",
                    session::size_words(free),
                    session::size_words(written)
                )
            } else {
                format!(
                    "the disk OBS records to has {} left",
                    session::size_words(free)
                )
            });
        }
        let lib = library.filter(|_| !same)?;
        let lib_free = free_bytes(&lib)?;
        session::short_of_room(lib_free, written).then(|| {
            format!(
                "the library's disk has {} left, and needs room for the recording's {}",
                session::size_words(lib_free),
                session::size_words(written)
            )
        })
    }

    /// After OBS went away: whether its file has held its size long enough to be whole.
    fn whole_after_loss(&self, r: &Recording, still: &mut Option<(u64, Instant)>) -> bool {
        let Some(f) = r.file.as_ref() else {
            return self
                .lock()
                .session_lost
                .is_some_and(|t| t.elapsed() >= STILL);
        };
        let size = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
        match still {
            Some((s, since)) if *s == size => since.elapsed() >= STILL,
            _ => {
                *still = Some((size, Instant::now()));
                false
            }
        }
    }

    /// Quit: a recording of reel's own OBS, or one reel started, is stopped and its file
    /// taken before OBS goes; one the user started in their own OBS goes on there.
    pub fn session_before_quit(&self) -> Option<String> {
        let own = !self.bundled_in_use();
        let stops = self
            .lock()
            .session
            .as_ref()
            .is_some_and(|r| !(own && r.by == By::Obs));
        if !stops {
            return None;
        }
        let rt = self.clone();
        let asked = std::thread::spawn(move || {
            tauri::async_runtime::block_on(rt.record_now(Ask::Stop, By::Quit))
        })
        .join()
        .unwrap_or_else(|_| Err("the stop did not run".into()));
        if let Err(e) = asked {
            tracing::warn!("quit: the session recording did not stop: {e}");
            return Some("the session recording did not stop".into());
        }
        let at = Instant::now();
        while self.lock().session.is_some() && at.elapsed() < QUIT_WAIT {
            std::thread::sleep(Duration::from_millis(100));
        }
        Some(if self.lock().session.is_some() {
            format!(
                "OBS did not close the session recording's file in {} s",
                QUIT_WAIT.as_secs()
            )
        } else if own {
            "the session recording is stopped in OBS and its file taken".into()
        } else {
            "the session recording is stopped and its file taken".into()
        })
    }

    /// The folder OBS records to, as OBS last said.
    fn record_dir(&self) -> Option<PathBuf> {
        let g = self.lock();
        g.obs_state
            .as_ref()
            .map(|s| PathBuf::from(&s.record_dir))
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| g.obs_dir.clone())
    }
}

/// A session recording runs or starts: an install waits until it has stopped.
pub fn holds_install(g: &crate::runtime::Inner) -> bool {
    g.session.is_some() || g.session_asked.is_some()
}

fn free_bytes(dir: &Path) -> Option<u64> {
    let at = dir.ancestors().find(|a| a.exists())?;
    match reel_pipeline::notify::free_and_total(at) {
        (_, 0) => None,
        (free, _) => Some(free),
    }
}

/// Whether two folders are on one disk, so one's files fill the other's room.
fn same_disk(a: &Path, b: &Path) -> bool {
    let at = |p: &Path| p.ancestors().find(|x| x.exists()).map(Path::to_path_buf);
    let (Some(a), Some(b)) = (at(a), at(b)) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(&a), std::fs::metadata(&b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev(),
            _ => false,
        }
    }
    #[cfg(windows)]
    {
        let root = |p: &Path| {
            p.components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        };
        root(&a) == root(&b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_says_how_long_and_what_started_it() {
        let started = SystemTime::now() - Duration::from_secs(72 * 60 + 5);
        let mut r = Recording::new(started, By::Hotkey);
        let l = line(Some(&r), None, false).unwrap();
        assert_eq!(l.line, "1 h 12 min, started by the hotkey");
        assert_eq!(l.length, "1 h 12 min");
        assert!(l.since_ms > 0 && !l.starting && !l.stopping);
        assert!(line(Some(&r), None, true)
            .unwrap()
            .line
            .ends_with("OBS does not answer, it may go on recording"));
        r.stopping = Some(By::Tray);
        assert!(line(Some(&r), None, false).unwrap().stopping);
        let asked = Asked { by: By::Window };
        let l = line(None, Some(&asked), false).unwrap();
        assert_eq!(
            (l.line.as_str(), l.starting),
            ("asked from the window", true)
        );
        assert_eq!(line(None, None, false), None);
    }

    #[test]
    fn two_folders_on_one_disk() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        std::fs::create_dir_all(&a).unwrap();
        assert!(same_disk(&a, &dir.path().join("b/not/made/yet")));
        assert!(free_bytes(&dir.path().join("not/made")).is_some());
    }
}
