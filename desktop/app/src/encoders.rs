//! The encoder the bundled OBS records with: the setup tries each one in OBS itself, and a
//! change of encoder that stops the recording is taken back.
//!
//! obs-websocket makes no output and no encoder and names no encoder OBS has: OBS's outputs
//! take their encoder from the profile when OBS loads it. So a try starts reel's OBS again
//! with the encoder and watches its replay buffer, and OBS's list of encoders comes from the
//! log it writes at each start.
use crate::runtime::Runtime;
use reel_desktop_core::bundled::ObsSetup;
use reel_desktop_core::encoders::{self, Sign, Verdict, X264};
use reel_desktop_core::obs::encoder_word;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// The app's connection to OBS and OBS's replay buffer as last seen; `gen` counts connections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Seen {
    pub gen: u64,
    pub connected: bool,
    pub active: bool,
}

/// How long an OBS started again may take to answer: a first start loads its plugins and,
/// on Windows, runs each maker's encoder test in a helper program of its own.
pub const START: Duration = Duration::from_secs(30);
/// From its answer, how long its replay buffer gets to run with the encoder: the encoder's
/// first start, as in the test encode.
pub const BUFFER: Duration = crate::setup::LIMIT;

/// Why OBS did not record with an encoder, in words, and OBS's own lines; `untried` when OBS
/// was not there to try it, which a test again does.
#[derive(Debug, Clone, PartialEq)]
pub struct Refused {
    pub line: String,
    pub details: String,
    pub untried: bool,
}

fn refused(line: impl Into<String>, details: impl Into<String>) -> Refused {
    Refused {
        line: line.into(),
        details: details.into(),
        untried: false,
    }
}

pub fn untried(line: impl Into<String>) -> Refused {
    Refused {
        untried: true,
        ..refused(line, "")
    }
}

fn ms(d: Duration) -> u64 {
    d.as_millis().try_into().unwrap_or(u64::MAX)
}

