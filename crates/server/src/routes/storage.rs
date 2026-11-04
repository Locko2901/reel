use crate::auth::User;
use crate::error::AppResult;
use crate::AppState;
use axum::extract::{Query, State};
use axum::{Extension, Json};
use reel_core::format;
use serde_json::{json, Value};

#[derive(serde::Deserialize)]
pub struct CapAsked {
    pub gb: Option<u64>,
}

/// What the library cap would move to the trash at this size, for Settings' line as it is typed.
pub async fn cap_preview(
    State(st): State<AppState>,
    Query(q): Query<CapAsked>,
) -> AppResult<Json<Value>> {
    let cfg = st.ctx.config();
    let gb = q.gb.unwrap_or(cfg.storage.cap_gb).min(100_000);
    let p = reel_pipeline::cap::preview(&st.ctx, gb * 1_000_000_000).await?;
    Ok(Json(json!({
        "line": reel_pipeline::cap::line(&p, cfg.share.trash_days),
        "holds": p.holds.bytes,
        "moves": p.moved.clips,
        "moves_bytes": p.moved.bytes,
        "left": p.left,
    })))
}

/// Every embed copy made under other rules than the current ones is queued again.
pub async fn remake_embeds(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    let r = reel_pipeline::enqueue::remake_embeds(&st.ctx).await?;
    tracing::info!(queued = r.queued, missing = r.missing, stale = r.stale, by = %me.name, "preview videos queued");
    Ok(Json(json!({
        "ok": true,
        "queued": r.queued,
        "missing": r.missing,
        "stale": r.stale,
        "line": if r.queued == 0 {
            "every version that needs a preview video has one under the current size and rules".to_string()
        } else {
            format!(
                "{} preview video{} queued: {} missing under the current size, {} made for another size or rule; they show in Jobs, and a link serves the version itself or the old one until the new one is ready",
                r.queued, if r.queued == 1 { "" } else { "s" }, r.missing, r.stale
            )
        },
    })))
}

pub async fn purge_trash(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    let purged = reel_pipeline::housekeeping::purge_trash(&st.ctx, None).await?;
    tracing::info!(purged, by = %me.name, "trash purged by hand");
    let _ = st.disk.get(&st.ctx.effective(), true).await;
    Ok(Json(json!({
        "ok": true,
        "purged": purged,
        "line": match purged {
            0 => "the trash was empty".to_string(),
            1 => "1 clip purged; its Undo is over".to_string(),
            n => format!("{n} clips purged; their Undo is over"),
        },
    })))
}

pub async fn clear_discord(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    let (gone, freed) = reel_pipeline::housekeeping::trim_discord_copies(&st.ctx, 0).await?;
    tracing::info!(gone, freed, by = %me.name, "copies under a size cleared");
    let _ = st.disk.get(&st.ctx.effective(), true).await;
    Ok(Json(json!({
        "ok": true,
        "gone": gone,
        "line": if gone == 0 {
            "there were no copies".to_string()
        } else {
            format!("{gone} cop{} cleared, {} freed; they are made again on demand", if gone == 1 { "y" } else { "ies" }, format::size(freed))
        },
    })))
}

pub async fn clear_web(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    let (gone, freed) = reel_pipeline::housekeeping::sweep_web_copies(&st.ctx, true).await?;
    tracing::info!(gone, freed, by = %me.name, "web copies cleared");
    let _ = st.disk.get(&st.ctx.effective(), true).await;
    Ok(Json(json!({
        "ok": true,
        "gone": gone,
        "line": if gone == 0 {
            "there were no web copies".to_string()
        } else {
            format!("{gone} web cop{} cleared, {} freed; each is made again on its page's next view", if gone == 1 { "y" } else { "ies" }, format::size(freed))
        },
    })))
}

pub async fn rebuild_derived(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    let queued = reel_pipeline::enqueue::rebuild_derived(&st.ctx).await?;
    tracing::info!(queued, by = %me.name, "posters and loops queued again");
    Ok(Json(json!({
        "ok": true,
        "queued": queued,
        "line": format!("{queued} job{} queued at the lowest priority; they show in Jobs", if queued == 1 { "" } else { "s" }),
    })))
}

pub async fn remake_posters(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    let queued = reel_pipeline::enqueue::remake_default_posters(&st.ctx).await?;
    tracing::info!(queued, by = %me.name, "default posters queued again");
    Ok(Json(json!({
        "ok": true,
        "queued": queued,
        "line": format!("{queued} job{} queued at the lowest priority; a poster you picked stays", if queued == 1 { "" } else { "s" }),
    })))
}

pub async fn disk_refresh(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let info = st.disk.get(&st.ctx.effective(), true).await;
    let clips = st.ctx.store.library_bytes().await? as u64;
    Ok(Json(json!({ "ok": true, "line": info.detail(clips) })))
}
