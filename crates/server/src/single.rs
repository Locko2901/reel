//! Single mode signs nobody in: reel answers only to the names of its own address.
use crate::AppState;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use std::net::SocketAddr;

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
}
