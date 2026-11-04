use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// The commit's date as given (an image's build has no `.git`), else git's, else "unknown".
pub fn build_date(given: Option<String>, git: impl FnOnce() -> Option<String>) -> String {
    let is_date = |s: &str| {
        s.len() == 10
            && s.char_indices().all(|(i, c)| match i {
                4 | 7 => c == '-',
                _ => c.is_ascii_digit(),
            })
    };
    given
        .map(|s| s.trim().to_string())
        .filter(|s| is_date(s))
        .or_else(git)
        .unwrap_or_else(|| "unknown".into())
}

fn main() {
    let sha = std::env::var("REEL_GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| git(&["rev-parse", "--short=10", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    let date = build_date(std::env::var("REEL_BUILD_DATE").ok(), || {
        git(&["log", "-1", "--format=%cs"])
    });
    let rustc = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "rustc".into());
    println!(
        "cargo:rustc-env=REEL_GIT_SHA={}",
        sha.chars().take(10).collect::<String>()
    );
    println!("cargo:rustc-env=REEL_BUILD_DATE={date}");
    println!("cargo:rustc-env=REEL_RUSTC={rustc}");
    println!("cargo:rerun-if-env-changed=REEL_GIT_SHA");
    println!("cargo:rerun-if-env-changed=REEL_BUILD_DATE");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../CHANGELOG.md");
}
