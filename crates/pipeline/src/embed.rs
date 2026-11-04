use crate::{encode, Ctx};
use anyhow::anyhow;
use reel_core::plan::copy_plan;
use reel_media::args;
use reel_store::models::*;

pub async fn run(ctx: &Ctx, job: &Job) -> anyhow::Result<()> {
    let rid = job
        .rendition_id
        .clone()
        .ok_or_else(|| anyhow!("embed job without rendition"))?;
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
    let input = ctx.config().abs(
        from.path
            .as_deref()
            .ok_or_else(|| anyhow!("source has no file"))?,
    );
    ctx.wait_while_held(job).await?;
    let duration = from.duration.unwrap_or(0.0);
    let src = crate::discord::source_info(ctx, &from).await?;
    let limit = embed_limit(ctx, &r);
    let mut plan = copy_plan(
        limit,
        duration,
        &src,
        &ctx.config().media,
        1080,
        reel_core::plan::Purpose::Embed,
    )
    .map_err(|e| anyhow!(e))?;
    ctx.store.rendition_state(&rid, "rendering", None).await?;
    let name = file_name(&r.kind, &from.id);
    let out = ctx.config().clip_dir(&r.clip_id).join(&name);
    let enc = ctx
        .encoder_for(from.vcodec.as_deref(), (src.width, src.height), src.fps)
        .await;
    for attempt in 0..3 {
        let p = plan.clone();
        encode::run_ladder(
            ctx,
            job,
            &enc,
            |e| args::embed(&p, e, &input),
            &out,
            duration,
        )
        .await
        .map_err(|e| anyhow!("embed encode failed: {e}"))?;
        let size = tokio::fs::metadata(&out).await?.len();
        if size <= limit || attempt == 2 {
            break;
        }
        plan.vbit = (plan.vbit as f64 * 0.85) as u64;
    }
    let probe = ctx
        .ff
        .probe(&out)
        .await
        .map_err(|e| anyhow!("the preview video does not probe: {e}"))?;
    let rel = format!("library/{}/{name}", r.clip_id);
    ctx.store.rendition_ready(&rid, &rel, &probe, false).await?;
    ctx.store.rendition_reason(&rid, &plan.reason).await?;
    Ok(())
}

/// The size a preview copy must land under: its recipe's, else the server's.
fn embed_limit(ctx: &Ctx, r: &Rendition) -> u64 {
    r.recipe
        .as_deref()
        .and_then(|x| serde_json::from_str::<reel_core::recipe::EmbedRecipe>(x).ok())
        .map(|x| x.size_mb * 1_000_000)
        .filter(|b| *b > 0)
        .unwrap_or_else(|| ctx.config().embed_bytes())
}

/// The server's preview and the storage's of one version lie side by side.
fn file_name(kind: &str, from: &str) -> String {
    if kind == "embed_ext" {
        format!("embed-ext-{from}.mp4")
    } else {
        format!("embed-{from}.mp4")
    }
}
