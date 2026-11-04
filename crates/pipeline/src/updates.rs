use crate::Ctx;
use reel_core::format::now;
use serde::Deserialize;
use std::time::Duration;

/// What the last check against the repository's releases found.
#[derive(Debug, Clone, Default)]
pub struct UpdateState {
    pub checked_at: Option<String>,
    pub latest: Option<Release>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub published_at: String,
    pub notes: String,
    /// Each file at the address a browser downloads it from, which is no API request.
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
}

impl UpdateState {
    pub fn newer(&self) -> Option<&Release> {
        self.latest
            .as_ref()
            .filter(|r| is_newer(&r.version, reel_core::VERSION))
    }
}

fn parts(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split(['.', '-'])
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

pub fn is_newer(candidate: &str, running: &str) -> bool {
    let (a, b) = (parts(candidate), parts(running));
    let n = a.len().max(b.len());
    for i in 0..n {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        if x != y {
            return x > y;
        }
    }
    false
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

pub fn parse_release(json: &str) -> Result<Release, String> {
    let r: GithubRelease = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(Release {
        version: r.tag_name.trim_start_matches('v').to_string(),
        url: r.html_url,
        published_at: r.published_at.unwrap_or_default(),
        notes: r.body.unwrap_or_default(),
        assets: r.assets,
    })
}

pub async fn check_once(ctx: &Ctx) -> UpdateState {
    check_at(ctx, &crate::app_update::github_api()).await
}

/// The repository's latest release from GitHub's API at `api`; the request carries reel's
/// version and nothing else.
pub async fn check_at(ctx: &Ctx, api: &str) -> UpdateState {
    let repo = ctx.config().advanced.update_repo.clone();
    let url = format!("{}/repos/{repo}/releases/latest", api.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(format!("reel/{}", reel_core::VERSION))
        .build();
    let result = async {
        let client = client.map_err(|e| e.to_string())?;
        let resp = client
            .get(&url)
            .header("accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| unreached(&e))?;
        if resp.status().as_u16() == 404 {
            return Err(format!("{repo} has no release yet"));
        }
        let spent = resp
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|v| v.as_bytes() == b"0");
        if spent && matches!(resp.status().as_u16(), 403 | 429) {
            return Err("GitHub's limit for this address is spent (60 questions an hour without an account); it answers again within the hour".into());
        }
        if !resp.status().is_success() {
            return Err(format!("GitHub answered {}", resp.status()));
        }
        let text = resp.text().await.map_err(|e| e.to_string())?;
        parse_release(&text)
    }
    .await;
    let state = match result {
        Ok(r) => UpdateState {
            checked_at: Some(now()),
            latest: Some(r),
            error: None,
        },
        Err(e) => {
            tracing::warn!(error = %e, "the newest release was not read");
            UpdateState {
                checked_at: Some(now()),
                latest: ctx.updates.read().await.latest.clone(),
                error: Some(e),
            }
        }
    };
    *ctx.updates.write().await = state.clone();
    if let Some(r) = state.newer() {
        tracing::info!(version = %r.version, "a newer release is out");
        ctx.emit(reel_core::Event::Update {
            version: r.version.clone(),
            url: r.url.clone(),
        });
    }
    state
}

/// A request that got no answer, in words without its address.
fn unreached(e: &reqwest::Error) -> String {
    let mut cause = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        cause = s.to_string();
        src = s.source();
    }
    if e.is_timeout() {
        "GitHub did not answer in time".into()
    } else if e.is_connect() {
        format!("GitHub could not be reached ({cause})")
    } else {
        format!("the request to GitHub failed ({cause})")
    }
}

pub async fn run(ctx: Ctx) {
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(60)) => {},
        _ = ctx.shutdown.cancelled() => return,
    }
    loop {
        if ctx.config().advanced.update_check {
            check_once(&ctx).await;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(24 * 3600)) => {},
            _ = ctx.shutdown.cancelled() => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("v0.4.0", "0.3.9"));
        assert!(is_newer("1.0.0", "0.99.1"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        assert!(is_newer("0.3.1", "0.3.0-rc1"));
    }

    #[test]
    fn a_github_release_parses() {
        let major: u64 = reel_core::VERSION
            .split('.')
            .next()
            .and_then(|m| m.parse().ok())
            .expect("a semver VERSION");
        let next = format!("{}.0.0", major + 1);
        let r = parse_release(&format!(
            r#"{{"tag_name":"v{next}","html_url":"https://github.com/x/reel/releases/tag/v{next}","published_at":"2026-10-14T10:00:00Z","body":"notes"}}"#
        ))
        .unwrap();
        assert_eq!(r.version, next);
        assert!(r.url.ends_with(&format!("v{next}")));
        assert_eq!(r.notes, "notes");
        assert!(r.assets.is_empty());
        let with_files = parse_release(r#"{"tag_name":"v2.0.0","html_url":"h","published_at":null,"body":null,"assets":[{"name":"reel_2.0.0_amd64.AppImage","browser_download_url":"https://github.com/o/r/releases/download/v2.0.0/reel_2.0.0_amd64.AppImage","url":"https://api.github.com/repos/o/r/releases/assets/1"}]}"#).unwrap();
        assert_eq!(
            with_files.assets,
            vec![Asset {
                name: "reel_2.0.0_amd64.AppImage".into(),
                url: "https://github.com/o/r/releases/download/v2.0.0/reel_2.0.0_amd64.AppImage"
                    .into()
            }],
            "a file at the address a browser downloads it from, which needs no API request"
        );
        assert_eq!(
            (with_files.notes.as_str(), with_files.published_at.as_str()),
            ("", "")
        );
        let state = UpdateState {
            checked_at: None,
            latest: Some(r),
            error: None,
        };
        assert!(
            state.newer().is_some(),
            "{next} is newer than {}",
            reel_core::VERSION
        );
        let same = UpdateState {
            checked_at: None,
            latest: parse_release(&format!(
                r#"{{"tag_name":"v{}","html_url":"https://github.com/x/reel/releases/tag/v{}","published_at":"2026-10-14T10:00:00Z","body":""}}"#,
                reel_core::VERSION,
                reel_core::VERSION
            ))
            .ok(),
            error: None,
        };
        assert!(
            same.newer().is_none(),
            "the running version is not an update"
        );
    }
}
