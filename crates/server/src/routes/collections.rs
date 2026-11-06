use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::routes::html;
use crate::routes::share::{
    gone, render, send_download, send_poster, send_video, send_web, visitor, web_state, Served,
};
use crate::views::{card, card_for, Card, Who};
use crate::AppState;
use askama::Template;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use reel_core::format;
use reel_core::visitors::Visitor;
use reel_store::models::{Clip, Collection, CollectionLink, Rendition};
use serde::Deserialize;
use serde_json::{json, Value};

fn link_url(st: &AppState, token: &str) -> String {
    format!(
        "{}/c/{token}",
        st.ctx.config().public_url.trim_end_matches('/')
    )
}

async fn owned(st: &AppState, me: &User, id: &str) -> AppResult<Collection> {
    let c = st
        .ctx
        .store
        .collection(id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !me.may_edit(c.owner_id.as_deref()) {
        return Err(AppError::Forbidden(
            "only who made this collection, or an admin, changes it".into(),
        ));
    }
    Ok(c)
}

/// A collection the user may change by hand; a game's follows the clips' games instead.
pub(crate) async fn by_hand(st: &AppState, me: &User, id: &str) -> AppResult<Collection> {
    let c = owned(st, me, id).await?;
    if c.is_game() {
        return Err(AppError::BadRequest(format!(
            "{} is the game's own collection: it holds every clip with that game; change a clip's game to take it out",
            c.name
        )));
    }
    Ok(c)
}

pub(crate) fn clean_name(raw: &str) -> AppResult<String> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::BadRequest(
            "a collection's name is 1 to 80 characters".into(),
        ));
    }
    Ok(name.to_string())
}

pub struct SetView {
    pub id: String,
    pub name: String,
    pub clips: String,
    pub link: Option<String>,
    pub game: bool,
    pub newest: String,
    pub posters: Vec<String>,
}

#[derive(Template)]
#[template(path = "collections.html")]
pub struct CollectionsPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub sets: Vec<SetView>,
}

fn clips_word(n: i64) -> String {
    if n == 1 {
        "1 clip".into()
    } else {
        format!("{n} clips")
    }
}

pub async fn list_page(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Response> {
    let mut sets = Vec::new();
    for c in st.ctx.store.collections().await? {
        let posters = st
            .ctx
            .store
            .collection_clips(&c.collection.id)
            .await?
            .iter()
            .filter_map(|r| card(r).poster)
            .take(4)
            .collect();
        sets.push(SetView {
            link: c.token.as_deref().map(|t| link_url(&st, t)),
            clips: clips_word(c.clips),
            game: c.collection.is_game(),
            newest: c.newest.as_deref().map(format::when).unwrap_or_default(),
            posters,
            id: c.collection.id,
            name: c.collection.name,
        });
    }
    Ok(html(CollectionsPage {
        who: Who::at(&st, &me).await,
        nav: "collections",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        sets,
    }))
}

#[derive(Template)]
#[template(path = "collection.html")]
pub struct CollectionPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub id: String,
    pub name: String,
    pub count: String,
    pub link: Option<String>,
    pub editable: bool,
    pub can_link: bool,
    pub game: bool,
    pub cards: Vec<Card>,
    pub show_owners: bool,
    /// The clips the link leaves out (or a link made now would): not its maker's to share.
    pub left_out: usize,
}

pub async fn page(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let row = st
        .ctx
        .store
        .collections()
        .await?
        .into_iter()
        .find(|c| c.collection.id == id)
        .ok_or(AppError::NotFound)?;
    let starred = st.ctx.store.starred_ids(&me.id).await?;
    let cards = st
        .ctx
        .store
        .collection_clips(&id)
        .await?
        .iter()
        .map(|r| {
            let mut c = card_for(r, &me);
            c.starred = starred.contains(&r.clip.id);
            c
        })
        .collect();
    let can_link = me.may_edit(row.collection.owner_id.as_deref());
    let sharer = match row.token.as_deref() {
        Some(t) => match st.ctx.store.collection_link_by_token(t).await? {
            Some(l) => crate::rights::sharer_of_link(&st.ctx.store, &l, &row.collection).await?,
            None => crate::rights::Sharer::of(&me),
        },
        None => crate::rights::Sharer::of(&me),
    };
    let left_out = st
        .ctx
        .store
        .collection_clips(&id)
        .await?
        .iter()
        .filter(|r| !sharer.may_share(&r.clip))
        .count();
    Ok(html(CollectionPage {
        who: Who::at(&st, &me).await,
        nav: "collections",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        editable: can_link && !row.collection.is_game(),
        can_link,
        game: row.collection.is_game(),
        link: row.token.as_deref().map(|t| link_url(&st, t)),
        count: clips_word(row.clips),
        id: row.collection.id,
        name: row.collection.name,
        cards,
        show_owners: st.ctx.store.user_count().await? > 1,
        left_out,
    }))
}

