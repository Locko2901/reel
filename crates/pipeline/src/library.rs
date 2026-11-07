//! Whether the Library folder is this reel's library, marked by a `.reel-library` file; without
//! it, while the database holds clips, nothing is written there until it is back.
use crate::Ctx;
use reel_core::config::{MARKER, TRASH};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const INSTANCE_KEY: &str = "instance_id";
/// The database's file in the data folder, and in a copy of the library.
pub const DB: &str = "reel.db";
const EVERY: Duration = Duration::from_secs(10);
const ANSWER_WITHIN: Duration = Duration::from_secs(10);
const FIRST_LINE: &str = "reel's library: clips are written here only while this file is here.";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Unknown,
    Ready,
    /// Why not, in words for the pages and the log.
    Away(String),
}

#[derive(Default)]
pub struct State {
    status: Mutex<Status>,
    /// One check at a time, so two never meet at `reading`. A check asked meanwhile waits and
    /// then looks itself: the one under way may have read the folder before it went away.
    checking: tokio::sync::Mutex<()>,
    /// A read of the folder still out: a share that hangs keeps its thread past the check's
    /// time limit, and no other read starts until it is back (else one thread more every 10 s).
    reading: Arc<AtomicBool>,
    old_trash_done: AtomicBool,
    /// Held while a clip's folder moves to the trash, back, or out of the data folder's trash.
    pub moves: tokio::sync::Mutex<()>,
}

impl State {
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
    pub fn ready(&self) -> bool {
        self.status() == Status::Ready
    }
    pub fn away(&self) -> Option<String> {
        match self.status() {
            Status::Away(why) => Some(why),
            _ => None,
        }
    }
}

/// A write to the library refused while it is away.
#[derive(Debug)]
pub struct Away(pub String);

impl std::fmt::Display for Away {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the library folder is not usable: {}", self.0)
    }
}

impl std::error::Error for Away {}

/// What the folder holds without a marker: whether it is there, empty, and has a clip's folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bare {
    pub exists: bool,
    pub empty: bool,
    pub clips: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Ready,
    /// The folder is the library: the marker goes in.
    Mark,
    /// Nothing to lose: the folder is made if need be, and marked.
    Make,
    Away(String),
}

/// The rule, apart from the disk: the marker found (its id, empty when made by hand) or what the
/// bare folder holds, and whether the database holds clips that are not in the trash.
pub fn decide(dir: &Path, marker: Option<&str>, bare: Bare, holds: bool, me: &str) -> Decision {
    let shown = dir.display();
    match marker {
        Some(id) if id == me => Decision::Ready,
        Some("") => Decision::Mark,
        Some(other) => Decision::Away(format!(
            "{shown} is another reel's library (its .reel-library names {other}); if it is this one's, delete that file"
        )),
        None if !holds => Decision::Make,
        None if bare.clips => Decision::Mark,
        None if !bare.exists => Decision::Away(format!("{shown} is not there")),
        None if bare.empty => Decision::Away(format!(
            "{shown} is empty, as a disk or a share that is not mounted leaves it"
        )),
        None => Decision::Away(format!(
            "{shown} has no .reel-library file and none of the clips' folders"
        )),
    }
}

pub fn marker_id(text: &str) -> &str {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("instance "))
        .map(str::trim)
        .unwrap_or("")
}

pub fn marker_text(id: &str) -> String {
    format!("{FIRST_LINE}\ninstance {id}\n")
}

