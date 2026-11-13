//! The app's record of the clips it found, sent and deleted: one JSON file, written atomically.
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ClipState {
    Found,
    Queued,
    Uploading,
    Confirmed,
    Failed {
        reason: String,
    },
    Deleted,
    /// Standalone: the file went into the library on this PC and the raw file is gone.
    Local,
}

impl ClipState {
    pub fn word(&self) -> &'static str {
        match self {
            ClipState::Found => "found",
            ClipState::Queued => "queued",
            ClipState::Uploading => "uploading",
            ClipState::Confirmed => "confirmed",
            ClipState::Failed { .. } => "failed",
            ClipState::Deleted => "deleted",
            ClipState::Local => "in the library",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClipRecord {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub found_at: Timestamp,
    pub upload_id: Option<String>,
    pub clip_id: Option<String>,
    #[serde(flatten)]
    pub state: ClipState,
    pub confirmed_at: Option<Timestamp>,
    pub share_url: Option<String>,
    pub game: Option<String>,
    pub title: Option<String>,
    pub recorded_at: Option<Timestamp>,
    /// The server had the file already.
    pub deduped: bool,
    /// Standalone: the clip's id in the library on this PC.
    pub local_clip_id: Option<String>,
    /// The library's own master, uploaded to a server: never a cache candidate.
    pub keep: bool,
    /// A session recording, not a saved replay.
    pub recording: bool,
}

impl Default for ClipRecord {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            name: String::new(),
            size: 0,
            sha256: None,
            found_at: Timestamp::now(),
            upload_id: None,
            clip_id: None,
            state: ClipState::Found,
            confirmed_at: None,
            share_url: None,
            game: None,
            title: None,
            recorded_at: None,
            deduped: false,
            local_clip_id: None,
            keep: false,
            recording: false,
        }
    }
}

