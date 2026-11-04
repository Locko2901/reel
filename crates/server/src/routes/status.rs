use crate::auth::User;
use crate::disk::Originals;
use crate::error::AppResult;
use crate::routes::html;
use crate::views::Who;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::response::Response;
use axum::{Extension, Json};
use reel_core::format;
use serde_json::{json, Value};

pub struct Check {
    pub name: &'static str,
    pub value: String,
    pub class: &'static str,
}

impl Check {
    /// The dot that goes with the value's colour.
    pub fn dot(&self) -> &'static str {
        match self.class {
            "st-ok" => "ok",
            "st-warn" => "warn",
            "st-err" => "err",
            "st-active" => "active",
            _ => "",
        }
    }
}

pub struct ChangeLine {
    pub heading: bool,
    pub pieces: Vec<Piece>,
}

/// A run of a changelog line: plain, `code` or **bold** (a scope).
#[derive(Debug, PartialEq)]
pub struct Piece {
    pub text: String,
    pub code: bool,
    pub bold: bool,
}

impl Piece {
    fn plain(text: &str) -> Piece {
        Piece {
            text: text.to_string(),
            code: false,
            bold: false,
        }
    }
}

/// The inline marks of a line: `code` and **bold**; a mark without its end is text.
pub fn pieces(line: &str) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut text = String::new();
    let mut rest = line;
    let mut bold = false;
    let mut flush = |text: &mut String, code: bool, bold: bool| {
        if !text.is_empty() {
            out.push(Piece {
                text: std::mem::take(text),
                code,
                bold,
            });
        }
    };
    while let Some(c) = rest.chars().next() {
        if c == '`' {
            if let Some(end) = rest[1..].find('`') {
                flush(&mut text, false, bold);
                let mut code = rest[1..1 + end].to_string();
                flush(&mut code, true, bold);
                rest = &rest[end + 2..];
                continue;
            }
        } else if rest.starts_with("**") && (bold || rest[2..].contains("**")) {
            flush(&mut text, false, bold);
            bold = !bold;
            rest = &rest[2..];
            continue;
        }
        text.push(c);
        rest = &rest[c.len_utf8()..];
    }
    flush(&mut text, false, bold);
    out
}

pub struct ReleaseView {
    pub title: String,
    pub lines: Vec<ChangeLine>,
}

#[derive(Template)]
#[template(path = "status.html")]
pub struct StatusPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub version: &'static str,
    pub built: String,
    pub running: String,
    pub library: String,
    pub update_line: String,
    pub update_class: &'static str,
    pub update_url: Option<String>,
    /// The newest release this server knows of.
    pub newest: String,
    pub update_on: bool,
    pub checks: Vec<Check>,
    pub releases: Vec<ReleaseView>,
    pub repo: String,
    /// The OBS the app's Windows package with OBS carries, that package's name, and where
    /// OBS's source is.
    pub obs_version: String,
    pub obs_package: &'static str,
    pub obs_source: String,
}

fn bundled_obs() -> (String, String) {
    let pin: serde_json::Value =
        serde_json::from_str(include_str!("../../../../desktop/obs/pinned.json"))
            .unwrap_or_default();
    let text = |k: &str| pin[k].as_str().unwrap_or_default().to_string();
    (text("version"), text("source"))
}

/// `[text](url)` becomes `text`; the commit links release-please appends go entirely.
pub fn strip_links(s: &str) -> String {
    unlink(s).replace("**", "")
}

