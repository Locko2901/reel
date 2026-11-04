pub mod auth;
pub mod config;
pub mod events;
pub mod format;
pub mod games;
pub mod ids;
pub mod names;
pub mod packages;
pub mod plan;
pub mod probe;
pub mod proc;
pub mod recipe;
pub mod settings;
pub mod versions;
pub mod visitors;

pub use config::Config;
pub use events::{Bus, Event};
pub use probe::ProbeInfo;
pub use recipe::CutRecipe;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_SHA: &str = env!("REEL_GIT_SHA");
pub const BUILD_DATE: &str = env!("REEL_BUILD_DATE");
pub const RUSTC: &str = env!("REEL_RUSTC");
pub const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");
