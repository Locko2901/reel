//! One window with one webview for the server's pages and the app's own, so Back works across
//! both. A webview made inside a synchronous command deadlocks on Windows (a white window).
use crate::runtime::Runtime;
use std::collections::HashSet;
use std::sync::Mutex;
use tauri::ipc::CapabilityBuilder;
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, EventTarget, LogicalPosition, LogicalSize, Manager, Url, Webview,
    WebviewUrl, Window,
};

/// The origins the webview may call the app from, each granted once.
struct Granted(Mutex<HashSet<String>>);

pub const MAIN: &str = "main";
pub const LIBRARY: &str = "library";

/// The app's own pages, served from the binary on the app's origin.
pub const SETTINGS_PAGE: &str = "index.html";
pub const OFFLINE_PAGE: &str = "offline.html";
pub const SETUP_PAGE: &str = "setup.html";

/// reel's page background under every page, shown before a page has drawn; WebView2 would show
/// white there, between two pages too.
const BACKGROUND: tauri::window::Color = tauri::window::Color(0x0d, 0x0b, 0x10, 0xff);

/// The only commands a reel page inside the app may call.
const REMOTE_PERMISSIONS: [&str; 11] = [
    "allow-app-status",
    "allow-app-page-error",
    "allow-app-share-last",
    "allow-app-save-clip",
    "allow-app-save-and-share",
    "allow-app-pause-set",
    "allow-app-recording-pause-set",
    "allow-app-session-set",
    "allow-app-obs-again",
    "allow-app-open-settings",
    "allow-app-download-open",
];

/// What the library the app shows may ask besides, on this PC or on the server it is paired
/// with: the answer to the update's question.
const LIBRARY_PERMISSIONS: [&str; 1] = ["allow-app-update-answer"];

/// What the library on this PC may ask besides: its files open where they are.
const STANDALONE_PERMISSIONS: [&str; 1] = ["allow-app-library-open"];

/// What an origin's pages are granted: the remote commands, and the library's own once it is
/// the library the app shows now (`base`).
fn grants(origin: &str, standalone: bool, base: &str) -> Vec<&'static str> {
    let mut all = REMOTE_PERMISSIONS.to_vec();
    if origin_of(base).as_deref() == Some(origin) {
        all.extend(LIBRARY_PERMISSIONS);
        if standalone {
            all.extend(STANDALONE_PERMISSIONS);
        }
    }
    all
}

/// Whether a command comes from the library's own page: the library's webview, on the origin of
/// the library the app shows now. A grant is never taken back, so a command of the library's
/// asks this too.
pub fn library_page_of(label: &str, url: Option<&str>, base: &str) -> bool {
    let here = origin_of(base);
    label == LIBRARY && here.is_some() && url.and_then(origin_of) == here
}

/// Every page's uncaught errors, unhandled rejections and scripts that did not load go to the
/// app's Log; installed before the page's own scripts, so a page that never starts tells why.
/// The app's own pages are not granted the command.
const PAGE_ERRORS: &str = r#"(() => {
  if (!/^https?:$/.test(location.protocol) || location.hostname === 'tauri.localhost') return;
  let sent = 0, leaving = false;
  addEventListener('pagehide', () => { leaving = true; });
  addEventListener('pageshow', () => { leaving = false; });
  const send = (message, stack) => {
    const t = window.__TAURI__, call = t && t.core && t.core.invoke;
    if (leaving || !call || ++sent > 20) return;
    call('app_page_error', { message: String(message).slice(0, 1000), stack: String(stack || '').slice(0, 4000), path: location.pathname }).catch(() => {});
  };
  addEventListener('error', (e) => {
    if (e.target instanceof HTMLScriptElement) { send(`a script did not load: ${e.target.src}`); return; }
    if (e.target !== window) return;
    send(e.message || (e.error && e.error.message) || 'an error', e.error && e.error.stack);
  }, true);
  addEventListener('unhandledrejection', (e) => {
    const r = e.reason, words = String((r && r.message) || r);
    // the notification plugin asks for its permission on every page, not granted to these
    if (words.startsWith('notification.is_permission_granted not allowed')) return;
    send(`unhandled rejection: ${words}`, r && r.stack);
  });
})();"#;

/// What the window shows; Offline is the app's page for a server out of reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Library,
    Settings,
    Offline,
    /// The setup of the OBS that came with the app.
    Setup,
}

