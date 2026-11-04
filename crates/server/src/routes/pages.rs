use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::routes::html;
use crate::views::*;
use crate::AppState;
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap};
use axum::response::Response;
use axum::{Extension, Json};
use reel_core::format;
use reel_store::Sort;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct LibraryQuery {
    pub q: Option<String>,
    pub sort: Option<String>,
}

pub struct Activity {
    pub text: String,
    pub class: String,
    pub progress: i64,
}

#[derive(Template)]
#[template(path = "library.html")]
pub struct LibraryPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub sort: String,
    pub summary: String,
    pub groups: Vec<DayGroup>,
    pub card_count: usize,
    pub disk_line: String,
    pub disk_detail: String,
    pub activity: Vec<Activity>,
    pub waiting: usize,
    pub settings_json: String,
    pub show_owners: bool,
    pub games: Vec<Pick>,
    pub sets: Vec<Pick>,
    /// Why the library folder is not usable, while it is not.
    pub library_away: Option<String>,
}

/// One item of the library's filters: a game (its name in small letters, empty for no game)
/// or a collection (its id), with how many clips it has.
pub struct Pick {
    pub key: String,
    pub name: String,
    pub count: i64,
}

async fn picks(st: &AppState) -> AppResult<(Vec<Pick>, Vec<Pick>)> {
    let games = st
        .ctx
        .store
        .games()
        .await?
        .into_iter()
        .map(|(g, n)| Pick {
            key: g.as_deref().map(|g| g.to_lowercase()).unwrap_or_default(),
            name: g.unwrap_or_else(|| "No game".into()),
            count: n,
        })
        .collect::<Vec<_>>();
    let sets = st
        .ctx
        .store
        .collections()
        .await?
        .into_iter()
        .filter(|c| !c.collection.is_game())
        .map(|c| Pick {
            key: c.collection.id,
            name: c.collection.name,
            count: c.clips,
        })
        .collect();
    let games = if games.iter().any(|g| !g.key.is_empty()) {
        games
    } else {
        Vec::new()
    };
    Ok((games, sets))
}

pub async fn library(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Query(q): Query<LibraryQuery>,
) -> AppResult<Response> {
    let query = q.q.unwrap_or_default();
    let sort = Sort::parse(q.sort.as_deref().unwrap_or("newest"));
    let rows = st.ctx.store.clips(&query, sort).await?;
    let (count, bytes) = st.ctx.store.clip_count_and_size().await?;
    let dated = matches!(sort, Sort::Newest | Sort::Oldest);
    let in_sets = st.ctx.store.collection_ids_by_clip().await?;
    let (games, sets) = picks(&st).await?;
    let starred = st.ctx.store.starred_ids(&me.id).await?;
    let card_of = |row: &reel_store::models::ClipRow| {
        let mut c = card_for(row, &me);
        c.sets = in_sets
            .get(&row.clip.id)
            .map(|v| v.join(" "))
            .unwrap_or_default();
        c.starred = starred.contains(&row.clip.id);
        c
    };
    let mut groups: Vec<DayGroup> = Vec::new();
    for row in &rows {
        let when = row
            .clip
            .recorded_at
            .clone()
            .unwrap_or_else(|| row.clip.created_at.clone());
        let (key, label) = if dated {
            (format::day_key(&when), format::day_label(&when))
        } else {
            (String::new(), String::new())
        };
        match groups.last_mut() {
            Some(g) if g.key == key => g.cards.push(card_of(row)),
            _ => groups.push(DayGroup {
                key,
                label,
                cards: vec![card_of(row)],
            }),
        }
    }
    let card_count = rows.len();
    let disk = st.disk.get(&st.ctx.effective(), false).await;
    let clips_bytes = st.ctx.store.library_bytes().await? as u64;
    let mut activity = Vec::new();
    for j in st.ctx.store.active_jobs().await? {
        if j.state != "running" {
            continue;
        }
        let title = match &j.clip_id {
            Some(id) => st
                .ctx
                .store
                .clip(id)
                .await?
                .map(|c| c.title)
                .unwrap_or_default(),
            None => String::new(),
        };
        let verb = match j.kind.as_str() {
            "ingest" => "encoding",
            "cut" => "cutting",
            "embed" => "preview video of",
            "web" => "web copy of",
            "discord" => "making a copy under a size",
            "anim" => "making an animated picture of",
            "montage" => "making the montage",
            k => k,
        };
        activity.push(Activity {
            text: format!("{verb} {title}"),
            class: "st-active".into(),
            progress: (j.progress * 100.0).round() as i64,
        });
    }
    let waiting = st
        .ctx
        .store
        .active_jobs()
        .await?
        .iter()
        .filter(|j| j.state == "queued")
        .count()
        + st.ctx
            .store
            .ingest_files()
            .await?
            .iter()
            .filter(|f| f.state == "seen")
            .count();
    let summary = if query.is_empty() {
        summary_line(count, bytes, sort)
    } else {
        format!(
            "{} match{} for \"{}\"",
            card_count,
            if card_count == 1 { "" } else { "es" },
            query
        )
    };
    Ok(html(LibraryPage {
        who: Who::at(&st, &me).await,
        nav: "library",
        q: query,
        sort: sort.key().into(),
        summary,
        groups,
        card_count,
        disk_line: disk.line(clips_bytes),
        disk_detail: disk.detail(clips_bytes),
        activity,
        waiting,
        settings_json: crate::settings::client_json(&st).await?,
        show_owners: st.ctx.store.user_count().await? > 1,
        games,
        sets,
        library_away: st.ctx.library.away(),
    }))
}

