//! Settings docked along the window's left edge, in a webview of its own beside the page the
//! library webview shows, on a window 1600 px wide or more; on a narrower one Settings is the
//! full page, as before. Made when it first opens, kept while the window lives.
use crate::shell::{self, View};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, Url, Webview};

pub const PANEL: &str = "settings";

/// From under the header of reel's pages to above the app's bar.
const TOP: f64 = 48.0;
const BAR: f64 = 30.0;

/// The window widths where the panel takes its two sizes; under `NARROW` there is none.
const WIDE: f64 = 2200.0;
const NARROW: f64 = 1600.0;
/// Nav and column as on the full page; under 960 px the nav is the select at the top.
const FULL: f64 = 980.0;
const SMALL: f64 = 760.0;

static OPEN: AtomicBool = AtomicBool::new(false);
static FITS: AtomicBool = AtomicBool::new(false);
static LIBRARY_ON_SERVER: AtomicBool = AtomicBool::new(false);
static MAKING: AtomicBool = AtomicBool::new(false);
/// The width the page beside was last given, 0 while the panel is closed.
static SHOWN: AtomicU32 = AtomicU32::new(0);
/// What the panel was asked to show while it was being made.
static WANTED: Mutex<Option<To>> = Mutex::new(None);
/// The page beside, kept while a narrowed window shows the panel's page as the full page, for
/// the panel to come back beside it when the window is wide again.
static MOVED: Mutex<Option<Url>> = Mutex::new(None);
#[cfg(target_os = "linux")]
static PX: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(FULL as i32);

/// What the panel shows: a page of the app's Settings, or one of the server's.
#[derive(Debug, Clone, PartialEq)]
pub enum To {
    App(Option<String>),
    Page(Url),
}

/// The panel's width on a window this wide, if it has one.
pub fn width_for(window: f64) -> Option<f64> {
    if window >= WIDE {
        Some(FULL)
    } else if window >= NARROW {
        Some(SMALL)
    } else {
        None
    }
}

fn settings_path(p: &str) -> bool {
    p == "/settings" || p.starts_with("/settings/")
}

/// Whether a page of the server is one of Settings', or reel on this PC's way in to one.
pub fn is_settings(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    settings_path(url.path())
        || (url.path() == "/app/open"
            && url
                .query_pairs()
                .any(|(k, v)| k == "to" && settings_path(v.split(['?', '#']).next().unwrap_or(""))))
}

/// Whether the panel keeps a navigation: the app's own page, Settings' pages and the sign-in on
/// the way; any other page of the server opens beside it.
pub fn keeps(url: &Url) -> bool {
    shell::is_local(url)
        || !matches!(url.scheme(), "http" | "https")
        || is_settings(url)
        || url.path() == "/signin"
}

/// Whether Settings opens in the panel now: the window wide enough, the library on a page of the
/// server.
pub fn docks(app: &AppHandle) -> bool {
    let Some(window) = app.get_window(shell::MAIN) else {
        return false;
    };
    let on_server = app
        .get_webview(shell::LIBRARY)
        .and_then(|w| w.url().ok())
        .is_some_and(|u| !shell::shows_app_page(Some(&u)));
    width_for(shell::logical_size(&window).0).is_some() && on_server
}

pub fn is_open() -> bool {
    OPEN.load(Ordering::SeqCst)
}

/// The library goes to `url`: a page of Settings goes to the panel instead while it docks. From
/// the library's navigation hook, which can ask the webviews nothing.
pub fn library_goes(app: &AppHandle, url: &Url) -> bool {
    let server = !shell::is_local(url) && matches!(url.scheme(), "http" | "https");
    if server
        && is_settings(url)
        && FITS.load(Ordering::SeqCst)
        && LIBRARY_ON_SERVER.load(Ordering::SeqCst)
    {
        let (app, url) = (app.clone(), url.clone());
        tauri::async_runtime::spawn(async move { open(&app, To::Page(url)) });
        return false;
    }
    LIBRARY_ON_SERVER.store(server, Ordering::SeqCst);
    let settings =
        is_settings(url) || (shell::is_local(url) && shell::page_of(url) == Some(View::Settings));
    if !settings {
        *lock(&MOVED) = None;
    }
    true
}

