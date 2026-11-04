use crate::Ctx;
use anyhow::{anyhow, Context};
use reel_core::ids::new_id;
use reel_core::names::parse_name;
use reel_core::{CutRecipe, Event};
use reel_store::models::*;
use serde_json::json;
use std::path::Path;

pub struct NewClip<'a> {
    pub input: &'a Path,
    pub source: &'a str,
    pub relative: Option<&'a str>,
    pub recipe: Option<CutRecipe>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub recorded_at: Option<String>,
    pub recorded_source: Option<&'a str>,
    pub owner_id: Option<String>,
    pub source_hash: Option<String>,
    pub device_id: Option<String>,
    pub game: Option<String>,
    /// Seconds: the clip is published with its end as the current version.
    pub keep_last: Option<f64>,
    /// The audio tracks' names a device gave, for a file that carries none (an MP4 of OBS's).
    pub audio_names: Option<Vec<String>>,
}

impl<'a> NewClip<'a> {
    pub fn new(input: &'a Path, source: &'a str) -> Self {
        Self {
            input,
            source,
            relative: None,
            recipe: None,
            title: None,
            description: None,
            recorded_at: None,
            recorded_source: None,
            owner_id: None,
            source_hash: None,
            device_id: None,
            game: None,
            keep_last: None,
            audio_names: None,
        }
    }
}

/// The title from a file's name: one the parser made from the name's moment is made again
/// from the moment the clip keeps, in the pages' zone, so it says what the meta line says.
pub fn parsed_title(parsed: &reel_core::names::Parsed, recorded_at: Option<&str>) -> String {
    if reel_core::names::made_from_moment(parsed) {
        if let Some(t) =
            recorded_at.and_then(|r| reel_core::names::title_at(parsed.game.as_deref(), r))
        {
            return t;
        }
    }
    parsed.title.clone()
}

pub async fn create_clip_for_file(ctx: &Ctx, new: NewClip<'_>) -> anyhow::Result<(Clip, Job)> {
    crate::library::ensure(ctx).await?;
    let name = new
        .input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("clip")
        .to_string();
    let parsed = parse_name(&name);
    let from_name = parsed
        .recorded_at
        .and_then(|d| d.to_zoned(jiff::tz::TimeZone::system()).ok())
        .map(|z| z.timestamp().to_string());
    let (recorded, recorded_source) = match (new.recorded_at, from_name) {
        (Some(given), _) => (
            Some(given),
            new.recorded_source.unwrap_or("given").to_string(),
        ),
        (None, Some(parsed_at)) => (Some(parsed_at), "name".to_string()),
        (None, None) => (None, String::new()),
    };
    let made = parsed_title(&parsed, recorded.as_deref());
    let mut clip = Clip::new(
        new_id(),
        new.title.filter(|t| !t.trim().is_empty()).unwrap_or(made),
        new.source,
        name,
        recorded,
    );
    if !recorded_source.is_empty() {
        clip.recorded_source = Some(recorded_source);
    }
    if let Some(d) = new.description {
        clip.description = d;
    }
    clip.owner_id = new.owner_id;
    clip.source_hash = new.source_hash;
    clip.device_id = new.device_id;
    clip.game = new.game.filter(|g| !g.trim().is_empty()).or(parsed.game);
    ctx.store.insert_clip(&clip).await.context("insert clip")?;
    let payload = json!({ "input": new.input.to_string_lossy(), "source": new.source, "relative": new.relative, "recipe": new.recipe, "keep_last": new.keep_last, "audio_names": new.audio_names });
    // an upload's clip is about to be shared: its ingest runs in the lane, others wait their turn
    let priority = if matches!(new.source, "device" | "upload") {
        PRIORITY_EMBED
    } else {
        PRIORITY_INGEST
    };
    let job = Job::new("ingest", Some(clip.id.clone()), None, payload, priority);
    ctx.store.enqueue(&job).await.context("enqueue ingest")?;
    ctx.emit(Event::Clip {
        id: clip.id.clone(),
        state: "processing".into(),
        title: clip.title.clone(),
    });
    Ok((clip, job))
}