#[derive(Deserialize)]
pub struct NameBody {
    pub name: String,
}

/// The collections, each saying whether the user may put clips in it (never a game's).
pub async fn list(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    let all: Vec<Value> = st
        .ctx
        .store
        .collections()
        .await?
        .into_iter()
        .map(|c| {
            let open = !c.collection.is_game() && me.may_edit(c.collection.owner_id.as_deref());
            json!({ "id": c.collection.id, "name": c.collection.name, "clips": c.clips, "open": open })
        })
        .collect();
    Ok(Json(json!({ "collections": all })))
}

pub async fn create(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<NameBody>,
) -> AppResult<Json<Value>> {
    let c = Collection::new(&clean_name(&body.name)?, Some(me.id.clone()));
    st.ctx.store.insert_collection(&c).await?;
    Ok(Json(json!({ "id": c.id, "name": c.name })))
}

pub async fn rename(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<NameBody>,
) -> AppResult<Json<Value>> {
    by_hand(&st, &me, &id).await?;
    let name = clean_name(&body.name)?;
    st.ctx.store.rename_collection(&id, &name).await?;
    Ok(Json(json!({ "id": id, "name": name })))
}

pub async fn delete(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    by_hand(&st, &me, &id).await?;
    st.ctx.store.delete_collection(&id).await?;
    crate::routes::api::sweep_web_later(&st);
    st.ctx.external.kick();
    Ok(Json(json!({ "ok": true })))
}

