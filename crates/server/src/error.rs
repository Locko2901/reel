use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use reel_pipeline::library::Away;

#[derive(Debug)]
pub enum AppError {
    NotFound,
    BadRequest(String),
    Conflict(String),
    TooLarge(String),
    Forbidden(String),
    Busy(String),
    TooMany(String, u64),
    /// Not offered in the app's standalone library: 404 with the reason.
    Standalone(String),
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        AppError::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({ "error": "not found" })),
            )
                .into_response(),
            AppError::BadRequest(m) => (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
            AppError::Conflict(m) => (
                StatusCode::CONFLICT,
                axum::Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
            AppError::TooLarge(m) => (
                StatusCode::PAYLOAD_TOO_LARGE,
                axum::Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
            AppError::Forbidden(m) => (
                StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
            AppError::Standalone(m) => (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({ "error": format!("Not in standalone: {m}."), "standalone": true })),
            )
                .into_response(),
            AppError::Busy(m) => (
                StatusCode::SERVICE_UNAVAILABLE,
                [(axum::http::header::RETRY_AFTER, "5")],
                axum::Json(serde_json::json!({ "error": m })),
            )
                .into_response(),
            AppError::TooMany(m, secs) => (
                StatusCode::TOO_MANY_REQUESTS,
                [(axum::http::header::RETRY_AFTER, secs.to_string())],
                axum::Json(serde_json::json!({ "error": m, "retry_after": secs })),
            )
                .into_response(),
            AppError::Internal(e) if e.downcast_ref::<Away>().is_some() => {
                AppError::Busy(e.to_string()).into_response()
            }
            AppError::Internal(e) => {
                tracing::error!(error = %format!("{e:#}"), "request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(serde_json::json!({ "error": "something broke, the log has it" })),
                )
                    .into_response()
            }
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// A 503 saying why while the library is away, before a client sends a file's bytes.
pub async fn library_ready(ctx: &reel_pipeline::Ctx) -> AppResult<()> {
    reel_pipeline::library::ensure(ctx)
        .await
        .map_err(|e| AppError::Busy(e.to_string()))
}
