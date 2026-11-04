use crate::enqueue::{create_clip_for_file, NewClip};
use crate::Ctx;
use reel_core::format::now;
use reel_core::Event;
use reel_store::models::IngestFile;
use std::collections::HashSet;
use std::time::Duration;

/// Names the folders never take: hidden files, partial copies, the notes next to parked files.
pub fn skip_name(name: &str) -> bool {
    name.starts_with('.')
        || name.ends_with(".part")
        || name.ends_with(".tmp")
        || name.ends_with(".crdownload")
        || name.ends_with(".txt")
}

pub async fn run_watcher(ctx: Ctx) {
    let period = Duration::from_secs(ctx.config().ingest.poll_seconds.max(1));
    loop {
        if ctx.shutdown.is_cancelled() {
            break;
        }
        if let Err(e) = poll_once(&ctx).await {
            tracing::warn!(error = %e, "incoming poll");
        }
        tokio::select! {
            _ = tokio::time::sleep(period) => {},
            _ = ctx.shutdown.cancelled() => break,
        }
    }
}

/// A file waits in the incoming folder while the library is away.
pub async fn poll_once(ctx: &Ctx) -> anyhow::Result<usize> {
    let dir = ctx.config().ingest.incoming_dir.clone();
    if !dir.is_dir() || !ctx.library.ready() {
        return Ok(0);
    }
    let mut rd = tokio::fs::read_dir(&dir).await?;
    let mut seen_now = HashSet::new();
    let mut queued = 0usize;
    while let Some(entry) = rd.next_entry().await? {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
        else {
            continue;
        };
        if skip_name(&name) || !ctx.config().accepts_extension(&path) {
            continue;
        }
        let meta = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }
        seen_now.insert(name.clone());
        let size = meta.len() as i64;
        let row = ctx.store.ingest_file(&name).await?;
        match row {
            None => {
                ctx.store
                    .upsert_ingest_file(&IngestFile {
                        path: name.clone(),
                        size,
                        first_seen_at: now(),
                        stable_polls: 0,
                        state: "seen".into(),
                        clip_id: None,
                        error: None,
                    })
                    .await?;
                ctx.emit(Event::Ingest {
                    name,
                    state: "seen".into(),
                });
            }
            Some(mut f) if f.state == "seen" => {
                if f.size == size {
                    f.stable_polls += 1;
                } else {
                    f.size = size;
                    f.stable_polls = 0;
                }
                if f.stable_polls >= ctx.config().ingest.stable_polls as i64
                    && std::fs::File::open(&path).is_ok()
                {
                    let (clip, _) = create_clip_for_file(
                        ctx,
                        NewClip {
                            relative: Some(&name),
                            ..NewClip::new(&path, "incoming")
                        },
                    )
                    .await?;
                    ctx.store.ingest_file_taken(&f, &clip.id).await?;
                    queued += 1;
                    ctx.emit(Event::Ingest {
                        name: name.clone(),
                        state: "queued".into(),
                    });
                } else {
                    ctx.store.upsert_ingest_file(&f).await?;
                }
            }
            // a file never taken starts over once it is back; a taken one only when it changed
            Some(mut f)
                if f.state == "gone"
                    || ((f.state == "failed" || f.state == "done") && f.size != size) =>
            {
                f.size = size;
                f.stable_polls = 0;
                f.state = "seen".into();
                f.clip_id = None;
                f.error = None;
                f.first_seen_at = now();
                ctx.store.upsert_ingest_file(&f).await?;
            }
            Some(_) => {}
        }
    }
    for f in ctx.store.ingest_files().await? {
        if f.state == "seen" && !seen_now.contains(&f.path) {
            ctx.store.ingest_file_state(&f.path, "gone", None).await?;
        }
    }
    Ok(queued)
}
