//! The animated pictures made of a clip's versions, listed under Send on its page.
use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::AppState;
use askama::Template;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use reel_core::format;
use reel_core::recipe::AnimRecipe;
use reel_store::models::Rendition;
use serde_json::{json, Value};

pub struct AnimView {
    pub id: String,
    pub format: &'static str,
    /// "720 px, 24 fps, 2.1 MB, of cut 2, current"
    pub words: String,
    /// "waiting" or "making, 45%" while it is made; empty once it is.
    pub making: String,
    pub download: String,
}

#[derive(Template)]
#[template(path = "anim-made.html")]
pub struct AnimMade {
    pub anims: Vec<AnimView>,
    pub can_edit: bool,
}

/// The clip's pictures, newest first: the made ones whose file is there, and the ones being made.
pub async fn views(
    st: &AppState,
    current: Option<&str>,
    renditions: &[Rendition],
) -> AppResult<Vec<AnimView>> {
    let cfg = st.ctx.config();
    let mut out = Vec::new();
    for r in renditions.iter().rev().filter(|r| r.kind == "anim") {
        let Some(recipe) = r
            .recipe
            .as_deref()
            .and_then(|j| serde_json::from_str::<AnimRecipe>(j).ok())
        else {
            continue;
        };
        let Some(from) = renditions
            .iter()
            .find(|v| Some(v.id.as_str()) == r.derived_from.as_deref())
        else {
            continue;
        };
        let making = match r.state.as_str() {
            "ready" if r.path.as_deref().is_some_and(|p| cfg.abs(p).exists()) => String::new(),
            "queued" | "rendering" => match st.ctx.store.active_job_for_rendition(&r.id).await? {
                Some(j) if j.state == "running" && j.progress >= 0.01 => {
                    format!("making, {}%", (j.progress * 100.0).round())
                }
                Some(j) if j.state == "running" => j
                    .route
                    .filter(|w| w.starts_with("waiting"))
                    .unwrap_or_else(|| "making".into()),
                Some(_) => "waiting".into(),
                None => continue,
            },
            _ => continue,
        };
        let width = if making.is_empty() {
            r.width.unwrap_or(0) as u32
        } else {
            recipe
                .size(
                    from.width.unwrap_or(0) as u32,
                    from.height.unwrap_or(0) as u32,
                )
                .0
        };
        let mut words = format!("{width} px, {} fps", recipe.fps);
        if making.is_empty() {
            words.push_str(&format!(", {}", format::size(r.size.unwrap_or(0) as u64)));
        }
        words.push_str(&format!(", of {}", version_words(from)));
        let is_current = current == Some(from.id.as_str());
        if is_current {
            words.push_str(", current");
        }
        out.push(AnimView {
            id: r.id.clone(),
            format: recipe.format.name(),
            words,
            making,
            download: format!("/media/renditions/{}/download", r.id),
        });
    }
    Ok(out)
}

fn version_words(v: &Rendition) -> String {
    match v.kind.as_str() {
        "master" => "the original".into(),
        _ => v.label(),
    }
}

/// The list as the clip page draws it, and whether one is being made so the page asks again.
pub async fn list(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let clip = st
        .ctx
        .store
        .clip(&id)
        .await?
        .filter(|c| c.deleted_at.is_none())
        .ok_or(AppError::NotFound)?;
    let renditions = st.ctx.store.renditions_of(&clip.id).await?;
    let anims = views(&st, clip.current_id.as_deref(), &renditions).await?;
    let making = anims.iter().any(|a| !a.making.is_empty());
    let html = AnimMade {
        anims,
        can_edit: me.may_edit(clip.owner_id.as_deref()),
    }
    .render()
    .map_err(|e| anyhow::anyhow!("the list of animated pictures did not render: {e}"))?;
    Ok(Json(json!({ "html": html, "making": making })))
}
