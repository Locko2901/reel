//! The device API: pairing, resumable chunked uploads, clips' status and links, and Devices.
use crate::auth::User;
use crate::device::{code_hash, token_hash, DeviceAuth};
use crate::error::{AppError, AppResult};
use crate::views::{serves_text, share_view};
use crate::AppState;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use reel_core::format;
use reel_core::settings::{self as catalog, Section, Source};
use reel_pipeline::enqueue::{create_clip_for_file, embed_grade, NewClip};
use reel_pipeline::hash::looks_like_sha256;
use reel_store::models::{Clip, Device, DeviceUpload, PairCode, Share};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const PAIR_MINUTES: i64 = 10;

#[derive(Deserialize)]
pub struct PairBody {
    pub code: String,
    pub name: String,
}

/// The app's first call: the code from Settings, Devices, and a name for the device.
pub async fn pair(State(st): State<AppState>, Json(body): Json<PairBody>) -> AppResult<Response> {
    let name = body.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(AppError::BadRequest(
            "a device name has 1 to 60 characters".into(),
        ));
    }
    let normal = reel_core::auth::normal_code(&body.code);
    if normal.len() != 8 {
        return Err(AppError::Forbidden(
            "that is not a pairing code; make one under Settings, Devices".into(),
        ));
    }
    let token = reel_core::ids::new_device_token();
    let mut device = Device::new("", &name, &token_hash(&token));
    let Some(code) = st
        .ctx
        .store
        .take_pair_code(&code_hash(&body.code), &device.id)
        .await?
    else {
        tracing::warn!(code = %&normal[..2], "pairing refused: wrong, spent or expired code");
        return Err(AppError::Forbidden(
            "that code is wrong, used already or past its ten minutes; make a new one under Settings, Devices".into(),
        ));
    };
    let owner = st
        .ctx
        .store
        .user(&code.user_id)
        .await?
        .filter(|u| u.disabled_at.is_none())
        .ok_or_else(|| AppError::Forbidden("the account that made the code is gone".into()))?;
    device.user_id = owner.id.clone();
    st.ctx.store.insert_device(&device).await?;
    tracing::info!(device = %device.id, name = %device.name, user = %owner.name, "device paired");
    let cfg = st.ctx.config();
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "token": token, "device_id": device.id, "name": device.name,
            "user": owner.name, "server_name": cfg.site_name,
        })),
    )
        .into_response())
}

/// The shape of the Settings pages for the app's own section: no setting value, and no section
/// at all for a device whose user is not an admin.
pub async fn sections(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
) -> AppResult<Json<Value>> {
    let user = User {
        id: me.user.id.clone(),
        name: me.user.name.clone(),
        admin: me.user.is_admin(),
    };
    let who = crate::views::Who::at(&st, &user).await;
    let eff = st.ctx.effective();
    let sections: Vec<Value> = if who.admin {
        Section::ALL
            .iter()
            .map(|s| {
                let changed = catalog::DEFS
                    .iter()
                    .filter(|d| d.section == *s && eff.source(d.key) == Source::Table)
                    .count();
                json!({ "id": s.id(), "label": s.label(), "line": s.line(), "changed": changed })
            })
            .collect()
    } else {
        Vec::new()
    };
    let (queued, running, failed) = st.ctx.store.job_counts_full().await?;
    Ok(Json(json!({
        "sections": sections,
        "who": { "name": who.name, "line": who.line, "admin": who.admin },
        "health": { "state": who.health, "line": who.health_line },
        "jobs": { "queued": queued, "running": running, "failed": failed },
    })))
}

pub async fn whoami(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
) -> AppResult<Json<Value>> {
    let cfg = st.ctx.config();
    Ok(Json(json!({
        "device_id": me.device.id, "name": me.device.name, "user": me.user.name,
        "server_name": cfg.site_name, "public_url": cfg.public_url,
        "chunk_bytes": cfg.device_chunk_bytes(), "max_chunk_bytes": cfg.device_max_chunk_bytes(),
        "version": reel_core::VERSION,
        "newest_release": reel_pipeline::app_update::newest(),
        "upload_max_bytes": cfg.upload_max_bytes(),
        "admin": me.user.role == "admin",
        "normalize_new": cfg.media.normalize_new,
        "loudness_target": cfg.media.loudness_target,
    })))
}

