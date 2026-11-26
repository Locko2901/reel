//! Settings, About: what this reel is, where its code and docs are, its licence, and the work of
//! others it ships with. Every link on it is absolute and https but the licence page, this
//! server's own: the page is the same in a browser, in the app's window and in reel on this PC.
use super::licences;
use serde_json::Value;

/// reel's repository and its docs site.
pub const REPO: &str = "https://github.com/Locko2901/reel";
pub const DOCS: &str = "https://locko2901.github.io/reel/";

pub struct AboutLink {
    pub label: String,
    /// What the link opens, for a screen reader, where the label alone does not say.
    pub aria: String,
    pub url: String,
}

/// A row in a Settings row's grid: the title and the line on the left; on the right a value,
/// with a part of it linked where there is one, or the links as buttons.
pub struct AboutRow {
    pub title: String,
    pub help: String,
    pub value: String,
    pub value_link: Option<AboutLink>,
    pub links: Vec<AboutLink>,
}

pub struct AboutView {
    pub version: &'static str,
    pub reel: Vec<AboutRow>,
    pub links: Vec<AboutRow>,
    pub credits: Vec<AboutRow>,
    /// The licence page: every crate's licence text, on this server.
    pub licences: &'static str,
}

fn link(label: impl Into<String>, aria: impl Into<String>, url: impl Into<String>) -> AboutLink {
    AboutLink {
        label: label.into(),
        aria: aria.into(),
        url: url.into(),
    }
}

fn row(title: impl Into<String>, help: impl Into<String>) -> AboutRow {
    AboutRow {
        title: title.into(),
        help: help.into(),
        value: String::new(),
        value_link: None,
        links: Vec::new(),
    }
}

/// A file of reel's repository as it is in this version's tag.
fn at_tag(path: &str) -> String {
    format!("{REPO}/blob/v{}/{path}", reel_core::VERSION)
}

/// A pin of desktop/: its version and, for OBS, where its source is.
fn pin(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_default()
}

fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}

/// The commit, linked where it is one; a build without git says "unknown".
fn commit(sha: &str) -> Option<AboutLink> {
    let hex = sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit());
    hex.then(|| {
        link(
            sha,
            format!("commit {sha} on GitHub"),
            format!("{REPO}/commit/{sha}"),
        )
    })
}

