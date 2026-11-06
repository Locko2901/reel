//! Who a request is for, the sign-in gate, the same-site check, the client address and admins.
use crate::accounts::{Backend, SEEN_AT, SIGNED_IN_AT};
use crate::AppState;
use askama::Template;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_login::AuthSession;
use reel_core::config::AuthMode;
use reel_store::models::User as UserRow;
use std::net::{IpAddr, SocketAddr};

pub const SETUP_CODE_KEY: &str = "setup_code";

/// Who makes the request: a row of the user table in every mode.
#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub name: String,
    pub admin: bool,
}

impl User {
    pub fn may_edit(&self, owner_id: Option<&str>) -> bool {
        self.admin || owner_id == Some(self.id.as_str())
    }
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorPage {
    title: String,
    line: String,
}

pub fn error_page(status: StatusCode, title: &str, line: &str) -> Response {
    let body = ErrorPage {
        title: title.into(),
        line: line.into(),
    }
    .render()
    .unwrap_or_default();
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(body),
    )
        .into_response()
}

fn wants_json(req: &Request) -> bool {
    let p = req.uri().path();
    p.starts_with("/api/") || p.starts_with("/media/")
}

pub(crate) fn refuse(req: &Request, status: StatusCode, title: &str, line: &str) -> Response {
    if wants_json(req) {
        (status, axum::Json(serde_json::json!({ "error": title }))).into_response()
    } else {
        error_page(status, title, line)
    }
}

/// Refuses a state-changing request from another site, by `Sec-Fetch-Site` or, from older
/// browsers, by an `Origin` that has to match the host.
pub fn same_origin(method: &Method, headers: &HeaderMap) -> bool {
    if matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS) {
        return true;
    }
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        return matches!(site, "same-origin" | "none");
    }
    match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        None => true,
        Some("null") => false,
        Some(origin) => {
            let origin_host = origin
                .trim_start_matches("https://")
                .trim_start_matches("http://");
            headers
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                .map(|h| h.eq_ignore_ascii_case(origin_host))
                .unwrap_or(false)
        }
    }
}

/// The first admin's one-time setup code: with built-in accounts while the user table is
/// empty, behind an identity provider while there is no admin. A code nobody needs any more is
/// dropped.
pub async fn ensure_setup_code(ctx: &reel_pipeline::Ctx) -> anyhow::Result<Option<String>> {
    if !crate::accounts::wants_setup_code(&ctx.store, &ctx.config()).await? {
        ctx.store.delete_setting(SETUP_CODE_KEY).await?;
        return Ok(None);
    }
    if let Some(code) = ctx.store.setting(SETUP_CODE_KEY).await? {
        return Ok(Some(code));
    }
    let code = reel_core::auth::new_setup_code();
    ctx.store.set_setting(SETUP_CODE_KEY, &code).await?;
    Ok(Some(code))
}

/// Behind a trusted proxy, the rightmost `X-Forwarded-For` address that is not a trusted proxy;
/// from anyone else, the peer, whatever the header says.
pub fn client_ip(headers: &HeaderMap, peer: IpAddr, cfg: &reel_core::Config) -> IpAddr {
    if !cfg.is_trusted_proxy(peer) {
        return peer;
    }
    headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|line| line.split(','))
        .filter_map(|s| s.trim().parse::<IpAddr>().ok())
        .rev()
        .find(|ip| !cfg.is_trusted_proxy(*ip))
        .unwrap_or(peer)
}

/// A session older than the lifetime is over, however active it was; 0 means no limit.
pub fn lifetime_over(signed_in_at: i64, now: i64, max_days: u64) -> bool {
    max_days > 0 && now - signed_in_at >= max_days as i64 * 86_400
}

fn seen_recently(u: &UserRow) -> bool {
    u.last_seen_at
        .as_deref()
        .and_then(reel_core::format::parse)
        .map(|t| jiff::Timestamp::now().as_second() - t.as_second() < 60)
        .unwrap_or(false)
}

/// The pages an admin is sent away from until the first run is done.
fn page_path(req: &Request) -> bool {
    if req.method() != Method::GET {
        return false;
    }
    let p = req.uri().path();
    p == "/"
        || p.starts_with("/clips/")
        || p == "/shares"
        || p == "/upload"
        || p == "/jobs"
        || p == "/status"
        || p.starts_with("/settings")
}

