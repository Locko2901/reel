//! The app's own updates: from its paired server, the installer of the server's version; in
//! standalone, the newer release's installer, which reel on this PC finds on GitHub. Either is
//! signed with the release key and checked against the key built into the app.
use crate::notify;
use crate::runtime::Runtime;
use reel_desktop_core::settings::{Mode, Settings, UpdateMode};
use reel_desktop_core::update_steps::{self, Kind, Note};
use reel_pipeline::app_update::{Flavour, NotOffered};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
use tauri::Manager;
use tauri_plugin_updater::{Updater, UpdaterBuilder, UpdaterExt};

pub fn releases_url() -> String {
    "https://github.com/Locko2901/reel/releases".into()
}

fn set_line(rt: &Runtime, line: impl Into<String>) {
    rt.lock().update_line = line.into();
    rt.push_status();
}

const EVERY: Duration = Duration::from_secs(6 * 3600);

/// 45 s after the start, then every six hours by the wall clock: a timer stops while the PC
/// sleeps, so a long one comes days late on a PC that sleeps at night. Standalone also looks
/// within a minute of each read of the releases by reel on this PC, and once that read is old.
pub fn spawn_checks(rt: Runtime) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(45)).await;
        loop {
            if let Err(e) = check(&rt, false).await {
                tracing::warn!(error = %e, "update check");
            }
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if new_read(&rt).await || read_is_old(&rt).await {
                    break;
                }
                let last = rt.lock().update_checked_at;
                let since = last.and_then(|t| SystemTime::now().duration_since(t).ok());
                if since.map(|d| d >= EVERY).unwrap_or(true) {
                    break;
                }
            }
        }
    });
}

/// The line when the server offered nothing: up to date, the app newer than its server, the
/// server without this app's installer yet, or the server behind the newest release.
pub fn none_line(v: &crate::status::Versions) -> String {
    use reel_core::versions::older;
    match (v.standing.as_str(), v.server.as_deref(), v.newest.as_deref()) {
        ("ahead", Some(s), _) => format!(
            "this app, {}, is newer than its server, {s}, which offers only its own version: the app updates again once the server runs a newer one",
            v.app
        ),
        ("behind", Some(s), _) => format!(
            "the server runs {s} and has no installer of it for this app yet; it fetches it from the release once a day"
        ),
        (_, Some(s), Some(n)) if older(s, n) => format!(
            "up to date with the server, {s}; reel {n} is out and comes to this app once the server runs it"
        ),
        _ => format!("up to date with the server, {}", v.app),
    }
}

/// Progress in the line every 5 %: the installer with OBS is 232 MB.
async fn download(
    rt: &Runtime,
    update: &tauri_plugin_updater::Update,
) -> Result<Vec<u8>, tauri_plugin_updater::Error> {
    let version = update.version.clone();
    let (mut got, mut said) = (0u64, 0u64);
    let line = rt.clone();
    update
        .download(
            |chunk, total| {
                got += chunk as u64;
                let Some(total) = total.filter(|t| *t > 0) else {
                    return;
                };
                let pct = got * 100 / total;
                if pct >= said + 5 {
                    said = pct;
                    set_line(
                        &line,
                        format!(
                            "downloading reel {version}: {} of {} MB",
                            got / 1_000_000,
                            total / 1_000_000
                        ),
                    );
                }
            },
            || {},
        )
        .await
}

/// A download that failed, after "reel x.y": one the key built into the app refuses, or one
/// signed as another version than offered, is never installed; any other failure may pass on a
/// second try.
fn not_downloaded(version: &str, e: &tauri_plugin_updater::Error, again: &str) -> String {
    use tauri_plugin_updater::Error as E;
    match e {
        E::SignedVersionMismatch { signed, .. } => format!(
            "reel {version} was refused: its installer is signed as reel {signed}, so nothing was installed"
        ),
        E::MissingSignedVersion => format!(
            "reel {version} was refused: its installer's signature names no version, so nothing was installed"
        ),
        E::Minisign(_) | E::Base64(_) | E::SignatureUtf8(_) => format!(
            "reel {version} was refused: the download does not match a signature of the release key built into this app, so nothing was installed"
        ),
        e => format!("reel {version} did not download: {e}; {again}"),
    }
}

/// How the update is put in place: the Windows installer, with OBS or without, or the AppImage.
pub fn kind(rt: &Runtime) -> Kind {
    Kind::here(rt.lock().bundled.is_some())
}

/// Which of the release's downloads this app came from, which its update brings again.
pub fn flavour(with_obs: bool) -> Flavour {
    #[cfg(target_os = "linux")]
    {
        let _ = with_obs;
        if crate::linux::appimage().is_some() {
            Flavour::AppImage
        } else {
            Flavour::Deb
        }
    }
    #[cfg(not(target_os = "linux"))]
    if with_obs {
        Flavour::WithObs
    } else {
        Flavour::Plain
    }
}

/// The updater's question to the paired server or to reel on this PC; reel light names no
/// flavour, as an app older than the two installers asks.
pub fn endpoint(base: &str, path: &str, with_obs: bool) -> Result<url::Url, String> {
    let flavour = if with_obs { "&flavour=with-obs" } else { "" };
    format!(
        "{}{path}?current={{{{current_version}}}}&target={{{{target}}}}&arch={{{{arch}}}}{flavour}",
        base.trim_end_matches('/')
    )
    .parse()
    .map_err(|e| format!("{e}"))
}

/// Where reel on this PC answers the app's own updater in standalone.
pub const RELEASE_PATH: &str = "/api/app/update";

/// The installer starts reel again in the tray, its integrity check skipped: the updater has
/// checked the signature.
pub fn builder<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    endpoint: url::Url,
) -> Result<UpdaterBuilder, String> {
    app.updater_builder()
        .endpoints(vec![endpoint])
        .and_then(|b| b.header("User-Agent", format!("reel/{}", app.package_info().version)))
        .map(|b| {
            b.restart_after_install(false)
                .installer_args(["/NCRC", "/R", "/ARGS", "--hidden"])
        })
        .map_err(|e| e.to_string())
}

/// The updater's question to reel on this PC, which answers the app alone: with this run's key.
pub fn local_builder<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    local: &crate::standalone::Local,
    with_obs: bool,
) -> Result<UpdaterBuilder, String> {
    builder(app, endpoint(&local.url, RELEASE_PATH, with_obs)?)?
        .header(reel_server::single::KEY_HEADER, local.key())
        .map_err(|e| e.to_string())
}

/// The release's installer comes from GitHub: the key of reel on this PC stays on the PC.
pub fn off_the_pc(mut update: tauri_plugin_updater::Update) -> tauri_plugin_updater::Update {
    update.headers.remove(reel_server::single::KEY_HEADER);
    update
}

