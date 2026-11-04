use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::routes::html;
use crate::views::Who;
use crate::AppState;
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::{Extension, Json};
use reel_core::format;
use reel_core::recipe::{JoinKind, MontageFit, MontageJoin, MontageRecipe, MontageShape};
use serde::Deserialize;
use serde_json::{json, Value};

pub struct PartView {
    pub id: String,
    /// The current version, by which its keyframes are asked for.
    pub rid: String,
    pub title: String,
    pub poster: String,
    pub src: String,
    /// The whole version's length.
    pub seconds: f64,
    pub length: String,
    pub size: String,
    /// The part used: the whole clip unless the page opens on a montage.
    pub start: f64,
    pub end: f64,
    pub volume: f64,
    /// Ticked: in the montage (one left out of a montage being edited is not listed at all).
    pub used: bool,
}

#[derive(Template)]
#[template(path = "montage.html")]
pub struct MontagePage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub parts: Vec<PartView>,
    pub from: String,
    pub from_href: String,
    pub title: String,
    pub most: usize,
    pub longest: String,
    pub join: &'static str,
    pub join_seconds: f64,
    pub shape: &'static str,
    pub fill: bool,
    pub even_loudness: bool,
    /// Editing a montage the user may change: it can be replaced by the new one.
    pub replace_id: String,
    pub replace_title: String,
    /// Parts of the montage being edited that are gone or changed since, in words.
    pub notes: Vec<String>,
}

#[derive(Deserialize)]
pub struct MontageAsked {
    /// A collection's clips, in its order.
    pub set: Option<String>,
    /// Or clips by id, comma separated, in this order (the picked clips of the library).
    pub clips: Option<String>,
    /// Or a montage, opened again with its parts, its joins and its frame.
    pub from: Option<String>,
}

fn part_view(r: reel_store::models::ClipRow) -> PartView {
    let rid = r.rendition_id.clone().unwrap_or_default();
    let seconds = r.duration.unwrap_or(0.0);
    PartView {
        poster: format!("/media/renditions/{rid}.jpg"),
        src: format!("/media/renditions/{rid}.mp4"),
        seconds,
        length: format::duration(seconds),
        size: match (r.width, r.height) {
            (Some(w), Some(h)) => format::resolution(w as u32, h as u32),
            _ => String::new(),
        },
        rid,
        id: r.clip.id,
        title: r.clip.title,
        start: 0.0,
        end: seconds,
        volume: 1.0,
        used: true,
    }
}

fn join_name(kind: JoinKind) -> &'static str {
    match kind {
        JoinKind::Cut => "cut",
        JoinKind::Fade => "fade",
        JoinKind::Black => "black",
    }
}

fn shape_name(shape: MontageShape) -> &'static str {
    match shape {
        MontageShape::Wide => "wide",
        MontageShape::Tall => "tall",
        MontageShape::Square => "square",
    }
}

async fn recipe_of(
    st: &AppState,
    clip: &reel_store::models::Clip,
) -> AppResult<Option<MontageRecipe>> {
    if clip.source != "montage" {
        return Ok(None);
    }
    let Some(mid) = clip.master_id.as_deref() else {
        return Ok(None);
    };
    Ok(st
        .ctx
        .store
        .rendition(mid)
        .await?
        .and_then(|r| r.recipe)
        .and_then(|j| serde_json::from_str(&j).ok()))
}

/// The montage page: the clips to put in order, every one in, from a collection or a pick.
pub async fn page(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Query(q): Query<MontageAsked>,
) -> AppResult<Response> {
    if let Some(id) = &q.from {
        return again(&st, &me, id).await;
    }
    let (rows, from, from_href) = match (&q.set, &q.clips) {
        (Some(set), _) => {
            let c = st
                .ctx
                .store
                .collection(set)
                .await?
                .ok_or(AppError::NotFound)?;
            (
                st.ctx.store.collection_clips(set).await?,
                c.name,
                format!("/collections/{set}"),
            )
        }
        (None, Some(ids)) => {
            let mut rows = Vec::new();
            for id in ids
                .split(',')
                .filter(|s| !s.is_empty())
                .take(reel_core::recipe::MONTAGE_MOST)
            {
                if let Some(r) = st.ctx.store.clip_row(id).await? {
                    rows.push(r);
                }
            }
            (rows, "Library".into(), "/".into())
        }
        (None, None) => {
            return Err(AppError::BadRequest(
                "a collection or clips to make it of".into(),
            ))
        }
    };
    let parts = rows
        .into_iter()
        .filter(|r| r.clip.state == "ready" && r.rendition_id.is_some())
        .map(part_view)
        .collect();
    Ok(html(MontagePage {
        who: Who::at(&st, &me).await,
        nav: "library",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        parts,
        from,
        from_href,
        title: reel_pipeline::montage::title(&format::now()),
        most: reel_core::recipe::MONTAGE_MOST,
        longest: format::duration(reel_pipeline::montage::LONGEST_SECONDS),
        join: "cut",
        join_seconds: 0.5,
        shape: "wide",
        fill: false,
        even_loudness: true,
        replace_id: String::new(),
        replace_title: String::new(),
        notes: Vec::new(),
    }))
}