fn unlink(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](") else {
            break;
        };
        let text = &rest[open + 1..open + close];
        let after = &rest[open + close + 2..];
        let Some(end) = after.find(')') else {
            break;
        };
        out.push_str(&rest[..open]);
        if !text.chars().all(|c| c.is_ascii_hexdigit()) || text.len() < 7 {
            out.push_str(text);
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out.replace(" ()", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The newest `max` releases of a release-please changelog.
pub fn parse_changelog(text: &str, max: usize) -> Vec<ReleaseView> {
    let mut out: Vec<ReleaseView> = Vec::new();
    for line in text.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            if out.len() == max {
                break;
            }
            out.push(ReleaseView {
                title: strip_links(title),
                lines: vec![],
            });
            continue;
        }
        let Some(current) = out.last_mut() else {
            continue;
        };
        if let Some(h) = line.strip_prefix("### ") {
            current.lines.push(ChangeLine {
                heading: true,
                pieces: vec![Piece::plain(h.trim())],
            });
        } else if let Some(item) = line.trim_start().strip_prefix("* ") {
            current.lines.push(ChangeLine {
                heading: false,
                pieces: pieces(&unlink(item)),
            });
        }
    }
    out
}

fn ago_words(secs: i64) -> String {
    if secs < 3600 {
        format!("{} min", (secs / 60).max(1))
    } else if secs < 86_400 {
        format!("{} h", secs / 3600)
    } else {
        format!("{} d, {} h", secs / 86_400, (secs % 86_400) / 3600)
    }
}