fn updater(rt: &Runtime, b: UpdaterBuilder) -> Result<Updater, String> {
    // Windows' installer ends the app at once: the log's last lines go first
    let out = rt.clone();
    b.on_before_exit(move || {
        tracing::info!("quit: the update's installer starts, and the app exits");
        drop(out.lock().log.take());
    })
    .build()
    .map_err(|e| e.to_string())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The first start after an update, or one that did not install: the note it left said at
/// once, the window opened where it was, the installer's steps in the log.
pub fn spawn_updated(rt: Runtime) {
    let now = rt.app.package_info().version.to_string();
    let dir = reel_desktop_core::settings::paths().config_dir;
    let note = Note::take(&dir, now_secs());
    let failure = update_steps::take_failure(&dir);
    let was = {
        let mut g = rt.lock();
        let was = std::mem::replace(&mut g.settings.last_version, now.clone());
        let file = g.paths.settings_file();
        if was != now {
            if let Err(e) = g.settings.save(&file) {
                tracing::warn!(error = %e, "the version that ran was not kept");
            }
        }
        was
    };
    if !was.is_empty() && was != now {
        tracing::info!(from = %was, to = %now, "reel was updated");
        rt.lock().updated_from = Some(was.clone());
    }
    if note.is_some() || (!was.is_empty() && was != now) {
        log_installer_steps();
        clear_installers(&rt.app.package_info().name);
    }
    let said = update_steps::after(note.as_ref(), &now, &was, failure.as_deref());
    let place = note
        .as_ref()
        .filter(|n| !n.quit)
        .and_then(|n| n.window.clone());
    let updated = !was.is_empty() && was != now;
    tauri::async_runtime::spawn(async move {
        if let Some((title, body)) = said {
            tracing::info!(%title, %body, "the update's outcome, said");
            notify::show(&rt.app, &title, &body);
        }
        if let Some(place) = place {
            for _ in 0..50 {
                if rt.app.try_state::<Runtime>().is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            tracing::info!(view = %place.view, "the window opens again where it was before the update");
            crate::shell::reopen(&rt, &place);
        }
        if !updated || !rt.bundled_in_use() {
            return;
        }
        for _ in 0..90 {
            if rt.lock().obs.connected {
                return;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        if let Some(b) = rt.status().obs_box {
            notify::show(
                &rt.app,
                "OBS did not start after the update",
                &format!("Its line: {}.", b.line),
            );
        }
    });
}

/// The updater leaves Windows' installer in a folder of its own in the temporary folder as the
/// app exits.
fn clear_installers(name: &str) {
    if !cfg!(windows) {
        return;
    }
    let Ok(dir) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in dir.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if !updaters_folder(name, &file) || !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        match std::fs::remove_dir_all(entry.path()) {
            Ok(()) => {
                tracing::info!(folder = %file, "the update's installer is removed from the temporary folder")
            }
            Err(e) => {
                tracing::warn!(folder = %file, error = %e, "the update's installer was not removed")
            }
        }
    }
}

/// The updater's folder for an installer: `<name>-<version>-updater-` and six random letters.
fn updaters_folder(name: &str, file: &str) -> bool {
    let Some(rest) = file.strip_prefix(name).and_then(|r| r.strip_prefix('-')) else {
        return false;
    };
    rest.rsplit_once("-updater-")
        .is_some_and(|(version, tail)| {
            version.starts_with(|c: char| c.is_ascii_digit())
                && tail.len() == 6
                && tail.chars().all(|c| c.is_ascii_alphanumeric())
        })
}

fn log_installer_steps() {
    let file = reel_desktop_core::settings::paths()
        .logs_dir()
        .join("installer.log");
    let fresh = std::fs::metadata(&file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(3600));
    if !fresh {
        return;
    }
    if let Ok(text) = std::fs::read_to_string(&file) {
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            tracing::info!("the update's installer: {line}");
        }
    }
}

pub async fn check(rt: &Runtime, by_hand: bool) -> Result<(), String> {
    let s = rt.settings();
    if s.mode == Mode::Standalone {
        return check_release(rt, by_hand, &s).await;
    }
    #[cfg(target_os = "linux")]
    if crate::linux::appimage().is_none() {
        set_line(
            rt,
            "installed from the .deb, which does not update itself: the next version's .deb is on the server, under Settings, Devices",
        );
        return Ok(());
    }
    if s.updates == UpdateMode::Off && !by_hand {
        set_line(rt, "off");
        return Ok(());
    }
    let token = rt.lock().tokens.get().ok().flatten();
    rt.lock().update_checked_at = Some(SystemTime::now());
    let (Some(token), false) = (token, s.server_url.is_empty()) else {
        set_line(rt, "the server is asked once the app is paired");
        return Ok(());
    };
    // plain http on localhost only; the installer's signature is checked either way
    if let Err(e) = reel_desktop_core::settings::check_server_url(&s.server_url) {
        set_line(rt, format!("not asked: {e}"));
        return Ok(());
    }
    let with_obs = rt.lock().bundled.is_some();
    let url = endpoint(&s.server_url, "/api/device/update", with_obs)?;
    let updater = updater(
        rt,
        builder(&rt.app, url)?
            .header("Authorization", format!("Bearer {token}"))
            .map_err(|e| e.to_string())?,
    )?;
    match updater.check().await {
        Ok(Some(update)) => ready(rt, &s, update, Source::Server).await,
        Ok(None) => {
            tracing::info!("the app is up to date");
            rt.lock().update = None;
            let st = rt.status();
            set_line(rt, none_line(&st.versions));
        }
        Err(e) => set_line(
            rt,
            format!("the server did not answer the check: {e}; it is asked again in six hours, or now with Check now"),
        ),
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
enum Source {
    Server,
    Release,
}

async fn ready(rt: &Runtime, s: &Settings, update: tauri_plugin_updater::Update, from: Source) {
    let version = update.version.clone();
    tracing::info!(
        version,
        from = if from == Source::Server {
            "the server"
        } else {
            "the release"
        },
        "an update of the app is ready"
    );
    let held = {
        let g = rt.lock();
        g.update_bytes.is_some() && g.update.as_ref().is_some_and(|u| u.version == version)
    };
    rt.lock().update = Some(update.clone());
    let current = rt.app.package_info().version.to_string();
    match s.updates {
        _ if test_build(&current) => {
            let at = match from {
                Source::Server => "is on the server",
                Source::Release => "is out",
            };
            set_line(
                rt,
                format!(
                    "reel {version} {at}; this test build ({current}) installs it only when asked"
                ),
            );
        }
        UpdateMode::OnQuit if held => {
            set_line(rt, format!("reel {version} installs when the app quits"));
        }
        UpdateMode::OnQuit => {
            set_line(
                rt,
                format!("reel {version} is downloading, to install when the app quits"),
            );
            match download(rt, &update).await {
                Ok(bytes) => {
                    rt.lock().update_bytes = Some(bytes);
                    set_line(rt, format!("reel {version} installs when the app quits"));
                }
                Err(e) => {
                    tracing::warn!(version, error = %e, "the update did not download");
                    set_line(rt, not_downloaded(&version, &e, "Check now tries again"));
                }
            }
        }
        _ => {
            set_line(rt, format!("reel {version} is ready to install"));
            let told = rt.lock().update_told.replace(version.clone());
            if told.as_deref() != Some(version.as_str()) {
                notify::show(
                    &rt.app,
                    &format!("reel {version} is ready"),
                    "Install it from the tray, or under Settings, Startup.",
                );
            }
        }
    }
}

/// When reel on this PC made the read of the releases the app last looked at.
static ACTED_ON: Mutex<Option<String>> = Mutex::new(None);

/// How old reel on this PC's read of the releases may be before the app's check reads again.
const READ_AGAIN: jiff::SignedDuration = jiff::SignedDuration::from_secs(6 * 3600);

/// Whether the app's check reads the releases again: Check for updates on, New versions not
/// off, and the last read six hours old by the wall clock, or stamped after now (the clock went
/// back). Before any read it waits: reel on this PC reads a minute after it starts.
fn read_again(
    check_on: bool,
    mode: UpdateMode,
    read_at: Option<&str>,
    now: jiff::Timestamp,
) -> bool {
    if !check_on || mode == UpdateMode::Off {
        return false;
    }
    let Some(at) = read_at else {
        return false;
    };
    reel_core::format::parse(at).is_none_or(|t| {
        let age = now.duration_since(t);
        age.is_negative() || age >= READ_AGAIN
    })
}

async fn read_is_old(rt: &Runtime) -> bool {
    let s = rt.settings();
    if s.mode != Mode::Standalone {
        return false;
    }
    let Some(local) = rt.lock().local.clone() else {
        return false;
    };
    let at = local.ctx.updates.read().await.checked_at.clone();
    read_again(
        local.ctx.config().advanced.update_check,
        s.updates,
        at.as_deref(),
        jiff::Timestamp::now(),
    )
}

async fn new_read(rt: &Runtime) -> bool {
    if rt.settings().mode != Mode::Standalone {
        return false;
    }
    let Some(local) = rt.lock().local.clone() else {
        return false;
    };
    let at = local.ctx.updates.read().await.checked_at.clone();
    at.is_some() && *ACTED_ON.lock().unwrap_or_else(|e| e.into_inner()) != at
}

/// The line for a newer release that offers this app nothing.
fn not_offered(version: &str, why: NotOffered) -> String {
    let v = version;
    match why {
        NotOffered::NotNewer => String::new(),
        NotOffered::Deb => format!(
            "reel {v} is out; the .deb does not update itself: install the new .deb from the releases page"
        ),
        NotOffered::TestBuild => format!("reel {v} is a test build, which is not offered"),
        NotOffered::NoInstaller => format!(
            "reel {v} is out, and its release has no installer for this app yet; Check now asks again"
        ),
        NotOffered::NoSignature => format!(
            "reel {v} is out, but its release has no signature for this app's installer, and none is installed without one; Check now asks again"
        ),
    }
}

/// Standalone: the newest release as reel on this PC last read it, which Check now reads again
/// and a check reads again once it is old, and this app's installer on it, offered by reel on
/// this PC as a server offers one.
async fn check_release(rt: &Runtime, by_hand: bool, s: &Settings) -> Result<(), String> {
    rt.lock().update_checked_at = Some(SystemTime::now());
    let local = rt.lock().local.clone();
    let Some(local) = local else {
        set_line(rt, "");
        return Ok(());
    };
    let read_at = local.ctx.updates.read().await.checked_at.clone();
    if by_hand {
        reel_pipeline::updates::check_once(&local.ctx).await;
    } else if read_again(
        local.ctx.config().advanced.update_check,
        s.updates,
        read_at.as_deref(),
        jiff::Timestamp::now(),
    ) {
        tracing::info!(
            read_at = read_at.as_deref().unwrap_or_default(),
            "reel on this PC read the releases six hours ago or more: the update check reads them again"
        );
        let again = reel_pipeline::updates::check_once(&local.ctx).await;
        match &again.error {
            Some(e) => tracing::warn!(
                error = %e,
                "the releases were not read again; the next try is in six hours, or now with Check now"
            ),
            None => tracing::info!(
                newest = again
                    .latest
                    .as_ref()
                    .map(|r| r.version.as_str())
                    .unwrap_or_default(),
                "the releases were read again"
            ),
        }
    }
    let read = local.ctx.updates.read().await.clone();
    *ACTED_ON.lock().unwrap_or_else(|e| e.into_inner()) = read.checked_at.clone();
    if read.error.is_some() {
        // an update found before stays offered; without one the line says why the read failed
        if rt.lock().update.is_none() {
            set_line(rt, "");
        }
        return Ok(());
    }
    let Some(latest) = read.latest else {
        rt.lock().update = None;
        set_line(rt, "");
        return Ok(());
    };
    let current = rt.app.package_info().version.to_string();
    let with_obs = rt.lock().bundled.is_some();
    let v = latest.version.clone();
    let offer = match reel_pipeline::app_update::from_release(&latest, &current, flavour(with_obs))
    {
        Ok(offer) => offer,
        Err(why) => {
            tracing::info!(version = %v, ?why, "the release offers this app nothing");
            rt.lock().update = None;
            set_line(rt, not_offered(&v, why));
            return Ok(());
        }
    };
    if s.updates == UpdateMode::Off && !by_hand {
        rt.lock().update = None;
        set_line(
            rt,
            format!("reel {v} is out; New versions is off, so Check now offers it"),
        );
        return Ok(());
    }
    if let Err(e) = reel_pipeline::app_update::release_signature(&offer.signature).await {
        tracing::warn!(error = %e, version = %v, "the installer's signature was not read from the release");
        rt.lock().update = None;
        set_line(
            rt,
            format!("reel {v} is out, but the signature of its installer could not be read from the release ({e}); Check now tries again"),
        );
        return Ok(());
    }
    let updater = updater(rt, local_builder(&rt.app, &local, with_obs)?)?;
    match updater.check().await {
        Ok(Some(update)) => ready(rt, s, off_the_pc(update), Source::Release).await,
        Ok(None) => {
            rt.lock().update = None;
            set_line(rt, "");
        }
        Err(e) => {
            tracing::warn!(error = %e, version = %v, "the release's installer was not offered");
            rt.lock().update = None;
            set_line(
                rt,
                format!("reel {v} is out, but its installer was not offered: {e}; Check now tries again"),
            );
        }
    }
    Ok(())
}

/// A build named after a commit (1.2.0-dev.abc1234): its release counts as newer, so it is
/// offered but never downloaded or installed on a quit by itself.
fn test_build(version: &str) -> bool {
    version.contains('-')
}

/// What an install waits for before reel closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    /// A clip on its way to the server, or a share being made.
    Clip,
    Move,
    /// reel on this PC switching to a folder that holds a library.
    Use,
    Copy,
}

impl Busy {
    fn until(self) -> &'static str {
        match self {
            Busy::Clip => "the clip on its way is up",
            Busy::Move => "the library has moved",
            Busy::Use => "the library is taken up",
            Busy::Copy => "the copy of the library is made",
        }
    }

    fn what(self) -> &'static str {
        match self {
            Busy::Clip => "the clip on its way",
            Busy::Move => "the library's move",
            Busy::Use => "taking up the library",
            Busy::Copy => "the copy of the library",
        }
    }

    fn library(self) -> bool {
        self != Busy::Clip
    }
}

/// The library goes first: a clip holds an install for a while, a move or a copy as long as it
/// goes on.
pub fn busy(uploading: bool, job: Option<&crate::library::Job>) -> Option<Busy> {
    match job.filter(|j| j.running).map(|j| j.kind.as_str()) {
        Some("copy") => Some(Busy::Copy),
        Some("use") => Some(Busy::Use),
        Some(_) => Some(Busy::Move),
        None => uploading.then_some(Busy::Clip),
    }
}

/// A clip holds an install this long at most.
const CLIP_BOUND: Duration = Duration::from_secs(600);
/// A move or a copy whose phase and bytes stood this long has a drive that stopped answering.
const STILL_BOUND: Duration = Duration::from_secs(600);

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Go,
    Wait(Busy),
    /// A move or a copy stood still: the update waits for another Install or the next start.
    GiveUp(Busy),
}

