use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::State;
use axum::response::{IntoResponse, Redirect, Response};
use reel_store::Sort;

pub async fn clip(State(st): State<AppState>) -> AppResult<Response> {
    let rows = st.ctx.store.clips("", Sort::Newest).await?;
    let pick = rows
        .iter()
        .find(|r| r.shares > 0 && r.clip.state == "ready")
        .or_else(|| rows.iter().find(|r| r.clip.state == "ready"))
        .ok_or(AppError::NotFound)?;
    Ok(Redirect::to(&format!("/clips/{}", pick.clip.id)).into_response())
}

pub async fn share(State(st): State<AppState>) -> AppResult<Response> {
    let shares = st.ctx.store.shares().await?;
    let pick = shares
        .iter()
        .find(|s| s.share.is_active() && s.clip_deleted_at.is_none())
        .ok_or(AppError::NotFound)?;
    Ok(Redirect::to(&format!("/s/{}", pick.share.token)).into_response())
}

pub async fn collection(State(st): State<AppState>) -> AppResult<Response> {
    let sets = st.ctx.store.collections().await?;
    let pick = sets.first().ok_or(AppError::NotFound)?;
    Ok(Redirect::to(&format!("/collections/{}", pick.collection.id)).into_response())
}

/// The montage page of the collection with the most clips.
pub async fn montage(State(st): State<AppState>) -> AppResult<Response> {
    let sets = st.ctx.store.collections().await?;
    let pick = sets
        .iter()
        .max_by_key(|c| c.clips)
        .ok_or(AppError::NotFound)?;
    Ok(Redirect::to(&format!("/montage?set={}", pick.collection.id)).into_response())
}

pub async fn collection_link(State(st): State<AppState>) -> AppResult<Response> {
    let sets = st.ctx.store.collections().await?;
    let token = sets
        .iter()
        .find_map(|c| c.token.clone())
        .ok_or(AppError::NotFound)?;
    Ok(Redirect::to(&format!("/c/{token}")).into_response())
}

pub async fn clip_tracks(State(st): State<AppState>) -> AppResult<Response> {
    let rows = st.ctx.store.clips("", Sort::Newest).await?;
    let pick = rows
        .iter()
        .find(|r| {
            r.clip.state == "ready"
                && r.clip
                    .original_probe
                    .as_deref()
                    .and_then(|p| serde_json::from_str::<reel_core::ProbeInfo>(p).ok())
                    .is_some_and(|p| p.audio_streams > 1)
        })
        .ok_or(AppError::NotFound)?;
    Ok(Redirect::to(&format!("/clips/{}", pick.clip.id)).into_response())
}
