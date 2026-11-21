//! The running check: every 3 s the programs of Games and apps that run, kept for the grace period
//! after they close; the replay buffer kept or stopped by the rules for games, a session
//! recording started and stopped with a game, the game the bar and the tray name, and the
//! countdown to what a grace period ends.
use crate::notify;
use crate::runtime::{Inner, Runtime};
use crate::session::Ask;
use reel_desktop_core::cues::Cue;
use reel_desktop_core::programs::{self, Class, Every, Running, Turn};
use reel_desktop_core::session::By;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const EVERY: Duration = Duration::from_secs(3);

/// Why the rules for games keep the replay buffer off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    /// Only while a game runs, and none runs.
    NoGame,
    /// A program that runs has the buffer Off as its own setting.
    Off(String),
}

impl Held {
    /// Why Save clip cannot run: the tray's line and the window's grey button say it.
    pub fn why(&self) -> String {
        match self {
            Held::NoGame => "the replay buffer runs only while a game runs".into(),
            Held::Off(name) => format!("no replay buffer while {} runs", short(name)),
        }
    }

    /// The Save hotkey's notification says it in full.
    pub fn refusal(&self) -> String {
        match self {
            Held::NoGame => "The replay buffer runs only while a game runs, as set under Settings, and no game runs now.".into(),
            Held::Off(name) => format!(
                "The replay buffer is off while {} runs, as set for it under Games and apps.",
                short(name)
            ),
        }
    }
}

/// A long name cut after its first words where a line repeats it: "Starfall Tactics". A dash
/// cuts only between spaces, so "Half-Life" stays whole.
pub fn short(name: &str) -> String {
    let mut cut = name.split([':', '(']).next().unwrap_or(name);
    for dash in [" - ", " \u{2013} "] {
        cut = cut.split(dash).next().unwrap_or(cut);
    }
    let cut = cut.trim();
    if cut.chars().count() >= 3 && cut.len() < name.len() {
        cut.to_string()
    } else {
        name.to_string()
    }
}

/// The game a line names now: the one that started last of the Games that run.
#[derive(Debug, Clone, serde::Serialize, Default, PartialEq)]
pub struct Playing {
    pub name: String,
    pub short: String,
    pub minutes: u64,
    /// The bar's menu's sentence: "Ember Pass runs for 14 min, a Game: the replay buffer runs."
    pub sentence: String,
    /// The tray's line under its first: "Ember Pass runs: the recording stops when it closes".
    pub line: String,
}

/// A countdown to what the end of a grace period changes: the bar counts down each second to
/// `until_ms` on the page's own clock, the bar's menu says it in a sentence, the tray in a line.
#[derive(Debug, Clone, serde::Serialize, Default, PartialEq)]
pub struct Grace {
    /// The end, in milliseconds since 1970.
    pub until_ms: u64,
    /// The bar's words before the clock: "buffer stops in"; "stops in" after "recording".
    pub bar: String,
    /// The time left when the status was read: "0:45".
    pub left: String,
    /// The menu's sentence, with `{left}` where the clock goes.
    pub sentence: String,
    /// That sentence with the time left when the status was read, as the menu's line has it.
    pub said: String,
    /// The tray's line, with the time of day it ends.
    pub line: String,
}

/// A countdown's time left as the bar shows it, rounded up to the second: "0:45", "10:00".
pub fn clock(left: Duration) -> String {
    let s = left.as_millis().div_ceil(1000);
    format!("{}:{:02}", s / 60, s % 60)
}

