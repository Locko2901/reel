use crate::enqueue::{
    ensure_embed_at, ensure_preview, move_or_copy, request_cut, request_loop_and_strip, CutTarget,
};
use crate::{derived, encode, worker::progress_sink, Ctx};
use anyhow::{anyhow, Context};
use reel_core::config::KeepOriginals;
use reel_core::ids::new_id;
use reel_core::plan::plan_master;
use reel_core::{CutRecipe, Event};
use reel_media::args;
use reel_store::models::*;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Payload {
    input: String,
    source: String,
    #[serde(default)]
    relative: Option<String>,
    #[serde(default)]
    recipe: Option<CutRecipe>,
    #[serde(default)]
    keep_last: Option<f64>,
    #[serde(default)]
    audio_names: Option<Vec<String>>,
}

/// The track names the device gave stand in when the file carries none: OBS writes them as
/// stream titles, which an MP4 drops.
fn name_tracks(probe: &mut reel_core::ProbeInfo, names: Option<&Vec<String>>) {
    if let Some(names) = names {
        if names.len() == probe.audio_titles.len()
            && probe.audio_titles.iter().all(|t| t.trim().is_empty())
        {
            probe.audio_titles = names.clone();
        }
    }
}

pub async fn run(ctx: &Ctx, job: &Job) -> anyhow::Result<()> {
    let payload: Payload = serde_json::from_str(&job.payload).context("ingest payload")?;
    let clip_id = job
        .clip_id
        .clone()
        .ok_or_else(|| anyhow!("ingest job without clip"))?;
    let clip = ctx
        .store
        .clip(&clip_id)
        .await?
        .ok_or_else(|| anyhow!("clip {clip_id} vanished"))?;
    let input = PathBuf::from(&payload.input);
    match process(ctx, job, &clip, &input, &payload).await {
        Ok(Outcome::Published) => {
            if let Some(rel) = &payload.relative {
                let _ = ctx.store.ingest_file_state(rel, "done", None).await;
            }
            ctx.emit(Event::Clip {
                id: clip.id.clone(),
                state: "ready".into(),
                title: clip.title.clone(),
            });
            Ok(())
        }
        Ok(Outcome::Duplicate(first)) => {
            tracing::info!(clip = %clip.id, first = %first.id, name = %clip.source_name, "the library has this file already; the copy is dropped");
            let _ = tokio::fs::remove_file(&input).await;
            let same_owner = match &clip.owner_id {
                Some(owner) => {
                    matches!(ctx.store.user(owner).await, Ok(Some(u)) if first.is_owned_by(&u))
                }
                None => true,
            };
            if same_owner {
                if let Some(dev) = &clip.device_id {
                    let _ = ctx.store.adopt_clip(&first.id, dev).await;
                }
                let _ = ctx.store.repoint_uploads(&clip.id, &first.id).await;
            }
            let _ = ctx.store.delete_clip_keep_jobs(&clip.id).await;
            if let Some(rel) = &payload.relative {
                let _ = ctx.store.ingest_file_done_as(rel, &first.id).await;
            }
            ctx.emit(Event::Clip {
                id: clip.id.clone(),
                state: "trashed".into(),
                title: clip.title.clone(),
            });
            Ok(())
        }
        Err(e) => {
            if ctx.is_cancelled(&job.id).await {
                crate::jobs::cleanup(ctx, job).await;
                return Err(e);
            }
            let msg = format!("{e:#}");
            park_failed(ctx, &input, &payload.source, &msg).await;
            let _ = ctx
                .store
                .set_clip_state(&clip.id, "failed", Some(&msg))
                .await;
            if let Some(rel) = &payload.relative {
                let _ = ctx.store.ingest_file_state(rel, "failed", Some(&msg)).await;
            }
            ctx.emit(Event::Clip {
                id: clip.id.clone(),
                state: "failed".into(),
                title: clip.title.clone(),
            });
            Err(e)
        }
    }
}

