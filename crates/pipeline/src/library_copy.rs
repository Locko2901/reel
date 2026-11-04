//! A copy of the library in a folder of its own, made while reel runs: the database first, then
//! the files of every clip it holds, then all of it read back before the folder gets its name.
use crate::copier::{self, Copied};
use crate::Ctx;
use anyhow::Context;
use reel_core::config::{MARKER, TRASH};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub const README: &str = "README.txt";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Database,
    Copying,
    Checking,
}

impl Phase {
    pub fn word(&self) -> &'static str {
        match self {
            Phase::Database => "database",
            Phase::Copying => "copying",
            Phase::Checking => "checking",
        }
    }
}

/// What a finished copy holds.
#[derive(Debug, Clone)]
pub struct Done {
    pub dir: PathBuf,
    pub clips: usize,
    pub bytes: u64,
    /// Clips deleted for good while the copy ran, left out of it.
    pub gone: usize,
}

/// The folder a copy made now gets in `into`, and its name while it is made.
pub fn names(into: &Path, at: &jiff::Zoned) -> (PathBuf, PathBuf) {
    let name = format!("reel copy {}", at.strftime("%Y-%m-%d %H%M"));
    (into.join(&name), into.join(format!("{name} (unfinished)")))
}

/// What a copy would take: the database, the clips' folders and the kept originals, without the
/// copies reel makes again.
pub fn measure(cfg: &reel_core::Config) -> u64 {
    let lib = cfg.library_dir();
    let mut bytes = std::fs::metadata(cfg.db_path())
        .map(|m| m.len())
        .unwrap_or(0);
    if let Ok(entries) = copier::list(&lib, &|r| is_part(r)) {
        bytes += copier::total(&entries);
    }
    let originals = &cfg.ingest.originals_dir;
    if originals.is_dir() {
        if let Ok(entries) = copier::list(originals, &|_| false) {
            bytes += copier::total(&entries);
        }
    }
    bytes
}

fn is_part(rel: &Path) -> bool {
    rel.extension().and_then(|e| e.to_str()) == Some("part")
}

/// Makes the copy in `into`, under the name `names` gives for `at`. On a stop or a failure the
/// unfinished folder goes.
pub async fn make(
    ctx: &Ctx,
    into: &Path,
    at: &jiff::Zoned,
    stop: Arc<AtomicBool>,
    progress: Arc<dyn Fn(Phase, u64, u64) + Send + Sync>,
) -> anyhow::Result<Done> {
    let (dir, part) = names(into, at);
    if dir.exists() || part.exists() {
        anyhow::bail!("{} is there already", dir.display());
    }
    crate::library::ensure(ctx).await?;
    tokio::fs::create_dir_all(&part)
        .await
        .with_context(|| format!("{} cannot be made", part.display()))?;
    let made = run(ctx, &part, at, stop, progress).await;
    match made {
        Ok(mut done) => {
            tokio::fs::rename(&part, &dir).await.with_context(|| {
                format!("{} cannot be renamed to {}", part.display(), dir.display())
            })?;
            done.dir = dir;
            Ok(done)
        }
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(&part).await;
            Err(e)
        }
    }
}

