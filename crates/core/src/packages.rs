//! The two Windows downloads as `desktop/packages.json` names them, for the server, the app,
//! the installers and the release; Linux keeps `reel_<version>_amd64.AppImage` and `.deb`.
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
struct Names {
    with_obs_name: String,
    with_obs_file: String,
    plain_name: String,
    plain_file: String,
}

static NAMES: LazyLock<Names> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../desktop/packages.json"))
        .expect("desktop/packages.json names both packages")
});

/// One Windows download: the name people read, and its installer's file before the version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Package {
    pub name: &'static str,
    pub file: &'static str,
}

/// The package with OBS in it, or the one for a PC with its own OBS.
pub fn windows(with_obs: bool) -> Package {
    let n = &*NAMES;
    match with_obs {
        true => Package {
            name: &n.with_obs_name,
            file: &n.with_obs_file,
        },
        false => Package {
            name: &n.plain_name,
            file: &n.plain_file,
        },
    }
}

/// Its installer as the release carries it.
pub fn windows_installer(version: &str, with_obs: bool) -> String {
    format!("{}_{version}_x64-setup.exe", windows(with_obs).file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_packages_are_named_apart() {
        let (full, light) = (windows(true), windows(false));
        assert!(!full.name.is_empty() && !light.name.is_empty() && full.name != light.name);
        for f in [full.file, light.file] {
            assert!(!f.is_empty() && f.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        }
        assert_ne!(full.file, light.file);
        assert_eq!(
            windows_installer("1.11.0", false),
            format!("{}_1.11.0_x64-setup.exe", light.file)
        );
    }
}