async fn park_failed(ctx: &Ctx, input: &Path, source: &str, reason: &str) {
    if !input.exists() {
        return;
    }
    let dir = if source == "upload" {
        ctx.config().uploads_dir().join("failed")
    } else {
        ctx.config().failed_dir()
    };
    let name = input
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    let target = dir.join(&name);
    if let Err(e) = move_or_copy(input, &target).await {
        tracing::warn!(error = %e, "could not park failed file");
        return;
    }
    let mut note = target.as_os_str().to_owned();
    note.push(".txt");
    let _ = tokio::fs::write(PathBuf::from(note), format!("{reason}\n")).await;
}

pub enum Outcome {
    Published,
    Duplicate(Box<Clip>),
}

async fn process(
    ctx: &Ctx,
    job: &Job,
    clip: &Clip,
    input: &Path,
    payload: &Payload,
) -> anyhow::Result<Outcome> {
    if !input.exists() {
        return Err(anyhow!("input file is gone: {}", input.display()));
    }
    let hash = match &clip.source_hash {
        Some(h) => h.clone(),
        None => {
            let h = crate::hash::sha256_file(input)
                .await
                .map_err(|e| anyhow!("could not read it: {e}"))?;
            ctx.store.set_clip_hash(&clip.id, &h).await?;
            h
        }
    };
    if let Some(first) = ctx.store.clip_by_source_hash(&hash, Some(&clip.id)).await? {
        return Ok(Outcome::Duplicate(Box::new(first)));
    }
    let mut probe = ctx
        .ff
        .probe(input)
        .await
        .map_err(|e| anyhow!("ffmpeg could not read it: {e}"))?;
    name_tracks(&mut probe, payload.audio_names.as_ref());
    if probe.duration <= 0.0 {
        return Err(anyhow!("ffmpeg could not read it: no duration"));
    }
    let plan = plan_master(&probe, &ctx.config().media);
    if !plan.copies_video() {
        ctx.wait_while_held(job).await?;
    }
    if clip.recorded_at.is_none() {
        match probe
            .creation_time
            .as_deref()
            .and_then(reel_core::format::parse)
        {
            Some(ct) => {
                let _ = ctx
                    .store
                    .set_recorded_at(&clip.id, &ct.to_string(), "container")
                    .await;
            }
            None => {
                if let Some(modified) = tokio::fs::metadata(input)
                    .await
                    .ok()
                    .and_then(|m| m.modified().ok())
                {
                    let ts = jiff::Timestamp::try_from(modified)
                        .map(|t| t.to_string())
                        .unwrap_or_else(|_| reel_core::format::now());
                    let _ = ctx.store.set_recorded_at(&clip.id, &ts, "file time").await;
                }
            }
        }
    }
    let dir = ctx.config().clip_dir(&clip.id);
    tokio::fs::create_dir_all(&dir).await?;
    let master_id = new_id();
    let r = Rendition::queued(master_id.clone(), clip.id.clone(), "master", None, None, 0);
    ctx.store.insert_rendition(&r).await?;
    ctx.store
        .rendition_state(&master_id, "rendering", None)
        .await?;
    let out = dir.join("master.mp4");
    let mut reason = plan.reason.clone();
    if plan.copies_video() {
        let enc = ctx
            .encoder_for(None, (probe.width, probe.height), probe.fps)
            .await;
        let a = args::master(&plan, &enc, input);
        let cancel = ctx.cancel_token(&job.id).await;
        ctx.ff
            .run_to_with(
                &a,
                &out,
                probe.duration,
                Some(progress_sink(ctx, job)),
                cancel.as_ref(),
            )
            .await
            .map_err(|e| anyhow!("encode failed: {e}"))?;
        let _ = ctx.store.job_route(&job.id, "stream copy").await;
    } else {
        let enc = ctx
            .encoder_for(
                probe.vcodec.as_deref(),
                (probe.width, probe.height),
                probe.fps,
            )
            .await;
        let used = encode::run_ladder(
            ctx,
            job,
            &enc,
            |e| args::master(&plan, e, input),
            &out,
            probe.duration,
        )
        .await?;
        reason = format!("{reason} ({})", used.label());
    }
    let mprobe = ctx
        .ff
        .probe(&out)
        .await
        .map_err(|e| anyhow!("the master does not probe: {e}"))?;
    let rel = format!("library/{}/master.mp4", clip.id);
    ctx.store
        .rendition_ready(&master_id, &rel, &mprobe, plan.copies_video())
        .await?;
    let master = ctx
        .store
        .rendition(&master_id)
        .await?
        .ok_or_else(|| anyhow!("master row missing"))?;
    let trimmed = first_version(ctx, job, clip, &master, payload.keep_last).await;
    let whole_later = trimmed.is_some();
    let (current, current_poster) = match trimmed {
        Some(first) => {
            // the original's poster and loudness come in the derive job, its preview behind the
            // links' work, and none with external storage on, since nothing serves it
            if !ctx.config().external_on() {
                ensure_embed_at(ctx, &master, PRIORITY_DERIVE).await?;
            }
            first
        }
        None => {
            let poster_time = derived::poster_default(ctx, &out, mprobe.duration).await?;
            crate::level::measure_quietly(ctx, &master).await;
            ensure_preview(ctx, &master).await?;
            (master_id.clone(), poster_time)
        }
    };
    let original_path = place_original(ctx, clip, input, plan.copies_video()).await?;
    let probe_json = serde_json::to_string(&probe)?;
    ctx.store
        .publish_clip_showing(
            &clip.id,
            &master_id,
            (&current, current_poster),
            original_path.as_deref(),
            &probe_json,
            &reason,
        )
        .await?;
    request_loop_and_strip(ctx, &clip.id, whole_later).await?;
    if let Some(recipe) = &payload.recipe {
        if !recipe.is_noop() {
            let fresh = ctx
                .store
                .clip(&clip.id)
                .await?
                .ok_or_else(|| anyhow!("clip row missing"))?;
            request_cut(ctx, &fresh, recipe.clone(), CutTarget::Version).await?;
        }
    }
    Ok(Outcome::Published)
}