/// A page of the library loaded: it moves right of the open panel, if one is.
pub fn library_loaded(webview: &Webview) {
    let w = SHOWN.load(Ordering::SeqCst);
    let _ = webview.eval(format!("window.__reelPanel && window.__reelPanel({w})"));
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Opens the panel on a page, making it first when the window has none yet.
pub fn open(app: &AppHandle, to: To) {
    if let Some(p) = app.get_webview(PANEL) {
        go(&p, to);
        show(app, &p);
        return;
    }
    *lock(&WANTED) = Some(to);
    if MAKING.swap(true, Ordering::SeqCst) {
        return;
    }
    // a webview made inside a synchronous command deadlocks on Windows
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let made = make(&app);
        MAKING.store(false, Ordering::SeqCst);
        let to = lock(&WANTED).take();
        match (made, to) {
            (Ok(p), Some(to)) => {
                go(&p, to);
                show(&app, &p);
            }
            (Ok(_), None) => {}
            (Err(e), _) => tracing::warn!(error = %e, "the settings panel could not be made"),
        }
    });
}

/// The panel's close button, Esc, and Settings in the header while it is open; `from` names
/// the webview that asked, for the log.
pub fn close(app: &AppHandle, from: &str) {
    if !OPEN.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Some(p) = app.get_webview(PANEL) {
        put(&p, false);
    }
    if let Some(w) = app.get_window(shell::MAIN) {
        shell::layout(&w);
    }
    if let Some(l) = app.get_webview(shell::LIBRARY) {
        let _ = l.set_focus();
    }
    tracing::info!(from, "settings panel closes");
}

/// The window was closed: a new one starts without a panel.
pub fn gone() {
    OPEN.store(false, Ordering::SeqCst);
    *lock(&MOVED) = None;
}

fn go(p: &Webview, to: To) {
    match to {
        To::Page(url) => {
            let _ = p.navigate(url);
        }
        To::App(section) => {
            let on = p.url().ok();
            if on.as_ref().and_then(shell::page_of) == Some(View::Settings) {
                if let Some(s) = section {
                    let _ = p.emit_to(EventTarget::labeled(PANEL), "open-section", s);
                }
                return;
            }
            // a page of the server's Settings stays when no page is asked for
            let on_server = on.is_some_and(|u| matches!(u.scheme(), "http" | "https"));
            if section.is_some() || !on_server {
                let page = match section {
                    Some(s) => format!("{}#{s}", shell::SETTINGS_PAGE),
                    None => shell::SETTINGS_PAGE.to_string(),
                };
                let _ = p.navigate(shell::local_url(&page));
            }
        }
    }
}

fn show(app: &AppHandle, p: &Webview) {
    let was = OPEN.swap(true, Ordering::SeqCst);
    if let Some(w) = app.get_window(shell::MAIN) {
        shell::layout(&w);
    }
    put(p, true);
    focus(p);
    if !was {
        tracing::info!("settings panel opens");
    }
}

/// The keys go to the panel's page once it is open, so Esc closes it without a click first.
fn focus(p: &Webview) {
    let _ = p.set_focus();
    // the overlay places it on its next allocation: the focus is taken again after that
    #[cfg(target_os = "linux")]
    let _ = p.with_webview(|pv| {
        use gtk::prelude::*;
        let view = pv.inner();
        webkit2gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || {
            view.grab_focus();
        });
    });
    let _ = p.eval("window.focus()");
}

/// Shows or hides the panel. On Linux a hidden overlay child can leave its last picture on the
/// screen when it is hidden during a resize: the overlay moves it out of the window instead.
fn put(p: &Webview, open: bool) {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;
        let _ = open;
        let _ = p.with_webview(|pv| pv.inner().queue_resize());
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = if open { p.show() } else { p.hide() };
    }
}

