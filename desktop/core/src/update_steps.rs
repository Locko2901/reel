//! The update as the user follows it: the words before it, the notification as it goes, and
//! the note for the reel that starts after it.
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The note, in the app's config folder. The installer reads `"quit":true` in it, to leave reel
/// closed after an update installed as reel quit, and writes `FAILED` beside it when it stops.
pub const NOTE: &str = "updating.json";
pub const FAILED: &str = "update-failed.txt";

/// A note older than this is from an update that never reached a start of reel: dropped.
const STALE_SECS: u64 = 24 * 3600;

/// How the update is put in place on this PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The Windows installer, with OBS's thousands of files or without them.
    Windows { with_obs: bool },
    /// The new AppImage written over the one reel runs from.
    AppImage,
}

impl Kind {
    /// This PC's: the Windows installer, with or without OBS, or the AppImage.
    pub fn here(with_obs: bool) -> Kind {
        if cfg!(windows) {
            Kind::Windows { with_obs }
        } else {
            Kind::AppImage
        }
    }
}

/// Where the window was when the update started.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    /// `library`, `settings`, `setup` or `offline`, as `shell::View` names them.
    pub view: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// A page of the server or of reel on this PC: its path and query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub from: String,
    pub to: String,
    /// Installed as reel quit: reel stays closed afterwards.
    #[serde(default)]
    pub quit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<Place>,
    /// A short line of what is new, when the release notes have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub news: Option<String>,
    /// When the update started, in seconds since 1970.
    #[serde(default)]
    pub at: u64,
}

impl Note {
    /// On one line, as the installer reads one; an older failure goes.
    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let _ = std::fs::remove_file(dir.join(FAILED));
        let line = serde_json::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(dir.join(NOTE), line)
    }

    /// The note of the update before this start, read once: it is removed as it is read.
    pub fn take(dir: &Path, now: u64) -> Option<Note> {
        let path = dir.join(NOTE);
        let text = std::fs::read_to_string(&path).ok();
        let _ = std::fs::remove_file(&path);
        let note: Note = serde_json::from_str(text?.trim()).ok()?;
        (now.saturating_sub(note.at) <= STALE_SECS).then_some(note)
    }
}

/// What stopped the installer, as it wrote it (UTF-16 with its byte order mark, or UTF-8),
/// read once.
pub fn take_failure(dir: &Path) -> Option<String> {
    let path = dir.join(FAILED);
    let bytes = std::fs::read(&path).ok();
    let _ = std::fs::remove_file(&path);
    let bytes = bytes?;
    let text = match bytes.strip_prefix(&[0xff, 0xfe]) {
        Some(wide) => {
            let units: Vec<u16> = wide
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            String::from_utf16_lossy(&units)
        }
        None => String::from_utf8_lossy(&bytes).into_owned(),
    };
    let text = text.trim().trim_end_matches('.').trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// About how long the update takes from the press to reel open again: the package with OBS
/// copies OBS's files when the update brings another OBS.
pub fn how_long(kind: Kind) -> &'static str {
    match kind {
        Kind::Windows { with_obs: true } => "about a minute",
        Kind::Windows { with_obs: false } => "about half a minute",
        Kind::AppImage => "a few seconds",
    }
}

/// What happens when the update is installed, said where it is offered (This app, Startup).
pub fn before(version: &str, kind: Kind, quit: bool) -> String {
    let long = how_long(kind);
    match (kind, quit) {
        (Kind::AppImage, false) => format!(
            "Install closes reel, puts reel {version} in place of this AppImage and opens it again by itself, in {long}."
        ),
        (Kind::AppImage, true) => format!(
            "reel {version} takes the place of this AppImage when you quit reel, and opens when you next start it."
        ),
        (Kind::Windows { .. }, false) => format!(
            "Install closes reel, its installer shows its progress, and reel {version} opens again by itself, in {long}."
        ),
        (Kind::Windows { .. }, true) => format!(
            "reel {version} installs when you quit reel: its installer shows its progress, then reel stays closed until you open it."
        ),
    }
}

