//! What the reel desktop app does without a window; nothing here needs Windows to be tested.
pub mod api;
pub mod audio;
pub mod bundled;
pub mod cache;
pub mod cues;
pub mod elevated;
pub mod gaming;
pub mod hash;
pub mod install;
pub mod lane;
pub mod ledger;
pub mod links;
pub mod log;
pub mod machine;
pub mod obs;
pub mod obsproc;
pub mod readback;
pub mod screens;
pub mod secrets;
pub mod settings;
pub mod shape;
pub mod uninstall;
pub mod update_steps;
pub mod uploader;
pub mod watcher;
pub mod webview;

#[cfg(test)]
pub(crate) mod mock;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// A dev build's mark (`dev.<commit>`); unset on a release.
pub const BUILD: Option<&str> = option_env!("REEL_DESKTOP_BUILD");

/// The version as the Status page shows it.
pub fn version_line() -> String {
    match BUILD {
        Some(b) if !b.is_empty() => format!("{VERSION} ({b})"),
        _ => VERSION.to_string(),
    }
}

pub fn user_agent() -> String {
    format!("reel-desktop/{VERSION}")
}