impl View {
    pub fn parse(s: &str) -> Option<View> {
        match s {
            "library" => Some(View::Library),
            "settings" => Some(View::Settings),
            "offline" => Some(View::Offline),
            "setup" => Some(View::Setup),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            View::Library => "library",
            View::Settings => "settings",
            View::Offline => "offline",
            View::Setup => "setup",
        }
    }
}

fn where_to(url: &Url) -> String {
    format!("{}{}", url.host_str().unwrap_or(""), url.path())
}

/// One of the app's own pages: Tauri serves them on `tauri://localhost`, on Windows on
/// `http://tauri.localhost`.
pub fn local_url(page: &str) -> Url {
    let base = if cfg!(windows) {
        "http://tauri.localhost/"
    } else {
        "tauri://localhost/"
    };
    format!("{base}{page}").parse().unwrap_or_else(|_| {
        format!("{base}{SETTINGS_PAGE}")
            .parse()
            .expect("the app's page")
    })
}

/// Whether a URL is one of the app's own pages rather than the server's.
pub fn is_local(url: &Url) -> bool {
    url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost")
}

/// Whether the webview shows the app's own page or none yet: WebView2 reads about:blank until
/// its first page commits.
pub fn shows_app_page(url: Option<&Url>) -> bool {
    url.is_none_or(|u| is_local(u) || !matches!(u.scheme(), "http" | "https"))
}

fn page_of(url: &Url) -> Option<View> {
    if !is_local(url) {
        return None;
    }
    Some(if url.path().ends_with(OFFLINE_PAGE) {
        View::Offline
    } else if url.path().ends_with(SETUP_PAGE) {
        View::Setup
    } else {
        View::Settings
    })
}

/// Whether a section id is safe to put into a URL.
pub fn section_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// The server's front page, or a blank page while no server is set.
pub fn library_url(server_url: &str, path: &str) -> Url {
    let base = server_url.trim_end_matches('/');
    if base.is_empty() {
        return "about:blank".parse().expect("blank url");
    }
    format!("{base}{path}")
        .parse()
        .unwrap_or_else(|_| "about:blank".parse().expect("blank url"))
}