/// The notification as the update starts, title and body: one line, the same steps.
pub fn going(version: &str, kind: Kind, quit: bool) -> (String, String) {
    let long = how_long(kind);
    match (kind, quit) {
        (_, true) => (
            format!("Installing reel {version}"),
            match kind {
                Kind::AppImage => "reel puts it in place as it quits, and stays closed until you open it.".to_string(),
                Kind::Windows { .. } => "Its installer shows its progress, and reel stays closed until you open it.".to_string(),
            },
        ),
        (Kind::AppImage, false) => (
            format!("Updating reel to {version}"),
            format!("reel closes now and opens again by itself in {long}."),
        ),
        (Kind::Windows { .. }, false) => (
            format!("Updating reel to {version}"),
            format!("reel closes now. Its installer shows its progress, and reel opens again by itself in {long}."),
        ),
    }
}

/// What the reel that starts after an update says, title and body: that it is updated, with
/// what is new, or that the update did not install and why. Nothing on an ordinary start.
pub fn after(
    note: Option<&Note>,
    now: &str,
    was: &str,
    failure: Option<&str>,
) -> Option<(String, String)> {
    if !was.is_empty() && was != now {
        let body = match note.and_then(|n| n.news.as_deref()) {
            Some(news) => format!("New: {news}"),
            None => format!("From {was}."),
        };
        return Some((format!("reel is updated to {now}"), body));
    }
    let note = note.filter(|n| n.to != now)?;
    let why = failure.map_or_else(
        || "Its installer stopped before the end".to_string(),
        str::to_string,
    );
    Some((
        format!("reel {} did not install", note.to),
        format!("{why}. reel {now} runs as before; Install under This app, Startup tries again."),
    ))
}

