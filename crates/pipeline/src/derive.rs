use crate::{derived, Ctx};
use anyhow::anyhow;
use reel_store::models::Job;
use serde::Deserialize;

/// `poster: false`: the poster is there already, only the loop and the filmstrip are made.
/// `whole`: the ingest published a cut first and left the original's poster and loudness for here.
#[derive(Deserialize)]
struct Payload {
    #[serde(default = "yes")]
    poster: bool,
    #[serde(default)]
    whole: bool,
}

fn yes() -> bool {
    true
}

/// Makes the poster, the loop, the filmstrip and the tint of every ready version of the clip
/// again, at the poster time the clip keeps for its current version.
pub async fn run(ctx: &Ctx, job: &Job) -> anyhow::Result<()> {
    let payload: Payload = serde_json::from_str(&job.payload).unwrap_or(Payload {
        poster: true,
        whole: false,
    });
    let clip_id = job
        .clip_id
        .clone()
        .ok_or_else(|| anyhow!("derive job without clip"))?;
    let clip = ctx
        .store
        .clip(&clip_id)
        .await?
        .ok_or_else(|| anyhow!("clip vanished"))?;
    if clip.deleted_at.is_some() {
        return Ok(());
    }
    ctx.wait_while_held(job).await?;
    let cancel = ctx.cancel_token(&job.id).await;
    for r in ctx.store.renditions_of(&clip_id).await? {
        if !(r.is_version() && r.is_ready()) {
            continue;
        }
        let Some(path) = r.path.as_deref() else {
            continue;
        };
        let duration = r.duration.unwrap_or(0.0);
        let at = if clip.current_id.as_deref() == Some(r.id.as_str()) {
            clip.poster_time
                .unwrap_or_else(|| derived::default_poster_time(duration))
        } else {
            derived::default_poster_time(duration)
        };
        let video = ctx.config().abs(path);
        if payload.whole && r.kind == "master" {
            derived::poster_default(ctx, &video, duration).await?;
            crate::level::measure_quietly(ctx, &r).await;
        }
        if payload.poster {
            derived::make_all(ctx, &video, at, duration, cancel.as_ref()).await?;
        } else {
            derived::loop_and_strip(ctx, &video, at, duration, cancel.as_ref()).await?;
        }
        if payload.poster || (payload.whole && r.kind == "master") {
            crate::poster_image::keep(ctx, &clip_id, &r.id).await;
        }
    }
    Ok(())
}
