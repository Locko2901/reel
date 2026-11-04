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

#[derive(Debug, Clone)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub published_at: String,
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
    published_at: String,
}

pub fn parse_release(json: &str) -> Result<Release, String> {
    let r: GithubRelease = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(Release {
        version: r.tag_name.trim_start_matches('v').to_string(),
        url: r.html_url,
        published_at: r.published_at,
    })
}

pub async fn check_once(ctx: &Ctx) -> UpdateState {
    let repo = ctx.config().advanced.update_repo.clone();
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
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
            .map_err(|e| e.to_string())?;
        if resp.status().as_u16() == 404 {
            return Err(format!("{repo} has no release yet"));
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
        Err(e) => UpdateState {
            checked_at: Some(now()),
            latest: ctx.updates.read().await.latest.clone(),
            error: Some(e),
        },
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