pub fn origin_of(base_url: &str) -> Option<String> {
    let url: Url = base_url.parse().ok()?;
    let host = url.host_str()?;
    Some(match url.port() {
        Some(p) => format!("{}://{host}:{p}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    })
}

/// Grants this server's pages the remote commands, once per origin. A capability cannot be
/// taken back at run time, so those carry nothing a stale origin could misuse.
pub fn allow_origin(app: &AppHandle, base_url: &str) {
    let Some(origin) = origin_of(base_url) else {
        return;
    };
    let n = {
        app.manage(Granted(Mutex::new(HashSet::new())));
        let granted = app.state::<Granted>();
        let mut set = granted.0.lock().unwrap_or_else(|e| e.into_inner());
        if !set.insert(origin.clone()) {
            return;
        }
        set.len()
    };
    let mut cap = CapabilityBuilder::new(format!("reel-pages-{n}"))
        .local(false)
        .webview(LIBRARY)
        .remote(format!("{origin}/*"));
    let (standalone, base) = app
        .try_state::<Runtime>()
        .map(|rt| (rt.is_standalone(), rt.library_base()))
        .unwrap_or_default();
    let granted = grants(&origin, standalone, &base);
    let own = granted.len() > REMOTE_PERMISSIONS.len();
    let count = granted.len();
    for p in granted {
        cap = cap.permission(p);
    }
    match app.add_capability(cap) {
        Ok(()) => tracing::info!(
            %origin,
            own,
            "the server's pages may call the app's {count} commands"
        ),
        Err(e) => tracing::warn!(%origin, error = %e, "the server's pages got no capability"),
    }
}

/// The window with its webview, hidden until something shows it, made for the library: it asks
/// for it at once, and a settings view shown on it lets that go.
pub fn build(app: &AppHandle, server_url: &str) -> tauri::Result<Window> {
    app.manage(Granted(Mutex::new(HashSet::new())));
    app.manage(crate::downloads::Downloads::default());
    let window = tauri::window::WindowBuilder::new(app, MAIN)
        .title("reel")
        .background_color(BACKGROUND)
        .inner_size(1280.0, 820.0)
        .min_inner_size(720.0, 480.0)
        .visible(false)
        .build()?;
    allow_origin(app, server_url);
    // standalone's local reel may still be starting: Status says so, the library follows
    let standalone = app
        .try_state::<Runtime>()
        .is_some_and(|rt| rt.is_standalone());
    let first = if server_url.is_empty() {
        local_url(&format!(
            "{SETTINGS_PAGE}#{}",
            if standalone { "status" } else { "server" }
        ))
    } else {
        app.try_state::<Runtime>()
            .and_then(|rt| rt.library_page("/").parse().ok())
            .unwrap_or_else(|| library_url(server_url, "/"))
    };
    let make = || library(app, first.clone());
    #[cfg(windows)]
    let made = audio::add(app, &window, make);
    #[cfg(not(windows))]
    let made = add(&window, make());
    let webview = match made {
        Ok(w) => w,
        Err(e) => {
            // a window without its webview stays blank; the next show makes both again
            let _ = window.destroy();
            return Err(e);
        }
    };
    crate::downloads::attach(&webview);
    #[cfg(target_os = "linux")]
    crate::media::attach(&webview);
    // not read back from the webview: on Windows its first page has not even started here
    if let Some(rt) = app.try_state::<Runtime>() {
        rt.go_library("/");
    }
    Ok(window)
}

fn library(app: &AppHandle, first: Url) -> WebviewBuilder<tauri::Wry> {
    let handle = app.clone();
    let opening = std::sync::atomic::AtomicBool::new(true);
    let first_page = first.to_string();
    WebviewBuilder::new(LIBRARY, WebviewUrl::External(first))
        .background_color(BACKGROUND)
        .initialization_script(format!(
            "window.__REEL_DESKTOP__ = {{ version: \"{}\" }};\n{PAGE_ERRORS}",
            reel_desktop_core::version_line()
        ))
        .on_navigation(move |url| {
            tracing::info!(to = %where_to(url), "library goes to");
            // WebView2 starts the first page after the library may have been asked for: it
            // overtakes nothing, so it ends no watch
            let own_first = opening.swap(false, std::sync::atomic::Ordering::SeqCst)
                && url.as_str() == first_page;
            if !own_first {
                if let Some(rt) = handle.try_state::<Runtime>() {
                    rt.navigating(url);
                }
            }
            true
        })
        .on_page_load(|_, payload| {
            if let PageLoadEvent::Finished = payload.event() {
                tracing::info!(at = %where_to(payload.url()), "library loaded");
            }
        })
        // with a hook set, wry picks where a download goes and WebView2 shows no bar of its own
        .on_download(|_, _| true)
}

/// Small on purpose: on Linux this size becomes a minimum; `layout` sets the real one.
fn add(window: &Window, library: WebviewBuilder<tauri::Wry>) -> tauri::Result<Webview> {
    window.add_child(
        library,
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(320.0, 240.0),
    )
}

fn logical_size(window: &Window) -> (f64, f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    match window.inner_size() {
        Ok(s) => (s.width as f64 / scale, s.height as f64 / scale),
        Err(_) => (1280.0, 820.0),
    }
}

/// After a resize: the webview takes the whole client area.
pub fn layout(window: &Window) {
    let (w, h) = logical_size(window);
    for wv in window.webviews() {
        if wv.label() == LIBRARY {
            let _ = wv.set_position(LogicalPosition::new(0.0, 0.0));
            let _ = wv.set_size(LogicalSize::new(w, h));
        }
    }
}

pub fn current(app: &AppHandle) -> View {
    app.get_webview(LIBRARY)
        .and_then(|w| w.url().ok())
        .and_then(|u| page_of(&u))
        .unwrap_or(View::Library)
}

/// Whether the user is on the app's settings or setup, which loading the library never
/// interrupts; the page the app shows while the window waits for the library is not theirs.
pub fn owner_busy(app: &AppHandle) -> bool {
    busy(
        current(app),
        app.try_state::<Runtime>()
            .is_some_and(|rt| rt.library_waits()),
    )
}

fn busy(on: View, library_waits: bool) -> bool {
    matches!(on, View::Settings | View::Setup) && !library_waits
}

/// Whether the webview shows one of the app's own pages, the only ones that get its events.
pub fn on_local_page(app: &AppHandle) -> bool {
    current(app) != View::Library
}

/// An event for the app's own page, when one is on; never into a server page.
pub fn emit_local<S: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    if on_local_page(app) {
        let _ = app.emit_to(EventTarget::labeled(LIBRARY), event, payload);
    }
}

/// Takes the webview to a view without showing the window; a server page already on stays.
pub fn go(app: &AppHandle, view: View, section: Option<&str>) {
    let Some(wv) = app.get_webview(LIBRARY) else {
        return;
    };
    let on = wv.url().ok().and_then(|u| page_of(&u));
    let section = section.filter(|s| section_ok(s));
    match view {
        View::Settings => {
            if on == Some(View::Settings) {
                if let Some(s) = section {
                    let _ = wv.emit_to(EventTarget::labeled(LIBRARY), "open-section", s);
                }
            } else {
                let page = match section {
                    Some(s) => format!("{SETTINGS_PAGE}#{s}"),
                    None => SETTINGS_PAGE.to_string(),
                };
                let _ = wv.navigate(local_url(&page));
            }
        }
        View::Offline => {
            if on != Some(View::Offline) {
                let _ = wv.navigate(local_url(OFFLINE_PAGE));
            }
        }
        View::Setup => {
            if on != Some(View::Setup) {
                let _ = wv.navigate(local_url(SETUP_PAGE));
            }
        }
        View::Library => {
            if on.is_some() {
                if let Some(rt) = app.try_state::<Runtime>() {
                    rt.go_library("/");
                }
            }
        }
    }
    tracing::info!(view = view.name(), "window shows");
}

/// The window, made again when wanted: closing it destroys its webview too, so the app in the
/// tray holds no WebView.
pub fn ensure(app: &AppHandle) -> Option<Window> {
    if crate::updates::leaving() {
        return None;
    }
    if let Some(w) = app.get_window(MAIN) {
        return Some(w);
    }
    let rt = app.try_state::<Runtime>()?;
    let base = rt.library_base();
    match build(app, &base) {
        Ok(w) => {
            tracing::info!("the window and its webview are made again");
            Some(w)
        }
        Err(e) => {
            tracing::warn!(error = %e, "the window could not be made");
            None
        }
    }
}

/// Shows the window as it is, on whatever page is on.
pub fn present(app: &AppHandle) {
    if let Some(window) = ensure(app) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        layout(&window);
    }
}

/// Shows the window on a view. The library without a server is the settings.
pub fn show(app: &AppHandle, view: View, section: Option<&str>) {
    let Some(window) = ensure(app) else {
        return;
    };
    if matches!(view, View::Settings | View::Setup) {
        if let Some(rt) = app.try_state::<Runtime>() {
            rt.library_let_go();
        }
    }
    let view = if view == View::Library && !has_server(app) {
        View::Settings
    } else {
        view
    };
    let _ = window.show();
    let _ = window.unminimize();
    go(app, view, section);
    if let Some(wv) = app.get_webview(LIBRARY) {
        let _ = wv.set_focus();
    }
    let _ = window.set_focus();
    layout(&window);
}

/// Where the open window is, for the reel after an update to open it there again.
pub fn place(app: &AppHandle) -> Option<reel_desktop_core::update_steps::Place> {
    let window = app.get_window(MAIN)?;
    if !window.is_visible().unwrap_or(false) {
        return None;
    }
    let url = app.get_webview(LIBRARY)?.url().ok()?;
    Some(match page_of(&url) {
        Some(view) => reel_desktop_core::update_steps::Place {
            view: view.name().to_string(),
            section: url
                .fragment()
                .filter(|s| view == View::Settings && section_ok(s))
                .map(str::to_string),
            path: None,
        },
        None => reel_desktop_core::update_steps::Place {
            view: View::Library.name().to_string(),
            section: None,
            path: Some(match url.query() {
                Some(q) => format!("{}?{q}", url.path()),
                None => url.path().to_string(),
            }),
        },
    })
}

/// The window again where `place` says it was.
pub fn reopen(rt: &Runtime, place: &reel_desktop_core::update_steps::Place) {
    match View::parse(&place.view) {
        Some(View::Settings) => show(&rt.app, View::Settings, place.section.as_deref()),
        Some(View::Setup) => show(&rt.app, View::Setup, None),
        _ => {
            let path = place.path.as_deref().filter(|p| p.starts_with('/'));
            match path.map(|p| (p, rt.open_library(p))) {
                Some((_, Ok(()))) => {}
                // reel on this PC still starts: the page waits for it
                Some((p, Err(_))) => {
                    show(&rt.app, View::Library, None);
                    rt.go_library(p);
                }
                None => show(&rt.app, View::Library, None),
            }
        }
    }
}

fn has_server(app: &AppHandle) -> bool {
    app.try_state::<Runtime>()
        .map(|rt| {
            let s = rt.settings();
            s.mode == reel_desktop_core::settings::Mode::Standalone || !s.server_url.is_empty()
        })
        .unwrap_or(false)
}

pub fn reload_library_if_home(app: &AppHandle) {
    if let Some(library) = app.get_webview(LIBRARY) {
        if let Ok(url) = library.url() {
            if !is_local(&url) && url.path() == "/" {
                let _ = library.reload();
            }
        }
    }
}

/// Takes the webview to a page of the server, granting its origin the remote commands first.
pub fn navigate(app: &AppHandle, server_url: &str, page: &str) {
    allow_origin(app, server_url);
    if let Some(library) = app.get_webview(LIBRARY) {
        let url = page
            .parse()
            .unwrap_or_else(|_| library_url(server_url, "/"));
        tracing::info!(to = %where_to(&url), "library window opens");
        let _ = library.navigate(url);
    }
}

/// Where WebView2 plays the window's sound: in its browser process by default, else in
/// Chromium's own audio process with a folder of its own.
#[cfg(windows)]
pub mod audio {
    use super::Runtime;
    use reel_desktop_core::settings::WebviewAudio;
    use reel_desktop_core::webview;
    use std::sync::OnceLock;
    use tauri::webview::WebviewBuilder;
    use tauri::{AppHandle, Manager, Webview, Window, Wry};

    /// The arguments and the data folder a webview is made with.
    type Plan = (Option<&'static str>, Option<&'static str>);

    /// This run's plan, from its first window: a webview made later with another could not
    /// join the browser process the first one started.
    static RUN: OnceLock<Plan> = OnceLock::new();

    fn with(
        app: &AppHandle,
        library: WebviewBuilder<Wry>,
        (args, folder): Plan,
    ) -> WebviewBuilder<Wry> {
        let library = match args {
            Some(a) => library.additional_browser_args(a),
            None => library,
        };
        match folder.zip(app.path().app_local_data_dir().ok()) {
            Some((f, local)) => library.data_directory(local.join(f)),
            None => library,
        }
    }

    pub fn add(
        app: &AppHandle,
        window: &Window,
        make: impl Fn() -> WebviewBuilder<Wry>,
    ) -> tauri::Result<Webview> {
        if let Some(plan) = RUN.get() {
            return super::add(window, with(app, make(), *plan));
        }
        let audio = app
            .try_state::<Runtime>()
            .map(|rt| rt.settings().webview_audio())
            .unwrap_or_default();
        let plan = (webview::args(audio), webview::folder(audio));
        // a WebView2 left running with other arguments holds the folder, which takes one set
        // of arguments at a time: wait for it to close
        let tries = match audio {
            WebviewAudio::Browser => {
                (webview::WAIT.as_millis() / webview::EVERY.as_millis()) as u32
            }
            WebviewAudio::Service => 1,
        };
        let made = webview::retry(
            tries,
            || std::thread::sleep(webview::EVERY),
            |n| {
                let made = super::add(window, with(app, make(), plan));
                if let Err(e) = &made {
                    if n == 1 {
                        tracing::info!(error = %e, "the webview could not be made: a WebView2 of the last run may still hold its folder; waiting");
                    }
                }
                made
            },
        );
        let (webview, plan) = match made {
            Ok(w) => (w, plan),
            Err(e) if audio == WebviewAudio::Browser => {
                tracing::warn!(error = %e, "a WebView2 of other arguments held the webview's folder for 8 s: this run plays the window's sound from WebView2's audio process, as 1.2.0 did, in the same folder");
                let held = (None, None);
                (super::add(window, with(app, make(), held))?, held)
            }
            Err(e) => return Err(e),
        };
        match plan {
            (Some(_), _) => tracing::info!("WebView2 plays the window's sound from its browser process"),
            (None, Some(_)) => tracing::info!("WebView2 plays the window's sound from its own audio process (--webview-audio=service), in a folder of its own"),
            (None, None) => {}
        }
        let _ = RUN.set(plan);
        Ok(webview)
    }

    /// Removes the test build's webview folder once nothing holds it: now, then every 5 s for
    /// a minute, then at the next start.
    pub fn clear_left_over(app: &AppHandle) {
        let Ok(local) = app.path().app_local_data_dir() else {
            return;
        };
        std::thread::spawn(move || {
            for n in 0..12 {
                match webview::remove_left_over(&local) {
                    Ok(true) => {
                        tracing::info!(
                            folder = webview::LEFT_OVER,
                            "the test build's webview folder removed"
                        );
                        return;
                    }
                    Ok(false) => return,
                    Err(e) if n == 11 => {
                        tracing::info!(folder = webview::LEFT_OVER, error = %e, "the test build's webview folder is still held; the next start tries again");
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_secs(5)),
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        s.parse().expect("a url")
    }

    #[test]
    fn the_update_answer_is_granted_to_the_library_the_app_shows_and_opening_files_to_this_pcs_alone(
    ) {
        let answer = "allow-app-update-answer";
        let open = "allow-app-library-open";
        let server = "https://clips.example.com";
        let paired = grants(server, false, "https://clips.example.com/");
        assert!(
            paired.contains(&answer) && !paired.contains(&open),
            "{paired:?}"
        );
        assert!(REMOTE_PERMISSIONS.iter().all(|p| paired.contains(p)));
        let other = grants("https://other.example.com", false, server);
        assert_eq!(
            other,
            REMOTE_PERMISSIONS.to_vec(),
            "another server's pages: the remote commands alone"
        );
        let here = grants("http://127.0.0.1:41234", true, "http://127.0.0.1:41234");
        assert!(here.contains(&answer) && here.contains(&open), "{here:?}");
        assert!(
            !grants("http://127.0.0.1:41235", true, "http://127.0.0.1:41234").contains(&answer),
            "an earlier run's port"
        );
        assert!(
            !grants(server, true, "http://127.0.0.1:41234").contains(&answer),
            "the server's pages while the app runs standalone"
        );
        assert_eq!(grants(server, false, ""), REMOTE_PERMISSIONS.to_vec());
    }

    #[test]
    fn only_the_librarys_own_page_answers_the_question() {
        let base = "https://clips.example.com";
        assert!(library_page_of(
            LIBRARY,
            Some("https://clips.example.com/clips/01J"),
            base
        ));
        assert!(library_page_of(
            LIBRARY,
            Some("http://127.0.0.1:41234/"),
            "http://127.0.0.1:41234"
        ));
        assert!(
            !library_page_of(LIBRARY, Some("https://old.example.com/"), base),
            "a server the app was paired with before keeps its grant, not the answer"
        );
        assert!(!library_page_of(
            LIBRARY,
            Some("http://clips.example.com/"),
            base
        ));
        assert!(!library_page_of(
            LIBRARY,
            Some("https://clips.example.com:8443/"),
            base
        ));
        assert!(!library_page_of(
            "other",
            Some("https://clips.example.com/"),
            base
        ));
        assert!(!library_page_of(LIBRARY, None, base));
        assert!(!library_page_of(
            LIBRARY,
            Some("https://clips.example.com/"),
            ""
        ));
    }

    #[test]
    fn a_webview_before_its_first_page_commits_shows_the_app_page() {
        assert!(shows_app_page(None));
        assert!(shows_app_page(Some(&url("about:blank"))));
        assert!(shows_app_page(Some(&url(
            "tauri://localhost/index.html#status"
        ))));
        assert!(shows_app_page(Some(&url(
            "http://tauri.localhost/index.html#status"
        ))));
        assert!(!shows_app_page(Some(&url("http://127.0.0.1:49152/"))));
        assert!(!shows_app_page(Some(&url(
            "https://clips.example.com/clips/A1"
        ))));
    }

    #[test]
    fn the_page_shown_while_the_library_waits_is_not_the_users() {
        assert!(busy(View::Settings, false));
        assert!(busy(View::Setup, false));
        assert!(!busy(View::Settings, true));
        assert!(!busy(View::Setup, true));
        assert!(!busy(View::Library, false));
        assert!(!busy(View::Offline, false));
    }
}