pub enum CutTarget {
    Version,
    NewClip {
        title: String,
        owner_id: Option<String>,
    },
}

pub struct CutRequest {
    pub rendition: Rendition,
    pub job: Job,
    pub new_clip: Option<Clip>,
}

pub async fn request_cut(
    ctx: &Ctx,
    clip: &Clip,
    recipe: CutRecipe,
    target: CutTarget,
) -> anyhow::Result<CutRequest> {
    let master_id = clip
        .master_id
        .clone()
        .ok_or_else(|| anyhow!("clip has no master yet"))?;
    let recipe_json = serde_json::to_string(&recipe)?;
    let (r, new_clip) = match target {
        CutTarget::Version => {
            let seq = ctx.store.next_cut_seq(&clip.id).await?;
            (
                Rendition::queued(
                    new_id(),
                    clip.id.clone(),
                    "cut",
                    Some(master_id),
                    Some(recipe_json),
                    seq,
                ),
                None,
            )
        }
        CutTarget::NewClip { title, owner_id } => {
            let title = if title.trim().is_empty() {
                format!("{} cut", clip.title)
            } else {
                title.trim().to_string()
            };
            let mut c = Clip::new(
                new_id(),
                title,
                "cut",
                clip.source_name.clone(),
                clip.recorded_at.clone(),
            );
            c.source_clip_id = Some(clip.id.clone());
            c.owner_id = owner_id.or_else(|| clip.owner_id.clone());
            ctx.store.insert_clip(&c).await?;
            ctx.emit(Event::Clip {
                id: c.id.clone(),
                state: "processing".into(),
                title: c.title.clone(),
            });
            (
                Rendition::queued(
                    new_id(),
                    c.id.clone(),
                    "master",
                    Some(master_id),
                    Some(recipe_json),
                    0,
                ),
                Some(c),
            )
        }
    };
    ctx.store.insert_rendition(&r).await?;
    let job = Job::new(
        "cut",
        Some(r.clip_id.clone()),
        Some(r.id.clone()),
        json!({}),
        PRIORITY_CUT,
    );
    ctx.store.enqueue(&job).await?;
    Ok(CutRequest {
        rendition: r,
        job,
        new_clip,
    })
}

pub enum CutDelete {
    Deleted,
    Refused(String),
}

pub async fn delete_cut(ctx: &Ctx, r: &Rendition) -> anyhow::Result<CutDelete> {
    if r.kind != "cut" {
        return Ok(CutDelete::Refused(
            "only cuts can be deleted; the original goes with the clip".into(),
        ));
    }
    let clip = ctx
        .store
        .clip(&r.clip_id)
        .await?
        .ok_or_else(|| anyhow!("clip vanished"))?;
    if clip.current_id.as_deref() == Some(r.id.as_str()) {
        return Ok(CutDelete::Refused(
            "this cut is the current version; make another one current first".into(),
        ));
    }
    let pinned = ctx.store.shares_pinned_to(&r.id).await?;
    if !pinned.is_empty() {
        let n = pinned.len();
        return Ok(CutDelete::Refused(format!(
            "{n} active link{} pinned to this cut; revoke {} first",
            if n == 1 { " is" } else { "s are" },
            if n == 1 { "it" } else { "them" }
        )));
    }
    for d in ctx.store.renditions_derived_from(&r.id).await? {
        if let Some(p) = &d.path {
            let _ = tokio::fs::remove_file(ctx.config().abs(p)).await;
        }
        ctx.store.delete_rendition(&d.id).await?;
    }
    if let Some(p) = &r.path {
        let video = ctx.config().abs(p);
        let d = crate::derived::paths(&video);
        for f in [video, d.poster, d.hover, d.strip] {
            let _ = tokio::fs::remove_file(f).await;
        }
    }
    ctx.store.delete_rendition(&r.id).await?;
    Ok(CutDelete::Deleted)
}

/// Discord fetches a preview with a few seconds of patience: a version over the preview size,
/// or one that is not H.264 with AAC, needs a copy under it.
pub fn needs_embed(cfg: &reel_core::Config, r: &Rendition) -> bool {
    r.size.unwrap_or(0) as u64 > cfg.embed_bytes()
        || r.vcodec.as_deref() != Some("h264")
        || !matches!(r.acodec.as_deref(), None | Some("aac"))
}

