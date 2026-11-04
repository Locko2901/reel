use crate::enqueue::{ensure_preview, request_loop_and_strip};
use crate::{derived, encode, level, worker::progress_sink, Ctx};
use anyhow::{anyhow, Context};
use reel_core::{CutRecipe, Event};
use reel_media::args::{self, CutAudio};
use reel_media::level::parse_loudnorm;
use reel_store::models::*;
use std::path::Path;

pub fn snap_start(start: f64, keyframes: &[f64], tolerance: f64) -> Option<f64> {
    keyframes
        .iter()
        .copied()
        .filter(|k| (k - start).abs() <= tolerance)
        .min_by(|a, b| (a - start).abs().partial_cmp(&(b - start).abs()).unwrap())
}

pub async fn run(ctx: &Ctx, job: &Job) -> anyhow::Result<()> {
    let rid = job
        .rendition_id
        .clone()
        .ok_or_else(|| anyhow!("cut job without rendition"))?;
    let r = ctx
        .store
        .rendition(&rid)
        .await?
        .ok_or_else(|| anyhow!("rendition vanished"))?;
    let clip = ctx
        .store
        .clip(&r.clip_id)
        .await?
        .ok_or_else(|| anyhow!("clip vanished"))?;
    let made = render(ctx, job, &r, &clip).await?;
    if r.kind == "master" {
        let source = ctx.store.clip(&made.from_clip).await?;
        let reason = format!(
            "cut from {}, {}",
            source
                .map(|c| c.title)
                .unwrap_or_else(|| "another clip".into()),
            if made.copy {
                "stream copy"
            } else {
                "re-encoded"
            }
        );
        ctx.store
            .publish_clip(
                &clip.id,
                &r.id,
                made.poster_time,
                None,
                &serde_json::to_string(&made.probe)?,
                &reason,
            )
            .await?;
    } else {
        let now = ctx
            .store
            .clip(&clip.id)
            .await?
            .unwrap_or_else(|| clip.clone());
        let version = ctx
            .store
            .rendition(&r.id)
            .await?
            .ok_or_else(|| anyhow!("cut row missing"))?;
        crate::enqueue::make_current(ctx, &now, &version).await?;
    }
    request_loop_and_strip(ctx, &clip.id, false).await?;
    ctx.emit(Event::Clip {
        id: clip.id.clone(),
        state: "ready".into(),
        title: clip.title.clone(),
    });
    Ok(())
}

/// What a rendered cut leaves for whoever publishes it.
pub struct Made {
    pub poster_time: f64,
    pub copy: bool,
    pub probe: reel_core::ProbeInfo,
    pub from_clip: String,
}

