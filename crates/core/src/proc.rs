//! The one way reel and the desktop app start a process: on Windows every child gets
//! CREATE_NO_WINDOW, so a console child does not flash a window from the windowed app.
use std::ffi::OsStr;
use std::process::Command;

/// The child gets no console window (Windows).
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The child runs in the idle priority class (Windows), for ffmpeg while a game plays.
pub const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;

/// A process without a console window.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    command_with(program, 0)
}

/// The same, with more creation flags (Windows); CREATE_NO_WINDOW is always among them.
pub fn command_with(program: impl AsRef<OsStr>, flags: u32) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW | flags);
    }
    #[cfg(not(windows))]
    {
        let _ = flags;
        if let Some(appdir) = std::env::var_os("APPDIR") {
            for (key, value) in outside_appimage(std::env::vars_os(), &appdir) {
                match value {
                    Some(v) => c.env(key, v),
                    None => c.env_remove(key),
                };
            }
        }
    }
    c
}

/// The environment changes, as (name, value or None to remove), that keep the AppImage's own
/// GTK, GIO and GdkPixbuf modules from a system-built program such as OBS, which stops on them.
pub fn outside_appimage(
    vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
    appdir: &OsStr,
) -> Vec<(std::ffi::OsString, Option<std::ffi::OsString>)> {
    let dir = appdir.to_string_lossy().trim_end_matches('/').to_string();
    let inside = |p: &str| !dir.is_empty() && (p == dir || p.starts_with(&format!("{dir}/")));
    let mut out = Vec::new();
    for (key, value) in vars {
        let name = key.to_string_lossy();
        if ["APPDIR", "APPIMAGE", "ARGV0", "OWD"].contains(&name.as_ref()) {
            out.push((key, None));
            continue;
        }
        let v = value.to_string_lossy();
        if !v.split(':').any(inside) {
            continue;
        }
        let kept: Vec<&str> = v
            .split(':')
            .filter(|p| !p.is_empty() && !inside(p))
            .collect();
        if kept.is_empty() {
            out.push((key, None));
        } else {
            out.push((key, Some(kept.join(":").into())));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_started_from_the_appimage_gets_the_systems_environment() {
        let vars = [
            ("APPDIR", "/tmp/.mount_reelX"),
            ("APPIMAGE", "/home/sam/reel.AppImage"),
            ("GTK_PATH", "/tmp/.mount_reelX//usr/lib/gtk-3.0"),
            ("GIO_MODULE_DIR", "/tmp/.mount_reelX//usr/lib/gio/modules"),
            (
                "XDG_DATA_DIRS",
                "/tmp/.mount_reelX/usr/share:/usr/share:/usr/local/share",
            ),
            ("HOME", "/home/sam"),
            ("PATH", "/usr/bin:/bin"),
            ("SOMEWHERE", "/tmp/.mount_reelXYZ/other"),
        ]
        .map(|(k, v)| (k.into(), v.into()));
        let changed = outside_appimage(vars.into_iter(), OsStr::new("/tmp/.mount_reelX/"));
        let get = |k: &str| changed.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(get("APPDIR"), Some(None));
        assert_eq!(get("APPIMAGE"), Some(None));
        assert_eq!(get("GTK_PATH"), Some(None), "only the AppImage's: gone");
        assert_eq!(get("GIO_MODULE_DIR"), Some(None));
        assert_eq!(
            get("XDG_DATA_DIRS"),
            Some(Some("/usr/share:/usr/local/share".into())),
            "the system's entries stay"
        );
        assert_eq!(get("HOME"), None, "untouched");
        assert_eq!(get("PATH"), None);
        assert_eq!(
            get("SOMEWHERE"),
            None,
            "another folder that only starts alike"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_childs_output_is_read_as_before() {
        let out = command("sh")
            .args(["-c", "printf out; printf err >&2; exit 3"])
            .output()
            .unwrap();
        assert_eq!(out.stdout, b"out");
        assert_eq!(out.stderr, b"err");
        assert_eq!(out.status.code(), Some(3));
    }

    #[cfg(windows)]
    #[test]
    fn a_childs_output_is_read_as_before() {
        let out = command("cmd").args(["/C", "echo out"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "out");
    }
}