#[derive(Deserialize)]
pub struct NewUpload {
    pub file_name: String,
    pub size: i64,
    pub sha256: String,
    #[serde(default)]
    pub recorded_at: Option<String>,
    #[serde(default)]
    pub game: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub keep_last: Option<f64>,
    /// The audio tracks' names in the file's order, for a file that carries none.
    #[serde(default)]
    pub audio_names: Option<Vec<String>>,
    /// "recording" for a session recording; anything else is a clip.
    #[serde(default)]
    pub kind: Option<String>,
}

fn short(s: Option<String>, max: usize) -> Option<String> {
    s.map(|t| t.trim().chars().take(max).collect::<String>())
        .filter(|t| !t.is_empty())
}

fn upload_dir(st: &AppState, id: &str) -> PathBuf {
    st.ctx.config().device_uploads_dir().join(id)
}

fn part_path(st: &AppState, u: &DeviceUpload) -> PathBuf {
    reel_media::ffmpeg::part_path(&upload_dir(st, &u.id).join(&u.file_name))
}

async fn active_progress(st: &AppState, clip_id: &str) -> AppResult<Option<(String, f64)>> {
    Ok(st
        .ctx
        .store
        .active_jobs()
        .await?
        .into_iter()
        .find(|j| j.clip_id.as_deref() == Some(clip_id))
        .map(|j| (j.kind, j.progress)))
}

/// Whether the share page would embed in Discord right now; when not, the copy's progress,
/// or the reason when the copy failed.
#[derive(Default)]
struct Preview {
    ready: bool,
    progress: Option<f64>,
    failed: Option<String>,
}

async fn preview_state(st: &AppState, clip: &Clip) -> AppResult<Preview> {
    if clip.state != "ready" {
        return Ok(Preview::default());
    }
    let Some(cid) = clip.current_id.as_deref() else {
        return Ok(Preview::default());
    };
    let Some(current) = st.ctx.store.rendition(cid).await? else {
        return Ok(Preview::default());
    };
    if st.ctx.config().external_on() {
        let r = reel_pipeline::external::readiness(&st.ctx, &current).await?;
        return Ok(Preview {
            ready: r.ready,
            progress: (!r.ready).then_some(r.progress),
            failed: None,
        });
    }
    match embed_grade(&st.ctx, &current).await? {
        Some(_) => Ok(Preview {
            ready: true,
            ..Preview::default()
        }),
        None => {
            let copy = st
                .ctx
                .store
                .derived_rendition(&current.id, "embed", None)
                .await?;
            match copy {
                Some(c) if c.state == "failed" => Ok(Preview {
                    failed: Some(
                        st.ctx.config().hide_paths(
                            c.error
                                .as_deref()
                                .unwrap_or("the preview video could not be made"),
                        ),
                    ),
                    ..Preview::default()
                }),
                Some(c) => Ok(Preview {
                    progress: st
                        .ctx
                        .store
                        .active_job_for_rendition(&c.id)
                        .await?
                        .map(|j| j.progress),
                    ..Preview::default()
                }),
                None => Ok(Preview::default()),
            }
        }
    }
}

async fn newest_active_share(st: &AppState, clip_id: &str) -> AppResult<Option<Share>> {
    Ok(st
        .ctx
        .store
        .shares_of(clip_id)
        .await?
        .into_iter()
        .find(|s| s.is_active() && s.rendition_id.is_none()))
}