/// The preview a new version's links need first: the storage's size while the storage is on,
/// the server's otherwise. It is queued before the version is published.
pub async fn ensure_preview(ctx: &Ctx, from: &Rendition) -> anyhow::Result<()> {
    if ctx.config().external_on() {
        ensure_embed_ext(ctx, from, PRIORITY_EMBED).await?;
    } else {
        ensure_embed(ctx, from).await?;
    }
    Ok(())
}

/// A version over the storage's preview size, or one that is not H.264 with AAC, needs a copy
/// for the storage; one under it goes there as it is.
pub fn needs_embed_ext(cfg: &reel_core::Config, r: &Rendition) -> bool {
    r.size.unwrap_or(0) as u64 > cfg.external_embed_bytes()
        || r.vcodec.as_deref() != Some("h264")
        || !matches!(r.acodec.as_deref(), None | Some("aac"))
}

/// The storage's preview copy of a version, queued when it needs one and has none at the
/// storage's current size.
pub async fn ensure_embed_ext(
    ctx: &Ctx,
    from: &Rendition,
    priority: i64,
) -> anyhow::Result<Option<Rendition>> {
    let cfg = ctx.config();
    if !cfg.share.previews || !needs_embed_ext(&cfg, from) {
        return Ok(None);
    }
    let recipe = reel_core::recipe::EmbedRecipe::for_size(&cfg, cfg.external.embed_mb).json();
    if let Some(existing) = ctx
        .store
        .derived_rendition(&from.id, "embed_ext", Some(&recipe))
        .await?
    {
        ctx.store.raise_job_priority(&existing.id, priority).await?;
        return Ok(Some(existing));
    }
    let r = Rendition::queued(
        new_id(),
        from.clip_id.clone(),
        "embed_ext",
        Some(from.id.clone()),
        Some(recipe),
        0,
    );
    ctx.store.insert_rendition(&r).await?;
    let job = Job::new(
        "embed",
        Some(from.clip_id.clone()),
        Some(r.id.clone()),
        json!({}),
        priority,
    );
    ctx.store.enqueue(&job).await?;
    Ok(Some(r))
}

/// The file a version's links get from the storage: the version itself when it needs no copy,
/// its copy at the storage's size once that is ready.
pub enum ExtFile {
    Ready(Box<Rendition>),
    Making(Option<f64>),
    Failed(String),
}

/// The file a version's preview would go up as, without making anything: None while its copy
/// is not made yet, the reason when it failed.
pub async fn ext_file_made(
    ctx: &Ctx,
    version: &Rendition,
) -> anyhow::Result<Result<Option<Rendition>, String>> {
    let cfg = ctx.config();
    if !needs_embed_ext(&cfg, version) {
        return Ok(Ok(Some(version.clone())));
    }
    let recipe = reel_core::recipe::EmbedRecipe::for_size(&cfg, cfg.external.embed_mb).json();
    Ok(
        match ctx
            .store
            .derived_rendition(&version.id, "embed_ext", Some(&recipe))
            .await?
        {
            Some(c) if c.is_ready() => Ok(Some(c)),
            Some(c) if c.state == "failed" => Err(cfg.hide_paths(
                c.error
                    .as_deref()
                    .unwrap_or("the preview video could not be made"),
            )),
            _ => Ok(None),
        },
    )
}

/// A copy that is missing is queued at `priority`.
pub async fn ext_file(ctx: &Ctx, version: &Rendition, priority: i64) -> anyhow::Result<ExtFile> {
    let cfg = ctx.config();
    if !needs_embed_ext(&cfg, version) {
        return Ok(ExtFile::Ready(Box::new(version.clone())));
    }
    let copy = match ensure_embed_ext(ctx, version, priority).await? {
        Some(c) => c,
        None => return Ok(ExtFile::Ready(Box::new(version.clone()))),
    };
    let copy = ctx.store.rendition(&copy.id).await?.unwrap_or(copy);
    Ok(match copy.state.as_str() {
        _ if copy.is_ready() => ExtFile::Ready(Box::new(copy)),
        "failed" => ExtFile::Failed(
            cfg.hide_paths(
                copy.error
                    .as_deref()
                    .unwrap_or("the preview video could not be made"),
            ),
        ),
        _ => ExtFile::Making(
            ctx.store
                .active_job_for_rendition(&copy.id)
                .await?
                .map(|j| j.progress),
        ),
    })
}

