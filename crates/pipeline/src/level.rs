use crate::Ctx;
use anyhow::anyhow;
use reel_media::Level;
use reel_store::models::Rendition;

pub async fn measure(ctx: &Ctx, r: &Rendition) -> anyhow::Result<Level> {
    let path = ctx.config().abs(
        r.path
            .as_deref()
            .ok_or_else(|| anyhow!("the version has no file"))?,
    );
    let level = if r.acodec.is_some() {
        ctx.ff.audio_level(&path).await?
    } else {
        Level::default()
    };
    ctx.store
        .rendition_audio_level(&r.id, level.loudness, level.peak)
        .await?;
    Ok(level)
}

pub async fn of(ctx: &Ctx, r: &Rendition) -> anyhow::Result<Level> {
    if r.audio_measured_at.is_some() {
        return Ok(Level {
            loudness: r.audio_loudness,
            peak: r.audio_peak,
        });
    }
    measure(ctx, r).await
}

/// The editor measures again when this fails.
pub async fn measure_quietly(ctx: &Ctx, r: &Rendition) {
    if let Err(e) = measure(ctx, r).await {
        tracing::warn!(rendition = %r.id, error = %e, "audio level not measured");
    }
}