/// One install's wait: since when a clip holds it, and the library job's last progress.
#[derive(Default)]
pub struct Hold {
    clip_since: Option<Instant>,
    last: Option<(String, u64, Instant)>,
}

impl Hold {
    pub fn look(
        &mut self,
        now: Instant,
        uploading: bool,
        job: Option<&crate::library::Job>,
    ) -> Verdict {
        let Some(b) = busy(uploading, job) else {
            return Verdict::Go;
        };
        if b == Busy::Clip {
            let since = *self.clip_since.get_or_insert(now);
            return if now.duration_since(since) >= CLIP_BOUND {
                Verdict::Go
            } else {
                Verdict::Wait(b)
            };
        }
        let (phase, done) = job.map(|j| (j.phase.clone(), j.done)).unwrap_or_default();
        match &self.last {
            Some((p, d, since)) if *p == phase && *d == done => {
                if now.duration_since(*since) >= STILL_BOUND {
                    Verdict::GiveUp(b)
                } else {
                    Verdict::Wait(b)
                }
            }
            _ => {
                self.last = Some((phase, done, now));
                Verdict::Wait(b)
            }
        }
    }
}

/// The update's line; for one held for the quit while a move or a copy runs, that its quit does
/// not install it.
pub fn line_now(line: String, held: Option<&str>, job: Option<&crate::library::Job>) -> String {
    match (held, busy(false, job)) {
        (Some(v), Some(b)) => format!("reel {v} installs at the first quit after {}", b.until()),
        _ => line,
    }
}