/// What a grace period ends, in words: `name` the program that closed, `left` the time until
/// its end, `at` that end's time of day. The bar's words wait for the state (`grace_bar`).
pub fn grace_words(e: &programs::Ending, name: &str, left: Duration, at: &str) -> Grace {
    let n = short(name);
    let (sentence, line) = match (e.recording, e.buffer) {
        (true, Some(Turn::Stops)) => (
            format!("{n} closed: the session recording and the replay buffer stop in {{left}}; if {n} starts again before then, both go on."),
            format!("{n} closed: the recording and the buffer stop at {at}"),
        ),
        (true, Some(Turn::Starts)) => (
            format!("{n} closed: the session recording stops and the replay buffer runs again in {{left}}; if {n} starts again before then, both stay as they are."),
            format!("{n} closed: the recording stops at {at}"),
        ),
        (true, None) => (
            format!("{n} closed: the session recording stops in {{left}}; if {n} starts again before then, it goes on."),
            format!("{n} closed: the recording stops at {at}"),
        ),
        (false, Some(Turn::Starts)) => (
            format!("{n} closed: the replay buffer runs again in {{left}}; if {n} starts again before then, it stays off."),
            format!("{n} closed: the replay buffer runs again at {at}"),
        ),
        (false, _) => (
            format!("{n} closed: the replay buffer stops in {{left}}; if a game starts before then, it goes on."),
            format!("{n} closed: the replay buffer stops at {at}"),
        ),
    };
    let left = clock(left);
    Grace {
        until_ms: 0,
        bar: String::new(),
        said: sentence.replace("{left}", &left),
        left,
        sentence,
        line,
    }
}

/// The bar's words before the clock, after the state's own word: short where that word says
/// what it is about: "stops in" after "recording".
pub fn grace_bar(e: &programs::Ending, state: &str) -> String {
    match (e.recording, e.buffer) {
        (true, _) if state == "recording" => "stops in",
        (true, _) => "recording stops in",
        (false, Some(Turn::Starts)) if state == "buffer_off" => "back in",
        (false, Some(Turn::Starts)) => "buffer back in",
        (false, _) => "buffer stops in",
    }
    .to_string()
}

/// A program not sorted yet that has the whole screen now, for the tray's offer to sort it.
#[derive(Debug, Clone, serde::Serialize, Default, PartialEq)]
pub struct Newcomer {
    pub id: String,
    pub name: String,
}

/// What a look decided, done once the lock is let go.
#[derive(Default)]
struct Acts {
    buffer: bool,
    record: Option<String>,
    stop: Option<String>,
    status: bool,
}