async fn clip_json(st: &AppState, clip: &Clip) -> AppResult<Value> {
    let (step, progress) = match active_progress(st, &clip.id).await? {
        Some((kind, p)) => (Some(kind), p),
        None => (None, if clip.state == "ready" { 1.0 } else { 0.0 }),
    };
    let preview = preview_state(st, clip).await?;
    let share_url = newest_active_share(st, &clip.id).await?.map(|s| {
        format!(
            "{}/s/{}",
            st.ctx.config().public_url.trim_end_matches('/'),
            s.token
        )
    });
    Ok(json!({
        "id": clip.id, "title": clip.title, "state": clip.state,
        "progress": progress, "step": step,
        "ready": clip.state == "ready", "preview_ready": preview.ready, "preview_progress": preview.progress,
        "preview_error": preview.failed,
        "error": clip.error.as_deref().map(|e| st.ctx.config().hide_paths(e)),
        "share_url": share_url, "created_at": clip.created_at, "recorded_at": clip.recorded_at,
        "game": clip.game,
    }))
}

pub async fn create_upload(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
    Json(body): Json<NewUpload>,
) -> AppResult<Response> {
    crate::error::library_ready(&st.ctx).await?;
    let cfg = st.ctx.config();
    let name = std::path::Path::new(body.file_name.trim())
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    if name.is_empty() || name.chars().count() > 200 {
        return Err(AppError::BadRequest("a file name is needed".into()));
    }
    if !cfg.accepts_extension(std::path::Path::new(&name)) {
        return Err(AppError::BadRequest(format!(
            "{name}: not a clip we take ({})",
            cfg.ingest.extensions.join(", ")
        )));
    }
    if body.size <= 0 {
        return Err(AppError::BadRequest("the size must be above zero".into()));
    }
    if body.size as u64 > cfg.upload_max_bytes() as u64 {
        return Err(AppError::TooLarge(format!(
            "an upload over {} GB is refused; the limit is under Settings, Imports",
            cfg.import.upload_max_gb
        )));
    }
    let sha = body.sha256.trim().to_ascii_lowercase();
    if !looks_like_sha256(&sha) {
        return Err(AppError::BadRequest(
            "sha256 is 64 hex characters of the whole file".into(),
        ));
    }
    let keep_last = match body.keep_last.filter(|s| *s != 0.0) {
        Some(s) if s.is_finite() && (5.0..=600.0).contains(&s) => Some(s),
        Some(_) => {
            return Err(AppError::BadRequest(
                "keep_last is 5 to 600 seconds, or 0 for the whole clip".into(),
            ))
        }
        None => None,
    };
    let recorded_at = match body.recorded_at.as_deref().map(str::trim) {
        Some(r) if !r.is_empty() => Some(
            reel_core::format::parse(r)
                .ok_or_else(|| AppError::BadRequest(format!("{r} is not an RFC 3339 time")))?
                .to_string(),
        ),
        _ => None,
    };
    if let Some(first) = the_users_copy(&st, &me, &sha).await? {
        st.ctx.store.adopt_clip(&first.id, &me.device.id).await?;
        if !st
            .ctx
            .store
            .device_has_clip(&me.device.id, &first.id)
            .await?
        {
            let mut u = DeviceUpload::new(&me.device, &name, body.size, &sha);
            u.state = "done".into();
            u.finished_at = Some(u.created_at.clone());
            u.clip_id = Some(first.id.clone());
            st.ctx.store.insert_upload(&u).await?;
        }
        let first = st.ctx.store.clip(&first.id).await?.unwrap_or(first);
        tracing::info!(device = %me.device.id, clip = %first.id, name, "upload skipped: the library has this file");
        return Ok(Json(json!({ "status": "exists", "clip_id": first.id, "clip": clip_json(&st, &first).await? }))
            .into_response());
    }
    if let Some(open) = st
        .ctx
        .store
        .open_upload_by_hash(&me.device.id, &sha)
        .await?
    {
        let have = tokio::fs::metadata(part_path(&st, &open))
            .await
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        let offset = have.min(open.received);
        if offset != open.received {
            st.ctx.store.upload_received(&open.id, offset).await?;
        }
        return Ok(Json(json!({ "status": "resumed", "upload_id": open.id, "offset": offset, "chunk_bytes": cfg.device_chunk_bytes() }))
            .into_response());
    }
    let mut u = DeviceUpload::new(&me.device, &name, body.size, &sha);
    u.recorded_at = recorded_at;
    u.game = reel_core::games::clean(short(body.game, 80).as_deref());
    u.title = short(body.title, 200);
    u.kind = body.kind.filter(|k| k == reel_pipeline::enqueue::RECORDING);
    u.keep_last = keep_last.filter(|_| u.kind.is_none());
    u.audio_names = body
        .audio_names
        .filter(|n| !n.is_empty() && n.len() <= 16)
        .map(|n| {
            let n: Vec<String> = n
                .into_iter()
                .map(|t| reel_core::probe::track_title(&t))
                .collect();
            serde_json::to_string(&n).unwrap_or_default()
        });
    tokio::fs::create_dir_all(upload_dir(&st, &u.id)).await?;
    tokio::fs::File::create(part_path(&st, &u)).await?;
    st.ctx.store.insert_upload(&u).await?;
    tracing::info!(device = %me.device.id, upload = %u.id, name, size = u.size, "upload created");
    Ok((
        StatusCode::CREATED,
        Json(json!({ "status": "created", "upload_id": u.id, "offset": 0, "chunk_bytes": cfg.device_chunk_bytes() })),
    )
        .into_response())
}