fn gave_up_line(version: &str, b: Busy) -> String {
    format!(
        "reel {version} did not install: {} has not gone on for 10 minutes; Install tries again, and the next start offers it again",
        b.what()
    )
}

fn holder(rt: &Runtime) -> Option<Busy> {
    let g = rt.lock();
    busy(
        g.uploading.is_some() || g.share_running,
        g.library_job.as_ref(),
    )
}

/// Waits while something holds the install; an Err once a move or a copy stood still.
async fn hold(rt: &Runtime, version: &str) -> Result<(), String> {
    let mut hold = Hold::default();
    let mut said = None;
    loop {
        let verdict = {
            let g = rt.lock();
            hold.look(
                Instant::now(),
                g.uploading.is_some() || g.share_running,
                g.library_job.as_ref(),
            )
        };
        match verdict {
            Verdict::Go => return Ok(()),
            Verdict::GiveUp(b) => {
                let words = gave_up_line(version, b);
                tracing::warn!(
                    version,
                    "the update did not install: {} stood still",
                    b.what()
                );
                set_line(rt, words.clone());
                return Err(words);
            }
            Verdict::Wait(b) if said != Some(b) => {
                said = Some(b);
                tracing::info!(version, "the update waits: {}", b.what());
                set_line(rt, format!("reel {version} installs once {}", b.until()));
            }
            Verdict::Wait(_) => {}
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// An Install under way, from the first look to reel closing: a second one is not a second
/// install.
static INSTALLING: AtomicBool = AtomicBool::new(false);

pub fn installing() -> bool {
    INSTALLING.load(Ordering::Relaxed)
}

struct Installing(Runtime);

impl Drop for Installing {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::SeqCst);
        self.0.push_status();
    }
}

/// Downloads with the window there, then installs as the app quits: on Windows the installer's
/// small window, then reel again; on Linux the new AppImage over the running one.
pub async fn install_now(rt: &Runtime) -> Result<(), String> {
    let update = rt
        .lock()
        .update
        .clone()
        .ok_or_else(|| "no update to install".to_string())?;
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let _installing = Installing(rt.clone());
    rt.push_status();
    let version = update.version.clone();
    let going = update_steps::going(&version, kind(rt), false);
    let first = holder(rt).filter(|b| b.library());
    if let Some(b) = first {
        let stop = match b {
            Busy::Move => "; the move's Stop lets it install now",
            Busy::Copy => "; the copy's Stop lets it install now",
            _ => "",
        };
        notify::show(
            &rt.app,
            &format!("reel {version} waits for the library"),
            &format!("It installs once {}{stop}.", b.until()),
        );
    } else {
        notify::show(&rt.app, &going.0, &going.1);
    }
    hold(rt, &version).await?;
    set_line(rt, format!("installing reel {version}"));
    tracing::info!(version, "installing the update");
    let ready = rt.lock().update_bytes.take();
    let bytes = match ready {
        Some(b) => b,
        None => download(rt, &update).await.map_err(|e| {
            tracing::warn!(version, error = %e, "the update did not download");
            let words = not_downloaded(&version, &e, "Install tries again");
            set_line(rt, words.clone());
            words
        })?,
    };
    // a move begun during the download holds it as well
    let late = holder(rt).is_some_and(Busy::library);
    hold(rt, &version).await?;
    if first.is_some() || late {
        notify::show(&rt.app, &going.0, &going.1);
    }
    leave(rt, "Install reel", Then::Install(Box::new(update), bytes));
    Ok(())
}

/// An installer or the uninstaller asks the app to quit before it touches the install folder:
/// OBS stopped for good, not started again as after a crash.
pub fn quit_for_install(rt: &Runtime) {
    tracing::info!("an installer asks the app to quit");
    leave(rt, "an installer", Then::Exit);
}

/// None on Linux or without an install.
pub fn uninstaller() -> Option<std::path::PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let path = std::env::current_exe()
        .ok()?
        .parent()?
        .join("uninstall.exe");
    path.is_file().then_some(path)
}

/// About's Uninstall reel: the uninstaller with its page of choices, the app out of its way.
pub fn uninstall(rt: &Runtime) -> Result<(), String> {
    let exe =
        uninstaller().ok_or("no uninstaller beside the app: remove reel in Apps & features")?;
    reel_core::proc::command(&exe)
        .spawn()
        .map_err(|e| format!("the uninstaller did not start: {e}"))?;
    tracing::info!(uninstaller = %exe.display(), "the uninstaller runs; the app quits for it");
    leave(rt, "Uninstall reel", Then::Exit);
    Ok(())
}

pub fn quit(rt: &Runtime, from: &'static str) {
    let ready = {
        let mut g = rt.lock();
        let library = busy(false, g.library_job.as_ref());
        match (g.update.clone(), g.update_bytes.take(), library) {
            (Some(u), Some(_), Some(b)) => {
                tracing::info!(version = %u.version, "the update is not installed on this quit: {} runs; the next start reads it again", b.what());
                None
            }
            (Some(u), Some(b), None) => Some((u, b)),
            _ => None,
        }
    };
    let then = match ready {
        Some((u, b)) => {
            let (title, body) = update_steps::going(&u.version, kind(rt), true);
            notify::show(&rt.app, &title, &body);
            Then::InstallOnQuit(Box::new(u), b)
        }
        None => Then::Exit,
    };
    leave(rt, from, then);
}

/// What the app does once OBS is stopped on its way out.
pub enum Then {
    Exit,
    /// The update the setting keeps for the quit; the app quits whether it installs or not.
    InstallOnQuit(Box<tauri_plugin_updater::Update>, Vec<u8>),
    /// Install reel from the tray or Settings, Startup; the app starts again if it does not install.
    Install(Box<tauri_plugin_updater::Update>, Vec<u8>),
}

/// The app is on its way out: nothing makes the window again.
static LEAVING: AtomicBool = AtomicBool::new(false);

/// A start of reel's while it was on its way out: its arguments, run once the app is out.
static START_AFTER: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// How long the way out may take before the app ends itself.
const BOUND: Duration = Duration::from_secs(20);

pub fn leaving() -> bool {
    LEAVING.load(Ordering::Relaxed)
}

/// reel starts again with these arguments once out, so a start right after Quit is not lost.
pub fn start_after(args: Vec<String>) {
    if args.iter().any(|a| a == "--quit-for-install") {
        return;
    }
    tracing::info!("reel was started while it quits: it starts again once out");
    *START_AFTER.lock().unwrap_or_else(|e| e.into_inner()) = Some(args);
}

/// The way out's steps in the log, timed from the moment it was asked.
pub struct Steps {
    from: &'static str,
    at: Instant,
}

impl Steps {
    pub fn new(from: &'static str) -> Steps {
        tracing::info!(from, "quit: asked");
        Steps {
            from,
            at: Instant::now(),
        }
    }

    pub fn step(&self, what: &str) {
        tracing::info!(from = self.from, at = %secs(self.at.elapsed()), "quit: {what}");
    }
}

pub fn secs(d: Duration) -> String {
    format!("{:.2} s", d.as_secs_f64())
}