fn read_marker(dir: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(dir.join(MARKER)) {
        Ok(text) => Ok(Some(marker_id(&text).to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn look(dir: &Path, clips: &[(String, bool)]) -> Bare {
    let Ok(mut rd) = std::fs::read_dir(dir) else {
        return Bare {
            exists: dir.exists(),
            empty: true,
            clips: false,
        };
    };
    let empty = rd.next().is_none();
    let clips = clips.iter().any(|(id, trashed)| {
        let at = if *trashed {
            dir.join(TRASH).join(id)
        } else {
            dir.join(id)
        };
        at.is_dir()
    });
    Bare {
        exists: true,
        empty,
        clips,
    }
}

fn write_marker(dir: &Path, id: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(MARKER), marker_text(id))
}

/// This reel's id, made once and kept in the database, so it moves with the database.
pub async fn instance_id(ctx: &Ctx) -> anyhow::Result<String> {
    if let Some(id) = ctx.store.setting(INSTANCE_KEY).await? {
        return Ok(id);
    }
    let id = reel_core::ids::new_id();
    ctx.store.set_setting(INSTANCE_KEY, &id).await?;
    Ok(id)
}

async fn blocking<T: Send + 'static>(
    reading: &Arc<AtomicBool>,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    blocking_within(reading, ANSWER_WITHIN, f).await
}

/// A read of the folder off the async threads, bounded in time, one at a time: the read itself
/// clears the flag, so a hung share holds one thread however long it hangs.
async fn blocking_within<T: Send + 'static>(
    reading: &Arc<AtomicBool>,
    within: Duration,
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    if reading.swap(true, Ordering::SeqCst) {
        return Err("an earlier read of it has not come back yet".into());
    }
    let flag = reading.clone();
    let task = tokio::task::spawn_blocking(move || {
        let v = f();
        flag.store(false, Ordering::SeqCst);
        v
    });
    match tokio::time::timeout(within, task).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => {
            // the read panicked before it could clear the flag
            reading.store(false, Ordering::SeqCst);
            Err(e.to_string())
        }
        Err(_) => Err(format!("did not answer within {} s", within.as_secs())),
    }
}

async fn inspect(ctx: &Ctx, dir: PathBuf) -> Result<Status, String> {
    let me = instance_id(ctx).await.map_err(|e| e.to_string())?;
    let d = dir.clone();
    let marker = blocking(&ctx.library.reading, move || read_marker(&d))
        .await
        .map_err(|e| format!("{} {e}", dir.display()))?
        .map_err(|e| format!("{} cannot be read: {e}", dir.display()))?;
    let (holds, bare) = match marker {
        Some(_) => (
            true,
            Bare {
                exists: true,
                empty: false,
                clips: false,
            },
        ),
        None => {
            let clips = ctx
                .store
                .clips_with_files(200)
                .await
                .map_err(|e| e.to_string())?;
            let holds = clips.iter().any(|(_, trashed)| !trashed);
            let d = dir.clone();
            let bare = blocking(&ctx.library.reading, move || look(&d, &clips))
                .await
                .map_err(|e| format!("{} {e}", dir.display()))?;
            (holds, bare)
        }
    };
    let decision = decide(&dir, marker.as_deref(), bare, holds, &me);
    if matches!(decision, Decision::Mark | Decision::Make) {
        let (d, id) = (dir.clone(), me.clone());
        blocking(&ctx.library.reading, move || write_marker(&d, &id))
            .await
            .map_err(|e| format!("{} {e}", dir.display()))?
            .map_err(|e| format!("{} cannot take its .reel-library file: {e}", dir.display()))?;
        tracing::info!(dir = %dir.display(), new = decision == Decision::Make, "the library folder is marked as this reel's");
    }
    Ok(match decision {
        Decision::Away(why) => Status::Away(why),
        _ => Status::Ready,
    })
}

/// Looks now, keeps what it found, and logs a change.
pub async fn check(ctx: &Ctx) -> Status {
    let _one = ctx.library.checking.lock().await;
    let dir = ctx.config().library_dir();
    let status = match inspect(ctx, dir.clone()).await {
        Ok(s) => s,
        Err(why) => Status::Away(why),
    };
    let mut held = ctx.library.status.lock().unwrap();
    if *held != status {
        match &status {
            Status::Away(why) => {
                tracing::error!(dir = %dir.display(), why = %why, "the library folder is not usable: nothing is ingested, uploaded, moved to or from the trash or tidied there until it is back")
            }
            Status::Ready if *held != Status::Unknown => {
                tracing::info!(dir = %dir.display(), "the library folder is back; reel goes on")
            }
            _ => {}
        }
        *held = status.clone();
    }
    status
}

/// Before a write to the library: looked at now, an error that says why when it is away.
pub async fn ensure(ctx: &Ctx) -> anyhow::Result<()> {
    match check(ctx).await {
        Status::Ready => Ok(()),
        Status::Away(why) => Err(Away(why).into()),
        Status::Unknown => Err(Away("not checked yet".into()).into()),
    }
}

