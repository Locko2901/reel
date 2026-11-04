use crate::routes::{html, html_status};
use crate::serve_file::{attachment_name, serve};
use crate::AppState;
use askama::Template;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use reel_core::format;
use reel_core::visitors::Visitor;
use reel_pipeline::enqueue::{embed_grade, ensure_web, web_copy, WebCopy};
use reel_pipeline::external::{self, Where};
use reel_store::models::{Clip, Rendition, Share};
use serde_json::json;

#[derive(Template)]
#[template(path = "share.html")]
pub struct SharePage {
    pub site_name: String,
    pub title: String,
    pub description: String,
    pub page_url: String,
    pub video_url: String,
    pub play_url: String,
    pub web_wait: Option<String>,
    pub poster_url: String,
    pub width: i64,
    pub height: i64,
    pub duration: String,
    pub meta: Vec<String>,
    pub download_url: Option<String>,
    pub download_size: String,
    pub download_res: String,
    pub expiry: Option<String>,
    pub tint: Option<String>,
    pub fps: f64,
}

#[derive(Template)]
#[template(path = "gone.html")]
pub struct GonePage {
    pub head: &'static str,
    pub line: &'static str,
}

impl GonePage {
    pub const RETIRED: GonePage = GonePage {
        head: "This link has been retired.",
        line: "Ask whoever sent it for a fresh one.",
    };
    pub const PREPARING: GonePage = GonePage {
        head: "This clip is still being prepared.",
        line: "Try again in a minute.",
    };
}

pub fn gone() -> Response {
    html_status(StatusCode::NOT_FOUND, GonePage::RETIRED)
}

async fn lookup(st: &AppState, token: &str) -> Option<(Share, Clip, Rendition)> {
    if !reel_core::ids::looks_like_token(token) {
        return None;
    }
    let share = st.ctx.store.share_by_token(token).await.ok()??;
    if !share.is_active() {
        return None;
    }
    let clip = st.ctx.store.clip(&share.clip_id).await.ok()??;
    if clip.deleted_at.is_some() || clip.state != "ready" {
        return None;
    }
    let pinned = match &share.rendition_id {
        Some(rid) => st
            .ctx
            .store
            .rendition(rid)
            .await
            .ok()?
            .filter(|r| r.is_ready() && r.clip_id == clip.id),
        None => None,
    };
    let served = match pinned {
        Some(r) => r,
        None => st
            .ctx
            .store
            .rendition(clip.current_id.as_deref()?)
            .await
            .ok()??,
    };
    if !served.is_ready() {
        return None;
    }
    Some((share, clip, served))
}

/// What a public link plays: the clip, the version served, and what the link allows.
pub struct Served {
    pub clip: Clip,
    pub served: Rendition,
    pub allow_download: bool,
    pub expires_at: Option<String>,
    /// "?v=2" on a link copied for its version: the same link, a new address to a chat app,
    /// which builds a fresh preview for it.
    pub suffix: String,
}

#[derive(serde::Deserialize, Default)]
pub struct PageQuery {
    #[serde(default)]
    pub v: Option<String>,
}

/// Who asked for a link's page: an app building the link's preview means the link was posted
/// somewhere, a person's browser that it was opened.
pub fn visitor(headers: &HeaderMap) -> Visitor {
    reel_core::visitors::visitor(
        headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
    )
}

pub async fn page(
    State(st): State<AppState>,
    Path(token): Path<String>,
    axum::extract::Query(q): axum::extract::Query<PageQuery>,
    headers: HeaderMap,
) -> Response {
    let Some((share, clip, served)) = lookup(&st, &token).await else {
        return gone();
    };
    let _ = match visitor(&headers) {
        Visitor::Previewer => st.ctx.store.mark_posted(&share.id).await,
        Visitor::Person => st.ctx.store.bump_views(&share.id).await,
        Visitor::Robot => Ok(()),
    };
    let suffix =
        q.v.filter(|v| !v.is_empty() && v.len() <= 6 && v.bytes().all(|b| b.is_ascii_digit()))
            .map(|v| format!("?v={v}"))
            .unwrap_or_default();
    let s = Served {
        clip,
        served,
        allow_download: share.allow_download == 1,
        expires_at: share.expires_at,
        suffix,
    };
    render(&st, &s, &format!("/s/{}", share.token)).await
}

