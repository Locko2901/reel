//! Single mode signs nobody in: reel answers only to the names of its own address, and the
//! app's standalone library only to the app, by a key made at each start.
use crate::AppState;
use axum::extract::{Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use std::net::SocketAddr;

pub const KEY_COOKIE: &str = "reel_app_key";
/// The key as a header, for the app's own requests that are not its window's.
pub const KEY_HEADER: &str = "x-reel-app-key";

/// How long a ticket into the window's way in lasts.
const TICKET_LIFE: std::time::Duration = std::time::Duration::from_secs(60);

/// The key the app shows the standalone library: the window by a cookie, the updater by a header.
#[derive(Clone)]
pub struct AppKey {
    secret: std::sync::Arc<str>,
    tickets: std::sync::Arc<std::sync::Mutex<Vec<(String, std::time::Instant)>>>,
}

fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl AppKey {
    pub fn new() -> AppKey {
        AppKey {
            secret: reel_core::ids::new_device_token().into(),
            tickets: Default::default(),
        }
    }

    pub fn secret(&self) -> &str {
        &self.secret
    }

    fn matches(&self, given: &str) -> bool {
        same(&self.secret, given)
    }

    /// A ticket for the window's way in, good once and for a minute, so the key itself is in
    /// no address, no history and no log.
    pub fn ticket(&self) -> String {
        let ticket = reel_core::ids::new_device_token();
        let mut t = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
        t.retain(|(_, made)| made.elapsed() < TICKET_LIFE);
        t.push((ticket.clone(), std::time::Instant::now()));
        ticket
    }

    fn spend(&self, given: &str) -> bool {
        let mut t = self.tickets.lock().unwrap_or_else(|e| e.into_inner());
        t.retain(|(_, made)| made.elapsed() < TICKET_LIFE);
        match t.iter().position(|(ticket, _)| same(ticket, given)) {
            Some(i) => {
                t.remove(i);
                true
            }
            None => false,
        }
    }

    /// Whether a request carries the key, in the header or the window's cookie.
    pub fn shown_by(&self, headers: &HeaderMap) -> bool {
        let by_header = headers
            .get(KEY_HEADER)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|k| self.matches(k));
        by_header
            || headers
                .get_all(header::COOKIE)
                .iter()
                .filter_map(|v| v.to_str().ok())
                .flat_map(|line| line.split(';'))
                .filter_map(|c| c.trim().split_once('='))
                .any(|(name, value)| name == KEY_COOKIE && self.matches(value))
    }
}

impl Default for AppKey {
    fn default() -> Self {
        AppKey::new()
    }
}

impl std::fmt::Debug for AppKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppKey(..)")
    }
}

/// The names reel may be called by without sign-in: the loopback names of the address it
/// listens on, and the public URL's.
struct Names {
    port: u16,
    own: [String; 4],
    public: Option<(String, String, u16)>,
}

impl Names {
    fn of(bound: SocketAddr, public_url: &str) -> Names {
        let ip = match bound {
            SocketAddr::V4(a) => a.ip().to_string(),
            SocketAddr::V6(a) => format!("[{}]", a.ip()),
        };
        let public = url::Url::parse(public_url).ok().and_then(|u| {
            Some((
                u.scheme().to_string(),
                u.host_str()?.to_ascii_lowercase(),
                u.port_or_known_default()?,
            ))
        });
        Names {
            port: bound.port(),
            own: ["127.0.0.1".into(), "localhost".into(), "[::1]".into(), ip],
            public,
        }
    }

    fn own(&self, name: &str, port: Option<u16>) -> bool {
        self.own.iter().any(|n| n == name) && port.unwrap_or(80) == self.port
    }

    fn public(&self, scheme: Option<&str>, name: &str, port: Option<u16>) -> bool {
        self.public.as_ref().is_some_and(|(s, n, p)| {
            scheme.is_none_or(|scheme| scheme == s)
                && n == name
                && port.unwrap_or(if s == "https" { 443 } else { 80 }) == *p
        })
    }