/// The check every few seconds, so reel picks up by itself once the library is back, and moves
/// the data folder's trash into the library's once it is there.
pub async fn run(ctx: Ctx) {
    loop {
        if check(&ctx).await == Status::Ready && !ctx.library.old_trash_done.load(Ordering::SeqCst)
        {
            match move_old_trash(&ctx).await {
                Ok(n) => {
                    if n > 0 {
                        tracing::info!(clips = n, "the trash moved into the library");
                    }
                    ctx.library
                        .old_trash_done
                        .store(!ctx.config().old_trash_dir().exists(), Ordering::SeqCst);
                }
                Err(e) => {
                    tracing::warn!(error = %format!("{e:#}"), "the trash did not all move into the library yet")
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(EVERY) => {},
            _ = ctx.shutdown.cancelled() => break,
        }
    }
}

/// Moves the data folder's trash into the library's a clip at a time; a restart halfway goes on
/// where it stopped. How many moved.
pub async fn move_old_trash(ctx: &Ctx) -> anyhow::Result<usize> {
    let cfg = ctx.config();
    let (old, new) = (cfg.old_trash_dir(), cfg.trash_dir());
    if !old.is_dir() || old == new {
        return Ok(0);
    }
    tokio::fs::create_dir_all(&new).await?;
    let mut moved = 0;
    let mut rd = tokio::fs::read_dir(&old).await?;
    while let Some(entry) = rd.next_entry().await? {
        let from = entry.path();
        let name = entry.file_name();
        if ctx.shutdown.is_cancelled() || !ctx.library.ready() {
            break;
        }
        let _held = ctx.library.moves.lock().await;
        if name.to_string_lossy().ends_with(".part") {
            // a copy into the trash a crash cut short; its clip is still in the library
            let _ = tokio::fs::remove_dir_all(&from).await;
            continue;
        }
        let to = new.join(&name);
        if to.exists() {
            // the copy was swapped in before a restart; this is what was left behind
            tokio::fs::remove_dir_all(&from).await?;
        } else {
            crate::enqueue::move_dir(&from, &to).await?;
        }
        moved += 1;
    }
    let _ = tokio::fs::remove_dir(&old).await;
    Ok(moved)
}

/// Whether a folder holds a library: its database, and its library with a marker. The id the
/// marker names, when it does.
pub fn holds_library(dir: &Path) -> Option<String> {
    if !dir.join(DB).is_file() {
        return None;
    }
    let text = std::fs::read_to_string(dir.join("library").join(MARKER)).ok()?;
    Some(marker_id(&text).to_string())
}

/// Rewrites each kept original's absolute path under the data folder as one relative to it, so
/// the folder can move or be copied with them. How many changed.
pub async fn relative_originals(
    store: &reel_store::Store,
    cfg: &reel_core::Config,
) -> anyhow::Result<usize> {
    let mut n = 0;
    for (id, path) in store.originals().await? {
        let stored = cfg.stored(Path::new(&path));
        if stored != path {
            store.set_original_path(&id, Some(&stored)).await?;
            n += 1;
        }
    }
    Ok(n)
}

/// Where a trashed clip's folder is: the library's trash, else the data folder's, not moved yet.
pub fn trashed_dir(cfg: &reel_core::Config, clip_id: &str) -> PathBuf {
    let new = cfg.trash_dir().join(clip_id);
    let old = cfg.old_trash_dir().join(clip_id);
    if !new.exists() && old.exists() {
        old
    } else {
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_read_that_hangs_holds_one_thread_and_no_second_read_starts() {
        let reading = Arc::new(AtomicBool::new(false));
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let first = blocking_within(&reading, Duration::from_millis(50), move || {
            let _ = wait.recv();
        })
        .await;
        assert!(first.unwrap_err().contains("did not answer"));
        let started = Arc::new(AtomicBool::new(false));
        let s = started.clone();
        let second = blocking_within(&reading, Duration::from_millis(50), move || {
            s.store(true, Ordering::SeqCst);
        })
        .await;
        assert!(second.unwrap_err().contains("has not come back"));
        assert!(
            !started.load(Ordering::SeqCst),
            "no second thread while the first hangs"
        );
        release.send(()).unwrap();
        for _ in 0..100 {
            if !reading.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let third = blocking_within(&reading, Duration::from_secs(5), || 7).await;
        assert_eq!(third.unwrap(), 7, "once it is back, the next read runs");
    }

    const ME: &str = "01J0000000000000000000ME01";

    fn bare(exists: bool, empty: bool, clips: bool) -> Bare {
        Bare {
            exists,
            empty,
            clips,
        }
    }

    #[test]
    fn the_marker_decides_and_without_it_the_clips_folders_do() {
        let dir = Path::new("/mnt/nas/reel");
        let any = bare(true, false, false);
        assert_eq!(decide(dir, Some(ME), any, true, ME), Decision::Ready);
        assert_eq!(
            decide(dir, Some(""), any, true, ME),
            Decision::Mark,
            "a file made by hand gives the folder to this reel"
        );
        assert!(matches!(
            decide(dir, Some("01OTHER"), any, true, ME),
            Decision::Away(w) if w.contains("another reel's library")
        ));
        assert_eq!(
            decide(dir, None, bare(false, true, false), false, ME),
            Decision::Make,
            "a new install, or one without clips, has nothing to lose"
        );
        assert_eq!(
            decide(dir, None, bare(true, false, true), true, ME),
            Decision::Mark,
            "an install from before: its library has the clips' folders"
        );
        assert!(matches!(
            decide(dir, None, bare(false, true, false), true, ME),
            Decision::Away(w) if w == "/mnt/nas/reel is not there"
        ));
        assert!(matches!(
            decide(dir, None, bare(true, true, false), true, ME),
            Decision::Away(w) if w == "/mnt/nas/reel is empty, as a disk or a share that is not mounted leaves it"
        ));
        assert!(matches!(
            decide(dir, None, bare(true, false, false), true, ME),
            Decision::Away(w) if w.contains("none of the clips' folders")
        ));
    }

    #[test]
    fn the_marker_reads_back_what_it_wrote() {
        assert_eq!(marker_id(&marker_text(ME)), ME);
        assert_eq!(marker_id(""), "");
        assert_eq!(marker_id("instance   X1  \n"), "X1");
        assert!(marker_text(ME).starts_with("reel's library"));
    }

    #[test]
    fn a_clips_folder_counts_in_the_library_or_in_its_trash() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = tmp.path().join("library");
        let clips = vec![("C1".to_string(), false), ("C2".to_string(), true)];
        assert_eq!(look(&lib, &clips), bare(false, true, false));
        std::fs::create_dir_all(&lib).unwrap();
        assert_eq!(look(&lib, &clips), bare(true, true, false));
        std::fs::create_dir_all(lib.join("lost+found")).unwrap();
        assert_eq!(look(&lib, &clips), bare(true, false, false));
        std::fs::create_dir_all(lib.join(TRASH).join("C2")).unwrap();
        assert_eq!(look(&lib, &clips), bare(true, false, true));
        std::fs::remove_dir_all(lib.join(TRASH)).unwrap();
        std::fs::create_dir_all(lib.join("C1")).unwrap();
        assert_eq!(look(&lib, &clips), bare(true, false, true));
        assert_eq!(read_marker(&lib).unwrap(), None);
        write_marker(&lib, ME).unwrap();
        assert_eq!(read_marker(&lib).unwrap().as_deref(), Some(ME));
    }

    #[test]
    fn a_folder_holds_a_library_with_its_database_and_marker() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(holds_library(tmp.path()), None);
        std::fs::write(tmp.path().join(DB), b"").unwrap();
        assert_eq!(
            holds_library(tmp.path()),
            None,
            "a database alone is not one"
        );
        std::fs::create_dir_all(tmp.path().join("library")).unwrap();
        std::fs::write(tmp.path().join("library").join(MARKER), marker_text("01ID")).unwrap();
        assert_eq!(holds_library(tmp.path()).as_deref(), Some("01ID"));
    }

    #[test]
    fn an_original_in_the_data_folder_is_kept_by_a_relative_path() {
        let cfg = reel_core::Config {
            data_dir: PathBuf::from("/data/reel"),
            ..Default::default()
        };
        assert_eq!(
            cfg.stored(Path::new("/data/reel/originals/C1.mkv")),
            "originals/C1.mkv"
        );
        assert_eq!(
            cfg.stored(Path::new("/originals/C1.mkv")),
            "/originals/C1.mkv",
            "a server's originals folder of its own keeps absolute paths"
        );
        assert_eq!(
            cfg.abs("originals/C1.mkv"),
            Path::new("/data/reel/originals/C1.mkv")
        );
        assert_eq!(
            cfg.abs("/originals/C1.mkv"),
            Path::new("/originals/C1.mkv"),
            "an absolute path reads as it is"
        );
    }
}