/// Waits until `ok` holds for what is seen. Err when the time runs out (None) or OBS's
/// supervisor says OBS stopped or does not answer (its line).
async fn until(
    rt: &Runtime,
    within: Duration,
    ok: impl Fn(&Seen) -> bool,
) -> Result<(), Option<String>> {
    let end = Instant::now() + within;
    loop {
        let (seen, line) = {
            let g = rt.lock();
            let seen = *g.obs_seen.borrow();
            (seen, g.obs_box.as_ref().map(|b| b.line()))
        };
        if ok(&seen) {
            return Ok(());
        }
        if let Some(l) =
            line.filter(|l| matches!(l.state.as_str(), "silent" | "restarting" | "failed"))
        {
            return Err(Some(l.line));
        }
        if Instant::now() >= end {
            return Err(None);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Reel's OBS running for the setup's tries; the saved setup comes back when this is dropped,
/// also when another test stops this one halfway.
pub struct Trial {
    rt: Runtime,
    tried: Mutex<Option<ObsSetup>>,
}

impl Trial {
    /// None when the app does not run OBS itself.
    pub fn begin(rt: &Runtime) -> Option<Trial> {
        if !rt.bundled_in_use() || rt.lock().obs_box.is_none() {
            return None;
        }
        Some(Trial {
            rt: rt.clone(),
            tried: Mutex::new(None),
        })
    }

    /// Starts OBS again with the saved setup but `id` as its encoder. The rights for games run
    /// as administrator stay off for it, as only Windows' prompt writes the copy's settings.
    fn start(&self, id: &str) {
        let s = self.rt.settings();
        let trial = ObsSetup {
            encoder: id.to_string(),
            elevated: false,
            ..s.obs.setup.clone()
        };
        let from = self
            .tried
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(trial.clone())
            .unwrap_or_else(|| s.obs.setup.clone());
        let bx = {
            let mut g = self.rt.lock();
            g.encoder_trial = Some(id.to_string());
            g.obs_box.clone()
        };
        if let Some(bx) = bx {
            bx.trying(true);
            bx.apply(
                trial,
                s.clips_dir.clone(),
                s.hotkeys.save.clone(),
                Some((from, s.clips_dir, s.hotkeys.save)),
            );
        }
    }
}

impl Trial {
    /// OBS back on the saved setup, waited for.
    pub async fn finish(self) {
        let tried = self.tried.lock().unwrap_or_else(|e| e.into_inner()).take();
        match tried {
            Some(tried) => restore(self.rt.clone(), tried).await,
            None => self.rt.lock().encoder_trial = None,
        }
    }
}

impl Drop for Trial {
    fn drop(&mut self) {
        let tried = self.tried.lock().unwrap_or_else(|e| e.into_inner()).take();
        let rt = self.rt.clone();
        match tried {
            Some(tried) => {
                tauri::async_runtime::spawn(restore(rt, tried));
            }
            None => rt.lock().encoder_trial = None,
        }
    }
}

/// OBS back on the saved setup after the tries; what the app reads from OBS and the watch of
/// a new encoder wait for it.
async fn restore(rt: Runtime, tried: ObsSetup) {
    let s = rt.settings();
    let (gen, bx) = {
        let g = rt.lock();
        let gen = g.obs_seen.borrow().gen;
        (gen, g.obs_box.clone())
    };
    if let Some(bx) = &bx {
        bx.apply(
            s.obs.setup.clone(),
            s.clips_dir.clone(),
            s.hotkeys.save.clone(),
            Some((tried, s.clips_dir, s.hotkeys.save)),
        );
    }
    let back = until(&rt, START, |x| x.gen > gen && x.connected).await;
    rt.lock().encoder_trial = None;
    if let Some(bx) = bx {
        bx.trying(false);
    }
    tracing::info!(
        answered = back.is_ok(),
        encoder = s.obs.setup.encoder_id(),
        "setup: OBS runs with the saved encoder again"
    );
}

/// The encoders OBS listed at its last start that got as far: the running OBS's log, or the
/// one that came with the app, which the setup's tries run.
fn listed(rt: &Runtime) -> Option<Vec<String>> {
    let bundled = rt.lock().bundled.clone();
    rt.running_obs()
        .and_then(|o| o.encoders_listed())
        .or_else(|| bundled.and_then(|b| b.encoders_listed()))
}

/// The setup's check of an encoder in OBS itself: OBS lists it, and reel's OBS started again
/// with it runs its replay buffer within BUFFER. Ok is how long that took.
pub async fn try_in_obs(trial: &Trial, id: &str, word: &str) -> Result<u64, Refused> {
    let rt = &trial.rt;
    if until(rt, START, |s| s.connected).await.is_err() {
        return Err(untried("not tried in OBS: OBS did not answer"));
    }
    let Some(b) = rt.lock().bundled.clone() else {
        return Err(untried("not tried in OBS: this app came without OBS"));
    };
    let Some(listed) = listed(rt) else {
        return Err(untried(
            "not tried in OBS: OBS's log has no list of its encoders yet",
        ));
    };
    if !listed.iter().any(|e| e == id) {
        return Err(refused(
            format!("OBS here has no {word}"),
            format!(
                "OBS's own list of its encoders at its last start: {}",
                listed.join(", ")
            ),
        ));
    }
    let t = Instant::now();
    let since = SystemTime::now();
    let gen = rt.lock().obs_seen.borrow().gen;
    trial.start(id);
    let why = match until(rt, START, |s| s.gen > gen && s.connected).await {
        Ok(()) => match until(rt, BUFFER, |s| s.gen > gen && s.active).await {
            Ok(()) => return Ok(ms(t.elapsed())),
            Err(line) => line.unwrap_or_else(|| {
                format!(
                    "its replay buffer did not run within {} s",
                    BUFFER.as_secs()
                )
            }),
        },
        Err(line) => line.unwrap_or_else(|| {
            format!(
                "OBS did not answer within {} s of its start with it",
                START.as_secs()
            )
        }),
    };
    let obs_said = b
        .log_since(since)
        .map(|l| encoders::obs_reason(&l))
        .unwrap_or_default();
    b.log_what_obs_said(
        since,
        &format!("setup: OBS did not record with {id} ({why})"),
    );
    let line = match obs_said.first() {
        Some(l) => format!("OBS could not record with it: {l}"),
        None => format!("OBS could not record with it: {why}"),
    };
    let details = if obs_said.is_empty() {
        why
    } else {
        obs_said.join("\n")
    };
    Err(refused(line, details))
}

/// The tests' verdicts kept for Settings and the next start.
pub fn keep_verdicts(rt: &Runtime, rows: &[crate::setup::EncoderTest]) {
    rt.keep_known(|k| {
        for r in rows.iter().filter(|r| r.id != X264) {
            k.set(Verdict {
                id: r.id.clone(),
                ok: r.ok,
                why: if r.ok { String::new() } else { r.line.clone() },
            });
        }
    });
}

/// The encoder the saved setup records with.
fn current(rt: &Runtime) -> String {
    rt.settings().obs.setup.encoder_id().to_string()
}

/// The watch over a change of encoder needs reel's own OBS, recording wanted, and no try of
/// the setup's running.
fn watched(rt: &Runtime) -> bool {
    let g = rt.lock();
    g.bundled.is_some()
        && g.settings.obs.bundled
        && g.settings.obs.start_replay
        && !g.recording_paused
        && g.encoder_trial.is_none()
}

/// Puts OBS's own words of its run since `since` into the app's log after `what`: the end of
/// its log, and the lines that say what failed.
pub fn obs_said(rt: &Runtime, since: Option<SystemTime>, what: &str) {
    let Some(b) = rt.running_obs() else {
        return;
    };
    let since = since.unwrap_or(std::time::UNIX_EPOCH);
    b.log_what_obs_said(since, what);
    let lines = b
        .log_since(since)
        .map(|l| encoders::obs_reason(&l))
        .unwrap_or_default();
    if !lines.is_empty() {
        tracing::warn!("{what}; OBS's lines about it:\n{}", lines.join("\n"));
    }
}

/// After each connection to reel's OBS: its replay buffer must run within BUFFER. When it does
/// not, OBS's words go in the log, and an encoder no replay was saved with yet is taken back.
pub async fn watch_start(rt: Runtime, gen: u64) {
    if !watched(&rt) {
        return;
    }
    let enc = current(&rt);
    let ran = until(&rt, BUFFER, |s| s.gen != gen || s.active).await;
    let (seen, started, since) = {
        let g = rt.lock();
        let seen = *g.obs_seen.borrow();
        (seen, g.encoder_started.clone(), g.obs_since)
    };
    if seen.gen != gen || seen.active || !watched(&rt) || current(&rt) != enc {
        return;
    }
    let why = match ran {
        Err(Some(line)) => line,
        _ => format!(
            "OBS's replay buffer did not run within {} s of its start",
            BUFFER.as_secs()
        ),
    };
    if encoders::undo(
        &rt.lock().encoders,
        &enc,
        started.as_deref(),
        Sign::NotStarted,
    ) {
        take_back(&rt, &enc, &why);
    } else {
        obs_said(
            &rt,
            since,
            &format!("the replay buffer did not start ({why})"),
        );
    }
}

/// OBS's supervisor: an OBS that does not answer or stopped by itself before its replay buffer
/// ran with a new encoder takes the change back.
pub fn on_box(rt: &Runtime, state: &str, line: &str) {
    if !matches!(state, "silent" | "restarting") || !watched(rt) {
        return;
    }
    let enc = current(rt);
    let started = rt.lock().encoder_started.clone();
    if encoders::undo(
        &rt.lock().encoders,
        &enc,
        started.as_deref(),
        Sign::NotStarted,
    ) {
        take_back(rt, &enc, line);
    }
}

/// A save OBS refused or wrote nothing for, with an encoder no replay was saved with yet.
pub fn on_save_failed(rt: &Runtime, why: &str) {
    if !watched(rt) || !rt.lock().obs.connected {
        return;
    }
    let enc = current(rt);
    if encoders::undo(&rt.lock().encoders, &enc, None, Sign::SaveFailed) {
        take_back(rt, &enc, why);
    }
}

/// Goes back to the last encoder that saved a replay (x264 when none did), refuses the one that
/// failed until the setup tests it again, and says so in the state line, the tray's tooltip and
/// a notification; OBS's own lines go in the app's log. It never picks the encoder that failed
/// or one refused; a second call for the same encoder, or one while the setup tries encoders,
/// does nothing.
pub fn take_back(rt: &Runtime, failed: &str, why: &str) {
    back(rt, failed, why, false)
}

fn back(rt: &Runtime, failed: &str, why: &str, now: bool) {
    let (back, b, since) = {
        let mut g = rt.lock();
        if g.encoder_trial.is_some() || g.encoder_back.as_ref().is_some_and(|(f, _)| f == failed) {
            return;
        }
        let back = g.encoders.back_to(failed);
        let words = format!(
            "{} did not record here, so reel records with {} again",
            encoder_word(failed),
            encoder_word(&back)
        );
        g.encoder_back = Some((failed.to_string(), words));
        (back, g.bundled.clone(), g.obs_since)
    };
    tracing::warn!(failed, back = %back, why, "the change of encoder is taken back");
    if b.is_some() {
        obs_said(rt, since, &format!("{failed} did not record ({why})"));
    }
    rt.keep_known(|k| {
        k.set(Verdict {
            id: failed.to_string(),
            ok: false,
            why: format!("it did not record here: {why}"),
        })
    });
    rt.set_encoder(&back, now);
    let words = rt
        .lock()
        .encoder_back
        .as_ref()
        .map(|(_, w)| w.clone())
        .unwrap_or_default();
    crate::notify::show(&rt.app, "Encoder changed back", &words);
    rt.push_status();
}

/// Before reel's OBS starts: an encoder OBS's last start did not list would hold OBS on its
/// "Missing Encoders" dialog, so the setup goes back first.
pub fn before_start(rt: &Runtime) {
    let (bundled, enc) = {
        let g = rt.lock();
        let enc = g.settings.obs.setup.encoder_id().to_string();
        (g.bundled.is_some() && g.settings.obs.bundled, enc)
    };
    let Some(listed) = listed(rt).filter(|_| bundled) else {
        return;
    };
    if enc != X264 && !listed.contains(&enc) {
        back(
            rt,
            &enc,
            &format!(
                "OBS's last start listed no {}; its encoders: {}",
                encoder_word(&enc),
                listed.join(", ")
            ),
            true,
        );
    }
}
