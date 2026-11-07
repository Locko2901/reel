//! The bundled OBS's lifetime: started hidden with the app, started again after a crash,
//! stopped cleanly on quit. On Windows it waits while the user's own OBS runs.
use crate::updates::secs;
use reel_desktop_core::bundled::{Bundled, ObsSetup, Platform};
use reel_desktop_core::obsproc;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// What the bundled OBS is doing, for Settings, OBS and the chip.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoxLine {
    /// starting, running, silent, waiting, restarting, stopped, failed
    pub state: String,
    pub line: String,
    pub shown: bool,
    pub version: String,
    pub elevated: bool,
    /// The setup asks for the rights, and OBS runs without them: no task starts it.
    pub without_rights: bool,
    /// The app can show and hide OBS's window: not an OBS with the rights in its own tray.
    pub can_show: bool,
}

/// How the app reaches the window of the OBS it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// Its own OBS: the app shows, hides and closes it itself.
    Own,
    /// The copy's OBS with the rights, through reel-elevate and the second task.
    Task,
    /// OBS with the rights in its own tray: Task Scheduler ends it.
    Tray,
}

/// The setup, the clips folder and the Save hotkey a change of reel's started from.
type From = Option<Box<(ObsSetup, PathBuf, String)>>;

enum Cmd {
    /// A change of reel's own settings from `From`: written once OBS has exited, then OBS
    /// starts again with it; without `From`, only started again.
    Apply(Box<ObsSetup>, PathBuf, String, From),
    /// A setup kept for the next start; what changed was set in the running OBS already.
    Update(Box<ObsSetup>, PathBuf, String),
    Show(bool),
    Stop(std::sync::mpsc::Sender<()>),
    /// Whether OBS's WebSocket answers the app.
    Answered(bool),
    /// A step in Program Files runs: OBS stops, and stays stopped until Release.
    Hold(std::sync::mpsc::Sender<()>),
    Release,
}

/// OBS held stopped while a step in Program Files runs; released when dropped.
pub struct Hold(mpsc::UnboundedSender<Cmd>);

impl Drop for Hold {
    fn drop(&mut self) {
        let _ = self.0.send(Cmd::Release);
    }
}

#[derive(Clone)]
pub struct ObsBox {
    tx: mpsc::UnboundedSender<Cmd>,
    line: Arc<Mutex<BoxLine>>,
    /// An OBS runs that was not asked to close: the app's WebSocket may connect.
    ready: Arc<tokio::sync::watch::Sender<bool>>,
    /// What the app says after OBS's own line.
    note: Arc<Mutex<Option<String>>>,
}

/// A crash right after a start counts toward the third; a run of five minutes clears the count.
const CALM: Duration = Duration::from_secs(300);
/// How long OBS gets to stop its outputs and exit before it is ended.
const GRACE: Duration = Duration::from_secs(10);
/// How long a started OBS may take to answer on its WebSocket before the app says so and
/// shows OBS's windows: an OBS that stops on an error waits on a dialog the app hid.
const SILENT: Duration = Duration::from_secs(15);
/// How long an OBS that answered may stay away on its WebSocket before it is taken for gone
/// (a crash dialog, a hang) and ended, to start again.
const DROPPED: Duration = Duration::from_secs(if cfg!(test) { 2 } else { 20 });

fn backoff(crashes: u32) -> Duration {
    Duration::from_secs(1u64 << crashes.saturating_sub(1).min(6)).min(Duration::from_secs(60))
}

impl ObsBox {
    /// Starts the supervisor; `changed` runs whenever the line changes, `crashed` with the
    /// count at the third crash in a row.
    pub fn start(
        (bundled, copy): (Bundled, Option<Bundled>),
        setup: ObsSetup,
        clips_dir: PathBuf,
        save_key: String,
        changed: impl Fn() + Send + Sync + 'static,
        crashed: impl Fn(u32) + Send + Sync + 'static,
    ) -> ObsBox {
        let (tx, rx) = mpsc::unbounded_channel();
        let line = Arc::new(Mutex::new(BoxLine {
            state: "starting".into(),
            line: "starting OBS".into(),
            shown: false,
            version: reel_desktop_core::bundled::pinned().version,
            elevated: false,
            without_rights: false,
            can_show: true,
        }));
        let pid = Arc::new(AtomicU32::new(0));
        let me = ObsBox {
            tx,
            line: line.clone(),
            ready: Arc::new(tokio::sync::watch::Sender::new(false)),
            note: Arc::new(Mutex::new(None)),
        };
        tauri::async_runtime::spawn(supervise(
            (bundled, copy),
            (setup, clips_dir, save_key),
            rx,
            (line, pid, me.ready.clone()),
            (Arc::new(changed), Arc::new(crashed)),
        ));
        me
    }