impl Inner {
    /// The listed program a running program is: its file or a Steam folder; by its file's name
    /// alone where Windows did not give its path.
    fn listed(&self, p: &Path) -> Option<String> {
        if p.is_absolute() {
            return self.programs.find(p).map(|e| e.id.clone());
        }
        let name = p.to_string_lossy();
        let mut hits = self.programs.entries.iter().filter(|e| {
            !e.folder
                && e.path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&name))
        });
        match (hits.next(), hits.next()) {
            (Some(e), None) => Some(e.id.clone()),
            _ => None,
        }
    }

    fn running(&self) -> Vec<Running<'_>> {
        let mut ids: Vec<&String> = self.runs.ids().collect();
        ids.sort();
        ids.into_iter()
            .filter_map(|id| self.programs.get(id))
            .map(|entry| Running {
                entry,
                fullscreen: self
                    .front_entry
                    .as_ref()
                    .is_some_and(|(id, full)| *full && *id == entry.id),
            })
            .collect()
    }

    /// The rules for games on the replay buffer, from what runs now.
    pub(crate) fn buffer_rule(&self) -> Option<Held> {
        let every = Every::of(&self.settings);
        let running = self.running();
        if programs::buffer_runs(&every, &running) {
            return None;
        }
        Some(match programs::buffer_stopper(&every, &running) {
            Some(e) => Held::Off(e.name.clone()),
            None => Held::NoGame,
        })
    }

    /// The replay buffer is to stay off: clips paused, or the rules for games.
    pub(crate) fn buffer_off(&self) -> bool {
        self.recording_paused || self.buffer_held.is_some()
    }

    fn look(&mut self, paths: &[PathBuf], now: Instant) -> Acts {
        let mut acts = Acts::default();
        let mut ids: Vec<String> = paths.iter().filter_map(|p| self.listed(p)).collect();
        ids.sort();
        ids.dedup();
        ids.retain(|id| {
            self.programs
                .get(id)
                .is_some_and(|e| e.class != Class::Ignored)
        });
        let grace = Duration::from_secs(u64::from(self.settings.session.grace_seconds));
        let looked = self.runs.look(&ids, now, grace);
        let first = !std::mem::replace(&mut self.runs_looked, true);
        let every = Every::of(&self.settings);
        let name = |g: &Inner, id: &str| {
            g.programs
                .get(id)
                .map(|e| e.name.clone())
                .unwrap_or_default()
        };
        for id in &looked.started {
            if self.programs.seen(id, jiff::Timestamp::now()) {
                if let Err(e) = self.programs.save() {
                    tracing::warn!(error = %e, "games and apps: the list was not written");
                }
            }
            if let Some(e) = self.programs.get(id) {
                tracing::info!(
                    "game: {} started ({}), {}{}",
                    e.name,
                    e.path.display(),
                    class_words(e.class),
                    if first {
                        ", running when reel started"
                    } else {
                        ""
                    }
                );
            }
        }
        for id in &looked.closing {
            let held = self.session.is_some() && self.stop_with.contains(id);
            tracing::info!(
                "game: {} closed{}",
                name(self, id),
                if held {
                    format!(
                        "; the session recording stops in {} s unless it starts again",
                        grace.as_secs()
                    )
                } else {
                    String::new()
                }
            );
        }
        for id in &looked.back {
            tracing::info!(
                "game: {} runs again within the grace period",
                name(self, id)
            );
        }
        let held = self.buffer_rule();
        if held != self.buffer_held {
            match &held {
                Some(Held::Off(n)) => {
                    tracing::info!("replay buffer stopped: {n} runs, its own setting says off")
                }
                Some(Held::NoGame) => tracing::info!(
                    "replay buffer stopped: it runs only while a game runs, and none runs"
                ),
                None => tracing::info!("replay buffer runs again: the rules for games let it"),
            }
            self.buffer_held = held;
            if let Some(bx) = &self.obs_box {
                bx.paused(self.buffer_off());
            }
            acts.buffer = true;
            acts.status = true;
        }
        if self.session.as_ref().is_some_and(|r| r.stopping.is_none()) {
            let stop_ids: Vec<String> = programs::stoppers(&every, &self.running())
                .into_iter()
                .map(|e| e.id.clone())
                .collect();
            self.stop_with.retain(|id| {
                self.programs
                    .get(id)
                    .is_some_and(|e| e.resolve(&every).stop)
            });
            for id in stop_ids {
                if !self.stop_with.contains(&id) {
                    self.stop_with.push(id);
                    acts.status = true;
                }
            }
            if !self.stop_with.is_empty() && self.stop_with.iter().all(|id| !self.runs.contains(id))
            {
                let last = self
                    .stop_with
                    .iter()
                    .rev()
                    .find(|id| looked.closed.contains(id))
                    .or(self.stop_with.last())
                    .cloned()
                    .unwrap_or_default();
                acts.stop = Some(name(self, &last));
                self.stop_with.clear();
            }
        } else if self.session.is_none() {
            self.stop_with.clear();
        }
        if !first && self.session.is_none() && self.session_asked.is_none() {
            acts.record = looked
                .started
                .iter()
                .filter_map(|id| self.programs.get(id))
                .find(|e| e.resolve(&every).record)
                .map(|e| e.name.clone());
        }
        if !looked.started.is_empty()
            || !looked.closed.is_empty()
            || !looked.back.is_empty()
            || !looked.closing.is_empty()
        {
            acts.status = true;
        }
        acts
    }

    /// The Game the bar names: of the Games that run, the one that started last.
    pub(crate) fn playing_id(&self) -> Option<&String> {
        let games = self.runs.ids().filter(|id| {
            self.runs.get(id).is_some_and(|r| r.gone_at.is_none())
                && self
                    .programs
                    .get(id)
                    .is_some_and(|e| e.class == Class::Game)
        });
        self.runs.newest(games)
    }

    /// The game the bar and the tray name, with what its rules give it.
    pub(crate) fn playing(&self) -> Option<Playing> {
        let every = Every::of(&self.settings);
        let id = self.playing_id()?;
        let e = self.programs.get(id)?;
        let run = self.runs.get(id)?;
        let r = e.resolve(&every);
        let s = short(&e.name);
        let minutes = run.since.elapsed().as_secs() / 60;
        let standalone = self.settings.mode == reel_desktop_core::settings::Mode::Standalone;
        let mut gives = Vec::new();
        let mut line = None;
        if self.buffer_held == Some(Held::Off(e.name.clone())) {
            // the state's line says it already
            line = Some("no replay buffer for it".to_string());
        } else if !self.recording_paused && self.buffer_held.is_none() {
            gives.push("the replay buffer runs".to_string());
        }
        if self.session.is_some() && self.stop_with.contains(&e.id) {
            gives.push("the session recording stops when it closes".to_string());
            line = line.or(Some("the recording stops when it closes".to_string()));
        }
        if r.wait == programs::Wait::InFront && cfg!(windows) {
            gives.push(if standalone {
                "encoding waits while it has the screen".to_string()
            } else {
                "uploads wait while it has the screen".to_string()
            });
        }
        let length = if minutes == 0 {
            "under a minute".to_string()
        } else {
            reel_desktop_core::session::length_words(Duration::from_secs(minutes * 60))
        };
        Some(Playing {
            name: e.name.clone(),
            short: s.clone(),
            minutes,
            sentence: format!(
                "{s} runs for {length}, a Game{}.",
                if gives.is_empty() {
                    String::new()
                } else {
                    format!(": {}", gives.join("; "))
                }
            ),
            line: match line {
                Some(l) => format!("{s} runs: {l}"),
                None => format!("{s} runs"),
            },
        })
    }

    /// What the end of a grace period will change, while one runs: the replay buffer, where the
    /// rules for games decide it and OBS would follow, and a session recording that stops with
    /// the programs that closed. The bar's words follow once the state is known (`grace_bar`).
    pub(crate) fn grace(&self) -> Option<(programs::Ending, Grace)> {
        let grace = Duration::from_secs(u64::from(self.settings.session.grace_seconds));
        let every = Every::of(&self.settings);
        let running = self.running();
        let set_up =
            self.settings.obs.enabled || (self.bundled.is_some() && self.settings.obs.bundled);
        let replay = self.obs_state.as_ref().map(|s| s.replay_active);
        // what OBS will do: a buffer that runs stops, one the app starts runs again
        let follows = if programs::buffer_runs(&every, &running) {
            replay == Some(true)
        } else {
            replay == Some(false) && self.settings.obs.start_replay
        };
        let buffer = set_up && self.obs.connected && !self.recording_paused && follows;
        let stops_with = self
            .session
            .as_ref()
            .filter(|r| r.stopping.is_none())
            .map(|_| self.stop_with.as_slice());
        let e = programs::ending(&every, &self.runs, &running, buffer, stops_with, grace)?;
        let left = e.at.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        let name = self.programs.get(&e.id)?.name.clone();
        let end = std::time::SystemTime::now() + left;
        let at = jiff::Timestamp::try_from(end)
            .map(|t| {
                t.to_zoned(jiff::tz::TimeZone::system())
                    .strftime("%H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        let mut g = grace_words(&e, &name, left, &at);
        g.until_ms = end
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or_default();
        Some((e, g))
    }

    /// A program not sorted yet with the whole screen now.
    pub(crate) fn newcomer(&self) -> Option<Newcomer> {
        let (id, full) = self.front_entry.as_ref()?;
        let e = self
            .programs
            .get(id)
            .filter(|e| *full && e.class == Class::NotSorted)?;
        Some(Newcomer {
            id: e.id.clone(),
            name: e.name.clone(),
        })
    }
}

/// The one Game that runs, when one alone does: it names a clip that nothing in front names,
/// as on Linux, where no program in front can be read.
pub(crate) fn game_running_alone(
    runs: &programs::Runs,
    list: &programs::Programs,
) -> Option<String> {
    let mut games = runs.ids().filter_map(|id| {
        let run = runs.get(id)?;
        let e = list.get(id)?;
        (run.gone_at.is_none() && e.class == Class::Game).then(|| e.name.clone())
    });
    match (games.next(), games.next()) {
        (Some(one), None) => Some(one),
        _ => None,
    }
}

fn class_words(c: Class) -> &'static str {
    match c {
        Class::Game => "a Game",
        Class::NotAGame => "Not a game",
        Class::NotSorted => "not sorted yet",
        Class::Ignored => "Ignored",
    }
}

impl Runtime {
    pub(crate) fn spawn_running_check(&self) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let fg = rt.lock().foreground.clone();
                let paths = tauri::async_runtime::spawn_blocking(move || fg.running())
                    .await
                    .unwrap_or_default();
                rt.look_at_running(&paths, Instant::now());
                // the next look comes at a grace period's end, where the bar's countdown ends
                let end = {
                    let g = rt.lock();
                    let grace = Duration::from_secs(u64::from(g.settings.session.grace_seconds));
                    g.runs.next_end(grace)
                };
                let wait = end.map_or(EVERY, |t| {
                    t.saturating_duration_since(Instant::now())
                        .clamp(Duration::from_millis(50), EVERY)
                });
                tokio::time::sleep(wait).await;
            }
        });
    }

    /// One look at what runs, and what the rules for games make of it.
    pub(crate) fn look_at_running(&self, paths: &[PathBuf], now: Instant) {
        let acts = self.lock().look(paths, now);
        if acts.buffer {
            self.apply_buffer();
        }
        if let Some(game) = acts.record {
            self.record_with(&game);
        }
        if let Some(game) = acts.stop {
            self.stop_with_game(&game);
        }
        if acts.status {
            self.push_status();
        }
    }

    /// Starts or stops the replay buffer as clips paused and the rules for games want it.
    pub(crate) fn apply_buffer(&self) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Ok(Some(st)) = rt.obs_state(true).await {
                rt.ensure_replay_buffer(&st).await;
            }
        });
    }

    fn record_with(&self, game: &str) {
        let (key, stops) = {
            let mut g = self.lock();
            g.session_with = Some(game.to_string());
            let every = Every::of(&g.settings);
            let stops = g
                .programs
                .entries
                .iter()
                .find(|e| e.name == game)
                .is_some_and(|e| e.resolve(&every).stop);
            (g.settings.hotkeys.record.clone(), stops)
        };
        tracing::info!("session recording: {game} started, and it records by itself");
        let s = short(game);
        let sooner = if key.is_empty() {
            "the tray or the window".to_string()
        } else {
            key
        };
        notify::show(
            &self.app,
            "Session recording started",
            &if stops {
                format!("{game} started. It stops when {s} closes; {sooner} stops it sooner.")
            } else {
                format!("{game} started. {sooner} stops it.")
            },
        );
        self.record(Ask::Start, By::Game);
    }

    fn stop_with_game(&self, game: &str) {
        let length = self
            .lock()
            .session
            .as_ref()
            .map(|r| {
                reel_desktop_core::session::length_words(r.length(std::time::SystemTime::now()))
            })
            .unwrap_or_default();
        tracing::info!("session recording: {game} closed and the grace period passed; it stops");
        notify::show(
            &self.app,
            &format!("Session recording stopped: {} closed", short(game)),
            &format!("{length} recorded. It goes into the library."),
        );
        self.record(Ask::Stop, By::GameClosed);
    }

    /// A save while the rules for games keep the replay buffer off saves nothing: the No clip
    /// sound, and a notification that says why and where it is set.
    pub(crate) fn buffer_is_held(&self) -> bool {
        let (held, key) = {
            let g = self.lock();
            (g.buffer_held.clone(), g.settings.hotkeys.record.clone())
        };
        let Some(h) = held else {
            return false;
        };
        tracing::info!("save refused: {}", h.why());
        self.cue(Cue::NoClip);
        let instead = if key.is_empty() {
            "Record a session, in the tray or the window, records instead.".to_string()
        } else {
            format!("{key} records a session instead.")
        };
        notify::show_games(
            &self.app,
            "No clip saved",
            &format!("{} {instead}", h.refusal()),
        );
        true
    }

    /// The window's words when its Save clip meets the same.
    pub(crate) fn held_words(&self) -> Option<String> {
        let (held, key) = {
            let g = self.lock();
            (g.buffer_held.clone(), g.settings.hotkeys.record.clone())
        };
        held.map(|h| {
            let record = if key.is_empty() {
                "Record".to_string()
            } else {
                format!("Record ({key})")
            };
            let mut why = h.refusal();
            if let Some(first) = why.get(..1) {
                why = format!("{}{}", first.to_lowercase(), &why[1..]);
            }
            format!("No clip saved: {why} {record} still works.")
        })
    }

    /// Sorts the program offered in the tray.
    pub(crate) fn sort_from_tray(&self, id: &str, class: Class) {
        let changed = {
            let mut g = self.lock();
            match g.programs.get_mut(id) {
                Some(e) => {
                    e.class = class;
                    tracing::info!(name = %e.name, class = ?class, "games and apps: sorted from the tray");
                    let _ = g.programs.save();
                    true
                }
                None => false,
            }
        };
        if changed {
            self.push_status();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_name_is_cut_after_its_first_words() {
        assert_eq!(
            short("Starfall Tactics: The Definitive Remastered Collection"),
            "Starfall Tactics"
        );
        assert_eq!(short("Ember Pass"), "Ember Pass");
        assert_eq!(short("Hl Proto (beta)"), "Hl Proto");
        assert_eq!(short("X: Y"), "X: Y", "a cut too short keeps the name");
        assert_eq!(short("Hollow Pines - Director's Cut"), "Hollow Pines");
        assert_eq!(
            short("Hollow Pines \u{2013} Director's Cut"),
            "Hollow Pines"
        );
        assert_eq!(
            short("Half-Life 2"),
            "Half-Life 2",
            "a dash inside a word stays"
        );
    }

    #[test]
    fn a_countdown_rounds_up_to_the_second() {
        assert_eq!(clock(Duration::from_millis(44_200)), "0:45");
        assert_eq!(clock(Duration::from_secs(60)), "1:00");
        assert_eq!(clock(Duration::from_millis(1)), "0:01");
        assert_eq!(clock(Duration::ZERO), "0:00");
        assert_eq!(clock(Duration::from_secs(600)), "10:00");
    }

    #[test]
    fn what_a_grace_period_ends_in_the_bar_the_menu_and_the_tray() {
        let ending = |recording, buffer| programs::Ending {
            at: Instant::now(),
            id: "p-1".into(),
            recording,
            buffer,
        };
        let left = Duration::from_millis(44_200);
        let name = "Ember Pass: The Long Road";
        let buffer = ending(false, Some(Turn::Stops));
        let g = grace_words(&buffer, name, left, "21:14:05");
        assert_eq!(
            g.said,
            "Ember Pass closed: the replay buffer stops in 0:45; if a game starts before then, it goes on."
        );
        assert_eq!(g.said, g.sentence.replace("{left}", &g.left));
        assert_eq!(
            g.line,
            "Ember Pass closed: the replay buffer stops at 21:14:05"
        );
        assert_eq!(grace_bar(&buffer, "idle"), "buffer stops in");

        let rec = ending(true, None);
        let g = grace_words(&rec, name, left, "21:14:05");
        assert_eq!(
            g.said,
            "Ember Pass closed: the session recording stops in 0:45; if Ember Pass starts again before then, it goes on."
        );
        assert_eq!(g.line, "Ember Pass closed: the recording stops at 21:14:05");
        assert_eq!(
            grace_bar(&rec, "recording"),
            "stops in",
            "after the state's word recording"
        );
        assert_eq!(
            grace_bar(&rec, "obs_off"),
            "recording stops in",
            "another state's word: the countdown says what stops"
        );

        let both = ending(true, Some(Turn::Stops));
        let g = grace_words(&both, name, left, "21:14:05");
        assert_eq!(
            g.said,
            "Ember Pass closed: the session recording and the replay buffer stop in 0:45; if Ember Pass starts again before then, both go on."
        );
        assert_eq!(
            g.line,
            "Ember Pass closed: the recording and the buffer stop at 21:14:05"
        );
        assert_eq!(grace_bar(&both, "recording"), "stops in", "one countdown");

        let back = ending(false, Some(Turn::Starts));
        let g = grace_words(&back, "Starfall Tactics", left, "21:14:05");
        assert_eq!(
            g.said,
            "Starfall Tactics closed: the replay buffer runs again in 0:45; if Starfall Tactics starts again before then, it stays off."
        );
        assert_eq!(
            g.line,
            "Starfall Tactics closed: the replay buffer runs again at 21:14:05"
        );
        assert_eq!(grace_bar(&back, "buffer_off"), "back in");
        assert_eq!(grace_bar(&back, "idle"), "buffer back in");
    }

    #[test]
    fn why_save_clip_cannot_run_and_the_refusal() {
        let off = Held::Off("Starfall Tactics: The Definitive Remastered Collection".into());
        assert_eq!(off.why(), "no replay buffer while Starfall Tactics runs");
        assert_eq!(
            off.refusal(),
            "The replay buffer is off while Starfall Tactics runs, as set for it under Games and apps."
        );
        assert_eq!(
            Held::NoGame.why(),
            "the replay buffer runs only while a game runs"
        );
    }
}
