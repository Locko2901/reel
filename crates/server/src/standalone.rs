//! A server's routes answer 404 in the app's standalone library, so no page or link reaches them.
use crate::AppState;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// What a group of routes is, for the refusal's words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// Making, waiting for, listing and revoking share links and a collection's link.
    Links,
    /// The pages a link opens: `/s/...` and `/c/...`.
    Public,
    /// Users, their sessions and sign-ins.
    Accounts,
    /// Pairing, the paired devices, their API and the installers offered to them.
    Devices,
    /// The rest of a server's: its first run, notifications, external storage, web copies and
    /// preview videos, its restart, config.toml and its own log.
    Server,
}

impl Part {
    pub fn words(self) -> &'static str {
        match self {
            Part::Links => "reel on this PC makes no share links: a link is a page on the internet, which needs a server. Download the clip, or a copy under a size, and send the file",
            Part::Public => "reel on this PC serves no public pages",
            Part::Accounts => "reel on this PC has one user, the person at this PC",
            Part::Devices => "reel on this PC pairs no devices: the app is its own",
            Part::Server => "this is a server's, and reel on this PC is the app's library alone",
        }
    }
}

#[derive(Clone)]
pub struct Guard {
    pub st: AppState,
    pub part: Part,
}

impl Guard {
    pub fn new(st: &AppState, part: Part) -> Guard {
        Guard {
            st: st.clone(),
            part,
        }
    }
}

/// A route of a server's: on through in a server, refused in the app's standalone library.
pub async fn guard(State(g): State<Guard>, req: Request, next: Next) -> Response {
    if !g.st.ctx.config().standalone() {
        return next.run(req).await;
    }
    tracing::info!(path = %req.uri().path(), part = ?g.part, "standalone: a server's route, refused");
    refusal(g.part, req.uri().path())
}

/// 404 with the words: JSON for the API, a page otherwise.
pub fn refusal(part: Part, path: &str) -> Response {
    let title = "Not in standalone";
    if path.starts_with("/api/") || path.starts_with("/media/") || path.starts_with("/app/") {
        (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": format!("{title}: {}.", part.words()),
                "standalone": true,
            })),
        )
            .into_response()
    } else {
        crate::auth::error_page(
            StatusCode::NOT_FOUND,
            title,
            &format!("{}.", capital(part.words())),
        )
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}
