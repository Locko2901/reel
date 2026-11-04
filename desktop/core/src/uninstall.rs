//! Whether the uninstaller may delete the clips folder: only a plain folder of its own. The
//! path is resolved before it is compared, so a link on the way does not hide where it is.
use crate::install::{path_key, under};
use std::path::{Path, PathBuf};

/// A folder the clips folder may neither be nor hold; `inside` keeps every folder in it too.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub path: String,
    pub what: String,
    pub inside: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Clips {
    /// No folder there.
    Missing,
    /// The folder as resolved, which is what goes.
    Offer(String),
    /// The folder as written.
    Keep { dir: String, why: String },
}

impl Clips {
    /// The line the uninstaller's page reads: `offer|dir|size`, `keep|dir|why` or `missing`.
    pub fn line(&self, size: impl Fn(&str) -> String) -> String {
        match self {
            Clips::Missing => "missing".into(),
            Clips::Offer(dir) => format!("offer|{dir}|{}", size(dir)),
            Clips::Keep { dir, why } => format!("keep|{dir}|{why}"),
        }
    }
}

pub trait Disk {
    /// Links and junctions followed; None when no folder is there.
    fn resolve(&self, path: &str) -> Option<String>;
    /// Whether the folder itself is a reparse point: a link, a junction, a mount.
    fn is_link(&self, path: &str) -> bool;
}

fn on_drive(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\'
}

/// Backslashes, without `\\?\` (`\\?\UNC\` is `\\`), no trailing backslash but a drive's own.
fn plain(p: &str) -> String {
    let p = p.replace('/', "\\");
    let p = if let Some(rest) = p.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = p.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        p
    };
    shown(&p)
}

/// No trailing backslash but a drive's own.
fn shown(p: &str) -> String {
    let t = p.trim_end_matches('\\');
    if t.len() == 2 && t.ends_with(':') && t.len() < p.len() {
        format!("{t}\\")
    } else {
        t.to_string()
    }
}

/// Why a path is not plainly a folder from a drive letter, if it is not.
fn written_wrong(p: &str) -> Option<&'static str> {
    if p.starts_with(r"\\") && !p.starts_with(r"\\?\") && !p.starts_with(r"\\.\") {
        return Some("it is a folder on the network");
    }
    if !on_drive(p) {
        return Some(r"its path does not start from a drive letter, such as D:\");
    }
    let rest = p[3..].trim_end_matches('\\');
    if rest.is_empty() {
        return Some("it is a whole drive");
    }
    let parts: Vec<&str> = rest.split('\\').collect();
    if parts
        .iter()
        .any(|s| !s.is_empty() && s.chars().all(|c| c == '.'))
    {
        return Some("its path has . or .. in it");
    }
    if parts.iter().any(|s| s.is_empty()) {
        return Some(r"its path has an empty part (\\) in it");
    }
    None
}

pub fn decide(raw: &str, places: &[Place], disk: &impl Disk) -> Clips {
    let written = raw.trim().replace('/', "\\");
    if written.is_empty() {
        return Clips::Missing;
    }
    let keep = |why: String| Clips::Keep {
        dir: shown(&written),
        why,
    };
    if let Some(why) = written_wrong(&written) {
        return keep(why.into());
    }
    let Some(real) = disk.resolve(&written) else {
        return Clips::Missing;
    };
    if disk.is_link(&written) {
        return keep("it is a link or a mount point, not a folder of its own".into());
    }
    let real = plain(&real);
    if let Some(why) = written_wrong(&real) {
        return keep(why.into());
    }
    for place in places.iter().filter(|p| !p.path.trim().is_empty()) {
        let at = disk
            .resolve(&place.path)
            .unwrap_or_else(|| place.path.clone());
        if path_key(&at) == path_key(&real) {
            return keep(format!("it is {}", place.what));
        }
        if under(&at, &real) {
            return keep(format!("it holds {}", place.what));
        }
        if place.inside && under(&real, &at) {
            return keep(format!("it is inside {}", place.what));
        }
    }
    Clips::Offer(real)
}

