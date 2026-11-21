//! Settings in words: the first page's sentence of what is set, and one line per page of what
//! is set there, for the rows that open them.
use reel_desktop_core::bundled::Quality;
use reel_desktop_core::settings::{Mode, ReplayBuffer, Settings, TrayIcon, UploadMode};
use serde::Serialize;
use std::collections::BTreeMap;

use crate::status::replay_words;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Pages {
    /// The first page's sentence, with `**` around what is set.
    pub summary: String,
    /// What is set on each of the app's pages, by the page's id.
    pub lines: BTreeMap<&'static str, String>,
}

/// What the words need beyond the settings.
#[derive(Debug, Default)]
pub struct Seen<'a> {
    pub paired: bool,
    /// The paired server: the name it gives itself, or its address while that is reel's own.
    pub server: &'a str,
    pub user: &'a str,
    /// reel's own OBS keeps the replay buffer; otherwise an OBS of one's own does.
    pub reel_obs: bool,
    /// An OBS of one's own, once it said: the seconds its replay buffer keeps.
    pub replay_seconds: Option<u32>,
    pub linux: bool,
    /// The games that record by themselves, by name.
    pub recording: Vec<String>,
    /// The app's version, and where it stands: the newest, an update ready.
    pub version: &'a str,
    pub standing: &'a str,
    /// The app's state in a word or two, as the bar says it.
    pub state: &'a str,
    /// Standalone: the library's folder, and the encoder reel on this PC uses.
    pub library_dir: &'a str,
    pub encoder: &'a str,
}

/// The words for the app as its status has it now.
pub fn of(
    s: &Settings,
    st: &crate::status::Status,
    programs: &reel_desktop_core::programs::Programs,
) -> Pages {
    use reel_desktop_core::programs::{Class, Own};
    let host = s
        .server_url
        .split_once("://")
        .map_or(s.server_url.as_str(), |(_, rest)| rest)
        .split('/')
        .next()
        .unwrap_or_default();
    let recording = programs
        .entries
        .iter()
        .filter(|e| e.class != Class::Ignored && e.own.record == Own::On)
        .map(|e| e.name.clone())
        .collect();
    let encoder = match &st.local {
        Some(l) => l.backend.as_str(),
        None => s.standalone.hwaccel.as_str(),
    };
    let library_dir = match &st.local {
        Some(l) => l.library_dir.clone(),
        None => s.standalone.library_dir.display().to_string(),
    };
    pages(
        s,
        &Seen {
            paired: st.paired,
            server: server_word(&st.server_name, host),
            user: &st.user,
            reel_obs: st.obs_box.is_some(),
            replay_seconds: st.obs_state.as_ref().map(|o| o.replay_seconds),
            linux: st.linux,
            recording,
            version: &st.versions.app,
            standing: &st.versions.standing,
            state: &st.state_word,
            library_dir: &library_dir,
            encoder: encoder_word(encoder),
        },
    )
}

/// The server by the name it gives itself; reel's own name, or none, says nothing of which
/// server it is, its address does.
pub fn server_word<'a>(name: &'a str, host: &'a str) -> &'a str {
    match name.trim() {
        "" | "reel" => host,
        _ => name,
    }
}

/// The library's encoder by the word Settings, Encoding gives it.
fn encoder_word(id: &str) -> &'static str {
    match id {
        "nvenc" => "NVIDIA NVENC",
        "amf" => "AMD AMF",
        "qsv" => "Intel Quick Sync",
        "vaapi" => "VAAPI",
        "none" => "x264 on the CPU",
        _ => "automatic GPU",
    }
}

