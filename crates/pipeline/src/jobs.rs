use crate::Ctx;
use anyhow::{anyhow, Context};
use reel_core::Event;
use reel_store::models::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct IngestPayload {
    input: String,
    source: String,
    #[serde(default)]
    relative: Option<String>,
}

/// Cancels a queued or running job. A running one is told through its token; the worker marks
/// it cancelled once its cleanup is done, so whoever sees the state can rely on the files.
pub async fn cancel(ctx: &Ctx, job_id: &str) -> anyhow::Result<bool> {
    let job = ctx
        .store
        .job(job_id)
        .await?
        .ok_or_else(|| anyhow!("no such job"))?;
    match job.state.as_str() {
        "running" => {
            if let Some(t) = ctx.cancel_token(job_id).await {
                t.cancel();
                return Ok(true);
            }
            cleanup(ctx, &job).await;
            ctx.store.cancel_job(job_id).await?;
            Ok(true)
        }
        "queued" => {
            cleanup(ctx, &job).await;
            ctx.store.cancel_job(job_id).await?;
            crate::worker::emit_job(ctx, &job, "cancelled", 0.0, None);
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// Once a cancelled job has returned: its pending version goes, then the state. An ingest
/// cleans up in its own error path.
pub async fn after_cancel(ctx: &Ctx, job: &Job) {
    if job.kind != "ingest" {
        cleanup(ctx, job).await;
    }
    let _ = ctx.store.cancel_job(&job.id).await;
}

/// A cancelled ingest leaves the raw file for the next poll and drops the half-made clip; any
/// other job loses its pending version, and a cut into a new clip or a montage its clip.
pub async fn cleanup(ctx: &Ctx, job: &Job) {
    match job.kind.as_str() {
        "ingest" => {
            if let Ok(p) = serde_json::from_str::<IngestPayload>(&job.payload) {
                if let Some(rel) = &p.relative {
                    let _ = ctx.store.delete_ingest_file(rel).await;
                }
            }
            if let Some(clip_id) = &job.clip_id {
                let dir = ctx.config().clip_dir(clip_id);
                if dir.exists() {
                    let _ = tokio::fs::remove_dir_all(&dir).await;
                }
                let title = ctx
                    .store
                    .clip(clip_id)
                    .await
                    .ok()
                    .flatten()
                    .map(|c| c.title)
                    .unwrap_or_default();
                let _ = ctx.store.delete_clip_keep_jobs(clip_id).await;
                ctx.emit(Event::Clip {
                    id: clip_id.clone(),
                    state: "removed".into(),
                    title,
                });
            }
        }
        _ => {
            if let Some(rid) = &job.rendition_id {
                if let Ok(Some(r)) = ctx.store.rendition(rid).await {
                    if let Some(p) = &r.path {
                        let _ = tokio::fs::remove_file(ctx.config().abs(p)).await;
                    }
                    let into_new_clip =
                        r.kind == "master" && (r.derived_from.is_some() || job.kind == "montage");
                    let _ = ctx.store.delete_rendition(rid).await;
                    if into_new_clip {
                        let dir = ctx.config().clip_dir(&r.clip_id);
                        if dir.exists() {
                            let _ = tokio::fs::remove_dir_all(&dir).await;
                        }
                        let _ = ctx.store.delete_clip_keep_jobs(&r.clip_id).await;
                        ctx.emit(Event::Clip {
                            id: r.clip_id.clone(),
                            state: "removed".into(),
                            title: String::new(),
                        });
                    }
                }
            }
        }
    }
}

fn parked_path(ctx: &Ctx, input: &Path, source: &str) -> PathBuf {
    let dir = if source == "upload" {
        ctx.config().uploads_dir().join("failed")
    } else {
        ctx.config().failed_dir()
    };
    dir.join(input.file_name().unwrap_or_default())
}

/// Runs a failed job again from the start: the parked raw file goes back, the pending version
/// is queued again, and a new job row carries the attempt count on.
pub async fn retry(ctx: &Ctx, job_id: &str) -> anyhow::Result<Job> {
    let old = ctx
        .store
        .job(job_id)
        .await?
        .ok_or_else(|| anyhow!("no such job"))?;
    if old.state != "failed" {
        return Err(anyhow!("only a failed job can be retried"));
    }
    match old.kind.as_str() {
        "ingest" => {
            let p: IngestPayload = serde_json::from_str(&old.payload).context("ingest payload")?;
            let input = PathBuf::from(&p.input);
            if !input.exists() {
                let parked = parked_path(ctx, &input, &p.source);
                if parked.exists() {
                    crate::enqueue::move_or_copy(&parked, &input)
                        .await
                        .context("bring the raw file back")?;
                    let mut note = parked.into_os_string();
                    note.push(".txt");
                    let _ = tokio::fs::remove_file(PathBuf::from(note)).await;
                } else {
                    return Err(anyhow!("the raw file is gone: {}", input.display()));
                }
            }
            if let Some(clip_id) = &old.clip_id {
                ctx.store
                    .set_clip_state(clip_id, "processing", None)
                    .await?;
                if let Some(rel) = &p.relative {
                    let _ = ctx.store.ingest_file_state(rel, "queued", None).await;
                }
            }
        }
        _ => {
            if let Some(rid) = &old.rendition_id {
                ctx.store.rendition_state(rid, "queued", None).await?;
            }
        }
    }
    let job = Job::retry_of(&old);
    ctx.store.enqueue(&job).await?;
    ctx.store.dismiss_job(&old.id).await?;
    if let (Some(clip_id), true) = (&old.clip_id, old.kind == "ingest") {
        let title = ctx
            .store
            .clip(clip_id)
            .await?
            .map(|c| c.title)
            .unwrap_or_default();
        ctx.emit(Event::Clip {
            id: clip_id.clone(),
            state: "processing".into(),
            title,
        });
    }
    crate::worker::emit_job(ctx, &job, "queued", 0.0, None);
    Ok(job)
}

pub async fn dismiss(ctx: &Ctx, job_id: &str) -> anyhow::Result<()> {
    let job = ctx
        .store
        .job(job_id)
        .await?
        .ok_or_else(|| anyhow!("no such job"))?;
    if job.state != "failed" {
        return Err(anyhow!("only a failed job can be dismissed"));
    }
    ctx.store.dismiss_job(job_id).await?;
    Ok(())
}

async fn remove_if_there(path: &Path) -> anyhow::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("remove {}", path.display())),
    }
}

