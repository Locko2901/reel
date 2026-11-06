//! The headers every answer carries: what a page may load and run, no framing, no sniffing.
use crate::AppState;
use axum::extract::{Request, State};
use axum::http::{header, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use reel_core::Config;

/// The origin of an address (`https://host[:port]`), as a policy names a source; none when it
/// is not an http or https URL.
fn origin(address: &str) -> Option<String> {
    let url = url::Url::parse(address.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url.host_str()?;
    Some(match url.port() {
        Some(p) => format!("{}://{host}:{p}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    })
}

/// The pages' policy: scripts only from reel's own files (a JSON block is data and runs
/// nothing), pictures and videos also from the public address a share page names and from the
/// external storage its previews may be sent to, no frames.
pub fn policy(cfg: &Config) -> String {
    let public = origin(&cfg.public_url)
        .map(|o| format!(" {o}"))
        .unwrap_or_default();
    let storage = if cfg.external_on() {
        origin(&cfg.external.public_url)
            .map(|o| format!(" {o}"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    // ipc: and http://ipc.localhost are the desktop app's bridge, for reel's pages in its window
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:{public}; media-src 'self'{public}{storage}; \
         connect-src 'self' ipc: http://ipc.localhost; font-src 'self'; worker-src 'self'; \
         manifest-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; \
         frame-ancestors 'none'"
    )
}

pub async fn add(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&policy(&st.ctx.config())) {
        h.entry(header::CONTENT_SECURITY_POLICY).or_insert(v);
    }
    h.entry(header::X_CONTENT_TYPE_OPTIONS)
        .or_insert(HeaderValue::from_static("nosniff"));
    h.entry(header::X_FRAME_OPTIONS)
        .or_insert(HeaderValue::from_static("DENY"));
    h.entry(header::REFERRER_POLICY)
        .or_insert(HeaderValue::from_static("same-origin"));
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_names_the_public_address_and_the_storage_only_when_it_is_on() {
        let mut cfg = Config {
            public_url: "https://clips.example.org/".into(),
            ..Config::default()
        };
        cfg.external.public_url = "https://media.example.com".into();
        let p = policy(&cfg);
        assert!(p.contains("script-src 'self';"), "{p}");
        assert!(p.contains("frame-ancestors 'none'"), "{p}");
        assert!(
            p.contains("img-src 'self' data: blob: https://clips.example.org;"),
            "{p}"
        );
        assert!(!p.contains("media.example.com"), "the storage is off: {p}");
        cfg.external.on = true;
        let p = policy(&cfg);
        assert!(
            p.contains("media-src 'self' https://clips.example.org https://media.example.com;"),
            "{p}"
        );
        cfg.public_url = "not an address".into();
        assert!(policy(&cfg).contains("img-src 'self' data: blob:;"));
    }

    /// Every template and module keeps to the policy: no script in the markup, no handler in an
    /// attribute, no javascript: address.
    #[test]
    fn no_page_runs_a_script_of_its_own_markup() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web");
        let mut files = Vec::new();
        for dir in ["templates", "assets", "assets/js"] {
            for e in std::fs::read_dir(root.join(dir)).unwrap().flatten() {
                let p = e.path();
                if matches!(p.extension().and_then(|x| x.to_str()), Some("html" | "js")) {
                    files.push(p);
                }
            }
        }
        assert!(files.len() > 20, "{files:?}");
        // ` onclick="` and the like: whitespace, on, letters, =, a quote
        let handler = |line: &str| {
            let b = line.as_bytes();
            (1..b.len().saturating_sub(3)).any(|i| {
                if !b[i - 1].is_ascii_whitespace() || &b[i..i + 2] != b"on" {
                    return false;
                }
                let mut j = i + 2;
                while j < b.len() && b[j].is_ascii_lowercase() {
                    j += 1;
                }
                j > i + 2 && b.get(j) == Some(&b'=') && matches!(b.get(j + 1), Some(b'"' | b'\''))
            })
        };
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            for (n, line) in text.lines().enumerate() {
                let at = format!("{}:{}", f.display(), n + 1);
                assert!(!handler(line), "a handler in markup at {at}");
                assert!(
                    !line.contains("javascript:"),
                    "a javascript: address at {at}"
                );
                if f.extension().is_some_and(|x| x == "html") {
                    for tag in line.split("<script").skip(1) {
                        let open = tag.split('>').next().unwrap_or("");
                        assert!(
                            open.contains(" src=") || open.contains("application/json"),
                            "a script in the markup at {at}"
                        );
                    }
                }
            }
        }
    }
}
