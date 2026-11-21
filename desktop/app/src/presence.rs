//! Discord's Rich Presence in the app: what friends see follows the Game that runs, the replay
//! buffer and a session recording, sent to the Discord app on this PC as they change.
use crate::detect::short;
use crate::runtime::{Inner, Runtime};
use reel_desktop_core::presence::{self, Facts, Report, Want};
use reel_desktop_core::programs::Class;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;

/// The line under Show on Discord: what reel sent to Discord, or why nothing.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Line {
    /// Empty, "sent" or "warn".
    pub tone: String,
    pub words: String,
    /// What was sent, after the words.
    pub what: String,
}

impl Line {
    fn new(tone: &str, words: &str, what: &str) -> Line {
        Line {
            tone: tone.into(),
            words: words.into(),
            what: what.into(),
        }
    }

    fn of(r: &Report) -> Line {
        match r {
            Report::Sent(s) => Line::new("sent", "Sent to Discord:", &s.words()),
            Report::Refused(why) => Line::new("warn", "Discord refused it:", why),
            Report::NotRunning => Line::new(
                "",
                "Discord is not running on this PC: reel tells it once Discord starts",
                "",
            ),
            Report::Trouble(e) => Line::new(
                "warn",
                &format!(
                    "{e}: reel tries again in {} s",
                    presence::TIMING.retry.as_secs()
                ),
                "",
            ),
            Report::Quiet(Want::Nothing(why)) => Line::new("", "Nothing to send now:", why),
            Report::Quiet(_) => Line::default(),
        }
    }
}

pub struct Discord {
    want: watch::Sender<Want>,
    pub line: Line,
    /// Discord has the presence now.
    pub sent: bool,
}

impl Default for Discord {
    fn default() -> Self {
        Discord {
            want: watch::Sender::new(Want::Off),
            line: Line::default(),
            sent: false,
        }
    }
}

/// The bar's menu says it while Discord has the game that runs: "...: the replay buffer runs; sent
/// to Discord."
pub fn sentence(s: &str, sent: bool) -> String {
    if !sent {
        return s.to_string();
    }
    let s = s.trim_end_matches('.');
    if s.ends_with("a Game") {
        format!("{s}: sent to Discord.")
    } else {
        format!("{s}; sent to Discord.")
    }
}