    fn host(&self, host: &str) -> bool {
        authority(host)
            .is_some_and(|(name, port)| self.own(&name, port) || self.public(None, &name, port))
    }

    fn origin(&self, origin: &str) -> bool {
        let Some((scheme, rest)) = origin.split_once("://") else {
            return false;
        };
        authority(rest).is_some_and(|(name, port)| {
            (scheme == "http" && self.own(&name, port)) || self.public(Some(scheme), &name, port)
        })
    }
}

/// A Host or an Origin's authority as a lowercase name and its port, if it names one.
fn authority(s: &str) -> Option<(String, Option<u16>)> {
    let (name, port) = match s.strip_prefix('[') {
        Some(rest) => {
            let (ip, after) = rest.split_once(']')?;
            let port = match after {
                "" => None,
                p => Some(p.strip_prefix(':')?.parse().ok()?),
            };
            (format!("[{ip}]"), port)
        }
        None => match s.rsplit_once(':') {
            Some((name, p)) => (name.to_string(), Some(p.parse().ok()?)),
            None => (s.to_string(), None),
        },
    };
    if name.is_empty() || name.contains(['/', '@', ' ']) {
        return None;
    }
    Some((name.to_ascii_lowercase(), port))
}

/// What the address check needs: the public URL, from the state, and the address reel listens on.
#[derive(Clone)]
pub struct Bound {
    pub st: AppState,
    pub addr: SocketAddr,
}

/// Single mode: a page whose own name was pointed at the loopback is not reel's, so a Host or
/// an Origin that is not reel's own address is refused before anything else.
pub async fn own_address(State(b): State<Bound>, req: Request, next: Next) -> Response {
    let public_url = b.st.ctx.config().public_url.clone();
    let names = Names::of(b.addr, &public_url);
    let host = req
        .headers()
        .get(header::HOST)
        .map(|v| v.to_str().unwrap_or("").to_string())
        .or_else(|| req.uri().authority().map(|a| a.to_string()));
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .map(|v| v.to_str().unwrap_or(""));
    let host_ok = host.as_deref().is_some_and(|h| names.host(h));
    if host_ok && origin.is_none_or(|o| names.origin(o)) {
        return next.run(req).await;
    }
    tracing::warn!(?host, ?origin, path = %req.uri().path(), "without sign-in: a request for a name that is not reel's address, refused");
    crate::auth::refuse(
        &req,
        StatusCode::MISDIRECTED_REQUEST,
        "That is not reel's address.",
        &format!("Without sign-in, reel answers only at {public_url}."),
    )
}

/// The app's window in standalone; anyone else gets this.
pub fn not_the_app(req: &Request) -> Response {
    crate::auth::refuse(
        req,
        StatusCode::UNAUTHORIZED,
        "reel on this PC opens in its app.",
        "Its library opens in the app's window alone: open it from the tray icon.",
    )
}

#[derive(Deserialize)]
pub struct Open {
    ticket: Option<String>,
    to: Option<String>,
}