/// `ffmpeg version 7.1.2-Jellyfin Copyright (c) ...` gives `7.1.2-Jellyfin`.
pub fn ffmpeg_words(line: &str) -> String {
    line.strip_prefix("ffmpeg version ")
        .unwrap_or(line)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

/// The page's content. `ffmpeg`: the first line of `ffmpeg -version` here, as Status shows it.
pub fn view(standalone: bool, ffmpeg: Option<&str>) -> AboutView {
    let version = reel_core::VERSION;
    let obs = pin(include_str!("../../../../desktop/obs/pinned.json"));
    let ff_win = pin(include_str!("../../../../desktop/ffmpeg/pinned.json"));
    let ff_linux = pin(include_str!("../../../../desktop/ffmpeg/pinned-linux.json"));
    let here = ffmpeg.map(ffmpeg_words).filter(|v| !v.is_empty());

    let mut version_row = row(
        "Version",
        if standalone {
            "reel on this PC, which the app runs; the same as the app's own"
        } else {
            "this server's"
        },
    );
    version_row.value = version.to_string();
    let mut built = row(
        "Built",
        "the commit it was built from, and that commit's day",
    );
    built.value = reel_core::BUILD_DATE.to_string();
    built.value_link = commit(reel_core::GIT_SHA);
    if built.value_link.is_none() {
        built.value = format!("{} from an unknown commit", reel_core::BUILD_DATE);
    }
    let mut licence = row(
        "Licence",
        "MIT, copyright (c) 2026 Lockoo: free to use, change and share, keeping this notice",
    );
    licence.links.push(link(
        "Read it",
        "Read it: reel's licence, MIT",
        at_tag("LICENSE"),
    ));

    let links = vec![
        AboutRow {
            links: vec![link("Open", "Open the docs", DOCS)],
            ..row(
                "Docs",
                "how reel works, every setting, and what to do when something goes wrong",
            )
        },
        AboutRow {
            links: vec![link("Open", "Open reel on GitHub", REPO)],
            ..row(
                "Source code",
                "reel's repository on GitHub, under the MIT licence",
            )
        },
        AboutRow {
            links: vec![link(
                "Open",
                format!("Open the release notes of reel {version}"),
                format!("{REPO}/releases/tag/v{version}"),
            )],
            ..row(
                "Release notes",
                format!("what reel {version} changed, on its release's page"),
            )
        },
        AboutRow {
            links: vec![link(
                "Open",
                "Open reel's issues on GitHub",
                format!("{REPO}/issues"),
            )],
            ..row(
                "Report a bug",
                "an issue on GitHub: what you did, what happened, and the version above",
            )
        },
    ];

    let obs_version = text(&obs, "version");
    let ffmpeg_pins = {
        let (w, l) = (text(&ff_win, "version"), text(&ff_linux, "version"));
        if w == l {
            w
        } else {
            format!("{w} on Windows, {l} on Linux")
        }
    };
    let ffmpeg_source = format!(
        "https://github.com/FFmpeg/FFmpeg/tree/n{}",
        text(&ff_win, "version")
    );
    let mut credits = vec![AboutRow {
        links: vec![
            link(
                "Licence",
                format!("Licence of OBS Studio {obs_version}, the GPL 2.0"),
                format!("https://github.com/obsproject/obs-studio/blob/{obs_version}/COPYING"),
            ),
            link(
                "Source",
                format!("Source of OBS Studio {obs_version}"),
                text(&obs, "source"),
            ),
        ],
        ..row(
            format!("OBS Studio {obs_version}"),
            "in reel for Windows (the installer with OBS), unmodified; GNU GPL 2.0 or later, its text in the install folder",
        )
    }];
    let ffmpeg_where = if standalone {
        match &here {
            Some(v) => format!("{v} runs every probe and encode of the library on this PC: in every download of the app, gyan.dev's build on Windows, BtbN's on Linux; GNU GPL 3.0, its text beside it"),
            None => "in every download of the app, gyan.dev's build on Windows, BtbN's on Linux, for the library on this PC; GNU GPL 3.0, its text beside it".into(),
        }
    } else {
        "in every download of the app, gyan.dev's build on Windows, BtbN's on Linux; GNU GPL 3.0, its text beside it".into()
    };
    credits.push(AboutRow {
        links: vec![link(
            "Source",
            format!("Source of FFmpeg {}", text(&ff_win, "version")),
            ffmpeg_source,
        )],
        ..row(format!("FFmpeg {ffmpeg_pins}"), ffmpeg_where)
    });
    if !standalone {
        match &here {
            Some(v) if v.to_lowercase().contains("jellyfin") => credits.push(AboutRow {
                links: vec![link(
                    "Source",
                    "Source of jellyfin-ffmpeg",
                    "https://github.com/jellyfin/jellyfin-ffmpeg",
                )],
                ..row(
                    format!("jellyfin-ffmpeg {}", v.trim_end_matches("-Jellyfin")),
                    "in the server's image, every probe and encode on this server, with Intel's media driver and libva (MIT); GNU GPL 3.0",
                )
            }),
            Some(v) => credits.push(row(
                format!("FFmpeg {v}"),
                "every probe and encode on this server: the one it was given, under its own build's licence",
            )),
            None => {}
        }
        credits.push(AboutRow {
            links: vec![link(
                "Source",
                "Source of Debian's packages, by version",
                "https://snapshot.debian.org/",
            )],
            ..row(
                "Debian 12",
                "the server's image is built on it; each package's licence is in /usr/share/doc/<package>/copyright there",
            )
        });
    }
    if !standalone || cfg!(target_os = "linux") {
        credits.push(AboutRow {
            links: vec![link(
                "Versions",
                "Versions the AppImage carries, with their source",
                format!("{}#debians-libraries", at_tag("THIRD_PARTY.md")),
            )],
            ..row(
                "WebKitGTK, GTK and GStreamer",
                "the app's window on Linux: Debian 12's builds in the AppImage, your distribution's with the .deb; GNU LGPL 2.1 or later, some plugins GPL",
            )
        });
    }
    if !standalone || cfg!(windows) {
        credits.push(AboutRow {
            links: vec![link(
                "Terms",
                "Terms of Microsoft's WebView2",
                "https://developer.microsoft.com/microsoft-edge/webview2/",
            )],
            ..row(
                "Microsoft Edge WebView2",
                "the app's window on Windows: the installers carry its bootstrapper, which installs it where Windows lacks it",
            )
        });
    }
    credits.push(AboutRow {
        links: vec![link(
            "Licence",
            "Licence of the Geist fonts, the SIL Open Font License",
            at_tag("web/assets/fonts/LICENSE-geist"),
        )],
        ..row(
            "Geist and Geist Mono",
            "the pages' type, copyright 2024 The Geist Project Authors; SIL Open Font License 1.1",
        )
    });
    let crates = match licences::crate_count() {
        Some(n) => format!("{n} crates the server and the app are built from: MIT or Apache 2.0 for most, a few MPL 2.0, all unmodified"),
        None => "the crates the server and the app are built from, each under its own licence".into(),
    };
    credits.push(AboutRow {
        links: vec![link(
            "The list",
            "The list of every crate, with its licence text",
            licences::PATH,
        )],
        ..row("Rust crates", crates)
    });

    AboutView {
        version,
        reel: vec![version_row, built, licence],
        links,
        credits,
        licences: licences::PATH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_url(v: &AboutView) -> Vec<String> {
        v.reel
            .iter()
            .chain(&v.links)
            .chain(&v.credits)
            .flat_map(|r| r.links.iter().chain(r.value_link.as_ref()))
            .map(|l| l.url.clone())
            .chain([v.licences.to_string()])
            .collect()
    }

    #[test]
    fn every_link_is_absolute_and_https_but_the_licence_page() {
        for standalone in [false, true] {
            for ffmpeg in [
                None,
                Some("ffmpeg version 7.1.2-Jellyfin Copyright (c) 2000-2025 the FFmpeg developers"),
                Some("ffmpeg version 6.1.1-3ubuntu5 Copyright (c) 2000-2023 the FFmpeg developers"),
            ] {
                let v = view(standalone, ffmpeg);
                let urls = every_url(&v);
                assert!(urls.len() > 8);
                assert_eq!(
                    urls.iter().filter(|u| *u == licences::PATH).count(),
                    2,
                    "Every one and The list"
                );
                for u in urls.into_iter().filter(|u| u != licences::PATH) {
                    let rest = u.strip_prefix("https://").unwrap_or_else(|| panic!("{u}"));
                    assert!(
                        rest.split('/').next().is_some_and(|h| h.contains('.')),
                        "{u} names its host"
                    );
                    assert!(!u.contains(char::is_whitespace), "{u}");
                }
            }
        }
    }

    #[test]
    fn the_page_names_this_version_and_the_pinned_obs_and_ffmpeg() {
        let v = view(false, None);
        assert_eq!(v.version, reel_core::VERSION);
        assert_eq!(v.reel[0].value, reel_core::VERSION);
        let obs = pin(include_str!("../../../../desktop/obs/pinned.json"));
        let obs_version = text(&obs, "version");
        let credit = v
            .credits
            .iter()
            .find(|r| r.title.starts_with("OBS Studio"))
            .expect("OBS is credited");
        assert_eq!(credit.title, format!("OBS Studio {obs_version}"));
        assert!(credit.links.iter().any(
            |l| l.url == format!("https://github.com/obsproject/obs-studio/tree/{obs_version}")
        ));
        assert_eq!(obs["license"], "GPL-2.0-or-later");
        assert!(credit.help.contains("GPL 2.0 or later"));
        let ff = pin(include_str!("../../../../desktop/ffmpeg/pinned.json"));
        assert!(v
            .credits
            .iter()
            .any(|r| r.title == format!("FFmpeg {}", text(&ff, "version"))));
        assert!(v
            .links
            .iter()
            .any(|r| r.links[0].url == format!("{REPO}/releases/tag/v{}", reel_core::VERSION)));
    }

    #[test]
    fn the_licence_line_is_the_licence_files() {
        let licence =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../LICENSE")).unwrap();
        assert!(licence.starts_with("MIT License"));
        let holder = licence
            .lines()
            .find_map(|l| l.strip_prefix("Copyright (c) "))
            .unwrap();
        assert!(view(false, None).reel[2]
            .help
            .contains(&format!("copyright (c) {holder}")));
    }

    #[test]
    fn the_crate_count_is_the_licence_pages() {
        let v = view(true, None);
        let crates = v
            .credits
            .iter()
            .find(|r| r.title == "Rust crates")
            .expect("the crates are credited");
        match licences::crate_count() {
            Some(n) => assert!(crates.help.starts_with(&format!("{n} crates"))),
            None => assert!(crates.help.starts_with("the crates")),
        }
        assert_eq!(crates.links[0].url, licences::PATH);
        assert_eq!(
            ffmpeg_words("ffmpeg version n9.0.2-17-g2a571b6068 Copyright (c) 2000-2026"),
            "n9.0.2-17-g2a571b6068"
        );
    }
}
