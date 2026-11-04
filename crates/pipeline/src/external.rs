//! Link previews from an S3-compatible bucket; one task keeps it as the links want it.

use crate::enqueue::{
    ensure_embed, ensure_embed_ext, ext_file, ext_file_made, needs_embed, ExtFile,
};
use crate::s3::{Bucket, S3};
use crate::Ctx;
use reel_core::{Config, Event};
use reel_store::models::{ExternalObject, Rendition, PRIORITY_DERIVE, PRIORITY_EMBED};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Notify};

/// Everything reel writes goes under this prefix; nothing outside it is touched.
pub const PREFIX: &str = "reel/";

pub fn bucket_of(cfg: &Config) -> Bucket {
    Bucket {
        endpoint: cfg.external.endpoint.clone(),
        region: if cfg.external.region.trim().is_empty() {
            "auto".into()
        } else {
            cfg.external.region.trim().to_string()
        },
        name: cfg.external.bucket.clone(),
        access_key: cfg.external.access_key.clone(),
        secret_key: cfg.external.secret_key.clone(),
    }
}

pub fn public_url(cfg: &Config, key: &str) -> String {
    format!("{}/{key}", cfg.external.public_url.trim_end_matches('/'))
}

/// What the storage task shares with the routes.
pub struct State {
    kick: Notify,
    passes: watch::Sender<u64>,
    pub report: std::sync::Mutex<Option<Report>>,
    listed_at: std::sync::Mutex<Option<std::time::Instant>>,
    alarm: std::sync::Mutex<Alarm>,
    /// When the next live link expires: its preview goes then, not at the ten minute pass.
    next_expiry: std::sync::Mutex<Option<jiff::Timestamp>>,
    /// The uploads under way, by version: the bytes sent and the size.
    going_up: std::sync::Mutex<HashMap<String, (Arc<AtomicU64>, u64)>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            kick: Notify::new(),
            passes: watch::channel(0).0,
            report: Default::default(),
            listed_at: Default::default(),
            alarm: Default::default(),
            next_expiry: Default::default(),
            going_up: Default::default(),
        }
    }
}

impl State {
    /// How far a version's preview is uploaded, 0 to 1, while it goes up.
    pub fn going_up(&self, version_id: &str) -> Option<f64> {
        let up = self.going_up.lock().unwrap();
        let (sent, size) = up.get(version_id)?;
        Some((sent.load(Ordering::Relaxed) as f64 / (*size).max(1) as f64).min(1.0))
    }
}

/// What the last pass found.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub at: String,
    pub used: u64,
    pub limit: u64,
    /// Bytes and objects per class, in the storage's order.
    pub by_class: Vec<(Class, u64, usize)>,
    /// Linked versions served from here, by reason.
    pub from_here: Vec<(String, usize)>,
    /// Linked versions on their way up.
    pub pending: usize,
    pub failures: Vec<String>,
    /// Why the storage could not be used at all.
    pub error: Option<String>,
    /// Records whose object the listing lacked, and objects under the prefix without a record.
    pub missing: usize,
    pub unknown: usize,
}

/// Where the alarms stand: the share of the limit last told about, and whether a pass failed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Alarm {
    pub level: u8,
    pub failing: bool,
}

/// The share of the limit that calls for a note: 100, 80, or 0 under that.
pub fn level(used: u64, limit: u64) -> u8 {
    if used >= limit && (used > 0 || limit > 0) {
        100
    } else if used * 5 >= limit * 4 && used > 0 {
        80
    } else {
        0
    }
}

/// The notes a pass calls for: a level reached that was not told before, and a failure after
/// a pass without one.
pub fn alarms(before: Alarm, now: Alarm) -> (Option<u8>, bool) {
    let level = (now.level > before.level).then_some(now.level);
    (level, now.failing && !before.failing)
}

impl Report {
    /// The line Status and the setting show.
    pub fn line(&self) -> String {
        use reel_core::format::size;
        if let Some(e) = &self.error {
            return format!("cannot be used: {e}");
        }
        let pct = (self.used * 100).checked_div(self.limit).unwrap_or(100);
        let mut line = format!("{} of {} ({pct} %)", size(self.used), size(self.limit));
        let classes: Vec<String> = self
            .by_class
            .iter()
            .filter(|(_, _, n)| *n > 0)
            .map(|(c, b, n)| format!("{} {} in {n}", c.name(), size(*b)))
            .collect();
        if !classes.is_empty() {
            line.push_str(&format!(": {}", classes.join(", ")));
        }
        let here: usize = self.from_here.iter().map(|(_, n)| n).sum();
        if here > 0 {
            let why: Vec<String> = self
                .from_here
                .iter()
                .map(|(w, n)| match self.from_here.len() {
                    1 => why_words(w).to_string(),
                    _ => format!("{} for {n}", why_words(w)),
                })
                .collect();
            line.push_str(&format!(
                "; {here} link{} served from your server: {}",
                if here == 1 {
                    "'s preview"
                } else {
                    "s' previews"
                },
                why.join(", ")
            ));
        }
        if self.pending > 0 {
            line.push_str(&format!("; {} going up", self.pending));
        }
        if let Some(f) = self.failures.first() {
            line.push_str(&format!("; failed: {f}"));
        }
        if self.missing > 0 {
            line.push_str(&format!("; {} found missing, going up again", self.missing));
        }
        if self.unknown > 0 {
            line.push_str(&format!(
                "; {} under reel/ without a record, left alone",
                self.unknown
            ));
        }
        line
    }

