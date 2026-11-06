use crate::accounts::Backend;
use crate::AppState;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use axum_login::AuthSession;
use serde_json::json;
use std::net::SocketAddr;

/// 503 while the library is away, so a container's health check says so. The details (version,
/// hardware, jobs, clips) go to an admin and to a request straight from this machine; anyone
/// else gets whether reel is ok, and the status.
pub async fn healthz(
    State(st): State<AppState>,
    req: Request,
) -> (StatusCode, Json<serde_json::Value>) {
    let asker = Asker::of(&req, &st);
    drop(req);
    let (queued, running) = st.ctx.store.job_counts().await.unwrap_or((0, 0));
    let hw = st.ctx.hw.read().await;
    let db_ok = st.ctx.store.settings().await.is_ok();
    let ffmpeg = hw.ffmpeg_version.is_some();
    let away = st.ctx.library.away();
    let status = if away.is_some() {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    let ok = db_ok && ffmpeg && away.is_none();
    if !asker.gets_details(&st).await {
        return (status, Json(json!({ "ok": ok })));
    }
    let (clips, _) = st.ctx.store.clip_count_and_size().await.unwrap_or((0, 0));
    let incoming = st.ctx.config().ingest.incoming_dir.is_dir();
    (
        status,
        Json(json!({
            "ok": ok,
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

/// Who asks, as far as the details go: read off the request before anything waits.
struct Asker {
    /// Through `/api/health`, behind the sign-in.
    user_admin: Option<bool>,
    /// A loopback peer that no proxy forwarded, as a proxy on the same machine would; for the
    /// app's standalone library, which every program of the PC reaches so, the app with its key.
    this_machine: bool,
    session_user: Option<String>,
}

impl Asker {
    fn of(req: &Request, st: &AppState) -> Asker {
        let peer = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip());
        Asker {
            user_admin: req.extensions().get::<crate::auth::User>().map(|u| u.admin),
            this_machine: peer.is_some_and(|ip| ip.is_loopback())
                && !forwarded(req.headers())
                && st
                    .app_key
                    .as_ref()
                    .is_none_or(|k| k.shown_by(req.headers())),
            session_user: req
                .extensions()
                .get::<AuthSession<Backend>>()
                .and_then(|a| a.user.as_ref().map(|u| u.id.clone())),
        }
    }

    /// An admin (signed in, or through `/api/health`), or the machine itself.
    async fn gets_details(&self, st: &AppState) -> bool {
        if let Some(admin) = self.user_admin {
            return admin;
        }
        if self.this_machine {
            return true;
        }
        let Some(id) = &self.session_user else {
            return false;
        };
        matches!(st.ctx.store.user(id).await, Ok(Some(u)) if u.is_admin() && u.disabled_at.is_none())
    }
}

fn forwarded(h: &HeaderMap) -> bool {
    ["x-forwarded-for", "forwarded", "x-real-ip"]
        .iter()
        .any(|k| h.contains_key(*k))
}
