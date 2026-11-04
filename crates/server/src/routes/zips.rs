//! The picked clips as one zip, sent as it is made, by a plain GET of a token asked for first.
use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::routes::bulk::{Picked, Skipped, GONE, NOT_READY};
use crate::zip::{Entry, Source, Zip};
use crate::AppState;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use reel_core::format;
use reel_store::models::{Clip, Rendition, Share};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// The most one download may weigh: a pick past it is refused before a byte is sent.
pub const MOST_BYTES: u64 = 50_000_000_000;
const KEPT: Duration = Duration::from_secs(3600);

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Which {
    Current,
    Original,
    Every,
}

impl Which {
    const ALL: [Which; 3] = [Which::Current, Which::Original, Which::Every];
    fn key(self) -> &'static str {
        match self {
            Which::Current => "current",
            Which::Original => "original",
            Which::Every => "every",
        }
    }
    fn words(self) -> &'static str {
        match self {
            Which::Current => "the current version of each",
            Which::Original => "the original of each",
            Which::Every => "every version of each",
        }
    }
}

/// The picks asked about, by token, for their downloads; an hour each.
#[derive(Default)]
pub struct Pending(Mutex<HashMap<String, Asked>>);

struct Asked {
    user: String,
    ids: Vec<String>,
    at: Instant,
}

impl Pending {
    async fn keep(&self, user: &str, ids: Vec<String>) -> String {
        let token = reel_core::ids::new_id();
        let mut map = self.0.lock().await;
        map.retain(|_, a| a.at.elapsed() < KEPT);
        map.insert(
            token.clone(),
            Asked {
                user: user.into(),
                ids,
                at: Instant::now(),
            },
        );
        token
    }

    async fn ids(&self, token: &str, user: &str) -> Option<Vec<String>> {
        let map = self.0.lock().await;
        map.get(token)
            .filter(|a| a.user == user && a.at.elapsed() < KEPT)
            .map(|a| a.ids.clone())
    }
}

struct Picked1 {
    clip: Clip,
    renditions: Vec<Rendition>,
}

/// The picked clips the user may see (in the library, not the trash), ready, in the order picked;
/// and the others with why.
async fn gather(st: &AppState, ids: &[String]) -> AppResult<(Vec<Picked1>, Vec<Skipped>)> {
    let mut ok = Vec::new();
    let mut skipped = Vec::new();
    for id in ids {
        match st
            .ctx
            .store
            .clip(id)
            .await?
            .filter(|c| c.deleted_at.is_none())
        {
            None => skipped.push(Skipped {
                id: id.clone(),
                why: GONE,
            }),
            Some(c) if c.state != "ready" || c.current_id.is_none() => skipped.push(Skipped {
                id: id.clone(),
                why: NOT_READY,
            }),
            Some(clip) => {
                let renditions = st.ctx.store.renditions_of(&clip.id).await?;
                ok.push(Picked1 { clip, renditions });
            }
        }
    }
    Ok((ok, skipped))
}

/// The versions a choice takes of a clip, oldest first.
fn chosen(p: &Picked1, which: Which) -> Vec<&Rendition> {
    let versions = p
        .renditions
        .iter()
        .filter(|r| r.is_version() && r.is_ready());
    match which {
        Which::Current => versions
            .filter(|r| Some(&r.id) == p.clip.current_id.as_ref())
            .collect(),
        Which::Original => versions
            .filter(|r| Some(&r.id) == p.clip.master_id.as_ref())
            .collect(),
        Which::Every => versions.collect(),
    }
}

fn weight(picked: &[Picked1], which: Which) -> (usize, u64) {
    picked.iter().fold((0, 0), |(n, b), p| {
        let v = chosen(p, which);
        (
            n + v.len(),
            b + v.iter().map(|r| r.size.unwrap_or(0) as u64).sum::<u64>(),
        )
    })
}

/// What the bar shows before a download: each choice's files and weight, and whether it is over
/// what one download may weigh; the clips left out and why; the token to download with.
pub async fn ask(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<Picked>,
) -> AppResult<Json<Value>> {
    let ids = crate::routes::bulk::picked(&body)?;
    crate::error::library_ready(&st.ctx).await?;
    let (picked, skipped) = gather(&st, &ids).await?;
    let mut choices = serde_json::Map::new();
    for which in Which::ALL {
        let (files, bytes) = weight(&picked, which);
        choices.insert(
            which.key().into(),
            json!({ "files": files, "bytes": bytes, "size": format::size(bytes), "over": bytes > MOST_BYTES }),
        );
    }
    let token = if picked.is_empty() {
        None
    } else {
        Some(st.zips.keep(&me.id, ids).await)
    };
    Ok(Json(json!({
        "clips": picked.len(),
        "skipped": skipped,
        "choices": choices,
        "most": format::size(MOST_BYTES),
        "token": token,
    })))
}

