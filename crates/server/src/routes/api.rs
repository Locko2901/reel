use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::views::{serves_text, share_view};
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use jiff::{Span, Timestamp};
use reel_core::CutRecipe;
use reel_pipeline::enqueue::{
    delete_cut as remove_cut, request_cut, restore_clip as restore, set_poster as poster,
    trash_clip, CutDelete, CutTarget,
};
use reel_store::models::Share;
use serde::Deserialize;
use serde_json::{json, Value};

/// The clip, when it exists and the caller may change it.
async fn editable(st: &AppState, me: &User, id: &str) -> AppResult<reel_store::models::Clip> {
    let clip = st.ctx.store.clip(id).await?.ok_or(AppError::NotFound)?;
    if !me.may_edit(clip.owner_id.as_deref()) {
        return Err(AppError::Forbidden(
            "only who uploaded this clip, or an admin, changes it".into(),
        ));
    }
    Ok(clip)
}

pub async fn clip_json(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let clip = st.ctx.store.clip(&id).await?.ok_or(AppError::NotFound)?;
    let starred = st.ctx.store.is_starred(&me.id, &id).await?;
    let renditions = st.ctx.store.renditions_of(&id).await?;
    let shares = st.ctx.store.shares_of(&id).await?;
    let jobs = st
        .ctx
        .store
        .active_jobs()
        .await?
        .into_iter()
        .filter(|j| j.clip_id.as_deref() == Some(&id))
        .collect::<Vec<_>>();
    let link = shares.iter().find(|s| s.state() == "active").map(|s| {
        format!(
            "{}/s/{}",
            st.ctx.config().public_url.trim_end_matches('/'),
            s.token
        )
    });
    Ok(Json(
        json!({ "clip": clip, "renditions": renditions, "shares": shares, "jobs": jobs, "link": link, "starred": starred }),
    ))
}

/// A star of one's own: no right on the clip is needed, and nobody else's stars change.
pub async fn star_clip(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    if !st.ctx.store.star(&me.id, &id).await? {
        return Err(AppError::NotFound);
    }
    Ok(Json(json!({ "starred": true })))
}

pub async fn unstar_clip(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    st.ctx.store.unstar(&me.id, &id).await?;
    Ok(Json(json!({ "starred": false })))
}

#[derive(Deserialize)]
pub struct PatchClip {
    pub title: Option<String>,
    pub description: Option<String>,
}

pub async fn patch_clip(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<PatchClip>,
) -> AppResult<Json<Value>> {
    editable(&st, &me, &id).await?;
    let title = body
        .title
        .as_deref()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty());
    if body.title.is_some() && title.is_none() {
        return Err(AppError::BadRequest("a title cannot be empty".into()));
    }
    st.ctx
        .store
        .update_clip_text(&id, title, body.description.as_deref().map(|d| d.trim()))
        .await?;
    changed(&st, &id).await;
    Ok(Json(json!({ "ok": true })))
}

/// Tells the open pages a clip changed: its title, its poster or its current version.
async fn changed(st: &AppState, id: &str) {
    if let Ok(Some(c)) = st.ctx.store.clip(id).await {
        st.ctx.emit(reel_core::events::Event::Clip {
            id: c.id,
            state: "updated".into(),
            title: c.title,
        });
    }
}

pub async fn delete_clip(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let clip = editable(&st, &me, &id).await?;
    trash_clip(&st.ctx, &clip).await?;
    st.ctx.store.set_deleted_by(&id, &me.name).await?;
    sweep_web_later(&st);
    Ok(Json(
        json!({ "ok": true, "trash_days": st.ctx.config().share.trash_days }),
    ))
}

