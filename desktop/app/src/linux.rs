//! Linux: the session type, the desktop entry, the tray check and WebKitGTK's NVIDIA troubles.
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    X11,
    Wayland,
}

pub fn session() -> Session {
    let kind = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    match kind.as_str() {
        "wayland" => Session::Wayland,
        "x11" => Session::X11,
        _ if std::env::var_os("WAYLAND_DISPLAY").is_some() => Session::Wayland,
        _ => Session::X11,
    }
}

fn set_unless_set(key: &str, value: &str) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, value);
    }
}

/// Before GTK starts: with NVIDIA's driver WebKitGTK's DMA-BUF renderer leaves the window blank
/// and explicit sync breaks Wayland windows, so each is off unless the user set it.
pub fn webkit_workarounds() {
    if !Path::new("/proc/driver/nvidia/version").exists() {
        return;
    }
    set_unless_set("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    if session() == Session::Wayland {
        set_unless_set("__NV_DISABLE_EXPLICIT_SYNC", "1");
    }
}

pub fn appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from)
}

/// What a desktop's own shortcut runs to reach the running app.
pub fn command() -> String {
    let path = appimage()
        .or_else(|| std::env::current_exe().ok())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "reel-desktop".into());
    if path.contains(' ') {
        format!("\"{path}\"")
    } else {
        path
    }
}

/// Writes the desktop entry named by the app id: xdg-desktop-portal's GlobalShortcuts refuses an
/// app outside a sandbox without one.
pub fn desktop_entry(data_dir: &Path) -> Result<PathBuf, String> {
    let name = format!("{}.desktop", crate::portal::APP_ID);
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    if let Some(p) = system
        .split(':')
        .map(|d| Path::new(d).join("applications").join(&name))
        .find(|p| p.is_file())
    {
        return Ok(p);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local").join("share"));
    let path = data_home.join("applications").join(&name);
    let icon = data_dir.join("reel.png");
    if !icon.is_file() {
        let _ = std::fs::create_dir_all(data_dir);
        let _ = std::fs::write(&icon, include_bytes!("../icons/128x128.png"));
    }
    let text = entry_text(&command(), &icon.display().to_string());
    match std::fs::read_to_string(&path) {
        Ok(had) if had == text || !had.contains(WRITTEN) => return Ok(path),
        _ => {}
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    tracing::info!(path = %path.display(), "the desktop entry the portals know reel by");
    Ok(path)
}

const WRITTEN: &str = "X-Reel-Written=true";

fn entry_text(exec: &str, icon: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=reel\nComment=Shares your OBS clips\nExec={exec}\nIcon={icon}\nStartupWMClass=reel-desktop\nTerminal=false\nNoDisplay=true\n{WRITTEN}\n"
    )
}

/// The note the Hotkeys page shows on Linux.
pub fn hotkeys_how() -> String {
    let cmd = command();
    format!(
        "This is the Linux version: global hotkeys depend on your desktop, so they may not work here, or may need to be set up in the desktop's own keyboard settings. There you can bind a key to {cmd} --save-and-share (or --save, --share-last)."
    )
}

/// Checks whether the desktop shows a tray: a StatusNotifier host on the session bus, or on X11
/// a tray of the older kind.
pub fn watch_tray(rt: crate::runtime::Runtime) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let shown = status_notifier_host().await || xembed_tray().await;
        let line = if shown {
            String::new()
        } else {
            tracing::info!("no tray on this desktop");
            "this desktop shows no tray icons (on GNOME the AppIndicator extension adds them); reel runs all the same: start it again to open its window".into()
        };
        rt.lock().tray_line = line;
        rt.push_status();
    });
}

async fn status_notifier_host() -> bool {
    use ashpd::zbus;
    let Ok(conn) = zbus::Connection::session().await else {
        return false;
    };
    let Ok(dbus) = zbus::fdo::DBusProxy::new(&conn).await else {
        return false;
    };
    let Ok(name) = zbus::names::BusName::try_from("org.kde.StatusNotifierWatcher") else {
        return false;
    };
    dbus.name_has_owner(name).await.unwrap_or(false)
}

/// Asks the X server, since GDK answers only for windows of this process.
async fn xembed_tray() -> bool {
    if session() != Session::X11 {
        return false;
    }
    tokio::task::spawn_blocking(|| {
        let Ok(x) = x11_dl::xlib::Xlib::open() else {
            return false;
        };
        // SAFETY: a display opened here, used for two requests and closed again.
        unsafe {
            let d = (x.XOpenDisplay)(std::ptr::null());
            if d.is_null() {
                return false;
            }
            let atom = (x.XInternAtom)(d, c"_NET_SYSTEM_TRAY_S0".as_ptr(), 0);
            let owner = (x.XGetSelectionOwner)(d, atom);
            (x.XCloseDisplay)(d);
            owner != 0
        }
    })
    .await
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_desktop_entry_runs_the_app_and_stays_out_of_the_menus() {
        let t = entry_text(
            "\"/home/sam/My Apps/reel.AppImage\"",
            "/home/sam/.local/share/reel/reel.png",
        );
        assert!(t.starts_with("[Desktop Entry]\nType=Application\nName=reel\n"));
        assert!(t.contains("\nExec=\"/home/sam/My Apps/reel.AppImage\"\n"));
        assert!(t.contains("\nNoDisplay=true\n"));
        assert!(t.contains(WRITTEN));
    }
}