pub async fn checks(st: &AppState) -> anyhow::Result<Vec<Check>> {
    let cfg = st.ctx.config();
    let hw = st.ctx.hw.read().await.clone();
    let mut out = Vec::new();
    match &hw.ffmpeg_version {
        Some(v) => out.push(Check {
            name: "ffmpeg",
            value: v.clone(),
            class: "st-ok",
        }),
        None => out.push(Check {
            name: "ffmpeg",
            value: "not found; nothing can be encoded".into(),
            class: "st-err",
        }),
    }
    let notes = if hw.notes.is_empty() {
        String::new()
    } else {
        format!("; {}", hw.notes.join("; "))
    };
    out.push(match hw.backend {
        reel_media::HwBackend::Software => Check {
            name: "hardware",
            value: format!("none, every re-encode on the CPU{notes}"),
            class: "st-neutral",
        },
        b => Check {
            name: "hardware",
            value: format!("{} on {}{notes}", b.name(), hw.device),
            class: "st-ok",
        },
    });
    out.push(Check {
        name: "decode",
        value: hw.decoders_line(),
        class: "",
    });
    out.push(Check {
        name: "animated",
        value: hw.anim_line(),
        class: "",
    });
    let standalone = cfg.standalone();
    if !standalone {
        let waiting = st
            .ctx
            .store
            .ingest_files()
            .await?
            .iter()
            .filter(|f| f.state == "seen")
            .count();
        out.push(if cfg.ingest.incoming_dir.is_dir() {
            Check {
                name: "incoming",
                value: format!(
                    "readable, {waiting} file{} waiting",
                    if waiting == 1 { "" } else { "s" }
                ),
                class: "st-ok",
            }
        } else {
            Check {
                name: "incoming",
                value: "missing; nothing is picked up".into(),
                class: "st-err",
            }
        });
    }
    let disk = st.disk.get(&st.ctx.effective(), false).await;
    out.push(match st.ctx.library.away() {
        None if standalone => Check {
            name: "library",
            value: format!(
                "on this PC, in {}{}",
                cfg.library_dir().display(),
                disk.library_share
                    .map(|k| format!(", on a network share ({k})"))
                    .unwrap_or_default()
            ),
            class: "st-ok",
        },
        None => Check {
            name: "library",
            value: format!(
                "{}{}",
                if cfg.library_elsewhere() {
                    "in its own folder, marked as this reel's"
                } else {
                    "in the data folder, marked as this reel's"
                },
                disk.library_share
                    .map(|k| format!(", on a network share ({k})"))
                    .unwrap_or_default()
            ),
            class: "st-ok",
        },
        Some(why) => Check {
            name: "library",
            value: format!("{why}; nothing is taken in or moved until it is back"),
            class: "st-err",
        },
    });
    out.push(match disk.originals {
        Originals::Kept(b) => Check {
            name: "originals",
            value: format!("kept, {}", format::size(b)),
            class: "st-ok",
        },
        Originals::Off => Check {
            name: "originals",
            value: "not kept (Keep originals: none)".into(),
            class: "st-ok",
        },
        Originals::NoneYet => Check {
            name: "originals",
            value: "none kept yet; the folder is made on first use".into(),
            class: "st-ok",
        },
        Originals::Unreachable => Check {
            name: "originals",
            value: "the Originals folder is not reachable".into(),
            class: "st-warn",
        },
    });
    if cfg.external_on() {
        let class = st
            .ctx
            .external
            .report
            .lock()
            .unwrap()
            .as_ref()
            .map(|r| r.class())
            .unwrap_or("");
        out.push(Check {
            name: "link storage",
            value: reel_pipeline::external::usage_line(&st.ctx),
            class,
        });
    }
    let low = disk.low(cfg.storage.disk_warn_gb);
    out.push(Check {
        name: "disk",
        value: format!(
            "{}; the library on disk {}, caches {} of {} GB, trash {}{}",
            disk.disks_line(),
            format::size(disk.library),
            format::size(disk.cache),
            cfg.share.discord_cache_gb,
            format::size(disk.trash),
            if low.is_empty() {
                String::new()
            } else {
                format!("; under the Disk warning: {}", low.join(", "))
            }
        ),
        class: if low.is_empty() { "" } else { "st-warn" },
    });
    let db_ok = st.ctx.store.settings().await.is_ok();
    let db_size = std::fs::metadata(cfg.db_path())
        .map(|m| format::size(m.len()))
        .unwrap_or_else(|_| "size unknown".into());
    out.push(Check {
        name: "database",
        value: if db_ok {
            format!("ok, {db_size}")
        } else {
            "not answering".into()
        },
        class: if db_ok { "st-ok" } else { "st-err" },
    });
    let (queued, running, failed) = st.ctx.store.job_counts_full().await?;
    out.push(Check {
        name: "jobs",
        value: format!("{running} running, {queued} queued, {failed} failed"),
        class: if failed > 0 { "st-warn" } else { "" },
    });
    if standalone {
        return Ok(out);
    }
    let healthy = db_ok && hw.ffmpeg_version.is_some() && st.ctx.library.ready();
    out.push(Check {
        name: "health",
        value: if healthy {
            "/healthz answers ok".into()
        } else {
            "/healthz answers not ok".into()
        },
        class: if healthy { "st-ok" } else { "st-err" },
    });
    out.push(Check {
        name: "access",
        value: format!(
            "{} mode, public URL {}",
            reel_core::settings::get(&cfg, "auth.mode"),
            cfg.public_url
        ),
        class: "",
    });
    use reel_pipeline::app_update::{installer_name, offer, Flavour};
    let mut have = Vec::new();
    let mut missing = Vec::new();
    for f in Flavour::ALL {
        let name = installer_name(reel_core::VERSION, f);
        if offer(&cfg, f).await.is_some() {
            have.push(name);
        } else {
            missing.push(name);
        }
    }
    out.push(Check {
        name: "app update",
        value: match (have.is_empty(), cfg.device.app_updates) {
            (_, false) => "off".into(),
            (false, true) if missing.is_empty() => format!(
                "reel {}, signed, for older paired apps and the downloads under Devices: {}",
                reel_core::VERSION,
                have.join(", ")
            ),
            (false, true) => format!(
                "reel {}, signed, for older paired apps and the downloads under Devices: {}; not yet {}",
                reel_core::VERSION,
                have.join(", "),
                missing.join(", ")
            ),
            (true, true) => format!(
                "none yet: fetched from the release once it has signed installers, or put into app-updates in the data folder, each with its .sig: {}",
                missing.join(", ")
            ),
        },
        class: if !have.is_empty() && cfg.device.app_updates {
            "st-ok"
        } else {
            ""
        },
    });
    Ok(out)
}