/// The page head's line: the count, the size of the current versions, the sort.
fn summary_line(count: i64, bytes: i64, sort: Sort) -> String {
    format!(
        "{count} clip{}, {}, {}",
        if count == 1 { "" } else { "s" },
        format::size(bytes as u64),
        sort_words(sort)
    )
}

#[derive(Template)]
#[template(path = "card.html")]
pub struct CardPartial {
    pub c: Card,
    pub show_owners: bool,
}

/// One library card as HTML, with its day and the head's line, for the live library to draw.
pub async fn library_card(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    Query(q): Query<LibraryQuery>,
) -> AppResult<Json<Value>> {
    let sort = Sort::parse(q.sort.as_deref().unwrap_or("newest"));
    let row = st
        .ctx
        .store
        .clip_row(&id)
        .await?
        .ok_or(AppError::NotFound)?;
    let when = row
        .clip
        .recorded_at
        .clone()
        .unwrap_or_else(|| row.clip.created_at.clone());
    let dated = matches!(sort, Sort::Newest | Sort::Oldest);
    let mut c = card_for(&row, &me);
    c.sets = st.ctx.store.collections_of_clip(&id).await?.join(" ");
    c.starred = st.ctx.store.is_starred(&me.id, &id).await?;
    let partial = CardPartial {
        c,
        show_owners: st.ctx.store.user_count().await? > 1,
    };
    let html = partial
        .render()
        .map_err(|e| anyhow::anyhow!("the card did not render: {e}"))?;
    let (count, bytes) = st.ctx.store.clip_count_and_size().await?;
    Ok(Json(json!({
        "id": id,
        "html": html,
        "day": {
            "key": if dated { format::day_key(&when) } else { String::new() },
            "label": if dated { format::day_label(&when) } else { String::new() },
        },
        "summary": summary_line(count, bytes, sort),
    })))
}

/// The page head's line alone, after a clip went.
pub async fn library_summary(
    State(st): State<AppState>,
    Extension(_me): Extension<User>,
    Query(q): Query<LibraryQuery>,
) -> AppResult<Json<Value>> {
    let sort = Sort::parse(q.sort.as_deref().unwrap_or("newest"));
    let (count, bytes) = st.ctx.store.clip_count_and_size().await?;
    Ok(Json(json!({ "summary": summary_line(count, bytes, sort) })))
}

fn sort_words(s: Sort) -> &'static str {
    match s {
        Sort::Newest => "newest first",
        Sort::Oldest => "oldest first",
        Sort::Longest => "longest first",
        Sort::Title => "by title",
        Sort::Shared => "shared first",
    }
}

/// One choice of the Download select: the original as it is, or a copy under a limit.
pub struct Quality {
    pub value: String,
    pub label: String,
    pub ready: bool,
}

pub struct Link {
    pub id: String,
    pub title: String,
}

fn video_box(width: i64, height: i64) -> u32 {
    let w = width.max(height * 16 / 9);
    if w <= 0 {
        1920
    } else {
        w.clamp(640, 7680) as u32
    }
}