impl ClipRecord {
    pub fn new(path: &Path, size: u64) -> Self {
        Self {
            path: path.to_path_buf(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            size,
            ..Default::default()
        }
    }
    pub fn is_confirmed(&self) -> bool {
        matches!(self.state, ClipState::Confirmed)
    }
    /// Whether the file is still this app's to delete: not once deleted or taken by the library.
    pub fn is_local(&self) -> bool {
        !matches!(self.state, ClipState::Deleted | ClipState::Local)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct Ledger {
    pub clips: Vec<ClipRecord>,
    #[serde(skip)]
    file: Option<PathBuf>,
}

impl Ledger {
    pub fn load(file: &Path) -> anyhow::Result<Ledger> {
        let mut l: Ledger = match std::fs::read_to_string(file) {
            Ok(text) => serde_json::from_str(&text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ledger::default(),
            Err(e) => return Err(e.into()),
        };
        l.file = Some(file.to_path_buf());
        Ok(l)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part = crate::settings::part_path(file);
        std::fs::write(&part, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&part, file)?;
        Ok(())
    }

    pub fn get(&self, path: &Path) -> Option<&ClipRecord> {
        self.clips.iter().find(|c| c.path == path)
    }

    pub fn get_mut(&mut self, path: &Path) -> Option<&mut ClipRecord> {
        self.clips.iter_mut().find(|c| c.path == path)
    }

    pub fn by_clip_id(&self, clip_id: &str) -> Option<&ClipRecord> {
        self.clips
            .iter()
            .find(|c| c.clip_id.as_deref() == Some(clip_id))
    }

    /// By its id on the server or in the library on this PC.
    pub fn by_either_id(&self, id: &str) -> Option<&ClipRecord> {
        self.by_clip_id(id).or_else(|| {
            self.clips
                .iter()
                .find(|c| c.local_clip_id.as_deref() == Some(id))
        })
    }

    /// Adds a record, or returns the one already there for that path.
    pub fn found(&mut self, path: &Path, size: u64) -> anyhow::Result<&mut ClipRecord> {
        if self.get(path).is_none() {
            self.clips.push(ClipRecord::new(path, size));
            self.save()?;
        }
        Ok(self.get_mut(path).expect("just added"))
    }

    fn change(&mut self, path: &Path, f: impl FnOnce(&mut ClipRecord)) -> anyhow::Result<bool> {
        let Some(c) = self.get_mut(path) else {
            return Ok(false);
        };
        f(c);
        self.save()?;
        Ok(true)
    }

    pub fn mark_queued(&mut self, path: &Path) -> anyhow::Result<bool> {
        self.change(path, |c| c.state = ClipState::Queued)
    }

    pub fn mark_uploading(&mut self, path: &Path, upload_id: Option<&str>) -> anyhow::Result<bool> {
        self.change(path, |c| {
            c.state = ClipState::Uploading;
            if upload_id.is_some() {
                c.upload_id = upload_id.map(String::from);
            }
        })
    }

    pub fn mark_hashed(&mut self, path: &Path, sha256: &str) -> anyhow::Result<bool> {
        self.change(path, |c| c.sha256 = Some(sha256.to_string()))
    }

    pub fn mark_confirmed(
        &mut self,
        path: &Path,
        clip_id: &str,
        deduped: bool,
    ) -> anyhow::Result<bool> {
        self.change(path, |c| {
            c.state = ClipState::Confirmed;
            c.clip_id = Some(clip_id.to_string());
            c.confirmed_at = Some(Timestamp::now());
            c.deduped = deduped;
        })
    }

    /// Standalone: the local pipeline took the file.
    pub fn mark_local(&mut self, path: &Path, clip_id: &str) -> anyhow::Result<bool> {
        self.change(path, |c| {
            c.state = ClipState::Local;
            c.local_clip_id = Some(clip_id.to_string());
        })
    }

    /// Standalone's "Upload to the server": the record moves to the library's master file,
    /// which the cache rules never delete, and goes back to the queue.
    pub fn relink(&mut self, from: &Path, to: &Path) -> anyhow::Result<bool> {
        let Some(c) = self.clips.iter_mut().find(|c| c.path == from) else {
            return Ok(false);
        };
        c.path = to.to_path_buf();
        c.size = std::fs::metadata(to).map(|m| m.len()).unwrap_or(c.size);
        c.keep = true;
        c.state = ClipState::Found;
        self.save()?;
        Ok(true)
    }

    pub fn last_local(&self) -> Option<&ClipRecord> {
        self.clips
            .iter()
            .filter(|c| c.local_clip_id.is_some())
            .max_by_key(|c| c.found_at)
    }

    pub fn mark_failed(&mut self, path: &Path, reason: &str) -> anyhow::Result<bool> {
        self.change(path, |c| {
            c.state = ClipState::Failed {
                reason: reason.to_string(),
            }
        })
    }

    pub fn mark_deleted(&mut self, path: &Path) -> anyhow::Result<bool> {
        self.change(path, |c| c.state = ClipState::Deleted)
    }

    pub fn set_share_url(&mut self, clip_id: &str, url: &str) -> anyhow::Result<bool> {
        let Some(c) = self
            .clips
            .iter_mut()
            .find(|c| c.clip_id.as_deref() == Some(clip_id))
        else {
            return Ok(false);
        };
        c.share_url = Some(url.to_string());
        self.save()?;
        Ok(true)
    }

    pub fn set_meta(
        &mut self,
        path: &Path,
        game: Option<String>,
        title: Option<String>,
        recorded_at: Option<Timestamp>,
    ) -> anyhow::Result<bool> {
        self.change(path, |c| {
            c.game = game;
            c.title = title;
            c.recorded_at = recorded_at;
        })
    }

    pub fn set_recording(&mut self, path: &Path) -> anyhow::Result<bool> {
        self.change(path, |c| c.recording = true)
    }

    /// The last saved replay the server confirmed; a session recording is not "the last clip".
    pub fn last_confirmed(&self) -> Option<&ClipRecord> {
        self.clips
            .iter()
            .filter(|c| {
                c.clip_id.is_some()
                    && !c.recording
                    && matches!(c.state, ClipState::Confirmed | ClipState::Deleted)
            })
            .max_by_key(|c| (c.confirmed_at, c.found_at))
    }

    pub fn recent(&self, n: usize) -> Vec<&ClipRecord> {
        let mut v: Vec<&ClipRecord> = self.clips.iter().collect();
        v.sort_by_key(|c| std::cmp::Reverse(c.found_at));
        v.truncate(n);
        v
    }

    pub fn pending(&self) -> Vec<&ClipRecord> {
        self.clips
            .iter()
            .filter(|c| {
                matches!(
                    c.state,
                    ClipState::Found | ClipState::Queued | ClipState::Uploading
                )
            })
            .collect()
    }

    pub fn cache_entries(&self) -> Vec<crate::cache::CacheEntry> {
        self.clips
            .iter()
            .filter(|c| c.is_local() && !c.keep)
            .map(|c| crate::cache::CacheEntry {
                path: c.path.clone(),
                bytes: c.size,
                confirmed_at: c.confirmed_at,
                uploaded: c.is_confirmed(),
            })
            .collect()
    }

    /// The clips named after `old` take `new`, and the titles reel made them with it; a title
    /// changed by hand stays. Each one's id in the library on this PC and its moment, for the
    /// library's titles. Not saved: the caller saves.
    pub fn rename_game(
        &mut self,
        old: &str,
        new: &str,
    ) -> Vec<(Option<String>, Option<Timestamp>)> {
        let mut out = Vec::new();
        for c in self
            .clips
            .iter_mut()
            .filter(|c| c.game.as_deref() == Some(old))
        {
            c.game = Some(new.to_string());
            if let (Some(t), Some(at)) = (&c.title, c.recorded_at) {
                if let Some(again) = crate::programs::retitle(t, old, new, at) {
                    c.title = Some(again);
                }
            }
            out.push((c.local_clip_id.clone(), c.recorded_at));
        }
        out
    }

    /// Forgets records whose files are gone, but keeps the ones the clip list shows.
    pub fn forget_missing(&mut self) -> anyhow::Result<usize> {
        let before = self.clips.len();
        self.clips.retain(|c| {
            c.path.exists()
                || matches!(
                    c.state,
                    ClipState::Confirmed | ClipState::Deleted | ClipState::Local
                )
        });
        let gone = before - self.clips.len();
        if gone > 0 {
            self.save()?;
        }
        Ok(gone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_move_through_their_states_and_survive_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ledger.json");
        let mut l = Ledger::load(&file).unwrap();
        let a = dir.path().join("a.mkv");
        let b = dir.path().join("b.mkv");
        l.found(&a, 10).unwrap();
        l.found(&a, 10).unwrap();
        assert_eq!(l.clips.len(), 1, "found twice is one record");
        l.found(&b, 20).unwrap();
        l.mark_queued(&a).unwrap();
        l.mark_uploading(&a, Some("u1")).unwrap();
        l.mark_hashed(&a, "ab").unwrap();
        l.mark_confirmed(&a, "c1", false).unwrap();
        l.mark_failed(&b, "no").unwrap();
        l.set_share_url("c1", "https://x/s/t").unwrap();
        assert!(!l.mark_queued(Path::new("/nope")).unwrap());
        let back = Ledger::load(&file).unwrap();
        assert_eq!(back.clips, l.clips);
        let a_rec = back.get(&a).unwrap();
        assert_eq!(a_rec.state, ClipState::Confirmed);
        assert_eq!(a_rec.upload_id.as_deref(), Some("u1"));
        assert_eq!(a_rec.share_url.as_deref(), Some("https://x/s/t"));
        assert_eq!(back.last_confirmed().unwrap().path, a);
        assert_eq!(back.get(&b).unwrap().state.word(), "failed");
        assert_eq!(back.recent(1).len(), 1);
        assert_eq!(back.pending().len(), 0);
        assert_eq!(back.cache_entries().len(), 2);
        assert!(back
            .cache_entries()
            .iter()
            .any(|e| e.uploaded && e.confirmed_at.is_some()));
    }

    #[test]
    fn a_session_recording_is_kept_as_one_and_is_not_the_last_clip_to_share() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ledger.json");
        let mut l = Ledger::load(&file).unwrap();
        let replay = dir.path().join("Replay 2026-11-11 21-00-00.mkv");
        let session = dir.path().join("2026-11-11 20-00-00.mkv");
        l.found(&replay, 1).unwrap();
        l.mark_confirmed(&replay, "REPLAY", false).unwrap();
        l.found(&session, 11_000_000_000).unwrap();
        assert!(l.set_recording(&session).unwrap());
        l.mark_confirmed(&session, "SESSION", false).unwrap();
        let back = Ledger::load(&file).unwrap();
        assert!(back.get(&session).unwrap().recording);
        assert!(!back.get(&replay).unwrap().recording);
        assert_eq!(
            back.last_confirmed().and_then(|c| c.clip_id.as_deref()),
            Some("REPLAY"),
            "confirmed after it, but a recording"
        );
        let older: Ledger = serde_json::from_str(r#"{"clips":[{"path":"/c/a.mkv","name":"a.mkv","size":1,"found_at":"2026-11-01T10:00:00Z","state":"found"}]}"#).unwrap();
        assert!(!older.clips[0].recording, "a ledger from before");
    }

    #[test]
    fn a_clip_is_found_by_its_id_on_the_server_or_on_this_pc() {
        let dir = tempfile::tempdir().unwrap();
        let mut l = Ledger::load(&dir.path().join("ledger.json")).unwrap();
        let sent = dir.path().join("sent.mkv");
        let local = dir.path().join("local.mkv");
        l.found(&sent, 1).unwrap();
        l.found(&local, 1).unwrap();
        l.mark_confirmed(&sent, "SERVER", false).unwrap();
        l.mark_local(&local, "LOCAL").unwrap();
        assert_eq!(l.by_either_id("SERVER").unwrap().path, sent);
        assert_eq!(l.by_either_id("LOCAL").unwrap().path, local);
        assert!(l.by_either_id("OTHER").is_none());
    }

    #[test]
    fn missing_files_are_forgotten_unless_they_were_sent() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ledger.json");
        let mut l = Ledger::load(&file).unwrap();
        let gone = dir.path().join("gone.mkv");
        let sent = dir.path().join("sent.mkv");
        let here = dir.path().join("here.mkv");
        std::fs::write(&here, b"x").unwrap();
        l.found(&gone, 1).unwrap();
        l.found(&sent, 1).unwrap();
        l.found(&here, 1).unwrap();
        l.mark_confirmed(&sent, "c", true).unwrap();
        l.mark_deleted(&sent).unwrap();
        assert_eq!(l.forget_missing().unwrap(), 1);
        assert!(l.get(&gone).is_none());
        assert!(l.get(&sent).is_some());
        assert!(
            l.last_confirmed().is_some(),
            "a deleted local copy still counts as the last clip"
        );
    }

    #[test]
    fn a_local_clip_leaves_the_cache_rules_and_is_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let mut l = Ledger::load(&dir.path().join("ledger.json")).unwrap();
        let raw = dir.path().join("Replay.mkv");
        std::fs::write(&raw, b"x").unwrap();
        l.found(&raw, 1).unwrap();
        assert!(l.mark_local(&raw, "clip1").unwrap());
        let rec = l.get(&raw).unwrap();
        assert_eq!(rec.state, ClipState::Local);
        assert_eq!(rec.local_clip_id.as_deref(), Some("clip1"));
        assert!(!rec.is_local(), "the raw file is the library's now");
        assert!(l.cache_entries().is_empty(), "nothing for the cache sweep");
        assert_eq!(l.last_local().map(|c| c.name.as_str()), Some("Replay.mkv"));
        std::fs::remove_file(&raw).unwrap();
        assert_eq!(
            l.forget_missing().unwrap(),
            0,
            "a local clip is kept for the list"
        );
        let master = dir.path().join("master.mp4");
        std::fs::write(&master, b"y").unwrap();
        l.found(&master, 1).unwrap();
        l.get_mut(&master).unwrap().keep = true;
        l.mark_confirmed(&master, "srv1", false).unwrap();
        assert!(
            l.cache_entries().is_empty(),
            "a kept file is never a cache candidate"
        );
    }

    #[test]
    fn a_rename_takes_the_clips_of_the_old_name_and_leaves_a_title_changed_by_hand() {
        let dir = tempfile::tempdir().unwrap();
        let mut l = Ledger::load(&dir.path().join("ledger.json")).unwrap();
        let at: Timestamp = "2026-11-09T21:14:00Z".parse().unwrap();
        let old = "De Libvm C Str Onkl Sto";
        for (i, (game, title, local)) in [
            (
                Some(old),
                Some(crate::gaming::clip_title(Some(old), at)),
                Some("lib1"),
            ),
            (Some(old), Some("My best round".to_string()), None),
            (
                Some("Ember Pass"),
                Some(crate::gaming::clip_title(Some("Ember Pass"), at)),
                None,
            ),
            (Some(old), None, None),
        ]
        .into_iter()
        .enumerate()
        {
            let p = dir.path().join(format!("{i}.mkv"));
            l.found(&p, 1).unwrap();
            let c = l.get_mut(&p).unwrap();
            c.game = game.map(str::to_string);
            c.title = title;
            c.recorded_at = Some(at);
            c.local_clip_id = local.map(str::to_string);
        }
        let moved = l.rename_game(old, "libvmcst");
        assert_eq!(moved.len(), 3, "the three named after it");
        assert_eq!(moved[0], (Some("lib1".to_string()), Some(at)));
        let title = |i: usize| {
            l.get(&dir.path().join(format!("{i}.mkv")))
                .unwrap()
                .title
                .clone()
        };
        let game = |i: usize| {
            l.get(&dir.path().join(format!("{i}.mkv")))
                .unwrap()
                .game
                .clone()
        };
        assert_eq!(
            title(0),
            Some(crate::gaming::clip_title(Some("libvmcst"), at))
        );
        assert_eq!(
            title(1).as_deref(),
            Some("My best round"),
            "changed by hand: it stays"
        );
        assert_eq!(
            game(1).as_deref(),
            Some("libvmcst"),
            "its game changes either way"
        );
        assert_eq!(
            game(2).as_deref(),
            Some("Ember Pass"),
            "another game's clip"
        );
        assert_eq!((game(3).as_deref(), title(3)), (Some("libvmcst"), None));
    }
}
