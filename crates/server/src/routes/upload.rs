use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::routes::html_status;
use crate::views::Who;
use crate::AppState;
use askama::Template;
use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Extension, Json};
use reel_pipeline::enqueue::{create_clip_for_file, NewClip};
use reel_pipeline::hash::looks_like_sha256;
use reel_store::models::WebUpload;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;

pub async fn upload(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    headers: HeaderMap,
    multipart: Multipart,
) -> AppResult<Json<Value>> {
    let clips = receive(&st, &me, &headers, multipart).await?;
    Ok(Json(json!({ "clips": clips })))
}

async fn receive(
    st: &AppState,
    me: &User,
    headers: &HeaderMap,
    mut multipart: Multipart,
) -> AppResult<Vec<Value>> {
    crate::error::library_ready(&st.ctx).await?;
    let cfg = st.ctx.config();
    let length: u64 = headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if length > cfg.upload_max_bytes() as u64 {
        return Err(AppError::TooLarge(format!(
            "an upload over {} GB is refused; the limit is under Settings, Imports",
            cfg.import.upload_max_gb
        )));
    }
    let mut clips = Vec::new();
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let Some(file_name) = field.file_name().map(|s| s.to_string()) else {
            continue;
        };
        let name = std::path::Path::new(&file_name)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("upload.bin")
            .to_string();
        if !st
            .ctx
            .config()
            .accepts_extension(std::path::Path::new(&name))
        {
            return Err(AppError::BadRequest(format!(
                "{name}: not a clip we take ({})",
                st.ctx.config().ingest.extensions.join(", ")
            )));
        }
        let dir = st.ctx.config().uploads_dir().join(reel_core::ids::new_id());
        tokio::fs::create_dir_all(&dir).await?;
        let target = dir.join(&name);
        let part = reel_media::ffmpeg::part_path(&target);
        let mut f = tokio::fs::File::create(&part).await?;
        let mut bytes: u64 = 0;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?
        {
            bytes += chunk.len() as u64;
            f.write_all(&chunk).await?;
        }
        f.flush().await?;
        drop(f);
        if bytes == 0 {
            let _ = tokio::fs::remove_dir_all(&dir).await;
            return Err(AppError::BadRequest(format!("{name} is empty")));
        }
        tokio::fs::rename(&part, &target).await?;
        let (clip, job) = create_clip_for_file(
            &st.ctx,
            NewClip {
                owner_id: Some(me.id.clone()),
                ..NewClip::new(&target, "upload")
            },
        )
        .await?;
        clips.push(json!({ "id": clip.id, "title": clip.title, "job_id": job.id, "bytes": bytes }));
    }
    if clips.is_empty() {
        return Err(AppError::BadRequest("no file in the upload".into()));
    }
    Ok(clips)
}

#[derive(Template)]
#[template(path = "share-target.html")]
pub struct ShareTargetPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub extensions: String,
    pub limit_bytes: usize,
    pub limit_gb: u64,
    pub refused: Option<String>,
}

async fn share_target_page(
    st: &AppState,
    me: &User,
    refused: Option<String>,
) -> AppResult<ShareTargetPage> {
    let cfg = st.ctx.config();
    Ok(ShareTargetPage {
        who: Who::at(st, me).await,
        nav: "share-target",
        q: String::new(),
        settings_json: crate::settings::client_json(st).await?,
        extensions: cfg
            .ingest
            .extensions
            .iter()
            .map(|e| format!(".{e}"))
            .collect::<Vec<_>>()
            .join(","),
        limit_bytes: cfg.upload_max_bytes(),
        limit_gb: cfg.import.upload_max_gb,
        refused,
    })
}

