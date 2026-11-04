pub mod advanced;
pub mod anims;
pub mod api;
pub mod assets;
pub mod bulk;
pub mod collections;
pub mod device;
pub mod events;
pub mod health;
pub mod imports;
pub mod jobs;
pub mod media;
pub mod montage;
pub mod pages;
pub mod pieces;
pub mod settings;
pub mod setup;
pub mod share;
#[cfg(feature = "style-page")]
pub mod shots;
pub mod stats;
pub mod status;
pub mod storage;
#[cfg(feature = "style-page")]
pub mod style;
pub mod trash;
pub mod upload;
pub mod users;
pub mod zips;

use askama::Template;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

pub fn html(t: impl Template) -> Response {
    match t.render() {
        Ok(body) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "template");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub fn html_status(status: StatusCode, t: impl Template) -> Response {
    let mut r = html(t);
    *r.status_mut() = status;
    r
}

/// A build's mark, the first segment of every asset's path, so a page and the modules it imports
/// are of one build; a demo build may take one from `REEL_ASSET_MARK`.
pub fn asset_v() -> &'static str {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        #[cfg(feature = "style-page")]
        if let Some(mark) = std::env::var("REEL_ASSET_MARK")
            .ok()
            .filter(|m| !m.is_empty())
        {
            return mark;
        }
        format!(
            "{}-{}-{}",
            reel_core::VERSION,
            reel_core::GIT_SHA,
            reel_core::BUILD_DATE.replace('-', "")
        )
    })
}