    pub fn line(&self) -> BoxLine {
        let mut l = self.line.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(n) = self
            .note
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_deref()
        {
            l.line = format!("{}; {n}", l.line);
        }
        l
    }

    pub fn note(&self, words: Option<String>) {
        *self.note.lock().unwrap_or_else(|e| e.into_inner()) = words;
    }

    /// Whether the app's WebSocket may connect: not to an OBS asked to close.
    pub fn ready(&self) -> tokio::sync::watch::Receiver<bool> {
        self.ready.subscribe()
    }

    /// Starts OBS again with the setup, a change from `from` written first.
    pub fn apply(
        &self,
        setup: ObsSetup,
        clips_dir: PathBuf,
        save_key: String,
        from: Option<(ObsSetup, PathBuf, String)>,
    ) {
        let _ = self.tx.send(Cmd::Apply(
            Box::new(setup),
            clips_dir,
            save_key,
            from.map(Box::new),
        ));
    }

    pub fn show(&self, shown: bool) {
        let _ = self.tx.send(Cmd::Show(shown));
    }

    /// Keeps the setup for the next start, OBS running on as it is.
    pub fn update(&self, setup: ObsSetup, clips_dir: PathBuf, save_key: String) {
        let _ = self
            .tx
            .send(Cmd::Update(Box::new(setup), clips_dir, save_key));
    }

    /// Stops OBS until the Hold is dropped: an OBS running from the copy would write over, as it
    /// exits, the settings a step in Program Files writes.
    pub fn hold(&self) -> Hold {
        let (tx, rx) = std::sync::mpsc::channel();
        let most = GRACE + Duration::from_secs(3);
        if self.tx.send(Cmd::Hold(tx)).is_ok() && rx.recv_timeout(most).is_err() {
            tracing::warn!(after = %secs(most), "OBS's exit was not seen in time for the step in Program Files");
        }
        Hold(self.tx.clone())
    }

    /// The app's WebSocket connection to this OBS came up, or went down.
    pub fn answered(&self, yes: bool) {
        let _ = self.tx.send(Cmd::Answered(yes));
    }

    /// Stops OBS and waits for it, at most the grace period and a little.
    pub fn stop(&self) {
        let (tx, rx) = std::sync::mpsc::channel();
        let most = GRACE + Duration::from_secs(3);
        if self.tx.send(Cmd::Stop(tx)).is_ok() && rx.recv_timeout(most).is_err() {
            tracing::warn!(after = %secs(most), "OBS's exit was not seen in time; the app goes on without it");
        }
    }
}

fn set(line: &Mutex<BoxLine>, changed: &(dyn Fn() + Send + Sync), state: &str, words: String) {
    {
        let mut l = line.lock().unwrap_or_else(|e| e.into_inner());
        if l.state == state && l.line == words {
            return;
        }
        l.state = state.into();
        l.line = words;
        tracing::info!(state, line = %l.line, "the bundled OBS");
    }
    changed();
}

type Callbacks = (Arc<dyn Fn() + Send + Sync>, Arc<dyn Fn(u32) + Send + Sync>);