pub async fn share_target(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Response> {
    Ok(crate::routes::html(
        share_target_page(&st, &me, None).await?,
    ))
}

pub async fn share_target_post(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    headers: HeaderMap,
    multipart: Multipart,
) -> AppResult<Response> {
    let (status, line) = match receive(&st, &me, &headers, multipart).await {
        Ok(clips) => {
            let to = match clips.as_slice() {
                [one] => format!("/clips/{}", one["id"].as_str().unwrap_or_default()),
                _ => "/".into(),
            };
            return Ok(Redirect::to(&to).into_response());
        }
        Err(AppError::TooLarge(m)) => (StatusCode::PAYLOAD_TOO_LARGE, m),
        Err(AppError::BadRequest(m)) => (StatusCode::BAD_REQUEST, m),
        Err(e) => return Err(e),
    };
    Ok(html_status(
        status,
        share_target_page(&st, &me, Some(line)).await?,
    ))
}

#[derive(Deserialize)]
pub struct NewPieces {
    pub file_name: String,
    pub size: i64,
    /// The browser's word for the file (its size and time): the same file picked again
    /// continues the same upload.
    #[serde(default)]
    pub file_key: String,
}

fn web_dir(st: &AppState, id: &str) -> std::path::PathBuf {
    st.ctx.config().web_uploads_dir().join(id)
}

fn pieces(st: &AppState, u: &WebUpload) -> super::pieces::Pieces {
    super::pieces::Pieces {
        kind: super::pieces::Kind::Web,
        id: u.id.clone(),
        part: reel_media::ffmpeg::part_path(&web_dir(st, &u.id).join(&u.file_name)),
        size: u.size,
        received: u.received,
    }
}

async fn own(st: &AppState, me: &User, id: &str) -> AppResult<WebUpload> {
    st.ctx
        .store
        .web_upload(id)
        .await?
        .filter(|u| u.user_id == me.id)
        .ok_or(AppError::NotFound)
}

/// Starts an upload, or goes on with the user's open one of the same file.
pub async fn create_pieces(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<NewPieces>,
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
        return Err(AppError::BadRequest(format!("{name} is empty")));
    }
    if body.size as u64 > cfg.upload_max_bytes() as u64 {
        return Err(AppError::TooLarge(format!(
            "an upload over {} GB is refused; the limit is under Settings, Imports",
            cfg.import.upload_max_gb
        )));
    }
    let key: String = body.file_key.trim().chars().take(100).collect();
    if let Some(open) = st
        .ctx
        .store
        .open_web_upload(&me.id, &name, body.size, &key)
        .await?
    {
        let p = pieces(&st, &open);
        let have = tokio::fs::metadata(&p.part)
            .await
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        let offset = have.min(open.received);
        if offset != open.received {
            st.ctx.store.web_upload_received(&open.id, offset).await?;
        }
        return Ok(Json(json!({ "status": "resumed", "upload_id": open.id, "offset": offset, "chunk_bytes": cfg.device_chunk_bytes() }))
            .into_response());
    }
    let u = WebUpload::new(&me.id, &name, body.size, &key);
    tokio::fs::create_dir_all(web_dir(&st, &u.id)).await?;
    tokio::fs::File::create(pieces(&st, &u).part).await?;
    st.ctx.store.insert_web_upload(&u).await?;
    tracing::info!(user = %me.id, upload = %u.id, name, size = u.size, "upload from the browser started");
    Ok((
        StatusCode::CREATED,
        Json(json!({ "status": "created", "upload_id": u.id, "offset": 0, "chunk_bytes": cfg.device_chunk_bytes() })),
    )
        .into_response())
}

pub async fn put_piece(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    Query(q): Query<super::device::ChunkQuery>,
    headers: HeaderMap,
    body: Body,
) -> AppResult<Response> {
    let u = own(&st, &me, &id).await?;
    if u.state != "open" {
        return Err(AppError::Conflict(format!("this upload is {}", u.state)));
    }
    super::pieces::put(&st, &pieces(&st, &u), q.offset, &headers, body).await
}

pub async fn pieces_status(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let u = own(&st, &me, &id).await?;
    Ok(Json(
        json!({ "upload_id": u.id, "offset": u.received, "size": u.size, "state": u.state, "clip_id": u.clip_id }),
    ))
}

pub async fn abandon_pieces(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let u = own(&st, &me, &id).await?;
    if u.state == "open" {
        let _ = tokio::fs::remove_dir_all(web_dir(&st, &u.id)).await;
        st.ctx.store.delete_web_upload(&u.id).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct Finish {
    pub sha256: String,
}

fn clip_answer(status: &str, clip: &reel_store::models::Clip, job_id: Option<&str>) -> Value {
    json!({ "status": status, "clip": { "id": clip.id, "title": clip.title, "state": clip.state, "job_id": job_id } })
}

/// The SHA-256 of the whole file checked against what arrived; then the clip, or the one
/// the library has of that file already (the same file twice stays one clip).
pub async fn finish_pieces(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<Finish>,
) -> AppResult<Response> {
    let u = own(&st, &me, &id).await?;
    if u.state == "done" {
        if let Some(clip) = match &u.clip_id {
            Some(cid) => st.ctx.store.clip(cid).await?,
            None => None,
        } {
            return Ok(Json(clip_answer("done", &clip, None)).into_response());
        }
        return Err(AppError::Conflict(
            "this upload is done, but its clip is gone".into(),
        ));
    }
    if u.state != "open" {
        return Err(AppError::Conflict(format!("this upload is {}", u.state)));
    }
    let sha = body.sha256.trim().to_ascii_lowercase();
    if !looks_like_sha256(&sha) {
        return Err(AppError::BadRequest(
            "sha256 is 64 hex characters of the whole file".into(),
        ));
    }
    let p = pieces(&st, &u);
    if let Err(answer) = super::pieces::complete(&st, &p, &sha).await? {
        return Ok(answer);
    }
    if let Some(first) = st.ctx.store.clip_by_source_hash(&sha, None).await? {
        let _ = tokio::fs::remove_dir_all(web_dir(&st, &u.id)).await;
        st.ctx.store.web_upload_done(&u.id, &first.id).await?;
        tracing::info!(user = %me.id, upload = %u.id, clip = %first.id, "upload from the browser: the library has this file");
        return Ok(Json(clip_answer("exists", &first, None)).into_response());
    }
    let target = web_dir(&st, &u.id).join(&u.file_name);
    tokio::fs::rename(&p.part, &target).await?;
    let (clip, job) = create_clip_for_file(
        &st.ctx,
        NewClip {
            owner_id: Some(me.id.clone()),
            source_hash: Some(sha),
            ..NewClip::new(&target, "upload")
        },
    )
    .await?;
    st.ctx.store.web_upload_done(&u.id, &clip.id).await?;
    tracing::info!(user = %me.id, upload = %u.id, clip = %clip.id, job = %job.id, "upload from the browser finished, ingest queued");
    Ok(Json(clip_answer("created", &clip, Some(&job.id))).into_response())
}
