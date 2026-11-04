use axum::extract::{Path, RawQuery};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

#[derive(rust_embed::Embed)]
#[folder = "../../web/assets"]
#[exclude = "*.md"]
struct Assets;

/// `/assets/<mark>/<path>` is this build's file, cached for good. Another build's mark finds
/// nothing, so a page of that build gets none of this one's modules.
pub async fn asset(Path(path): Path<String>, RawQuery(query): RawQuery) -> Response {
    let (marked, path) = match path.split_once('/') {
        Some((first, rest)) if first == super::asset_v() => (true, rest.to_string()),
        _ => (false, path),
    };
    match Assets::get(&path) {
        Some(f) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            let versioned = marked
                || query
                    .as_deref()
                    .map(|q| q.contains(&format!("v={}", super::asset_v())))
                    .unwrap_or(false);
            let cache = if path.starts_with("fonts/") || versioned {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            let mut r = (
                [
                    (
                        header::CONTENT_TYPE,
                        HeaderValue::from_str(mime.as_ref()).unwrap(),
                    ),
                    (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
                ],
                f.data.into_owned(),
            )
                .into_response();
            if path == "sw.js" {
                r.headers_mut()
                    .insert("service-worker-allowed", HeaderValue::from_static("/"));
            }
            r
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