/// One word for the header's dot and the words for its tooltip: err when a check fails, warn
/// when a check warns or a newer release is out, ok otherwise.
pub async fn level(st: &AppState) -> (&'static str, String) {
    let checks = match checks(st).await {
        Ok(c) => c,
        Err(e) => return ("err", format!("Status: the checks could not run: {e}")),
    };
    let failing: Vec<&str> = checks
        .iter()
        .filter(|c| c.class == "st-err")
        .map(|c| c.name)
        .collect();
    let cfg = st.ctx.config();
    if cfg.standalone() {
        let at = format!("the library is in {}", cfg.library_dir().display());
        let warning: Vec<&str> = checks
            .iter()
            .filter(|c| c.class == "st-warn")
            .map(|c| c.name)
            .collect();
        return if !failing.is_empty() {
            (
                "err",
                format!("a check fails: {}; {at}", failing.join(", ")),
            )
        } else if !warning.is_empty() {
            (
                "warn",
                format!("something to look at: {}; {at}", warning.join(", ")),
            )
        } else {
            ("ok", format!("reel runs here, all is well; {at}"))
        };
    }
    if !failing.is_empty() {
        return (
            "err",
            format!("Status: a check fails: {}", failing.join(", ")),
        );
    }
    let mut warning: Vec<&str> = checks
        .iter()
        .filter(|c| c.class == "st-warn")
        .map(|c| c.name)
        .collect();
    if st.ctx.updates.read().await.newer().is_some() {
        warning.push("an update is out");
    }
    if !warning.is_empty() {
        return (
            "warn",
            format!("Status: something to look at: {}", warning.join(", ")),
        );
    }
    ("ok", "Status: all is well".into())
}

