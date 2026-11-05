use reel_core::config::Config;
use reel_core::settings::{self, Def, Kind, Section, DEFS};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Variables outside the catalogue that a user may set.
const OTHER: &[(&str, &str, &str)] = &[
    (
        "REEL_CONFIG",
        "The server",
        "The path of `config.toml`, the file layer under the environment. The image sets `/data/config.toml`; without it reel reads no file.",
    ),
    (
        "REEL_OIDC_CLIENT_SECRET",
        "The server",
        "The identity provider's client secret, with `auth.mode` set to `oidc`. Never stored in the database or shown under Settings.",
    ),
    (
        "REEL_OIDC_CLIENT_SECRET_FILE",
        "The server",
        "A file holding that secret instead, such as a Docker secret under `/run/secrets/`.",
    ),
    (
        "REEL_PASSWORD",
        "The server",
        "Read by `reel passwd <name>` only: the new password of that user, twelve characters at least. `docker exec -e REEL_PASSWORD='...' reel reel passwd --make-admin admin` is the way back in when the admin is locked out.",
    ),
    (
        "REEL_DESKTOP_DIR",
        "The desktop app",
        "Puts the app's settings (under `config`) and its data, the logs and a standalone library included (under `data`), in this one folder instead of the user's own folders.",
    ),
];

/// A line under a setting that points to the page about changing it.
const SEE: &[(&str, &str)] = &[(
    "auth.mode",
    "Changing it on a server with users: [Switching the sign-in method](switching-sign-in.md)",
)];

