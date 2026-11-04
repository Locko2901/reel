use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

/// 503 while the library is away, so a container's health check says so.
pub async fn healthz(State(st): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    let (queued, running) = st.ctx.store.job_counts().await.unwrap_or((0, 0));
    let (clips, _) = st.ctx.store.clip_count_and_size().await.unwrap_or((0, 0));
    let hw = st.ctx.hw.read().await;
    let db_ok = st.ctx.store.settings().await.is_ok();
    let incoming = st.ctx.config().ingest.incoming_dir.is_dir();
    let ffmpeg = hw.ffmpeg_version.is_some();
    let away = st.ctx.library.away();
    (
        if away.is_some() {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::OK
        },
        Json(json!({
            "ok": db_ok && ffmpeg && away.is_none(),
            "version": reel_core::VERSION,
            "uptime_s": jiff::Timestamp::now().as_second() - st.started_at.as_second(),
            "db": db_ok,
            "ffmpeg": ffmpeg,
            "hwaccel": hw.backend.name(),
            "hw_decode": hw.to_json()["decoders"],
            "incoming_readable": incoming,
            "library": away.unwrap_or_else(|| "ok".into()),
            "jobs": { "queued": queued, "running": running },
            "clips": clips,
        })),
    )
}