    /// Status's colour for the line.
    pub fn class(&self) -> &'static str {
        if self.error.is_some() {
            "st-err"
        } else if !self.failures.is_empty() || level(self.used, self.limit) > 0 {
            "st-warn"
        } else {
            "st-ok"
        }
    }
}

impl State {
    /// Something the storage follows changed: a link, a version, a setting.
    pub fn kick(&self) {
        self.kick.notify_one();
    }
}

/// What a preview in the storage is there for, in the order the storage keeps them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    Shared,
    Edited,
    New,
    Old,
}

impl Class {
    pub fn name(&self) -> &'static str {
        match self {
            Class::Shared => "shared",
            Class::Edited => "edited",
            Class::New => "new",
            Class::Old => "old",
        }
    }
}

/// One version whose preview may be in the storage.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub version_id: String,
    pub class: Class,
    pub size: u64,
    /// Its preview is there already, the file it would be now.
    pub stored: bool,
    /// One of its links was made inside the protected window.
    pub protected: bool,
    /// The latest post or opening among its links, in seconds.
    pub activity: Option<i64>,
    /// When its newest link was made, in seconds.
    pub newest_link: i64,
    /// When its clip came in, in seconds, for the previews made ahead.
    pub age: i64,
}

fn tier(c: &Candidate) -> u8 {
    match c.class {
        Class::Shared if c.protected && c.stored => 0,
        Class::Shared if c.protected => 1,
        Class::Shared if c.activity.is_some() => 2,
        Class::Shared => 3,
        Class::Edited => 4,
        Class::New => 5,
        Class::Old => 6,
    }
}

/// What the storage keeps, in the order it keeps them, and what is left out for want of room.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub keep: Vec<usize>,
    pub out: Vec<usize>,
}

/// Down the order of `tier`, each preview goes in while it fits under the limit; a protected
/// preview already there stays whatever the limit.
pub fn plan(candidates: &[Candidate], limit: u64) -> Plan {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&a, &b| {
        let (x, y) = (&candidates[a], &candidates[b]);
        tier(x)
            .cmp(&tier(y))
            .then_with(|| match tier(x) {
                0 | 1 => x.newest_link.cmp(&y.newest_link),
                2 => y.activity.cmp(&x.activity),
                3 => y.newest_link.cmp(&x.newest_link),
                _ => y.age.cmp(&x.age),
            })
            .then_with(|| x.version_id.cmp(&y.version_id))
    });
    let mut used = 0u64;
    let mut out = Plan::default();
    for i in order {
        let c = &candidates[i];
        if tier(c) == 0 || used + c.size <= limit {
            used += c.size;
            out.keep.push(i);
        } else {
            out.out.push(i);
        }
    }
    out
}

/// Why a version's links are served from this server while the storage is on.
pub fn why_words(why: &str) -> &'static str {
    match why {
        "limit" => "the storage's limit was reached",
        "unreachable" => "the storage cannot be reached",
        "failed" => "the upload failed",
        "copy" => "the preview video for the storage could not be made",
        _ => "the storage is off",
    }
}

/// Where a version's links get their preview from.
#[derive(Debug, Clone)]
pub enum Where {
    /// The storage is off and holds nothing of this version.
    Off,
    Storage(Box<ExternalObject>),
    Server {
        why: String,
        detail: String,
    },
    /// On its way to the storage.
    Pending,
}

pub async fn where_from(ctx: &Ctx, version_id: &str) -> anyhow::Result<Where> {
    let stored = ctx.store.stored_external_object(version_id).await?;
    if !ctx.config().external_on() {
        return Ok(match stored {
            Some(_) => Where::Server {
                why: "off".into(),
                detail: String::new(),
            },
            None => Where::Off,
        });
    }
    if let Some(o) = stored {
        return Ok(Where::Storage(Box::new(o)));
    }
    Ok(match ctx.store.external_fallback(version_id).await? {
        Some(f) => Where::Server {
            why: f.why,
            detail: f.detail,
        },
        None => Where::Pending,
    })
}

/// The words a link's row says about its preview; empty while the storage was never used.
pub fn where_words(w: &Where) -> String {
    match w {
        Where::Off => String::new(),
        Where::Storage(_) => "preview from storage".into(),
        Where::Server { why, .. } => format!("preview from your server: {}", why_words(why)),
        Where::Pending => "preview going up to the storage".into(),
    }
}

/// What a new link's maker reads when its preview is not in the storage, or not ready yet.
pub fn where_note(w: &Where, ready: bool) -> String {
    match (w, ready) {
        (Where::Server { why, .. }, true) => {
            format!("Its preview comes from your server: {}.", why_words(why))
        }
        (Where::Server { why, .. }, false) => format!(
            "Its preview comes from your server ({}) and is still being made there; post the link in a minute.",
            why_words(why)
        ),
        (Where::Pending, _) => {
            "Its preview is still going up to the storage; post the link in a minute.".into()
        }
        _ => String::new(),
    }
}

/// Where every version's links get their preview, loaded once for a page of links.
pub struct Overview {
    on: bool,
    stored: HashSet<String>,
    fallbacks: HashMap<String, String>,
}

