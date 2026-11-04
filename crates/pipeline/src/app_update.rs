use crate::Ctx;
use reel_core::Config;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The updater names its platform as `windows` or `linux` with the architecture apart, or as
/// one `windows-x86_64`, `linux-x86_64`; the server has the 64-bit apps only.
pub fn serves(target: Option<&str>, arch: Option<&str>) -> bool {
    let target_ok = matches!(
        target,
        None | Some("windows") | Some("windows-x86_64") | Some("linux") | Some("linux-x86_64")
    );
    let arch_ok = matches!(arch, None | Some("x86_64"));
    target_ok && arch_ok
}
pub const GITHUB_API: &str = "https://api.github.com";

/// Whether an app at `current` is older than this server: by number, and a pre-release of
/// the same number (a dev build) counts as older, as the updater's own comparison does.
pub fn offers_to(current: &str) -> bool {
    !current.trim().is_empty() && older(current, reel_core::VERSION)
}

pub use reel_core::versions::older;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Release {
    #[serde(rename = "tag_name")]
    pub tag: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}

impl Release {
    pub fn version(&self) -> &str {
        self.tag.trim_start_matches('v')
    }
}

/// The releases last read from the repository, published ones only, newest first.
static SEEN: std::sync::Mutex<Vec<Release>> = std::sync::Mutex::new(Vec::new());

fn seen() -> std::sync::MutexGuard<'static, Vec<Release>> {
    SEEN.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn keep_releases(mut list: Vec<Release>) {
    list.retain(|r| !r.draft && !r.prerelease);
    list.sort_by(|a, b| {
        if older(a.version(), b.version()) {
            std::cmp::Ordering::Greater
        } else if older(b.version(), a.version()) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    });
    *seen() = list;
}

/// The newest release the repository has, as last read; None before a read.
pub fn newest() -> Option<String> {
    seen().first().map(|r| r.version().to_string())
}

/// The notes of every release after `from` up to `to`, newest first, each under its version,
/// for the app to show what an update brings; just "reel <to>" without a read.
pub fn notes_between(from: &str, to: &str) -> String {
    let list = seen();
    let parts: Vec<String> = list
        .iter()
        .filter(|r| older(from, r.version()) && !older(to, r.version()))
        .map(|r| {
            let body = r.body.as_deref().unwrap_or("").trim();
            match body.is_empty() {
                true => format!("reel {}", r.version()),
                false => format!("reel {}\n\n{body}", r.version()),
            }
        })
        .collect();
    match parts.is_empty() {
        true => format!("reel {to}"),
        false => parts.join("\n\n"),
    }
}

pub async fn read_releases(api: &str, repo: &str, token: &str) -> Result<Vec<Release>, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(format!("reel/{}", reel_core::VERSION))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client
        .get(format!("{api}/repos/{repo}/releases?per_page=30"))
        .header("accept", "application/vnd.github+json");
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("GitHub answered {}", resp.status()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

/// Which installer an app came from, each offered its own kind; the .deb does not update
/// itself. On Windows `WithObs` is the package called reel, `Plain` reel light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavour {
    Plain,
    WithObs,
    AppImage,
    Deb,
}

impl Flavour {
    pub const ALL: [Flavour; 4] = [
        Flavour::Plain,
        Flavour::WithObs,
        Flavour::AppImage,
        Flavour::Deb,
    ];

    /// As the app names itself in its update request: `with-obs` with OBS, none without.
    pub fn parse(s: Option<&str>) -> Flavour {
        match s {
            Some("with-obs") | Some("reel-with-obs") => Flavour::WithObs,
            Some("appimage") | Some("reel-appimage") => Flavour::AppImage,
            Some("deb") | Some("reel-deb") => Flavour::Deb,
            _ => Flavour::Plain,
        }
    }

    /// The kind an update request is for: a Linux app runs from the AppImage, as the .deb
    /// asks for none.
    pub fn for_update(flavour: Option<&str>, target: Option<&str>) -> Flavour {
        match target {
            Some(t) if t.starts_with("linux") => Flavour::AppImage,
            _ => Flavour::parse(flavour),
        }
    }