/// Each animated picture's weight for the current version, from a calm picture to a busy one,
/// by "format-width-fps", and whether the version is too long for one.
fn anim_json(current: Option<&reel_store::models::Rendition>) -> String {
    use reel_core::recipe::{AnimFormat, AnimRecipe, ANIM_FPS, ANIM_MAX_SECONDS, ANIM_WIDTHS};
    let Some(c) = current else {
        return "null".into();
    };
    let (w, h, d) = (
        c.width.unwrap_or(0) as u32,
        c.height.unwrap_or(0) as u32,
        c.duration.unwrap_or(0.0),
    );
    let mut sizes = serde_json::Map::new();
    for f in [AnimFormat::Gif, AnimFormat::Webp] {
        for width in ANIM_WIDTHS {
            for fps in ANIM_FPS {
                if let Some(r) = AnimRecipe::new(f, width, fps) {
                    let (low, high) = r.estimate(w, h, d);
                    sizes.insert(
                        format!("{}-{width}-{fps}", f.ext()),
                        json!([format::size(low), format::size(high)]),
                    );
                }
            }
        }
    }
    json!({ "max": ANIM_MAX_SECONDS, "long": d > ANIM_MAX_SECONDS + 0.05, "length": format::duration(d), "sizes": sizes }).to_string()
}

/// What a montage's master was made of, each clip by its title as it is now.
async fn montage_parts(
    st: &AppState,
    master: Option<&reel_store::models::Rendition>,
) -> AppResult<Vec<Link>> {
    let Some(recipe) = master
        .and_then(|m| m.recipe.as_deref())
        .and_then(|r| serde_json::from_str::<reel_core::recipe::MontageRecipe>(r).ok())
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for p in recipe.parts {
        out.push(
            match st
                .ctx
                .store
                .clip(&p.clip)
                .await?
                .filter(|c| c.deleted_at.is_none())
            {
                Some(c) => Link {
                    id: c.id,
                    title: c.title,
                },
                None => Link {
                    id: String::new(),
                    title: "a clip that is gone".into(),
                },
            },
        );
    }
    Ok(out)
}

#[derive(Template)]
#[template(path = "clip.html")]
pub struct ClipPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub id: String,
    pub title: String,
    pub description: String,
    pub state: String,
    pub error: Option<String>,
    pub ready: bool,
    pub video_url: Option<String>,
    pub poster_url: Option<String>,
    pub strip_url: Option<String>,
    pub meta: Vec<String>,
    pub details: Vec<(String, String)>,
    pub original_download: Option<(String, String)>,
    pub source: Option<Link>,
    pub spinoffs: Vec<Link>,
    pub shares: Vec<ShareView>,
    pub versions: Vec<VersionView>,
    pub duration: f64,
    pub fps: f64,
    pub has_audio: bool,
    pub keyframes_json: String,
    pub level_json: String,
    pub tracks: Vec<String>,
    pub loudness_target: i32,
    pub quick_trims: Vec<u32>,
    pub current_label: String,
    pub current_size: String,
    /// The widest the player gets on a wide screen: the current version's own width, or for a
    /// tall video the width of the 16:9 box its height fills.
    pub video_box: u32,
    pub qualities: Vec<Quality>,
    pub copy_notes: Vec<String>,
    pub original_under: Option<u64>,
    pub trash_days: u64,
    pub download_name: String,
    pub active_job: Option<(String, String, i64)>,
    pub current_id: String,
    pub master_id: String,
    /// The current cut the editor opens on: its label, and its range and sound on the
    /// original as JSON; empty and null while the original is current.
    pub edit_from_label: String,
    pub edit_from_json: String,
    pub can_edit: bool,
    pub owner: String,
    pub sets: Vec<SetPick>,
    /// The game's own collection the clip is in, by its game.
    pub game_set: Option<(String, String)>,
    pub back: crate::back::Back,
    pub starred: bool,
    /// The animated pictures the Send panel offers: formats, weights, and whether it is too long.
    pub anim_json: String,
    pub anim_webp: bool,
    /// The animated pictures made of its versions, and being made.
    pub anims: Vec<crate::routes::anims::AnimView>,
    /// Whether the poster is an uploaded image, and the shape the poster takes.
    pub poster_image: bool,
    pub poster_aspect: String,
    pub poster_time: f64,
    /// The clips a montage is made of, in its order; a clip gone has no id.
    pub montage_parts: Vec<Link>,
}

/// A collection on the clip page: the ones the user may change, ticked when the clip is in it.
pub struct SetPick {
    pub id: String,
    pub name: String,
    pub on: bool,
}

