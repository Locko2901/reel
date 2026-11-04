use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::routes::html;
use crate::routes::settings::{row, under, RowView};
use crate::routes::status::{checks, Check};
use crate::AppState;
use askama::Template;
use axum::extract::{Query, State};
use axum::response::Response;
use axum::{Extension, Json};
use reel_core::config::AuthMode;
use reel_core::format;
use reel_core::settings as catalog;
use reel_pipeline::Ctx;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

pub const KEY: &str = "setup_done";
pub const STEPS: [&str; 6] = [
    "Welcome", "Folders", "Hardware", "Access", "Sharing", "Done",
];

/// Set up once, or in use already: a library with clips was set up by hand.
pub async fn is_done(ctx: &Ctx) -> bool {
    if ctx.store.setting(KEY).await.ok().flatten().is_some() {
        return true;
    }
    let (clips, _) = ctx.store.clip_count_and_size().await.unwrap_or((0, 0));
    if clips > 0 {
        let _ = ctx
            .store
            .set_setting(KEY, "in use before the first run existed")
            .await;
        return true;
    }
    false
}

pub struct StepView {
    pub n: usize,
    pub name: &'static str,
    pub done: bool,
    pub now: bool,
}

pub struct FolderView {
    pub key: &'static str,
    pub label: &'static str,
    pub path: String,
    pub state: String,
    pub class: &'static str,
    pub can_create: bool,
}

#[derive(Template)]
#[template(path = "setup.html")]
pub struct SetupPage {
    pub site: String,
    pub step: usize,
    pub steps: Vec<StepView>,
    pub rows: Vec<RowView>,
    pub folders: Vec<FolderView>,
    pub checks: Vec<Check>,
    pub mode: String,
    pub access_line: String,
    pub users: Vec<String>,
    pub summary: Vec<(String, String)>,
    pub done: bool,
    /// The app's two installers, offered on the last step.
    pub apps: Vec<crate::routes::device::AppDownload>,
}

#[derive(Deserialize)]
pub struct StepQuery {
    pub step: Option<usize>,
}

fn keys_for(step: usize, mode: AuthMode) -> Vec<&'static str> {
    match step {
        1 => vec!["site_name", "public_url", "timezone"],
        2 => vec![
            "data_dir",
            "library_dir",
            "ingest.incoming_dir",
            "ingest.originals_dir",
        ],
        3 => vec!["media.hwaccel", "pipeline.workers"],
        4 => match mode {
            AuthMode::Proxy => vec!["auth.trusted_proxies", "auth.admins"],
            AuthMode::Local => vec!["auth.admins", "auth.session_days"],
            AuthMode::Oidc => vec![
                "auth.admins",
                "auth.oidc_role_claim",
                "auth.oidc_admin_value",
                "auth.session_days",
            ],
            AuthMode::Single => vec!["auth.dev_user"],
        },
        5 => vec![
            "share.discord_limit_mb",
            "share.download_default",
            "share.expiry_default",
            "ingest.keep_originals",
        ],
        _ => vec![],
    }
}

