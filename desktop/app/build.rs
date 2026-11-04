//! Copies reel's stylesheet, fonts and shared scripts from web/assets into the app's pages.
use std::path::Path;

fn copy_assets() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let web = root.join("../../web/assets");
    let out = root.join("../ui/assets");
    std::fs::create_dir_all(out.join("fonts"))?;
    std::fs::copy(web.join("reel.css"), out.join("reel.css"))?;
    std::fs::create_dir_all(out.join("js"))?;
    for js in ["switch.js", "walk.js"] {
        std::fs::copy(web.join("js").join(js), out.join("js").join(js))?;
        println!(
            "cargo:rerun-if-changed={}",
            web.join("js").join(js).display()
        );
    }
    for entry in std::fs::read_dir(web.join("fonts"))? {
        let entry = entry?;
        std::fs::copy(entry.path(), out.join("fonts").join(entry.file_name()))?;
    }
    println!("cargo:rerun-if-changed={}", web.join("reel.css").display());
    println!("cargo:rerun-if-changed={}", web.join("fonts").display());
    Ok(())
}

/// Every command, so each gets its own permission (granted in capabilities/default.json).
const COMMANDS: &[&str] = &[
    "status_get",
    "settings_get",
    "standalone_cap",
    "settings_save",
    "hotkey_try",
    "hotkeys_listen",
    "pair",
    "unpair",
    "obs_test",
    "cue_preview",
    "pick_folder",
    "pick_wav",
    "log_tail",
    "open_log_folder",
    "share_last",
    "save_clip",
    "save_and_share",
    "copy_link",
    "open_clip",
    "open_library",
    "open_page",
    "view_show",
    "view_get",
    "obs_state_get",
    "obs_apply",
    "upload_local",
    "open_folder",
    "pause_set",
    "upload_now",
    "obs_show",
    "obs_done",
    "obs_copy_again",
    "obs_copy_remove",
    "obs_audio",
    "audio_programs",
    "screens_get",
    "audio_devices",
    "obs_own_mic",
    "obs_mic_filters",
    "obs_volume",
    "system_memory",
    "obs_video",
    "encoders_test",
    "obs_items",
    "obs_read",
    "obs_input_set",
    "obs_meters",
    "quit",
    "update_check",
    "update_install",
    "open_releases",
    "uninstall",
    "app_status",
    "app_share_last",
    "app_save_clip",
    "app_save_and_share",
    "app_pause_set",
    "app_open_settings",
    "app_download_open",
    "offline_retry",
    "page_shape_get",
    "page_section",
];

fn main() {
    if let Err(e) = copy_assets() {
        println!("cargo:warning=reel.css was not copied into the window: {e}");
    }
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build");
}