/// The clip the library has of this file already, when it is the device's user's own. Another
/// user's clip of the same file is not handed over: the upload is refused.
async fn the_users_copy(st: &AppState, me: &DeviceAuth, sha: &str) -> AppResult<Option<Clip>> {
    let found = st.ctx.store.clips_by_source_hash(sha).await?;
    if found.is_empty() {
        return Ok(None);
    }
    match found.into_iter().find(|c| c.is_owned_by(&me.user)) {
        Some(c) => Ok(Some(c)),
        None => {
            tracing::info!(device = %me.device.id, "upload refused: the library has this file as another user's clip");
            Err(AppError::Forbidden(
                "the library has this file already, as another user's clip".into(),
            ))
        }
    }
}

async fn own_upload(st: &AppState, me: &DeviceAuth, id: &str) -> AppResult<DeviceUpload> {
    st.ctx
        .store
        .upload(id)
        .await?
        .filter(|u| u.device_id == me.device.id)
        .ok_or(AppError::NotFound)
}

#[derive(Deserialize)]
pub struct ChunkQuery {
    pub offset: i64,
}

fn pieces(st: &AppState, u: &DeviceUpload) -> super::pieces::Pieces {
    super::pieces::Pieces {
        kind: super::pieces::Kind::Device,
        id: u.id.clone(),
        part: part_path(st, u),
        size: u.size,
        received: u.received,
    }
}

/// One chunk, appended at the offset it names; a wrong offset is answered with the real end.
pub async fn put_chunk(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
    Path(id): Path<String>,
    Query(q): Query<ChunkQuery>,
    headers: HeaderMap,
    body: Body,
) -> AppResult<Response> {
    let u = own_upload(&st, &me, &id).await?;
    if u.state != "open" {
        return Err(AppError::Conflict(format!("this upload is {}", u.state)));
    }
    super::pieces::put(&st, &pieces(&st, &u), q.offset, &headers, body).await
}

pub async fn upload_status(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let u = own_upload(&st, &me, &id).await?;
    Ok(Json(
        json!({ "upload_id": u.id, "offset": u.received, "size": u.size, "state": u.state, "clip_id": u.clip_id }),
    ))
}