/// The short line of what is new: the newest release's first entry in the notes the server
/// hands over (release-please's Markdown, the newest first), as plain words, when it is short.
pub fn news_line(notes: &str) -> Option<String> {
    let entry = notes
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("* ").or_else(|| l.strip_prefix("- ")))?;
    let mut text = entry.to_string();
    if text.starts_with("**") {
        if let Some(end) = text[2..].find(":**") {
            text = text[2 + end + 3..].trim_start().to_string();
        }
    }
    while let Some(open) = text.rfind(" ([") {
        if !text.ends_with(')') {
            break;
        }
        text.truncate(open);
    }
    let text = text.trim().trim_end_matches('.').to_string();
    (!text.is_empty() && text.chars().count() <= 100).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: Kind = Kind::Windows { with_obs: false };
    const WITH_OBS: Kind = Kind::Windows { with_obs: true };

    #[test]
    fn before_an_update_the_app_says_reel_closes_the_installer_shows_and_reel_opens_again() {
        assert_eq!(
            before("1.10.0", WIN, false),
            "Install closes reel, its installer shows its progress, and reel 1.10.0 opens again by itself, in about half a minute."
        );
        assert!(before("1.10.0", WITH_OBS, false).ends_with("in about a minute."));
        assert_eq!(
            before("1.10.0", WIN, true),
            "reel 1.10.0 installs when you quit reel: its installer shows its progress, then reel stays closed until you open it."
        );
        assert!(before("1.10.0", Kind::AppImage, false).contains("this AppImage"));
        assert!(before("1.10.0", Kind::AppImage, false).ends_with("in a few seconds."));
    }

    #[test]
    fn the_notification_says_the_same_steps_in_one_line() {
        assert_eq!(
            going("1.10.0", WITH_OBS, false),
            (
                "Updating reel to 1.10.0".to_string(),
                "reel closes now. Its installer shows its progress, and reel opens again by itself in about a minute.".to_string()
            )
        );
        let (title, body) = going("1.10.0", WIN, true);
        assert_eq!(title, "Installing reel 1.10.0");
        assert!(body.contains("stays closed"), "{body}");
        assert!(!body.contains('\n'));
        let (_, body) = going("1.10.0", Kind::AppImage, false);
        assert_eq!(
            body,
            "reel closes now and opens again by itself in a few seconds."
        );
    }

    #[test]
    fn after_an_update_reel_says_it_is_updated_with_what_is_new() {
        let note = Note {
            from: "1.9.1".into(),
            to: "1.10.0".into(),
            news: Some("the installers in a light window".into()),
            ..Default::default()
        };
        assert_eq!(
            after(Some(&note), "1.10.0", "1.9.1", None),
            Some((
                "reel is updated to 1.10.0".to_string(),
                "New: the installers in a light window".to_string()
            ))
        );
        // an app of 1.9 leaves no note: the version alone says it
        assert_eq!(
            after(None, "1.10.0", "1.9.1", None).unwrap().1,
            "From 1.9.1."
        );
        assert_eq!(after(None, "1.10.0", "1.10.0", None), None);
        assert_eq!(
            after(None, "1.10.0", "", None),
            None,
            "a first start says nothing"
        );
    }

    #[test]
    fn a_failed_install_starts_the_reel_before_which_says_why() {
        let note = Note {
            from: "1.9.1".into(),
            to: "1.10.0".into(),
            ..Default::default()
        };
        assert_eq!(
            after(
                Some(&note),
                "1.9.1",
                "1.9.1",
                Some("reel could not stop what runs from its folder")
            ),
            Some((
                "reel 1.10.0 did not install".to_string(),
                "reel could not stop what runs from its folder. reel 1.9.1 runs as before; Install under This app, Startup tries again.".to_string()
            ))
        );
        assert!(after(Some(&note), "1.9.1", "1.9.1", None)
            .unwrap()
            .1
            .starts_with("Its installer stopped before the end."));
    }

    #[test]
    fn the_note_is_one_line_the_installer_reads_and_is_read_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FAILED), "from before").unwrap();
        let note = Note {
            from: "1.9.1".into(),
            to: "1.10.0".into(),
            quit: true,
            window: Some(Place {
                view: "settings".into(),
                section: Some("startup".into()),
                path: None,
            }),
            news: None,
            at: 1_000,
        };
        note.write(dir.path()).unwrap();
        let text = std::fs::read_to_string(dir.path().join(NOTE)).unwrap();
        assert!(!text.contains('\n'));
        assert!(text.contains(r#""quit":true"#), "{text}");
        assert!(
            !dir.path().join(FAILED).exists(),
            "a failure of an update before goes"
        );
        assert_eq!(Note::take(dir.path(), 1_060), Some(note.clone()));
        assert_eq!(Note::take(dir.path(), 1_060), None, "read once");
        note.write(dir.path()).unwrap();
        assert_eq!(
            Note::take(dir.path(), 1_000 + 2 * STALE_SECS),
            None,
            "stale"
        );
        assert!(!dir.path().join(NOTE).exists());
    }

    #[test]
    fn the_installers_words_are_read_in_utf16_or_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let mut wide = vec![0xff, 0xfe];
        for u in "Qt6Core.dll is in use, by obs64.exe.\r\n".encode_utf16() {
            wide.extend_from_slice(&u.to_le_bytes());
        }
        std::fs::write(dir.path().join(FAILED), wide).unwrap();
        assert_eq!(
            take_failure(dir.path()).as_deref(),
            Some("Qt6Core.dll is in use, by obs64.exe")
        );
        assert_eq!(take_failure(dir.path()), None);
        std::fs::write(dir.path().join(FAILED), "  \n").unwrap();
        assert_eq!(take_failure(dir.path()), None);
    }

    #[test]
    fn what_is_new_is_the_newest_releases_first_entry_in_plain_words() {
        let notes = "reel 1.10.0\n\n### Features\n\n* **desktop:** the installers in a light window ([#42](https://github.com/x/y/issues/42)) ([abc1234](https://github.com/x/y/commit/abc1234))\n* an update you can see\n\nreel 1.9.1\n\n### Bug Fixes\n\n* something older";
        assert_eq!(
            news_line(notes).as_deref(),
            Some("the installers in a light window")
        );
        assert_eq!(news_line("reel 1.10.0"), None);
        let long = format!("reel 2.0.0\n\n* {}", "a very long line ".repeat(10));
        assert_eq!(news_line(&long), None);
    }
}
