use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::State;
use axum::{Extension, Json};
use reel_pipeline::enqueue::{create_clip_for_file, NewClip};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Deserialize)]
pub struct ScanBody {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub mode: String,
}

/// Queues the folder's clip files (not subfolders, never reel's own) through the ingest: copied,
/// skipping one already imported from that path, or taken with `mode = move`.
pub async fn scan(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<ScanBody>,
) -> AppResult<Json<Value>> {
    crate::error::library_ready(&st.ctx).await?;
    let cfg = st.ctx.config();
    let given = body.path.trim();
    let moving = body.mode == "move";
    if given.is_empty() {
        return Err(AppError::BadRequest("a folder to scan".into()));
    }
    let dir = PathBuf::from(given);
    if !dir.is_dir() {
        return Err(AppError::BadRequest(format!("{given}: no such folder")));
    }
    let dir = dir
        .canonicalize()
        .map_err(|e| AppError::BadRequest(format!("{given}: {e}")))?;
    for (own, what) in [
        (
            cfg.ingest.incoming_dir.clone(),
            "the incoming folder, which is watched already",
        ),
        (cfg.ingest.originals_dir.clone(), "the originals folder"),
        (cfg.data_dir.clone(), "reel's data folder"),
        (cfg.library_dir(), "reel's library folder"),
    ] {
        if own
            .canonicalize()
            .map(|o| dir.starts_with(&o))
            .unwrap_or(false)
        {
            return Err(AppError::BadRequest(format!("that is {what}")));
        }
    }
    let mut rd = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| AppError::BadRequest(format!("{given}: {e}")))?;
    let (mut queued, mut skipped, mut already) = (0usize, 0usize, 0usize);
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?
    {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if !path.is_file()
            || reel_pipeline::watcher::skip_name(&name)
            || !cfg.accepts_extension(&path)
        {
            skipped += 1;
            continue;
        }
        let key = path.to_string_lossy().into_owned();
        if st.ctx.store.imported_already(&key).await? {
            already += 1;
            continue;
        }
        let input = if moving {
            path.clone()
        } else {
            let into = cfg.uploads_dir().join(reel_core::ids::new_id());
            tokio::fs::create_dir_all(&into).await?;
            let copy = into.join(&name);
            if let Err(e) = tokio::fs::copy(&path, &copy).await {
                tracing::warn!(file = %path.display(), error = %e, "folder scan copy");
                skipped += 1;
                continue;
            }
            copy
        };
        let mut new = NewClip::new(&input, "imported");
        new.owner_id = Some(me.id.clone());
        match create_clip_for_file(&st.ctx, new).await {
            Ok((clip, _)) => {
                st.ctx.store.record_import(&key, &clip.id).await?;
                queued += 1;
            }
            Err(e) => {
                tracing::warn!(file = %path.display(), error = %e, "folder scan");
                skipped += 1;
            }
        }
    }
    tracing::info!(folder = %dir.display(), queued, skipped, already, moving, by = %me.name, "folder scanned");
    Ok(Json(json!({
        "ok": true,
        "queued": queued,
        "skipped": skipped,
        "already": already,
        "line": format!(
            "{queued} file{} queued from {}{}, {already} imported before and skipped, {skipped} skipped; the clips show in Jobs and the library as they land",
            if queued == 1 { "" } else { "s" },
            dir.display(),
            if moving { ", moved into reel" } else { ", the folder left as it was" }
        ),
    })))
}
