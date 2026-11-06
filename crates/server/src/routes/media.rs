use crate::error::{AppError, AppResult};
use crate::serve_file::{attachment_name, serve};
use crate::AppState;
use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use reel_pipeline::enqueue::ensure_discord;
use serde::Deserialize;
use serde_json::json;

fn video_mime() -> mime::Mime {
    "video/mp4".parse().unwrap()
}
fn jpeg_mime() -> mime::Mime {
    mime::IMAGE_JPEG
}

pub async fn current(
    State(st): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> AppResult<Response> {
    let clip = st.ctx.store.clip(&id).await?.ok_or(AppError::NotFound)?;
    let rid = clip.current_id.ok_or(AppError::NotFound)?;
    let r = st
        .ctx
        .store
        .rendition(&rid)
        .await?
        .filter(|r| r.is_ready())
        .ok_or(AppError::NotFound)?;
    Ok(serve(
        req,
        &st.ctx.config().abs(r.path.as_deref().unwrap()),
        video_mime(),
        None,
    )
    .await)
}

pub async fn download(
    State(st): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> AppResult<Response> {
    let clip = st.ctx.store.clip(&id).await?.ok_or(AppError::NotFound)?;
    let rid = clip.current_id.clone().ok_or(AppError::NotFound)?;
    let r = st
        .ctx
        .store
        .rendition(&rid)
        .await?
        .filter(|r| r.is_ready())
        .ok_or(AppError::NotFound)?;
    Ok(serve(
        req,
        &st.ctx.config().abs(r.path.as_deref().unwrap()),
        video_mime(),
        Some(attachment_name(&clip.title, "mp4")),
    )
    .await)
}

pub async fn original(
    State(st): State<AppState>,
    Path(id): Path<String>,
    req: Request,
) -> AppResult<Response> {
    let clip = st.ctx.store.clip(&id).await?.ok_or(AppError::NotFound)?;
    let path = st
        .ctx
        .config()
        .abs(clip.original_path.as_deref().ok_or(AppError::NotFound)?);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("mkv");
    let stem = clip
        .source_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(&clip.source_name);
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    Ok(serve(
        req,
        &path,
        mime,
        Some(attachment_name(&format!("{stem} raw"), ext)),
    )
    .await)
}

pub async fn rendition_download(
    State(st): State<AppState>,
    Path(rid): Path<String>,
    req: Request,
) -> AppResult<Response> {
    let r = st
        .ctx
        .store
        .rendition(&rid)
        .await?
        .filter(|r| r.is_ready() && (r.is_version() || r.kind == "anim"))
        .ok_or(AppError::NotFound)?;
    let clip = st
        .ctx
        .store
        .clip(&r.clip_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let path = st.ctx.config().abs(r.path.as_deref().unwrap());
    if r.kind == "anim" {
        return Ok(anim_download(&st, &clip, &r, path, req).await);
    }
    let name = attachment_name(&format!("{} {}", clip.title, r.label()), "mp4");
    Ok(serve(req, &path, video_mime(), Some(name)).await)
}

/// A made picture from the list under Send, named for its version as Make and download names it.
async fn anim_download(
    st: &AppState,
    clip: &reel_store::models::Clip,
    r: &reel_store::models::Rendition,
    path: std::path::PathBuf,
    req: Request,
) -> Response {
    use reel_core::recipe::{AnimFormat, AnimRecipe};
    let format = r
        .recipe
        .as_deref()
        .and_then(|j| serde_json::from_str::<AnimRecipe>(j).ok())
        .map(|a| a.format)
        .unwrap_or(AnimFormat::Gif);
    let from = match r.derived_from.as_deref() {
        Some(f) => st.ctx.store.rendition(f).await.ok().flatten(),
        None => None,
    };
    let stem = match from {
        Some(v) if clip.current_id.as_deref() != Some(v.id.as_str()) => {
            format!("{} {}", clip.title, v.label())
        }
        _ => clip.title.clone(),
    };
    serve(
        req,
        &path,
        format.mime().parse().unwrap(),
        Some(attachment_name(&stem, format.ext())),
    )
    .await
}

#[derive(Deserialize)]
pub struct DiscordQuery {
    pub limit: Option<u64>,
}

pub async fn discord(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<DiscordQuery>,
    req: Request,
) -> AppResult<Response> {
    let clip = st.ctx.store.clip(&id).await?.ok_or(AppError::NotFound)?;
    let rid = clip.current_id.clone().ok_or(AppError::NotFound)?;
    let r = st
        .ctx
        .store
        .rendition(&rid)
        .await?
        .filter(|r| r.is_ready())
        .ok_or(AppError::NotFound)?;
    let settings = crate::settings::client(&st.ctx.config());
    let limit_mb = q.limit.unwrap_or(settings.discord_limit_mb);
    if !st.ctx.config().share.discord_limits_mb.contains(&limit_mb) {
        return Err(AppError::BadRequest(format!(
            "limit must be one of {:?}",
            st.ctx.config().share.discord_limits_mb
        )));
    }
    let limit_bytes = limit_mb * 1_000_000;
    if r.size.unwrap_or(0) as u64 <= limit_bytes {
        let name = attachment_name(&format!("{} discord", clip.title), "mp4");
        return Ok(serve(
            req,
            &st.ctx.config().abs(r.path.as_deref().unwrap()),
            video_mime(),
            Some(name),
        )
        .await);
    }
    let copy = ensure_discord(&st.ctx, &r, limit_bytes)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    if copy.is_ready() {
        let name = attachment_name(&format!("{} {}mb", clip.title, limit_mb), "mp4");
        return Ok(serve(
            req,
            &st.ctx.config().abs(copy.path.as_deref().unwrap()),
            video_mime(),
            Some(name),
        )
        .await);
    }
    let job = st.ctx.store.active_job_for_rendition(&copy.id).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "state": copy.state, "rendition_id": copy.id, "job_id": job.map(|j| j.id), "limit_mb": limit_mb }))).into_response())
}