pub async fn add_clip(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path((id, clip_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    by_hand(&st, &me, &id).await?;
    st.ctx
        .store
        .clip(&clip_id)
        .await?
        .filter(|c| c.deleted_at.is_none())
        .ok_or(AppError::NotFound)?;
    st.ctx.store.add_to_collection(&id, &clip_id).await?;
    st.ctx.external.kick();
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_clip(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path((id, clip_id)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    by_hand(&st, &me, &id).await?;
    st.ctx.store.remove_from_collection(&id, &clip_id).await?;
    crate::routes::api::sweep_web_later(&st);
    st.ctx.external.kick();
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
pub struct LinkBody {
    #[serde(default)]
    pub allow_download: Option<bool>,
}

/// One live link per collection: a new one retires the one before.
pub async fn make_link(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    body: Option<Json<LinkBody>>,
) -> AppResult<Json<Value>> {
    owned(&st, &me, &id).await?;
    let allow = body
        .and_then(|b| b.0.allow_download)
        .unwrap_or(st.ctx.config().share.download_default);
    st.ctx.store.revoke_collection_links(&id).await?;
    let l = CollectionLink::new(&id, allow, &me.id);
    st.ctx.store.insert_collection_link(&l).await?;
    tracing::info!(collection = %id, token = %&l.token[..4], "collection link made");
    let sharer = crate::rights::Sharer::of(&me);
    let mut left_out = 0;
    for row in st.ctx.store.collection_clips(&id).await? {
        if !sharer.may_share(&row.clip) {
            left_out += 1;
            continue;
        }
        if let Some(r) = current_of(&st, &row.clip).await {
            let _ = reel_pipeline::enqueue::ensure_web(&st.ctx, &r).await;
        }
    }
    st.ctx.external.kick();
    Ok(Json(
        json!({ "url": link_url(&st, &l.token), "left_out": left_out }),
    ))
}

pub async fn revoke_link(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    owned(&st, &me, &id).await?;
    st.ctx.store.revoke_collection_links(&id).await?;
    crate::routes::api::sweep_web_later(&st);
    st.ctx.external.kick();
    Ok(Json(json!({ "ok": true })))
}

pub struct Tile {
    pub href: String,
    pub poster: String,
    pub title: String,
    pub duration: String,
}

#[derive(Template)]
#[template(path = "collection-share.html")]
pub struct CollectionSharePage {
    pub site_name: String,
    pub name: String,
    pub count: String,
    pub page_url: String,
    pub image: Option<String>,
    pub tiles: Vec<Tile>,
}

async fn link_of(st: &AppState, token: &str) -> Option<(CollectionLink, Collection)> {
    if !reel_core::ids::looks_like_token(token) {
        return None;
    }
    let link = st.ctx.store.collection_link_by_token(token).await.ok()??;
    let c = st.ctx.store.collection(&link.collection_id).await.ok()??;
    Some((link, c))
}

async fn current_of(st: &AppState, clip: &Clip) -> Option<Rendition> {
    if clip.deleted_at.is_some() || clip.state != "ready" {
        return None;
    }
    let r = st
        .ctx
        .store
        .rendition(clip.current_id.as_deref()?)
        .await
        .ok()??;
    r.is_ready().then_some(r)
}

/// A collection link's page, or one of its clips', was asked for: posted or opened.
async fn count_visit(st: &AppState, link: &CollectionLink, headers: &HeaderMap) {
    let _ = match visitor(headers) {
        Visitor::Previewer => st.ctx.store.mark_collection_link_posted(&link.id).await,
        Visitor::Person => st.ctx.store.bump_collection_link_views(&link.id).await,
        Visitor::Robot => Ok(()),
    };
}

pub async fn public_page(
    State(st): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some((link, c)) = link_of(&st, &token).await else {
        return gone();
    };
    count_visit(&st, &link, &headers).await;
    let Ok(sharer) = crate::rights::sharer_of_link(&st.ctx.store, &link, &c).await else {
        return gone();
    };
    let base = st.ctx.config().public_url.trim_end_matches('/').to_string();
    let mut tiles = Vec::new();
    for row in st
        .ctx
        .store
        .collection_clips(&c.id)
        .await
        .unwrap_or_default()
    {
        if !sharer.may_share(&row.clip) {
            continue;
        }
        let Some(r) = current_of(&st, &row.clip).await else {
            continue;
        };
        tiles.push(Tile {
            href: format!("/c/{token}/{}", row.clip.id),
            poster: format!(
                "{base}/c/{token}/{}/poster.jpg?r={}&t={}",
                row.clip.id,
                r.id,
                row.clip.poster_key()
            ),
            title: row.clip.title.clone(),
            duration: format::duration(r.duration.unwrap_or(0.0)),
        });
    }
    if c.is_game() && tiles.is_empty() {
        return gone();
    }
    html(CollectionSharePage {
        site_name: st.ctx.config().site_name.clone(),
        image: tiles.first().map(|t| t.poster.clone()),
        count: clips_word(tiles.len() as i64),
        page_url: format!("{base}/c/{token}"),
        name: c.name,
        tiles,
    })
}

async fn served_in(st: &AppState, token: &str, clip_id: &str) -> Option<Served> {
    let (link, c) = link_of(st, token).await?;
    let sets = st.ctx.store.collections_of_clip(clip_id).await.ok()?;
    if !sets.contains(&c.id) {
        return None;
    }
    let clip = st.ctx.store.clip(clip_id).await.ok()??;
    let sharer = crate::rights::sharer_of_link(&st.ctx.store, &link, &c)
        .await
        .ok()?;
    if !sharer.may_share(&clip) {
        return None;
    }
    let served = current_of(st, &clip).await?;
    Some(Served {
        clip,
        served,
        allow_download: link.allow_download == 1,
        expires_at: None,
        suffix: String::new(),
    })
}

pub async fn public_clip(
    State(st): State<AppState>,
    Path((token, clip_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    match served_in(&st, &token, &clip_id).await {
        Some(s) => {
            if let Some((link, _)) = link_of(&st, &token).await {
                count_visit(&st, &link, &headers).await;
            }
            render(&st, &s, &format!("/c/{token}/{clip_id}")).await
        }
        None => gone(),
    }
}

pub async fn public_video(
    State(st): State<AppState>,
    Path((token, clip_id)): Path<(String, String)>,
    req: Request,
) -> Response {
    match served_in(&st, &token, &clip_id).await {
        Some(s) => send_video(&st, &s.served, req).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn public_web(
    State(st): State<AppState>,
    Path((token, clip_id)): Path<(String, String)>,
    req: Request,
) -> Response {
    match served_in(&st, &token, &clip_id).await {
        Some(s) => send_web(&st, &s.served, req).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn public_web_json(
    State(st): State<AppState>,
    Path((token, clip_id)): Path<(String, String)>,
) -> Response {
    match served_in(&st, &token, &clip_id).await {
        Some(s) => web_state(&st, &s.served, &format!("/c/{token}/{clip_id}")).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn public_poster(
    State(st): State<AppState>,
    Path((token, clip_id)): Path<(String, String)>,
    req: Request,
) -> Response {
    match served_in(&st, &token, &clip_id).await {
        Some(s) => send_poster(&st, &s.served, req).await,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn public_download(
    State(st): State<AppState>,
    Path((token, clip_id)): Path<(String, String)>,
    req: Request,
) -> Response {
    match served_in(&st, &token, &clip_id).await {
        Some(s) => send_download(&st, &s, req).await,
        None => gone(),
    }
}
