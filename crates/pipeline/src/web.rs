use crate::{encode, Ctx};
use anyhow::{anyhow, Context};
use reel_core::plan::web_plan;
use reel_core::recipe::WebRecipe;
use reel_media::args;
use reel_store::models::*;
use std::time::Duration;

pub async fn run(ctx: &Ctx, job: &Job) -> anyhow::Result<()> {
    let rid = job
        .rendition_id
        .clone()
        .ok_or_else(|| anyhow!("web job without rendition"))?;
    let r = ctx
        .store
        .rendition(&rid)
        .await?
        .ok_or_else(|| anyhow!("rendition vanished"))?;
    let from = ctx
        .store
        .rendition(r.derived_from.as_deref().unwrap_or(""))
        .await?
        .ok_or_else(|| anyhow!("source rendition missing"))?;
    if !ctx.store.served_versions().await?.contains(&from.id) {
        ctx.store.delete_rendition(&rid).await?;
        tracing::info!(clip = %r.clip_id, version = %from.id, "web copy not made: no live link plays the version");
        return Ok(());
    }
    let mut told = false;
    while let Some(preview) = ctx
        .store
        .derived_rendition(&from.id, "embed", None)
        .await?
        .filter(|e| e.state == "queued" || e.state == "rendering")
    {
        if ctx.is_cancelled(&job.id).await {
            anyhow::bail!("cancelled while waiting for the preview video");
        }
        if !told {
            told = true;
            let _ = ctx
                .store
                .job_route(&job.id, "waiting for the preview video")
                .await;
            tracing::info!(job = %job.id, preview = %preview.id, "the web copy waits for the preview video");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let input = ctx.config().abs(
        from.path
            .as_deref()
            .ok_or_else(|| anyhow!("source has no file"))?,
    );
    ctx.wait_while_held(job).await?;
    let recipe: WebRecipe =
        serde_json::from_str(r.recipe.as_deref().unwrap_or("{}")).context("web recipe")?;
    let duration = from.duration.unwrap_or(0.0);
    let src = crate::discord::source_info(ctx, &from).await?;
    let plan = web_plan(&src, &ctx.config().media, recipe.mbit, recipe.max_height);
    ctx.store.rendition_state(&rid, "rendering", None).await?;
    tokio::fs::create_dir_all(ctx.config().web_cache_dir()).await?;
    let name = format!("{}-{}.mp4", from.id, rid);
    let out = ctx.config().web_cache_dir().join(&name);
    let enc = ctx
        .encoder_for(from.vcodec.as_deref(), (src.width, src.height), src.fps)
        .await;
    encode::run_ladder(
        ctx,
        job,
        &enc,
        |e| args::embed(&plan, e, &input),
        &out,
        duration,
    )
    .await
    .map_err(|e| anyhow!("web copy encode failed: {e}"))?;
    let probe = ctx
        .ff
        .probe(&out)
        .await
        .map_err(|e| anyhow!("the web copy does not probe: {e}"))?;
    ctx.store
        .rendition_ready(&rid, &format!("cache/web/{name}"), &probe, false)
        .await?;
    ctx.store.rendition_reason(&rid, &plan.reason).await?;
    tracing::info!(clip = %r.clip_id, version = %from.id, size = probe.size, "web copy made");
    Ok(())
}
