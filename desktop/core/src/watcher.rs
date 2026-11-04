//! A clip counts once its size has held still and it opens for reading (OBS keeps a lock
//! while it writes).
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub const POLL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub size: u64,
}

/// `backlog`: it was there before the watcher started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub size: u64,
    pub backlog: bool,
}

pub fn skip_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    name.starts_with('.')
        || lower.ends_with(".part")
        || lower.ends_with(".tmp")
        || lower.ends_with(".crdownload")
}

pub fn has_extension(path: &Path, extensions: &[String]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

/// The clip files in the folder, not into subfolders.
pub fn scan(dir: &Path, extensions: &[String]) -> std::io::Result<Vec<Candidate>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if skip_name(name) || !has_extension(&path, extensions) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        out.push(Candidate {
            path,
            size: meta.len(),
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    Wait,
    Ready,
}

/// Ready once the size has not changed for the stable time; a change restarts the clock.
#[derive(Debug, Clone)]
pub struct Stability {
    stable: Duration,
    last_size: Option<u64>,
    since: Option<Instant>,
}

impl Stability {
    pub fn new(stable: Duration) -> Stability {
        Stability {
            stable,
            last_size: None,
            since: None,
        }
    }

    pub fn observe(&mut self, size: u64, now: Instant) -> Readiness {
        match (self.last_size, self.since) {
            (Some(s), Some(since)) if s == size => {
                if now.duration_since(since) >= self.stable {
                    Readiness::Ready
                } else {
                    Readiness::Wait
                }
            }
            _ => {
                self.last_size = Some(size);
                self.since = Some(now);
                if self.stable.is_zero() {
                    Readiness::Ready
                } else {
                    Readiness::Wait
                }
            }
        }
    }
}

/// Whether the file can be read now: on Windows an exclusive open, which fails while
/// OBS still has the file; elsewhere a plain open.
pub fn can_open(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(path)
            .is_ok()
    }
    #[cfg(not(windows))]
    {
        std::fs::File::open(path).is_ok()
    }
}

pub struct WatchConfig {
    pub dir: PathBuf,
    pub extensions: Vec<String>,
    pub stable: Duration,
    pub take_backlog: bool,
}

/// Reports each clip once until cancelled; without `take_backlog` the files there at the start
/// are skipped.
pub async fn run(cfg: WatchConfig, tx: mpsc::Sender<Found>, cancel: CancellationToken) {
    let (wake_tx, mut wake_rx) = mpsc::unbounded_channel::<()>();
    let mut watcher: Option<notify::RecommendedWatcher> = None;
    let mut reported: HashSet<PathBuf> = HashSet::new();
    let mut pending: std::collections::HashMap<PathBuf, Stability> =
        std::collections::HashMap::new();
    let mut first = true;
    loop {
        if cancel.is_cancelled() {
            break;
        }
        if !cfg.dir.is_dir() {
            if let Err(e) = std::fs::create_dir_all(&cfg.dir) {
                tracing::warn!(dir = %cfg.dir.display(), error = %e, "the clips folder is missing");
            }
        }
        if watcher.is_none() && cfg.dir.is_dir() {
            watcher = make_watcher(&cfg.dir, wake_tx.clone());
        }
        let now = Instant::now();
        match scan(&cfg.dir, &cfg.extensions) {
            Ok(list) => {
                let present: HashSet<PathBuf> = list.iter().map(|c| c.path.clone()).collect();
                pending.retain(|p, _| present.contains(p));
                for c in list {
                    if reported.contains(&c.path) {
                        continue;
                    }
                    if first && !cfg.take_backlog {
                        reported.insert(c.path.clone());
                        continue;
                    }
                    let st = pending
                        .entry(c.path.clone())
                        .or_insert_with(|| Stability::new(cfg.stable));
                    if st.observe(c.size, now) == Readiness::Ready && can_open(&c.path) {
                        pending.remove(&c.path);
                        reported.insert(c.path.clone());
                        tracing::info!(file = %c.path.display(), size = c.size, backlog = first, "clip found");
                        if tx
                            .send(Found {
                                path: c.path,
                                size: c.size,
                                backlog: first,
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                reported.retain(|p| present.contains(p));
            }
            Err(e) => {
                tracing::warn!(dir = %cfg.dir.display(), error = %e, "cannot read the clips folder")
            }
        }
        first = false;
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(POLL) => {},
            _ = wake_rx.recv() => {
                tokio::time::sleep(Duration::from_millis(200)).await;
                while wake_rx.try_recv().is_ok() {}
            }
        }
    }
}

fn make_watcher(dir: &Path, wake: mpsc::UnboundedSender<()>) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher;
    let mut w = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        if res.is_ok() {
            let _ = wake.send(());
        }
    })
    .ok()?;
    match w.watch(dir, notify::RecursiveMode::NonRecursive) {
        Ok(()) => Some(w),
        Err(e) => {
            tracing::warn!(dir = %dir.display(), error = %e, "no file system events; polling only");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exts() -> Vec<String> {
        vec!["mkv".into(), "mp4".into()]
    }

    #[test]
    fn scan_skips_parts_hidden_files_other_extensions_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        for n in [
            "a.mkv",
            "b.MP4",
            "c.part",
            "c.mkv.part",
            ".d.mkv",
            "e.txt",
            "f.tmp",
        ] {
            std::fs::write(dir.path().join(n), b"x").unwrap();
        }
        std::fs::create_dir(dir.path().join("sub.mkv")).unwrap();
        let names: Vec<String> = scan(dir.path(), &exts())
            .unwrap()
            .iter()
            .map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a.mkv", "b.MP4"]);
        assert!(skip_name("x.PART"));
        assert!(!skip_name("x.mkv"));
    }

    #[test]
    fn the_size_stable_rule() {
        let t0 = Instant::now();
        let mut s = Stability::new(Duration::from_secs(5));
        assert_eq!(s.observe(100, t0), Readiness::Wait);
        assert_eq!(s.observe(100, t0 + Duration::from_secs(3)), Readiness::Wait);
        assert_eq!(
            s.observe(200, t0 + Duration::from_secs(4)),
            Readiness::Wait,
            "a change restarts"
        );
        assert_eq!(s.observe(200, t0 + Duration::from_secs(8)), Readiness::Wait);
        assert_eq!(
            s.observe(200, t0 + Duration::from_secs(9)),
            Readiness::Ready
        );
        let mut zero = Stability::new(Duration::ZERO);
        assert_eq!(zero.observe(1, t0), Readiness::Ready);
    }

    #[tokio::test]
    async fn a_growing_file_waits_a_still_one_is_reported_once_and_the_backlog_is_silent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.mkv"), b"old").unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let cfg = WatchConfig {
            dir: dir.path().to_path_buf(),
            extensions: exts(),
            stable: Duration::from_millis(300),
            take_backlog: false,
        };
        let c = cancel.clone();
        let task = tokio::spawn(async move { run(cfg, tx, c).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let growing = dir.path().join("new.mkv");
        std::fs::write(&growing, b"part one").unwrap();
        std::fs::write(dir.path().join("skip.part"), b"nope").unwrap();
        tokio::time::sleep(Duration::from_millis(250)).await;
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&growing)
                .unwrap();
            f.write_all(b" and two").unwrap();
        }
        assert!(
            rx.try_recv().is_err(),
            "nothing yet: the old file is backlog, the new one grows"
        );
        let found = tokio::time::timeout(Duration::from_secs(6), rx.recv())
            .await
            .expect("found in time")
            .unwrap();
        assert_eq!(found.path, growing);
        assert_eq!(found.size, "part one and two".len() as u64);
        assert!(!found.backlog);
        tokio::time::sleep(Duration::from_millis(2500)).await;
        assert!(rx.try_recv().is_err(), "reported once");
        cancel.cancel();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn the_backlog_is_reported_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.mkv"), b"old").unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let cfg = WatchConfig {
            dir: dir.path().to_path_buf(),
            extensions: exts(),
            stable: Duration::ZERO,
            take_backlog: true,
        };
        let c = cancel.clone();
        let task = tokio::spawn(async move { run(cfg, tx, c).await });
        let found = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(found.backlog);
        assert!(found.path.ends_with("old.mkv"));
        cancel.cancel();
        task.await.unwrap();
    }
}