/// Throws a failed ingest away for good: the parked file and its note, the clip's folder and
/// rows. Once housekeeping pruned its job rows, only the parked file is known, by its name.
pub async fn discard(ctx: &Ctx, clip_id: &str) -> anyhow::Result<()> {
    let clip = ctx
        .store
        .clip(clip_id)
        .await?
        .ok_or_else(|| anyhow!("no such clip"))?;
    if clip.state != "failed" {
        return Err(anyhow!("only a failed clip can be discarded"));
    }
    let jobs = ctx.store.jobs_of_clip(clip_id).await?;
    let payload = jobs
        .iter()
        .filter(|j| j.kind == "ingest")
        .find_map(|j| serde_json::from_str::<IngestPayload>(&j.payload).ok());
    let parked = match &payload {
        Some(p) => parked_path(ctx, Path::new(&p.input), &p.source),
        None => parked_path(ctx, Path::new(&clip.source_name), &clip.source),
    };
    let mut note = parked.clone().into_os_string();
    note.push(".txt");
    remove_if_there(&parked).await?;
    remove_if_there(&PathBuf::from(note)).await?;
    if let Some(p) = &payload {
        let still_mine = match &p.relative {
            Some(rel) => ctx
                .store
                .ingest_file(rel)
                .await?
                .map(|f| f.clip_id.as_deref() == Some(clip_id))
                .unwrap_or(false),
            None => true,
        };
        if still_mine {
            remove_if_there(Path::new(&p.input)).await?;
        }
    }
    ctx.store.delete_ingest_files_of_clip(clip_id).await?;
    let dir = ctx.config().clip_dir(clip_id);
    if dir.exists() {
        tokio::fs::remove_dir_all(&dir)
            .await
            .context("remove the clip folder")?;
    }
    ctx.store.delete_clip_rows(clip_id).await?;
    for j in jobs.iter().filter(|j| j.state == "failed") {
        crate::worker::emit_job(ctx, j, "discarded", 0.0, None);
    }
    ctx.emit(Event::Clip {
        id: clip_id.to_string(),
        state: "removed".into(),
        title: clip.title,
    });
    Ok(())
}