async fn run(
    ctx: &Ctx,
    part: &Path,
    at: &jiff::Zoned,
    stop: Arc<AtomicBool>,
    progress: Arc<dyn Fn(Phase, u64, u64) + Send + Sync>,
) -> anyhow::Result<Done> {
    let cfg = ctx.config();
    let total = measure(&cfg);
    progress(Phase::Database, 0, total);
    let db = part.join(crate::library::DB);
    ctx.store.backup_to(&db).await.context("the database")?;
    let secrets: Vec<&str> = reel_core::settings::secret_keys().collect();
    reel_store::Store::blank_settings_in(&db, &secrets).await?;
    let copy = reel_store::Store::open(&db).await?;
    let id = crate::library::instance_id(ctx).await?;
    if copy.setting("instance_id").await?.is_none() {
        copy.set_setting("instance_id", &id).await?;
    }
    let mut done = std::fs::metadata(&db).map(|m| m.len()).unwrap_or(0);
    progress(Phase::Copying, done, total);
    let mut copied: Vec<Copied> = Vec::new();
    let mut gone = Vec::new();
    let places = copy.clip_places().await?;
    for (clip, trashed) in &places {
        let from = {
            let live = cfg.clip_dir(clip);
            if live.is_dir() {
                live
            } else {
                crate::library::trashed_dir(&cfg, clip)
            }
        };
        let place = if *trashed {
            Path::new("library").join(TRASH).join(clip)
        } else {
            Path::new("library").join(clip)
        };
        // a move to or from the trash waits until this clip's folder is copied
        let _held = ctx.library.moves.lock().await;
        if !from.is_dir() {
            gone.push(clip.clone());
            continue;
        }
        let (to, st, pr) = (part.join(&place), stop.clone(), progress.clone());
        let base = done;
        let (files, n) = tokio::task::spawn_blocking(move || {
            let entries = copier::list(&from, &|r| is_part(r))?;
            let mut n = 0;
            let files = copier::copy_all(&from, &to, &entries, None, &st, &mut |b| {
                n += b;
                pr(Phase::Copying, base + n, total.max(base + n));
            })?;
            Ok::<_, std::io::Error>((files, n))
        })
        .await??;
        done += n;
        copied.extend(files.into_iter().map(|mut c| {
            c.rel = place.join(&c.rel);
            c
        }));
    }
    for clip in &gone {
        copy.delete_clip_rows(clip).await?;
        tracing::info!(
            clip,
            "a clip deleted for good while the copy ran is left out of it"
        );
    }
    for (clip, path) in copy.originals().await? {
        let from = cfg.abs(&path);
        let Some(name) = from.file_name().map(|n| n.to_owned()) else {
            continue;
        };
        let rel = Path::new("originals").join(&name);
        copy.set_original_path(
            &clip,
            Some(&format!("originals/{}", name.to_string_lossy())),
        )
        .await?;
        if !from.is_file() {
            continue;
        }
        let (to, st, pr) = (part.join(&rel), stop.clone(), progress.clone());
        let base = done;
        let mut c = tokio::task::spawn_blocking(move || {
            let mut n = 0;
            copier::copy_file(&from, &to, &st, &mut |b| {
                n += b;
                pr(Phase::Copying, base + n, total.max(base + n));
            })
        })
        .await??;
        done += c.size;
        c.rel = rel;
        copied.push(c);
    }
    std::fs::create_dir_all(part.join("library"))?;
    std::fs::write(
        part.join("library").join(MARKER),
        crate::library::marker_text(&id),
    )?;
    let ok = copy.quick_check().await?;
    copy.close().await;
    if ok != "ok" {
        anyhow::bail!("the copy of the database does not check out: {ok}");
    }
    let clips = places.len() - gone.len();
    let bytes = done;
    std::fs::write(part.join(README), readme(at, clips, bytes))?;
    let (to, st, pr) = (part.to_path_buf(), stop.clone(), progress.clone());
    tokio::task::spawn_blocking(move || {
        let mut n = 0;
        copier::check_all(&to, &copied, &st, &mut |b| {
            n += b;
            pr(Phase::Checking, n, bytes);
        })
    })
    .await??;
    Ok(Done {
        dir: part.to_path_buf(),
        clips,
        bytes,
        gone: gone.len(),
    })
}

/// What the copy's own README says.
pub fn readme(at: &jiff::Zoned, clips: usize, bytes: u64) -> String {
    format!(
        "A copy of a reel library

Made by reel {version} on {day} at {time}: {clips} {noun}, {size}.

reel.db     the database: the titles, versions, collections, stars and settings
library     every clip with its versions, poster and loops; the trash in .trash
originals   the raw files reel kept of clips it encoded again

To use it, in the reel app on this PC or another one: Settings, This app,
Server, Library folder, Move, and pick this folder. reel takes up the library
in it as it is and works in this folder from then on. To keep this copy as it
is, copy the folder to where the library should live first, and pick that one.
",
        version = reel_core::VERSION,
        day = at.strftime("%Y-%m-%d"),
        time = at.strftime("%H:%M"),
        noun = if clips == 1 { "clip" } else { "clips" },
        size = reel_core::format::size(bytes),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_is_named_for_its_minute_and_unfinished_until_checked() {
        let at: jiff::Zoned = "2026-11-04T21:30:59[UTC]".parse().unwrap();
        let (dir, part) = names(Path::new("/backup"), &at);
        assert_eq!(dir, Path::new("/backup/reel copy 2026-11-04 2130"));
        assert_eq!(
            part,
            Path::new("/backup/reel copy 2026-11-04 2130 (unfinished)")
        );
        let text = readme(&at, 1, 1_500_000);
        assert!(
            text.contains("on 2026-11-04 at 21:30: 1 clip, 1.5 MB."),
            "{text}"
        );
        assert!(text.contains("Library folder, Move"));
    }
}
