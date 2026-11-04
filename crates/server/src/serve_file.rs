use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use std::path::Path;
use tower::ServiceExt;
use tower_http::services::ServeFile;

pub fn attachment_name(title: &str, ext: &str) -> String {
    let mut s: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    s = s.trim().trim_matches('.').to_string();
    if s.is_empty() {
        s = "clip".into();
    }
    if s.len() > 80 {
        s.truncate(80);
    }
    format!("{s}.{ext}")
}

pub async fn serve(
    req: Request,
    path: &Path,
    mime: mime::Mime,
    attachment: Option<String>,
) -> Response {
    if !path.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let svc = ServeFile::new_with_mime(path, &mime);
    let req = req.map(|_| Body::empty());
    let res = match svc.oneshot(req).await {
        Ok(r) => r,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut res = res.into_response();
    if let Some(name) = attachment {
        if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
            res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
        }
    }
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=3600"),
    );
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_names_are_safe() {
        assert_eq!(
            attachment_name("ace, last round", "mp4"),
            "ace_ last round.mp4"
        );
        assert_eq!(
            attachment_name("../../etc/passwd", "mp4"),
            "_.._etc_passwd.mp4"
        );
        assert_eq!(attachment_name("", "mp4"), "clip.mp4");
    }
}
