use crate::auth::User;
use crate::error::AppResult;
use crate::serve_file::serve;
use crate::AppState;
use axum::extract::{Query, Request, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct LogsQuery {
    pub lines: Option<usize>,
}

pub async fn logs(Query(q): Query<LogsQuery>) -> Json<Value> {
    let n = q.lines.unwrap_or(200).clamp(1, 2000);
    Json(json!({ "lines": reel_pipeline::logging::last(n) }))
}

pub async fn logs_download() -> Response {
    let text = reel_pipeline::logging::since(3600).join("\n");
    (
        [
            (
                header::CONTENT_TYPE,
                "text/plain; charset=utf-8".to_string(),
            ),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"reel-log-{}.txt\"",
                    jiff::Timestamp::now().strftime("%Y%m%d-%H%M")
                ),
            ),
        ],
        text,
    )
        .into_response()
}

/// A consistent copy of the database; the temporary file goes once the response has it open.
pub async fn export_database(State(st): State<AppState>, req: Request) -> AppResult<Response> {
    let dir = st.ctx.config().cache_dir();
    tokio::fs::create_dir_all(&dir).await?;
    let path = dir.join(format!(".export-{}.db", reel_core::ids::new_id()));
    st.ctx.store.backup_to(&path).await?;
    let secrets: Vec<&str> = reel_core::settings::secret_keys().collect();
    reel_store::Store::blank_settings_in(&path, &secrets).await?;
    let name = format!("reel-{}.db", jiff::Timestamp::now().strftime("%Y%m%d-%H%M"));
    let mime: mime::Mime = "application/vnd.sqlite3"
        .parse()
        .unwrap_or(mime::APPLICATION_OCTET_STREAM);
    let resp = serve(req, &path, mime, Some(name)).await;
    let _ = tokio::fs::remove_file(&path).await;
    Ok(resp)
}

/// The settings in force as a config file, the secrets blank.
pub async fn export_config(State(st): State<AppState>) -> AppResult<Response> {
    let cfg = reel_core::settings::without_secrets(&st.ctx.config());
    let text = toml::to_string_pretty(&cfg).map_err(|e| anyhow::anyhow!(e))?;
    Ok((
        [
            (
                header::CONTENT_TYPE,
                "text/plain; charset=utf-8".to_string(),
            ),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"config.toml\"".to_string(),
            ),
        ],
        format!(
            "# reel's settings as they are in force, exported {}; the secrets are blank\n{text}",
            jiff::Timestamp::now().strftime("%Y-%m-%d %H:%M")
        ),
    )
        .into_response())
}

/// Exits cleanly a moment after answering; a container's restart policy or service manager
/// brings it back.
pub async fn restart(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    tracing::warn!(by = %me.name, "restart asked from Settings");
    let shutdown = st.ctx.shutdown.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        shutdown.cancel();
    });
    Ok(Json(json!({
        "ok": true,
        "line": "reel is stopping; whatever runs it starts it again, and this page reloads in a moment",
        "reload": true,
    })))
}