/// Renders a cut rendition from its master, marks it ready with its poster, its level and
/// its preview copy queued; publishing it is the caller's.
pub async fn render(ctx: &Ctx, job: &Job, r: &Rendition, clip: &Clip) -> anyhow::Result<Made> {
    let rid = r.id.clone();
    let master = ctx
        .store
        .rendition(r.derived_from.as_deref().unwrap_or(""))
        .await?
        .ok_or_else(|| anyhow!("master missing"))?;
    let master_path = ctx.config().abs(
        master
            .path
            .as_deref()
            .ok_or_else(|| anyhow!("master has no file"))?,
    );
    let mut recipe: CutRecipe =
        serde_json::from_str(r.recipe.as_deref().unwrap_or("{}")).context("recipe")?;
    let duration = master.duration.unwrap_or(0.0);
    let fps = master.fps.unwrap_or(60.0).max(1.0);
    if let Some(e) = recipe.end {
        if e >= duration - 0.05 {
            recipe.end = None;
        }
    }
    if let Some(s) = recipe.start {
        if s <= 0.0 {
            recipe.start = None;
        }
    }
    // a reframed or faded picture is encoded again whatever its start
    let copy = !recipe.pictured()
        && match recipe.start {
            None => true,
            Some(s) => {
                let kf = ctx.ff.keyframes(&master_path).await.unwrap_or_default();
                match snap_start(s, &kf, 1.0 / fps + 0.001) {
                    Some(k) => {
                        recipe.start = if k <= 0.0 { None } else { Some(k) };
                        true
                    }
                    None => false,
                }
            }
        };
    let into_new_clip = r.kind == "master";
    ctx.store.rendition_state(&rid, "rendering", None).await?;
    let dir = ctx.config().clip_dir(&clip.id);
    tokio::fs::create_dir_all(&dir).await?;
    let file = if into_new_clip {
        "master.mp4".to_string()
    } else {
        format!("cut-{rid}.mp4")
    };
    let out = dir.join(&file);
    let len = recipe.end.unwrap_or(duration) - recipe.start.unwrap_or(0.0);
    let audio_file = dir.join(format!("{file}.audio.part"));
    let mut audio_note = None;
    let tracks = match master.acodec {
        Some(_) => ctx.ff.audio_streams(&master_path).await.unwrap_or(1),
        None => 0,
    };
    let active: Vec<(usize, f64)> = recipe
        .active_tracks()
        .into_iter()
        .filter(|(i, _)| *i < tracks)
        .collect();
    let as_it_is = recipe.gain_db == 0.0
        && recipe.loudness.is_none()
        && !recipe.fades()
        && matches!(active[..], [(_, g)] if g == 0.0);
    let audio = if active.is_empty() {
        CutAudio::None
    } else if as_it_is {
        CutAudio::Copy(active[0].0)
    } else {
        let note = render_audio(ctx, job, &recipe, &active, &master_path, &audio_file, len)
            .await
            .map_err(|e| anyhow!("the audio failed: {e}"));
        if note.is_err() {
            let _ = tokio::fs::remove_file(&audio_file).await;
        }
        audio_note = Some(note?);
        CutAudio::File(&audio_file)
    };
    let rendered = render_video(
        ctx,
        job,
        &recipe,
        copy,
        &master,
        &master_path,
        audio,
        &out,
        len,
        fps,
    )
    .await;
    let _ = tokio::fs::remove_file(&audio_file).await;
    rendered?;
    if copy {
        let route = match &audio_note {
            Some(note) => format!("video copied, {note}"),
            None => "stream copy".into(),
        };
        let _ = ctx.store.job_route(&job.id, &route).await;
    }
    let probe = ctx
        .ff
        .probe(&out)
        .await
        .map_err(|e| anyhow!("the cut does not probe: {e}"))?;
    let rel = format!("library/{}/{file}", clip.id);
    ctx.store.rendition_ready(&rid, &rel, &probe, copy).await?;
    let fresh = ctx
        .store
        .rendition(&rid)
        .await?
        .ok_or_else(|| anyhow!("cut row missing"))?;
    ensure_preview(ctx, &fresh).await?;
    let poster_time = derived::poster_default(ctx, &out, probe.duration).await?;
    level::measure_quietly(ctx, &fresh).await;
    Ok(Made {
        poster_time,
        copy,
        probe,
        from_clip: master.clip_id.clone(),
    })
}