impl Overview {
    pub async fn load(ctx: &Ctx) -> anyhow::Result<Self> {
        Ok(Self {
            on: ctx.config().external_on(),
            stored: ctx
                .store
                .external_objects()
                .await?
                .into_iter()
                .filter(|o| o.state == "stored")
                .map(|o| o.version_id)
                .collect(),
            fallbacks: ctx
                .store
                .external_fallbacks()
                .await?
                .into_iter()
                .map(|f| (f.version_id, f.why))
                .collect(),
        })
    }

    /// The words of a live link's row, for the version it serves.
    pub fn words(&self, version_id: &str) -> String {
        let place = match (self.on, self.stored.contains(version_id)) {
            (false, false) => Where::Off,
            (false, true) => Where::Server {
                why: "off".into(),
                detail: String::new(),
            },
            (true, true) => return "preview from storage".into(),
            (true, false) => match self.fallbacks.get(version_id) {
                Some(why) => Where::Server {
                    why: why.clone(),
                    detail: String::new(),
                },
                None => Where::Pending,
            },
        };
        where_words(&place)
    }

    /// The same in a word for a table's cell, and whether the storage left this link out.
    pub fn word(&self, version_id: &str) -> (&'static str, bool) {
        match (self.on, self.stored.contains(version_id)) {
            (false, false) => ("", false),
            (false, true) => ("server", false),
            (true, true) => ("storage", false),
            (true, false) if self.fallbacks.contains_key(version_id) => ("server", true),
            (true, false) => ("going up", false),
        }
    }
}

/// The server's preview of a version when it is served from here: the version itself when it
/// needs no copy, its copy once that is ready; `queue` makes the copy when there is none.
pub async fn server_preview(
    ctx: &Ctx,
    version: &Rendition,
    queue: bool,
) -> anyhow::Result<Option<Rendition>> {
    if !needs_embed(&ctx.config(), version) {
        return Ok(Some(version.clone()));
    }
    if queue {
        ensure_embed(ctx, version).await?;
    }
    Ok(ctx
        .store
        .derived_rendition(&version.id, "embed", None)
        .await?
        .filter(|c| c.is_ready()))
}

/// Whether a version's links are ready to hand out, and if not the step they wait on.
pub struct Readiness {
    pub ready: bool,
    pub step: &'static str,
    pub progress: f64,
}

/// With the storage on: ready once the preview is there, or once it is served from here
/// with its server copy made; otherwise waiting for the storage's copy or its upload.
pub async fn readiness(ctx: &Ctx, version: &Rendition) -> anyhow::Result<Readiness> {
    let waiting = |step, progress: Option<f64>| Readiness {
        ready: false,
        step,
        progress: progress.unwrap_or(0.0),
    };
    match where_from(ctx, &version.id).await? {
        Where::Storage(_) => Ok(Readiness {
            ready: true,
            step: "",
            progress: 1.0,
        }),
        Where::Server { .. } | Where::Off => match server_preview(ctx, version, true).await? {
            Some(_) => Ok(Readiness {
                ready: true,
                step: "",
                progress: 1.0,
            }),
            None => {
                let copy = ctx
                    .store
                    .derived_rendition(&version.id, "embed", None)
                    .await?;
                let p = match copy {
                    Some(c) => ctx
                        .store
                        .active_job_for_rendition(&c.id)
                        .await?
                        .map(|j| j.progress),
                    None => None,
                };
                Ok(waiting("preview", p))
            }
        },
        Where::Pending => Ok(match ext_file(ctx, version, PRIORITY_EMBED).await? {
            ExtFile::Making(p) => waiting("preview", p),
            _ => waiting("upload", ctx.external.going_up(&version.id)),
        }),
    }
}

/// Waits at most `limit` until a version's links have their preview, in the storage or served
/// from here with the server's copy made, kicking the storage task. Whether it is ready by then.
pub async fn settle(
    ctx: &Ctx,
    version: &Rendition,
    limit: Duration,
) -> anyhow::Result<(Where, bool)> {
    let mut passes = ctx.external.passes.subscribe();
    let deadline = tokio::time::Instant::now() + limit;
    ctx.external.kick();
    loop {
        let w = where_from(ctx, &version.id).await?;
        let ready = match &w {
            Where::Storage(_) => true,
            Where::Pending => false,
            Where::Server { .. } | Where::Off => {
                server_preview(ctx, version, true).await?.is_some()
            }
        };
        if ready {
            return Ok((w, true));
        }
        let waited = tokio::select! {
            r = passes.changed() => r.is_ok(),
            _ = tokio::time::sleep(Duration::from_millis(500)) => true,
        };
        if !waited || tokio::time::Instant::now() >= deadline {
            return Ok((w, false));
        }
    }
}

/// A pass asked for now, and its report's line once it is done (Check now); at most a minute.
pub async fn check_now(ctx: &Ctx) -> String {
    let mut passes = ctx.external.passes.subscribe();
    ctx.external.kick();
    for _ in 0..2 {
        if tokio::time::timeout(Duration::from_secs(60), passes.changed())
            .await
            .is_err()
        {
            break;
        }
    }
    usage_line(ctx)
}