/// The way out of every quit: the window and the tray icon go at once, then OBS is stopped
/// (bounded), an update installed, and the app goes.
pub fn leave(rt: &Runtime, from: &'static str, then: Then) {
    if LEAVING.swap(true, Ordering::SeqCst) {
        tracing::info!(from, "quit: asked again while on the way out");
        return;
    }
    let steps = Steps::new(from);
    leave_note(rt, &then);
    if let Some(w) = rt.app.get_window(crate::shell::MAIN) {
        let _ = w.destroy();
    }
    if let Some(t) = rt.app.tray_by_id(crate::tray::TRAY_ID) {
        let _ = t.set_visible(false);
    }
    steps.step("the window and the tray icon are gone");
    std::thread::spawn(move || {
        std::thread::sleep(BOUND);
        tracing::warn!(after = %secs(BOUND), "quit: still not out; the app ends itself");
        std::thread::sleep(Duration::from_millis(200));
        std::process::exit(0);
    });
    let rt = rt.clone();
    std::thread::spawn(move || {
        if rt.stop_bundled() {
            steps.step("OBS is stopped");
        }
        let start_again = match then {
            Then::Exit => false,
            Then::InstallOnQuit(update, bytes) => {
                tracing::info!(version = %update.version, "installing the update on the way out");
                if let Err(e) = update.install(bytes) {
                    tracing::warn!(error = %e, "the update did not install");
                    failed(&format!("its installer did not start: {e}"));
                }
                false
            }
            Then::Install(update, bytes) => {
                if let Err(e) = update.install(bytes) {
                    tracing::warn!(error = %e, "the update did not install; reel starts again as it was");
                    failed(&format!("its installer did not start: {e}"));
                }
                true
            }
        };
        let again = START_AFTER
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .or_else(|| {
                start_again.then(|| {
                    let exe = std::env::args().next().unwrap_or_default();
                    vec![exe, "--hidden".to_string()]
                })
            });
        steps.step(if again.is_some() {
            "done; reel starts again"
        } else {
            "done; the app exits"
        });
        drop(rt.lock().log.take());
        if let Some(args) = again {
            tauri_plugin_single_instance::destroy(&rt.app);
            if let Ok(exe) = tauri::process::current_binary(&rt.app.env()) {
                let _ = reel_core::proc::command(exe)
                    .args(args.iter().skip(1))
                    .spawn();
            }
        }
        rt.app.exit(0);
    });
}

/// The note for the reel that starts after the update, with the window's place unless the
/// update installs as reel quits.
fn leave_note(rt: &Runtime, then: &Then) {
    let (update, quit) = match then {
        Then::Exit => return,
        Then::InstallOnQuit(u, _) => (u, true),
        Then::Install(u, _) => (u, false),
    };
    let note = Note {
        from: rt.app.package_info().version.to_string(),
        to: update.version.clone(),
        quit,
        window: (!quit).then(|| crate::shell::place(&rt.app)).flatten(),
        news: update.body.as_deref().and_then(update_steps::news_line),
        at: now_secs(),
    };
    let dir = reel_desktop_core::settings::paths().config_dir;
    match note.write(&dir) {
        Ok(()) => {
            tracing::info!(to = %note.to, quit, window = note.window.is_some(), "the update's note is left for the next start")
        }
        Err(e) => tracing::warn!(error = %e, "the update's note was not written"),
    }
}

fn failed(why: &str) {
    let dir = reel_desktop_core::settings::paths().config_dir;
    if let Err(e) = std::fs::write(dir.join(update_steps::FAILED), why) {
        tracing::warn!(error = %e, "why the update stopped was not written");
    }
}

#[cfg(test)]
mod tests {
    use super::{busy, line_now, Busy, Hold, Verdict};
    use crate::library::Job;
    use base64::Engine;
    use std::time::{Duration, Instant};

    fn job(kind: &str, phase: &str, done: u64) -> Job {
        Job {
            kind: kind.into(),
            phase: phase.into(),
            done,
            total: 500_000_000_000,
            running: true,
            ..Job::default()
        }
    }

    #[test]
    fn an_install_waits_for_a_move_a_copy_or_a_clip_and_the_library_first() {
        let moving = job("move", "copying", 1);
        let copying = job("copy", "copying", 1);
        let using = job("use", "starting", 0);
        assert_eq!(busy(false, Some(&moving)), Some(Busy::Move));
        assert_eq!(busy(false, Some(&copying)), Some(Busy::Copy));
        assert_eq!(busy(false, Some(&using)), Some(Busy::Use));
        assert_eq!(busy(true, None), Some(Busy::Clip));
        assert_eq!(
            busy(true, Some(&copying)),
            Some(Busy::Copy),
            "the library first"
        );
        assert_eq!(busy(false, None), None);
        let ended = Job {
            running: false,
            ok: true,
            line: "The library moved".into(),
            ..moving
        };
        assert_eq!(
            busy(false, Some(&ended)),
            None,
            "a move that ended holds nothing"
        );
        assert_eq!(busy(true, Some(&ended)), Some(Busy::Clip));
    }

    #[test]
    fn a_move_holds_an_install_as_long_as_it_goes_on_and_a_stop_lets_it_go() {
        let t0 = Instant::now();
        let mins = |m: u64| t0 + Duration::from_secs(m * 60);
        let mut hold = Hold::default();
        // five hours of a move across drives, its bytes going on every few minutes
        for m in (0..300).step_by(4) {
            let j = job("move", "copying", m * 1_000_000_000);
            assert_eq!(
                hold.look(mins(m), false, Some(&j)),
                Verdict::Wait(Busy::Move),
                "{m} min"
            );
        }
        let checking = job("move", "checking", 0);
        assert_eq!(
            hold.look(mins(300), false, Some(&checking)),
            Verdict::Wait(Busy::Move),
            "a new phase is progress"
        );
        assert_eq!(
            hold.look(mins(309), false, Some(&checking)),
            Verdict::Wait(Busy::Move)
        );
        assert_eq!(
            hold.look(mins(310), false, Some(&checking)),
            Verdict::GiveUp(Busy::Move),
            "ten minutes without a byte: the drive stopped answering"
        );
        let mut hold = Hold::default();
        let copy = job("copy", "database", 0);
        assert_eq!(
            hold.look(mins(0), false, Some(&copy)),
            Verdict::Wait(Busy::Copy)
        );
        let stopped = Job {
            running: false,
            line: "The copy was stopped".into(),
            ..copy
        };
        assert_eq!(
            hold.look(mins(1), false, Some(&stopped)),
            Verdict::Go,
            "a Stop lets the update go on"
        );
        assert!(super::gave_up_line("1.12.1", Busy::Copy).starts_with(
            "reel 1.12.1 did not install: the copy of the library has not gone on for 10 minutes"
        ));
    }

    #[test]
    fn a_clip_holds_an_install_ten_minutes_at_most() {
        let t0 = Instant::now();
        let mut hold = Hold::default();
        assert_eq!(hold.look(t0, true, None), Verdict::Wait(Busy::Clip));
        let nine = t0 + Duration::from_secs(9 * 60);
        assert_eq!(hold.look(nine, true, None), Verdict::Wait(Busy::Clip));
        let ten = t0 + Duration::from_secs(10 * 60);
        assert_eq!(
            hold.look(ten, true, None),
            Verdict::Go,
            "the clip goes up again after the start"
        );
        assert_eq!(
            Hold::default().look(t0, false, None),
            Verdict::Go,
            "nothing holds it"
        );
    }