pub async fn abandon_upload(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let u = own_upload(&st, &me, &id).await?;
    if u.state == "open" {
        let _ = tokio::fs::remove_dir_all(upload_dir(&st, &u.id)).await;
        st.ctx.store.delete_upload(&u.id).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

/// Checks the size and hash, then ingests the file as the owner's clip unless the library has it.
pub async fn finish_upload(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let u = own_upload(&st, &me, &id).await?;
    if u.state == "done" {
        if let Some(cid) = &u.clip_id {
            if let Some(clip) = st.ctx.store.clip(cid).await? {
                return Ok(Json(json!({ "clip_id": clip.id, "state": clip.state })).into_response());
            }
        }
        return Err(AppError::Conflict(
            "this upload is done, but its clip is gone".into(),
        ));
    }
    if u.state != "open" {
        return Err(AppError::Conflict(format!("this upload is {}", u.state)));
    }
    let part = part_path(&st, &u);
    if let Err(answer) = super::pieces::complete(&st, &pieces(&st, &u), &u.sha256).await? {
        return Ok(answer);
    }
    let sha = u.sha256.clone();
    let first = match the_users_copy(&st, &me, &sha).await {
        Ok(first) => first,
        Err(e) => {
            let _ = tokio::fs::remove_dir_all(upload_dir(&st, &u.id)).await;
            st.ctx
                .store
                .upload_failed(&u.id, "another user's clip has this file")
                .await?;
            return Err(e);
        }
    };
    if let Some(first) = first {
        let _ = tokio::fs::remove_dir_all(upload_dir(&st, &u.id)).await;
        st.ctx.store.adopt_clip(&first.id, &me.device.id).await?;
        st.ctx.store.upload_done(&u.id, &first.id).await?;
        return Ok(Json(json!({ "clip_id": first.id, "state": first.state })).into_response());
    }
    let target = upload_dir(&st, &u.id).join(&u.file_name);
    tokio::fs::rename(&part, &target).await?;
    let (clip, job) = create_clip_for_file(
        &st.ctx,
        NewClip {
            owner_id: Some(me.user.id.clone()),
            title: u.title.clone(),
            recorded_at: u.recorded_at.clone(),
            recorded_source: Some("device"),
            source_hash: Some(sha),
            device_id: Some(me.device.id.clone()),
            game: u.game.clone(),
            keep_last: u.keep_last,
            audio_names: u
                .audio_names
                .as_deref()
                .and_then(|n| serde_json::from_str(n).ok()),
            kind: u.kind.as_deref(),
            ..NewClip::new(&target, "device")
        },
    )
    .await?;
    st.ctx.store.upload_done(&u.id, &clip.id).await?;
    tracing::info!(device = %me.device.id, upload = %u.id, clip = %clip.id, job = %job.id, "upload finished, ingest queued");
    Ok(Json(json!({ "clip_id": clip.id, "state": clip.state })).into_response())
}

pub async fn clips(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
) -> AppResult<Json<Value>> {
    let mut out = Vec::new();
    for c in st.ctx.store.clips_of_device(&me.device.id, 50).await? {
        out.push(clip_json(&st, &c).await?);
    }
    Ok(Json(json!({ "clips": out })))
}

async fn own_clip(st: &AppState, me: &DeviceAuth, id: &str) -> AppResult<Clip> {
    let clip = st
        .ctx
        .store
        .clip(id)
        .await?
        .filter(|c| c.deleted_at.is_none() && c.is_owned_by(&me.user))
        .ok_or(AppError::NotFound)?;
    if !st
        .ctx
        .store
        .device_has_clip(&me.device.id, &clip.id)
        .await?
    {
        return Err(AppError::NotFound);
    }
    Ok(clip)
}

pub async fn clip(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let c = own_clip(&st, &me, &id).await?;
    Ok(Json(clip_json(&st, &c).await?))
}

#[derive(Deserialize, Default)]
pub struct ShareBody {
    #[serde(default)]
    pub clip_id: Option<String>,
}

/// A link for the device's newest clip, or the one named, once Discord would embed it.
pub async fn share(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
    body: Option<Json<ShareBody>>,
) -> AppResult<Response> {
    let wanted = body.and_then(|b| b.0.clip_id).filter(|c| !c.is_empty());
    let clip = match wanted {
        Some(id) => own_clip(&st, &me, &id).await?,
        None => st
            .ctx
            .store
            .clips_of_device(&me.device.id, 1)
            .await?
            .into_iter()
            .next()
            .ok_or(AppError::NotFound)?,
    };
    if clip.state == "failed" {
        return Err(AppError::Conflict(format!(
            "the clip failed: {}",
            st.ctx
                .config()
                .hide_paths(clip.error.as_deref().unwrap_or("no reason"))
        )));
    }
    let preparing = |step: &str, progress: f64| {
        (
            StatusCode::ACCEPTED,
            Json(json!({ "status": "preparing", "clip_id": clip.id, "title": clip.title, "step": step, "progress": progress })),
        )
            .into_response()
    };
    if clip.state != "ready" {
        let p = active_progress(&st, &clip.id)
            .await?
            .map(|(_, p)| p)
            .unwrap_or(0.0);
        return Ok(preparing("ingest", p));
    }
    if st.ctx.config().external_on() {
        return share_through_storage(&st, &me, &clip, preparing).await;
    }
    let preview = preview_state(&st, &clip).await?;
    if let Some(why) = preview.failed {
        return Err(AppError::Conflict(format!(
            "the preview video could not be made: {why}"
        )));
    }
    if !preview.ready {
        return Ok(preparing("preview", preview.progress.unwrap_or(0.0)));
    }
    let share = link_for(&st, &me, &clip).await?;
    let url = format!(
        "{}/s/{}",
        st.ctx.config().public_url.trim_end_matches('/'),
        share.token
    );
    Ok(
        Json(json!({ "status": "ready", "url": url, "clip_id": clip.id, "title": clip.title }))
            .into_response(),
    )
}

/// With the external storage on, the link goes to the app once its copy and preview are there,
/// or the preview is served from here with the reason recorded.
async fn share_through_storage(
    st: &AppState,
    me: &DeviceAuth,
    clip: &Clip,
    preparing: impl Fn(&str, f64) -> Response,
) -> AppResult<Response> {
    let current = match clip.current_id.as_deref() {
        Some(id) => st.ctx.store.rendition(id).await?,
        None => None,
    }
    .ok_or(AppError::NotFound)?;
    let copy =
        reel_pipeline::enqueue::ext_file(&st.ctx, &current, reel_store::models::PRIORITY_EMBED)
            .await?;
    if let reel_pipeline::enqueue::ExtFile::Making(p) = copy {
        return Ok(preparing("preview", p.unwrap_or(0.0)));
    }
    let share = link_for(st, me, clip).await?;
    st.ctx.external.kick();
    let r = reel_pipeline::external::readiness(&st.ctx, &current).await?;
    if !r.ready {
        return Ok(preparing(r.step, r.progress));
    }
    let url = format!(
        "{}/s/{}",
        st.ctx.config().public_url.trim_end_matches('/'),
        share.token
    );
    Ok(
        Json(json!({ "status": "ready", "url": url, "clip_id": clip.id, "title": clip.title }))
            .into_response(),
    )
}

/// The clip's newest link that follows its current version, made with the defaults if none.
async fn link_for(st: &AppState, me: &DeviceAuth, clip: &Clip) -> AppResult<Share> {
    Ok(match newest_active_share(st, &clip.id).await? {
        Some(s) => s,
        None => {
            let settings = crate::settings::client(&st.ctx.config());
            let expires_at = crate::routes::api::expiry_from(&settings.share_expiry_default)?;
            let s = Share::new(
                clip.id.clone(),
                String::new(),
                settings.share_download_default,
                expires_at,
                None,
            );
            st.ctx.store.insert_share(&s).await?;
            tracing::info!(clip = %clip.id, share = %s.id, token = %&s.token[..4], device = %me.device.id, "share created by a device");
            let versions = st.ctx.store.renditions_of(&clip.id).await?;
            if let Some(r) = versions
                .iter()
                .find(|r| Some(r.id.as_str()) == clip.current_id.as_deref())
            {
                let _ = reel_pipeline::enqueue::ensure_web(&st.ctx, r).await;
            }
            let view = share_view(
                &s,
                &st.ctx.config().public_url,
                serves_text(&s, &versions, clip.current_id.as_deref()),
            );
            reel_pipeline::notify::send_later(
                &st.ctx,
                reel_pipeline::notify::Note::new(
                    "link_made",
                    format!("{}: a link was made", st.ctx.config().site_name),
                    format!("{} by {} ({})", clip.title, me.user.name, me.device.name),
                    view.url,
                ),
            );
            s
        }
    })
}

#[derive(Deserialize, Default)]
pub struct UpdateQuery {
    #[serde(default)]
    pub current: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    /// `with-obs` from the app with OBS, `appimage` from the AppImage, none from the one without.
    #[serde(default)]
    pub flavour: Option<String>,
}

/// This server's app for an older paired app, in the updater's dynamic format; 204 if none.
pub async fn update(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
    Query(q): Query<UpdateQuery>,
) -> AppResult<Response> {
    use reel_pipeline::app_update;
    let cfg = st.ctx.config();
    let none = || StatusCode::NO_CONTENT.into_response();
    let current = q.current.unwrap_or_default();
    if !cfg.device.app_updates
        || !app_update::serves(q.target.as_deref(), q.arch.as_deref())
        || !app_update::offers_to(&current)
    {
        return Ok(none());
    }
    let flavour = app_update::Flavour::for_update(q.flavour.as_deref(), q.target.as_deref());
    if flavour == app_update::Flavour::Deb {
        return Ok(none());
    }
    let Some(offer) = app_update::offer(&cfg, flavour).await else {
        return Ok(none());
    };
    tracing::info!(device = %me.device.id, from = %current, to = %offer.version, installer = flavour.id(), "app update offered");
    let mut url = format!(
        "{}/api/device/update/installer",
        cfg.public_url.trim_end_matches('/')
    );
    if let Some(f) = flavour.query() {
        url.push_str(&format!("?flavour={f}"));
    }
    Ok(Json(json!({
        "version": offer.version,
        "notes": app_update::notes_between(&current, &offer.version),
        "pub_date": offer.published,
        "url": url,
        "signature": offer.signature,
    }))
    .into_response())
}

/// Standalone only: the newer release the last check read, with this app's installer at its
/// address on the release, in the updater's format; 204 if none.
pub async fn release_update(
    State(st): State<AppState>,
    Query(q): Query<UpdateQuery>,
) -> AppResult<Response> {
    use reel_pipeline::app_update;
    if !st.ctx.config().standalone() {
        return Err(AppError::NotFound);
    }
    let none = || StatusCode::NO_CONTENT.into_response();
    let current = q.current.unwrap_or_default();
    if !app_update::serves(q.target.as_deref(), q.arch.as_deref()) {
        return Ok(none());
    }
    let flavour = app_update::Flavour::for_update(q.flavour.as_deref(), q.target.as_deref());
    let latest = st.ctx.updates.read().await.latest.clone();
    let Some(offer) = latest
        .as_ref()
        .and_then(|r| app_update::from_release(r, &current, flavour).ok())
    else {
        return Ok(none());
    };
    let signature = match app_update::release_signature(&offer.signature).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, version = %offer.version, "the installer's signature was not read from the release");
            return Ok((
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("the installer's signature was not read: {e}") })),
            )
                .into_response());
        }
    };
    tracing::info!(from = %current, to = %offer.version, installer = flavour.id(), "the release's app offered to this app");
    Ok(Json(json!({
        "version": offer.version,
        "notes": offer.notes,
        "pub_date": Some(offer.published).filter(|d| !d.is_empty()),
        "url": offer.installer,
        "signature": signature,
    }))
    .into_response())
}