/// After the countdown's sentence, while Discord has the game that closed: the bar's page counts
/// that sentence down in place, so these words come after it.
pub fn after_grace(s: &str, sent: bool) -> String {
    if sent {
        format!("{s} Discord has it until then.")
    } else {
        s.to_string()
    }
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Inner {
    /// The game Discord is told of: the one the bar names while it runs; once it closed, the one
    /// the bar counts a grace period down for, until the countdown's end. Whether it closed.
    fn discord_game(&self) -> Option<(String, bool)> {
        if let Some(id) = self.playing_id() {
            return Some((id.clone(), false));
        }
        let (e, _) = self.grace()?;
        Some((e.id, true))
    }

    /// Discord has the game now, and whether it closed: what the bar's menu says of it.
    pub(crate) fn discord_told(&self) -> Option<bool> {
        if !self.discord.sent {
            return None;
        }
        self.discord_game().map(|(_, closed)| closed)
    }

    /// The moment for Discord: the game above, whether OBS's replay buffer runs, and a session
    /// recording.
    fn discord_facts(&self) -> Facts {
        let s = &self.settings.discord;
        let game = self.discord_game().and_then(|(id, _)| {
            let e = self.programs.get(&id).filter(|e| e.class == Class::Game)?;
            let run = self.runs.get(&id)?;
            let now = SystemTime::now();
            let started = now.checked_sub(run.since.elapsed()).unwrap_or(now);
            Some((short(&e.name), secs(started)))
        });
        Facts {
            show: s.show,
            name_game: s.name_game,
            button: s.button,
            game,
            replay_buffer: self.obs.connected
                && self.obs_state.as_ref().is_some_and(|o| o.replay_active),
            recording: self.session.as_ref().map(|r| secs(r.started)),
        }
    }
}

impl Runtime {
    pub(crate) fn spawn_discord(&self) {
        let want = self.lock().discord.want.subscribe();
        let rt = self.clone();
        tauri::async_runtime::spawn(presence::run(
            want,
            presence::here,
            presence::TIMING,
            move |r| rt.discord_report(&r),
        ));
    }

    /// With every status: what Discord is to show now, for the task to send once it changed.
    pub(crate) fn discord_poke(&self) {
        let g = self.lock();
        let next = if crate::updates::leaving() {
            Want::Off
        } else {
            presence::want(&g.discord_facts())
        };
        g.discord.want.send_if_modified(|w| {
            let changed = w.differs(&next);
            if changed {
                *w = next;
            }
            changed
        });
    }

    fn discord_report(&self, r: &Report) {
        let line = Line::of(r);
        let sent = matches!(r, Report::Sent(_));
        let changed = {
            let mut g = self.lock();
            let changed = g.discord.line != line || g.discord.sent != sent;
            g.discord.line = line;
            g.discord.sent = sent;
            changed
        };
        if changed {
            self.push_status();
        }
    }

    /// On the way out: Discord drops the presence once the connection closes anyway; clear it
    /// first, for a second at most. Whether there was one.
    pub(crate) fn discord_quit(&self) -> bool {
        let shown = {
            let g = self.lock();
            g.discord.want.send_replace(Want::Off);
            g.discord.sent
        };
        if !shown {
            return false;
        }
        let until = std::time::Instant::now() + Duration::from_secs(1);
        while std::time::Instant::now() < until && self.lock().discord.sent {
            std::thread::sleep(Duration::from_millis(50));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reel_desktop_core::presence::{Doing, Shown};

    #[test]
    fn the_bar_says_discord_has_it_after_what_the_game_gets() {
        let s = "Ember Pass runs for 14 min, a Game: the replay buffer runs.";
        assert_eq!(sentence(s, false), s);
        assert_eq!(
            sentence(s, true),
            "Ember Pass runs for 14 min, a Game: the replay buffer runs; sent to Discord."
        );
        assert_eq!(
            sentence("Ember Pass runs for 14 min, a Game.", true),
            "Ember Pass runs for 14 min, a Game: sent to Discord."
        );
    }

    #[test]
    fn after_a_countdown_the_bar_says_discord_has_the_game_that_closed() {
        let g = "Hollow Pines closed: the replay buffer stops in 0:48; if a game starts before then, it goes on.";
        assert_eq!(after_grace(g, false), g);
        let said = after_grace(g, true);
        assert_eq!(
            said,
            "Hollow Pines closed: the replay buffer stops in 0:48; if a game starts before then, it goes on. Discord has it until then."
        );
        assert!(
            said.starts_with(g),
            "the page counts the countdown's sentence down in place"
        );
    }

    #[test]
    fn the_setting_line_says_what_was_sent_or_why_not() {
        let s = Shown {
            doing: Doing::Clipping,
            game: Some("Starfall Tactics".into()),
            since: 1,
            button: true,
        };
        assert_eq!(
            Line::of(&Report::Sent(s)),
            Line::new("sent", "Sent to Discord:", "Starfall Tactics, Clipping")
        );
        assert_eq!(Line::of(&Report::Quiet(Want::Off)), Line::default());
        assert_eq!(
            Line::of(&Report::Quiet(Want::Nothing("no game runs".into()))),
            Line::new("", "Nothing to send now:", "no game runs")
        );
        assert_eq!(
            Line::of(&Report::Refused("child \"state\" fails (4000)".into())).tone,
            "warn"
        );
        assert!(Line::of(&Report::NotRunning)
            .words
            .contains("once Discord starts"));
    }
}
