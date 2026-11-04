//! The install folder while an installer works on it: the processes that run from it, and the
//! mark that keeps the app from starting OBS meanwhile.
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A Windows path as compared: without a `\\?\` or `\??\` prefix (`\\?\UNC\` is `\\`), with
/// backslashes, no trailing one, in lower case, as Windows compares names.
pub fn path_key(p: &str) -> String {
    let p = p.replace('/', "\\");
    let p = if let Some(rest) = p.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = p.strip_prefix(r"\\?\").or_else(|| p.strip_prefix(r"\??\")) {
        rest.to_string()
    } else {
        p
    };
    p.trim_end_matches('\\').to_lowercase()
}

/// Whether `path` is `dir` or inside it.
pub fn under(path: &str, dir: &str) -> bool {
    let (p, d) = (path_key(path), path_key(dir));
    p == d || p.starts_with(&format!("{d}\\"))
}

/// A process as the stop helper sees it; `path` is None where Windows does not say it.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    pub path: Option<String>,
}

/// What the stop helper ends in the install folder `dir`: the app first, as it starts its
/// OBS again when that stops; then everything that runs from `obs\` (OBS and its helpers).
pub fn plan(list: &[Process], dir: &str) -> (Vec<Process>, Vec<Process>) {
    let app = format!(r"{dir}\reel-desktop.exe");
    let obs = format!(r"{dir}\obs");
    let of = |want: &dyn Fn(&str) -> bool| {
        list.iter()
            .filter(|p| p.path.as_deref().is_some_and(want))
            .cloned()
            .collect::<Vec<_>>()
    };
    (
        of(&|p| path_key(p) == path_key(&app)),
        of(&|p| under(p, &obs)),
    )
}

/// An OBS whose path Windows keeps from a process without the highest rights: the one the
/// task for games that run as administrator started, which the task ends.
pub fn unreadable_obs(list: &[Process]) -> bool {
    list.iter()
        .any(|p| p.path.is_none() && crate::bundled::is_obs_exe(&p.name))
}

pub const MARK: &str = "reel-installing";

pub fn mark(dir: &Path) -> PathBuf {
    dir.join(MARK)
}

/// An installer at work on the folder: its mark, at most ten minutes old, as one that stopped
/// half way leaves it behind.
pub fn installing(dir: &Path) -> bool {
    std::fs::metadata(mark(dir))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(600))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, name: &str, path: Option<&str>) -> Process {
        Process {
            pid,
            name: name.into(),
            path: path.map(str::to_string),
        }
    }

    #[test]
    fn a_path_is_matched_in_any_form_it_is_written_in() {
        let dir = r"C:\Users\Robin\AppData\Local\reel";
        for path in [
            r"C:\Users\Robin\AppData\Local\reel\obs\bin\64bit\obs64.exe",
            r"\\?\C:\Users\Robin\AppData\Local\reel\obs\bin\64bit\obs64.exe",
            r"\??\C:\Users\Robin\AppData\Local\reel\obs\bin\64bit\obs64.exe",
            r"c:\users\robin\appdata\local\REEL\OBS\BIN\64BIT\OBS64.EXE",
            "C:/Users/Robin/AppData/Local/reel/obs/bin/64bit/obs64.exe",
        ] {
            assert!(under(path, &format!(r"{dir}\obs")), "{path}");
        }
        assert!(under(
            r"\\?\UNC\nas\share\reel\obs\x.dll",
            r"\\nas\share\reel"
        ));
        assert!(!under(
            r"C:\Users\Robin\AppData\Local\reel-other\obs\bin\64bit\obs64.exe",
            dir
        ));
        assert!(!under(
            r"C:\Program Files\obs-studio\bin\64bit\obs64.exe",
            dir
        ));
        assert!(
            under(dir, &format!(r"{dir}\")),
            "the folder itself, a trailing backslash"
        );
        assert!(
            under(r"C:\Games [old]\reel\obs\obs64.exe", r"C:\Games [old]\reel"),
            "brackets are no wildcards here"
        );
    }

    #[test]
    fn the_app_is_ended_first_then_what_runs_from_obs_and_nothing_else() {
        let dir = r"C:\Users\Robin\AppData\Local\reel";
        let list = [
            p(
                1,
                "reel-desktop.exe",
                Some(r"\\?\C:\Users\Robin\AppData\Local\reel\reel-desktop.exe"),
            ),
            p(
                2,
                "obs64.exe",
                Some(r"C:\USERS\ROBIN\APPDATA\LOCAL\REEL\OBS\BIN\64BIT\OBS64.EXE"),
            ),
            p(
                3,
                "obs-ffmpeg-mux.exe",
                Some(r"C:\Users\Robin\AppData\Local\reel\obs\bin\64bit\obs-ffmpeg-mux.exe"),
            ),
            p(
                4,
                "obs64.exe",
                Some(r"C:\Program Files\obs-studio\bin\64bit\obs64.exe"),
            ),
            p(
                5,
                "reel-desktop.exe",
                Some(r"D:\elsewhere\reel-desktop.exe"),
            ),
            p(6, "reel-capture.exe", None),
            p(7, "explorer.exe", None),
        ];
        let (app, obs) = plan(&list, dir);
        assert_eq!(app.iter().map(|x| x.pid).collect::<Vec<_>>(), [1]);
        assert_eq!(
            obs.iter().map(|x| x.pid).collect::<Vec<_>>(),
            [2, 3],
            "never the user's own OBS"
        );
        assert!(
            unreadable_obs(&list),
            "one OBS Windows does not show the path of"
        );
        assert!(!unreadable_obs(&list[..5]));
        assert!(
            unreadable_obs(&[p(8, "obs64.exe", None)]),
            "reel's OBS of an older version's install"
        );
    }

    #[test]
    fn the_mark_counts_for_ten_minutes() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!installing(dir.path()));
        std::fs::write(mark(dir.path()), "").unwrap();
        assert!(installing(dir.path()));
        let old = std::fs::File::options()
            .write(true)
            .open(mark(dir.path()))
            .unwrap();
        old.set_modified(SystemTime::now() - Duration::from_secs(700))
            .unwrap();
        assert!(!installing(dir.path()), "a mark an installer left behind");
    }
}