pub async fn clip(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let clip = st
        .ctx
        .store
        .clip(&id)
        .await?
        .filter(|c| c.deleted_at.is_none())
        .ok_or(AppError::NotFound)?;
    let renditions = st.ctx.store.renditions_of(&clip.id).await?;
    let current = clip
        .current_id
        .as_deref()
        .and_then(|cid| renditions.iter().find(|r| r.id == cid))
        .cloned();
    let master = clip
        .master_id
        .as_deref()
        .and_then(|mid| renditions.iter().find(|r| r.id == mid))
        .cloned();
    let previews = reel_pipeline::external::Overview::load(&st.ctx).await?;
    let shares: Vec<ShareView> = st
        .ctx
        .store
        .shares_of(&clip.id)
        .await?
        .iter()
        .filter(|s| s.revoked_at.is_none())
        .map(|s| {
            let mut v = share_view(
                s,
                &st.ctx.config().public_url,
                serves_text(s, &renditions, clip.current_id.as_deref()),
            );
            if s.is_active() {
                if let Some(vid) = s.rendition_id.as_deref().or(clip.current_id.as_deref()) {
                    v.preview = previews.words(vid);
                }
            }
            crate::views::cleanup_note(&mut v, s, &st.ctx.config());
            // posted, then edited: Discord keeps the preview the link was posted with, so a
            // new address for this version gets a fresh one
            if let (true, None, Some(posted), Some(cur)) =
                (s.is_active(), &s.rendition_id, &s.last_posted_at, &current)
            {
                if format::parse(&cur.created_at) > format::parse(posted) {
                    v.version_url = Some(format!("{}?v={}", v.url, cur.seq));
                }
            }
            v
        })
        .collect();
    let versions: Vec<VersionView> = renditions
        .iter()
        .filter(|r| r.is_version())
        .map(|r| version_view(r, clip.current_id.as_deref()))
        .collect();
    let probe: Option<reel_core::ProbeInfo> = clip
        .original_probe
        .as_deref()
        .and_then(|p| serde_json::from_str(p).ok());
    let recorded_from = match clip.recorded_source.as_deref() {
        Some("name") => "from the file name",
        Some("imported") => "from the imported library",
        Some("container") => "from the file's metadata",
        Some("file time") => "file time, the last resort",
        Some("device") => "from the desktop app",
        _ => "",
    };
    let recorded_text = match clip.recorded_at.as_deref() {
        Some(at) if !recorded_from.is_empty() => {
            format!("{} ({recorded_from})", format::when(at))
        }
        Some(at) => format::when(at),
        None => "unknown".into(),
    };
    let mut meta = vec![format!("recorded {recorded_text}")];
    let via = match clip.source.as_str() {
        "device" => "the desktop app".to_string(),
        other => other.to_string(),
    };
    let mut details: Vec<(String, String)> = vec![
        ("file".into(), clip.source_name.clone()),
        ("recorded".into(), recorded_text.clone()),
        (
            "arrived".into(),
            format!("{} via {via}", format::when(&clip.created_at)),
        ),
    ];
    if let Some(game) = clip.game.as_deref().filter(|g| !g.is_empty()) {
        details.push(("game".into(), game.to_string()));
    }
    if let Some(c) = &current {
        meta.push(format::duration(c.duration.unwrap_or(0.0)));
        meta.push(format!(
            "{}, {} fps",
            format::resolution(c.width.unwrap_or(0) as u32, c.height.unwrap_or(0) as u32),
            c.fps.unwrap_or(0.0).round()
        ));
        meta.push(format::size(c.size.unwrap_or(0) as u64));
    }
    if let Some(reason) = &clip.plan_reason {
        meta.push(reason.clone());
    }
    if let Some(p) = &probe {
        details.push((
            "video".into(),
            format!(
                "{} {}, {}, {} fps, {}",
                p.vcodec.clone().unwrap_or_default(),
                p.profile.clone().unwrap_or_default(),
                format::resolution(p.width, p.height),
                p.fps.round(),
                format::mbit(p.vbitrate.unwrap_or(0))
            ),
        ));
        details.push((
            "audio".into(),
            match (&p.acodec, p.abitrate) {
                (Some(a), bits) if a == "aac" && !st.ctx.config().media.normalize_audio => {
                    format!("AAC {} kbit/s, copied", bits.unwrap_or(0) / 1000)
                }
                (Some(a), bits) => format!(
                    "{a} {} kbit/s, re-encoded to AAC {} kbit/s{}",
                    bits.unwrap_or(0) / 1000,
                    reel_core::plan::audio_kbit(&st.ctx.config().media, bits),
                    if st.ctx.config().media.normalize_audio {
                        ", normalized"
                    } else {
                        ""
                    }
                ),
                (None, _) => "none".into(),
            },
        ));
    }
    let mut original_download = None;
    match &clip.original_path {
        Some(p) => {
            let size = std::fs::metadata(p)
                .map(|m| format::size(m.len()))
                .unwrap_or_else(|_| "file missing".into());
            details.push(("original".into(), format!("kept, {size}")));
            if std::path::Path::new(p).is_file() {
                original_download = Some((format!("/media/clips/{}/original", clip.id), size));
            }
        }
        None => {
            let why = match clip.source.as_str() {
                "cut" => "none, this clip is a cut of another",
                "montage" => "none, this clip is a montage of others",
                _ => "not kept, the stream was copied",
            };
            details.push(("original".into(), why.into()));
        }
    }
    if let Some(c) = current.as_ref().filter(|_| !st.ctx.config().standalone()) {
        let embed = st.ctx.store.derived_rendition(&c.id, "embed", None).await?;
        let cfg = st.ctx.config();
        details.push((
            "Preview video".into(),
            match embed {
                Some(e) if e.is_ready() => format!(
                    "{}, {}p{}",
                    format::size(e.size.unwrap_or(0) as u64),
                    e.height.unwrap_or(0),
                    e.reason
                        .as_deref()
                        .map(|r| format!(", {r}"))
                        .unwrap_or_default()
                ),
                Some(e) => format!("a preview video is {}", e.state),
                None if reel_pipeline::enqueue::needs_embed(&cfg, c) => format!(
                    "none yet; {} is over {} MB, so Storage, Preview videos, Remake makes one",
                    format::size(c.size.unwrap_or(0) as u64),
                    cfg.share.embed_mb
                ),
                None => format!(
                    "sent as it is, {} is under {} MB",
                    format::size(c.size.unwrap_or(0) as u64),
                    cfg.share.embed_mb
                ),
            },
        ));
    }
    details.push(("id".into(), clip.id.clone()));
    let source = match &clip.source_clip_id {
        Some(sid) => st
            .ctx
            .store
            .clip(sid)
            .await?
            .filter(|c| c.deleted_at.is_none())
            .map(|c| Link {
                id: c.id,
                title: c.title,
            }),
        None => None,
    };
    let spinoffs: Vec<Link> = st
        .ctx
        .store
        .clips_from(&clip.id)
        .await?
        .into_iter()
        .map(|c| Link {
            id: c.id,
            title: c.title,
        })
        .collect();
    let mut qualities = Vec::new();
    let mut copy_notes: Vec<String> = Vec::new();
    let mut original_under = None;
    if let Some(c) = &current {
        qualities.push(Quality {
            value: "original".into(),
            label: format!(
                "{} as it is, {}",
                c.label(),
                format::size(c.size.unwrap_or(0) as u64)
            ),
            ready: true,
        });
        let mut limits = st.ctx.config().share.discord_limits_mb.clone();
        limits.sort_unstable_by(|a, b| b.cmp(a));
        for limit in limits {
            if c.size.unwrap_or(0) as u64 <= limit * 1_000_000 {
                original_under = Some(limit);
                continue;
            }
            let recipe = reel_core::recipe::DiscordRecipe::for_limit(
                limit * 1_000_000,
                &st.ctx.config().media,
            )
            .json();
            let ready = st
                .ctx
                .store
                .derived_rendition(&c.id, "discord", Some(&recipe))
                .await?
                .filter(|r| r.is_ready());
            if let Some(why) = ready.as_ref().and_then(|r| r.reason.clone()) {
                copy_notes.push(format!("Under {limit} MB: {why}."));
            }
            qualities.push(Quality {
                value: limit.to_string(),
                label: match &ready {
                    Some(r) => format!(
                        "Under {limit} MB, ready, {}",
                        format::size(r.size.unwrap_or(0) as u64)
                    ),
                    None => format!("Under {limit} MB"),
                },
                ready: ready.is_some(),
            });
        }
    }
    // with a cut current the editor opens on its range, so nudging it is one small drag
    let edit_from = current
        .as_ref()
        .filter(|c| c.kind == "cut")
        .and_then(|c| {
            let r: reel_core::CutRecipe = serde_json::from_str(c.recipe.as_deref()?).ok()?;
            let whole = master.as_ref().and_then(|m| m.duration).unwrap_or(0.0);
            Some((
                c.label(),
                json!({
                    "label": c.label(),
                    "start": r.start.unwrap_or(0.0),
                    "end": r.end.unwrap_or(whole).min(whole),
                    "mute": r.mute,
                    "gain_db": r.gain_db,
                    "normalize": r.loudness.is_some(),
                    "tracks": r.tracks.as_ref().map(|t| t.iter().map(|x| json!({ "on": x.on, "gain_db": x.gain_db })).collect::<Vec<_>>()),
                    "reframe": r.reframe.map(|f| json!({ "shape": f.shape.words(), "x": f.x })),
                    "fade_in": r.fade_in,
                    "fade_out": r.fade_out,
                }),
            ))
        });
    let keyframes_json = match &master {
        Some(m) if m.is_ready() => {
            keyframes_cached(&st, &st.ctx.config().abs(m.path.as_deref().unwrap())).await
        }
        _ => "[]".into(),
    };
    let tracks = match &master {
        Some(m) if m.is_ready() && m.acodec.is_some() => {
            tracks_cached(&st, &st.ctx.config().abs(m.path.as_deref().unwrap()), &clip).await
        }
        _ => Vec::new(),
    };
    if !tracks.is_empty() {
        let at = details
            .iter()
            .position(|(k, _)| k == "id")
            .unwrap_or(details.len());
        let words = match tracks.len() {
            1 => "1 audio track".to_string(),
            n => format!("{n} audio tracks: {}", tracks.join(", ")),
        };
        details.insert(at, ("tracks".into(), words));
    }
    let level_json = {
        let cfg = st.ctx.config();
        serde_json::json!({
            "master": master.as_ref().map(|m| m.id.clone()),
            "audio": master.as_ref().map(|m| m.acodec.is_some()).unwrap_or(false),
            "measured": master.as_ref().map(|m| m.audio_measured_at.is_some()).unwrap_or(false),
            "loudness": master.as_ref().and_then(|m| m.audio_loudness),
            "peak": master.as_ref().and_then(|m| m.audio_peak),
            "target": cfg.media.loudness_target,
            "kbit": cfg.media.audio_kbit,
            "ceiling": reel_media::args::PEAK_CEILING,
            "tracks": tracks,
        })
        .to_string()
    };
    let in_sets = st.ctx.store.collections_of_clip(&clip.id).await?;
    let all_sets = st.ctx.store.collections().await?;
    let game_set = all_sets
        .iter()
        .find(|c| c.collection.is_game() && in_sets.contains(&c.collection.id))
        .map(|c| (c.collection.id.clone(), c.collection.name.clone()));
    let sets = all_sets
        .into_iter()
        .filter(|c| !c.collection.is_game() && me.may_edit(c.collection.owner_id.as_deref()))
        .map(|c| SetPick {
            on: in_sets.contains(&c.collection.id),
            id: c.collection.id,
            name: c.collection.name,
        })
        .collect();
    let active_job = st
        .ctx
        .store
        .active_jobs()
        .await?
        .into_iter()
        .find(|j| j.clip_id.as_deref() == Some(&clip.id))
        .map(|j| (j.id, j.kind, (j.progress * 100.0).round() as i64));
    let public = crate::back::authority(&st.ctx.config().public_url);
    let hosts: Vec<&str> = [
        headers.get(header::HOST).and_then(|v| v.to_str().ok()),
        public.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    let referer = headers.get(header::REFERER).and_then(|v| v.to_str().ok());
    let back = crate::back::resolve(&st, crate::back::origin(referer, &hosts), &clip.id).await;
    Ok(html(ClipPage {
        who: Who::at(&st, &me).await,
        nav: "library",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        id: clip.id.clone(),
        title: clip.title.clone(),
        description: clip.description.clone(),
        state: clip.state.clone(),
        error: clip
            .error
            .as_deref()
            .map(|e| hide_paths(e, &st.ctx.config())),
        ready: clip.state == "ready" && current.is_some(),
        video_url: current
            .as_ref()
            .map(|c| format!("/media/renditions/{}.mp4", c.id)),
        poster_url: current
            .as_ref()
            .map(|c| format!("/media/renditions/{}.jpg?t={}", c.id, clip.poster_key())),
        strip_url: master
            .as_ref()
            .filter(|m| m.is_ready())
            .map(|m| format!("/media/renditions/{}.strip.jpg", m.id)),
        meta,
        details,
        original_download,
        source,
        spinoffs,
        shares,
        versions,
        duration: master.as_ref().and_then(|m| m.duration).unwrap_or(0.0),
        fps: master.as_ref().and_then(|m| m.fps).unwrap_or(60.0),
        has_audio: master.as_ref().map(|m| m.acodec.is_some()).unwrap_or(false),
        keyframes_json,
        level_json,
        tracks,
        loudness_target: st.ctx.config().media.loudness_target,
        quick_trims: st.ctx.config().edit.quick_trims.clone(),
        current_label: current.as_ref().map(|c| c.label()).unwrap_or_default(),
        video_box: current
            .as_ref()
            .map(|c| video_box(c.width.unwrap_or(0), c.height.unwrap_or(0)))
            .unwrap_or(1920),
        current_size: current
            .as_ref()
            .map(|c| format::size(c.size.unwrap_or(0) as u64))
            .unwrap_or_default(),
        qualities,
        copy_notes,
        original_under,
        trash_days: st.ctx.config().share.trash_days,
        download_name: crate::serve_file::attachment_name(&clip.title, "mp4"),
        active_job,
        current_id: clip.current_id.clone().unwrap_or_default(),
        master_id: clip.master_id.clone().unwrap_or_default(),
        edit_from_label: edit_from
            .as_ref()
            .map(|(l, _)| l.clone())
            .unwrap_or_default(),
        edit_from_json: edit_from
            .map(|(_, j)| j.to_string())
            .unwrap_or_else(|| "null".into()),
        can_edit: me.may_edit(clip.owner_id.as_deref()),
        sets,
        game_set,
        back,
        starred: st.ctx.store.is_starred(&me.id, &clip.id).await?,
        anim_webp: st.ctx.hw.read().await.anim_webp,
        anim_json: anim_json(current.as_ref()),
        anims: crate::routes::anims::views(&st, clip.current_id.as_deref(), &renditions).await?,
        poster_image: clip.poster_image.is_some(),
        poster_aspect: current
            .as_ref()
            .map(|c| {
                let (w, h) = reel_pipeline::poster_image::poster_size(
                    c.width.unwrap_or(0),
                    c.height.unwrap_or(0),
                );
                format!("{w} / {h}")
            })
            .unwrap_or_else(|| "16 / 9".into()),
        poster_time: clip.poster_time.unwrap_or(0.0),
        montage_parts: montage_parts(&st, master.as_ref()).await?,
        owner: match &clip.owner_id {
            Some(o) => st
                .ctx
                .store
                .user(o)
                .await?
                .map(|u| u.name)
                .unwrap_or_else(|| "someone who is gone".into()),
            None => String::new(),
        },
    }))
}

/// The audio tracks of an original by name: the raw file's titles (the original is an MP4,
/// which keeps none), else "Track 1", "Track 2"; kept beside the original as JSON.
async fn tracks_cached(
    st: &AppState,
    master: &std::path::Path,
    clip: &reel_store::models::Clip,
) -> Vec<String> {
    let cache = master.with_extension("tracks.json");
    if let Ok(text) = tokio::fs::read_to_string(&cache).await {
        if let Ok(names) = serde_json::from_str::<Vec<String>>(&text) {
            return names;
        }
    }
    let Ok(n) = st.ctx.ff.audio_streams(master).await else {
        return Vec::new();
    };
    let titles = clip
        .original_probe
        .as_deref()
        .and_then(|p| serde_json::from_str::<reel_core::ProbeInfo>(p).ok())
        .map(|p| p.audio_titles)
        .filter(|t| t.len() == n)
        .unwrap_or_default();
    let names: Vec<String> = (0..n)
        .map(|i| match titles.get(i) {
            Some(t) if !t.is_empty() => t.clone(),
            _ => format!("Track {}", i + 1),
        })
        .collect();
    let _ = tokio::fs::write(&cache, serde_json::to_string(&names).unwrap_or_default()).await;
    names
}

/// A version's keyframes as JSON, read once with ffprobe and kept next to its file.
pub(crate) async fn keyframes_cached(st: &AppState, master: &std::path::Path) -> String {
    let cache = master.with_extension("kf.json");
    if let Ok(text) = tokio::fs::read_to_string(&cache).await {
        return text;
    }
    match st.ctx.ff.keyframes(master).await {
        Ok(kf) => {
            let text = serde_json::to_string(&kf).unwrap_or_else(|_| "[]".into());
            let _ = tokio::fs::write(&cache, &text).await;
            text
        }
        Err(e) => {
            tracing::warn!(error = %e, "keyframes");
            "[]".into()
        }
    }
}

#[derive(Deserialize)]
pub struct SharesQuery {
    pub state: Option<String>,
}

pub struct ShareRowView {
    pub share: ShareView,
    pub clip_id: String,
    pub title: String,
    pub thumb: Option<String>,
    pub created: String,
    /// The short cells of a wide table, their sentences in the cards and the titles.
    pub created_day: String,
    pub posted_day: Option<String>,
    pub version: String,
    pub preview_word: &'static str,
    pub preview_warn: bool,
    pub expires: String,
    pub expires_class: String,
    pub link_head: String,
    pub link_tail: String,
}

#[derive(Template)]
#[template(path = "shares.html")]
pub struct SharesPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub filter: String,
    pub summary: String,
    pub rows: Vec<ShareRowView>,
    /// Whether any row says where its preview comes from, for the column.
    pub previews: bool,
}

