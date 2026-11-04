//! Where the clip page's back button goes: the page of reel's that `Referer` names.

use crate::AppState;

/// A page of reel's a clip page can be opened from.
#[derive(Debug, PartialEq, Eq)]
pub enum Origin {
    Library,
    Collections,
    Collection(String),
    Trash,
    Shares,
    Jobs,
    Status,
    Settings,
    Upload,
    Stats,
    Clip(String),
    Other,
}

/// `history` when the page before is that one, so going back in history returns to it as left.
#[derive(Debug, PartialEq, Eq)]
pub struct Back {
    pub href: String,
    pub label: String,
    pub history: bool,
}

impl Back {
    fn library() -> Self {
        Back {
            href: "/".into(),
            label: "Library".into(),
            history: false,
        }
    }
}

/// The path, query and page a `Referer` names when its host is this server's (the request's or
/// the public address's).
pub fn origin(referer: Option<&str>, hosts: &[&str]) -> Option<(String, Origin)> {
    let url = url::Url::parse(referer?).ok()?;
    let host = url.host_str()?;
    let authority = match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    };
    if !hosts.iter().any(|h| h.eq_ignore_ascii_case(&authority)) {
        return None;
    }
    let path = url.path();
    // "//evil.test/" (and "/\\evil.test/", which the parser turns into it) links to another site
    if path.starts_with("//") {
        return None;
    }
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let page = match parts.as_slice() {
        [""] => Origin::Library,
        ["collections"] => Origin::Collections,
        ["collections", id] => Origin::Collection(id.to_string()),
        ["trash"] => Origin::Trash,
        ["shares"] => Origin::Shares,
        ["jobs"] => Origin::Jobs,
        ["status"] => Origin::Status,
        ["settings", ..] => Origin::Settings,
        ["upload"] => Origin::Upload,
        ["stats"] => Origin::Stats,
        ["clips", id] => Origin::Clip(id.to_string()),
        ["signin", ..] | ["setup", ..] | ["oidc", ..] | ["share-target", ..] => return None,
        _ => Origin::Other,
    };
    let href = match url.query() {
        Some(q) => format!("{path}?{q}"),
        None => path.to_string(),
    };
    Some((href, page))
}

/// The authority of an address, as a `Host` header says it.
pub fn authority(address: &str) -> Option<String> {
    let url = url::Url::parse(address).ok()?;
    let host = url.host_str()?;
    Some(match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.to_string(),
    })
}

/// A collection or clip by its name, a gone or unknown page as "Back", anywhere else the library.
pub async fn resolve(st: &AppState, from: Option<(String, Origin)>, clip_id: &str) -> Back {
    let Some((href, page)) = from else {
        return Back::library();
    };
    let label = match page {
        Origin::Library => "Library".to_string(),
        Origin::Collections => "Collections".into(),
        Origin::Collection(id) => match st.ctx.store.collection(&id).await {
            Ok(Some(c)) => c.name,
            _ => "Back".into(),
        },
        Origin::Trash => "Trash".into(),
        Origin::Shares => "Shares".into(),
        Origin::Jobs => "Jobs".into(),
        Origin::Status => "Status".into(),
        Origin::Settings => "Settings".into(),
        Origin::Upload => "Upload".into(),
        Origin::Stats => "Stats".into(),
        Origin::Clip(id) if id == clip_id => return Back::library(),
        Origin::Clip(id) => match st.ctx.store.clip(&id).await {
            Ok(Some(c)) if c.deleted_at.is_none() => c.title,
            _ => "Back".into(),
        },
        Origin::Other => "Back".into(),
    };
    Back {
        href,
        label,
        history: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTS: &[&str] = &["clips.example.org", "127.0.0.1:8746"];

    fn at(referer: &str) -> Option<(String, Origin)> {
        origin(Some(referer), HOSTS)
    }

    #[test]
    fn the_pages_of_reel_by_their_paths() {
        assert_eq!(
            at("https://clips.example.org/"),
            Some(("/".into(), Origin::Library))
        );
        assert_eq!(
            at("https://clips.example.org/?q=ace&game=valorant&set=01ABC"),
            Some(("/?q=ace&game=valorant&set=01ABC".into(), Origin::Library))
        );
        assert_eq!(
            at("https://clips.example.org/collections/01SET"),
            Some((
                "/collections/01SET".into(),
                Origin::Collection("01SET".into())
            ))
        );
        assert_eq!(
            at("https://clips.example.org/collections"),
            Some(("/collections".into(), Origin::Collections))
        );
        assert_eq!(
            at("http://127.0.0.1:8746/trash"),
            Some(("/trash".into(), Origin::Trash))
        );
        assert_eq!(
            at("http://127.0.0.1:8746/settings/storage"),
            Some(("/settings/storage".into(), Origin::Settings))
        );
        assert_eq!(
            at("http://127.0.0.1:8746/stats"),
            Some(("/stats".into(), Origin::Stats))
        );
        assert_eq!(
            at("http://127.0.0.1:8746/clips/01CLIP"),
            Some(("/clips/01CLIP".into(), Origin::Clip("01CLIP".into())))
        );
        assert_eq!(
            at("https://clips.example.org/s/token"),
            Some(("/s/token".into(), Origin::Other))
        );
    }

    #[test]
    fn another_site_or_none_is_no_way_back() {
        assert_eq!(origin(None, HOSTS), None);
        assert_eq!(at("https://example.com/"), None);
        assert_eq!(at("https://clips.example.org.evil.test/"), None);
        assert_eq!(at("http://127.0.0.1:8747/"), None);
        assert_eq!(at("https://clips.example.org//evil.test/x"), None);
        assert_eq!(at("https://clips.example.org/\\evil.test/x"), None);
        assert_eq!(at("not a url"), None);
        assert_eq!(at("tauri://localhost/index.html"), None);
    }

    #[test]
    fn the_sign_in_and_the_first_run_are_no_place_to_go_back_to() {
        assert_eq!(
            at("https://clips.example.org/signin?next=/clips/01CLIP"),
            None
        );
        assert_eq!(at("https://clips.example.org/setup?step=2"), None);
    }

    #[test]
    fn an_authority_as_a_host_header_says_it() {
        assert_eq!(
            authority("https://clips.example.org").as_deref(),
            Some("clips.example.org")
        );
        assert_eq!(
            authority("http://10.0.0.5:8746/").as_deref(),
            Some("10.0.0.5:8746")
        );
        assert_eq!(authority("").as_deref(), None);
    }
}
