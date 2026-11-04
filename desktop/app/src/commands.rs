use crate::runtime::Runtime;
use crate::status::Status;
use reel_desktop_core::cues::Cue;
use reel_desktop_core::settings::Settings;
use serde_json::{json, Value};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

type R<T> = Result<T, String>;

#[tauri::command]
pub fn status_get(rt: State<'_, Runtime>) -> Status {
    rt.status()
}

#[tauri::command]
pub fn settings_get(rt: State<'_, Runtime>) -> Settings {
    rt.settings()
}

/// What the standalone library's cap would move to the trash at `gb`, before it is set.
#[tauri::command]
pub async fn standalone_cap(rt: State<'_, Runtime>, gb: u64) -> R<Value> {
    Ok(json!({ "line": rt.cap_line(gb).await }))
}

/// The library folder: where it is, its size, its drive's room.
#[tauri::command]
pub async fn library_get(rt: State<'_, Runtime>) -> R<Value> {
    Ok(rt.library_info().await)
}

/// What a picked folder would be for the library: a move into it, or the use of the library in
/// it; or why not.
#[tauri::command]
pub async fn library_plan(rt: State<'_, Runtime>, picked: String) -> R<crate::library::Target> {
    rt.library_plan(picked.into()).await
}

/// Moves the library into the picked folder, or uses the library in it; the status follows it.
#[tauri::command]
pub fn library_move(rt: State<'_, Runtime>, picked: String) -> R<()> {
    rt.library_move(picked.into())
}

#[tauri::command]
pub async fn library_copy_plan(
    rt: State<'_, Runtime>,
    into: String,
) -> R<crate::library::CopyPlan> {
    rt.library_copy_plan(into.into()).await
}

/// Makes a copy of the library in a new folder in the picked one; the status follows it.
#[tauri::command]
pub fn library_copy(rt: State<'_, Runtime>, into: String) -> R<()> {
    rt.library_copy(into.into())
}

/// Stops a copy, or a move before it switched; both roll back.
#[tauri::command]
pub fn library_stop(rt: State<'_, Runtime>) {
    rt.library_stop()
}

/// Saves the settings; a changed server URL is refused when nothing answers there, unless
/// `force` (the page's "Save anyway").
#[tauri::command]
pub async fn settings_save(
    rt: State<'_, Runtime>,
    settings: Settings,
    base: Option<Settings>,
    force: Option<bool>,
) -> R<Settings> {
    let old = rt.settings().server_url;
    let changed = settings.server_url.trim() != old && !settings.server_url.trim().is_empty();
    if changed && !force.unwrap_or(false) {
        if let Err(why) = crate::runtime::probe_server(settings.server_url.trim()).await {
            return Err(format!(
                "{} {} ({}) Save anyway?",
                why.title, why.words, why.code
            ));
        }
    }
    rt.save_settings(settings, base)
}

#[tauri::command]
pub async fn hotkey_try(rt: State<'_, Runtime>, key: String, combo: String) -> R<String> {
    rt.hotkey_try(&key, &combo)
}

#[tauri::command]
pub async fn hotkeys_listen(rt: State<'_, Runtime>, on: bool) -> R<()> {
    rt.hotkeys_listen(on);
    Ok(())
}

#[tauri::command]
pub async fn pair(
    rt: State<'_, Runtime>,
    server_url: String,
    code: String,
    name: String,
) -> R<Status> {
    rt.pair(&server_url, &code, &name).await?;
    Ok(rt.status())
}

#[tauri::command]
pub fn unpair(rt: State<'_, Runtime>) -> Status {
    rt.unpair();
    rt.status()
}

#[tauri::command]
pub async fn obs_test(host: String, port: u16, password: String) -> R<Value> {
    match Runtime::obs_test(&host, port, &password).await {
        Ok(line) => Ok(json!({ "ok": true, "line": line })),
        Err((line, detail)) => Ok(json!({ "ok": false, "line": line, "detail": detail })),
    }
}