pub async fn ensure_embed(ctx: &Ctx, from: &Rendition) -> anyhow::Result<Option<Rendition>> {
    ensure_embed_at(ctx, from, PRIORITY_EMBED).await
}

pub async fn ensure_embed_at(
    ctx: &Ctx,
    from: &Rendition,
    priority: i64,
) -> anyhow::Result<Option<Rendition>> {
    let cfg = ctx.config();
    if !cfg.share.previews || !needs_embed(&cfg, from) {
        return Ok(None);
    }
    let recipe = reel_core::recipe::EmbedRecipe::current(&cfg).json();
    if let Some(existing) = ctx
        .store
        .derived_rendition(&from.id, "embed", Some(&recipe))
        .await?
    {
        ctx.store.raise_job_priority(&existing.id, priority).await?;
        return Ok(Some(existing));
    }
    let r = Rendition::queued(
        new_id(),
        from.clip_id.clone(),
        "embed",
        Some(from.id.clone()),
        Some(recipe),
        0,
    );
    ctx.store.insert_rendition(&r).await?;
    let job = Job::new(
        "embed",
        Some(from.clip_id.clone()),
        Some(r.id.clone()),
        json!({}),
        priority,
    );
    ctx.store.enqueue(&job).await?;
    Ok(Some(r))
}

pub async fn ensure_discord(
    ctx: &Ctx,
    from: &Rendition,
    limit_bytes: u64,
) -> anyhow::Result<Rendition> {
    let recipe =
        reel_core::recipe::DiscordRecipe::for_limit(limit_bytes, &ctx.config().media).json();
    if let Some(existing) = ctx
        .store
        .derived_rendition(&from.id, "discord", Some(&recipe))
        .await?
    {
        if existing.is_ready() {
            if ctx
                .config()
                .abs(existing.path.as_deref().unwrap_or(""))
                .exists()
            {
                return Ok(existing);
            }
            ctx.store.delete_rendition(&existing.id).await?;
        } else {
            return Ok(existing);
        }
    }
    let src = crate::discord::source_info(ctx, from).await?;
    reel_core::plan::copy_plan(
        limit_bytes,
        from.duration.unwrap_or(0.0),
        &src,
        &ctx.config().media,
        1440,
        reel_core::plan::Purpose::Limited,
    )
    .map_err(|e| anyhow!(e))?;
    let r = Rendition::queued(
        new_id(),
        from.clip_id.clone(),
        "discord",
        Some(from.id.clone()),
        Some(recipe),
        0,
    );
    ctx.store.insert_rendition(&r).await?;
    let job = Job::new(
        "discord",
        Some(from.clip_id.clone()),
        Some(r.id.clone()),
        json!({}),
        PRIORITY_DISCORD,
    );
    ctx.store.enqueue(&job).await?;
    Ok(r)
}

/// The animated picture of a version with this recipe: the one made before when its file is
/// there, else one queued at the lowest priority; a failed one is tried again.
pub async fn ensure_anim(
    ctx: &Ctx,
    from: &Rendition,
    recipe: reel_core::recipe::AnimRecipe,
) -> anyhow::Result<Rendition> {
    let key = recipe.json();
    if let Some(existing) = ctx
        .store
        .derived_rendition(&from.id, "anim", Some(&key))
        .await?
    {
        match existing.state.as_str() {
            "ready"
                if ctx
                    .config()
                    .abs(existing.path.as_deref().unwrap_or(""))
                    .exists() =>
            {
                return Ok(existing)
            }
            "queued" | "rendering" => return Ok(existing),
            _ => ctx.store.delete_rendition(&existing.id).await?,
        }
    }
    let r = Rendition::queued(
        new_id(),
        from.clip_id.clone(),
        "anim",
        Some(from.id.clone()),
        Some(key),
        0,
    );
    ctx.store.insert_rendition(&r).await?;
    let job = Job::new(
        "anim",
        Some(from.clip_id.clone()),
        Some(r.id.clone()),
        json!({}),
        PRIORITY_ANIM,
    );
    ctx.store.enqueue(&job).await?;
    crate::worker::emit_job(ctx, &job, "queued", 0.0, None);
    Ok(r)
}