fn to_setup(st: &AppState, user: &User, req: &Request) -> Option<Response> {
    if user.admin && !st.setup_done.load(std::sync::atomic::Ordering::Relaxed) && page_path(req) {
        Some(Redirect::to("/setup").into_response())
    } else {
        None
    }
}

async fn admit(st: &AppState, row: UserRow) -> User {
    if !seen_recently(&row) {
        let _ = st.ctx.store.touch_user(&row.id).await;
    }
    let admin = row.is_admin();
    User {
        id: row.id,
        name: row.name,
        admin,
    }
}

fn to_signin(req: &Request) -> Response {
    if wants_json(req) {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({ "error": "sign in first" })),
        )
            .into_response();
    }
    let next_path = req
        .uri()
        .path_and_query()
        .map(|p| p.as_str())
        .unwrap_or("/");
    let encoded: String =
        percent_encoding::utf8_percent_encode(next_path, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
    Redirect::to(&format!("/signin?next={encoded}")).into_response()
}

fn disabled(req: &Request) -> Response {
    refuse(
        req,
        StatusCode::FORBIDDEN,
        "This account is disabled.",
        "An admin can enable it again under Settings, Access.",
    )
}

fn table_down(req: &Request, e: impl std::fmt::Display, what: &str) -> Response {
    tracing::error!(error = %e, what);
    refuse(
        req,
        StatusCode::INTERNAL_SERVER_ERROR,
        "The user table did not answer.",
        "The log has it.",
    )
}

pub async fn require_user(State(st): State<AppState>, mut req: Request, next: Next) -> Response {
    let cfg = st.ctx.config();
    if !same_origin(req.method(), req.headers()) {
        return refuse(
            &req,
            StatusCode::FORBIDDEN,
            "That came from another site.",
            "reel takes changes only from its own pages.",
        );
    }
    match cfg.auth.mode {
        AuthMode::Single => {
            match crate::accounts::user_on_sight(&st.ctx.store, &cfg, &cfg.auth.dev_user, None)
                .await
            {
                Ok(Some(row)) => {
                    let user = admit(&st, row).await;
                    if let Some(r) = to_setup(&st, &user, &req) {
                        return r;
                    }
                    req.extensions_mut().insert(user);
                    next.run(req).await
                }
                Ok(None) => table_down(&req, "no row for the name", "single user"),
                Err(e) => table_down(&req, e, "single user"),
            }
        }
        AuthMode::Proxy => {
            let peer = req
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|c| c.0.ip());
            let trusted = peer.map(|ip| cfg.is_trusted_proxy(ip)).unwrap_or(false);
            let name = req
                .headers()
                .get("x-authentik-username")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            match (trusted, name) {
                (true, Some(n)) => {
                    match crate::accounts::user_on_sight(&st.ctx.store, &cfg, &n, None).await {
                        Ok(None) => refuse(
                            &req,
                            StatusCode::FORBIDDEN,
                            "Another user has this name in another case.",
                            "reel takes a name exactly as the proxy sends it. An admin can rename one of the two under Settings, Access.",
                        ),
                        Ok(Some(row)) if row.disabled_at.is_some() => disabled(&req),
                        Ok(Some(row)) => {
                            let user = admit(&st, row).await;
                            if let Some(r) = to_setup(&st, &user, &req) {
                                return r;
                            }
                            req.extensions_mut().insert(user);
                            next.run(req).await
                        }
                        Err(e) => table_down(&req, e, "proxy user"),
                    }
                }
                _ => {
                    tracing::warn!(peer = ?peer, trusted, path = %req.uri().path(), "request without a proxy identity");
                    refuse(
                        &req,
                        StatusCode::UNAUTHORIZED,
                        "reel sits behind a sign-in proxy.",
                        "This port is not the front door. Come in through the proxy.",
                    )
                }
            }
        }
        AuthMode::Local | AuthMode::Oidc => {
            let Some(mut auth) = req.extensions().get::<AuthSession<Backend>>().cloned() else {
                return table_down(&req, "no auth layer", "session");
            };
            let Some(session_user) = auth.user.clone() else {
                return to_signin(&req);
            };
            let now = jiff::Timestamp::now().as_second();
            let signed_in_at: Option<i64> = auth.session.get(SIGNED_IN_AT).await.ok().flatten();
            if let Some(at) = signed_in_at {
                if lifetime_over(at, now, cfg.auth.session_max_days) {
                    tracing::info!(user = %session_user.name, "session past its lifetime");
                    let _ = auth.logout().await;
                    return to_signin(&req);
                }
            }
            // a session's idle expiry moves only when it is written: once an hour is enough
            let seen_at: Option<i64> = auth.session.get(SEEN_AT).await.ok().flatten();
            if seen_at.is_none_or(|t| now - t >= 3600) {
                let _ = auth.session.insert(SEEN_AT, now).await;
            }
            match st.ctx.store.user(&session_user.id).await {
                Ok(Some(row)) if row.disabled_at.is_none() => {
                    let user = admit(&st, row).await;
                    if let Some(r) = to_setup(&st, &user, &req) {
                        return r;
                    }
                    req.extensions_mut().insert(user);
                    next.run(req).await
                }
                Ok(_) => {
                    let _ = auth.logout().await;
                    disabled(&req)
                }
                Err(e) => table_down(&req, e, "session user"),
            }
        }
    }
}