pub async fn shares(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Query(q): Query<SharesQuery>,
) -> AppResult<Response> {
    let filter = q.state.unwrap_or_else(|| "active".into());
    let all = st.ctx.store.shares().await?;
    let active = all
        .iter()
        .filter(|r| r.share.is_active() && r.clip_deleted_at.is_none())
        .count();
    let previews = reel_pipeline::external::Overview::load(&st.ctx).await?;
    let rows: Vec<ShareRowView> = all
        .iter()
        .filter(|r| match filter.as_str() {
            "active" => r.share.is_active() && r.clip_deleted_at.is_none(),
            "expired" => r.share.revoked_at.is_none() && r.share.is_expired(),
            "revoked" => r.share.revoked_at.is_some(),
            _ => true,
        })
        .map(|r| {
            let cfg = st.ctx.config();
            let mut v = share_view(&r.share, &cfg.public_url, r.serves());
            let mut word = ("", false);
            if r.share.is_active() && r.clip_deleted_at.is_none() {
                if let Some(vid) = r.share.rendition_id.as_deref().or(r.current_id.as_deref()) {
                    v.preview = previews.words(vid);
                    word = previews.word(vid);
                }
            }
            crate::views::cleanup_note(&mut v, &r.share, &cfg);
            let (expires, expires_class) = crate::views::expiry_short(&r.share, &cfg);
            let (link_head, link_tail) = format::split_tail(&r.share.token, 6);
            ShareRowView {
                share: v,
                clip_id: r.share.clip_id.clone(),
                title: r.title.clone(),
                thumb: r
                    .share
                    .rendition_id
                    .clone()
                    .or_else(|| r.current_id.clone())
                    .map(|c| format!("/media/renditions/{c}.jpg")),
                created: format::when(&r.share.created_at),
                created_day: format::day(&r.share.created_at),
                posted_day: r.share.last_posted_at.as_deref().map(format::day),
                version: r.version(),
                preview_word: word.0,
                preview_warn: word.1,
                expires,
                expires_class,
                link_head,
                link_tail,
            }
        })
        .collect();
    let summary = format!(
        "{} link{}, {} active, every active link is public until it is revoked or expires",
        all.len(),
        if all.len() == 1 { "" } else { "s" },
        active
    );
    Ok(html(SharesPage {
        who: Who::at(&st, &me).await,
        nav: "shares",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        filter,
        summary,
        previews: rows.iter().any(|r| !r.share.preview.is_empty()),
        rows,
    }))
}

#[derive(Template)]
#[template(path = "upload.html")]
pub struct UploadPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub extensions: String,
}

pub async fn upload_page(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Response> {
    let extensions = st
        .ctx
        .config()
        .ingest
        .extensions
        .iter()
        .map(|e| format!(".{e}"))
        .collect::<Vec<_>>()
        .join(",");
    Ok(html(UploadPage {
        who: Who::at(&st, &me).await,
        nav: "upload",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        extensions,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_player_grows_to_what_the_video_holds() {
        assert_eq!(video_box(1920, 1080), 1920);
        assert_eq!(video_box(1280, 720), 1280);
        assert_eq!(
            video_box(1080, 1920),
            3413,
            "a tall video fills the box by its height"
        );
        assert_eq!(video_box(0, 0), 1920, "unknown: a 1080p box");
        assert_eq!(video_box(320, 180), 640);
    }
}
