//! The Tauri shell: tray, window, plugins and hotkeys; the logic lives in reel-desktop-core.
mod commands;
mod downloads;
mod encoders;
mod hotkey;
mod library;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod media;
mod notify;
mod obsbox;
mod page_errors;
#[cfg(target_os = "linux")]
mod portal;
mod runtime;
mod setup;
mod shell;
mod standalone;
mod startup;
mod status;
mod tray;
mod updates;

use reel_desktop_core::links;
use runtime::Runtime;
use tauri::{Manager, RunEvent, WindowEvent};

/// The flags a desktop's own shortcut passes to the running app, as the hotkeys do.
pub const ACTIONS: [&str; 7] = [
    "--save-and-share",
    "--share-last",
    "--upload-last",
    "--save",
    "--pause-recording",
    "--resume-recording",
    "--start-obs",
];

fn action(args: &[String]) -> Option<&'static str> {
    ACTIONS.into_iter().find(|f| args.iter().any(|a| a == f))
}

static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn start_step(what: &str) {
    let at = STARTED.get().map(|t| t.elapsed()).unwrap_or_default();
    tracing::info!(at = %updates::secs(at), "start: {what}");
}

pub fn run() {
    let _ = STARTED.set(std::time::Instant::now());
    let logs = reel_desktop_core::settings::paths().logs_dir();
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        reel_desktop_core::log::panicked(&logs, info);
        before(info);
    }));
    #[cfg(target_os = "linux")]
    linux::webkit_workarounds();
    let args: Vec<String> = std::env::args().collect();
    // a start for one of the app's links (a notification's button) does only that
    let first_link = links::from_args(&args);
    let linked = first_link != links::Asked::Nothing;
    let first_action = action(&args).filter(|_| !linked);
    let hidden = args.iter().any(|a| a == "--hidden") || first_action.is_some() || linked;
    let webview_audio = reel_desktop_core::settings::WebviewAudio::from_args(&args);
    let builder =
        tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            tracing::info!(?args, "a second start handed its arguments over");
            if updates::leaving() {
                updates::start_after(args);
                return;
            }
            match links::from_args(&args) {
                links::Asked::Link(link) => {
                    app.state::<Runtime>().run_link(link);
                    return;
                }
                links::Asked::Refused(arg) => {
                    tracing::info!(%arg, "a reel-desktop link that is not one of the app's: ignored");
                    return;
                }
                links::Asked::Nothing => {}
            }
            if args.iter().any(|a| a == "--quit-for-install") {
                updates::quit_for_install(&app.state::<Runtime>());
                return;
            }
            if let Some(flag) = action(&args) {
                app.state::<Runtime>().run_action(flag);
                return;
            }
            let section = args
                .iter()
                .find_map(|a| a.strip_prefix("--section=").map(|s| s.to_string()));
            let page = args
                .iter()
                .find_map(|a| a.strip_prefix("--page=").map(|s| s.to_string()));
            let rt = app.state::<Runtime>();
            let applied = rt.apply_args(&args);
            if section.is_some() {
                shell::show(app, shell::View::Settings, section.as_deref());
            } else if let Some(p) = page {
                if let Err(e) = rt.open_library(&p) {
                    tracing::warn!(error = %e, "the page asked for from the command line");
                }
            } else if !applied {
                shell::show(app, shell::View::Library, None);
            }
        }));
    // X11's key grabs need an X display; a Wayland desktop without one has the portal alone.
    let builder = if hotkey::grabs() {
        builder.plugin(tauri_plugin_global_shortcut::Builder::new().build())
    } else {
        builder
    };
    let app = builder
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::status_get,
            commands::settings_get,
            commands::standalone_cap,
            commands::library_get,
            commands::library_plan,
            commands::library_move,
            commands::library_copy_plan,
            commands::library_copy,
            commands::library_stop,
            commands::settings_save,
            commands::hotkey_try,
            commands::hotkeys_listen,
            commands::pair,
            commands::unpair,
            commands::obs_test,
            commands::cue_preview,
            commands::pick_folder,
            commands::pick_wav,
            commands::log_tail,
            commands::open_log_folder,
            commands::share_last,
            commands::save_clip,
            commands::save_and_share,
            commands::copy_link,
            commands::open_clip,
            commands::open_library,
            commands::open_page,
            commands::app_status,
            commands::app_page_error,
            commands::app_share_last,
            commands::app_save_clip,
            commands::app_save_and_share,
            commands::app_pause_set,
            commands::app_recording_pause_set,
            commands::app_open_settings,
            commands::app_download_open,
            commands::view_show,
            commands::view_get,
            commands::offline_retry,
            commands::page_shape_get,
            commands::page_section,
            commands::obs_state_get,
            commands::obs_apply,
            commands::upload_local,
            commands::open_folder,
            commands::pause_set,
            commands::recording_pause_set,
            commands::upload_now,
            commands::obs_show,
            commands::obs_again,
            commands::obs_done,
            commands::obs_read,
            commands::obs_copy_again,
            commands::obs_copy_remove,
            commands::obs_audio,
            commands::audio_programs,
            commands::screens_get,
            commands::audio_devices,
            commands::obs_own_mic,
            commands::obs_mic_filters,
            commands::obs_volume,
            commands::system_memory,
            commands::obs_suggested_quality,
            commands::obs_video,
            commands::encoders_test,
            commands::encoders_seen,
            commands::obs_items,
            commands::obs_input_set,
            commands::obs_meters,
            commands::quit,
            commands::uninstall,
            commands::update_check,
            commands::update_install,
            commands::open_releases,
        ])
        .setup(move |app| {
            let kind = reel_desktop_core::settings::Settings::load(
                &reel_desktop_core::settings::paths().settings_file(),
            )
            .map(|s| s.tray_icon)
            .unwrap_or_default();
            let icon = tray::icon(app.handle(), kind)?;
            let first = tray::menu(app.handle(), &status::Status::default())?;
            tauri::tray::TrayIconBuilder::with_id(tray::TRAY_ID)
                .icon(icon)
                .tooltip("reel")
                .menu(&first)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| {
                    let rt = app.state::<Runtime>();
                    match e.id.as_ref() {
                        "settings" => shell::show(app, shell::View::Settings, None),
                        "quit" => updates::quit(&rt, "the tray's Quit"),
                        "update" => {
                            let rt = rt.inner().clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = updates::install_now(&rt).await {
                                    notify::show(&rt.app, "The update did not install", &e);
                                }
                            });
                        }
                        "share" => rt.share_last(None),
                        "obs-show" => rt.show_bundled_and_read(true),
                        "obs-again" => rt.start_obs_again(),
                        "obs-hide" => rt.show_bundled_and_read(false),
                        "save" => rt.save_clip(),
                        "save-share" => rt.save_and_share(),
                        "pause" => {
                            let now = rt.lock().paused_by_user;
                            rt.set_paused(!now);
                        }
                        "recording" => {
                            let now = rt.lock().recording_paused;
                            rt.set_recording_paused(!now);
                        }
                        "library" => shell::show(app, shell::View::Library, None),
                        other => {
                            if let Some(id) = other.strip_prefix("copy:") {
                                rt.share_last(Some(id.to_string()));
                            } else if let Some(id) = other.strip_prefix("open:") {
                                if let Err(e) = rt.open_library(&format!("/clips/{id}")) {
                                    notify::show(app, "The clip did not open", &e);
                                }
                            }
                        }
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                        shell::show(tray.app_handle(), shell::View::Library, None);
                    }
                })
                .build(app)?;
            let rt = Runtime::start(app.handle().clone())?;
            start_step("the tray icon and the app are up");
            // before the first window, which reads it
            if let Some(audio) = webview_audio {
                rt.apply_args_now(&[format!("--webview-audio={}", audio.word())]);
            }
            #[cfg(windows)]
            shell::audio::clear_left_over(app.handle());
            #[cfg(unix)]
            {
                let on_term = rt.clone();
                tauri::async_runtime::spawn(async move {
                    use tokio::signal::unix::{signal, SignalKind};
                    if let Ok(mut term) = signal(SignalKind::terminate()) {
                        term.recv().await;
                        tracing::info!("asked to stop (SIGTERM): quitting as the tray's Quit does");
                        updates::quit(&on_term, "SIGTERM");
                    }
                });
            }
            let (standalone, fresh, server_url) = {
                let g = rt.lock();
                (
                    g.settings.mode == reel_desktop_core::settings::Mode::Standalone,
                    g.fresh,
                    g.settings.server_url.clone(),
                )
            };
            let first_run = fresh || (!standalone && server_url.is_empty());
            let wizard = {
                let g = rt.lock();
                g.bundled.is_some() && !g.settings.obs.setup.done && !g.settings.obs.setup_later
            };
            #[cfg(target_os = "linux")]
            linux::watch_tray(rt.clone());
            #[cfg(target_os = "linux")]
            media::watch(rt.clone());
            if let Some(flag) = first_action {
                rt.run_action_when_ready(flag);
            }
            match first_link {
                links::Asked::Link(link) => rt.run_link_when_ready(link),
                links::Asked::Refused(arg) => {
                    tracing::info!(%arg, "a reel-desktop link that is not one of the app's: ignored")
                }
                links::Asked::Nothing => {}
            }
            app.manage(rt);
            if !hidden || wizard || first_run {
                let base = if standalone { "" } else { server_url.as_str() };
                shell::build(app.handle(), base)?;
                if !base.is_empty() {
                    app.state::<Runtime>().inner().go_library("/");
                }
            }
            if wizard {
                shell::show(app.handle(), shell::View::Setup, None);
            } else if first_run && standalone {
                shell::show(app.handle(), shell::View::Settings, Some("obs"));
            } else if first_run {
                shell::show(app.handle(), shell::View::Settings, Some("server"));
            } else if !hidden {
                shell::show(app.handle(), shell::View::Library, None);
            }
            start_step(if app.get_window(shell::MAIN).is_some() {
                "the window is made"
            } else {
                "no window: started hidden"
            });
            app.state::<Runtime>().start_obs();
            start_step("OBS starts behind the window");
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Destroyed if window.label() == shell::MAIN && !updates::leaving() => {
                tracing::info!(
                    "the window is closed and its webview gone; the app goes on in the tray"
                );
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                shell::layout(window);
            }
            WindowEvent::Focused(focused) => {
                hotkey::settings_shortcut(window.app_handle(), *focused);
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("the reel desktop shell");
    app.run(|_app, event| {
        if let RunEvent::ExitRequested {
            api, code: None, ..
        } = event
        {
            api.prevent_exit();
        }
    });
}
