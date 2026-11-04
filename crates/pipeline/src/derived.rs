use crate::Ctx;
use anyhow::Context;
use reel_media::args;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

pub struct DerivedPaths {
    pub poster: PathBuf,
    pub hover: PathBuf,
    pub strip: PathBuf,
    pub tint: PathBuf,
}

pub fn paths(video: &Path) -> DerivedPaths {
    let stem = video.with_extension("");
    let s = stem.to_string_lossy();
    DerivedPaths {
        poster: PathBuf::from(format!("{s}.jpg")),
        hover: PathBuf::from(format!("{s}.loop.mp4")),
        strip: PathBuf::from(format!("{s}.strip.jpg")),
        tint: PathBuf::from(format!("{s}.tint")),
    }
}

pub fn default_poster_time(duration: f64) -> f64 {
    (duration * 0.25).clamp(0.0, (duration - 0.1).max(0.0))
}

/// The first frame above black within three seconds of the default time, else the frame at it.
/// The clip keeps the default time, so a remake knows this poster was not picked by hand.
pub async fn poster_default(ctx: &Ctx, video: &Path, duration: f64) -> anyhow::Result<f64> {
    let at = default_poster_time(duration);
    let d = paths(video);
    let window = (duration - at).clamp(0.2, 3.0);
    match ctx
        .ff
        .run_to(
            &args::poster_bright(video, at, window),
            &d.poster,
            0.0,
            None,
        )
        .await
    {
        Ok(()) => {
            tint(ctx, video).await;
            Ok(at)
        }
        Err(e) => {
            tracing::debug!(error = %e, "no bright frame near the default time; the plain frame");
            poster(ctx, video, at).await?;
            Ok(at)
        }
    }
}

pub async fn poster(ctx: &Ctx, video: &Path, at: f64) -> anyhow::Result<()> {
    let d = paths(video);
    ctx.ff
        .run_to(&args::poster(video, at), &d.poster, 0.0, None)
        .await
        .context("poster")?;
    tint(ctx, video).await;
    Ok(())
}

pub async fn tint(ctx: &Ctx, video: &Path) {
    let d = paths(video);
    match ctx.ff.average_color(&d.poster).await {
        Ok([r, g, b]) => {
            let _ = tokio::fs::write(&d.tint, format!("{r},{g},{b}\n")).await;
        }
        Err(e) => tracing::warn!(error = %e, "poster tint skipped"),
    }
}

pub async fn read_tint(video: &Path) -> Option<[u8; 3]> {
    let text = tokio::fs::read_to_string(paths(video).tint).await.ok()?;
    let mut it = text.trim().split(',').map(|v| v.parse::<u8>().ok());
    Some([it.next()??, it.next()??, it.next()??])
}

pub async fn make_all(
    ctx: &Ctx,
    video: &Path,
    poster_time: f64,
    duration: f64,
    cancel: Option<&CancellationToken>,
) -> anyhow::Result<()> {
    if (poster_time - default_poster_time(duration)).abs() < 0.001 {
        poster_default(ctx, video, duration).await?;
    } else {
        poster(ctx, video, poster_time).await?;
    }
    loop_and_strip(ctx, video, poster_time, duration, cancel).await
}

/// The slow pair, made after the clip is published. A failed one is skipped with a warning;
/// a cancelled job stops here with an error.
pub async fn loop_and_strip(
    ctx: &Ctx,
    video: &Path,
    poster_time: f64,
    duration: f64,
    cancel: Option<&CancellationToken>,
) -> anyhow::Result<()> {
    let d = paths(video);
    let loop_len = 4.0_f64.min(duration.max(0.5));
    let loop_start = if poster_time + loop_len <= duration {
        poster_time
    } else {
        (duration - loop_len).max(0.0)
    };
    let stopped = || cancel.map(|c| c.is_cancelled()).unwrap_or(false);
    if let Err(e) = ctx
        .ff
        .run_to_with(
            &args::hover_loop(video, loop_start, loop_len),
            &d.hover,
            loop_len,
            None,
            cancel,
        )
        .await
    {
        if stopped() {
            return Err(anyhow::anyhow!("cancelled"));
        }
        tracing::warn!(error = %e, "hover loop skipped");
    }
    if let Err(e) = ctx
        .ff
        .run_to_with(
            &args::filmstrip(video, duration, 12),
            &d.strip,
            0.0,
            None,
            cancel,
        )
        .await
    {
        if stopped() {
            return Err(anyhow::anyhow!("cancelled"));
        }
        tracing::warn!(error = %e, "filmstrip skipped");
    }
    Ok(())
}