#[derive(Deserialize)]
pub struct AnimQuery {
    pub w: Option<u32>,
    pub fps: Option<u32>,
}

/// An animated GIF of the clip's current version, made once and kept.
pub async fn anim_gif(
    st: State<AppState>,
    id: Path<String>,
    q: Query<AnimQuery>,
    req: Request,
) -> AppResult<Response> {
    anim(st, id, q, req, reel_core::recipe::AnimFormat::Gif).await
}

/// An animated WebP of it, where this ffmpeg has libwebp_anim.
pub async fn anim_webp(
    st: State<AppState>,
    id: Path<String>,
    q: Query<AnimQuery>,
    req: Request,
) -> AppResult<Response> {
    anim(st, id, q, req, reel_core::recipe::AnimFormat::Webp).await
}

/// The picture when it is made, else 202 with the rendition and the job to follow.
async fn anim(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<AnimQuery>,
    req: Request,
    format: reel_core::recipe::AnimFormat,
) -> AppResult<Response> {
    use reel_core::recipe::{AnimFormat, AnimRecipe, ANIM_MAX_SECONDS};
    let clip = st.ctx.store.clip(&id).await?.ok_or(AppError::NotFound)?;
    let rid = clip.current_id.clone().ok_or(AppError::NotFound)?;
    let r = st
        .ctx
        .store
        .rendition(&rid)
        .await?
        .filter(|r| r.is_ready())
        .ok_or(AppError::NotFound)?;
    if format == AnimFormat::Webp && !st.ctx.hw.read().await.anim_webp {
        return Err(AppError::BadRequest(
            "this server's ffmpeg has no WebP encoder (libwebp_anim); a GIF it makes".into(),
        ));
    }
    let duration = r.duration.unwrap_or(0.0);
    if duration > ANIM_MAX_SECONDS + 0.05 {
        return Err(AppError::BadRequest(format!(
            "the clip is {} long and an animated picture is {ANIM_MAX_SECONDS} s at most: trim it in the editor first",
            reel_core::format::duration(duration)
        )));
    }
    let default_fps = if format == AnimFormat::Gif { 12 } else { 24 };
    let recipe = AnimRecipe::new(format, q.w.unwrap_or(480), q.fps.unwrap_or(default_fps))
        .ok_or_else(|| {
            AppError::BadRequest("the width is 480 or 720 and the rate 12 or 24".into())
        })?;
    let made = reel_pipeline::enqueue::ensure_anim(&st.ctx, &r, recipe).await?;
    if made.is_ready() {
        let name = attachment_name(&clip.title, format.ext());
        return Ok(serve(
            req,
            &st.ctx.config().abs(made.path.as_deref().unwrap()),
            format.mime().parse().unwrap(),
            Some(name),
        )
        .await);
    }
    let job = st.ctx.store.active_job_for_rendition(&made.id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "state": made.state, "rendition_id": made.id, "job_id": job.map(|j| j.id) })),
    )
        .into_response())
}