/// The preview a link's tags point at, and the file whose size they give. With the storage in
/// use, reel's own address, so a message posted before an edit or an eviction still plays.
async fn link_preview(
    st: &AppState,
    served: &Rendition,
    video_route: &str,
) -> Option<(String, Rendition)> {
    let place = external::where_from(&st.ctx, &served.id)
        .await
        .unwrap_or(Where::Off);
    let file = match &place {
        Where::Off => {
            let preview = embed_grade(&st.ctx, served).await.ok().flatten()?;
            return Some((format!("{video_route}?r={}", preview.id), preview));
        }
        Where::Storage(o) => st.ctx.store.rendition(&o.file_id).await.ok().flatten(),
        _ => from_here(st, served, &place).await,
    }?;
    Some((format!("{video_route}?r={}", served.id), file))
}

/// The preview served from here while the storage is in use: the copy at the server's size or,
/// until that is made, the storage's copy, which lies here too.
async fn from_here(st: &AppState, served: &Rendition, place: &Where) -> Option<Rendition> {
    let queue = !matches!(place, Where::Pending);
    if let Ok(Some(copy)) = external::server_preview(&st.ctx, served, queue).await {
        return Some(copy);
    }
    reel_pipeline::enqueue::ext_file_made(&st.ctx, served)
        .await
        .ok()?
        .ok()
        .flatten()
}

/// The share page of one clip, its files under `prefix` (a share link's or a collection link's).
pub async fn render(st: &AppState, s: &Served, prefix: &str) -> Response {
    let (clip, served) = (&s.clip, &s.served);
    let base = st.ctx.config().public_url.trim_end_matches('/').to_string();
    let Some((video_url, embed)) =
        link_preview(st, served, &format!("{base}{prefix}/video.mp4")).await
    else {
        return html_status(StatusCode::SERVICE_UNAVAILABLE, GonePage::PREPARING);
    };
    let page_url = format!("{base}{prefix}{}", s.suffix);
    let duration = format::duration(served.duration.unwrap_or(0.0));
    let web = match web_copy(&st.ctx, served).await {
        Ok(WebCopy::Missing) => {
            let _ = ensure_web(&st.ctx, served).await;
            WebCopy::Making
        }
        Ok(w) => w,
        Err(_) => WebCopy::Failed,
    };
    let (play_url, plays, web_wait) = match &web {
        WebCopy::Ready(w) => (format!("{prefix}/web.mp4?r={}", w.id), &**w, None),
        WebCopy::Making => (
            video_url.clone(),
            &embed,
            Some(format!("{prefix}/web.json")),
        ),
        _ => (video_url.clone(), &embed, None),
    };
    let mut meta = vec![duration.clone()];
    if let Some(h) = plays.height {
        meta.push(
            if plays.kind.starts_with("embed") && h < served.height.unwrap_or(0) {
                format!("{h}p preview")
            } else {
                format!("{h}p")
            },
        );
    }
    meta.push(format::when(
        clip.recorded_at.as_deref().unwrap_or(&clip.created_at),
    ));
    let tint = reel_pipeline::derived::read_tint(
        &st.ctx.config().abs(served.path.as_deref().unwrap_or("")),
    )
    .await
    .map(|[r, g, b]| format!("rgb({r} {g} {b})"));
    let expiry = s
        .expires_at
        .as_deref()
        .map(|e| format!("expires {}", format::until(e)));
    html(SharePage {
        site_name: st.ctx.config().site_name.clone(),
        title: clip.title.clone(),
        description: if clip.description.trim().is_empty() {
            meta.join(", ")
        } else {
            clip.description.clone()
        },
        page_url,
        video_url,
        play_url,
        web_wait,
        poster_url: format!(
            "{base}{prefix}/poster.jpg?r={}&t={}",
            served.id,
            clip.poster_key()
        ),
        width: embed.width.unwrap_or(1280),
        height: embed.height.unwrap_or(720),
        duration,
        meta,
        download_url: if s.allow_download {
            Some(format!("{prefix}/download"))
        } else {
            None
        },
        download_size: format::size(served.size.unwrap_or(0) as u64),
        download_res: served.height.map(|h| format!("{h}p")).unwrap_or_default(),
        expiry,
        tint,
        fps: served.fps.unwrap_or(60.0),
    })
}