fn writable(dir: &std::path::Path) -> bool {
    let probe = dir.join(".reel-write-test");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

pub fn folder_views(ctx: &Ctx) -> Vec<FolderView> {
    let cfg = ctx.config();
    let mut out = Vec::new();
    for (key, label, path, create) in [
        ("data_dir", "Data folder", cfg.data_dir.clone(), false),
        ("library_dir", "Library folder", cfg.library_dir(), false),
        (
            "ingest.incoming_dir",
            "Incoming folder",
            cfg.ingest.incoming_dir.clone(),
            true,
        ),
        (
            "ingest.originals_dir",
            "Originals folder",
            cfg.ingest.originals_dir.clone(),
            true,
        ),
    ] {
        let away = (key == "library_dir").then(|| ctx.library.away()).flatten();
        let (state, class, can_create) = if let Some(why) = away {
            (format!("not usable: {why}"), "st-err", false)
        } else if !path.exists() {
            let parent_ok = path
                .parent()
                .map(|p| p.is_dir() && writable(p))
                .unwrap_or(false);
            (
                if parent_ok {
                    "missing; it can be made here".to_string()
                } else {
                    "missing, and its parent cannot be written; mount it".to_string()
                },
                "st-err",
                create && parent_ok,
            )
        } else if !path.is_dir() {
            ("that is a file, not a folder".to_string(), "st-err", false)
        } else if !writable(&path) {
            (
                "exists, but reel cannot write there".to_string(),
                "st-warn",
                false,
            )
        } else {
            let (free, total) = crate::disk::free_and_total(&path);
            (
                format!(
                    "ready, {} free of {}",
                    format::size(free),
                    format::size(total)
                ),
                "st-ok",
                false,
            )
        };
        out.push(FolderView {
            key,
            label,
            path: path.display().to_string(),
            state,
            class,
            can_create,
        });
    }
    out
}

pub async fn page(
    State(st): State<AppState>,
    Extension(_me): Extension<User>,
    Query(q): Query<StepQuery>,
) -> AppResult<Response> {
    let cfg = st.ctx.config();
    let step = q.step.unwrap_or(1).clamp(1, STEPS.len());
    let eff = st.ctx.effective();
    let under = under(&st.ctx);
    let rows: Vec<RowView> = keys_for(step, cfg.auth.mode)
        .iter()
        .filter_map(|k| catalog::find(k))
        .map(|d| row(d, &eff, &under))
        .collect();
    let steps = STEPS
        .iter()
        .enumerate()
        .map(|(i, name)| StepView {
            n: i + 1,
            name,
            done: i + 1 < step,
            now: i + 1 == step,
        })
        .collect();
    let folders = if step == 2 {
        folder_views(&st.ctx)
    } else {
        vec![]
    };
    let checks = if step == 3 {
        checks(&st).await?
    } else {
        vec![]
    };
    let users: Vec<String> = if step == 4 {
        st.ctx
            .store
            .users()
            .await?
            .iter()
            .map(|u| format!("{} ({})", u.name, u.role))
            .collect()
    } else {
        vec![]
    };
    let access_line = match cfg.auth.mode {
        AuthMode::Proxy => "A proxy in front (Authentik through NPM) names the user in a header; only the addresses listed may do that. Users get a row on first sight; the first one is an admin.".into(),
        AuthMode::Local => "reel keeps its own accounts. The admin exists already (you); the other users are made under Settings, Access, and sign in with a password.".into(),
        AuthMode::Oidc => "The identity provider signs everyone in; a user gets a row on first sight, bound to the provider's subject. The Admins list and the role claim decide who is an admin.".into(),
        AuthMode::Single => "This reel runs for one person: everyone who reaches it is that user. For a shared server, run it behind a proxy or with built-in accounts (the mode is set when the process starts).".into(),
    };
    let summary = if step == 6 {
        let (clips, _) = st.ctx.store.clip_count_and_size().await?;
        let hw = st.ctx.hw.read().await.backend.name().to_string();
        vec![
            (
                "site".into(),
                format!("{} at {}", cfg.site_name, cfg.public_url),
            ),
            (
                "incoming".into(),
                cfg.ingest.incoming_dir.display().to_string(),
            ),
            ("hardware".into(), hw),
            ("access".into(), catalog::get(&cfg, "auth.mode")),
            ("library".into(), format!("{clips} clips so far")),
        ]
    } else {
        vec![]
    };
    Ok(html(SetupPage {
        site: cfg.site_name.clone(),
        step,
        steps,
        rows,
        folders,
        checks,
        mode: catalog::get(&cfg, "auth.mode"),
        access_line,
        users,
        summary,
        done: st.setup_done.load(Ordering::Relaxed),
        apps: crate::routes::device::downloads(&st.ctx.config()).await,
    }))
}

pub async fn folder_checks(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let folders: Vec<Value> = folder_views(&st.ctx)
        .iter()
        .map(|f| json!({ "key": f.key, "label": f.label, "path": f.path, "state": f.state, "ok": f.class == "st-ok", "can_create": f.can_create }))
        .collect();
    Ok(Json(json!({ "folders": folders })))
}

#[derive(Deserialize)]
pub struct MkdirBody {
    pub key: String,
}

pub async fn mkdir(
    State(st): State<AppState>,
    Json(body): Json<MkdirBody>,
) -> AppResult<Json<Value>> {
    let cfg = st.ctx.config();
    let path = match body.key.as_str() {
        "ingest.incoming_dir" => cfg.ingest.incoming_dir.clone(),
        "ingest.originals_dir" => cfg.ingest.originals_dir.clone(),
        other => {
            return Err(AppError::BadRequest(format!(
                "{other} is not a folder reel makes"
            )))
        }
    };
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|e| AppError::BadRequest(format!("could not make it: {e}")))?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn url_check(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let cfg = st.ctx.config();
    let url = format!("{}/healthz", cfg.public_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(format!("reel/{}", reel_core::VERSION))
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let result = client.get(&url).send().await;
    Ok(Json(match result {
        Ok(r) if r.status().is_success() => {
            json!({ "ok": true, "url": url, "line": "the public URL answers; the apps that show link previews will reach the share pages" })
        }
        Ok(r) => {
            json!({ "ok": false, "url": url, "line": format!("the public URL answered {}; check the proxy", r.status()) })
        }
        Err(e) => {
            json!({ "ok": false, "url": url, "line": format!("the public URL did not answer from here: {e}") })
        }
    }))
}

pub async fn done(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    st.ctx
        .store
        .set_setting(KEY, &format!("{} by {}", format::now(), me.name))
        .await?;
    st.setup_done.store(true, Ordering::Relaxed);
    tracing::info!(by = %me.name, "first run done");
    Ok(Json(json!({ "ok": true })))
}