pub async fn update_installer(
    State(st): State<AppState>,
    Extension(me): Extension<DeviceAuth>,
    Query(q): Query<UpdateQuery>,
    req: axum::extract::Request,
) -> AppResult<Response> {
    let flavour = reel_pipeline::app_update::Flavour::parse(q.flavour.as_deref());
    let offer = reel_pipeline::app_update::offer(&st.ctx.config(), flavour)
        .await
        .ok_or(AppError::NotFound)?;
    tracing::info!(device = %me.device.id, version = %offer.version, installer = flavour.id(), "app update downloaded");
    let name = reel_pipeline::app_update::installer_name(&offer.version, flavour);
    Ok(crate::serve_file::serve(
        req,
        &offer.installer,
        mime::APPLICATION_OCTET_STREAM,
        Some(name),
    )
    .await)
}

pub async fn pair_code(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    let code = reel_core::auth::new_pair_code();
    let expires = jiff::Timestamp::now()
        .checked_add(jiff::Span::new().minutes(PAIR_MINUTES))
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    st.ctx
        .store
        .insert_pair_code(&PairCode {
            code_hash: code_hash(&code),
            user_id: me.id.clone(),
            created_at: format::now(),
            expires_at: expires.to_string(),
            used_at: None,
            device_id: None,
        })
        .await?;
    tracing::info!(user = %me.name, code = %&code[..2], "pairing code made");
    Ok(Json(json!({
        "code": code, "expires_at": expires.to_string(), "expires_in": PAIR_MINUTES * 60,
        "server_url": st.ctx.config().public_url,
    })))
}