/// A version plays as it is on its link's page when it is H.264 with AAC, no taller than the
/// web copy's height and under its rate, video and audio together; any other gets a web copy.
pub fn needs_web(cfg: &reel_core::Config, r: &Rendition) -> bool {
    let bits = r.size.unwrap_or(0) as f64 * 8.0 / r.duration.unwrap_or(0.0).max(0.1);
    bits > (cfg.share.web_mbit * 1_000_000) as f64
        || r.height.unwrap_or(0) > cfg.share.web_max_height as i64
        || r.vcodec.as_deref() != Some("h264")
        || !matches!(r.acodec.as_deref(), None | Some("aac"))
}

/// The web copy of a version a link was just made for, queued after its preview video; a
/// failed one is tried again.
pub async fn ensure_web(ctx: &Ctx, from: &Rendition) -> anyhow::Result<Option<Rendition>> {
    let cfg = ctx.config();
    if !from.is_ready() || !from.is_version() || !needs_web(&cfg, from) {
        return Ok(None);
    }
    let recipe = reel_core::recipe::WebRecipe::current(&cfg).json();
    if let Some(existing) = ctx.store.derived_latest(&from.id, "web", &recipe).await? {
        let file_gone = existing.is_ready()
            && !ctx
                .config()
                .abs(existing.path.as_deref().unwrap_or(""))
                .exists();
        if existing.state != "failed" && !file_gone {
            return Ok(Some(existing));
        }
        ctx.store.delete_rendition(&existing.id).await?;
    }
    let r = Rendition::queued(
        new_id(),
        from.clip_id.clone(),
        "web",
        Some(from.id.clone()),
        Some(recipe),
        0,
    );
    ctx.store.insert_rendition(&r).await?;
    let job = Job::new(
        "web",
        Some(from.clip_id.clone()),
        Some(r.id.clone()),
        json!({}),
        PRIORITY_WEB,
    );
    ctx.store.enqueue(&job).await?;
    tracing::info!(clip = %from.clip_id, version = %from.id, "web copy queued");
    Ok(Some(r))
}

/// What a link's page plays of a version: the version itself when it needs no web copy, else
/// its web copy once that is ready.
pub enum WebCopy {
    Ready(Box<Rendition>),
    Making,
    Failed,
    Missing,
}

pub async fn web_copy(ctx: &Ctx, version: &Rendition) -> anyhow::Result<WebCopy> {
    let cfg = ctx.config();
    if !needs_web(&cfg, version) {
        return Ok(WebCopy::Ready(Box::new(version.clone())));
    }
    let recipe = reel_core::recipe::WebRecipe::current(&cfg).json();
    Ok(
        match ctx
            .store
            .derived_latest(&version.id, "web", &recipe)
            .await?
        {
            Some(r) if r.is_ready() && cfg.abs(r.path.as_deref().unwrap_or("")).exists() => {
                WebCopy::Ready(Box::new(r))
            }
            Some(r) if r.state == "failed" => WebCopy::Failed,
            Some(r) if !r.is_ready() => WebCopy::Making,
            _ => WebCopy::Missing,
        },
    )
}

/// Queued once the clip is published with its poster, so the library shows it before the slow
/// pair. `whole`: the original's poster and loudness too.
pub async fn request_loop_and_strip(ctx: &Ctx, clip_id: &str, whole: bool) -> anyhow::Result<()> {
    let job = Job::new(
        "derive",
        Some(clip_id.to_string()),
        None,
        json!({ "poster": false, "whole": whole }),
        PRIORITY_DERIVE,
    );
    ctx.store.enqueue(&job).await?;
    crate::worker::emit_job(ctx, &job, "queued", 0.0, None);
    Ok(())
}