#[allow(clippy::too_many_arguments)]
async fn render_video(
    ctx: &Ctx,
    job: &Job,
    recipe: &CutRecipe,
    copy: bool,
    master: &Rendition,
    master_path: &Path,
    audio: CutAudio<'_>,
    out: &Path,
    len: f64,
    fps: f64,
) -> anyhow::Result<()> {
    let source = (
        master.width.unwrap_or(0) as u32,
        master.height.unwrap_or(0) as u32,
    );
    if copy {
        let enc = ctx.encoder_for(None, source, fps).await;
        let a = args::cut(recipe, true, &enc, master_path, audio);
        let cancel = ctx.cancel_token(&job.id).await;
        ctx.ff
            .run_to_with(
                &a,
                out,
                len.max(0.1),
                Some(progress_sink(ctx, job)),
                cancel.as_ref(),
            )
            .await
            .map_err(|e| anyhow!("cut failed: {e}"))?;
    } else {
        ctx.wait_while_held(job).await?;
        let mut enc = ctx.encoder_for(master.vcodec.as_deref(), source, fps).await;
        if recipe.pictured() {
            // the crop and the fades are software filters: the frames come to them decoded
            enc = enc.software_decode();
        }
        encode::run_ladder(
            ctx,
            job,
            &enc,
            |e| args::cut_with(recipe, false, e, master_path, audio, Some(len)),
            out,
            len.max(0.1),
        )
        .await
        .map_err(|e| anyhow!("cut failed: {e}"))?;
    }
    Ok(())
}

/// The AAC encoder can take a limited peak over the ceiling; the audio is then made again
/// with the aim lowered by the excess and 0.2 dB.
async fn render_audio(
    ctx: &Ctx,
    job: &Job,
    recipe: &CutRecipe,
    active: &[(usize, f64)],
    master_path: &Path,
    out: &Path,
    len: f64,
) -> anyhow::Result<String> {
    let kbit = ctx.config().media.audio_kbit;
    let raises = recipe.gain_db > 0.0
        || recipe.loudness.is_some()
        || active.len() > 1
        || active.iter().any(|(_, g)| *g > 0.0);
    let cancel = ctx.cancel_token(&job.id).await;
    let mut aim = args::FIRST_AIM;
    for attempt in 1..=3 {
        let measured = match recipe.loudness {
            Some(_) => parse_loudnorm(
                &ctx.ff
                    .analyse(&args::loudnorm_pass(recipe, master_path, active, aim))
                    .await?,
            ),
            None => None,
        };
        let mut post =
            args::audio_filter(recipe, measured.as_ref(), aim).unwrap_or_else(|| "anull".into());
        if let Some(fades) = args::audio_fades(recipe, len) {
            post = if post == "anull" {
                fades
            } else {
                format!("{post},{fades}")
            };
        }
        let mut a = args::cut_audio(recipe, &args::mix_graph(active, &post), kbit, master_path);
        a.push(out.to_string_lossy().into_owned());
        ctx.ff
            .run_with(&a, len.max(0.1), None, cancel.as_ref())
            .await?;
        let what = match recipe.loudness {
            Some(t) => format!("audio to {t:.0} LUFS"),
            None if recipe.gain_db != 0.0 => {
                format!("audio {}", reel_core::format::gain(recipe.gain_db))
            }
            None if recipe.fades() && active.len() == 1 => "audio faded".to_string(),
            None => format!("audio from {} tracks", active.len()),
        };
        if !raises {
            return Ok(what);
        }
        let peak = ctx.ff.audio_level(out).await?.peak;
        match peak {
            Some(p) if p > args::PEAK_CEILING && attempt < 3 => {
                tracing::info!(job = %job.id, peak = p, aim, "the encoded audio went over the ceiling; again, lower");
                aim = (aim - (p - args::PEAK_CEILING) - 0.2).max(-9.0);
            }
            Some(p) if p > args::PEAK_CEILING => {
                tracing::warn!(job = %job.id, peak = p, "the audio stays over the ceiling after three tries");
                return Ok(format!(
                    "{what}, peak {}",
                    reel_core::format::level(p, "dBTP")
                ));
            }
            _ => return Ok(what),
        }
    }
    unreachable!("the last attempt returns")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_only_within_tolerance() {
        let kf = [0.0, 2.0, 4.0, 6.0];
        assert_eq!(snap_start(2.01, &kf, 0.02), Some(2.0));
        assert_eq!(snap_start(3.0, &kf, 0.02), None);
        assert_eq!(snap_start(0.0, &kf, 0.02), Some(0.0));
    }
}