pub async fn list_devices(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let rows: Vec<Value> = st
        .ctx
        .store
        .devices()
        .await?
        .iter()
        .map(|d| {
            json!({
                "id": d.device.id, "name": d.device.name, "owner": d.owner_name,
                "created": format::date(&d.device.created_at),
                "last_seen": d.device.last_seen_at.as_deref().map(format::datetime).unwrap_or_else(|| "never".into()),
                "uploads": d.uploads, "revoked": d.device.revoked_at.is_some(),
            })
        })
        .collect();
    Ok(Json(json!({ "devices": rows })))
}

pub async fn revoke_device(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let d = st.ctx.store.device(&id).await?.ok_or(AppError::NotFound)?;
    st.ctx.store.revoke_device(&d.id).await?;
    tracing::info!(device = %d.id, name = %d.name, by = %me.name, "device revoked");
    Ok(Json(json!({ "ok": true })))
}

/// One of the app's installers as the server's pages offer it.
pub struct AppDownload {
    pub name: String,
    pub who: &'static str,
    pub url: Option<String>,
    pub size: String,
}

/// The installers of this server's version, each with who it is for and a link once it has it.
pub async fn downloads(cfg: &reel_core::Config) -> Vec<AppDownload> {
    use reel_core::packages;
    use reel_pipeline::app_update::{offer, Flavour};
    let mut out = Vec::new();
    for flavour in [
        Flavour::WithObs,
        Flavour::Plain,
        Flavour::AppImage,
        Flavour::Deb,
    ] {
        let have = offer(cfg, flavour).await;
        let size = match &have {
            Some(o) => tokio::fs::metadata(&o.installer)
                .await
                .map(|m| reel_core::format::size(m.len()))
                .unwrap_or_default(),
            None => String::new(),
        };
        out.push(AppDownload {
            name: match flavour {
                Flavour::WithObs => format!("{}, for Windows", packages::windows(true).name),
                Flavour::Plain => format!("{}, for Windows", packages::windows(false).name),
                Flavour::AppImage => "reel for Linux, the AppImage".into(),
                Flavour::Deb => "reel for Debian and Ubuntu, the .deb".into(),
            },
            who: match flavour {
                Flavour::WithObs => "everything in one install, OBS included and set up by the app: for a PC without OBS, or to let reel run its own",
                Flavour::Plain => "the app alone, for a PC with OBS set up already: it connects to that OBS",
                Flavour::AppImage => "one file for any distribution, with OBS from the distribution or Flathub: make it executable and run it; it updates itself from this server",
                Flavour::Deb => "the same app as a package, with OBS from the distribution or Flathub; it does not update itself: install each new version's .deb the same way",
            },
            url: have.map(|_| format!("/app/installer/{}", flavour.id())),
            size,
        });
    }
    out
}

/// An installer for a signed-in user, from the Devices page and the first run.
pub async fn installer_download(
    State(st): State<AppState>,
    Path(id): Path<String>,
    req: axum::extract::Request,
) -> AppResult<Response> {
    use reel_pipeline::app_update::{installer_name, offer, Flavour};
    let Some(flavour) = Flavour::from_id(&id) else {
        return Err(AppError::NotFound);
    };
    let o = offer(&st.ctx.config(), flavour)
        .await
        .ok_or(AppError::NotFound)?;
    let name = installer_name(&o.version, flavour);
    Ok(crate::serve_file::serve(
        req,
        &o.installer,
        mime::APPLICATION_OCTET_STREAM,
        Some(name),
    )
    .await)
}