    #[test]
    fn an_update_held_for_the_quit_says_it_waits_for_the_first_quit_after_the_library() {
        let line = || "reel 1.12.1 installs when the app quits".to_string();
        let moving = job("move", "copying", 1);
        assert_eq!(
            line_now(line(), Some("1.12.1"), Some(&moving)),
            "reel 1.12.1 installs at the first quit after the library has moved"
        );
        assert_eq!(
            line_now(line(), Some("1.12.1"), Some(&job("copy", "checking", 1))),
            "reel 1.12.1 installs at the first quit after the copy of the library is made"
        );
        assert_eq!(
            line_now(line(), Some("1.12.1"), None),
            line(),
            "no move: the quit installs it"
        );
        assert_eq!(
            line_now(line(), None, Some(&moving)),
            line(),
            "nothing held for the quit"
        );
    }

    #[test]
    fn the_line_of_a_check_that_found_nothing_says_where_the_app_stands() {
        let v = |app: &str, server: &str, newest: Option<&str>, standing: &str| {
            crate::status::Versions {
                app: app.into(),
                server: Some(server.into()),
                newest: newest.map(str::to_string),
                standing: standing.into(),
                ..Default::default()
            }
        };
        assert_eq!(
            super::none_line(&v("1.4.1", "1.4.1", Some("1.4.1"), "same")),
            "up to date with the server, 1.4.1"
        );
        assert_eq!(
            super::none_line(&v("1.4.1", "1.4.0", Some("1.4.1"), "ahead")),
            "this app, 1.4.1, is newer than its server, 1.4.0, which offers only its own version: the app updates again once the server runs a newer one"
        );
        assert_eq!(
            super::none_line(&v("1.4.0", "1.4.0", Some("1.4.1"), "same")),
            "up to date with the server, 1.4.0; reel 1.4.1 is out and comes to this app once the server runs it"
        );
        assert!(super::none_line(&v("1.3.1", "1.4.0", None, "behind"))
            .starts_with("the server runs 1.4.0 and has no installer"));
    }

    #[test]
    fn only_the_updaters_own_folders_go_from_the_temporary_folder() {
        assert!(super::updaters_folder("reel", "reel-1.11.0-updater-a1B2c3"));
        assert!(super::updaters_folder(
            "reel",
            "reel-1.11.0-dev.e58080a-updater-zzzzzz"
        ));
        for other in [
            "reel-1.11.0-updater-abc",
            "reel-updater-abcdef",
            "reels-1.11.0-updater-abcdef",
            "obs-1.11.0-updater-abcdef",
            "reel-1.11.0-installer.exe",
            "reel",
        ] {
            assert!(!super::updaters_folder("reel", other), "{other}");
        }
    }

    #[test]
    fn a_build_named_after_a_commit_never_updates_itself() {
        assert!(super::test_build("1.2.0-dev.e58080a"));
        assert!(!super::test_build("1.2.0"));
    }