async fn supervise(
    (bundled, copy): (Bundled, Option<Bundled>),
    (mut setup, mut clips, mut save_key): (ObsSetup, PathBuf, String),
    mut rx: mpsc::UnboundedReceiver<Cmd>,
    (line, pid, ready): (
        Arc<Mutex<BoxLine>>,
        Arc<AtomicU32>,
        Arc<tokio::sync::watch::Sender<bool>>,
    ),
    (changed, crashed): Callbacks,
) {
    let mut crashes = 0u32;
    let mut stop_reply: Option<std::sync::mpsc::Sender<()>> = None;
    let mut held = false;
    let mut hold_reply: Option<std::sync::mpsc::Sender<()>> = None;
    let mut pending: From = None;
    let here = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(std::path::Path::to_path_buf));
    'run: loop {
        ready.send_replace(false);
        if let Some(r) = hold_reply.take() {
            let _ = r.send(());
        }
        if held {
            set(
                &line,
                &*changed,
                "waiting",
                "OBS starts again once the step in Program Files is done".into(),
            );
            match rx.recv().await {
                Some(Cmd::Release) => {
                    held = false;
                    tracing::info!("the step in Program Files is done; OBS starts again");
                }
                Some(Cmd::Hold(r)) => {
                    let _ = r.send(());
                }
                Some(Cmd::Apply(s, c, k, f)) => {
                    pending = pending.or(f);
                    setup = *s;
                    clips = c;
                    save_key = k;
                }
                Some(Cmd::Update(s, c, k)) => {
                    setup = *s;
                    clips = c;
                    save_key = k;
                }
                Some(Cmd::Show(_)) | Some(Cmd::Answered(_)) => {}
                Some(Cmd::Stop(reply)) => {
                    let _ = reply.send(());
                    break 'run;
                }
                None => break 'run,
            }
            continue 'run;
        }
        if here
            .as_deref()
            .is_some_and(reel_desktop_core::install::installing)
        {
            set(
                &line,
                &*changed,
                "waiting",
                "reel is being installed; OBS starts once the installer is done".into(),
            );
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => continue 'run,
                cmd = rx.recv() => match cmd {
                    Some(Cmd::Apply(s, c, k, f)) => { pending = pending.or(f); setup = *s; clips = c; save_key = k; continue 'run; }
                    Some(Cmd::Update(s, c, k)) => { setup = *s; clips = c; save_key = k; continue 'run; }
                    Some(Cmd::Show(_)) | Some(Cmd::Answered(_)) => continue 'run,
                    Some(Cmd::Stop(reply)) => { let _ = reply.send(()); break 'run; }
                    Some(Cmd::Hold(r)) => { held = true; let _ = r.send(()); continue 'run; }
                    Some(Cmd::Release) => { held = false; continue 'run; }
                    None => break 'run,
                },
            }
        }
        let asked = setup.elevated && cfg!(windows);
        let copy_there = asked && copy.as_ref().is_some_and(|c| c.exe.is_file());
        let missing = match asked {
            true => rights_missing(copy_there).await,
            false => None,
        };
        let elevated = asked && missing.is_none();
        let from_copy = elevated && copy_there;
        let run = |s: &ObsSetup| ObsSetup {
            elevated: s.elevated && missing.is_none(),
            ..s.clone()
        };
        let reach = match (elevated, from_copy) {
            (false, _) => Reach::Own,
            (true, true)
                if tokio::task::spawn_blocking(obsproc::window_task_exists)
                    .await
                    .unwrap_or(false) =>
            {
                Reach::Task
            }
            _ => Reach::Tray,
        };
        let obs: &Bundled = match (&copy, from_copy) {
            (Some(c), true) => c,
            _ => &bundled,
        };
        let running = reel_running(obs, &bundled, copy.as_ref(), elevated).await;
        let mut adopt = None;
        for p in running.ours {
            if elevated {
                adopt = Some(p);
            } else {
                tracing::info!(pid = p, "a bundled OBS left from before is ended");
                obsproc::kill(p);
            }
        }
        // On Linux reel's OBS is a second one with settings of its own, beside the user's; on
        // Windows the two would capture the same games at once.
        let beside = bundled.platform == Platform::Linux;
        if let (None, Some(other), false) = (adopt, running.others.first(), beside) {
            set(
                &line,
                &*changed,
                "waiting",
                format!("your own OBS is running ({other}); reel's OBS starts once it is closed, so the two do not capture at once"),
            );
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(10)) => continue 'run,
                cmd = rx.recv() => match cmd {
                    Some(Cmd::Apply(s, c, k, f)) => { pending = pending.or(f); setup = *s; clips = c; save_key = k; continue 'run; }
                    Some(Cmd::Update(s, c, k)) => { setup = *s; clips = c; save_key = k; continue 'run; }
                    Some(Cmd::Show(_)) | Some(Cmd::Answered(_)) => continue 'run,
                    Some(Cmd::Stop(reply)) => { let _ = reply.send(()); break 'run; }
                    Some(Cmd::Hold(r)) => { held = true; let _ = r.send(()); continue 'run; }
                    Some(Cmd::Release) => { held = false; continue 'run; }
                    None => break 'run,
                },
            }
        }
        bundled.link_plugins();
        // The copy's settings only its elevated step writes.
        let from = pending.take();
        let written = if from_copy {
            Ok(())
        } else {
            let change = match &from {
                Some(f) => bundled.put(&bundled.change(
                    &bundled,
                    (&run(&f.0), &f.1, &f.2),
                    (&run(&setup), &clips, &save_key),
                )),
                None => Ok(()),
            };
            // An OBS written for the rights takes the Save hotkey and keeps a tray icon: both
            // go while it runs without them.
            let without = match missing.is_some() && bundled.read_files().is_some() {
                true => bundled.put(&bundled.change(
                    &bundled,
                    (&setup, &clips, &save_key),
                    (&run(&setup), &clips, &save_key),
                )),
                false => Ok(()),
            };
            change
                .and(without)
                .and_then(|_| bundled.ensure(&run(&setup), &clips, &save_key))
        };
        if written.is_err() {
            pending = from;
        }
        if let Err(e) = written {
            set(
                &line,
                &*changed,
                "failed",
                format!("OBS's settings could not be written: {e}"),
            );
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(30)) => continue 'run,
                cmd = rx.recv() => match cmd {
                    Some(Cmd::Stop(reply)) => { let _ = reply.send(()); break 'run; }
                    Some(Cmd::Hold(r)) => { held = true; let _ = r.send(()); continue 'run; }
                    Some(Cmd::Release) => { held = false; continue 'run; }
                    None => break 'run,
                    Some(Cmd::Apply(s, c, k, f)) => { pending = pending.or(f); setup = *s; clips = c; save_key = k; continue 'run; }
                    Some(Cmd::Update(s, c, k)) => { setup = *s; clips = c; save_key = k; continue 'run; }
                    Some(Cmd::Show(_)) | Some(Cmd::Answered(_)) => continue 'run,
                },
            }
        }
        if let Ok(mut l) = line.lock() {
            l.shown = false;
        }
        let started_pid = if let Some(p) = adopt {
            tracing::info!(
                pid = p,
                "the bundled OBS the task started at sign-in is taken over"
            );
            Ok((p, None))
        } else if elevated {
            start_through_task(obs, from_copy).await.map(|p| (p, None))
        } else {
            obsproc::spawn(&bundled)
                .map(|p| (p.pid, Some(p)))
                .map_err(|e| e.to_string())
        };
        let (p, child) = match started_pid {
            Ok(x) => x,
            Err(e) => {
                crashes += 1;
                let wait = backoff(crashes);
                set(
                    &line,
                    &*changed,
                    "failed",
                    format!(
                        "OBS did not start: {e}; trying again in {} s",
                        wait.as_secs()
                    ),
                );
                tokio::select! {
                    _ = tokio::time::sleep(wait) => continue 'run,
                    cmd = rx.recv() => match cmd {
                        Some(Cmd::Stop(reply)) => { let _ = reply.send(()); break 'run; }
                        Some(Cmd::Hold(r)) => { held = true; let _ = r.send(()); continue 'run; }
                        Some(Cmd::Release) => { held = false; continue 'run; }
                        None => break 'run,
                        Some(Cmd::Apply(s, c, k, f)) => { pending = pending.or(f); setup = *s; clips = c; save_key = k; continue 'run; }
                        Some(Cmd::Update(s, c, k)) => { setup = *s; clips = c; save_key = k; continue 'run; }
                        Some(Cmd::Show(_)) | Some(Cmd::Answered(_)) => continue 'run,
                    },
                }
            }
        };
        let started = Instant::now();
        pid.store(p, Ordering::Relaxed);
        ready.send_replace(true);
        let version = match from_copy {
            true => reel_desktop_core::elevated::copy_version(obs),
            false => None,
        }
        .unwrap_or_else(|| reel_desktop_core::bundled::pinned().version);
        if let Ok(mut l) = line.lock() {
            l.elevated = elevated;
            l.without_rights = missing.is_some();
            l.version = version.clone();
            l.can_show = reach != Reach::Tray;
        }
        tracing::info!(pid = p, exe = %obs.exe.display(), elevated, from_copy, reach = ?reach, version, "the bundled OBS started");
        let words = running_words(&bundled, reach, from_copy, missing.as_deref(), &version);
        if reach == Reach::Task && reel_desktop_core::elevated::tray_on(obs) == Some(true) {
            tracing::warn!(
                settings = %obs.config_root.join("obs-studio").join("user.ini").display(),
                "the copy's OBS has its tray icon on in its settings, not as the step wrote them; Update the copy writes them again"
            );
        }
        set(&line, &*changed, "running", words.clone());
        let started_at = std::time::SystemTime::now();
        let hurry = Arc::new(AtomicBool::new(false));
        let watched = hurry.clone();
        let waiter = tokio::task::spawn_blocking(move || match child {
            Some(c) => obsproc::wait(c),
            None => obsproc::wait_pid(p, &watched),
        });
        tokio::pin!(waiter);
        let silence = tokio::time::sleep(SILENT);
        tokio::pin!(silence);
        let mut answered = false;
        let mut silent = false;
        let mut shown_for_silence = false;
        let mut hidden_once = false;
        let mut restarting = false;
        let mut stopping = false;
        let mut asked: Option<Instant> = None;
        let gone = tokio::time::sleep(DROPPED);
        tokio::pin!(gone);
        let mut dropped = false;
        // Why the app ended this OBS itself, for the words after its exit.
        let mut ended: Option<String> = None;
        loop {
            tokio::select! {
                _ = &mut gone, if dropped && !stopping && !restarting => {
                    let p = pid.load(Ordering::Relaxed);
                    let theirs = line.lock().map(|l| l.shown && !shown_for_silence).unwrap_or(false);
                    if theirs {
                        gone.as_mut().reset(tokio::time::Instant::now() + DROPPED);
                        set(&line, &*changed, "silent", "OBS's WebSocket does not answer; reel starts OBS again once its window is hidden".into());
                    } else {
                        dropped = false;
                        let why = format!("OBS stopped answering for {} s, so reel ended it", DROPPED.as_secs());
                        obs.log_what_obs_said(started_at, &format!("the bundled OBS (pid {p}) stopped answering on its WebSocket and is ended"));
                        ended = Some(why);
                        end(reach, p);
                    }
                }
                _ = &mut silence, if !answered && !silent && !dropped && ended.is_none() && !stopping && !restarting => {
                    silent = true;
                    let p = pid.load(Ordering::Relaxed);
                    let words = not_answering(obs, p, started_at, reach);
                    if reach != Reach::Tray {
                        if let Ok(mut l) = line.lock() {
                            shown_for_silence = !l.shown;
                            l.shown = true;
                        }
                        show(reach, p, true);
                    }
                    set(&line, &*changed, "silent", words);
                }
                code = &mut waiter => {
                    ready.send_replace(false);
                    let code = code.unwrap_or(-1);
                    let p = pid.swap(0, Ordering::Relaxed);
                    match asked {
                        Some(a) => tracing::info!(pid = p, code, after = %secs(a.elapsed()), "the bundled OBS exited, after it was asked to close"),
                        None if ended.is_some() => tracing::info!(pid = p, code, "the bundled OBS exited, ended by the app"),
                        None => {
                            tracing::info!(pid = p, code, "the bundled OBS exited");
                            obs.log_what_obs_said(started_at, &format!("the bundled OBS exited by itself (exit code {code})"));
                        }
                    }
                    if stopping {
                        set(&line, &*changed, "stopped", "stopped with the app".into());
                        if let Some(r) = stop_reply.take() { let _ = r.send(()); }
                        break 'run;
                    }
                    if restarting {
                        continue 'run;
                    }
                    if started.elapsed() >= CALM {
                        crashes = 0;
                    }
                    crashes += 1;
                    if crashes == 3 {
                        crashed(crashes);
                    }
                    let wait = backoff(crashes);
                    let why = ended.take().unwrap_or_else(|| format!("OBS stopped (exit code {code})"));
                    set(&line, &*changed, "restarting", format!("{why}; starting it again in {} s", wait.as_secs()));
                    tokio::select! {
                        _ = tokio::time::sleep(wait) => continue 'run,
                        cmd = rx.recv() => match cmd {
                            Some(Cmd::Stop(reply)) => { let _ = reply.send(()); break 'run; }
                            Some(Cmd::Hold(r)) => { held = true; let _ = r.send(()); continue 'run; }
                            Some(Cmd::Release) => { held = false; continue 'run; }
                            None => break 'run,
                            Some(Cmd::Apply(s, c, k, f)) => { pending = pending.or(f); setup = *s; clips = c; save_key = k; continue 'run; }
                            Some(Cmd::Update(s, c, k)) => { setup = *s; clips = c; save_key = k; continue 'run; }
                            Some(Cmd::Show(v)) => { if let Ok(mut l) = line.lock() { l.shown = v; } continue 'run; }
                            Some(Cmd::Answered(_)) => continue 'run,
                        },
                    }
                }
                cmd = rx.recv() => {
                    match cmd {
                        Some(Cmd::Apply(s, c, k, f)) => {
                            pending = pending.or(f);
                            setup = *s;
                            clips = c;
                            save_key = k;
                            restarting = true;
                            asked = Some(Instant::now());
                            hurry.store(true, Ordering::Relaxed);
                            ready.send_replace(false);
                            set(&line, &*changed, "starting", "starting OBS again with the new setup".into());
                            close_then_end(&pid, reach);
                        }
                        Some(Cmd::Show(v)) if reach != Reach::Tray => {
                            tracing::debug!(shown = v, "OBS's window shown or hidden");
                            if let Ok(mut l) = line.lock() { l.shown = v; }
                            show(reach, pid.load(Ordering::Relaxed), v);
                            changed();
                        }
                        Some(Cmd::Show(_)) => {}
                        Some(Cmd::Update(s, c, k)) => {
                            setup = *s;
                            clips = c;
                            save_key = k;
                        }
                        Some(Cmd::Answered(true)) => {
                            answered = true;
                            if std::mem::take(&mut dropped) {
                                tracing::info!("OBS's WebSocket answers again");
                                set(&line, &*changed, "running", words.clone());
                            }
                            // its main window is there by the first answer, and goes unless
                            // someone asked to see it
                            if reach != Reach::Tray && !hidden_once && !beside {
                                hidden_once = true;
                                let keep = line.lock().map(|l| l.shown && !shown_for_silence).unwrap_or(false);
                                if !keep {
                                    if let Ok(mut l) = line.lock() { l.shown = false; }
                                    show(reach, pid.load(Ordering::Relaxed), false);
                                }
                            }
                            if silent {
                                silent = false;
                                if shown_for_silence && reach != Reach::Tray {
                                    if let Ok(mut l) = line.lock() { l.shown = false; }
                                    show(reach, pid.load(Ordering::Relaxed), false);
                                }
                                shown_for_silence = false;
                                set(&line, &*changed, "running", words.clone());
                            }
                        }
                        Some(Cmd::Answered(false)) => {
                            if let Some(a) = asked {
                                tracing::info!(after = %secs(a.elapsed()), "OBS's WebSocket closed while it stops");
                            }
                            // an OBS that answered and went away is gone unless it is back soon;
                            // the silence is for a start that never answered
                            if answered && !stopping && !restarting {
                                answered = false;
                                dropped = true;
                                gone.as_mut().reset(tokio::time::Instant::now() + DROPPED);
                                tracing::warn!(after = %secs(started.elapsed()), "OBS's WebSocket went away; OBS is ended unless it answers again within {} s", DROPPED.as_secs());
                            }
                        }
                        Some(Cmd::Hold(r)) => {
                            held = true;
                            hold_reply = Some(r);
                            restarting = true;
                            asked = Some(Instant::now());
                            hurry.store(true, Ordering::Relaxed);
                            ready.send_replace(false);
                            set(&line, &*changed, "waiting", "OBS stops for the step in Program Files".into());
                            close_then_end(&pid, reach);
                        }
                        Some(Cmd::Release) => {}
                        Some(Cmd::Stop(reply)) => {
                            stopping = true;
                            stop_reply = Some(reply);
                            asked = Some(Instant::now());
                            hurry.store(true, Ordering::Relaxed);
                            ready.send_replace(false);
                            close_then_end(&pid, reach);
                        }
                        None => {
                            stopping = true;
                            hurry.store(true, Ordering::Relaxed);
                            ready.send_replace(false);
                            close_then_end(&pid, reach);
                        }
                    }
                }
            }
        }
    }
    let p = pid.swap(0, Ordering::Relaxed);
    if p != 0 {
        obsproc::kill(p);
    }
}

/// The words for an OBS that does not answer; the end of its own log goes in the app's log,
/// since the user may not reach OBS's folder.
fn not_answering(
    bundled: &Bundled,
    pid: u32,
    since: std::time::SystemTime,
    reach: Reach,
) -> String {
    let titles = obsproc::window_titles(pid);
    let wrote = bundled.log_tail(since, 1).is_some();
    bundled.log_what_obs_said(
        since,
        &format!("the bundled OBS (pid {pid}) does not answer"),
    );
    tracing::warn!(pid, ?titles, "the bundled OBS's windows");
    let mut words = format!(
        "OBS started but does not answer after {} s; it may be showing an error",
        SILENT.as_secs()
    );
    let titles: Vec<&String> = titles.iter().filter(|t| !t.is_empty()).collect();
    if !titles.is_empty() {
        let quoted: Vec<String> = titles.iter().map(|t| format!("\"{t}\"")).collect();
        words.push_str(&format!(" (its windows: {})", quoted.join(", ")));
    }
    if !wrote {
        words.push_str("; it wrote no log of this start, so it stopped before it could");
    }
    words.push_str(if reach == Reach::Tray {
        "; its tray icon opens it"
    } else {
        "; its windows are shown, and the app's log has the details"
    });
    words
}

/// Ends an OBS that does not answer: one with the highest rights through its task, as the
/// app may not end it itself.
fn end(reach: Reach, pid: u32) {
    match reach {
        Reach::Own => obsproc::kill(pid),
        Reach::Task | Reach::Tray => {
            obsproc::task_end();
            obsproc::kill(pid);
        }
    }
}

fn show(reach: Reach, pid: u32, shown: bool) {
    use reel_desktop_core::elevated::Window;
    match reach {
        Reach::Own => obsproc::set_shown(pid, shown),
        Reach::Task => {
            if let Err(e) = obsproc::window_run(if shown { Window::Show } else { Window::Hide }) {
                tracing::warn!(error = %e, shown, "OBS's window with the rights");
            }
        }
        Reach::Tray => {}
    }
}

/// Asks OBS to close and ends it after the grace period, unless the cell was cleared on exit;
/// an OBS with the highest rights is reached through a task: Windows' UIPI drops the app's messages.
fn close_then_end(cell: &Arc<AtomicU32>, reach: Reach) {
    let pid = cell.load(Ordering::Relaxed);
    if pid == 0 {
        return;
    }
    let t = Instant::now();
    match reach {
        Reach::Task => match obsproc::window_run(reel_desktop_core::elevated::Window::Close) {
            Ok(()) => {
                tracing::info!(pid, took = %secs(t.elapsed()), "OBS with the highest rights asked to close through the second task");
                return;
            }
            Err(e) => {
                tracing::warn!(error = %e, "OBS with the highest rights could not be asked to close; Task Scheduler ends it")
            }
        },
        Reach::Tray => {}
        Reach::Own => {
            obsproc::close(pid);
            tracing::info!(pid, took = %secs(t.elapsed()), "OBS asked to close");
            let cell = cell.clone();
            std::thread::spawn(move || {
                // An OBS still starting has no main window to take the close: asked again each
                // second; one that is closing already lets the next ask go.
                let until = Instant::now() + GRACE;
                while Instant::now() < until {
                    std::thread::sleep(Duration::from_secs(1));
                    if cell.load(Ordering::Relaxed) != pid {
                        return;
                    }
                    obsproc::close(pid);
                }
                if cell.load(Ordering::Relaxed) == pid {
                    tracing::warn!(pid, "OBS did not close in time and is ended");
                    obsproc::kill(pid);
                }
            });
            return;
        }
    }
    obsproc::task_end();
    tracing::info!(pid, took = %secs(t.elapsed()), "OBS with the highest rights asked to stop through Task Scheduler");
}

fn running_words(
    obs: &Bundled,
    reach: Reach,
    from_copy: bool,
    missing: Option<&str>,
    version: &str,
) -> String {
    if obs.platform == Platform::Linux {
        return format!(
            "running in its own tray icon: {}, with settings of its own",
            obs.which()
        );
    }
    let words = match (reach, from_copy) {
        (Reach::Task, _) => {
            "running with the highest rights from its copy in Program Files, hidden"
        }
        (Reach::Tray, true) => {
            "running with the highest rights from its copy in Program Files, in its own tray"
        }
        (Reach::Tray, false) => "running with the highest rights, in its own tray",
        (Reach::Own, _) => "running, hidden",
    };
    let wanted = reel_desktop_core::bundled::pinned().version;
    match missing {
        Some(why) => format!("{words}, without the highest rights, so games that run as administrator are not captured: {why}"),
        None if from_copy && version != wanted => format!(
            "{words}; the copy is OBS {version}, and this version of reel brought OBS {wanted}: Settings, OBS, Update the copy"
        ),
        None => words.to_string(),
    }
}

/// Why OBS cannot run with the highest rights the setup asks for.
async fn rights_missing(copy_there: bool) -> Option<String> {
    let task = tokio::task::spawn_blocking(obsproc::task_exists)
        .await
        .unwrap_or(false);
    match (task, copy_there) {
        (true, _) => None,
        (false, true) => Some(
            "the task that starts its copy in Program Files is missing; Settings, OBS, Update the copy puts it back".into(),
        ),
        (false, false) => Some(
            "the copy of OBS in Program Files is not there; Settings, OBS, Put it in place".into(),
        ),
    }
}

/// `obs`'s processes and the user's own OBS; one with the highest rights shows no path while
/// reel's task runs, and reel's other OBS is ended rather than taken for the user's.
async fn reel_running(
    obs: &Bundled,
    bundled: &Bundled,
    copy: Option<&Bundled>,
    elevated: bool,
) -> obsproc::Running {
    let mut running = obsproc::running(obs);
    if elevated
        && running.ours.is_empty()
        && !running.unreadable.is_empty()
        && tokio::task::spawn_blocking(obsproc::task_running)
            .await
            .unwrap_or(false)
    {
        let p = running.unreadable[0];
        tracing::info!(
            pid = p,
            "an OBS runs whose path Windows does not show, while reel's task runs: taken for the task's"
        );
        running.ours.push(p);
        if let Some(i) = running
            .others
            .iter()
            .position(|o| reel_desktop_core::bundled::is_obs_exe(o))
        {
            running.others.remove(i);
        }
    }
    for other in [Some(bundled), copy].into_iter().flatten() {
        if other.exe == obs.exe {
            continue;
        }
        let key = reel_desktop_core::install::path_key(&other.exe.to_string_lossy());
        let left = obsproc::running(other).ours;
        if !left.is_empty() {
            tracing::info!(exe = %other.exe.display(), "reel's other OBS, left from before, is ended");
            if copy.is_some_and(|c| c.exe == other.exe) {
                obsproc::task_end();
            }
            for p in left {
                obsproc::kill(p);
            }
        }
        running
            .others
            .retain(|o| reel_desktop_core::install::path_key(o) != key);
    }
    running
}

/// Starts OBS through the task and finds its process, the task giving no process id back.
async fn start_through_task(b: &Bundled, from_copy: bool) -> Result<u32, String> {
    if !obsproc::task_exists() {
        return Err("the task for games that run as administrator is missing; turn Capture games that run as administrator off and on again".into());
    }
    obsproc::task_run()?;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let r = obsproc::running(b);
        if let Some(p) = r.ours.first() {
            return Ok(*p);
        }
        if from_copy && !r.unreadable.is_empty() {
            return Ok(r.unreadable[0]);
        }
    }
    Err("the task ran, and no OBS came up in twenty seconds".into())
}

#[cfg(test)]
mod tests {
    use super::backoff;
    use std::time::Duration;

    /// A stand-in OBS that writes "start" as it starts and "stop" when asked to close, with
    /// what it wrote and a wait for it.
    #[cfg(target_os = "linux")]
    struct Standin {
        dir: tempfile::TempDir,
        exe: std::path::PathBuf,
        log: std::path::PathBuf,
    }

    #[cfg(target_os = "linux")]
    impl Standin {
        fn new() -> Standin {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let log = dir.path().join("log");
            let exe = dir.path().join("standin-obs");
            std::fs::write(
                &exe,
                format!(
                    "#!/bin/sh\necho start >> {0}\ntrap 'echo stop >> {0}; exit 0' TERM\nwhile :; do sleep 0.1; done\n",
                    log.display()
                ),
            )
            .unwrap();
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
            Standin { dir, exe, log }
        }

        fn said(&self) -> String {
            std::fs::read_to_string(&self.log).unwrap_or_default()
        }

        fn wait_for(&self, want: &str) {
            for _ in 0..100 {
                if self.said() == want {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            panic!("the stand-in said {:?}, not {want:?}", self.said());
        }

        fn start(&self, setup: reel_desktop_core::bundled::ObsSetup) -> super::ObsBox {
            super::ObsBox::start(
                (
                    reel_desktop_core::bundled::Bundled::linux(self.exe.clone(), self.dir.path()),
                    None,
                ),
                setup,
                self.dir.path().join("clips"),
                "F9".into(),
                || {},
                |_| {},
            )
        }
    }

    /// The box's line once it is in `state`.
    #[cfg(target_os = "linux")]
    fn line_in(bx: &super::ObsBox, state: &str) -> super::BoxLine {
        for _ in 0..200 {
            let l = bx.line();
            if l.state == state {
                return l;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the line is {:?}, not in {state}", bx.line());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_obs_whose_websocket_goes_away_is_ended_and_started_again() {
        let st = Standin::new();
        let bx = st.start(Default::default());
        st.wait_for("start\n");
        bx.answered(true);
        bx.answered(false);
        let l = line_in(&bx, "restarting");
        assert!(
            l.line.starts_with(
                "OBS stopped answering for 2 s, so reel ended it; starting it again in "
            ),
            "{}",
            l.line
        );
        st.wait_for("start\nstart\n");
        bx.answered(true);
        bx.answered(false);
        bx.answered(true);
        std::thread::sleep(super::DROPPED + Duration::from_secs(1));
        assert_eq!(st.said(), "start\nstart\n", "back in time: left running");
        assert_eq!(bx.line().state, "running");
        bx.stop();
        st.wait_for("start\nstart\nstop\n");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_held_obs_stays_stopped_until_released_then_starts_once() {
        use reel_desktop_core::bundled::ObsSetup;
        let st = Standin::new();
        let (said, wait_for) = (|| st.said(), |w: &str| st.wait_for(w));
        let clips = st.dir.path().join("clips");
        let bx = st.start(ObsSetup::default());
        wait_for("start\n");
        let held = bx.hold();
        assert_eq!(said(), "start\nstop\n", "out before the hold returns");
        bx.apply(
            ObsSetup {
                clip_seconds: 45,
                ..ObsSetup::default()
            },
            clips,
            "F9".into(),
            None,
        );
        std::thread::sleep(Duration::from_secs(3));
        assert_eq!(said(), "start\nstop\n", "not started again while held");
        assert_eq!(bx.line().state, "waiting");
        drop(held);
        wait_for("start\nstop\nstart\n");
        std::thread::sleep(Duration::from_secs(2));
        assert_eq!(said(), "start\nstop\nstart\n", "started once");
        bx.stop();
        wait_for("start\nstop\nstart\nstop\n");
    }

    #[test]
    fn the_line_says_why_obs_runs_without_the_rights_and_which_obs_the_copy_is() {
        use super::{running_words, Reach};
        let b = reel_desktop_core::bundled::Bundled::windows(std::path::Path::new(r"C:\reel"));
        let wanted = reel_desktop_core::bundled::pinned().version;
        assert_eq!(
            running_words(&b, Reach::Own, false, None, &wanted),
            "running, hidden"
        );
        let why = "the copy of OBS in Program Files is not there; Settings, OBS, Put it in place";
        assert_eq!(
            running_words(&b, Reach::Own, false, Some(why), &wanted),
            format!("running, hidden, without the highest rights, so games that run as administrator are not captured: {why}")
        );
        assert_eq!(
            running_words(&b, Reach::Task, true, None, &wanted),
            "running with the highest rights from its copy in Program Files, hidden"
        );
        assert_eq!(
            running_words(&b, Reach::Task, true, None, "32.2.2"),
            format!("running with the highest rights from its copy in Program Files, hidden; the copy is OBS 32.2.2, and this version of reel brought OBS {wanted}: Settings, OBS, Update the copy")
        );
    }

    #[test]
    fn the_pause_before_a_restart_grows_to_a_minute() {
        let secs: Vec<u64> = (1..=9).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(secs, [1, 2, 4, 8, 16, 32, 60, 60, 60]);
        assert_eq!(backoff(0), Duration::from_secs(1));
    }
}