/// A path on this site: one slash, no backslash, nothing a browser reads as another host.
fn here(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains('\\')
        && !path.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// The app's window comes in here with a ticket from the app: the key becomes its cookie, and
/// the window goes on to the page it asked for.
pub async fn open(State(st): State<AppState>, Query(q): Query<Open>, req: Request) -> Response {
    let Some(key) = st.app_key.as_ref() else {
        return crate::error::AppError::NotFound.into_response();
    };
    if !q.ticket.as_deref().is_some_and(|t| key.spend(t)) {
        tracing::warn!("standalone: the app's way in was asked without a ticket of the app's");
        return not_the_app(&req);
    }
    let to = q.to.as_deref().filter(|p| here(p)).unwrap_or("/");
    let cookie = tower_sessions::cookie::Cookie::build((KEY_COOKIE, key.secret()))
        .http_only(true)
        .same_site(tower_sessions::cookie::SameSite::Lax)
        .path("/")
        .build();
    (
        [
            (header::SET_COOKIE, cookie.to_string()),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Redirect::to(to),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(public: &str) -> Names {
        Names::of("127.0.0.1:41234".parse().unwrap(), public)
    }

    #[test]
    fn the_names_of_its_own_address_and_the_public_urls_alone() {
        let n = names("http://127.0.0.1:41234");
        for ok in [
            "127.0.0.1:41234",
            "localhost:41234",
            "LocalHost:41234",
            "[::1]:41234",
        ] {
            assert!(n.host(ok), "{ok}");
        }
        for refused in [
            "rebound.example:41234",
            "127.0.0.1:41235",
            "127.0.0.1",
            "localhost",
            "127.0.0.2:41234",
            "[::2]:41234",
            "::1:41234",
            "",
            "127.0.0.1:41234/x",
            "user@127.0.0.1:41234",
        ] {
            assert!(!n.host(refused), "{refused}");
        }
        assert!(n.origin("http://127.0.0.1:41234"));
        assert!(n.origin("http://localhost:41234"));
        for refused in [
            "null",
            "http://rebound.example:41234",
            "https://127.0.0.1:41234",
            "tauri://localhost",
            "http://127.0.0.1",
        ] {
            assert!(!n.origin(refused), "{refused}");
        }
    }

    #[test]
    fn behind_a_proxy_on_the_same_pc_the_public_name_counts() {
        let n = names("https://clips.example.org");
        assert!(n.host("clips.example.org"));
        assert!(n.host("clips.example.org:443"));
        assert!(n.host("Clips.Example.org"));
        assert!(!n.host("clips.example.org:8443"));
        assert!(!n.host("example.org"));
        assert!(n.origin("https://clips.example.org"));
        assert!(!n.origin("http://clips.example.org"));
        assert!(n.host("127.0.0.1:41234"), "its own address still");
        let port80 = Names::of("127.0.0.1:80".parse().unwrap(), "http://localhost");
        assert!(port80.host("localhost") && port80.host("127.0.0.1"));
        assert!(port80.origin("http://localhost"));
    }

    #[test]
    fn the_key_in_a_header_or_the_windows_cookie() {
        let key = AppKey::new();
        let mut h = HeaderMap::new();
        assert!(!key.shown_by(&h));
        h.insert(
            header::COOKIE,
            "reel_session=x; reel_app_key=wrong".parse().unwrap(),
        );
        assert!(!key.shown_by(&h));
        h.insert(
            header::COOKIE,
            format!("reel_session=x; {KEY_COOKIE}={}", key.secret())
                .parse()
                .unwrap(),
        );
        assert!(key.shown_by(&h));
        let mut h = HeaderMap::new();
        h.insert(KEY_HEADER, key.secret().parse().unwrap());
        assert!(key.shown_by(&h));
        assert!(!key.matches(&key.secret()[1..]));
        assert_ne!(
            key.secret(),
            AppKey::new().secret(),
            "a new key at each start"
        );
        assert_eq!(format!("{key:?}"), "AppKey(..)");
    }

    #[test]
    fn a_ticket_is_good_once() {
        let key = AppKey::new();
        let (a, b) = (key.ticket(), key.ticket());
        assert_ne!(a, b);
        assert_ne!(a, key.secret());
        assert!(!key.spend(key.secret()), "the key is no ticket");
        assert!(key.spend(&b) && key.spend(&a));
        assert!(!key.spend(&a), "spent");
        assert!(!AppKey::new().spend(&key.ticket()), "another start's");
    }

    #[test]
    fn the_way_in_goes_on_to_a_page_of_this_site_alone() {
        for ok in ["/", "/clips/01J?t=2", "/settings/library#x"] {
            assert!(here(ok), "{ok}");
        }
        for refused in [
            "",
            "//evil.example/",
            "/\\evil.example",
            "/\t/evil.example",
            "https://evil.example/",
            "clips",
        ] {
            assert!(!here(refused), "{refused}");
        }
    }
}