/// From Windows' known folders and environment, reel's own folders, and `also`; each once.
pub fn places(install: &str, settings_dir: &Path, also: &[String]) -> Vec<Place> {
    let env = |k: &str| std::env::var(k).ok().map(PathBuf::from);
    let named: Vec<(Option<PathBuf>, &str, bool)> = vec![
        (dirs::home_dir(), "your user folder", false),
        (env("USERPROFILE"), "your user folder", false),
        (dirs::video_dir(), "your Videos folder", false),
        (dirs::document_dir(), "your Documents folder", false),
        (dirs::desktop_dir(), "your desktop", false),
        (dirs::picture_dir(), "your Pictures folder", false),
        (dirs::audio_dir(), "your Music folder", false),
        (dirs::download_dir(), "your Downloads folder", false),
        (dirs::public_dir(), "the Public folder", false),
        (env("OneDrive"), "your OneDrive folder", false),
        (env("OneDriveConsumer"), "your OneDrive folder", false),
        (env("OneDriveCommercial"), "your OneDrive folder", false),
        (env("SystemRoot"), "Windows", true),
        (env("windir"), "Windows", true),
        (env("ProgramFiles"), "Program Files", false),
        (env("ProgramW6432"), "Program Files", false),
        (env("ProgramFiles(x86)"), "Program Files (x86)", false),
        (env("CommonProgramFiles"), "Common Files", false),
        (env("CommonProgramW6432"), "Common Files", false),
        (env("CommonProgramFiles(x86)"), "Common Files", false),
        (env("ProgramData"), "ProgramData", false),
        (env("ALLUSERSPROFILE"), "ProgramData", false),
        (dirs::data_local_dir(), "your local app data", false),
        (env("LOCALAPPDATA"), "your local app data", false),
        (dirs::data_dir(), "your app data", false),
        (env("APPDATA"), "your app data", false),
        (Some(PathBuf::from(install)), "reel's install folder", false),
        (Some(settings_dir.to_path_buf()), "reel's settings", false),
    ];
    let mut out: Vec<Place> = Vec::new();
    let mut add = |path: String, what: &str, inside: bool| {
        if !path.trim().is_empty() && !out.iter().any(|p| path_key(&p.path) == path_key(&path)) {
            out.push(Place {
                path,
                what: what.into(),
                inside,
            });
        }
    };
    for (path, what, inside) in named {
        if let Some(p) = path {
            add(p.to_string_lossy().into_owned(), what, inside);
        }
    }
    for p in also {
        add(p.clone(), "a folder of Windows' own", false);
    }
    out
}

pub struct RealDisk;

impl Disk for RealDisk {
    fn resolve(&self, path: &str) -> Option<String> {
        let real = std::fs::canonicalize(path).ok()?;
        real.is_dir().then(|| real.to_string_lossy().into_owned())
    }

    fn is_link(&self, path: &str) -> bool {
        let Ok(m) = std::fs::symlink_metadata(path) else {
            return false;
        };
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        }
        #[cfg(not(windows))]
        {
            m.file_type().is_symlink()
        }
    }
}