/// Made before the publish, so no link or library card shows the original first: the last
/// seconds asked for and Normalise new clips, in one cut.
async fn first_version(
    ctx: &Ctx,
    job: &Job,
    clip: &Clip,
    master: &Rendition,
    keep_last: Option<f64>,
) -> Option<(String, f64)> {
    let cfg = ctx.config();
    let path = cfg.abs(master.path.as_deref()?);
    let mut recipe = match keep_last {
        Some(seconds) => {
            let keyframes = ctx.ff.keyframes(&path).await.unwrap_or_default();
            CutRecipe::keep_last(master.duration?, &keyframes, seconds).unwrap_or_default()
        }
        None => CutRecipe::default(),
    };
    if cfg.media.normalize_new && !cfg.media.normalize_audio && master.acodec.is_some() {
        recipe.loudness = Some(cfg.media.loudness_target as f64);
    }
    if recipe.is_noop() {
        return None;
    }
    let seq = ctx.store.next_cut_seq(&clip.id).await.ok()?;
    let r = Rendition::queued(
        new_id(),
        clip.id.clone(),
        "cut",
        Some(master.id.clone()),
        serde_json::to_string(&recipe).ok(),
        seq,
    );
    ctx.store.insert_rendition(&r).await.ok()?;
    match crate::cut::render(ctx, job, &r, clip).await {
        Ok(made) => {
            tracing::info!(clip = %clip.id, cut = %r.id, what = %recipe.describe(made.probe.duration), copy = made.copy, "published with its first version");
            Some((r.id, made.poster_time))
        }
        Err(e) => {
            tracing::warn!(clip = %clip.id, error = %format!("{e:#}"), "the first version failed; the clip stays as it came");
            let _ = ctx
                .store
                .rendition_state(&r.id, "failed", Some(&format!("{e:#}")))
                .await;
            None
        }
    }
}

async fn place_original(
    ctx: &Ctx,
    clip: &Clip,
    input: &Path,
    copied: bool,
) -> anyhow::Result<Option<String>> {
    let keep = match ctx.config().ingest.keep_originals {
        KeepOriginals::All => true,
        KeepOriginals::Reencoded => !copied,
        KeepOriginals::None => false,
    };
    if keep {
        let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("bin");
        let target = ctx
            .config()
            .ingest
            .originals_dir
            .join(format!("{}.{}", clip.id, ext));
        move_or_copy(input, &target)
            .await
            .context("keep original")?;
        Ok(Some(ctx.config().stored(&target)))
    } else {
        tokio::fs::remove_file(input)
            .await
            .context("remove raw file")?;
        Ok(None)
    }
}