/// One job per clip with a ready version, at the lowest priority, that makes the posters,
/// loops and filmstrips again. How many were queued.
pub async fn rebuild_derived(ctx: &Ctx) -> anyhow::Result<usize> {
    let mut queued = 0;
    for row in ctx.store.clips("", reel_store::Sort::Newest).await? {
        if row.clip.state != "ready" {
            continue;
        }
        let job = Job::new(
            "derive",
            Some(row.clip.id.clone()),
            None,
            json!({}),
            PRIORITY_DERIVE,
        );
        ctx.store.enqueue(&job).await?;
        crate::worker::emit_job(ctx, &job, "queued", 0.0, None);
        queued += 1;
    }
    Ok(queued)
}

/// Queues a derive job for every ready clip whose poster time is still its version's default;
/// a picked poster stays.
pub async fn remake_default_posters(ctx: &Ctx) -> anyhow::Result<usize> {
    let mut queued = 0;
    for row in ctx.store.clips("", reel_store::Sort::Newest).await? {
        if row.clip.state != "ready" || row.clip.poster_image.is_some() {
            continue;
        }
        let duration = row.duration.unwrap_or(0.0);
        let default = crate::derived::default_poster_time(duration);
        if let Some(t) = row.clip.poster_time {
            if (t - default).abs() >= 0.001 {
                continue;
            }
        }
        let job = Job::new(
            "derive",
            Some(row.clip.id.clone()),
            None,
            json!({ "poster": true }),
            PRIORITY_DERIVE,
        );
        ctx.store.enqueue(&job).await?;
        crate::worker::emit_job(ctx, &job, "queued", 0.0, None);
        queued += 1;
    }
    Ok(queued)
}

/// What the remake found: copies queued, of which missing under the current size, and made
/// under other rules or another size.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Remade {
    pub queued: usize,
    pub missing: usize,
    pub stale: usize,
}

/// Queues a preview copy for every current version that needs one and has none under the
/// current rules.
pub async fn remake_embeds(ctx: &Ctx) -> anyhow::Result<Remade> {
    let cfg = ctx.config();
    let want = reel_core::recipe::EmbedRecipe::current(&cfg).json();
    let mut out = Remade::default();
    for row in ctx.store.clips("", reel_store::Sort::Newest).await? {
        let Some(cid) = row.clip.current_id.as_deref() else {
            continue;
        };
        let Some(current) = ctx.store.rendition(cid).await? else {
            continue;
        };
        if !current.is_ready() || !needs_embed(&cfg, &current) {
            continue;
        }
        let old = ctx
            .store
            .derived_rendition(&current.id, "embed", None)
            .await?;
        match &old {
            Some(o) if o.recipe.as_deref() == Some(want.as_str()) => continue,
            Some(o) => {
                if o.is_ready() {
                    if let Some(p) = &o.path {
                        let _ = tokio::fs::remove_file(ctx.config().abs(p)).await;
                    }
                }
                ctx.store.delete_rendition(&o.id).await?;
                out.stale += 1;
            }
            None => out.missing += 1,
        }
        if ensure_embed(ctx, &current).await?.is_some() {
            out.queued += 1;
        }
    }
    Ok(out)
}

/// What the share page serves: the embed copy once ready, nothing while a copy is on its way
/// or failed, the version itself when no copy was ever queued.
pub async fn embed_grade(ctx: &Ctx, current: &Rendition) -> anyhow::Result<Option<Rendition>> {
    let copy = ctx
        .store
        .derived_rendition(&current.id, "embed", None)
        .await?;
    match copy {
        Some(c) if c.is_ready() => Ok(Some(c)),
        Some(_) => Ok(None),
        None => Ok(Some(current.clone())),
    }
}

fn start_in_original(version: &Rendition) -> f64 {
    if version.kind == "master" {
        return 0.0;
    }
    version
        .recipe
        .as_deref()
        .and_then(|r| serde_json::from_str::<CutRecipe>(r).ok())
        .and_then(|r| r.start)
        .unwrap_or(0.0)
        .max(0.0)
}