pub async fn rendition_file(
    State(st): State<AppState>,
    Path(file): Path<String>,
    req: Request,
) -> AppResult<Response> {
    let (rid, suffix) = file.split_once('.').ok_or(AppError::NotFound)?;
    if !reel_core::ids::looks_like_id(rid) {
        return Err(AppError::NotFound);
    }
    let r = st
        .ctx
        .store
        .rendition(rid)
        .await?
        .filter(|r| r.is_ready())
        .ok_or(AppError::NotFound)?;
    let video = in_place_or_trash(&st, &r.clip_id, r.path.as_deref().unwrap());
    let d = reel_pipeline::derived::paths(&video);
    let (path, mime) = match suffix {
        "mp4" => (video, video_mime()),
        "jpg" => (d.poster, jpeg_mime()),
        "loop.mp4" => (d.hover, video_mime()),
        "strip.jpg" => (d.strip, jpeg_mime()),
        _ => return Err(AppError::NotFound),
    };
    Ok(serve(req, &path, mime, None).await)
}

/// A version's file where it is, or in the trash with its clip, for the trash page's posters.
fn in_place_or_trash(st: &AppState, clip_id: &str, rel: &str) -> std::path::PathBuf {
    let cfg = st.ctx.config();
    let video = cfg.abs(rel);
    if video.exists() {
        return video;
    }
    match rel.strip_prefix(&format!("library/{clip_id}/")) {
        Some(rest) => reel_pipeline::library::trashed_dir(&cfg, clip_id).join(rest),
        None => video,
    }
}

/// One audio track of a version as its own file, for the editor's preview of the tracks: a
/// stream copy made on the first ask and kept beside the version as `<name>.a<n>.m4a`.
pub async fn rendition_track(
    State(st): State<AppState>,
    Path((rid, file)): Path<(String, String)>,
    req: Request,
) -> AppResult<Response> {
    let n: usize = file
        .strip_suffix(".m4a")
        .and_then(|n| n.parse().ok())
        .ok_or(AppError::NotFound)?;
    if !reel_core::ids::looks_like_id(&rid) || n >= reel_core::recipe::TRACKS_MAX {
        return Err(AppError::NotFound);
    }
    let r = st
        .ctx
        .store
        .rendition(&rid)
        .await?
        .filter(|r| r.is_ready() && r.is_version() && r.acodec.is_some())
        .ok_or(AppError::NotFound)?;
    let video = st.ctx.config().abs(r.path.as_deref().unwrap());
    let out = video.with_extension(format!("a{n}.m4a"));
    if !out.exists() {
        if n >= st.ctx.ff.audio_streams(&video).await.unwrap_or(0) {
            return Err(AppError::NotFound);
        }
        let args = vec![
            "-i".to_string(),
            video.to_string_lossy().into_owned(),
            "-map".into(),
            format!("0:a:{n}"),
            "-c".into(),
            "copy".into(),
            "-map_metadata".into(),
            "-1".into(),
            "-movflags".into(),
            "+faststart".into(),
            "-f".into(),
            "mp4".into(),
        ];
        st.ctx
            .ff
            .run_to(&args, &out, r.duration.unwrap_or(1.0), None)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("the track was not extracted: {e}")))?;
    }
    Ok(serve(req, &out, "audio/mp4".parse().unwrap(), None).await)
}
