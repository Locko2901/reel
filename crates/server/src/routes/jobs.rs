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
use reel_store::models::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct JobView {
    pub id: String,
    pub can_act: bool,
    pub kind: String,
    pub clip_id: Option<String>,
    pub title: String,
    pub thumb: Option<String>,
    pub what: String,
    pub route: String,
    pub progress: i64,
    pub eta: String,
    pub started: String,
    pub waiting: String,
    pub next: bool,
    pub reason: String,
    pub attempts: i64,
    pub finished: String,
    pub finished_ago: String,
    pub result: String,
    pub took: String,
}

#[derive(Template)]
#[template(path = "jobs.html")]
pub struct JobsPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub show: String,
    pub summary: String,
    pub running: Vec<JobView>,
    pub queued: Vec<JobView>,
    pub failed: Vec<JobView>,
    pub recent: Vec<JobView>,
    pub just: Vec<JobView>,
}

#[derive(Deserialize)]
pub struct JobsQuery {
    pub show: Option<String>,
}

struct Lookup {
    clips: HashMap<String, Option<Clip>>,
    renditions: HashMap<String, Option<Rendition>>,
}

impl Lookup {
    fn new() -> Self {
        Self {
            clips: HashMap::new(),
            renditions: HashMap::new(),
        }
    }
    async fn clip(&mut self, st: &AppState, id: &str) -> Option<Clip> {
        if !self.clips.contains_key(id) {
            let c = st.ctx.store.clip(id).await.ok().flatten();
            self.clips.insert(id.to_string(), c);
        }
        self.clips.get(id).cloned().flatten()
    }
    async fn rendition(&mut self, st: &AppState, id: &str) -> Option<Rendition> {
        if !self.renditions.contains_key(id) {
            let r = st.ctx.store.rendition(id).await.ok().flatten();
            self.renditions.insert(id.to_string(), r);
        }
        self.renditions.get(id).cloned().flatten()
    }
}

/// How long a finished job stays on the Now view, so a cut just made is seen there done.
const JUST_FINISHED_SECONDS: i64 = 30 * 60;

fn seconds_between(a: &str, b: &str) -> Option<i64> {
    Some(format::parse(b)?.as_second() - format::parse(a)?.as_second())
}

fn seconds_words(s: i64) -> String {
    if s < 60 {
        format!("{s} s")
    } else if s < 3600 {
        format!("{} min {} s", s / 60, s % 60)
    } else {
        format!("{} h {} min", s / 3600, (s % 3600) / 60)
    }
}

async fn view(
    st: &AppState,
    lookup: &mut Lookup,
    job: &Job,
    position: usize,
    me: &User,
) -> JobView {
    let clip = match &job.clip_id {
        Some(id) => lookup.clip(st, id).await,
        None => None,
    };
    let can_act = me.admin
        || clip
            .as_ref()
            .map(|c| me.may_edit(c.owner_id.as_deref()))
            .unwrap_or(false);
    let rendition = match &job.rendition_id {
        Some(id) => lookup.rendition(st, id).await,
        None => None,
    };
    let now = format::now();
    let elapsed = job
        .started_at
        .as_deref()
        .and_then(|s| seconds_between(s, &now))
        .unwrap_or(0);
    let eta = if job.state == "running" && job.progress > 0.02 && elapsed > 2 {
        let left = (elapsed as f64 / job.progress * (1.0 - job.progress)).round() as i64;
        format!("about {} left", seconds_words(left.max(1)))
    } else {
        String::new()
    };
    let took = match (&job.started_at, &job.finished_at) {
        (Some(s), Some(f)) => seconds_between(s, f).map(seconds_words).unwrap_or_default(),
        _ => String::new(),
    };
    let thumb = clip
        .as_ref()
        .and_then(|c| c.current_id.clone().or(c.master_id.clone()))
        .map(|r| format!("/media/renditions/{r}.jpg"));
    JobView {
        id: job.id.clone(),
        can_act,
        kind: job.kind.clone(),
        clip_id: job.clip_id.clone(),
        title: clip
            .as_ref()
            .map(|c| c.title.clone())
            .unwrap_or_else(|| "a clip that is gone".into()),
        thumb,
        what: reel_pipeline::jobs::what(
            job,
            clip.as_ref(),
            rendition.as_ref(),
            st.ctx.config().share.embed_mb,
        ),
        route: job.route.clone().unwrap_or_default(),
        progress: (job.progress * 100.0).round() as i64,
        eta,
        started: job
            .started_at
            .as_deref()
            .map(format::when)
            .unwrap_or_default(),
        waiting: format::relative(&job.created_at).replace(" ago", ""),
        next: position == 0,
        reason: crate::views::hide_paths(job.error.as_deref().unwrap_or(""), &st.ctx.config()),
        attempts: job.attempts,
        finished: job
            .finished_at
            .as_deref()
            .map(format::datetime)
            .unwrap_or_default(),
        finished_ago: job
            .finished_at
            .as_deref()
            .map(format::relative)
            .unwrap_or_default(),
        result: match job.state.as_str() {
            "done" => "done".into(),
            "failed" => "failed".into(),
            "cancelled" => "cancelled".into(),
            s => s.into(),
        },
        took,
    }
}