/// Discard from the job's side: the failed ingest's clip goes with it.
pub async fn discard_job(ctx: &Ctx, job_id: &str) -> anyhow::Result<()> {
    let job = ctx
        .store
        .job(job_id)
        .await?
        .ok_or_else(|| anyhow!("no such job"))?;
    if job.kind != "ingest" || job.state != "failed" {
        return Err(anyhow!("only a failed ingest can be discarded"));
    }
    match &job.clip_id {
        Some(clip_id) => discard(ctx, clip_id).await,
        None => {
            if let Ok(p) = serde_json::from_str::<IngestPayload>(&job.payload) {
                let parked = parked_path(ctx, Path::new(&p.input), &p.source);
                let mut note = parked.clone().into_os_string();
                note.push(".txt");
                remove_if_there(&parked).await?;
                remove_if_there(&PathBuf::from(note)).await?;
            }
            ctx.store.delete_job(job_id).await?;
            crate::worker::emit_job(ctx, &job, "discarded", 0.0, None);
            Ok(())
        }
    }
}

/// The words for what a job does, from its kind, its clip and its rendition.
pub fn what(
    job: &Job,
    clip: Option<&Clip>,
    rendition: Option<&Rendition>,
    embed_mb: u64,
) -> String {
    match job.kind.as_str() {
        "ingest" => match clip.and_then(|c| c.plan_reason.clone()) {
            Some(reason) if job.state == "done" => reason,
            _ => "probe, plan and publish".into(),
        },
        "derive" => {
            if job.payload.contains("\"poster\":false") {
                "the hover loop and the filmstrip".into()
            } else {
                "posters, loops and filmstrips again".into()
            }
        }
        "cut" => {
            let mut parts = Vec::new();
            if let Some(r) = rendition {
                if let Some(recipe) = r
                    .recipe
                    .as_deref()
                    .and_then(|x| serde_json::from_str::<reel_core::CutRecipe>(x).ok())
                {
                    parts.push(recipe.describe_cut(r.duration.unwrap_or(0.0)));
                }
                if r.kind == "master" {
                    parts.push("as a new clip".into());
                } else {
                    parts.push(format!("as {}", r.label()));
                }
            }
            if parts.is_empty() {
                "a cut".into()
            } else {
                parts.join(", ")
            }
        }
        "embed" => match rendition {
            Some(r) if r.kind == "embed_ext" => {
                "a preview video for the link previews from the external storage".into()
            }
            _ => format!("a preview video under {embed_mb} MB for the link previews"),
        },
        "web" => match rendition.filter(|r| r.is_ready()) {
            Some(r) => format!(
                "a web copy for the link's page, {}p, {}",
                r.height.unwrap_or(0),
                reel_core::format::size(r.size.unwrap_or(0) as u64)
            ),
            None => "a web copy for the link's page".into(),
        },
        "anim" => {
            let what = rendition
                .and_then(|r| r.recipe.as_deref())
                .and_then(|x| serde_json::from_str::<reel_core::recipe::AnimRecipe>(x).ok())
                .map(|a| {
                    format!(
                        "an animated {}, {} px, {} fps",
                        a.format.name(),
                        a.width,
                        a.fps
                    )
                })
                .unwrap_or_else(|| "an animated picture".into());
            match rendition
                .and_then(|r| r.size)
                .filter(|_| job.state == "done")
            {
                Some(size) => format!("{what}, {}", reel_core::format::size(size as u64)),
                None => what,
            }
        }
        "montage" => {
            let parts = rendition
                .and_then(|r| r.recipe.as_deref())
                .and_then(|x| serde_json::from_str::<reel_core::recipe::MontageRecipe>(x).ok())
                .map(|m| m.parts.len())
                .unwrap_or(0);
            format!("a montage of {parts} clips")
        }
        "discord" => {
            let limit = rendition
                .and_then(|r| r.recipe.as_deref())
                .and_then(|x| serde_json::from_str::<reel_core::recipe::DiscordRecipe>(x).ok())
                .map(|d| d.limit_bytes / 1_000_000)
                .unwrap_or(0);
            match rendition
                .and_then(|r| r.size)
                .filter(|_| job.state == "done")
            {
                Some(size) => format!(
                    "copy under {limit} MB, {}",
                    reel_core::format::size(size as u64)
                ),
                None => format!("copy under {limit} MB"),
            }
        }
        other => other.to_string(),
    }
}