/// After a resize: the panel's size and place, the page beside it, and Settings moved between
/// the panel and the full page when the window crosses 1600 px.
pub fn layout(window: &tauri::Window, w: f64, h: f64) {
    let width = width_for(w);
    FITS.store(width.is_some(), Ordering::SeqCst);
    let app = window.app_handle();
    let panel = app.get_webview(PANEL);
    let library = app.get_webview(shell::LIBRARY);
    if let (Some(p), Some(l)) = (&panel, &library) {
        if is_open() && width.is_none() {
            narrowed(p, l);
        } else if !is_open() && width.is_some() {
            widened(p, l);
        }
    }
    let shown = if is_open() { width.unwrap_or(0.0) } else { 0.0 };
    SHOWN.store(shown as u32, Ordering::SeqCst);
    if let Some(p) = &panel {
        if shown > 0.0 {
            let _ = p.set_position(LogicalPosition::new(0.0, TOP));
            let _ = p.set_size(LogicalSize::new(shown, (h - TOP - BAR).max(100.0)));
        }
        // closed, it keeps its width: its page does not reflow out of sight
        #[cfg(target_os = "linux")]
        if shown > 0.0 && PX.swap(shown as i32, Ordering::SeqCst) != shown as i32 {
            put(p, true);
        }
    }
    if let Some(l) = &library {
        let _ = l.eval(format!("window.__reelPanel && window.__reelPanel({shown})"));
    }
}

fn narrowed(p: &Webview, l: &Webview) {
    OPEN.store(false, Ordering::SeqCst);
    put(p, false);
    if let (Ok(page), Ok(before)) = (p.url(), l.url()) {
        if matches!(page.scheme(), "http" | "https") || shell::is_local(&page) {
            *lock(&MOVED) = Some(before);
            let _ = l.navigate(page);
        }
    }
    tracing::info!("settings panel closes: the window is too narrow, its page is the full page");
}

fn widened(p: &Webview, l: &Webview) {
    let Some(before) = lock(&MOVED).take() else {
        return;
    };
    let Ok(page) = l.url() else {
        return;
    };
    let settings = is_settings(&page) || shell::page_of(&page) == Some(View::Settings);
    if !settings {
        return;
    }
    let _ = p.navigate(page);
    let _ = l.navigate(before);
    OPEN.store(true, Ordering::SeqCst);
    put(p, true);
    focus(p);
    tracing::info!("settings panel opens: the window is wide again, its page goes back into it");
}

/// Where the open panel is, for the window after an update: the app's section, or the server's
/// path.
pub fn place(app: &AppHandle) -> Option<String> {
    if !is_open() {
        return None;
    }
    let url = app.get_webview(PANEL)?.url().ok()?;
    if shell::is_local(&url) {
        let section = url
            .fragment()
            .filter(|s| shell::section_ok(s))
            .unwrap_or("first");
        return Some(format!("app:{section}"));
    }
    is_settings(&url).then(|| match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_string(),
    })
}

/// The panel again where `place` said it was.
pub fn reopen(app: &AppHandle, at: &str) {
    let to = match at.strip_prefix("app:") {
        Some(s) => To::App(Some(s.to_string()).filter(|s| shell::section_ok(s))),
        None if settings_path(at.split('?').next().unwrap_or("")) => {
            let Some(rt) = app.try_state::<crate::runtime::Runtime>() else {
                return;
            };
            match rt.library_page(at).parse() {
                Ok(u) => To::Page(u),
                Err(_) => return,
            }
        }
        None => return,
    };
    let fits = app
        .get_window(shell::MAIN)
        .is_some_and(|w| width_for(shell::logical_size(&w).0).is_some());
    match to {
        to if fits => open(app, to),
        // a window too narrow for the panel shows its page as the full page
        To::App(section) => shell::show(app, View::Settings, section.as_deref()),
        To::Page(url) => {
            if let Some(l) = app.get_webview(shell::LIBRARY) {
                let _ = l.navigate(url);
            }
        }
    }
}

fn make(app: &AppHandle) -> tauri::Result<Webview> {
    let window = app
        .get_window(shell::MAIN)
        .ok_or(tauri::Error::WindowNotFound)?;
    let handle = app.clone();
    let builder = WebviewBuilder::new(
        PANEL,
        tauri::WebviewUrl::External("about:blank".parse().expect("blank url")),
    )
    .background_color(shell::BACKGROUND)
    .initialization_script(shell::first_script())
    .initialization_script(script(true))
    .initialization_script(shell::server_nav())
    .on_navigation(move |url| {
        tracing::info!(to = %shell::where_to(url), "settings panel goes to");
        if keeps(url) {
            return true;
        }
        let (app, url) = (handle.clone(), url.clone());
        tauri::async_runtime::spawn(async move {
            if let Some(l) = app.get_webview(shell::LIBRARY) {
                let _ = l.navigate(url);
            }
        });
        false
    })
    .on_page_load(|_, payload| {
        if let PageLoadEvent::Finished = payload.event() {
            tracing::info!(at = %shell::where_to(payload.url()), "settings panel loaded");
        }
    })
    .on_download(|_, _| true);
    let p = shell::add_beside(app, &window, builder)?;
    #[cfg(windows)]
    crate::downloads::attach(&p);
    #[cfg(target_os = "linux")]
    overlay(&p);
    #[cfg(not(target_os = "linux"))]
    let _ = p.hide();
    tracing::info!("settings panel made");
    Ok(p)
}