/// What the storage holds, as the setting and Status say it.
pub fn usage_line(ctx: &Ctx) -> String {
    if !ctx.config().external_on() {
        return "the storage is off".into();
    }
    match ctx.external.report.lock().unwrap().as_ref() {
        Some(r) => r.line(),
        None => "not checked yet".into(),
    }
}

/// The storage task: a pass at start, then at every kick, every change of a clip, every
/// preview copy made, and every ten minutes.
pub async fn run(ctx: Ctx) {
    let mut bus = ctx.bus.subscribe();
    loop {
        if let Err(e) = pass(&ctx).await {
            tracing::warn!(error = %e, "external storage");
        }
        ctx.external.passes.send_modify(|n| *n += 1);
        let wake = async {
            loop {
                match bus.recv().await {
                    Ok(Event::Clip { .. }) => return,
                    Ok(Event::Job { kind, state, .. })
                        if kind == "embed"
                            && matches!(state.as_str(), "done" | "failed" | "cancelled") =>
                    {
                        return
                    }
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => std::future::pending::<()>().await,
                }
            }
        };
        let until_expiry = ctx
            .external
            .next_expiry
            .lock()
            .unwrap()
            .map(|t| (t.as_second() - jiff::Timestamp::now().as_second()).max(0) as u64 + 1)
            .unwrap_or(600);
        tokio::select! {
            _ = ctx.external.kick.notified() => {}
            _ = wake => {}
            _ = tokio::time::sleep(Duration::from_secs(until_expiry.min(600))) => {}
            _ = ctx.shutdown.cancelled() => break,
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

fn seconds(ts: &str) -> i64 {
    reel_core::format::parse(ts)
        .map(|t| t.as_second())
        .unwrap_or(0)
}

/// What one pass knows of a version its links want.
struct Wanted {
    class: Class,
    protected: bool,
    activity: Option<i64>,
    newest_link: i64,
    age: i64,
}

/// One pass: what the links want and what is made ahead, the plan, the deletes, then the
/// uploads; then what the storage holds, for Status and the alarms.
pub async fn pass(ctx: &Ctx) -> anyhow::Result<()> {
    let cfg = ctx.config();
    if !cfg.external_on() {
        *ctx.external.report.lock().unwrap() = None;
        return Ok(());
    }
    let now = jiff::Timestamp::now().as_second();
    let protect_from = now - cfg.external.protect_days as i64 * 86_400;
    let mut wanted: HashMap<String, Wanted> = HashMap::new();
    for l in ctx.store.live_links().await? {
        let Some(vid) = l.version_id else { continue };
        let made = seconds(&l.created_at);
        let act = [l.last_posted_at.as_deref(), l.last_viewed_at.as_deref()]
            .into_iter()
            .flatten()
            .map(seconds)
            .max();
        let w = wanted.entry(vid).or_insert(Wanted {
            class: Class::Shared,
            protected: false,
            activity: None,
            newest_link: made,
            age: 0,
        });
        w.protected |= cfg.external.protect_days > 0 && made >= protect_from;
        w.activity = w.activity.max(act);
        w.newest_link = w.newest_link.max(made);
    }
    let objects = ctx.store.external_objects().await?;
    let there: HashSet<&str> = objects
        .iter()
        .filter(|o| o.state == "stored")
        .map(|o| o.version_id.as_str())
        .collect();
    if cfg.external.ahead != "off" {
        let new_from = now - cfg.external.new_days as i64 * 86_400;
        for cv in ctx.store.current_versions().await? {
            if wanted.contains_key(&cv.version_id) {
                continue;
            }
            let age = seconds(&cv.created_at);
            let class = if cv.kind == "cut" {
                Class::Edited
            } else if age >= new_from {
                Class::New
            } else {
                Class::Old
            };
            let new_too = cfg.external.ahead == "edited_new";
            let take = match class {
                Class::Edited => true,
                Class::New => new_too,
                Class::Old => new_too && there.contains(cv.version_id.as_str()),
                Class::Shared => false,
            };
            if take {
                wanted.insert(
                    cv.version_id,
                    Wanted {
                        class,
                        protected: false,
                        activity: None,
                        newest_link: 0,
                        age,
                    },
                );
            }
        }
    }
    let shared: HashSet<String> = wanted
        .iter()
        .filter(|(_, w)| w.class == Class::Shared)
        .map(|(v, _)| v.clone())
        .collect();
    let cap = cfg.external_embed_bytes();
    // the plan comes before any copy is made: a missing copy counts at the storage's cap, so a
    // copy is made only for a preview that may go up
    let mut candidates = Vec::new();
    let mut entries = Vec::new();
    let mut fallbacks: HashMap<String, (&'static str, String)> = HashMap::new();
    let mut failures = Vec::new();
    for (vid, w) in &wanted {
        let Some(version) = ctx
            .store
            .rendition(vid)
            .await?
            .filter(|r| r.is_ready() && r.is_version())
        else {
            continue;
        };
        // an object whose address a link handed out never stays for Made ahead alone: once no
        // live link serves its version it goes, and the version goes up under a new key
        let there = objects
            .iter()
            .find(|o| &o.version_id == vid && o.state == "stored")
            .filter(|o| w.class == Class::Shared || o.exposed == 0)
            .cloned();
        let file = match ext_file_made(ctx, &version).await? {
            Ok(f) => f,
            Err(why) => {
                fallbacks.insert(vid.clone(), ("copy", why));
                continue;
            }
        };
        // a copy being made at another size leaves the object there serving meanwhile
        let (size, stored) = match (&file, &there) {
            (Some(f), Some(o)) => (f.size.unwrap_or(0).max(0) as u64, o.file_id == f.id),
            (Some(f), None) => (f.size.unwrap_or(0).max(0) as u64, false),
            (None, Some(o)) => (o.size.max(0) as u64, true),
            (None, None) => (cap, false),
        };
        candidates.push(Candidate {
            version_id: vid.clone(),
            class: w.class,
            size,
            stored,
            protected: w.protected,
            activity: w.activity,
            newest_link: w.newest_link,
            age: w.age,
        });
        entries.push((version, file, there, w.protected));
    }
    let limit = cfg.external_limit_bytes();
    let plan = plan(&candidates, limit);
    let keep: HashMap<&str, (&str, Class)> = plan
        .keep
        .iter()
        .filter(|&&i| candidates[i].stored)
        .filter_map(|&i| {
            entries[i].2.as_ref().map(|o| {
                (
                    candidates[i].version_id.as_str(),
                    (o.file_id.as_str(), candidates[i].class),
                )
            })
        })
        .collect();

    let s3 = match S3::new(bucket_of(&cfg)) {
        Ok(s) => s,
        Err(e) => {
            for c in &candidates {
                if !c.stored {
                    fallbacks.insert(c.version_id.clone(), ("unreachable", e.to_string()));
                }
            }
            settle_fallbacks(ctx, &shared, &fallbacks).await?;
            return report(ctx, &cfg, &shared, Vec::new(), Some(e.to_string()), None).await;
        }
    };
    for o in &objects {
        if o.state == "stored" {
            if let Some((file, class)) = keep.get(o.version_id.as_str()) {
                if *file == o.file_id.as_str() {
                    if o.class != class.name() {
                        ctx.store
                            .set_external_object_class(&o.id, class.name())
                            .await?;
                    }
                    if *class == Class::Shared && o.exposed == 0 {
                        ctx.store.expose_external_object(&o.id).await?;
                    }
                    continue;
                }
            }
        }
        ctx.store
            .set_external_object_state(&o.id, "deleting", None)
            .await?;
        match s3.delete(&o.key).await {
            Ok(()) => {
                ctx.store.delete_external_object(&o.id).await?;
                tracing::info!(version = %o.version_id, key = %o.key, "a preview left the storage");
            }
            Err(e) => {
                let msg = cfg.hide_paths(&e.to_string());
                tracing::warn!(key = %o.key, error = %msg, "a preview could not be deleted from the storage");
                ctx.store
                    .set_external_object_state(&o.id, "deleting", Some(&msg))
                    .await?;
                failures.push(msg);
            }
        }
    }
    let mut used = in_use(ctx).await?;
    for &i in &plan.keep {
        let c = &candidates[i];
        if c.stored {
            continue;
        }
        let (version, file, _, protected) = &entries[i];
        let Some(file) = file else {
            let priority = if *protected {
                PRIORITY_EMBED
            } else {
                PRIORITY_DERIVE
            };
            ensure_embed_ext(ctx, version, priority).await?;
            continue;
        };
        if used + c.size > limit {
            fallbacks.insert(c.version_id.clone(), ("limit", String::new()));
            continue;
        }
        match upload(ctx, &s3, c, version, file).await {
            Ok(()) => used += c.size,
            Err(e) => {
                let msg = cfg.hide_paths(&e.to_string());
                let why = if msg.contains("could not connect") || msg.contains("no answer in time")
                {
                    "unreachable"
                } else {
                    "failed"
                };
                tracing::warn!(version = %c.version_id, error = %msg, "a preview did not go up to the storage");
                fallbacks.insert(c.version_id.clone(), (why, msg.clone()));
                failures.push(msg);
            }
        }
    }
    for &i in &plan.out {
        fallbacks.insert(candidates[i].version_id.clone(), ("limit", String::new()));
    }
    fallbacks.retain(|v, _| shared.contains(v));
    settle_fallbacks(ctx, &shared, &fallbacks).await?;
    let listing = check_listing(ctx, &s3).await;
    report(ctx, &cfg, &shared, failures, None, listing).await
}

/// What reel's objects hold in the storage now, those it is deleting too.
async fn in_use(ctx: &Ctx) -> anyhow::Result<u64> {
    Ok(ctx
        .store
        .external_objects()
        .await?
        .iter()
        .filter(|o| o.state != "uploading")
        .map(|o| o.size.max(0) as u64)
        .sum())
}

/// Every ten minutes, reel's records against a listing of its prefix: a missing object loses
/// its record, an unknown one is counted and left alone. The counts, or why the listing failed.
async fn check_listing(ctx: &Ctx, s3: &S3) -> Option<Result<(usize, usize), String>> {
    {
        let mut at = ctx.external.listed_at.lock().unwrap();
        if at.is_some_and(|t| t.elapsed() < Duration::from_secs(600)) {
            return None;
        }
        *at = Some(std::time::Instant::now());
    }
    let listed = match s3.list(PREFIX).await {
        Ok(l) => l,
        Err(e) => return Some(Err(ctx.config().hide_paths(&e.to_string()))),
    };
    let Ok(objects) = ctx.store.external_objects().await else {
        return None;
    };
    let keys: HashSet<&str> = listed.iter().map(|l| l.key.as_str()).collect();
    let ours: HashSet<&str> = objects.iter().map(|o| o.key.as_str()).collect();
    let mut missing = 0;
    for o in objects.iter().filter(|o| o.state == "stored") {
        if !keys.contains(o.key.as_str()) {
            tracing::warn!(key = %o.key, "a preview reel put in the storage is not there; it goes up again when wanted");
            let _ = ctx.store.delete_external_object(&o.id).await;
            missing += 1;
        }
    }
    let unknown = listed
        .iter()
        .filter(|l| !ours.contains(l.key.as_str()))
        .count();
    Some(Ok((missing, unknown)))
}

async fn upload(
    ctx: &Ctx,
    s3: &S3,
    c: &Candidate,
    version: &Rendition,
    file: &Rendition,
) -> anyhow::Result<()> {
    let path = ctx.config().abs(file.path.as_deref().unwrap_or(""));
    let body = tokio::fs::read(&path).await?;
    let o = ExternalObject {
        id: reel_core::ids::new_id(),
        version_id: c.version_id.clone(),
        clip_id: version.clip_id.clone(),
        file_id: file.id.clone(),
        key: format!(
            "{PREFIX}{}-{}.mp4",
            c.version_id,
            reel_core::ids::new_token()
        ),
        size: body.len() as i64,
        class: c.class.name().into(),
        state: "uploading".into(),
        created_at: reel_core::format::now(),
        stored_at: None,
        error: None,
        exposed: (c.class == Class::Shared) as i64,
    };
    ctx.store.insert_external_object(&o).await?;
    let started = std::time::Instant::now();
    let sent = Arc::new(AtomicU64::new(0));
    ctx.external
        .going_up
        .lock()
        .unwrap()
        .insert(c.version_id.clone(), (sent.clone(), o.size as u64));
    let put = s3.put_counted(&o.key, body, "video/mp4", Some(sent)).await;
    ctx.external.going_up.lock().unwrap().remove(&c.version_id);
    match put {
        Ok(()) => {
            ctx.store
                .set_external_object_state(&o.id, "stored", None)
                .await?;
            ctx.store.clear_external_fallback(&c.version_id).await?;
            tracing::info!(version = %c.version_id, key = %o.key, size = o.size, secs = started.elapsed().as_secs_f64(), "a preview went up to the storage");
            Ok(())
        }
        Err(e) => {
            let _ = s3.delete(&o.key).await;
            ctx.store.delete_external_object(&o.id).await?;
            Err(e)
        }
    }
}

/// The reasons of this pass go on record; a version stored or no longer wanted loses its
/// own, and one served from here gets its server copy made.
async fn settle_fallbacks(
    ctx: &Ctx,
    shared: &HashSet<String>,
    fallbacks: &HashMap<String, (&'static str, String)>,
) -> anyhow::Result<()> {
    let stored: HashSet<String> = ctx
        .store
        .external_objects()
        .await?
        .into_iter()
        .filter(|o| o.state == "stored")
        .map(|o| o.version_id)
        .collect();
    for f in ctx.store.external_fallbacks().await? {
        if !shared.contains(&f.version_id) || stored.contains(&f.version_id) {
            ctx.store.clear_external_fallback(&f.version_id).await?;
        }
    }
    for (vid, (why, detail)) in fallbacks {
        if stored.contains(vid) {
            continue;
        }
        ctx.store.set_external_fallback(vid, why, detail).await?;
        if let Some(v) = ctx.store.rendition(vid).await? {
            if needs_embed(&ctx.config(), &v)
                && ctx
                    .store
                    .derived_rendition(&v.id, "embed", None)
                    .await?
                    .is_none()
            {
                crate::enqueue::ensure_embed_at(ctx, &v, PRIORITY_DERIVE).await?;
            }
        }
    }
    Ok(())
}

/// What the storage holds after a pass, and the notes it calls for.
async fn report(
    ctx: &Ctx,
    cfg: &Config,
    shared: &HashSet<String>,
    failures: Vec<String>,
    error: Option<String>,
    listing: Option<Result<(usize, usize), String>>,
) -> anyhow::Result<()> {
    let objects = ctx.store.external_objects().await?;
    let mut by_class = Vec::new();
    for class in [Class::Shared, Class::Edited, Class::New, Class::Old] {
        let of: Vec<_> = objects
            .iter()
            .filter(|o| o.state == "stored" && o.class == class.name())
            .collect();
        by_class.push((
            class,
            of.iter().map(|o| o.size.max(0) as u64).sum(),
            of.len(),
        ));
    }
    let stored: HashSet<&str> = objects
        .iter()
        .filter(|o| o.state == "stored")
        .map(|o| o.version_id.as_str())
        .collect();
    let mut from_here: HashMap<String, usize> = HashMap::new();
    let mut listed_fallback = HashSet::new();
    for f in ctx.store.external_fallbacks().await? {
        if shared.contains(&f.version_id) {
            *from_here.entry(f.why.clone()).or_default() += 1;
            listed_fallback.insert(f.version_id);
        }
    }
    let pending = shared
        .iter()
        .filter(|v| !stored.contains(v.as_str()) && !listed_fallback.contains(*v))
        .count();
    let mut failures = failures;
    let before = ctx.external.report.lock().unwrap().clone();
    let (missing, unknown) = match listing {
        Some(Ok(counts)) => counts,
        Some(Err(e)) => {
            failures.push(format!("the listing: {e}"));
            (0, 0)
        }
        None => before.as_ref().map(|r| (0, r.unknown)).unwrap_or((0, 0)),
    };
    let mut from_here: Vec<(String, usize)> = from_here.into_iter().collect();
    from_here.sort();
    let report = Report {
        at: reel_core::format::now(),
        used: objects
            .iter()
            .filter(|o| o.state != "uploading")
            .map(|o| o.size.max(0) as u64)
            .sum(),
        limit: cfg.external_limit_bytes(),
        by_class,
        from_here,
        pending,
        failures,
        error,
        missing,
        unknown,
    };
    let now = Alarm {
        level: level(report.used, report.limit),
        failing: !report.failures.is_empty() || report.error.is_some(),
    };
    let before = {
        let mut a = ctx.external.alarm.lock().unwrap();
        let b = *a;
        *a = now;
        b
    };
    let (told, failed) = alarms(before, now);
    let link = format!("{}/settings/sharing", cfg.public_url.trim_end_matches('/'));
    if let Some(l) = told {
        let title = if l >= 100 {
            format!("{}: the link previews' storage is full", cfg.site_name)
        } else {
            format!(
                "{}: the link previews' storage is at {l} % of its limit",
                cfg.site_name
            )
        };
        crate::notify::send_later(
            ctx,
            crate::notify::Note::new(
                "storage",
                title,
                format!(
                    "{}. The previews made ahead make room first, then the least used links'; a link inside the protected window keeps its own. Your provider's own billing alert sees the whole account.",
                    report.line()
                ),
                link.clone(),
            ),
        );
    }
    if failed {
        let why = report
            .error
            .clone()
            .or_else(|| report.failures.first().cloned())
            .unwrap_or_default();
        crate::notify::send_later(
            ctx,
            crate::notify::Note::new(
                "storage",
                format!("{}: the link previews' storage failed", cfg.site_name),
                format!("{why}. The links concerned are served from your server meanwhile."),
                format!("{}/status", cfg.public_url.trim_end_matches('/')),
            ),
        );
    }
    *ctx.external.report.lock().unwrap() = Some(report);
    *ctx.external.next_expiry.lock().unwrap() = ctx
        .store
        .next_share_expiry()
        .await?
        .and_then(|t| reel_core::format::parse(&t));
    Ok(())
}

/// The settings' Test: a small file written, read back over the public address and deleted.
/// The answer says which step failed and why.
pub async fn test(cfg: &Config) -> Result<String, String> {
    if cfg.external.public_url.is_empty() {
        return Err("the storage needs its public address".into());
    }
    let s3 = S3::new(bucket_of(cfg)).map_err(|e| e.to_string())?;
    let key = format!("{PREFIX}test-{}.txt", reel_core::ids::new_token());
    let body = format!("reel {} tests its storage", reel_core::VERSION);
    s3.put(&key, body.clone().into_bytes(), "text/plain")
        .await
        .map_err(|e| e.to_string())?;
    let url = public_url(cfg, &key);
    let read = async {
        let r = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("written, but {url} did not answer: {}", e.without_url()))?;
        if !r.status().is_success() {
            return Err(format!(
                "written, but {url} answered {}: is the public address the bucket's, and may anyone read it?",
                r.status().as_u16()
            ));
        }
        match r.text().await {
            Ok(t) if t == body => Ok(()),
            _ => Err(format!("written, but {url} answered with another file")),
        }
    }
    .await;
    let deleted = s3.delete(&key).await.map_err(|e| e.to_string());
    read?;
    deleted.map_err(|e| format!("written and read back, but {e}"))?;
    let mut line = format!(
        "a test file was written, read back over {} and deleted: the storage works",
        cfg.external.public_url.trim_end_matches('/')
    );
    if cfg.external.public_url.contains(".r2.dev") {
        line.push_str("; r2.dev is rate limited, so give the bucket a custom domain for the links");
    }
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1_000_000;

    fn c(id: &str, class: Class, size_mb: u64) -> Candidate {
        Candidate {
            version_id: id.into(),
            class,
            size: size_mb * MB,
            stored: false,
            protected: false,
            activity: None,
            newest_link: 0,
            age: 0,
        }
    }

    fn ids(cands: &[Candidate], idx: &[usize]) -> Vec<String> {
        idx.iter().map(|&i| cands[i].version_id.clone()).collect()
    }

    #[test]
    fn the_order_of_removal() {
        let cands = vec![
            Candidate {
                age: 1,
                ..c("old", Class::Old, 40)
            },
            Candidate {
                age: 5,
                ..c("new-older", Class::New, 40)
            },
            Candidate {
                age: 9,
                ..c("new-newer", Class::New, 40)
            },
            Candidate {
                age: 2,
                ..c("edited", Class::Edited, 40)
            },
            Candidate {
                newest_link: 3,
                ..c("never-older", Class::Shared, 40)
            },
            Candidate {
                newest_link: 8,
                ..c("never-newer", Class::Shared, 40)
            },
            Candidate {
                activity: Some(4),
                newest_link: 1,
                ..c("opened-long-ago", Class::Shared, 40)
            },
            Candidate {
                activity: Some(7),
                newest_link: 1,
                ..c("posted-lately", Class::Shared, 40)
            },
            Candidate {
                protected: true,
                newest_link: 9,
                ..c("protected", Class::Shared, 40)
            },
        ];
        let keep_order = [
            "protected",
            "posted-lately",
            "opened-long-ago",
            "never-newer",
            "never-older",
            "edited",
            "new-newer",
            "new-older",
            "old",
        ];
        for n in 0..=cands.len() {
            let p = plan(&cands, n as u64 * 40 * MB);
            assert_eq!(ids(&cands, &p.keep), keep_order[..n], "room for {n}");
            let mut out = ids(&cands, &p.out);
            out.sort();
            let mut want: Vec<String> = keep_order[n..].iter().map(|s| s.to_string()).collect();
            want.sort();
            assert_eq!(out, want, "room for {n}");
        }
    }

    #[test]
    fn the_limit_is_never_passed_by_what_goes_up() {
        let sizes = [3, 40, 12, 40, 7, 25, 40, 1, 33, 18, 40, 9];
        let classes = [Class::Shared, Class::Edited, Class::New, Class::Old];
        let cands: Vec<Candidate> = sizes
            .iter()
            .enumerate()
            .map(|(i, s)| Candidate {
                activity: (i % 3 == 0).then_some(i as i64),
                newest_link: i as i64,
                age: i as i64,
                stored: i % 4 == 1,
                ..c(&format!("v{i}"), classes[i % 4], *s)
            })
            .collect();
        for limit_mb in [0, 1, 5, 40, 60, 99, 150, 268, 1000] {
            let p = plan(&cands, limit_mb * MB);
            let kept: u64 = p.keep.iter().map(|&i| cands[i].size).sum();
            assert!(kept <= limit_mb * MB, "{kept} under {limit_mb} MB");
            assert_eq!(p.keep.len() + p.out.len(), cands.len());
            for &o in &p.out {
                assert!(
                    kept + cands[o].size > limit_mb * MB
                        || p.keep.iter().any(|&k| tier(&cands[k]) < tier(&cands[o])),
                    "{} left out though it fits",
                    cands[o].version_id
                );
            }
        }
    }

    #[test]
    fn the_alarms_at_their_thresholds() {
        let limit = 5_000 * MB;
        assert_eq!(level(0, limit), 0);
        assert_eq!(level(3_999 * MB, limit), 0);
        assert_eq!(level(4_000 * MB, limit), 80);
        assert_eq!(level(4_999 * MB, limit), 80);
        assert_eq!(level(5_000 * MB, limit), 100);
        assert_eq!(
            level(6_000 * MB, limit),
            100,
            "over, after the limit was lowered"
        );
        let a = |level, failing| Alarm { level, failing };
        assert_eq!(alarms(a(0, false), a(0, false)), (None, false));
        assert_eq!(
            alarms(a(0, false), a(80, false)),
            (Some(80), false),
            "80 % told once"
        );
        assert_eq!(
            alarms(a(80, false), a(80, false)),
            (None, false),
            "and not again while it holds"
        );
        assert_eq!(
            alarms(a(80, false), a(100, false)),
            (Some(100), false),
            "then the limit"
        );
        assert_eq!(
            alarms(a(0, false), a(100, false)),
            (Some(100), false),
            "straight to the limit"
        );
        assert_eq!(
            alarms(a(100, false), a(80, false)),
            (None, false),
            "nothing when it falls"
        );
        assert_eq!(
            alarms(a(0, false), a(80, false)),
            (Some(80), false),
            "again once it fell under"
        );
        assert_eq!(
            alarms(a(0, false), a(0, true)),
            (None, true),
            "a failure told"
        );
        assert_eq!(
            alarms(a(0, true), a(0, true)),
            (None, false),
            "once while it lasts"
        );
        assert_eq!(alarms(a(0, true), a(0, false)), (None, false));
    }

    #[test]
    fn a_protected_preview_stays_at_the_limit_and_a_new_link_is_served_from_here() {
        let cands = vec![
            Candidate {
                protected: true,
                stored: true,
                newest_link: 1,
                ..c("there", Class::Shared, 40)
            },
            Candidate {
                protected: true,
                newest_link: 2,
                ..c("new-link", Class::Shared, 40)
            },
            Candidate {
                stored: true,
                activity: Some(9),
                ..c("busy-old-link", Class::Shared, 40)
            },
        ];
        let p = plan(&cands, 40 * MB);
        assert_eq!(ids(&cands, &p.keep), vec!["there"]);
        let out = ids(&cands, &p.out);
        assert!(
            out.contains(&"new-link".to_string()),
            "the new link's preview is served from here"
        );
        assert!(
            out.contains(&"busy-old-link".to_string()),
            "an old link makes room"
        );
        let p = plan(&cands, 20 * MB);
        assert_eq!(
            ids(&cands, &p.keep),
            vec!["there"],
            "a lowered limit removes no protected preview"
        );
        let p = plan(&cands, 80 * MB);
        assert_eq!(
            ids(&cands, &p.keep),
            vec!["there", "new-link"],
            "the protected ones before an old link"
        );
    }
}