#[derive(Deserialize)]
pub struct WhichQuery {
    pub which: Option<Which>,
}

/// The zip of a pick: README.txt first, then a folder per clip with its files and clip.json.
pub async fn download(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(token): Path<String>,
    Query(q): Query<WhichQuery>,
) -> AppResult<Response> {
    let which = q.which.unwrap_or(Which::Current);
    let ids = st
        .zips
        .ids(&token, &me.id)
        .await
        .ok_or_else(|| AppError::NotFound)?;
    // without the library every video would be "missing": a zip of clip.json files alone
    crate::error::library_ready(&st.ctx).await?;
    let (picked, skipped) = gather(&st, &ids).await?;
    let (_, bytes) = weight(&picked, which);
    let over = |bytes: u64| {
        AppError::BadRequest(format!(
            "{} is over the {} one download may weigh: pick fewer clips",
            format::size(bytes),
            format::size(MOST_BYTES)
        ))
    };
    if bytes > MOST_BYTES {
        return Err(over(bytes));
    }
    let now = jiff::Timestamp::now().to_zoned(format::timezone());
    let entries = entries(&st, &picked, &skipped, which, &me, &now).await?;
    let clips = picked.len();
    let zip = Zip::new(entries, now.datetime());
    let length = zip.length();
    if length > MOST_BYTES {
        return Err(over(length));
    }
    let (mut tx, rx) = tokio::io::duplex(1 << 20);
    let by = me.name.clone();
    tokio::spawn(async move {
        match zip.write(&mut tx).await {
            Ok(sent) => {
                tracing::info!(clips, bytes = sent, by = %by, "picked clips downloaded as a zip")
            }
            Err(e) => tracing::info!(clips, error = %e, by = %by, "a zip of picked clips stopped"),
        }
    });
    let name = crate::serve_file::attachment_name(
        &format!(
            "reel {clips} clip{} {}",
            if clips == 1 { "" } else { "s" },
            now.strftime("%Y-%m-%d")
        ),
        "zip",
    );
    let mut r = Body::from_stream(tokio_util::io::ReaderStream::with_capacity(rx, 256 * 1024))
        .into_response();
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{name}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert("x-accel-buffering", HeaderValue::from_static("no"));
    *r.status_mut() = StatusCode::OK;
    Ok(r)
}

const README: &str = "README.txt";