/// A montage opened again in the editor. A clip whose current version changed comes back whole
/// (its old part was of the other version), a gone one is left out; both are named.
async fn again(st: &AppState, me: &User, id: &str) -> AppResult<Response> {
    let clip = st
        .ctx
        .store
        .clip(id)
        .await?
        .filter(|c| c.deleted_at.is_none())
        .ok_or(AppError::NotFound)?;
    let recipe = recipe_of(st, &clip)
        .await?
        .ok_or_else(|| AppError::BadRequest("that clip is not a montage".into()))?;
    let mut parts = Vec::new();
    let mut notes = Vec::new();
    for part in &recipe.parts {
        let Some(row) = st
            .ctx
            .store
            .clip_row(&part.clip)
            .await?
            .filter(|r| r.clip.state == "ready" && r.rendition_id.is_some())
        else {
            notes.push("a clip of it is gone or not ready, so it is left out".to_string());
            continue;
        };
        let same = row.rendition_id.as_deref() == Some(part.rendition.as_str());
        let title = row.clip.title.clone();
        let mut v = part_view(row);
        if same {
            v.start = part.start.unwrap_or(0.0).clamp(0.0, v.seconds);
            v.end = part.end.unwrap_or(v.seconds).clamp(v.start, v.seconds);
        } else {
            notes.push(format!(
                "{title} has another version now, so it is whole again"
            ));
        }
        v.volume = part.volume;
        parts.push(v);
    }
    let mine = me.may_edit(clip.owner_id.as_deref());
    Ok(html(MontagePage {
        who: Who::at(st, me).await,
        nav: "library",
        q: String::new(),
        settings_json: crate::settings::client_json(st).await?,
        parts,
        from: clip.title.clone(),
        from_href: format!("/clips/{}", clip.id),
        title: clip.title.clone(),
        most: reel_core::recipe::MONTAGE_MOST,
        longest: format::duration(reel_pipeline::montage::LONGEST_SECONDS),
        join: join_name(recipe.join.kind),
        join_seconds: if recipe.join.overlap() > 0.0 {
            recipe.join.seconds
        } else {
            0.5
        },
        shape: shape_name(MontageShape::of(recipe.width, recipe.height)),
        fill: recipe.fit == MontageFit::Fill,
        even_loudness: recipe.even_loudness,
        replace_id: if mine { clip.id.clone() } else { String::new() },
        replace_title: if mine { clip.title } else { String::new() },
        notes,
    }))
}

#[derive(Deserialize)]
pub struct NewPart {
    pub clip: String,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub volume: Option<f64>,
}

#[derive(Deserialize)]
pub struct NewMontage {
    /// The clips whole, in order.
    #[serde(default)]
    pub clips: Vec<String>,
    /// Or each clip with its part and its volume.
    #[serde(default)]
    pub parts: Vec<NewPart>,
    pub title: Option<String>,
    /// cut, fade or black.
    pub join: Option<String>,
    pub join_seconds: Option<f64>,
    /// wide, tall or square.
    pub shape: Option<String>,
    /// Fill the frame, cutting off what is over, rather than bars.
    pub fill: Option<bool>,
    pub even_loudness: Option<bool>,
    /// A montage to send to the trash once this one is made.
    pub replaces: Option<String>,
}

/// A new clip of the person asking; the sources stay as they are, so seeing them is all it
/// takes. Replacing a montage takes the right to change it.
pub async fn create(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<NewMontage>,
) -> AppResult<Json<Value>> {
    crate::error::library_ready(&st.ctx).await?;
    let mut asked =
        reel_pipeline::montage::Asked::whole(&body.clips, body.title.clone(), me.name.clone());
    if !body.parts.is_empty() {
        asked.parts = body
            .parts
            .iter()
            .map(|p| reel_pipeline::montage::AskedPart {
                clip: p.clip.clone(),
                start: p.start,
                end: p.end,
                volume: p.volume,
            })
            .collect();
    }
    if let Some(j) = &body.join {
        asked.join = MontageJoin {
            kind: JoinKind::parse(j)
                .ok_or_else(|| AppError::BadRequest(format!("no join called {j}")))?,
            seconds: body.join_seconds.unwrap_or(0.5),
        };
    }
    if let Some(sh) = &body.shape {
        asked.shape = MontageShape::parse(sh)
            .ok_or_else(|| AppError::BadRequest(format!("no frame called {sh}")))?;
    }
    if body.fill == Some(true) {
        asked.fit = MontageFit::Fill;
    }
    if let Some(even) = body.even_loudness {
        asked.even_loudness = even;
    }
    if let Some(old) = &body.replaces {
        let clip = st
            .ctx
            .store
            .clip(old)
            .await?
            .filter(|c| c.deleted_at.is_none() && c.source == "montage")
            .ok_or_else(|| AppError::BadRequest("the montage to replace is not there".into()))?;
        if !me.may_edit(clip.owner_id.as_deref()) {
            return Err(AppError::Forbidden(
                "only who made a montage, or an admin, replaces it".into(),
            ));
        }
        asked.replaces = Some(clip.id);
    }
    match reel_pipeline::montage::request(&st.ctx, &asked, Some(me.id.clone())).await? {
        Ok((clip, job)) => Ok(Json(
            json!({ "clip_id": clip.id, "title": clip.title, "job_id": job.id }),
        )),
        Err(why) => Err(AppError::BadRequest(why.to_string())),
    }
}

/// A version's keyframes for the trimmer to snap a part's start to, read once with ffprobe and
/// kept next to the file; its rate says how near a frame is.
pub async fn keyframes(
    State(st): State<AppState>,
    Path(rid): Path<String>,
) -> AppResult<Json<Value>> {
    let r = st
        .ctx
        .store
        .rendition(&rid)
        .await?
        .filter(|r| r.is_version() && r.is_ready())
        .ok_or(AppError::NotFound)?;
    let text = crate::routes::pages::keyframes_cached(
        &st,
        &st.ctx.config().abs(r.path.as_deref().unwrap()),
    )
    .await;
    let keyframes: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!([]));
    Ok(Json(json!({ "keyframes": keyframes, "fps": r.fps })))
}