/// A link's preview video: with the storage in use, a redirect to the object while it is there,
/// else the preview from here.
pub async fn send_video(st: &AppState, served: &Rendition, req: Request) -> Response {
    let place = external::where_from(&st.ctx, &served.id)
        .await
        .unwrap_or(Where::Off);
    let file = match &place {
        Where::Off => embed_grade(&st.ctx, served).await.ok().flatten(),
        Where::Storage(o) => {
            // its address is handed out now: it goes with the last link that serves it
            let _ = st.ctx.store.expose_external_object(&o.id).await;
            return (
                StatusCode::FOUND,
                [
                    (
                        header::LOCATION,
                        external::public_url(&st.ctx.config(), &o.key),
                    ),
                    (header::CACHE_CONTROL, "no-store".to_string()),
                ],
            )
                .into_response();
        }
        _ => from_here(st, served, &place).await,
    };
    let Some(file) = file else {
        return StatusCode::NOT_FOUND.into_response();
    };
    serve(
        req,
        &st.ctx.config().abs(file.path.as_deref().unwrap()),
        "video/mp4".parse().unwrap(),
        None,
    )
    .await
}

pub async fn send_web(st: &AppState, served: &Rendition, req: Request) -> Response {
    match web_copy(&st.ctx, served).await {
        Ok(WebCopy::Ready(w)) => {
            serve(
                req,
                &st.ctx.config().abs(w.path.as_deref().unwrap_or("")),
                "video/mp4".parse().unwrap(),
                None,
            )
            .await
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Whether the web copy is ready yet, for a page that plays the preview meanwhile.
pub async fn web_state(st: &AppState, served: &Rendition, prefix: &str) -> Response {
    match web_copy(&st.ctx, served).await {
        Ok(WebCopy::Ready(w)) => Json(json!({
            "ready": true,
            "url": format!("{prefix}/web.mp4?r={}", w.id),
            "height": w.height,
        }))
        .into_response(),
        Ok(WebCopy::Making) => Json(json!({ "ready": false })).into_response(),
        _ => Json(json!({ "ready": false, "gone": true })).into_response(),
    }
}

pub async fn send_poster(st: &AppState, served: &Rendition, req: Request) -> Response {
    let d = reel_pipeline::derived::paths(&st.ctx.config().abs(served.path.as_deref().unwrap()));
    serve(req, &d.poster, mime::IMAGE_JPEG, None).await
}

pub async fn send_download(st: &AppState, s: &Served, req: Request) -> Response {
    if !s.allow_download {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve(
        req,
        &st.ctx.config().abs(s.served.path.as_deref().unwrap()),
        "video/mp4".parse().unwrap(),
        Some(attachment_name(&s.clip.title, "mp4")),
    )
    .await
}

pub async fn video(
    State(st): State<AppState>,
    Path(token): Path<String>,
    req: Request,
) -> Response {
    let Some((_, _, served)) = lookup(&st, &token).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    send_video(&st, &served, req).await
}

pub async fn web(State(st): State<AppState>, Path(token): Path<String>, req: Request) -> Response {
    let Some((_, _, served)) = lookup(&st, &token).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    send_web(&st, &served, req).await
}

pub async fn web_json(State(st): State<AppState>, Path(token): Path<String>) -> Response {
    let Some((share, _, served)) = lookup(&st, &token).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    web_state(&st, &served, &format!("/s/{}", share.token)).await
}

pub async fn poster(
    State(st): State<AppState>,
    Path(token): Path<String>,
    req: Request,
) -> Response {
    let Some((_, _, served)) = lookup(&st, &token).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    send_poster(&st, &served, req).await
}

pub async fn download(
    State(st): State<AppState>,
    Path(token): Path<String>,
    req: Request,
) -> Response {
    let Some((share, clip, served)) = lookup(&st, &token).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let s = Served {
        clip,
        served,
        allow_download: share.allow_download == 1,
        expires_at: None,
        suffix: String::new(),
    };
    send_download(&st, &s, req).await
}
