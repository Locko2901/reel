//! Builds in the licence page `ci/licence-page.sh` writes, named by `REEL_LICENCES`; a build without
//! one builds in nothing, and `/licences` says so.
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=REEL_LICENCES");
    let page = match std::env::var("REEL_LICENCES")
        .ok()
        .filter(|p| !p.is_empty())
    {
        Some(path) => {
            println!("cargo:rerun-if-changed={path}");
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("REEL_LICENCES={path}: {e}"))
        }
        None => String::new(),
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    std::fs::write(out.join("licences.html"), page).expect("OUT_DIR is writable");
}
