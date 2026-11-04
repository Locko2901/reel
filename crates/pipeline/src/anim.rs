//! An animated picture (GIF or WebP) of a version, kept in the data folder's cache.
use crate::{worker::progress_sink, Ctx};
use anyhow::{anyhow, Context};
use reel_core::recipe::{AnimFormat, AnimRecipe, ANIM_MAX_SECONDS};
use reel_media::args;
use reel_store::models::*;

pub fn dir(ctx: &Ctx) -> std::path::PathBuf {
    ctx.config().data_dir.join("cache").join("anim")
}

pub async fn run(ctx: &Ctx, job: &Job) -> anyhow::Result<()> {
    let rid = job
        .rendition_id
        .clone()
        .ok_or_else(|| anyhow!("anim job without rendition"))?;
    let r = ctx
        .store
        .rendition(&rid)
        .await?
        .ok_or_else(|| anyhow!("rendition vanished"))?;
    let recipe: AnimRecipe =
        serde_json::from_str(r.recipe.as_deref().unwrap_or("{}")).context("anim recipe")?;
    let from = ctx
        .store
        .rendition(r.derived_from.as_deref().unwrap_or(""))
        .await?
        .ok_or_else(|| anyhow!("source version missing"))?;
    let duration = from.duration.unwrap_or(0.0);
    if duration > ANIM_MAX_SECONDS + 0.05 {
        return Err(anyhow!(
            "the version is {} long; an animated picture is {ANIM_MAX_SECONDS} s at most",
            reel_core::format::duration(duration)
        ));
    }
    let input = ctx.config().abs(
        from.path
            .as_deref()
            .ok_or_else(|| anyhow!("the version has no file"))?,
    );
    ctx.wait_while_held(job).await?;
    ctx.store.rendition_state(&rid, "rendering", None).await?;
    let size = recipe.size(
        from.width.unwrap_or(0) as u32,
        from.height.unwrap_or(0) as u32,
    );
    let _ = ctx
        .store
        .job_route(
            &job.id,
            &format!(
                "{}, {} by {}, {} fps",
                recipe.format.name(),
                size.0,
                size.1,
                recipe.fps
            ),
        )
        .await;
    tokio::fs::create_dir_all(dir(ctx)).await?;
    let name = format!(
        "{}-{}-{}-{}.{}",
        from.id,
        recipe.format.ext(),
        recipe.width,
        recipe.fps,
        recipe.format.ext()
    );
    let out = dir(ctx).join(&name);
    let cancel = ctx.cancel_token(&job.id).await;
    ctx.ff
        .run_to_with(
            &args::anim(&input, recipe.format, size, recipe.fps),
            &out,
            duration,
            Some(progress_sink(ctx, job)),
            cancel.as_ref(),
        )
        .await
        .map_err(|e| anyhow!("the {} did not render: {e}", recipe.format.name()))?;
    let bytes = tokio::fs::metadata(&out).await?.len();
    let probe = reel_core::ProbeInfo {
        duration,
        size: bytes,
        format_name: recipe.format.ext().into(),
        width: size.0,
        height: size.1,
        vcodec: Some(
            match recipe.format {
                AnimFormat::Gif => "gif",
                AnimFormat::Webp => "webp",
            }
            .into(),
        ),
        fps: f64::from(recipe.fps),
        ..Default::default()
    };
    ctx.store
        .rendition_ready(&rid, &format!("cache/anim/{name}"), &probe, false)
        .await?;
    tracing::info!(clip = %from.clip_id, bytes, format = recipe.format.ext(), "an animated picture made");
    Ok(())
}

/// Deletes a made picture's file and row; one still being made is left to its job.
pub async fn delete(ctx: &Ctx, r: &Rendition) -> anyhow::Result<bool> {
    if r.kind != "anim" || matches!(r.state.as_str(), "queued" | "rendering") {
        return Ok(false);
    }
    if let Some(p) = &r.path {
        let _ = tokio::fs::remove_file(ctx.config().abs(p)).await;
    }
    ctx.store.delete_rendition(&r.id).await?;
    Ok(true)
}