    /// The download pages' address for it: a Windows installer's file before its version.
    pub fn id(self) -> &'static str {
        match self {
            Flavour::Plain => reel_core::packages::windows(false).file,
            Flavour::WithObs => reel_core::packages::windows(true).file,
            Flavour::AppImage => "reel-appimage",
            Flavour::Deb => "reel-deb",
        }
    }

    pub fn from_id(id: &str) -> Option<Flavour> {
        Flavour::ALL.into_iter().find(|f| f.id() == id)
    }

    /// As the update's address names it, none for the app without OBS.
    pub fn query(self) -> Option<&'static str> {
        match self {
            Flavour::Plain => None,
            Flavour::WithObs => Some("with-obs"),
            Flavour::AppImage => Some("appimage"),
            Flavour::Deb => Some("deb"),
        }
    }
}

pub fn installer_name(version: &str, flavour: Flavour) -> String {
    match flavour {
        Flavour::Plain => reel_core::packages::windows_installer(version, false),
        Flavour::WithObs => reel_core::packages::windows_installer(version, true),
        Flavour::AppImage => format!("reel_{version}_amd64.AppImage"),
        Flavour::Deb => format!("reel_{version}_amd64.deb"),
    }
}

pub fn dir(cfg: &Config) -> PathBuf {
    cfg.data_dir.join("app-updates")
}

/// The app of this server's version, ready to hand to a paired app.
#[derive(Debug, Clone)]
pub struct Offer {
    pub version: String,
    pub installer: PathBuf,
    pub signature: String,
    pub published: String,
}

pub async fn offer(cfg: &Config, flavour: Flavour) -> Option<Offer> {
    let installer = dir(cfg).join(installer_name(reel_core::VERSION, flavour));
    let mut sig = installer.as_os_str().to_owned();
    sig.push(".sig");
    let signature = tokio::fs::read_to_string(PathBuf::from(sig)).await.ok()?;
    let meta = tokio::fs::metadata(&installer).await.ok()?;
    let published = meta
        .modified()
        .ok()
        .and_then(|t| jiff::Timestamp::try_from(t).ok())
        .map(|t| t.to_string())
        .unwrap_or_else(reel_core::format::now);
    Some(Offer {
        version: reel_core::VERSION.to_string(),
        installer,
        signature: signature.trim().to_string(),
        published,
    })
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    url: String,
}

#[derive(Deserialize)]
struct ReleaseAssets {
    assets: Vec<Asset>,
}

/// Fetches the installer of one kind and its signature from the release `v<version>`, each
/// through a part file; false when either is missing. `token` is empty for a public repository.
pub async fn fetch_from_release(
    api: &str,
    repo: &str,
    version: &str,
    token: &str,
    into: &Path,
    flavour: Flavour,
) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .user_agent(format!("reel/{}", reel_core::VERSION))
        .build()
        .map_err(|e| e.to_string())?;
    let authed = |r: reqwest::RequestBuilder| {
        if token.is_empty() {
            r
        } else {
            r.bearer_auth(token)
        }
    };
    let release = authed(client.get(format!("{api}/repos/{repo}/releases/tags/v{version}")))
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if release.status().as_u16() == 404 {
        return Ok(false);
    }
    if !release.status().is_success() {
        return Err(format!("GitHub answered {}", release.status()));
    }
    let assets: ReleaseAssets = release.json().await.map_err(|e| e.to_string())?;
    let exe = installer_name(version, flavour);
    let sig = format!("{exe}.sig");
    let pick = |name: &str| assets.assets.iter().find(|a| a.name == name);
    let (Some(exe_asset), Some(sig_asset)) = (pick(&exe), pick(&sig)) else {
        return Ok(false);
    };
    tokio::fs::create_dir_all(into)
        .await
        .map_err(|e| e.to_string())?;
    for (asset, name) in [(sig_asset, &sig), (exe_asset, &exe)] {
        let resp = authed(client.get(&asset.url))
            .header("accept", "application/octet-stream")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("{name}: GitHub answered {}", resp.status()));
        }
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        let part = into.join(format!("{name}.part"));
        tokio::fs::write(&part, &bytes)
            .await
            .map_err(|e| e.to_string())?;
        tokio::fs::rename(&part, into.join(name))
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(true)
}