/// Admins only, after `require_user`.
pub async fn require_admin(req: Request, next: Next) -> Response {
    let admin = req
        .extensions()
        .get::<User>()
        .map(|u| u.admin)
        .unwrap_or(false);
    if admin {
        next.run(req).await
    } else {
        refuse(
            &req,
            StatusCode::FORBIDDEN,
            "Admins only.",
            "Settings and the users are the admins' business; everything else is yours.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_address_comes_from_the_header_only_behind_a_trusted_proxy() {
        let mut cfg = reel_core::Config::default();
        cfg.auth.trusted_proxies = vec!["192.0.2.10/32".into(), "127.0.0.1/32".into()];
        let proxy: IpAddr = "192.0.2.10".parse().unwrap();
        let stranger: IpAddr = "203.0.113.77".parse().unwrap();
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.5, 127.0.0.1".parse().unwrap());
        assert_eq!(client_ip(&h, proxy, &cfg).to_string(), "203.0.113.5");
        assert_eq!(client_ip(&h, stranger, &cfg), stranger);
        let mut only_proxies = HeaderMap::new();
        only_proxies.insert("x-forwarded-for", "127.0.0.1".parse().unwrap());
        assert_eq!(client_ip(&only_proxies, proxy, &cfg), proxy);
        assert_eq!(client_ip(&HeaderMap::new(), proxy, &cfg), proxy);
        let mut spoofed = HeaderMap::new();
        spoofed.insert("x-forwarded-for", "1.2.3.4, 203.0.113.5".parse().unwrap());
        assert_eq!(client_ip(&spoofed, proxy, &cfg).to_string(), "203.0.113.5");
    }

    #[test]
    fn same_site_by_fetch_metadata_or_origin() {
        let mut h = HeaderMap::new();
        assert!(same_origin(&Method::GET, &h));
        assert!(same_origin(&Method::POST, &h));
        h.insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert!(!same_origin(&Method::POST, &h));
        h.insert("sec-fetch-site", "same-origin".parse().unwrap());
        assert!(same_origin(&Method::POST, &h));
        let mut o = HeaderMap::new();
        o.insert(header::HOST, "clips.example.org".parse().unwrap());
        o.insert(header::ORIGIN, "https://evil.example.net".parse().unwrap());
        assert!(!same_origin(&Method::POST, &o));
        o.insert(header::ORIGIN, "https://clips.example.org".parse().unwrap());
        assert!(same_origin(&Method::POST, &o));
        o.insert(header::ORIGIN, "null".parse().unwrap());
        assert!(!same_origin(&Method::POST, &o));
    }

    #[test]
    fn the_lifetime_rule() {
        assert!(!lifetime_over(0, 86_399, 1));
        assert!(lifetime_over(0, 86_400, 1));
        assert!(!lifetime_over(0, 1_000_000_000, 0));
    }
}