/// The frame someone chose as the clip's poster, at its time in `next`: none for an automatic
/// frame or an uploaded image, and none when `next` does not hold that moment of the original.
async fn chosen_frame_in(ctx: &Ctx, clip: &Clip, next: &Rendition) -> anyhow::Result<Option<f64>> {
    let (None, Some(at), Some(current_id)) = (
        &clip.poster_image,
        clip.poster_time,
        clip.current_id.as_deref(),
    ) else {
        return Ok(None);
    };
    let Some(current) = ctx.store.rendition(current_id).await? else {
        return Ok(None);
    };
    let auto = crate::derived::default_poster_time(current.duration.unwrap_or(0.0));
    if (at - auto).abs() < 0.001 {
        return Ok(None);
    }
    let t = start_in_original(&current) + at - start_in_original(next);
    let last = (next.duration.unwrap_or(0.0) - 0.1).max(0.0);
    Ok((-0.05..=last + 0.05)
        .contains(&t)
        .then(|| t.clamp(0.0, last)))
}

/// Makes a version the clip's current one with the poster that follows the clip: its uploaded
/// image, the frame chosen when the version holds it, else the version's automatic frame.
pub async fn make_current(ctx: &Ctx, clip: &Clip, version: &Rendition) -> anyhow::Result<()> {
    let duration = version.duration.unwrap_or(0.0);
    let chosen = chosen_frame_in(ctx, clip, version).await?;
    let at = chosen.unwrap_or_else(|| crate::derived::default_poster_time(duration));
    ctx.store.set_current(&clip.id, &version.id, at).await?;
    if clip.poster_image.is_some() {
        crate::poster_image::keep(ctx, &clip.id, &version.id).await;
    } else if let Some(path) = version.path.as_deref() {
        let video = ctx.config().abs(path);
        let made = match chosen {
            Some(t) => crate::derived::poster(ctx, &video, t).await,
            None => crate::derived::poster_default(ctx, &video, duration)
                .await
                .map(|_| ()),
        };
        if let Err(e) = made {
            tracing::warn!(clip = %clip.id, error = %e, "the poster of the version made current");
        }
    }
    Ok(())
}

pub async fn set_poster(ctx: &Ctx, clip: &Clip, at: f64) -> anyhow::Result<()> {
    let current = match &clip.current_id {
        Some(id) => ctx.store.rendition(id).await?,
        None => None,
    };
    let current = current.ok_or_else(|| anyhow!("no current rendition"))?;
    let path = ctx.config().abs(
        current
            .path
            .as_deref()
            .ok_or_else(|| anyhow!("rendition has no file"))?,
    );
    let at = at.clamp(0.0, current.duration.unwrap_or(0.0).max(0.0));
    crate::derived::poster(ctx, &path, at).await?;
    ctx.store.set_poster_time(&clip.id, at).await?;
    Ok(())
}

/// To the trash: the links are revoked, the directory moves, and the jobs still making things
/// for the clip are cancelled.
pub async fn trash_clip(ctx: &Ctx, clip: &Clip) -> anyhow::Result<()> {
    crate::library::ensure(ctx).await?;
    ctx.store.trash_clip(&clip.id).await?;
    let busy = |jobs: &[Job]| {
        jobs.iter()
            .filter(|j| j.kind != "ingest" && matches!(j.state.as_str(), "queued" | "running"))
            .map(|j| j.id.clone())
            .collect::<Vec<_>>()
    };
    for id in busy(&ctx.store.jobs_of_clip(&clip.id).await?) {
        let _ = crate::jobs::cancel(ctx, &id).await;
    }
    for _ in 0..100 {
        if busy(&ctx.store.jobs_of_clip(&clip.id).await?).is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let from = ctx.config().clip_dir(&clip.id);
    let to = ctx.config().trash_dir().join(&clip.id);
    if from.exists() {
        let _held = ctx.library.moves.lock().await;
        tokio::fs::create_dir_all(ctx.config().trash_dir()).await?;
        move_dir(&from, &to).await.context("move to trash")?;
    }
    ctx.emit(Event::Clip {
        id: clip.id.clone(),
        state: "trashed".into(),
        title: clip.title.clone(),
    });
    Ok(())
}

pub async fn restore_clip(ctx: &Ctx, clip_id: &str) -> anyhow::Result<bool> {
    crate::library::ensure(ctx).await?;
    let _held = ctx.library.moves.lock().await;
    let from = crate::library::trashed_dir(&ctx.config(), clip_id);
    let to = ctx.config().clip_dir(clip_id);
    if from.exists() && !to.exists() {
        move_dir(&from, &to).await.context("restore from trash")?;
    }
    ctx.store.restore_clip(clip_id).await.map_err(Into::into)
}

/// Renamed where it can be; across filesystems, copied beside its target under a part name,
/// renamed there, and only then removed where it was.
pub async fn move_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    match tokio::fs::rename(from, to).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            copy_dir_then_swap(from, to).await
        }
        Err(e) => Err(e.into()),
    }
}