#[tauri::command]
pub fn cue_preview(rt: State<'_, Runtime>, cue: String) -> R<Value> {
    let cue = Cue::parse(&cue).ok_or_else(|| format!("{cue} is not a cue"))?;
    let g = rt.lock();
    let s = match cue {
        Cue::Captured => &g.settings.sounds.captured,
        Cue::Uploaded => &g.settings.sounds.uploaded,
        Cue::LinkCopied => &g.settings.sounds.link_copied,
    };
    g.player.preview(cue, s).map_err(|e| e.to_string())?;
    Ok(json!({ "line": format!("played through {}", g.player.backend().name()) }))
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle, current: Option<String>) -> R<Option<String>> {
    let mut d = app.dialog().file();
    if let Some(c) = current.filter(|c| std::path::Path::new(c).is_dir()) {
        d = d.set_directory(c);
    }
    let picked = tauri::async_runtime::spawn_blocking(move || d.blocking_pick_folder())
        .await
        .map_err(|e| e.to_string())?;
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn pick_wav(app: AppHandle) -> R<Option<String>> {
    let d = app.dialog().file().add_filter("WAV sound", &["wav"]);
    let picked = tauri::async_runtime::spawn_blocking(move || d.blocking_pick_file())
        .await
        .map_err(|e| e.to_string())?;
    Ok(picked.map(|p| p.to_string()))
}

#[tauri::command]
pub fn log_tail(rt: State<'_, Runtime>, lines: Option<usize>) -> Vec<String> {
    let g = rt.lock();
    g.log
        .as_ref()
        .map(|l| l.ring.lines(lines.unwrap_or(200)))
        .unwrap_or_default()
}

#[tauri::command]
pub fn open_log_folder(app: AppHandle, rt: State<'_, Runtime>) -> R<()> {
    let dir = rt.lock().paths.logs_dir();
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

fn not_in_standalone(rt: &Runtime, what: &str) -> R<()> {
    if rt.is_standalone() {
        Err(format!("{what} is not part of standalone: reel keeps the clips in the library on this PC and makes no links"))
    } else {
        Ok(())
    }
}

#[tauri::command]
pub fn share_last(rt: State<'_, Runtime>) -> R<Value> {
    not_in_standalone(&rt, "Share the last clip")?;
    rt.share_last(None);
    Ok(json!({ "line": "asking the server for the link" }))
}

#[tauri::command]
pub fn save_clip(rt: State<'_, Runtime>) -> R<Value> {
    rt.save_clip();
    Ok(json!({ "line": if rt.is_standalone() {
        "OBS saves the replay; it goes into the library on this PC"
    } else {
        "OBS saves the replay; it goes up as any clip does"
    } }))
}

#[tauri::command]
pub fn save_and_share(rt: State<'_, Runtime>) -> R<Value> {
    not_in_standalone(&rt, "Save and share")?;
    rt.save_and_share();
    Ok(json!({ "line": "OBS saves the replay; the link follows once it is up" }))
}

#[tauri::command]
pub fn copy_link(rt: State<'_, Runtime>, clip_id: String) -> R<Value> {
    not_in_standalone(&rt, "Copy link")?;
    rt.share_last(Some(clip_id));
    Ok(json!({ "line": "asking the server for the link; it lands on the clipboard" }))
}

#[tauri::command]
pub async fn open_clip(rt: State<'_, Runtime>, clip_id: String) -> R<()> {
    rt.open_library(&format!("/clips/{clip_id}"))
}

#[tauri::command]
pub async fn open_library(rt: State<'_, Runtime>) -> R<()> {
    rt.open_library("/")
}

#[tauri::command]
pub async fn obs_state_get(rt: State<'_, Runtime>, refresh: bool) -> R<Value> {
    let st = rt.obs_state(refresh).await?;
    Ok(json!({ "connected": st.is_some(), "state": st }))
}

#[tauri::command]
pub async fn obs_apply(
    rt: State<'_, Runtime>,
    change: reel_desktop_core::obs::ObsChange,
) -> R<Value> {
    let lines = rt.obs_apply(change).await?;
    Ok(json!({ "lines": lines }))
}

#[tauri::command]
pub async fn open_page(rt: State<'_, Runtime>, path: String) -> R<()> {
    let path = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };
    rt.open_library(&path)
}

#[tauri::command]
pub fn app_status(rt: State<'_, Runtime>) -> R<Value> {
    Ok(rt.app_status())
}

#[tauri::command]
pub fn app_share_last(rt: State<'_, Runtime>) -> R<()> {
    rt.share_last(None);
    Ok(())
}

#[tauri::command]
pub fn app_save_clip(rt: State<'_, Runtime>) -> R<()> {
    rt.save_clip();
    Ok(())
}

#[tauri::command]
pub fn app_save_and_share(rt: State<'_, Runtime>) -> R<()> {
    rt.save_and_share();
    Ok(())
}

#[tauri::command]
pub fn app_pause_set(rt: State<'_, Runtime>, paused: bool) -> R<Value> {
    rt.set_paused(paused);
    Ok(rt.app_status())
}

/// Show in folder and Open on a download's toast: only a file this run downloaded, and Open
/// only for a video or a picture, so no page can start a program it downloaded.
#[tauri::command]
pub fn app_download_open(app: AppHandle, id: u64, reveal: bool) -> R<()> {
    let path = crate::downloads::saved(&app, id).ok_or("that download is not known here")?;
    if !reveal && !crate::downloads::openable(&path) {
        return Err("only a video or a picture opens from here; Show in folder has it".into());
    }
    if reveal {
        app.opener().reveal_item_in_dir(&path)
    } else {
        app.opener()
            .open_path(path.display().to_string(), None::<&str>)
    }
    .map_err(|e| e.to_string())?;
    tracing::info!(id, reveal, "a download shown from its toast");
    Ok(())
}

/// "This app" on a server's Settings page; the page hands over the shape of reel's Settings
/// pages, checked before it is kept.
#[tauri::command]
pub fn app_open_settings(
    app: AppHandle,
    rt: State<'_, Runtime>,
    section: Option<String>,
    shape: Option<Value>,
) -> R<()> {
    if let Some(s) = shape.and_then(reel_desktop_core::shape::Shape::checked) {
        rt.take_shape(s);
    }
    crate::shell::show(&app, crate::shell::View::Settings, section.as_deref());
    Ok(())
}

/// What the app's own Settings section draws its header and nav from.
#[tauri::command]
pub async fn page_shape_get(rt: State<'_, Runtime>) -> R<Value> {
    Ok(rt.page_shape().await)
}

/// Logs which section the app's page opened.
#[tauri::command]
pub fn page_section(section: String) -> R<()> {
    if crate::shell::section_ok(&section) {
        tracing::info!(section = %section, "app page");
    }
    Ok(())
}

#[tauri::command]
pub fn upload_local(rt: State<'_, Runtime>, clip_id: String) -> R<()> {
    rt.upload_local(&clip_id)
}

#[tauri::command]
pub fn view_show(app: AppHandle, view: String, section: Option<String>) -> R<()> {
    let v = crate::shell::View::parse(&view).ok_or_else(|| format!("{view} is not a view"))?;
    crate::shell::show(&app, v, section.as_deref());
    Ok(())
}

/// The fallback page's Retry: probe the server now and go back to its pages if it answers.
#[tauri::command]
pub async fn offline_retry(rt: State<'_, Runtime>) -> R<()> {
    rt.retry_now().await;
    Ok(())
}

#[tauri::command]
pub fn view_get(app: AppHandle) -> R<String> {
    Ok(crate::shell::current(&app).name().to_string())
}

#[tauri::command]
pub fn open_folder(app: AppHandle, path: String) -> R<()> {
    let p = std::path::PathBuf::from(&path);
    if p.is_dir() {
        app.opener()
            .open_path(p.display().to_string(), None::<&str>)
            .map_err(|e| e.to_string())
    } else if p.is_file() {
        app.opener()
            .reveal_item_in_dir(&p)
            .map_err(|e| e.to_string())
    } else {
        let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or(p);
        app.opener()
            .open_path(dir.display().to_string(), None::<&str>)
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn pause_set(rt: State<'_, Runtime>, paused: bool) -> Status {
    rt.set_paused(paused);
    rt.status()
}

/// The microphone with filters in the user's own OBS, read from its scene collection, never
/// written.
#[tauri::command]
pub async fn obs_own_mic(
    rt: State<'_, Runtime>,
) -> Result<Option<reel_desktop_core::bundled::OwnMic>, String> {
    let config = match rt.lock().bundled.as_ref() {
        Some(b) => b.own_config(),
        None => reel_desktop_core::bundled::own_obs_config(),
    };
    Ok(tauri::async_runtime::spawn_blocking(move || {
        config.and_then(|c| reel_desktop_core::bundled::own_mic(&c))
    })
    .await
    .ok()
    .flatten())
}

/// Sets the Microphone's filters in the running OBS from the setup's draft, before anything
/// is saved.
#[tauri::command]
pub async fn obs_mic_filters(
    rt: State<'_, Runtime>,
    setup: reel_desktop_core::bundled::ObsSetup,
) -> R<()> {
    rt.apply_mic_filters(&setup).await
}

/// A source's volume in the running OBS, for the setup's slider before anything is saved.
#[tauri::command]
pub async fn obs_volume(rt: State<'_, Runtime>, input: String, db: f64) -> R<()> {
    rt.obs_volume(&input, db).await
}

/// The PC's memory in MB.
#[tauri::command]
pub fn system_memory() -> Option<u64> {
    reel_desktop_core::machine::memory_mb()
}

/// The audio outputs and inputs as Windows lists them, by the ids OBS takes; None where there
/// is no such list.
#[tauri::command]
pub async fn audio_devices() -> Option<reel_desktop_core::audio::Devices> {
    tauri::async_runtime::spawn_blocking(reel_desktop_core::audio::devices)
        .await
        .ok()
        .flatten()
}

/// What a clip hears for a setup: the chosen sources with their tracks, and each in words.
#[derive(serde::Serialize)]
pub struct AudioPlan {
    pub sources: Vec<reel_desktop_core::bundled::AudioSource>,
    pub words: Vec<String>,
    /// Application audio capture is there (Windows): the game alone and Discord alone.
    pub apart: bool,
}

#[tauri::command]
pub fn obs_audio(rt: State<'_, Runtime>, setup: reel_desktop_core::bundled::ObsSetup) -> AudioPlan {
    use reel_desktop_core::bundled::Platform;
    let platform = rt
        .lock()
        .bundled
        .as_ref()
        .map(|b| b.platform)
        .unwrap_or(if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        });
    let apart = platform == Platform::Windows || rt.lock().obs_app_audio == Some(true);
    AudioPlan {
        sources: setup.audio(platform),
        words: setup.audio_words(platform),
        apart,
    }
}

/// The programs with a window now, to capture the game's audio alone.
#[tauri::command]
pub async fn audio_programs() -> Vec<reel_desktop_core::audio::Program> {
    tauri::async_runtime::spawn_blocking(reel_desktop_core::audio::programs)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn screens_get() -> Vec<reel_desktop_core::bundled::Screen> {
    reel_desktop_core::screens::screens()
}

/// What a setup comes to (output, frame rate, bitrate, the replay buffer's memory).
#[tauri::command]
pub fn obs_video(setup: reel_desktop_core::bundled::ObsSetup) -> reel_desktop_core::bundled::Video {
    setup.video()
}

#[tauri::command]
pub async fn encoders_test() -> Vec<crate::setup::EncoderTest> {
    crate::setup::test_encoders().await
}

#[tauri::command]
pub async fn obs_items(
    rt: State<'_, Runtime>,
    input: String,
    property: String,
) -> R<Vec<reel_desktop_core::obs::Item>> {
    rt.obs_items(input, property).await
}

#[tauri::command]
pub async fn obs_input_set(
    rt: State<'_, Runtime>,
    input: String,
    settings: serde_json::Value,
) -> R<()> {
    rt.obs_input_set(input, settings).await
}

#[tauri::command]
pub async fn obs_meters(rt: State<'_, Runtime>, on: bool) -> R<()> {
    rt.obs_meters(on).await
}

/// Shows or hides the OBS that came with the app.
#[tauri::command]
pub fn obs_show(rt: State<'_, Runtime>, shown: bool) -> Status {
    rt.show_bundled_and_read(shown);
    rt.status()
}

/// Makes the copy of OBS in Program Files again, after an update or when it is missing;
/// Windows asks once.
#[tauri::command]
pub async fn obs_copy_again(rt: State<'_, Runtime>) -> R<Status> {
    rt.copy_again()?;
    Ok(rt.status())
}

/// Removes a copy left in Program Files with the switch off; Windows asks once.
#[tauri::command]
pub async fn obs_copy_remove(rt: State<'_, Runtime>) -> R<Status> {
    rt.copy_remove()?;
    Ok(rt.status())
}

/// What the app took from OBS, after Hide OBS (`obs_done`) or a read alone (`obs_read`).
#[derive(serde::Serialize)]
pub struct Done {
    pub taken: Vec<String>,
    pub settings: reel_desktop_core::settings::Settings,
    pub status: Status,
}

#[tauri::command]
pub async fn obs_done(rt: State<'_, Runtime>) -> R<Done> {
    let taken = rt.obs_done().await;
    Ok(Done {
        taken,
        settings: rt.settings(),
        status: rt.status(),
    })
}

#[tauri::command]
pub async fn obs_read(rt: State<'_, Runtime>) -> R<Done> {
    let taken = rt.read_obs().await;
    Ok(Done {
        taken,
        settings: rt.settings(),
        status: rt.status(),
    })
}

#[tauri::command]
pub fn upload_now(rt: State<'_, Runtime>, path: String) -> Status {
    let p = std::path::PathBuf::from(path);
    rt.take_again(&p);
    rt.status()
}

#[tauri::command]
pub fn quit(rt: State<'_, Runtime>) {
    crate::updates::quit(&rt, "Quit in the app");
}

#[tauri::command]
pub fn uninstall(rt: State<'_, Runtime>) -> R<()> {
    crate::updates::uninstall(&rt)
}

#[tauri::command]
pub async fn update_check(rt: State<'_, Runtime>) -> R<Status> {
    crate::updates::check(&rt, true).await?;
    Ok(rt.status())
}

#[tauri::command]
pub async fn update_install(rt: State<'_, Runtime>) -> R<()> {
    crate::updates::install_now(&rt).await
}

#[tauri::command]
pub fn open_releases(app: AppHandle) -> R<()> {
    app.opener()
        .open_url(crate::updates::releases_url(), None::<&str>)
        .map_err(|e| e.to_string())
}
