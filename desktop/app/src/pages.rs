//! Settings in words: the sentence of what is set, at the head of Essentials.
use reel_desktop_core::bundled::Quality;
use reel_desktop_core::settings::{Mode, ReplayBuffer, Settings, UploadMode};
use serde::Serialize;

use crate::status::replay_words;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Pages {
    /// Essentials' sentence, with `**` around what is set.
    pub summary: String,
}

/// What the sentence needs beyond the settings.
#[derive(Debug, Default)]
pub struct Seen<'a> {
    pub paired: bool,
    /// The paired server: the name it gives itself, or its address while that is reel's own.
    pub server: &'a str,
    /// reel's own OBS keeps the replay buffer; otherwise an OBS of one's own does.
    pub reel_obs: bool,
    /// An OBS of one's own, once it said: the seconds its replay buffer keeps.
    pub replay_seconds: Option<u32>,
}

/// The words for the app as its status has it now.
pub fn of(s: &Settings, st: &crate::status::Status) -> Pages {
    let host = s
        .server_url
        .split_once("://")
        .map_or(s.server_url.as_str(), |(_, rest)| rest)
        .split('/')
        .next()
        .unwrap_or_default();
    let seen = Seen {
        paired: st.paired,
        server: server_word(&st.server_name, host),
        reel_obs: st.obs_box.is_some(),
        replay_seconds: st.obs_state.as_ref().map(|o| o.replay_seconds),
    };
    Pages {
        summary: summary(s, &seen),
    }
}

/// The server by the name it gives itself; reel's own name, or none, says nothing of which
/// server it is, its address does.
pub fn server_word<'a>(name: &'a str, host: &'a str) -> &'a str {
    match name.trim() {
        "" | "reel" => host,
        _ => name,
    }
}

fn quality_word(q: Quality) -> &'static str {
    match q {
        Quality::Light => "Light",
        Quality::Smooth => "Smooth",
        Quality::Sharp => "Sharp",
        Quality::Native => "Native",
        Quality::Custom => "the quality set in OBS",
    }
}

/// What saves a clip, how much, when, at what, and where it goes.
pub fn summary(s: &Settings, f: &Seen) -> String {
    let secs = if f.reel_obs {
        Some(s.obs.setup.clip_seconds)
    } else {
        f.replay_seconds
    };
    let len = match secs {
        Some(x) => format!("the last **{}**", replay_words(x)),
        None => "the last seconds".to_string(),
    };
    let key = s.hotkeys.save.trim();
    let who = if key.is_empty() {
        "Save clip".to_string()
    } else {
        format!("**{key}**")
    };
    let when = match s.obs.replay_buffer {
        ReplayBuffer::Always => "at any time",
        ReplayBuffer::Games => "while a game runs",
    };
    let at = match s.obs.setup.quality {
        _ if !f.reel_obs => String::new(),
        Quality::Custom => ", at the quality set in OBS".to_string(),
        q => format!(", at **{}**", quality_word(q)),
    };
    let goes = if s.mode == Mode::Standalone {
        "Each clip goes into the library on this PC.".to_string()
    } else if !f.paired {
        "No server is paired yet: Connection pairs one.".to_string()
    } else {
        let server = f.server;
        match s.upload.mode {
            UploadMode::Auto => format!("Each clip goes up to **{server}**."),
            UploadMode::Hotkey if !s.hotkeys.upload_last.trim().is_empty() => format!(
                "The last clip goes up to **{server}** on **{}**.",
                s.hotkeys.upload_last.trim()
            ),
            UploadMode::Hotkey => format!("The last clip goes up to **{server}** on its key."),
            UploadMode::Manual => {
                format!("A clip goes up to **{server}** when you send it from Status.")
            }
        }
    };
    format!("{who} saves {len} {when}{at}. {goes}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen() -> Seen<'static> {
        Seen {
            paired: true,
            server: "clips.example.com",
            reel_obs: true,
            ..Seen::default()
        }
    }

    fn settings() -> Settings {
        let mut s = Settings::default();
        s.hotkeys.save = "Alt+F10".into();
        s
    }

    #[test]
    fn the_sentence_says_the_key_the_length_when_the_quality_and_where() {
        let mut s = settings();
        assert_eq!(
            summary(&s, &seen()),
            "**Alt+F10** saves the last **60 s** at any time, at **Smooth**. Each clip goes up to **clips.example.com**."
        );
        s.obs.replay_buffer = ReplayBuffer::Games;
        s.obs.setup.clip_seconds = 120;
        s.obs.setup.quality = Quality::Sharp;
        assert_eq!(
            summary(&s, &seen()),
            "**Alt+F10** saves the last **2 min** while a game runs, at **Sharp**. Each clip goes up to **clips.example.com**."
        );
        s.hotkeys.save.clear();
        s.upload.mode = UploadMode::Hotkey;
        assert_eq!(
            summary(&s, &seen()),
            "Save clip saves the last **2 min** while a game runs, at **Sharp**. The last clip goes up to **clips.example.com** on **Ctrl+Alt+U**."
        );
        s.upload.mode = UploadMode::Manual;
        assert!(summary(&s, &seen())
            .ends_with("A clip goes up to **clips.example.com** when you send it from Status."));
        s.obs.setup.quality = Quality::Custom;
        assert!(summary(&s, &seen()).contains(", at the quality set in OBS."));
    }

    #[test]
    fn the_sentence_for_each_mode_and_an_obs_of_ones_own() {
        let mut s = settings();
        let own = Seen {
            reel_obs: false,
            replay_seconds: Some(300),
            ..seen()
        };
        assert_eq!(
            summary(&s, &own),
            "**Alt+F10** saves the last **5 min** at any time. Each clip goes up to **clips.example.com**."
        );
        let quiet = Seen {
            reel_obs: false,
            ..seen()
        };
        assert!(summary(&s, &quiet).starts_with("**Alt+F10** saves the last seconds at any time."));
        let unpaired = Seen {
            paired: false,
            ..seen()
        };
        assert!(summary(&s, &unpaired).ends_with("No server is paired yet: Connection pairs one."));
        s.mode = Mode::Standalone;
        assert!(summary(&s, &unpaired).ends_with("Each clip goes into the library on this PC."));
    }

    #[test]
    fn a_server_by_its_name_or_its_address() {
        assert_eq!(
            server_word("Sam's clips", "clips.example.com"),
            "Sam's clips"
        );
        assert_eq!(
            server_word("reel", "clips.example.com"),
            "clips.example.com"
        );
        assert_eq!(server_word("", "127.0.0.1:8746"), "127.0.0.1:8746");
    }
}
