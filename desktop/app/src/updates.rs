//! The app's own updates from its paired server: the installer of the server's version, signed
//! with the release key and checked against the key built into the app.
use crate::notify;
use crate::runtime::Runtime;
use reel_desktop_core::settings::{Mode, UpdateMode};
use reel_desktop_core::update_steps::{self, Kind, Note};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;

pub fn releases_url() -> String {
    "https://github.com/Locko2901/reel/releases".into()
}

fn set_line(rt: &Runtime, line: impl Into<String>) {
    rt.lock().update_line = line.into();
    rt.push_status();
}

const EVERY: Duration = Duration::from_secs(6 * 3600);

/// 45 s after the start, then every six hours by the wall clock: a timer stops while the PC
/// sleeps, so a long one comes days late on a PC that sleeps at night.
pub fn spawn_checks(rt: Runtime) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(45)).await;
        loop {
            if let Err(e) = check(&rt, false).await {
                tracing::warn!(error = %e, "update check");
            }
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
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
async fn download(rt: &Runtime, update: &tauri_plugin_updater::Update) -> Result<Vec<u8>, String> {
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
        .map_err(|e| e.to_string())
}

/// How the update is put in place: the Windows installer, with OBS or without, or the AppImage.
pub fn kind(rt: &Runtime) -> Kind {
    Kind::here(rt.lock().bundled.is_some())
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
        // the line is what the standalone release check read; Check now reads the release again
        rt.lock().update_checked_at = Some(SystemTime::now());
        let local = rt.lock().local.clone();
        if let (true, Some(local)) = (by_hand, local) {
            reel_pipeline::updates::check_once(&local.ctx).await;
        }
        rt.push_status();
        return Ok(());
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
    // reel light sends no flavour, the request of an app older than the two installers
    let flavour = if rt.lock().bundled.is_some() {
        "&flavour=with-obs"
    } else {
        ""
    };
    let url = format!(
        "{}/api/device/update?current={{{{current_version}}}}&target={{{{target}}}}&arch={{{{arch}}}}{flavour}",
        s.server_url.trim_end_matches('/')
    )
    .parse()
    .map_err(|e| format!("{e}"))?;
    let updater = rt
        .app
        .updater_builder()
        .endpoints(vec![url])
        .and_then(|b| b.header("Authorization", format!("Bearer {token}")))
        .map(|b| {
            // the installer starts reel again in the tray; its integrity check is skipped, the
            // update's signature having been checked
            b.restart_after_install(false)
                .installer_args(["/NCRC", "/R", "/ARGS", "--hidden"])
        })
        .and_then(|b| {
            // Windows' installer ends the app at once: the log's last lines go first
            let out = rt.clone();
            b.on_before_exit(move || {
                tracing::info!("quit: the update's installer starts, and the app exits");
                drop(out.lock().log.take());
            })
            .build()
        })
        .map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            tracing::info!(version, "an update of the app is ready on the server");
            rt.lock().update = Some(update.clone());
            let current = rt.app.package_info().version.to_string();
            match s.updates {
                _ if test_build(&current) => {
                    set_line(
                        rt,
                        format!("reel {version} is on the server; this test build ({current}) installs it only when asked"),
                    );
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
                        Err(e) => set_line(
                            rt,
                            format!("reel {version} did not download: {e}; Check now tries again"),
                        ),
                    }
                }
                _ => {
                    set_line(rt, format!("reel {version} is ready to install"));
                    let told = rt.lock().update_told.replace(version.clone());
                    if told.as_deref() != Some(version.as_str()) {
                        notify::show(
                            &rt.app,
                            &format!("reel {version} is ready"),
                            "Install it from the tray, or under This app, Startup.",
                        );
                    }
                }
            }
        }
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

/// A build named after a commit (1.2.0-dev.abc1234): its release counts as newer, so it is
/// offered but never downloaded or installed on a quit by itself.
fn test_build(version: &str) -> bool {
    version.contains('-')
}

/// Downloads with the window there, then installs as the app quits: on Windows the installer's
/// small window, then reel again; on Linux the new AppImage over the running one.
pub async fn install_now(rt: &Runtime) -> Result<(), String> {
    let update = rt
        .lock()
        .update
        .clone()
        .ok_or_else(|| "no update to install".to_string())?;
    let (title, body) = update_steps::going(&update.version, kind(rt), false);
    notify::show(&rt.app, &title, &body);
    // never in the middle of a clip: one on its way to the server goes up first
    for n in 0..300 {
        let busy = {
            let g = rt.lock();
            g.uploading.is_some() || g.share_running
        };
        if !busy {
            break;
        }
        if n == 0 {
            set_line(
                rt,
                format!(
                    "reel {} installs once the clip on its way is up",
                    update.version
                ),
            );
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    set_line(rt, format!("installing reel {}", update.version));
    tracing::info!(version = %update.version, "installing the update");
    let ready = rt.lock().update_bytes.take();
    let bytes = match ready {
        Some(b) => b,
        None => download(rt, &update).await.map_err(|e| {
            let words = format!(
                "reel {} did not download: {e}; Install tries again",
                update.version
            );
            set_line(rt, words.clone());
            words
        })?,
    };
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
        match (g.update.clone(), g.update_bytes.take()) {
            (Some(u), Some(b)) => Some((u, b)),
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
    /// Install reel from the tray or This app; the app starts again if it does not install.
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
    use base64::Engine;

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
    fn a_build_named_after_a_commit_never_updates_itself() {
        assert!(super::test_build("1.2.0-dev.e58080a"));
        assert!(!super::test_build("1.2.0"));
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
