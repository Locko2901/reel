use crate::{encode, worker::progress_sink, Ctx};
use anyhow::{anyhow, Context};
use reel_core::plan::{copy_plan, SourceInfo};
use reel_core::recipe::DiscordRecipe;
use reel_media::args;
use reel_store::models::*;

/// What the copy is made from, probed so the audio rate is known.
pub async fn source_info(ctx: &Ctx, from: &Rendition) -> anyhow::Result<SourceInfo> {
    let input = ctx.config().abs(
        from.path
            .as_deref()
            .ok_or_else(|| anyhow!("source has no file"))?,
    );
    let probe = ctx
        .ff
        .probe(&input)
        .await
        .map_err(|e| anyhow!("the source does not probe: {e}"))?;
    Ok(SourceInfo {
        width: probe.width,
        height: probe.height,
        fps: probe.fps,
        acodec: probe.acodec.clone(),
        abitrate: probe.abitrate,
        vbitrate: probe.vbitrate,
    })
}

pub async fn run(ctx: &Ctx, job: &Job) -> anyhow::Result<()> {
    let rid = job
        .rendition_id
        .clone()
        .ok_or_else(|| anyhow!("discord job without rendition"))?;
    let r = ctx
        .store
        .rendition(&rid)
        .await?
        .ok_or_else(|| anyhow!("rendition vanished"))?;
    let recipe: DiscordRecipe =
        serde_json::from_str(r.recipe.as_deref().unwrap_or("{}")).context("discord recipe")?;
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
    let src = source_info(ctx, &from).await?;
    let mut plan = copy_plan(
        recipe.limit_bytes,
        duration,
        &src,
        &ctx.config().media,
        1440,
        reel_core::plan::Purpose::Limited,
    )
    .map_err(|e| anyhow!(e))?;
    ctx.store.rendition_state(&rid, "rendering", None).await?;
    if ctx.config().media.copies_on_hardware
        && ctx.backend().await != reel_media::HwBackend::Software
    {
        return hardware_copy(ctx, job, &rid, &from, plan, &src, &input, duration, &recipe).await;
    }
    let _ = ctx
        .store
        .job_route(
            &job.id,
            &format!(
                "libx264 two-pass {}, {}p{}",
                plan.preset,
                plan.height,
                plan.fps.round()
            ),
        )
        .await;
    tokio::fs::create_dir_all(ctx.config().cache_dir()).await?;
    let name = format!("{}-{}.mp4", from.id, recipe.limit_bytes / 1_000_000);
    let out = ctx.config().cache_dir().join(&name);
    let logdir = tempfile_dir(&ctx.config().cache_dir(), &rid).await?;
    let passlog = logdir.join("pass");
    let cancel = ctx.cancel_token(&job.id).await;
    let result = async {
        for attempt in 0..3 {
            let p1 = args::copy_pass(1, &plan, &input, &passlog);
            let mut first = progress_sink(ctx, job);
            ctx.ff
                .run_with(
                    &p1,
                    duration,
                    Some(Box::new(move |p| first(p * 0.5))),
                    cancel.as_ref(),
                )
                .await
                .map_err(|e| anyhow!("pass 1 failed: {e}"))?;
            let p2 = args::copy_pass(2, &plan, &input, &passlog);
            let mut second = progress_sink(ctx, job);
            ctx.ff
                .run_to_with(
                    &p2,
                    &out,
                    duration,
                    Some(Box::new(move |p| second(0.5 + p * 0.5))),
                    cancel.as_ref(),
                )
                .await
                .map_err(|e| anyhow!("pass 2 failed: {e}"))?;
            let size = tokio::fs::metadata(&out).await?.len();
            if size <= recipe.limit_bytes {
                return Ok(());
            }
            if attempt == 2 {
                return Err(anyhow!(
                    "still {} after three tries",
                    reel_core::format::size(size)
                ));
            }
            plan.vbit = (plan.vbit as f64 * 0.9) as u64;
        }
        Ok(())
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&logdir).await;
    result?;
    let probe = ctx
        .ff
        .probe(&out)
        .await
        .map_err(|e| anyhow!("the copy does not probe: {e}"))?;
    ctx.store
        .rendition_ready(&rid, &format!("cache/discord/{name}"), &probe, false)
        .await?;
    ctx.store.rendition_reason(&rid, &plan.reason).await?;
    Ok(())
}

/// One pass on the GPU encoder; the bitrate drops by 15 % per try while the file lands over
/// the limit, three tries.
#[allow(clippy::too_many_arguments)]
async fn hardware_copy(
    ctx: &Ctx,
    job: &Job,
    rid: &str,
    from: &Rendition,
    mut plan: reel_core::plan::CopyPlan,
    src: &SourceInfo,
    input: &std::path::Path,
    duration: f64,
    recipe: &DiscordRecipe,
) -> anyhow::Result<()> {
    let enc = ctx
        .encoder_for(from.vcodec.as_deref(), (src.width, src.height), src.fps)
        .await;
    let _ = ctx
        .store
        .job_route(
            &job.id,
            &format!(
                "{} one pass, {}p{}",
                enc.label(),
                plan.height,
                plan.fps.round()
            ),
        )
        .await;
    tokio::fs::create_dir_all(ctx.config().cache_dir()).await?;
    let name = format!("{}-{}.mp4", from.id, recipe.limit_bytes / 1_000_000);
    let out = ctx.config().cache_dir().join(&name);
    for attempt in 0..3 {
        let p = plan.clone();
        encode::run_ladder(
            ctx,
            job,
            &enc,
            |e| args::embed(&p, e, input),
            &out,
            duration,
        )
        .await
        .map_err(|e| anyhow!("copy encode failed: {e}"))?;
        let size = tokio::fs::metadata(&out).await?.len();
        if size <= recipe.limit_bytes {
            break;
        }
        if attempt == 2 {
            return Err(anyhow!(
                "still {} after three tries",
                reel_core::format::size(size)
            ));
        }
        plan.vbit = (plan.vbit as f64 * 0.85) as u64;
    }
    let probe = ctx
        .ff
        .probe(&out)
        .await
        .map_err(|e| anyhow!("the copy does not probe: {e}"))?;
    ctx.store
        .rendition_ready(rid, &format!("cache/discord/{name}"), &probe, false)
        .await?;
    ctx.store
        .rendition_reason(
            rid,
            &format!("{} (one pass on {})", plan.reason, enc.label()),
        )
        .await?;
    Ok(())
}

async fn tempfile_dir(base: &std::path::Path, id: &str) -> anyhow::Result<std::path::PathBuf> {
    let d = base.join(format!(".pass-{id}"));
    tokio::fs::create_dir_all(&d).await?;
    Ok(d)
}