/// At start and once a day while one of this version's installers is missing, fetches every
/// kind; each pass also reads the repository's releases.
pub async fn run(ctx: Ctx) {
    loop {
        let cfg = ctx.config();
        let token = cfg.device.updates_token.clone();
        if cfg.device.app_updates {
            match read_releases(GITHUB_API, &cfg.advanced.update_repo, &token).await {
                Ok(list) => {
                    keep_releases(list);
                    tracing::info!(newest = ?newest(), "the repository's releases read");
                }
                Err(e) => tracing::info!(error = %e, "the repository's releases were not read"),
            }
        }
        for flavour in Flavour::ALL {
            if !cfg.device.app_updates || offer(&cfg, flavour).await.is_some() {
                continue;
            }
            match fetch_from_release(
                GITHUB_API,
                &cfg.advanced.update_repo,
                reel_core::VERSION,
                &token,
                &dir(&cfg),
                flavour,
            )
            .await
            {
                Ok(true) => tracing::info!(
                    version = reel_core::VERSION,
                    installer = flavour.id(),
                    "the app's installer for this version is fetched"
                ),
                Ok(false) => tracing::info!(
                    version = reel_core::VERSION,
                    installer = flavour.id(),
                    "the release has no signed installer of this kind for this version yet"
                ),
                Err(e) => {
                    tracing::warn!(error = %e, installer = flavour.id(), "the app's installer could not be fetched")
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(24 * 3600)) => {},
            _ = ctx.shutdown.cancelled() => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_release_and_the_notes_an_update_spans() {
        let rel = |tag: &str, body: &str| Release {
            tag: tag.into(),
            body: Some(body.into()),
            published_at: None,
            draft: false,
            prerelease: false,
        };
        assert!(
            older("1.3.1", "1.4.0") && older("1.4.1-dev.abc", "1.4.1") && !older("1.4.1", "1.4.1")
        );
        assert!(older("1.9.0", "1.10.0") && !older("2.0.0", "1.10.0"));
        let mut draft = rel("v9.0.0", "not out");
        draft.draft = true;
        keep_releases(vec![
            rel("v1.3.2", "OBS 33"),
            rel("v1.4.1", "one width"),
            draft,
            rel("v1.4.0", "a menu per clip"),
            rel("v1.3.3", ""),
        ]);
        assert_eq!(newest().as_deref(), Some("1.4.1"));
        assert_eq!(
            notes_between("1.3.1", "1.4.0"),
            "reel 1.4.0\n\na menu per clip\n\nreel 1.3.3\n\nreel 1.3.2\n\nOBS 33"
        );
        assert_eq!(
            notes_between("1.4.0", "1.4.0"),
            "reel 1.4.0",
            "nothing between: the version alone"
        );
        keep_releases(Vec::new());
        assert_eq!(newest(), None);
    }

    #[test]
    fn an_older_app_and_a_dev_build_of_this_version_are_offered_it() {
        let v = reel_core::VERSION;
        assert!(offers_to("0.1.0"));
        assert!(offers_to(&format!("{v}-dev.88822c0")));
        assert!(!offers_to(v));
        assert!(!offers_to("999.0.0"));
        assert!(!offers_to(""));
        assert_eq!(
            installer_name("1.2.0", Flavour::WithObs),
            "reel_1.2.0_x64-setup.exe",
            "the package with OBS is reel"
        );
        assert_eq!(
            installer_name("1.2.0", Flavour::Plain),
            "reel-light_1.2.0_x64-setup.exe",
            "the one without is reel light"
        );
        assert_eq!(
            installer_name("1.2.0", Flavour::AppImage),
            "reel_1.2.0_amd64.AppImage"
        );
        assert_eq!(
            installer_name("1.2.0", Flavour::Deb),
            "reel_1.2.0_amd64.deb"
        );
        assert_eq!(
            Flavour::parse(None),
            Flavour::Plain,
            "an app without OBS, of any version, names no flavour"
        );
        assert_eq!(Flavour::parse(Some("with-obs")), Flavour::WithObs);
        assert_eq!(
            Flavour::for_update(None, Some("linux")),
            Flavour::AppImage,
            "a Linux app updates its AppImage"
        );
        assert_eq!(
            Flavour::for_update(Some("with-obs"), Some("windows")),
            Flavour::WithObs
        );
        for f in Flavour::ALL {
            assert_eq!(Flavour::from_id(f.id()), Some(f));
            assert_eq!(Flavour::parse(f.query()), f);
        }
        assert_eq!(Flavour::from_id("reel-with-obs"), None);
        assert!(serves(Some("windows"), Some("x86_64")));
        assert!(serves(Some("windows-x86_64"), None));
        assert!(serves(Some("linux"), Some("x86_64")));
        assert!(serves(Some("linux-x86_64"), None));
        assert!(!serves(Some("darwin"), Some("x86_64")));
        assert!(!serves(Some("windows"), Some("aarch64")));
    }
}