const NOT_FOR_USERS: &[&str] = &[
    "REEL_GIT_SHA",
    "REEL_BUILD_DATE",
    "REEL_RUSTC",
    "REEL_ASSET_MARK",
    "REEL_DESKTOP_BUILD",
    "REEL_DESKTOP_SILENT",
    "REEL_OBS_EXE",
    "REEL_GITHUB_API",
    "REEL_TEST_S3",
    "REEL_TEST_S3_ENDPOINT",
    "REEL_DEMO_STANDALONE",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sentence(text: &str) -> String {
    let mut c = text.chars();
    let mut out: String = match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    };
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

fn options(key: &str, values: &[&str]) -> String {
    values
        .iter()
        .map(|v| {
            let word = settings::option_word(key, v);
            if word == *v {
                format!("`{v}`")
            } else {
                format!("`{v}` ({word})")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn accepts(d: &Def) -> String {
    match d.kind {
        Kind::Int { min, max, unit: "" } => {
            format!("a whole number, {min} to {max}")
        }
        Kind::Int { min, max, unit } => format!("a whole number, {min} to {max} {unit}"),
        Kind::Bool => "`true` or `false`".into(),
        Kind::Text => "text".into(),
        Kind::Url if d.key.starts_with("notify.") => {
            "a URL, https (plain http only for a host on the private network)".into()
        }
        Kind::Url => "a URL starting with http:// or https://".into(),
        Kind::Path => "a path".into(),
        Kind::List => {
            "a list: comma separated in the environment, an array in `config.toml`".into()
        }
        Kind::Secret => "a secret, never shown again once saved".into(),
        Kind::Select(values) => format!("one of {}", options(d.key, values)),
        Kind::Set(values) => format!("any of {}, comma separated", options(d.key, values)),
    }
}

fn default_of(d: &Def) -> String {
    let value = settings::get(&Config::default(), d.key);
    if value.is_empty() {
        return "empty".into();
    }
    match d.kind {
        Kind::Select(_) => {
            let word = settings::option_word(d.key, &value);
            if word == value {
                format!("`{value}`")
            } else {
                format!("`{value}` ({word})")
            }
        }
        _ => format!("`{value}`"),
    }
}

fn toml_place(key: &str) -> String {
    match key.rsplit_once('.') {
        Some((table, name)) => format!("`{name}` under `[{table}]`"),
        None => format!("`{key}`, at the top"),
    }
}

fn image_env() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(root().join("Dockerfile")).unwrap_or_default();
    let re = regex::Regex::new(r"(REEL_[A-Z0-9_]+)=(\S+)").unwrap();
    text.lines()
        .filter(|l| !l.trim_start().starts_with("ARG") && !l.contains("=$"))
        .flat_map(|l| {
            re.captures_iter(l)
                .map(|c| {
                    (
                        c[1].to_string(),
                        c[2].trim_end_matches('\\').trim().to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn render() -> String {
    let image = image_env();
    let mut p = String::new();
    p.push_str("<!-- Made by `make docs-config` from the settings catalogue; edit the catalogue, not this page. -->\n\n");
    p.push_str("# Configuration\n\n");
    p.push_str("Every setting of the server, where it is under Settings, its key in `config.toml`, its environment variable and what it accepts.\n\n");
    p.push_str("A value comes from four layers, each over the one before: reel's default, the value saved under Settings, `config.toml`, then the environment. ");
    p.push_str("A setting given in `config.toml` or the environment shows read only under Settings, with where it came from. ");
    p.push_str("A change under Settings takes effect as it is saved, except where a setting says it waits for the next start.\n\n");
    p.push_str("In the compose file the environment goes under `environment:`, as `REEL_PUBLIC_URL: https://clips.example.com`. ");
    p.push_str(
        "`config.toml` lives where `REEL_CONFIG` says, `/data/config.toml` in the image:\n\n",
    );
    p.push_str("```toml\npublic_url = \"https://clips.example.com\"\n\n[ingest]\npoll_seconds = 5\nextensions = [\"mkv\", \"mp4\"]\n```\n\n");
    for section in Section::ALL {
        let defs: Vec<&Def> = DEFS.iter().filter(|d| d.section == section).collect();
        let _ = writeln!(p, "## {}\n\n{}\n", section.label(), section.line());
        for d in defs {
            let _ = writeln!(p, "### {}\n\n{}\n", d.label, sentence(d.help));
            let _ = writeln!(p, "- Under Settings: {}, {}", section.label(), d.panel);
            if let Some(parent) = d.depends.and_then(settings::find) {
                let _ = writeln!(
                    p,
                    "- Shows while [{}](#{}) is on; its value stays when that is off",
                    parent.label,
                    slug(parent.label)
                );
            }
            let _ = writeln!(p, "- In `config.toml`: {}", toml_place(d.key));
            let _ = writeln!(p, "- Environment: `{}`", d.env);
            let _ = writeln!(p, "- Accepts: {}", accepts(d));
            if !matches!(d.kind, Kind::Secret) {
                let _ = writeln!(p, "- Default: {}", default_of(d));
            }
            if let Some((_, v)) = image.iter().find(|(k, _)| k == d.env) {
                let _ = writeln!(p, "- In the image: `{v}`, from its environment");
            }
            if d.fixed {
                let _ = writeln!(
                    p,
                    "- Read when the process starts, from `config.toml` or the environment only"
                );
            } else if d.restart {
                let _ = writeln!(p, "- Takes effect at the next start");
            }
            if let Some((_, line)) = SEE.iter().find(|(key, _)| *key == d.key) {
                let _ = writeln!(p, "- {line}");
            }
            p.push('\n');
        }
    }
    p.push_str("## Other variables\n\n");
    p.push_str("| Variable | For | What it does |\n|---|---|---|\n");
    for (name, what, help) in OTHER {
        let _ = writeln!(p, "| `{name}` | {what} | {help} |");
    }
    p.push('\n');
    p.push_str("`RUST_LOG`, when set, wins over [Log level](#log-level). `TZ` sets the container's clock; [Time zone](#time-zone) is what the pages follow.\n");
    p
}

fn slug(label: &str) -> String {
    label
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

#[test]
fn the_configuration_page_is_the_catalogue() {
    let page = render();
    let path = root().join("docs/configuration.md");
    if std::env::var_os("UPDATE_DOCS").is_some() {
        std::fs::write(&path, &page).expect("write docs/configuration.md");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        on_disk == page,
        "docs/configuration.md is not what the settings catalogue makes: run make docs-config"
    );
}

#[test]
fn every_setting_has_its_own_heading() {
    let mut seen = BTreeSet::new();
    for d in DEFS {
        assert!(
            seen.insert(slug(d.label)),
            "two settings share the heading {}",
            d.label
        );
        assert!(
            !d.help.contains(['\u{2013}', '\u{2014}']),
            "{} has a dash",
            d.key
        );
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn every_variable_reel_reads_is_on_the_page() {
    let mut files = Vec::new();
    for dir in ["crates", "desktop/app/src", "desktop/core/src"] {
        rust_files(&root().join(dir), &mut files);
    }
    files.retain(|f| f.components().any(|c| c.as_os_str() == "src"));
    assert!(files.len() > 20, "the sources were not found");
    let re = regex::Regex::new(r"(?:^|[^A-Za-z0-9_])(REEL_[A-Z0-9_]*[A-Z0-9])").unwrap();
    let page = render();
    let mut missing = BTreeSet::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        for c in re.captures_iter(&text) {
            let name = &c[1];
            if !NOT_FOR_USERS.contains(&name) && !page.contains(&format!("`{name}`")) {
                missing.insert(name.to_string());
            }
        }
    }
    assert!(
        missing.is_empty(),
        "read by reel but not on docs/configuration.md: {missing:?}"
    );
}