/// The bytes and files under `dir`; a link is not followed, as deleting it takes the link alone.
pub fn size(dir: &Path) -> (u64, u64) {
    let (mut bytes, mut files) = (0, 0);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(t) = e.file_type() else {
                continue;
            };
            if t.is_symlink() {
                continue;
            }
            if t.is_dir() {
                stack.push(e.path());
            } else if let Ok(m) = e.metadata() {
                bytes += m.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

/// "4.8 GB, 312 files" or "empty".
pub fn size_words(bytes: u64, files: u64) -> String {
    if files == 0 {
        return "empty".into();
    }
    const K: u64 = 1024;
    let size = if bytes >= K * K * K {
        format!(
            "{}.{} GB",
            bytes / (K * K * K),
            bytes % (K * K * K) * 10 / (K * K * K)
        )
    } else if bytes >= K * K {
        format!("{} MB", bytes / (K * K))
    } else {
        format!("{} KB", bytes.div_ceil(K))
    };
    let s = if files == 1 { "" } else { "s" };
    format!("{size}, {files} file{s}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeDisk {
        folders: HashMap<String, String>,
        links: Vec<String>,
    }

    impl Disk for FakeDisk {
        fn resolve(&self, path: &str) -> Option<String> {
            self.folders.get(&path_key(path)).cloned()
        }
        fn is_link(&self, path: &str) -> bool {
            self.links.contains(&path_key(path))
        }
    }

    fn disk() -> FakeDisk {
        let mut folders = HashMap::new();
        let mut real = |p: &str| {
            folders.insert(path_key(p), format!(r"\\?\{p}"));
        };
        for p in [
            r"C:\",
            r"D:\",
            r"C:\Clips",
            r"D:\Games\Clips",
            r"C:\Users",
            r"C:\Users\sam",
            r"C:\Users\sam\Videos",
            r"C:\Users\sam\Videos\reel",
            r"C:\Users\sam\Downloads",
            r"C:\Users\sam\AppData",
            r"C:\Users\sam\AppData\Local",
            r"C:\Users\sam\AppData\Local\reel",
            r"C:\Users\sam\AppData\Roaming",
            r"C:\Users\sam\AppData\Roaming\reel",
            r"C:\Users\Public",
            r"C:\Windows",
            r"C:\Windows\System32",
            r"C:\Program Files",
            r"C:\Program Files\Common Files",
            r"C:\Program Files (x86)",
            r"C:\ProgramData",
            r"D:\Media",
            r"D:\Media\Videos",
            r"D:\Real",
        ] {
            real(p);
        }
        let mut via = |written: &str, to: &str| {
            folders.insert(path_key(written), to.to_string());
        };
        via(r"C:\Link", r"\\?\D:\Real");
        via(r"C:\Via\Clips", r"\\?\C:\Windows\Clips");
        via(r"C:\Up", r"\\?\C:\Users");
        via(r"X:\sam", r"\\?\C:\Users\sam");
        via(r"X:\Clips", r"\\?\C:\Clips");
        via(r"Z:\Clips", r"\\?\UNC\nas\share\Clips");
        via(
            r"C:\Mount\Clips",
            r"\\?\Volume{5e2c1d4a-0000-0000-0000-100000000000}\Clips",
        );
        FakeDisk {
            folders,
            links: vec![path_key(r"C:\Link"), path_key(r"C:\Up")],
        }
    }

    fn fixture() -> Vec<Place> {
        let p = |path: &str, what: &str, inside: bool| Place {
            path: path.into(),
            what: what.into(),
            inside,
        };
        vec![
            p(r"C:\Users\sam", "your user folder", false),
            p(r"D:\Media\Videos", "your Videos folder", false),
            p(r"C:\Users\sam\Downloads", "your Downloads folder", false),
            p(r"C:\Users\Public", "the Public folder", false),
            p(r"C:\Windows", "Windows", true),
            p(r"C:\Program Files", "Program Files", false),
            p(r"C:\Program Files (x86)", "Program Files (x86)", false),
            p(r"C:\Program Files\Common Files", "Common Files", false),
            p(r"C:\ProgramData", "ProgramData", false),
            p(r"C:\Users\sam\AppData\Local", "your local app data", false),
            p(r"C:\Users\sam\AppData\Roaming", "your app data", false),
            p(
                r"C:\Users\sam\AppData\Local\reel",
                "reel's install folder",
                false,
            ),
            p(
                r"C:\Users\sam\AppData\Roaming\reel",
                "reel's settings",
                false,
            ),
        ]
    }

    #[test]
    fn the_clips_folder_goes_only_when_it_is_a_plain_folder_of_its_own() {
        let offer = |d: &str| Clips::Offer(d.to_string());
        let keep = |d: &str, why: &str| Clips::Keep {
            dir: d.to_string(),
            why: why.to_string(),
        };
        let drive = r"its path does not start from a drive letter, such as D:\";
        let table: Vec<(&str, Clips)> = vec![
            (r"C:\Clips", offer(r"C:\Clips")),
            ("c:/clips/", offer(r"C:\Clips")),
            (r"D:\Games\Clips", offer(r"D:\Games\Clips")),
            (
                r"C:\Users\sam\Videos\reel",
                offer(r"C:\Users\sam\Videos\reel"),
            ),
            (r"X:\Clips", offer(r"C:\Clips")),
            ("", Clips::Missing),
            ("   ", Clips::Missing),
            (r"C:\Nothing", Clips::Missing),
            (r"C:\", keep(r"C:\", "it is a whole drive")),
            (r"D:\\", keep(r"D:\", "it is a whole drive")),
            ("D:/", keep(r"D:\", "it is a whole drive")),
            ("C:", keep("C:", drive)),
            ("Clips", keep("Clips", drive)),
            (r"C:Clips", keep("C:Clips", drive)),
            (r"\Clips", keep(r"\Clips", drive)),
            (r"\\?\C:\Clips", keep(r"\\?\C:\Clips", drive)),
            (r"\\.\C:\Clips", keep(r"\\.\C:\Clips", drive)),
            (
                r"\\nas\share",
                keep(r"\\nas\share", "it is a folder on the network"),
            ),
            (
                r"\\nas\share\Clips",
                keep(r"\\nas\share\Clips", "it is a folder on the network"),
            ),
            (r"\\nas", keep(r"\\nas", "it is a folder on the network")),
            (
                r"C:\Clips\..\Users",
                keep(r"C:\Clips\..\Users", "its path has . or .. in it"),
            ),
            (
                r"C:\.\Clips",
                keep(r"C:\.\Clips", "its path has . or .. in it"),
            ),
            (
                r"C:\Clips\..",
                keep(r"C:\Clips\..", "its path has . or .. in it"),
            ),
            (
                r"C:\Clips\...",
                keep(r"C:\Clips\...", "its path has . or .. in it"),
            ),
            (
                r"C:\Clips\\Valorant",
                keep(
                    r"C:\Clips\\Valorant",
                    r"its path has an empty part (\\) in it",
                ),
            ),
            (r"C:\Users", keep(r"C:\Users", "it holds your user folder")),
            (
                r"C:\Users\sam",
                keep(r"C:\Users\sam", "it is your user folder"),
            ),
            (
                r"C:\USERS\SAM\",
                keep(r"C:\USERS\SAM", "it is your user folder"),
            ),
            (
                r"C:\Users\sam\AppData",
                keep(r"C:\Users\sam\AppData", "it holds your local app data"),
            ),
            (
                r"C:\Users\sam\AppData\Local",
                keep(r"C:\Users\sam\AppData\Local", "it is your local app data"),
            ),
            (
                r"C:\Users\sam\AppData\Roaming",
                keep(r"C:\Users\sam\AppData\Roaming", "it is your app data"),
            ),
            (
                r"C:\Users\sam\AppData\Local\reel",
                keep(
                    r"C:\Users\sam\AppData\Local\reel",
                    "it is reel's install folder",
                ),
            ),
            (
                r"C:\Users\sam\AppData\Roaming\reel",
                keep(
                    r"C:\Users\sam\AppData\Roaming\reel",
                    "it is reel's settings",
                ),
            ),
            (
                r"C:\Users\sam\Downloads",
                keep(r"C:\Users\sam\Downloads", "it is your Downloads folder"),
            ),
            (
                r"C:\Users\Public",
                keep(r"C:\Users\Public", "it is the Public folder"),
            ),
            (
                r"D:\Media\Videos",
                keep(r"D:\Media\Videos", "it is your Videos folder"),
            ),
            (
                r"D:\Media",
                keep(r"D:\Media", "it holds your Videos folder"),
            ),
            (r"C:\Windows", keep(r"C:\Windows", "it is Windows")),
            (
                r"C:\Windows\System32",
                keep(r"C:\Windows\System32", "it is inside Windows"),
            ),
            (
                r"C:\Program Files",
                keep(r"C:\Program Files", "it is Program Files"),
            ),
            (
                r"C:\Program Files (x86)",
                keep(r"C:\Program Files (x86)", "it is Program Files (x86)"),
            ),
            (
                r"C:\Program Files\Common Files",
                keep(r"C:\Program Files\Common Files", "it is Common Files"),
            ),
            (
                r"C:\ProgramData",
                keep(r"C:\ProgramData", "it is ProgramData"),
            ),
            (
                r"C:\Link",
                keep(
                    r"C:\Link",
                    "it is a link or a mount point, not a folder of its own",
                ),
            ),
            (
                r"C:\Up",
                keep(
                    r"C:\Up",
                    "it is a link or a mount point, not a folder of its own",
                ),
            ),
            (
                r"C:\Via\Clips",
                keep(r"C:\Via\Clips", "it is inside Windows"),
            ),
            (r"X:\sam", keep(r"X:\sam", "it is your user folder")),
            (
                r"Z:\Clips",
                keep(r"Z:\Clips", "it is a folder on the network"),
            ),
            (r"C:\Mount\Clips", keep(r"C:\Mount\Clips", drive)),
        ];
        let (disk, places) = (disk(), fixture());
        for (raw, want) in table {
            assert_eq!(decide(raw, &places, &disk), want, "{raw:?}");
        }
    }

    #[test]
    fn the_places_name_each_folder_once_and_the_uninstallers_own_too() {
        let also = vec![
            r"C:\Users\sam\AppData\Local\reel\".to_string(),
            r"C:\Elsewhere".to_string(),
            String::new(),
        ];
        let list = places(
            r"C:\Users\sam\AppData\Local\reel",
            Path::new("/cfg/reel"),
            &also,
        );
        let of = |p: &str| {
            list.iter()
                .filter(|x| path_key(&x.path) == path_key(p))
                .count()
        };
        assert_eq!(of(r"C:\Users\sam\AppData\Local\reel"), 1);
        let install = list
            .iter()
            .find(|x| path_key(&x.path) == path_key(r"C:\Users\sam\AppData\Local\reel"))
            .unwrap();
        assert_eq!(install.what, "reel's install folder");
        assert!(list
            .iter()
            .any(|x| x.path == r"C:\Elsewhere" && x.what == "a folder of Windows' own"));
        assert!(list.iter().any(|x| x.what == "reel's settings"));
        assert!(list.iter().all(|x| !x.path.is_empty()));
    }

    #[test]
    fn a_link_inside_the_folder_is_not_followed_for_its_size() {
        let t = tempfile::tempdir().unwrap();
        let clips = t.path().join("Clips");
        let other = t.path().join("Other");
        std::fs::create_dir_all(clips.join("Valorant")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(clips.join("Valorant").join("a.mp4"), vec![0u8; 3000]).unwrap();
        std::fs::write(clips.join("b.mkv"), vec![0u8; 2000]).unwrap();
        std::fs::write(other.join("not ours.txt"), vec![0u8; 9000]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&other, clips.join("to other")).unwrap();
        assert_eq!(size(&clips), (5000, 2));
        assert!(!RealDisk.is_link(&clips.to_string_lossy()));
        #[cfg(unix)]
        assert!(RealDisk.is_link(&clips.join("to other").to_string_lossy()));
    }

    #[test]
    fn the_size_in_words() {
        assert_eq!(size_words(0, 0), "empty");
        assert_eq!(size_words(0, 1), "0 KB, 1 file");
        assert_eq!(size_words(1, 1), "1 KB, 1 file");
        assert_eq!(size_words(5000, 2), "5 KB, 2 files");
        assert_eq!(size_words(5 * 1024 * 1024 + 1, 3), "5 MB, 3 files");
        assert_eq!(
            size_words(4 * 1024 * 1024 * 1024 + 900 * 1024 * 1024, 312),
            "4.8 GB, 312 files"
        );
    }

    #[test]
    fn the_pages_line() {
        let size = |_: &str| "5 KB, 2 files".to_string();
        assert_eq!(Clips::Missing.line(size), "missing");
        assert_eq!(
            Clips::Offer(r"C:\Clips".into()).line(size),
            r"offer|C:\Clips|5 KB, 2 files"
        );
        let kept = Clips::Keep {
            dir: r"C:\Users".into(),
            why: "it holds your user folder".into(),
        };
        assert_eq!(kept.line(size), r"keep|C:\Users|it holds your user folder");
    }
}