/// A clip's folder: its title, made safe for every system's files, numbered when two are alike.
pub fn folder(title: &str, taken: &mut HashSet<String>) -> String {
    let mut s: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    s = s.trim().trim_end_matches(['.', ' ']).to_string();
    if s.chars().count() > 80 {
        s = s
            .chars()
            .take(80)
            .collect::<String>()
            .trim_end_matches(|c: char| c == '.' || c.is_whitespace())
            .to_string();
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    if s.is_empty() || reserved {
        s = format!("{s}_").trim_start_matches('_').to_string();
        if s.is_empty() {
            s = "clip".into();
        }
    }
    let mut name = s.clone();
    let mut n = 2;
    while !taken.insert(name.to_lowercase()) {
        name = format!("{s} ({n})");
        n += 1;
    }
    name
}

async fn entries(
    st: &AppState,
    picked: &[Picked1],
    skipped: &[Skipped],
    which: Which,
    me: &User,
    now: &jiff::Zoned,
) -> AppResult<Vec<Entry>> {
    let cfg = st.ctx.config();
    let base = cfg.public_url.trim_end_matches('/').to_string();
    // a clip titled README.txt gets a folder of its own name, numbered
    let mut taken = HashSet::from([README.to_lowercase()]);
    let mut out = vec![Entry::bytes(README, Vec::new())];
    let mut missing = 0;
    let mut owners: HashMap<String, Option<String>> = HashMap::new();
    for p in picked {
        let dir = folder(&p.clip.title, &mut taken);
        let mut files = HashMap::new();
        for r in chosen(p, which) {
            let path = cfg.abs(r.path.as_deref().unwrap_or(""));
            match tokio::fs::metadata(&path).await {
                Ok(m) if m.is_file() => {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("mp4")
                        .to_ascii_lowercase();
                    let file = format!("{}.{ext}", r.label());
                    out.push(Entry {
                        name: format!("{dir}/{file}"),
                        size: m.len(),
                        source: Source::File(path),
                    });
                    files.insert(r.id.clone(), file);
                }
                _ => missing += 1,
            }
        }
        let owner = match &p.clip.owner_id {
            Some(id) => match owners.get(id) {
                Some(n) => n.clone(),
                None => {
                    let n = st.ctx.store.user(id).await?.map(|u| u.name);
                    owners.insert(id.clone(), n.clone());
                    n
                }
            },
            None => None,
        };
        let shares = st.ctx.store.shares_of(&p.clip.id).await?;
        let json = clip_json(p, &files, owner, &shares, &base);
        out.push(Entry::bytes(
            format!("{dir}/clip.json"),
            serde_json::to_vec_pretty(&json).unwrap_or_default(),
        ));
    }
    let readme = readme(picked.len(), skipped, missing, which, me, now, &base);
    out[0] = Entry::bytes(README, readme.into_bytes());
    Ok(out)
}

fn clip_json(
    p: &Picked1,
    files: &HashMap<String, String>,
    owner: Option<String>,
    shares: &[Share],
    base: &str,
) -> Value {
    let c = &p.clip;
    let versions: Vec<Value> = p
        .renditions
        .iter()
        .filter(|r| r.is_version() && r.is_ready())
        .map(|r| {
            json!({
                "label": r.label(),
                "kind": r.kind,
                "file": files.get(&r.id),
                "current": c.current_id.as_deref() == Some(r.id.as_str()),
                "bytes": r.size,
                "seconds": r.duration,
                "width": r.width,
                "height": r.height,
                "fps": r.fps,
                "video_codec": r.vcodec,
                "audio_codec": r.acodec,
                "made": r.finished_at.as_deref().unwrap_or(&r.created_at),
            })
        })
        .collect();
    let links: Vec<Value> = shares
        .iter()
        .filter(|s| s.is_active())
        .map(|s| {
            let pinned = s
                .rendition_id
                .as_deref()
                .and_then(|id| p.renditions.iter().find(|r| r.id == id))
                .map(|r| r.label());
            json!({
                "url": format!("{base}/s/{}", s.token),
                "label": s.label,
                "plays": pinned.unwrap_or_else(|| "the current version".into()),
                "download": s.allow_download != 0,
                "made": s.created_at,
                "expires": s.expires_at,
            })
        })
        .collect();
    json!({
        "title": c.title,
        "game": c.game,
        "description": c.description,
        "recorded": c.recorded_at,
        "added": c.created_at,
        "owner": owner,
        "id": c.id,
        "page": format!("{base}/clips/{}", c.id),
        "versions": versions,
        "links": links,
    })
}

fn readme(
    clips: usize,
    skipped: &[Skipped],
    missing: usize,
    which: Which,
    me: &User,
    now: &jiff::Zoned,
    base: &str,
) -> String {
    let files = match which {
        Which::Current => "its current version (original.mp4, or the cut's, as cut 2.mp4)",
        Which::Original => "its original (original.mp4)",
        Which::Every => "every version (original.mp4, cut 1.mp4, ...)",
    };
    let mut s = format!(
        "reel: {clips} clip{}, {}\nMade {} by {} from {base}\n\nEach folder is one clip, named by its title (a number added where two share one): {files}, and clip.json with its title, game, description, when it was recorded and added, who uploaded it, its versions (which one is current) and its live links.\n",
        if clips == 1 { "" } else { "s" },
        which.words(),
        now.strftime("%Y-%m-%d %H:%M %Z"),
        me.name,
    );
    if !skipped.is_empty() {
        let mut by: Vec<(&str, usize)> = Vec::new();
        for k in skipped {
            match by.iter_mut().find(|(w, _)| *w == k.why) {
                Some((_, n)) => *n += 1,
                None => by.push((k.why, 1)),
            }
        }
        let why: Vec<String> = by.iter().map(|(w, n)| format!("{n} {w}")).collect();
        s.push_str(&format!("\nLeft out: {}.\n", why.join("; ")));
    }
    if missing > 0 {
        s.push_str(&format!(
            "\n{missing} file{} could not be read on the server and {} not in it.\n",
            if missing == 1 { "" } else { "s" },
            if missing == 1 { "is" } else { "are" }
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_the_title_made_safe_and_numbered() {
        let mut taken = HashSet::new();
        assert_eq!(folder("ace, last round", &mut taken), "ace, last round");
        assert_eq!(folder("Ace, Last Round", &mut taken), "Ace, Last Round (2)");
        assert_eq!(folder("ace, last round", &mut taken), "ace, last round (3)");
        assert_eq!(
            folder("a/b\\c: d*e?\"f<g>h|i", &mut taken),
            "a_b_c_ d_e__f_g_h_i"
        );
        assert_eq!(folder("  trailing dots... ", &mut taken), "trailing dots");
        assert_eq!(folder("", &mut taken), "clip");
        assert_eq!(folder("...", &mut taken), "clip (2)");
        assert_eq!(folder("con", &mut taken), "con_");
        assert_eq!(folder("COM1.mp4", &mut taken), "COM1.mp4_");
        assert_eq!(folder("Überhang", &mut taken), "Überhang");
        let long = "x".repeat(200);
        assert_eq!(folder(&long, &mut taken).chars().count(), 80);
        assert_eq!(
            folder(&format!("{}.y", "x".repeat(79)), &mut taken),
            "x".repeat(79)
        );
        let mut taken = HashSet::from([README.to_lowercase()]);
        assert_eq!(folder("readme.TXT", &mut taken), "readme.TXT (2)");
    }
}
