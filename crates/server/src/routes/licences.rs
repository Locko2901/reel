//! `/licences`: the licence text of every crate the server and the app are built from, each text
//! once with the crates under it, made by `ci/licence-page.sh` and built in. Anyone who reaches
//! the server may read it, signed in or not.
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

/// The page's path, the one link of About that is not https.
pub const PATH: &str = "/licences";

const PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/licences.html"));

/// Whether this build has the page in it.
pub fn built_in() -> bool {
    !PAGE.is_empty()
}

/// How many crates the page lists, as it counts them itself.
pub fn crate_count() -> Option<usize> {
    let mark = r#"<meta name="reel-crates" content=""#;
    let at = PAGE.find(mark)? + mark.len();
    PAGE[at..].split('"').next()?.parse().ok()
}

pub async fn page() -> Response {
    if !built_in() {
        return crate::auth::error_page(
            StatusCode::NOT_FOUND,
            "This build has no licence page.",
            "ci/licence-page.sh makes it for a build; THIRD_PARTY.md in reel's repository names every crate and its licence.",
        );
    }
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        PAGE,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_is_the_pages_own() {
        match crate_count() {
            Some(n) => {
                assert!(n > 100, "{n}");
                assert!(PAGE.contains(&format!("built from {n} Rust crates")));
            }
            None => assert!(!built_in(), "a page names its count"),
        }
    }
}
