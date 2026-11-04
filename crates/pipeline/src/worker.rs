use crate::Ctx;
use reel_core::Event;
use reel_media::Progress;
use reel_store::models::Job;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// `lane`: the worker takes only the jobs someone waits for.
pub async fn run_worker(ctx: Ctx, idx: usize, lane: bool) {
    tracing::info!(worker = idx, lane, "worker up");
    let up_to = if lane {
        reel_store::models::PRIORITY_LANE
    } else {
        i64::MAX
    };
    loop {
        if ctx.shutdown.is_cancelled() {
            break;
        }
        if !ctx.library.ready() {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => {},
                _ = ctx.shutdown.cancelled() => break,
            }
            continue;
        }
        let job = match ctx.store.claim_job_up_to(up_to).await {
            Ok(Some(j)) => j,
            Ok(None) => {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(1000)) => {},
                    _ = ctx.shutdown.cancelled() => break,
                }
                continue;
            }
            Err(e) => {
                tracing::error!(error = %e, "claim job");
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        };
        tracing::info!(job = %job.id, kind = %job.kind, clip = ?job.clip_id, "job start");
        emit_job(&ctx, &job, "running", 0.0, None);
        let cancel = CancellationToken::new();
        ctx.cancels
            .lock()
            .await
            .insert(job.id.clone(), cancel.clone());
        let result = tokio::select! {
            r = run_in_its_lane(&ctx, &job) => r,
            _ = ctx.shutdown.cancelled() => Err("shut down while running".to_string()),
        };
        let cancelled = cancel.is_cancelled() && result.is_err();
        ctx.cancels.lock().await.remove(&job.id);
        match result {
            Err(_) if cancelled => {
                crate::jobs::after_cancel(&ctx, &job).await;
                emit_job(&ctx, &job, "cancelled", 0.0, None);
                tracing::info!(job = %job.id, kind = %job.kind, "job cancelled");
            }
            Ok(()) => {
                let _ = ctx.store.job_done(&job.id).await;
                emit_job(&ctx, &job, "done", 1.0, None);
                tracing::info!(job = %job.id, kind = %job.kind, "job done");
            }
            Err(msg) => {
                let _ = ctx.store.job_failed(&job.id, &msg).await;
                if let Some(rid) = &job.rendition_id {
                    let _ = ctx.store.rendition_state(rid, "failed", Some(&msg)).await;
                }
                emit_job(&ctx, &job, "failed", 0.0, Some(msg.clone()));
                tracing::warn!(job = %job.id, kind = %job.kind, error = %msg, "job failed");
            }
        }
    }
    tracing::info!(worker = idx, "worker down");
}

/// A job someone waits for pauses the background jobs' ffmpeg while it runs; a background
/// job's ffmpeg is told to the lane, which pauses it then.
async fn run_in_its_lane(ctx: &Ctx, job: &Job) -> Result<(), String> {
    if crate::lane::in_lane(job) {
        let _running = ctx.lane.enter();
        run_job(ctx, job).await
    } else {
        let told: reel_media::ffmpeg::OnSpawn = ctx.lane.clone();
        reel_media::ffmpeg::ON_SPAWN
            .scope(told, run_job(ctx, job))
            .await
    }
}

async fn run_job(ctx: &Ctx, job: &Job) -> Result<(), String> {
    let r = match job.kind.as_str() {
        "ingest" => crate::ingest::run(ctx, job).await,
        "cut" => crate::cut::run(ctx, job).await,
        "embed" => crate::embed::run(ctx, job).await,
        "web" => crate::web::run(ctx, job).await,
        "discord" => crate::discord::run(ctx, job).await,
        "anim" => crate::anim::run(ctx, job).await,
        "montage" => crate::montage::run(ctx, job).await,
        "derive" => crate::derive::run(ctx, job).await,
        other => Err(anyhow::anyhow!("unknown job kind {other}")),
    };
    r.map_err(|e| format!("{e:#}"))
}

pub fn emit_job(ctx: &Ctx, job: &Job, state: &str, progress: f64, error: Option<String>) {
    ctx.emit(Event::Job {
        id: job.id.clone(),
        kind: job.kind.clone(),
        clip_id: job.clip_id.clone(),
        rendition_id: job.rendition_id.clone(),
        state: state.into(),
        progress,
        error,
    });
}

/// A progress callback that writes to the store and the bus at most twice a second.
pub fn progress_sink(ctx: &Ctx, job: &Job) -> Progress {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<f64>();
    let c = ctx.clone();
    let j = job.clone();
    tokio::spawn(async move {
        let mut last = std::time::Instant::now() - Duration::from_secs(1);
        while let Some(p) = rx.recv().await {
            if last.elapsed() < Duration::from_millis(500) {
                continue;
            }
            last = std::time::Instant::now();
            let _ = c.store.job_progress(&j.id, p).await;
            emit_job(&c, &j, "running", p, None);
        }
    });
    Box::new(move |p| {
        let _ = tx.send(p);
    })
}
