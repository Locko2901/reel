//! Video and sound in the window on Linux. A WebKitGTK web process missing a GStreamer element
//! it counts on (autoaudiosink) hangs for good, so media is turned off when that may happen.
use crate::runtime::Runtime;
use crate::shell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Manager, Webview};
use webkit2gtk::{SettingsExt, WebViewExt};

/// The GStreamer 1.x plugins a clip's playback goes through.
const NEEDED: &[&str] = &[
    "libgstcoreelements.so",
    "libgstplayback.so",
    "libgsttypefindfunctions.so",
    "libgstapp.so",
    "libgstaudioconvert.so",
    "libgstaudioresample.so",
    "libgstautodetect.so",
    "libgstisomp4.so",
];

/// How long a web process may go without answering, after WebKit's own three seconds.
const GRACE: Duration = Duration::from_secs(4);

static OFF: AtomicBool = AtomicBool::new(false);

/// Where GStreamer looks for plugins; the AppImage's AppRun sets the system path to its own.
fn plugin_dirs() -> Vec<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    let mut dirs = Vec::new();
    if let Some(v) = var("GST_PLUGIN_PATH_1_0").or_else(|| var("GST_PLUGIN_PATH")) {
        dirs.extend(std::env::split_paths(&v));
    }
    match var("GST_PLUGIN_SYSTEM_PATH_1_0").or_else(|| var("GST_PLUGIN_SYSTEM_PATH")) {
        Some(v) => dirs.extend(std::env::split_paths(&v)),
        None => {
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(Path::new(&home).join(".local/share/gstreamer-1.0/plugins"));
            }
            let arch = std::env::consts::ARCH;
            dirs.push(PathBuf::from(format!(
                "/usr/lib/{arch}-linux-gnu/gstreamer-1.0"
            )));
            dirs.push(PathBuf::from("/usr/lib64/gstreamer-1.0"));
            dirs.push(PathBuf::from("/usr/lib/gstreamer-1.0"));
        }
    }
    dirs.retain(|d| !d.as_os_str().is_empty());
    dirs
}

pub fn missing_in(dirs: &[PathBuf]) -> Vec<&'static str> {
    NEEDED
        .iter()
        .copied()
        .filter(|f| !dirs.iter().any(|d| d.join(f).is_file()))
        .collect()
}

fn media_on() -> bool {
    static CHECKED: OnceLock<bool> = OnceLock::new();
    let on = *CHECKED.get_or_init(|| {
        let dirs = plugin_dirs();
        let missing = missing_in(&dirs);
        if missing.is_empty() {
            return true;
        }
        let looked: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
        tracing::warn!(
            missing = %missing.join(", "),
            looked = %looked.join(":"),
            "video and sound in the window are off: GStreamer lacks plugins the clips need"
        );
        false
    });
    on && !OFF.load(Ordering::Relaxed)
}

fn turn_off(view: &webkit2gtk::WebView) {
    if let Some(s) = WebViewExt::settings(view) {
        s.set_enable_media(false);
        s.set_enable_mediasource(false);
    }
}

/// Turns media off on a new webview when it cannot play, and watches its web process.
pub fn attach(webview: &Webview) {
    let on = media_on();
    let app = webview.app_handle().clone();
    let _ = webview.with_webview(move |pv| {
        let view = pv.inner();
        if !on {
            turn_off(&view);
        }
        view.connect_is_web_process_responsive_notify(move |view| {
            let answers = view.is_web_process_responsive();
            tracing::debug!(answers, "WebKit on the page's web process");
            if answers {
                return;
            }
            let (view, app) = (view.clone(), app.clone());
            webkit2gtk::glib::timeout_add_local_once(GRACE, move || {
                if !view.is_web_process_responsive() {
                    stalled(&view, &app, "WebKit says its web process does not answer");
                }
            });
        });
    });
}

fn stalled(view: &webkit2gtk::WebView, app: &AppHandle, why: &str) {
    tracing::warn!(
        why,
        "the page stopped answering; its web process is ended and the page loads again without video and sound"
    );
    OFF.store(true, Ordering::Relaxed);
    turn_off(view);
    view.terminate_web_process();
    if let Some(rt) = app.try_state::<Runtime>() {
        rt.reload_page();
    }
}

/// The page calls the app every two seconds while it shows; one silent this long has hung,
/// whatever WebKit says.
const SILENT: Duration = Duration::from_secs(10);

/// Reloads the server's page without media once it stops calling the app.
pub fn watch(rt: Runtime) {
    tauri::async_runtime::spawn(async move {
        let tick = Duration::from_secs(2);
        let mut last = std::time::Instant::now();
        loop {
            tokio::time::sleep(tick).await;
            let late = last.elapsed() > tick * 3;
            last = std::time::Instant::now();
            if late || shell::current(&rt.app) != shell::View::Library {
                continue;
            }
            let Some(window) = rt.app.get_window(shell::MAIN) else {
                continue;
            };
            if !window.is_visible().unwrap_or(false) || window.is_minimized().unwrap_or(true) {
                continue;
            }
            if !rt.page_silent(SILENT) {
                continue;
            }
            if let Some(wv) = rt.app.get_webview(shell::LIBRARY) {
                let app = rt.app.clone();
                let _ = wv.with_webview(move |pv| {
                    stalled(
                        &pv.inner(),
                        &app,
                        "the page has not called the app for ten seconds",
                    )
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_missing_from_every_folder_is_named() {
        let root = std::env::temp_dir().join(format!("reel-media-{}", std::process::id()));
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        for (i, f) in NEEDED.iter().enumerate() {
            if *f != "libgstautodetect.so" {
                std::fs::write(if i % 2 == 0 { &a } else { &b }.join(f), b"").unwrap();
            }
        }
        let dirs = vec![a.clone(), b.clone()];
        assert_eq!(missing_in(&dirs), vec!["libgstautodetect.so"]);
        std::fs::write(b.join("libgstautodetect.so"), b"").unwrap();
        assert!(missing_in(&dirs).is_empty());
        assert_eq!(missing_in(&[root.join("none")]).len(), NEEDED.len());
        let _ = std::fs::remove_dir_all(&root);
    }
}