pub async fn page(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Response> {
    let cfg = st.ctx.config();
    let (count, bytes) = st.ctx.store.clip_count_and_size().await?;
    let uptime = jiff::Timestamp::now().as_second() - st.ctx.started_at.as_second();
    let updates = st.ctx.updates.read().await.clone();
    let (update_line, update_class, update_url) =
        match (&updates.newer(), &updates.latest, &updates.error) {
            (Some(r), _, _) => (
                format!(
                    "{} is out{}; you run {}",
                    r.version,
                    r.published_at
                        .as_str()
                        .get(..10)
                        .map(|d| format!(" since {d}"))
                        .unwrap_or_default(),
                    reel_core::VERSION
                ),
                "st-warn",
                Some(r.url.clone()),
            ),
            (None, Some(r), _) => (
                format!("you run the newest, {}", r.version),
                "st-ok",
                Some(r.url.clone()),
            ),
            (None, None, Some(e)) => (format!("could not check: {e}"), "st-neutral", None),
            (None, None, None) if cfg.advanced.update_check => (
                "not checked yet; the first check runs a minute after start".into(),
                "",
                None,
            ),
            _ => ("checking is off (Settings, Advanced)".into(), "", None),
        };
    let checked = updates
        .checked_at
        .as_deref()
        .map(|t| format!(", checked {}", format::relative(t)))
        .unwrap_or_default();
    Ok(html(StatusPage {
        who: Who::at(&st, &me).await,
        nav: "status",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        version: reel_core::VERSION,
        built: format!(
            "{} from {}, {}",
            reel_core::BUILD_DATE,
            reel_core::GIT_SHA,
            reel_core::RUSTC
        ),
        running: format!(
            "since {} ({}), {count} clip{}, {}",
            format::when(&st.ctx.started_at.to_string()),
            ago_words(uptime),
            if count == 1 { "" } else { "s" },
            format::size(bytes as u64)
        ),
        library: format!("{count} clips, {}", format::size(bytes as u64)),
        newest: updates
            .latest
            .as_ref()
            .map(|r| r.version.clone())
            .or_else(reel_pipeline::app_update::newest)
            .unwrap_or_else(|| "not known yet".into()),
        update_line: format!("{update_line}{checked}"),
        update_class,
        update_url,
        update_on: cfg.advanced.update_check,
        checks: checks(&st).await?,
        releases: parse_changelog(reel_core::CHANGELOG, 5),
        repo: cfg.advanced.update_repo.clone(),
        obs_version: bundled_obs().0,
        obs_package: reel_core::packages::windows(true).name,
        obs_source: bundled_obs().1,
    }))
}

pub async fn doctor(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let cfg = st.ctx.config();
    let status = reel_media::hw::detect(&st.ctx.ff, &cfg.media).await;
    let json = status.to_json();
    *st.ctx.hw.write().await = status;
    let _ = st.disk.get(&st.ctx.effective(), true).await;
    Ok(Json(json!({ "ok": true, "hardware": json })))
}

pub async fn update_check(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let state = reel_pipeline::updates::check_once(&st.ctx).await;
    Ok(Json(json!({
        "ok": state.error.is_none(),
        "error": state.error,
        "latest": state.latest.as_ref().map(|r| json!({ "version": r.version, "url": r.url, "published_at": r.published_at })),
        "newer": state.newer().map(|r| r.version.clone()),
        "running": reel_core::VERSION,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_please_changelog_renders_as_releases() {
        let text = "# Changelog\n\n## [0.3.0](https://github.com/x/reel/compare/v0.2.0...v0.3.0) (2026-10-14)\n\n### Features\n\n* **jobs:** the queue page ([a1b2c3d4e](https://github.com/x/reel/commit/a1b2c3d4e))\n* settings in the UI\n\n### Bug Fixes\n\n* **send:** the link, not the file\n\n## 0.2.0 (2026-10-08)\n\n### Features\n\n* phones\n";
        let r = parse_changelog(text, 5);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].title, "0.3.0 (2026-10-14)");
        let words = |l: &ChangeLine| l.pieces.iter().map(|p| p.text.as_str()).collect::<String>();
        assert_eq!(words(&r[0].lines[0]), "Features");
        assert!(r[0].lines[0].heading);
        assert_eq!(words(&r[0].lines[1]), "jobs: the queue page");
        assert_eq!(words(&r[0].lines[3]), "Bug Fixes");
        assert_eq!(r[1].title, "0.2.0 (2026-10-08)");
        assert_eq!(parse_changelog(text, 1).len(), 1);
        assert!(parse_changelog("# Changelog\n\nnothing yet\n", 5).is_empty());
    }

    #[test]
    fn a_changelog_line_shows_its_code_and_scope_and_no_marks() {
        let code = |t: &str| Piece {
            text: t.into(),
            code: true,
            bold: false,
        };
        let bold = |t: &str| Piece {
            text: t.into(),
            code: false,
            bold: true,
        };
        assert_eq!(
            pieces("`c` and Send to an app take the clip's newest live link"),
            vec![
                code("c"),
                Piece::plain(" and Send to an app take the clip's newest live link")
            ]
        );
        assert_eq!(
            pieces("the server image's clock is UTC unless you set `TZ`"),
            vec![
                Piece::plain("the server image's clock is UTC unless you set "),
                code("TZ")
            ]
        );
        assert_eq!(
            pieces("**desktop:** the app keeps `**`"),
            vec![
                bold("desktop:"),
                Piece::plain(" the app keeps "),
                code("**")
            ]
        );
        assert_eq!(
            pieces("a lone ` and a lone ** stay"),
            vec![Piece::plain("a lone ` and a lone ** stay")]
        );
        let text = "# Changelog\n\n## [1.11.1](https://github.com/x/reel/compare/v1.11.0...v1.11.1) (2026-11-04)\n\n### Bug Fixes\n\n* **desktop:** `reel-desktop --save-and-share` copies the link ([#53](https://github.com/x/reel/issues/53)) ([86408f8](https://github.com/x/reel/commit/86408f8))\n";
        let r = parse_changelog(text, 5);
        assert_eq!(r[0].title, "1.11.1 (2026-11-04)");
        assert_eq!(
            r[0].lines[1].pieces,
            vec![
                bold("desktop:"),
                Piece::plain(" "),
                code("reel-desktop --save-and-share"),
                Piece::plain(" copies the link (#53)")
            ]
        );
    }
}
