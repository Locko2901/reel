//! The encoder the bundled OBS records with: the setup tries each one in OBS itself.
//!
//! obs-websocket makes no output and no encoder and names no encoder OBS has: OBS's outputs
//! take their encoder from the profile when OBS loads it. So a try starts reel's OBS again
//! with the encoder and watches its replay buffer, and OBS's list of encoders comes from the
//! log it writes at each start.
use crate::runtime::Runtime;
use reel_desktop_core::bundled::ObsSetup;
use reel_desktop_core::encoders::{self, Verdict, X264};
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
    if let Some(bx) = bx {
        bx.apply(
            s.obs.setup.clone(),
            s.clips_dir.clone(),
            s.hotkeys.save.clone(),
            Some((tried, s.clips_dir, s.hotkeys.save)),
        );
    }
    let back = until(&rt, START, |x| x.gen > gen && x.connected).await;
    rt.lock().encoder_trial = None;
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