async fn copy_dir_then_swap(from: &Path, to: &Path) -> anyhow::Result<()> {
    let part = reel_media::ffmpeg::part_path(to);
    // the trash is emptied by clip id, so a part left by a crash is cleared here
    let _ = tokio::fs::remove_dir_all(&part).await;
    let (src, dst) = (from.to_path_buf(), part.clone());
    let copied = tokio::task::spawn_blocking(move || copy_tree(&src, &dst)).await?;
    if let Err(e) = copied {
        let _ = tokio::fs::remove_dir_all(&part).await;
        return Err(e).with_context(|| format!("copy {} to {}", from.display(), to.display()));
    }
    tokio::fs::rename(&part, to).await?;
    tokio::fs::remove_dir_all(from).await?;
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub async fn move_or_copy(from: &Path, to: &Path) -> anyhow::Result<()> {
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    match tokio::fs::rename(from, to).await {
        Ok(()) => Ok(()),
        Err(_) => {
            let part = reel_media::ffmpeg::part_path(to);
            tokio::fs::copy(from, &part)
                .await
                .with_context(|| format!("copy {} to {}", from.display(), to.display()))?;
            tokio::fs::rename(&part, to).await?;
            tokio::fs::remove_file(from).await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(root: &Path) {
        std::fs::create_dir_all(root.join("v1")).unwrap();
        std::fs::write(root.join("master.mp4"), b"master").unwrap();
        std::fs::write(root.join("v1").join("cut.mp4"), b"cut").unwrap();
    }

    #[tokio::test]
    async fn a_directory_moves_within_a_filesystem() {
        let tmp = tempfile::tempdir().unwrap();
        let (from, to) = (
            tmp.path().join("library").join("c1"),
            tmp.path().join("trash").join("c1"),
        );
        tree(&from);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        move_dir(&from, &to).await.unwrap();
        assert!(!from.exists());
        assert_eq!(
            std::fs::read(to.join("v1").join("cut.mp4")).unwrap(),
            b"cut"
        );
    }

    #[tokio::test]
    async fn across_filesystems_it_is_copied_then_swapped() {
        // the path a rename across filesystems takes (CrossesDevices), on one disk
        let tmp = tempfile::tempdir().unwrap();
        let (from, to) = (
            tmp.path().join("library").join("c1"),
            tmp.path().join("trash").join("c1"),
        );
        tree(&from);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::create_dir_all(reel_media::ffmpeg::part_path(&to).join("stale")).unwrap();
        copy_dir_then_swap(&from, &to).await.unwrap();
        assert!(!from.exists());
        assert!(!reel_media::ffmpeg::part_path(&to).exists());
        assert_eq!(std::fs::read(to.join("master.mp4")).unwrap(), b"master");
        assert_eq!(
            std::fs::read(to.join("v1").join("cut.mp4")).unwrap(),
            b"cut"
        );
        assert!(!to.join("stale").exists());
    }

    #[tokio::test]
    async fn a_failed_copy_leaves_the_source_and_no_part() {
        let tmp = tempfile::tempdir().unwrap();
        let to = tmp.path().join("trash").join("c1");
        let missing = tmp.path().join("library").join("gone");
        assert!(copy_dir_then_swap(&missing, &to).await.is_err());
        assert!(!reel_media::ffmpeg::part_path(&to).exists());
        assert!(!to.exists());
    }
}