impl JobView {
    /// The first line of the reason, cut for the table; the whole sits in the block under it.
    pub fn reason_short(&self) -> String {
        crate::views::short_error(&self.reason)
    }
}

async fn views(st: &AppState, lookup: &mut Lookup, jobs: &[Job], me: &User) -> Vec<JobView> {
    let mut out = Vec::new();
    for (i, j) in jobs.iter().enumerate() {
        out.push(view(st, lookup, j, i, me).await);
    }
    out
}

async fn may_act(st: &AppState, me: &User, id: &str) -> AppResult<()> {
    if me.admin {
        return Ok(());
    }
    let job = st.ctx.store.job(id).await?.ok_or(AppError::NotFound)?;
    let owner = match &job.clip_id {
        Some(c) => st.ctx.store.clip(c).await?.and_then(|c| c.owner_id),
        None => None,
    };
    if me.may_edit(owner.as_deref()) {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "only who owns the clip, or an admin, acts on its jobs".into(),
        ))
    }
}

pub async fn page(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Query(q): Query<JobsQuery>,
) -> AppResult<Response> {
    let show = q.show.unwrap_or_else(|| "now".into());
    let now = st.ctx.store.jobs_now().await?;
    let running: Vec<Job> = now
        .iter()
        .filter(|j| j.state == "running")
        .cloned()
        .collect();
    let queued: Vec<Job> = now
        .iter()
        .filter(|j| j.state == "queued")
        .cloned()
        .collect();
    let failed = st.ctx.store.failed_jobs().await?;
    let recent = if show == "history" {
        st.ctx.store.finished_jobs(100).await?
    } else {
        vec![]
    };
    let now_at = format::now();
    let just: Vec<Job> = if show == "now" {
        st.ctx
            .store
            .finished_jobs(20)
            .await?
            .into_iter()
            .filter(|j| j.state != "failed")
            .filter(|j| {
                j.finished_at
                    .as_deref()
                    .and_then(|f| seconds_between(f, &now_at))
                    .is_some_and(|s| s <= JUST_FINISHED_SECONDS)
            })
            .take(5)
            .collect()
    } else {
        vec![]
    };
    let mut lookup = Lookup::new();
    let summary = format!(
        "{} running, {} queued, {} failed; the worker takes the queue by priority: cuts first, then the copies under a size, the preview videos, then ingest",
        running.len(),
        queued.len(),
        failed.len()
    );
    Ok(html(JobsPage {
        who: Who::at(&st, &me).await,
        nav: "jobs",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        running: views(&st, &mut lookup, &running, &me).await,
        queued: views(&st, &mut lookup, &queued, &me).await,
        failed: views(&st, &mut lookup, &failed, &me).await,
        recent: views(&st, &mut lookup, &recent, &me).await,
        just: views(&st, &mut lookup, &just, &me).await,
        show,
        summary,
    }))
}

pub async fn counts(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let (queued, running, failed) = st.ctx.store.job_counts_full().await?;
    Ok(Json(
        json!({ "queued": queued, "running": running, "failed": failed }),
    ))
}

pub async fn list(
    State(st): State<AppState>,
    Query(q): Query<JobsQuery>,
) -> AppResult<Json<Value>> {
    let jobs = match q.show.as_deref().unwrap_or("now") {
        "failed" => st.ctx.store.failed_jobs().await?,
        "history" => st.ctx.store.finished_jobs(100).await?,
        _ => st.ctx.store.jobs_now().await?,
    };
    let cfg = st.ctx.config();
    let jobs: Vec<Job> = jobs.into_iter().map(|j| shown(j, &cfg)).collect();
    Ok(Json(json!({ "jobs": jobs })))
}

/// A job as the API shows it: the server's folders in its payload and its error named, as the
/// pages show them.
pub fn shown(mut job: Job, cfg: &reel_core::Config) -> Job {
    fn hide(v: &mut Value, cfg: &reel_core::Config) {
        match v {
            Value::String(s) => *s = cfg.hide_paths(s),
            Value::Array(items) => items.iter_mut().for_each(|i| hide(i, cfg)),
            Value::Object(map) => map.values_mut().for_each(|i| hide(i, cfg)),
            _ => {}
        }
    }
    job.payload = match serde_json::from_str::<Value>(&job.payload) {
        Ok(mut v) => {
            hide(&mut v, cfg);
            v.to_string()
        }
        Err(_) => cfg.hide_paths(&job.payload),
    };
    job.error = job.error.map(|e| cfg.hide_paths(&e));
    job
}

pub async fn cancel(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    may_act(&st, &me, &id).await?;
    let ok = reel_pipeline::jobs::cancel(&st.ctx, &id)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    if !ok {
        return Err(AppError::Conflict(
            "that job is not running or queued".into(),
        ));
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn retry(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    may_act(&st, &me, &id).await?;
    let job = reel_pipeline::jobs::retry(&st.ctx, &id)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(
        json!({ "ok": true, "job_id": job.id, "attempts": job.attempts }),
    ))
}

pub async fn dismiss(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    may_act(&st, &me, &id).await?;
    reel_pipeline::jobs::dismiss(&st.ctx, &id)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn discard(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    may_act(&st, &me, &id).await?;
    reel_pipeline::jobs::discard_job(&st.ctx, &id)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}