pub fn pages(s: &Settings, f: &Seen) -> Pages {
    Pages {
        summary: summary(s, f),
        lines: lines(s, f),
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

fn names(list: &[String]) -> String {
    match list {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// One line per page: what is set there, the most telling first.
pub fn lines(s: &Settings, f: &Seen) -> BTreeMap<&'static str, String> {
    let standalone = s.mode == Mode::Standalone;
    let join = |parts: Vec<String>| {
        parts
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" \u{b7} ")
    };
    let mut out = BTreeMap::new();

    let keys = [&s.hotkeys.save_share, &s.hotkeys.share_last]
        .iter()
        .filter(|k| !k.trim().is_empty())
        .count();
    let cues = s.sounds.captured.enabled as usize
        + s.sounds.uploaded.enabled as usize
        + s.sounds.no_clip.enabled as usize
        + (!standalone && s.sounds.link_copied.enabled) as usize;
    out.insert(
        "clips",
        join(vec![
            if s.upload.keep_last == 0 {
                "Keep the last off".into()
            } else {
                format!("Keep the last {}", replay_words(s.upload.keep_last as u32))
            },
            if standalone || keys == 0 {
                String::new()
            } else {
                plural(keys, "more key", "more keys")
            },
            plural(cues, "sound", "sounds"),
            if standalone {
                String::new()
            } else if s.cache.days > 0 {
                format!("kept {} days", s.cache.days)
            } else if s.cache.gb > 0 {
                format!("kept up to {} GB", s.cache.gb)
            } else {
                "deleted once on the server".into()
            },
        ]),
    );

    out.insert(
        "session",
        join(vec![
            if s.hotkeys.record.trim().is_empty() {
                "no key".into()
            } else {
                s.hotkeys.record.trim().to_string()
            },
            if s.session.record_by_itself {
                "every game by itself".into()
            } else if f.recording.is_empty() {
                String::new()
            } else {
                let by = if f.recording.len() == 1 {
                    "itself"
                } else {
                    "themselves"
                };
                format!("{} by {by}", names(&f.recording))
            },
            if s.session.remind_minutes == 0 {
                "no reminders".into()
            } else {
                format!("remind every {} min", s.session.remind_minutes)
            },
        ]),
    );

    let setup = &s.obs.setup;
    out.insert(
        "capture",
        if f.reel_obs {
            join(vec![
                reel_desktop_core::obs::encoder_word(if setup.encoder.is_empty() {
                    "obs_x264"
                } else {
                    &setup.encoder
                }),
                if setup.game {
                    "the game in front".into()
                } else {
                    "the screen".into()
                },
                if setup.desktop {
                    "desktop audio".into()
                } else {
                    String::new()
                },
                "reel's OBS".into(),
            ])
        } else if s.obs.enabled {
            join(vec![
                "your own OBS".into(),
                format!("{}:{}", s.obs.host, s.obs.port),
            ])
        } else {
            "no OBS yet".into()
        },
    );

    out.insert(
        "games",
        join(vec![
            format!("Steam {}", if s.games.steam { "on" } else { "off" }),
            if s.discord.show {
                "on Discord".into()
            } else {
                String::new()
            },
            format!("grace period {}", replay_words(s.session.grace_seconds)),
        ]),
    );

    if !standalone {
        out.insert(
            "upload",
            join(vec![
                if s.upload.bandwidth_kbit == 0 {
                    "no limit".into()
                } else {
                    format!("up to {} kbit/s", s.upload.bandwidth_kbit)
                },
                match (f.linux, s.upload.wait_for_games) {
                    (true, _) => String::new(),
                    (false, true) => "they wait for a game".into(),
                    (false, false) => "they never wait for a game".into(),
                },
            ]),
        );
    }

    out.insert(
        "server",
        match (standalone, f.paired) {
            (false, true) => join(vec![f.server.into(), format!("paired as {}", f.user)]),
            (false, false) if s.server_url.trim().is_empty() => "no server yet".into(),
            (false, false) => join(vec![f.server.into(), "not paired".into()]),
            (true, true) => join(vec!["standalone".into(), format!("{} paired", f.server)]),
            (true, false) => "standalone \u{b7} no server".into(),
        },
    );

    out.insert(
        "startup",
        join(vec![
            match f.standing {
                "same" | "alone" if !f.version.is_empty() => format!("{}, the newest", f.version),
                "update" => format!("{}, an update is ready", f.version),
                _ => f.version.into(),
            },
            format!(
                "tray icon {}",
                match s.tray_icon {
                    TrayIcon::Colour => "in colour",
                    TrayIcon::White => "white",
                    TrayIcon::Black => "black",
                }
            ),
        ]),
    );

    out.insert("status", join(vec![f.state.into(), "the log".into()]));

    if standalone {
        out.insert(
            "library",
            join(vec![
                f.library_dir.into(),
                if s.standalone.cap_gb == 0 {
                    "no cap".into()
                } else {
                    format!("up to {} GB", s.standalone.cap_gb)
                },
            ]),
        );
        out.insert(
            "encoder",
            join(vec![
                f.encoder.into(),
                match (f.linux, s.upload.wait_for_games) {
                    (true, _) => String::new(),
                    (false, true) => "waits for a game".into(),
                    (false, false) => "never waits".into(),
                },
            ]),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen() -> Seen<'static> {
        Seen {
            paired: true,
            server: "clips.example.com",
            user: "sam",
            reel_obs: true,
            version: "1.19.0",
            standing: "same",
            state: "ready",
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
    fn each_page_says_what_is_set_there() {
        let mut s = settings();
        s.cache.days = 30;
        s.session.remind_minutes = 30;
        let f = Seen {
            recording: vec!["Ember Pass".into(), "Ridgeback Rally".into()],
            ..seen()
        };
        let l = lines(&s, &f);
        assert_eq!(
            l["clips"],
            "Keep the last off \u{b7} 2 more keys \u{b7} 4 sounds \u{b7} kept 30 days"
        );
        assert_eq!(
            l["session"],
            "Alt+F9 \u{b7} Ember Pass and Ridgeback Rally by themselves \u{b7} remind every 30 min"
        );
        assert_eq!(l["games"], "Steam on \u{b7} grace period 60 s");
        assert_eq!(l["upload"], "no limit \u{b7} they never wait for a game");
        assert_eq!(l["server"], "clips.example.com \u{b7} paired as sam");
        assert_eq!(
            l["startup"],
            "1.19.0, the newest \u{b7} tray icon in colour"
        );
        assert_eq!(l["status"], "ready \u{b7} the log");
        assert!(!l.contains_key("library") && !l.contains_key("encoder"));
        let one = Seen {
            recording: vec!["Ember Pass".into()],
            ..seen()
        };
        assert_eq!(
            lines(&s, &one)["session"],
            "Alt+F9 \u{b7} Ember Pass by itself \u{b7} remind every 30 min"
        );
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

    #[test]
    fn standalone_has_the_librarys_pages_and_no_uploads() {
        let mut s = settings();
        s.mode = Mode::Standalone;
        let f = Seen {
            paired: false,
            library_dir: "D:\\reel",
            encoder: "NVIDIA NVENC",
            ..seen()
        };
        let l = lines(&s, &f);
        assert!(!l.contains_key("upload"));
        assert_eq!(l["clips"], "Keep the last off \u{b7} 3 sounds");
        assert_eq!(l["server"], "standalone \u{b7} no server");
        assert_eq!(l["library"], "D:\\reel \u{b7} up to 100 GB");
        s.standalone.cap_gb = 0;
        assert_eq!(lines(&s, &f)["library"], "D:\\reel \u{b7} no cap");
        assert_eq!(l["encoder"], "NVIDIA NVENC \u{b7} never waits");
    }
}