pub async fn discard_clip(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    editable(&st, &me, &id).await?;
    reel_pipeline::jobs::discard(&st.ctx, &id)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn restore_clip(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    editable(&st, &me, &id).await?;
    if !restore(&st.ctx, &id).await? {
        return Err(AppError::NotFound);
    }
    if let Some(c) = st.ctx.store.clip(&id).await? {
        st.ctx.emit(reel_core::events::Event::Clip {
            id: c.id,
            state: "ready".into(),
            title: c.title,
        });
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct NewCut {
    #[serde(flatten)]
    pub recipe: CutRecipe,
    #[serde(default)]
    pub normalize: bool,
    #[serde(default)]
    pub as_new_clip: bool,
    #[serde(default)]
    pub title: Option<String>,
}

pub async fn create_cut(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<NewCut>,
) -> AppResult<Json<Value>> {
    let clip = editable(&st, &me, &id).await?;
    if clip.state != "ready" {
        return Err(AppError::NotFound);
    }
    let mut recipe = body.recipe;
    recipe.gain_db = CutRecipe::check_gain(recipe.gain_db).map_err(AppError::BadRequest)?;
    if let Some(tracks) = recipe.tracks.as_mut() {
        if tracks.len() > reel_core::recipe::TRACKS_MAX {
            return Err(AppError::BadRequest("too many tracks".into()));
        }
        for t in tracks.iter_mut() {
            t.gain_db = CutRecipe::check_gain(t.gain_db).map_err(AppError::BadRequest)?;
        }
    }
    recipe.loudness = body
        .normalize
        .then(|| st.ctx.config().media.loudness_target as f64);
    if recipe.pictured() {
        let whole = match clip.master_id.as_deref() {
            Some(m) => st.ctx.store.rendition(m).await?.and_then(|m| m.duration),
            None => None,
        }
        .unwrap_or(0.0);
        let len = recipe.end.unwrap_or(whole).min(whole) - recipe.start.unwrap_or(0.0).max(0.0);
        recipe.check_picture(len).map_err(AppError::BadRequest)?;
    }
    let recipe = recipe.settled();
    if recipe.audio_changed() {
        let master = match clip.master_id.as_deref() {
            Some(m) => st.ctx.store.rendition(m).await?,
            None => None,
        };
        if master.and_then(|m| m.acodec).is_none() {
            return Err(AppError::BadRequest(
                "this clip has no audio to change".into(),
            ));
        }
    }
    if let (Some(s), Some(e)) = (recipe.start, recipe.end) {
        if e <= s + 0.1 {
            return Err(AppError::BadRequest(
                "the end must come after the start".into(),
            ));
        }
    }
    if recipe.is_noop() {
        return Err(AppError::BadRequest(
            "nothing to cut: no trim, no mute, no change of volume, no frame or fades".into(),
        ));
    }
    let target = if body.as_new_clip {
        CutTarget::NewClip {
            title: body.title.unwrap_or_default(),
            owner_id: Some(me.id.clone()),
        }
    } else {
        CutTarget::Version
    };
    let req = request_cut(&st.ctx, &clip, recipe, target).await?;
    Ok(Json(json!({
        "rendition_id": req.rendition.id,
        "job_id": req.job.id,
        "label": if req.new_clip.is_some() { "the new clip".to_string() } else { req.rendition.label() },
        "clip_id": req.new_clip.as_ref().map(|c| c.id.clone()),
        "clip_title": req.new_clip.as_ref().map(|c| c.title.clone()),
    })))
}

/// Measures a version that ingest did not, one at a time.
pub async fn rendition_level(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(rid): Path<String>,
) -> AppResult<Json<Value>> {
    static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let find = || async {
        st.ctx
            .store
            .rendition(&rid)
            .await?
            .filter(|r| r.is_version() && r.is_ready())
            .ok_or(AppError::NotFound)
    };
    let r = find().await?;
    editable(&st, &me, &r.clip_id).await?;
    let _one = ONE.lock().await;
    let r = find().await?;
    let level = reel_pipeline::level::of(&st.ctx, &r)
        .await
        .map_err(|e| AppError::Internal(e.context("measuring the audio")))?;
    Ok(Json(json!({
        "audio": r.acodec.is_some(),
        "loudness": level.loudness,
        "peak": level.peak,
    })))
}

pub async fn delete_cut(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(rid): Path<String>,
) -> AppResult<impl IntoResponse> {
    let r = st
        .ctx
        .store
        .rendition(&rid)
        .await?
        .ok_or(AppError::NotFound)?;
    editable(&st, &me, &r.clip_id).await?;
    if r.kind == "anim" {
        return if reel_pipeline::anim::delete(&st.ctx, &r).await? {
            tracing::info!(clip = %r.clip_id, by = %me.name, "an animated picture deleted");
            Ok((StatusCode::OK, Json(json!({ "ok": true }))))
        } else {
            Ok((
                StatusCode::CONFLICT,
                Json(json!({ "error": "it is being made: cancel it in Jobs" })),
            ))
        };
    }
    match remove_cut(&st.ctx, &r).await? {
        CutDelete::Deleted => {
            st.ctx.external.kick();
            Ok((StatusCode::OK, Json(json!({ "ok": true }))))
        }
        CutDelete::Refused(why) => Ok((StatusCode::CONFLICT, Json(json!({ "error": why })))),
    }
}

#[derive(Deserialize)]
pub struct SetCurrent {
    pub rendition_id: String,
}

pub async fn set_current(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<SetCurrent>,
) -> AppResult<Json<Value>> {
    let clip = editable(&st, &me, &id).await?;
    let r = st
        .ctx
        .store
        .rendition(&body.rendition_id)
        .await?
        .filter(|r| r.clip_id == id && r.is_ready() && r.is_version())
        .ok_or(AppError::NotFound)?;
    reel_pipeline::enqueue::make_current(&st.ctx, &clip, &r).await?;
    changed(&st, &id).await;
    Ok(Json(json!({ "ok": true, "current": r.label() })))
}

#[derive(Deserialize)]
pub struct SetPoster {
    pub time: f64,
}

pub async fn set_poster(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<SetPoster>,
) -> AppResult<Json<Value>> {
    let clip = editable(&st, &me, &id).await?;
    poster(&st.ctx, &clip, body.time).await?;
    changed(&st, &id).await;
    let key = st
        .ctx
        .store
        .clip(&id)
        .await?
        .map(|c| c.poster_key())
        .unwrap_or_default();
    Ok(Json(
        json!({ "ok": true, "poster": format!("/media/renditions/{}.jpg?t={key}", clip.current_id.unwrap_or_default()) }),
    ))
}

/// An uploaded image as the poster; it is decoded and encoded again before any of it is served.
pub async fn upload_poster(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> AppResult<Json<Value>> {
    let clip = editable(&st, &me, &id).await?;
    if clip.state != "ready" || clip.deleted_at.is_some() {
        return Err(AppError::NotFound);
    }
    crate::error::library_ready(&st.ctx).await?;
    match reel_pipeline::poster_image::take(&st.ctx, &clip, &body).await? {
        Ok(key) => {
            changed(&st, &id).await;
            tracing::info!(clip = %id, by = %me.name, "poster uploaded");
            Ok(Json(
                json!({ "ok": true, "poster": format!("/media/renditions/{}.jpg?t={key}", clip.current_id.unwrap_or_default()) }),
            ))
        }
        Err(why @ reel_pipeline::poster_image::Refused::Busy) => {
            Err(AppError::Conflict(why.to_string()))
        }
        Err(why) => Err(AppError::BadRequest(why.to_string())),
    }
}

#[derive(Deserialize)]
pub struct NewShare {
    pub label: Option<String>,
    pub expires_in: Option<String>,
    pub allow_download: Option<bool>,
    pub rendition_id: Option<String>,
}

pub fn expiry_from(expires_in: &str) -> Result<Option<String>, AppError> {
    let now = Timestamp::now();
    let span = match expires_in {
        "" | "never" => return Ok(None),
        "1d" => Span::new().hours(24),
        "7d" => Span::new().hours(24 * 7),
        "30d" => Span::new().hours(24 * 30),
        other => {
            let t = reel_core::format::parse(other)
                .ok_or_else(|| AppError::BadRequest(format!("{other} is not an expiry")))?;
            if t <= now {
                return Err(AppError::BadRequest("that date is in the past".into()));
            }
            return Ok(Some(t.to_string()));
        }
    };
    Ok(Some(
        now.checked_add(span)
            .map_err(|e| AppError::BadRequest(e.to_string()))?
            .to_string(),
    ))
}

/// How long a new link waits for its preview on the external storage before it is handed out.
const LINK_WAITS: std::time::Duration = std::time::Duration::from_secs(90);

type Slot = std::sync::Arc<tokio::sync::Mutex<Option<Share>>>;

/// The links being made, by ask: a second press or tab while one waits gets that link, not another.
#[derive(Default)]
pub struct LinksMaking(std::sync::Mutex<std::collections::HashMap<String, Slot>>);

/// The first request of an ask: it fills in the link, and the ask is over when it answers.
struct Making {
    all: std::sync::Arc<LinksMaking>,
    ask: String,
    held: Option<tokio::sync::OwnedMutexGuard<Option<Share>>>,
}

impl Making {
    fn made(&mut self, share: &Share) {
        if let Some(mut held) = self.held.take() {
            *held = Some(share.clone());
        }
    }
}

impl Drop for Making {
    fn drop(&mut self) {
        self.all.0.lock().unwrap().remove(&self.ask);
    }
}

enum Turn {
    First(Making),
    After(Slot),
}

fn join(all: &std::sync::Arc<LinksMaking>, ask: &str) -> Turn {
    let mut asks = all.0.lock().unwrap();
    if let Some(slot) = asks.get(ask) {
        return Turn::After(slot.clone());
    }
    let slot = Slot::default();
    let held = slot.clone().try_lock_owned().ok();
    asks.insert(ask.to_string(), slot);
    Turn::First(Making {
        all: all.clone(),
        ask: ask.to_string(),
        held,
    })
}

pub async fn create_share(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<NewShare>,
) -> AppResult<Json<Value>> {
    let clip = editable(&st, &me, &id).await?;
    if clip.state != "ready" || clip.deleted_at.is_some() {
        return Err(AppError::NotFound);
    }
    let settings = crate::settings::client(&st.ctx.config());
    let allow = body
        .allow_download
        .unwrap_or(settings.share_download_default);
    let expires_at = expiry_from(
        body.expires_in
            .as_deref()
            .unwrap_or(&settings.share_expiry_default),
    )?;
    let versions = st.ctx.store.renditions_of(&clip.id).await?;
    let pinned = match body.rendition_id.as_deref().filter(|r| !r.is_empty()) {
        Some(rid) => {
            if !versions
                .iter()
                .any(|r| r.id == rid && r.is_ready() && r.is_version())
            {
                return Err(AppError::BadRequest(
                    "that version does not exist on this clip".into(),
                ));
            }
            Some(rid.to_string())
        }
        None => None,
    };
    let label = body.label.unwrap_or_default().trim().to_string();
    let ask = format!(
        "{}\n{label}\n{allow}\n{}\n{}",
        clip.id,
        body.expires_in
            .as_deref()
            .unwrap_or(&settings.share_expiry_default),
        pinned.as_deref().unwrap_or("")
    );
    let mut first = None;
    let earlier = match join(&st.links_making, &ask) {
        Turn::First(m) => {
            first = Some(m);
            None
        }
        Turn::After(slot) => slot.lock().await.clone(),
    };
    let made = earlier.is_none();
    let share = match earlier {
        Some(s) => {
            tracing::info!(clip = %clip.id, share = %s.id, "the same link again: it was being made");
            s
        }
        None => {
            let share = Share::new(clip.id.clone(), label, allow, expires_at, pinned);
            st.ctx.store.insert_share(&share).await?;
            if let Some(m) = first.as_mut() {
                m.made(&share);
            }
            tracing::info!(clip = %clip.id, share = %share.id, token = %&share.token[..4], "share created");
            share
        }
    };
    let served = share.rendition_id.as_deref().or(clip.current_id.as_deref());
    if let (true, Some(r)) = (
        made,
        versions.iter().find(|r| Some(r.id.as_str()) == served),
    ) {
        let _ = reel_pipeline::enqueue::ensure_web(&st.ctx, r).await;
    }
    let mut view = share_view(
        &share,
        &st.ctx.config().public_url,
        serves_text(&share, &versions, clip.current_id.as_deref()),
    );
    let mut note = String::new();
    let version = versions.iter().find(|r| Some(r.id.as_str()) == served);
    if let (true, Some(v)) = (st.ctx.config().external_on(), version) {
        let (place, ready) = reel_pipeline::external::settle(&st.ctx, v, LINK_WAITS).await?;
        view.preview = reel_pipeline::external::where_words(&place);
        note = reel_pipeline::external::where_note(&place, ready);
    }
    if made {
        reel_pipeline::notify::send_later(
            &st.ctx,
            reel_pipeline::notify::Note::new(
                "link_made",
                format!("{}: a link was made", st.ctx.config().site_name),
                format!("{} by {}", clip.title, me.name),
                view.url.clone(),
            ),
        );
    }
    Ok(Json(
        json!({ "id": share.id, "token": share.token, "url": view.url, "expiry": view.expiry, "download": view.download, "label": view.label, "created": view.created, "serves": view.serves, "activity": view.activity, "preview": view.preview, "preview_note": note }),
    ))
}

#[derive(Deserialize)]
pub struct LinkWait {
    pub r: Option<String>,
}

/// What a link being made waits for, for its button: the step and how far.
pub async fn link_wait(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<LinkWait>,
) -> AppResult<Json<Value>> {
    let clip = editable(&st, &me, &id).await?;
    let rid =
        q.r.filter(|r| !r.is_empty())
            .or(clip.current_id)
            .ok_or(AppError::NotFound)?;
    let version = st
        .ctx
        .store
        .rendition(&rid)
        .await?
        .filter(|r| r.clip_id == clip.id)
        .ok_or(AppError::NotFound)?;
    if !st.ctx.config().external_on() {
        return Ok(Json(json!({ "ready": true, "step": "", "progress": 1.0 })));
    }
    let r = reel_pipeline::external::readiness(&st.ctx, &version).await?;
    Ok(Json(
        json!({ "ready": r.ready, "step": r.step, "progress": r.progress }),
    ))
}

pub async fn revoke_share(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let share = st.ctx.store.share(&id).await?.ok_or(AppError::NotFound)?;
    editable(&st, &me, &share.clip_id).await?;
    st.ctx.store.revoke_share(&id).await?;
    st.ctx.external.kick();
    sweep_web_later(&st);
    Ok(Json(json!({ "ok": true })))
}

/// The web copies no live link plays any more go, once the request is answered.
pub fn sweep_web_later(st: &AppState) {
    let ctx = st.ctx.clone();
    tokio::spawn(async move {
        if let Err(e) = reel_pipeline::housekeeping::sweep_web_copies(&ctx, false).await {
            tracing::warn!(error = %e, "web copies");
        }
    });
}

pub async fn job_json(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let job = st.ctx.store.job(&id).await?.ok_or(AppError::NotFound)?;
    Ok(Json(serde_json::to_value(job)?))
}

#[derive(Deserialize)]
pub struct DiskQuery {
    pub refresh: Option<u8>,
}

pub async fn disk(
    State(st): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<DiskQuery>,
) -> AppResult<Json<Value>> {
    let info = st
        .disk
        .get(&st.ctx.effective(), q.refresh.unwrap_or(0) == 1)
        .await;
    let clips = st.ctx.store.library_bytes().await? as u64;
    Ok(Json(info.to_json(clips)))
}