/// The script the app runs on every page of both webviews for the panel.
pub fn script(panel: bool) -> String {
    format!(
        "(() => {{ const PANEL = {panel};\n{}\n}})();",
        include_str!("panel.js")
    )
}

/// WebKitGTK's child webviews share one GtkBox, which stacks them and ignores their bounds: the
/// panel goes into an overlay over the library instead, placed by the overlay itself.
#[cfg(target_os = "linux")]
fn overlay(panel: &Webview) {
    use gtk::prelude::*;
    let _ = panel.with_webview(move |pv| {
        let view: gtk::Widget = pv.inner().upcast();
        let Some(parent) = view.parent().and_then(|p| p.downcast::<gtk::Box>().ok()) else {
            tracing::warn!("settings panel: no box to take it from");
            return;
        };
        let Some(library) = parent.children().into_iter().find(|c| c != &view) else {
            tracing::warn!("settings panel: no library beside it");
            return;
        };
        let overlay = gtk::Overlay::new();
        parent.remove(&library);
        parent.remove(&view);
        overlay.add(&library);
        overlay.add_overlay(&view);
        let mine = view.clone();
        overlay.connect_get_child_position(move |ov, child| {
            if child != &mine {
                return None;
            }
            let w = PX.load(Ordering::SeqCst).max(1);
            let h = (ov.allocated_height() - TOP as i32 - BAR as i32).max(100);
            // closed, it waits out of the window
            let x = if is_open() { 0 } else { -w - 100 };
            Some(gtk::gdk::Rectangle::new(x, TOP as i32, w, h))
        });
        parent.pack_start(&overlay, true, true, 0);
        view.show();
        overlay.show();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        s.parse().unwrap()
    }

    #[test]
    fn the_panel_takes_two_widths_and_none_under_1600() {
        assert_eq!(width_for(2556.0), Some(980.0));
        assert_eq!(width_for(2200.0), Some(980.0));
        assert_eq!(width_for(2199.0), Some(760.0));
        assert_eq!(width_for(1920.0), Some(760.0));
        assert_eq!(width_for(1600.0), Some(760.0));
        assert_eq!(width_for(1599.0), None);
        assert_eq!(width_for(1280.0), None);
    }

    #[test]
    fn settings_pages_and_the_way_in_to_them_go_to_the_panel() {
        for s in [
            "http://nas:8746/settings",
            "http://nas:8746/settings/storage",
            "https://clips.example.com/settings/advanced?x=1",
            "http://127.0.0.1:34567/app/open?ticket=t&to=%2Fsettings%2Fstorage",
            "http://127.0.0.1:34567/app/open?ticket=t&to=/settings",
        ] {
            assert!(is_settings(&url(s)), "{s}");
        }
        for s in [
            "http://nas:8746/",
            "http://nas:8746/settingsx",
            "http://nas:8746/clips/01H/settings",
            "http://nas:8746/jobs",
            "http://127.0.0.1:34567/app/open?ticket=t&to=/",
            "http://127.0.0.1:34567/app/open?ticket=t&to=/settingsx",
            "tauri://localhost/index.html#clips",
            "about:blank",
        ] {
            assert!(!is_settings(&url(s)), "{s}");
        }
    }

    #[test]
    fn the_panel_keeps_settings_and_sends_the_rest_beside_it() {
        for s in [
            "tauri://localhost/index.html#clips",
            "http://tauri.localhost/index.html#first",
            "http://nas:8746/settings/ingest",
            "http://nas:8746/signin",
            "about:blank",
        ] {
            assert!(keeps(&url(s)), "{s}");
        }
        for s in [
            "http://nas:8746/",
            "http://nas:8746/trash",
            "http://nas:8746/status",
            "http://nas:8746/api/logs/download",
            "http://nas:8746/setup",
        ] {
            assert!(!keeps(&url(s)), "{s}");
        }
    }
}