    #[test]
    fn the_releases_are_read_again_once_the_last_read_is_six_hours_old_by_the_wall_clock() {
        use reel_desktop_core::settings::UpdateMode::{Ask, Off, OnQuit};
        // reel on an office PC read the releases at 10:08; a release went out at 10:10
        let at = |t: &str| -> jiff::Timestamp { t.parse().unwrap() };
        let read = Some("2026-11-11T10:08:00.250Z");
        let again = |on, mode, read: Option<&str>, now| super::read_again(on, mode, read, at(now));
        assert!(!again(true, Ask, read, "2026-11-11T16:07:59Z"));
        assert!(again(true, Ask, read, "2026-11-11T16:08:01Z"));
        assert!(
            again(true, OnQuit, read, "2026-11-13T08:30:00Z"),
            "a PC that slept two nights reads again at its first check"
        );
        assert!(
            again(true, Ask, read, "2026-11-11T09:00:00Z"),
            "a read stamped after now: the clock went back"
        );
        assert!(again(true, Ask, Some("not a time"), "2026-11-11T10:09:00Z"));
        assert!(
            !again(true, Ask, None, "2026-11-11T10:09:00Z"),
            "before the first read: reel on this PC reads a minute after it starts"
        );
        assert!(
            !again(true, Off, read, "2026-11-20T00:00:00Z"),
            "New versions off reads nothing more than reel on this PC does"
        );
        assert!(
            !again(false, Ask, read, "2026-11-20T00:00:00Z"),
            "Check for updates off: nothing asks GitHub by itself"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_read_of_the_releases_while_one_runs_takes_that_reads_answer() {
        let gh = StandIn::start(RELEASE, Vec::new()).await;
        let dir = tempfile::tempdir().unwrap();
        let local = standalone_read(&gh.base, dir.path()).await;
        let reads = || {
            gh.asked
                .lock()
                .unwrap()
                .iter()
                .filter(|(path, ..)| path.ends_with("/releases/latest"))
                .count()
        };
        assert_eq!(reads(), 1);
        let (first, second) = tokio::join!(
            reel_pipeline::updates::check_at(&local.ctx, &gh.base),
            reel_pipeline::updates::check_at(&local.ctx, &gh.base),
        );
        assert_eq!(reads(), 2, "two asks at once, one read");
        assert_eq!(first.checked_at, second.checked_at);
        assert_eq!(
            second.latest.map(|r| r.version).as_deref(),
            Some(RELEASE),
            "the second ask has the first one's answer"
        );
    }

    fn decoded(b64: &str) -> String {
        String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn the_quit_says_its_times_in_seconds_and_keeps_a_start_but_an_installers() {
        use std::time::Duration;
        assert_eq!(super::secs(Duration::from_millis(1234)), "1.23 s");
        assert_eq!(super::secs(Duration::from_millis(4)), "0.00 s");
        super::start_after(vec!["reel-desktop".into(), "--quit-for-install".into()]);
        assert!(
            super::START_AFTER.lock().unwrap().is_none(),
            "an installer's request starts nothing"
        );
        super::start_after(vec!["reel-desktop".into(), "--hidden".into()]);
        assert_eq!(
            super::START_AFTER.lock().unwrap().take().as_deref(),
            Some(&["reel-desktop".to_string(), "--hidden".to_string()][..])
        );
        assert!(!super::leaving());
    }

    struct StandIn {
        base: String,
        forged: std::sync::Arc<std::sync::atomic::AtomicBool>,
        asked: Asked,
    }

    const RELEASE: &str = "9.9.0";

    type Asked = std::sync::Arc<std::sync::Mutex<Vec<(String, String, bool)>>>;

    impl StandIn {
        async fn start(release: &str, files: Vec<(String, Vec<u8>)>) -> StandIn {
            use axum::extract::{Path, State};
            use axum::http::{HeaderMap, StatusCode};
            use axum::response::IntoResponse;
            use std::sync::atomic::Ordering;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let forged = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let asked: Asked = Default::default();
            let assets: Vec<serde_json::Value> = files
                .iter()
                .map(|(name, _)| {
                    serde_json::json!({ "name": name, "browser_download_url": format!("{base}/download/v{release}/{name}"), "url": format!("{base}/api-only") })
                })
                .collect();
            let release = serde_json::json!({
                "tag_name": format!("v{release}"),
                "html_url": format!("{base}/releases/tag/v{release}"),
                "published_at": "2026-11-03T10:00:00Z",
                "body": "### Features\n\n* **desktop:** a standalone app installs its own updates ([#50](https://example.org/50))\n",
                "assets": assets,
            });
            type Shared = (
                std::sync::Arc<std::sync::atomic::AtomicBool>,
                Asked,
                serde_json::Value,
                std::sync::Arc<Vec<(String, Vec<u8>)>>,
            );
            fn note(asked: &Asked, path: String, h: &HeaderMap) {
                let ua = h
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                let carried = h.contains_key("authorization")
                    || h.contains_key(reel_server::single::KEY_HEADER);
                asked.lock().unwrap().push((path, ua, carried));
            }
            let app = axum::Router::new()
                .route(
                    "/repos/{owner}/{repo}/releases/latest",
                    axum::routing::get(
                        |State((_, asked, release, _)): State<Shared>,
                         Path((owner, repo)): Path<(String, String)>,
                         h: HeaderMap| async move {
                            note(&asked, format!("/repos/{owner}/{repo}/releases/latest"), &h);
                            axum::Json(release)
                        },
                    ),
                )
                .route(
                    "/download/{tag}/{name}",
                    axum::routing::get(
                        |State((forged, asked, _, files)): State<Shared>,
                         Path((_tag, name)): Path<(String, String)>,
                         h: HeaderMap| async move {
                            note(&asked, format!("/download/{name}"), &h);
                            let Some((_, bytes)) = files.iter().find(|(n, _)| *n == name) else {
                                return StatusCode::NOT_FOUND.into_response();
                            };
                            let mut bytes = bytes.clone();
                            if !name.ends_with(".sig") && forged.load(Ordering::SeqCst) {
                                bytes.extend_from_slice(b"forged");
                            }
                            bytes.into_response()
                        },
                    ),
                )
                .with_state((
                    forged.clone(),
                    asked.clone(),
                    release,
                    std::sync::Arc::new(files),
                ));
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            StandIn {
                base,
                forged,
                asked,
            }
        }
    }

    async fn standalone_read(api: &str, dir: &std::path::Path) -> crate::standalone::Local {
        let local = crate::standalone::Local::start(&crate::standalone::LocalSettings {
            data_dir: dir.join("data"),
            hwaccel: "none".into(),
            cap_gb: 0,
            cap_keeps_unsent: true,
            copies_on_hardware: false,
        })
        .await
        .unwrap();
        let read = reel_pipeline::updates::check_at(&local.ctx, api).await;
        assert!(read.latest.is_some(), "{:?}", read.error);
        local
    }

    fn mock_app(pubkey: &str) -> tauri::App<tauri::test::MockRuntime> {
        let mut ctx = tauri::test::mock_context(tauri::test::noop_assets());
        let mut conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        conf["plugins"]["updater"]["pubkey"] = pubkey.trim().into();
        ctx.config_mut()
            .plugins
            .0
            .insert("updater".into(), conf["plugins"]["updater"].clone());
        tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(ctx)
            .unwrap()
    }

    #[cfg(target_os = "linux")]
    fn release_key() -> String {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        conf["plugins"]["updater"]["pubkey"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// Run under Wine by desktop/ci/check-release-update.sh, which follows the installer once
    /// the updater has ended the process. With `STANDIN_INSTALLER` the release is a stand-in's
    /// with that installer of reel light; without, it is GitHub's newest.
    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "under Wine, by desktop/ci/check-release-update.sh"]
    async fn a_standalone_app_on_windows_runs_the_releases_installer() {
        use reel_pipeline::app_update::{installer_name, Flavour};
        use std::sync::atomic::Ordering;
        use tauri::Manager;
        let pubkey = std::env::var("UPDATE_PUBKEY").expect("UPDATE_PUBKEY");
        // the installer is signed as the version it was built as, which the release names
        let release = std::env::var("STANDIN_VERSION").unwrap_or_else(|_| RELEASE.into());
        let gh = match std::env::var("STANDIN_INSTALLER") {
            Ok(exe) => {
                let name = installer_name(&release, Flavour::Plain);
                Some(
                    StandIn::start(
                        &release,
                        vec![
                            (name.clone(), std::fs::read(&exe).unwrap()),
                            (
                                format!("{name}.sig"),
                                std::fs::read(format!("{exe}.sig")).unwrap(),
                            ),
                        ],
                    )
                    .await,
                )
            }
            Err(_) => None,
        };
        let api = gh
            .as_ref()
            .map(|g| g.base.clone())
            .unwrap_or_else(reel_pipeline::app_update::github_api);
        let dir = tempfile::tempdir().unwrap();
        let local = standalone_read(&api, dir.path()).await;
        let app = mock_app(&pubkey);
        let updater = super::local_builder(app.app_handle(), &local, false)
            .unwrap()
            .build()
            .unwrap();
        let update = super::off_the_pc(
            updater
                .check()
                .await
                .unwrap()
                .expect("the newer release is offered"),
        );
        let name = installer_name(&update.version, Flavour::Plain);
        assert!(
            update.download_url.as_str().ends_with(&name),
            "{}",
            update.download_url
        );
        eprintln!(
            "ok   reel {} found on the release, its installer without OBS offered: {}",
            update.version, update.download_url
        );
        if let Some(gh) = &gh {
            gh.forged.store(true, Ordering::SeqCst);
            let refused = update.download(|_, _| {}, || {}).await.unwrap_err();
            assert!(
                super::not_downloaded(&update.version, &refused, "").contains("was refused"),
                "{refused}"
            );
            eprintln!("ok   a forged installer refused for its signature: {refused}");
            gh.forged.store(false, Ordering::SeqCst);
        }
        let bytes = update.download(|_, _| {}, || {}).await.unwrap();
        eprintln!(
            "ok   the installer downloaded ({} bytes), its signature checked; it runs as an update",
            bytes.len()
        );
        if let Some(gh) = &gh {
            for (path, ua, auth) in gh.asked.lock().unwrap().iter() {
                assert!(ua.starts_with("reel/") && !auth, "{path}: {ua}");
            }
        }
        update.install(bytes).unwrap();
        unreachable!("the updater ends the app as the installer starts");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread")]
    async fn a_standalone_app_installs_the_newer_release_signed_and_refuses_a_forged_one() {
        use reel_pipeline::app_update::{installer_name, Flavour};
        use std::sync::atomic::Ordering;
        use tauri::Manager;
        let name = installer_name(RELEASE, Flavour::AppImage);
        let gh = StandIn::start(
            RELEASE,
            vec![
                (
                    name.clone(),
                    include_bytes!("../tests/release/reel.AppImage").to_vec(),
                ),
                (
                    format!("{name}.sig"),
                    include_bytes!("../tests/release/reel.AppImage.sig").to_vec(),
                ),
            ],
        )
        .await;
        let dir = tempfile::tempdir().unwrap();
        let local = standalone_read(&gh.base, dir.path()).await;
        let app = mock_app(include_str!("../tests/release/key.pub"));
        let running = dir.path().join("reel.AppImage");
        std::fs::write(&running, b"the AppImage that runs").unwrap();
        let updater = |pubkey: Option<String>| {
            let mut b = super::local_builder(app.app_handle(), &local, false)
                .unwrap()
                .executable_path(&running);
            if let Some(key) = pubkey {
                b = b.pubkey(key);
            }
            b.build().unwrap()
        };

        let url = super::endpoint(&local.url, super::RELEASE_PATH, false).unwrap();
        let without_key = super::builder(app.app_handle(), url)
            .unwrap()
            .build()
            .unwrap()
            .check()
            .await;
        assert!(
            without_key.is_err(),
            "reel on this PC answers the app alone: {:?}",
            without_key.map(|u| u.map(|u| u.version))
        );
        let update = super::off_the_pc(
            updater(None)
                .check()
                .await
                .unwrap()
                .expect("the newer release is offered"),
        );
        assert_eq!(update.version, RELEASE);
        assert_eq!(
            update.download_url.as_str(),
            format!(
                "{}/download/v{RELEASE}/reel_{RELEASE}_amd64.AppImage",
                gh.base
            ),
            "the release's own file, straight from it"
        );
        assert_eq!(
            update.signature,
            include_str!("../tests/release/reel.AppImage.sig").trim()
        );
        assert_eq!(
            update
                .body
                .as_deref()
                .and_then(reel_desktop_core::update_steps::news_line),
            Some("a standalone app installs its own updates".to_string())
        );

        gh.forged.store(true, Ordering::SeqCst);
        let refused = update.download(|_, _| {}, || {}).await.unwrap_err();
        assert_eq!(
            super::not_downloaded(RELEASE, &refused, "Install tries again"),
            "reel 9.9.0 was refused: the download does not match a signature of the release key built into this app, so nothing was installed",
            "{refused}"
        );
        gh.forged.store(false, Ordering::SeqCst);

        let by_another = super::off_the_pc(
            updater(Some(release_key()))
                .check()
                .await
                .unwrap()
                .expect("offered, as the key is checked on the download"),
        );
        let refused = by_another.download(|_, _| {}, || {}).await.unwrap_err();
        assert!(
            super::not_downloaded(RELEASE, &refused, "").contains("was refused"),
            "a signature by a key other than the app's: {refused}"
        );
        assert_eq!(std::fs::read(&running).unwrap(), b"the AppImage that runs");

        let bytes = update.download(|_, _| {}, || {}).await.unwrap();
        update.install(&bytes).unwrap();
        assert_eq!(
            std::fs::read(&running).unwrap(),
            include_bytes!("../tests/release/reel.AppImage"),
            "the new AppImage in the place of the one that ran"
        );

        let asked = gh.asked.lock().unwrap().clone();
        assert!(
            asked
                .iter()
                .any(|(p, _, _)| p.ends_with("/releases/latest"))
                && asked.iter().any(|(p, _, _)| p.ends_with(".AppImage.sig"))
                && asked.iter().any(|(p, _, _)| p.ends_with(".AppImage")),
            "{asked:?}"
        );
        for (path, ua, auth) in &asked {
            assert!(ua.starts_with("reel/"), "{path}: {ua}");
            assert!(!auth, "{path} carried an authorisation or the key");
        }
        assert_eq!(
            asked.iter().filter(|(p, _, _)| p.ends_with(".sig")).count(),
            1,
            "the signature is read from the release once: {asked:?}"
        );

        let mut read = local.ctx.updates.read().await.clone();
        if let Some(r) = read.latest.as_mut() {
            r.version = "0.1.0".into();
        }
        *local.ctx.updates.write().await = read;
        assert!(
            updater(None).check().await.unwrap().is_none(),
            "a release as old as the app offers nothing"
        );
        local.stop();
    }

    /// A paired server that offers `installer` as reel 9.9.0, with `signature`.
    #[cfg(target_os = "linux")]
    async fn paired_offering(installer: Vec<u8>, signature: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let offer = serde_json::json!({
            "version": RELEASE,
            "url": format!("{base}/installer"),
            "signature": signature.trim(),
        });
        let app = axum::Router::new()
            .route(
                "/api/device/update",
                axum::routing::get(move || async move { axum::Json(offer) }),
            )
            .route(
                "/installer",
                axum::routing::get(move || async move { installer }),
            );
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        base
    }

    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread")]
    async fn a_paired_server_cannot_pass_an_older_release_off_as_a_newer_one() {
        use tauri::Manager;
        let app = mock_app(include_str!("../tests/release/key.pub"));
        let dir = tempfile::tempdir().unwrap();
        let running = dir.path().join("reel.AppImage");
        let check = |base: String| {
            let url = super::endpoint(&base, "/api/device/update", false).unwrap();
            let updater = super::builder(app.app_handle(), url)
                .unwrap()
                .header("Authorization", "Bearer a-device-token")
                .unwrap()
                .executable_path(&running)
                .build()
                .unwrap();
            async move { updater.check().await.unwrap().expect("offered as 9.9.0") }
        };
        let older = include_bytes!("../tests/release/older.AppImage");
        for (signature, what) in [
            (
                include_str!("../tests/release/older.AppImage.sig"),
                "reel 1.0.0, signed as 1.0.0",
            ),
            (
                include_str!("../tests/release/older.AppImage.unversioned.sig"),
                "reel 1.0.0, signed before signatures named a version",
            ),
        ] {
            std::fs::write(&running, b"the AppImage that runs").unwrap();
            let update = check(paired_offering(older.to_vec(), signature).await).await;
            assert_eq!(update.version, RELEASE, "{what}");
            let refused = match update.download(|_, _| {}, || {}).await {
                Ok(_) => panic!("{what}, offered as {RELEASE}: taken as the newer version"),
                Err(e) => e,
            };
            let line = super::not_downloaded(RELEASE, &refused, "Install tries again");
            assert!(
                line.starts_with("reel 9.9.0 was refused")
                    && line.ends_with("nothing was installed"),
                "{what}: {line}"
            );
            assert!(update.install(older).is_err(), "{what}");
            assert_eq!(
                std::fs::read(&running).unwrap(),
                b"the AppImage that runs",
                "{what}"
            );
        }

        let release = include_bytes!("../tests/release/reel.AppImage");
        let update = check(
            paired_offering(
                release.to_vec(),
                include_str!("../tests/release/reel.AppImage.sig"),
            )
            .await,
        )
        .await;
        let bytes = update.download(|_, _| {}, || {}).await.unwrap();
        update.install(&bytes).unwrap();
        assert_eq!(
            std::fs::read(&running).unwrap(),
            release,
            "reel 9.9.0, signed as 9.9.0, in the place of the one that ran"
        );
    }

    #[test]
    fn an_installer_signed_as_another_version_says_so() {
        use tauri_plugin_updater::Error as E;
        let mismatch = E::SignedVersionMismatch {
            signed: "1.12.0".into(),
            announced: "1.13.0".into(),
        };
        assert_eq!(
            super::not_downloaded("1.13.0", &mismatch, "Install tries again"),
            "reel 1.13.0 was refused: its installer is signed as reel 1.12.0, so nothing was installed"
        );
        assert_eq!(
            super::not_downloaded("1.13.0", &E::MissingSignedVersion, "Install tries again"),
            "reel 1.13.0 was refused: its installer's signature names no version, so nothing was installed"
        );
    }

    #[test]
    fn a_release_that_offers_nothing_says_why() {
        use reel_pipeline::app_update::NotOffered;
        assert_eq!(super::not_offered("1.12.0", NotOffered::NotNewer), "");
        assert_eq!(
            super::not_offered("1.12.0", NotOffered::NoInstaller),
            "reel 1.12.0 is out, and its release has no installer for this app yet; Check now asks again"
        );
        assert_eq!(
            super::not_offered("1.12.0", NotOffered::NoSignature),
            "reel 1.12.0 is out, but its release has no signature for this app's installer, and none is installed without one; Check now asks again"
        );
        assert!(super::not_offered("1.12.0", NotOffered::Deb).contains("install the new .deb"));
        assert!(super::not_offered("1.12.0-rc.1", NotOffered::TestBuild).contains("test build"));
    }

    #[test]
    fn the_key_in_the_app_verifies_what_the_release_key_signed() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let pubkey = conf["plugins"]["updater"]["pubkey"].as_str().unwrap();
        let key = minisign_verify::PublicKey::decode(&decoded(pubkey)).unwrap();
        let data = include_bytes!("../tests/signed.txt");
        let sig =
            minisign_verify::Signature::decode(&decoded(include_str!("../tests/signed.txt.sig")))
                .unwrap();
        key.verify(data, &sig, true).unwrap();
        let mut forged = data.to_vec();
        forged.push(b'!');
        assert!(key.verify(&forged, &sig, true).is_err());
        assert_eq!(
            conf["plugins"]["updater"]["windows"]["installMode"],
            "passive"
        );
    }
}
