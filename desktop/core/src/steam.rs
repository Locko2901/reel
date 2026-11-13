//! Steam's installed games, read from Steam's own files on this PC: its libraries, each app's
//! manifest, and the app types in its cache `appcache/appinfo.vdf`. Nothing is sent anywhere.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteamKind {
    /// A game, a demo or a mod.
    Game,
    /// An application, or a type reel does not know: it comes in as Not sorted yet.
    Application,
    /// A program added to Steam by hand, outside its store.
    Shortcut,
}

/// One app that comes into the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamApp {
    pub app_id: u32,
    pub name: String,
    /// Its folder in a Steam library; for a shortcut, its program.
    pub path: PathBuf,
    pub kind: SteamKind,
}

/// How the apps' types were read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Types {
    AppInfo {
        version: u32,
        missing: usize,
    },
    /// Steam's cache could not be read, and why: the fallback rules decide.
    Fallback(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub dir: PathBuf,
    pub libraries: Vec<PathBuf>,
    pub apps: Vec<SteamApp>,
    /// Steam's own tools, soundtracks and other types that stay out, by name.
    pub out: Vec<String>,
    /// Apps not fully installed yet; they come in once they are.
    pub installing: usize,
    pub types: Types,
}

impl Found {
    /// The Log's line, so a read of a real library can be checked.
    pub fn log_line(&self) -> String {
        let types = match &self.types {
            Types::AppInfo {
                version,
                missing: 0,
            } => format!("appinfo.vdf version {version}"),
            Types::AppInfo { version, missing } => {
                format!("appinfo.vdf version {version}, {missing} not in it by the fallback rules")
            }
            Types::Fallback(why) => format!("the fallback rules ({why})"),
        };
        let count = |k: SteamKind| self.apps.iter().filter(|a| a.kind == k).count();
        format!(
            "Steam read in {}: {} libraries, the types from {types}; {} as Game, {} as Not sorted yet ({} of them added to Steam by hand), {} stayed out, {} still installing",
            self.dir.display(),
            self.libraries.len(),
            count(SteamKind::Game),
            count(SteamKind::Application) + count(SteamKind::Shortcut),
            count(SteamKind::Shortcut),
            self.out.len(),
            self.installing
        )
    }
}

/// Steam's folder: `REEL_STEAM_DIR` when set (for tests; a folder without Steam in it is no
/// Steam), else where Steam installs itself on this system.
pub fn locate() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("REEL_STEAM_DIR").filter(|v| !v.is_empty()) {
        let dir = PathBuf::from(dir);
        return dir.join("steamapps").is_dir().then_some(dir);
    }
    steamlocate::locate().ok().map(|s| s.path().to_path_buf())
}

/// The bit of an app manifest's `StateFlags` that says it is fully installed.
const FULLY_INSTALLED: u64 = 4;

/// Steam's own tools by app id, for when appinfo.vdf cannot say: the Steam Linux Runtimes
/// (Valve's steam-runtime README), the Proton runtimes for anti-cheat and Proton Next
/// (ProtonUp-Qt's constants), and the shared redistributables and older Proton builds
/// steamtinkerlaunch lists as no games. Proton's other builds are told by their folder.
pub const TOOLS: &[u32] = &[
    228980, 858280, 961940, 1054830, 1070560, 1113280, 1161040, 1245040, 1391110, 1420170, 1628350,
    1826330, 2230260, 4183110, 4185400,
];

pub fn read(steam: &Path) -> anyhow::Result<Found> {
    let dir = steamlocate::SteamDir::from_dir(steam)?;
    let libraries: Vec<PathBuf> = dir.library_paths()?;
    let mut manifests = Vec::new();
    let mut installing = 0;
    for lib in &libraries {
        let Ok(library) = steamlocate::Library::from_dir(lib) else {
            continue;
        };
        for app in library.apps().filter_map(Result::ok) {
            let installed = app.state_flags.is_some_and(|f| f.0 & FULLY_INSTALLED != 0);
            if installed {
                manifests.push((library.path().to_path_buf(), app));
            } else {
                installing += 1;
            }
        }
    }
    let wanted: HashSet<u32> = manifests.iter().map(|(_, a)| a.app_id).collect();
    let (types, read) = match std::fs::read(steam.join("appcache").join("appinfo.vdf")) {
        Ok(bytes) => match app_types(&bytes, &wanted) {
            Ok((version, map)) => (Some(version), Ok(map)),
            Err(why) => (None, Err(why)),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (None, Err("appinfo.vdf is not there".into()))
        }
        Err(e) => (None, Err(format!("appinfo.vdf could not be read: {e}"))),
    };
    let mut apps = Vec::new();
    let mut out = Vec::new();
    let mut missing = 0;
    for (lib, app) in manifests {
        let name = app
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| app.install_dir.clone());
        let steamapps = lib.join("steamapps");
        let folder = steamapps.join("common").join(&app.install_dir);
        let by_type = read
            .as_ref()
            .ok()
            .and_then(|m| m.get(&app.app_id))
            .map(|t| kind_of_type(t));
        if read.is_ok() && by_type.is_none() {
            missing += 1;
        }
        let kind = by_type.unwrap_or_else(|| {
            fallback(
                app.app_id,
                &name,
                &app.install_dir,
                &folder,
                &steamapps.join("music").join(&app.install_dir),
            )
        });
        match kind {
            Some(kind) => apps.push(SteamApp {
                app_id: app.app_id,
                name,
                path: as_seen(&folder),
                kind,
            }),
            None => out.push(name),
        }
    }
    if let Ok(shortcuts) = dir.shortcuts() {
        for s in shortcuts.filter_map(Result::ok) {
            let exe = s.executable.trim().trim_matches('"');
            if exe.is_empty() || !Path::new(exe).is_absolute() {
                continue;
            }
            apps.push(SteamApp {
                app_id: s.app_id,
                name: s.app_name.trim().to_string(),
                path: as_seen(Path::new(exe)),
                kind: SteamKind::Shortcut,
            });
        }
    }
    let types = match (types, read) {
        (Some(version), Ok(_)) => Types::AppInfo { version, missing },
        (_, Err(why)) => Types::Fallback(why),
        (None, Ok(_)) => Types::Fallback("no version".into()),
    };
    Ok(Found {
        dir: steam.to_path_buf(),
        libraries,
        apps,
        out,
        installing,
        types,
    })
}

/// The path a running program has: on Linux through no link, as `/proc/<pid>/exe` says it.
fn as_seen(p: &Path) -> PathBuf {
    if cfg!(windows) {
        p.to_path_buf()
    } else {
        std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
    }
}

/// None stays out.
fn kind_of_type(t: &str) -> Option<SteamKind> {
    match t {
        "game" | "demo" | "mod" => Some(SteamKind::Game),
        "tool" | "config" | "music" | "video" | "dlc" | "series" | "episode" | "media"
        | "hardware" | "advertising" | "driver" => None,
        _ => Some(SteamKind::Application),
    }
}

/// Without the type: a tool by its id, Proton by its folder and a soundtrack by where its
/// files are stay out; the rest is a game, which with the defaults changes nothing.
fn fallback(
    id: u32,
    name: &str,
    install_dir: &str,
    common: &Path,
    music: &Path,
) -> Option<SteamKind> {
    let proton = |s: &str| s.starts_with("Proton") || s.starts_with("SteamLinuxRuntime");
    if TOOLS.contains(&id) || proton(install_dir) || proton(name) {
        return None;
    }
    if !common.is_dir() && music.is_dir() {
        return None;
    }
    Some(SteamKind::Game)
}

/// The `common.type` of the wanted apps in appinfo.vdf, lowercased, and the file's version.
pub fn app_types(
    bytes: &[u8],
    wanted: &HashSet<u32>,
) -> Result<(u32, HashMap<u32, String>), String> {
    let mut r = Reader { b: bytes, at: 0 };
    let magic = r.u32().ok_or("appinfo.vdf is empty")?;
    if magic >> 8 != 0x07_5644 {
        return Err("appinfo.vdf is not one".into());
    }
    let version = magic & 0xff;
    if !(40..=41).contains(&version) {
        return Err(format!(
            "appinfo.vdf is of version {version}, which reel cannot read"
        ));
    }
    r.u32().ok_or("appinfo.vdf ends early")?;
    let strings = if version >= 41 {
        let offset = r.i64().ok_or("appinfo.vdf ends early")?;
        Some(string_table(bytes, offset).ok_or("appinfo.vdf's string table is broken")?)
    } else {
        None
    };
    let mut out = HashMap::new();
    loop {
        let id = r.u32().ok_or("appinfo.vdf ends early")?;
        if id == 0 {
            break;
        }
        let size = r.u32().ok_or("appinfo.vdf ends early")? as usize;
        let end = r.at.checked_add(size).filter(|e| *e <= bytes.len());
        let end = end.ok_or("appinfo.vdf ends early")?;
        // info state, last updated, PICS token, the text's SHA-1, change number, the binary's SHA-1
        const HEAD: usize = 4 + 4 + 8 + 20 + 4 + 20;
        if wanted.contains(&id) && size > HEAD {
            if let Some(t) = type_in(&bytes[r.at + HEAD..end], strings.as_deref()) {
                out.insert(id, t);
            }
        }
        r.at = end;
    }
    Ok((version, out))
}

fn string_table(bytes: &[u8], offset: i64) -> Option<Vec<String>> {
    let mut r = Reader {
        b: bytes,
        at: usize::try_from(offset).ok()?,
    };
    let n = r.u32()?;
    let mut out = Vec::with_capacity((n as usize).min(1 << 20));
    for _ in 0..n {
        out.push(r.cstr()?);
    }
    Some(out)
}

/// `appinfo.common.type` in one app's binary KeyValues.
fn type_in(vdf: &[u8], strings: Option<&[String]>) -> Option<String> {
    let mut r = Reader { b: vdf, at: 0 };
    let mut path: Vec<String> = Vec::new();
    loop {
        let t = r.u8()?;
        if t == 0x08 || t == 0x0b {
            path.pop()?;
            continue;
        }
        let key = match strings {
            Some(s) => s.get(usize::try_from(r.i32()?).ok()?)?.clone(),
            None => r.cstr()?,
        };
        match t {
            0x00 => path.push(key),
            0x01 => {
                let value = r.cstr()?;
                let at = |i: usize, w: &str| path.get(i).is_some_and(|p| p.eq_ignore_ascii_case(w));
                if path.len() == 2
                    && at(0, "appinfo")
                    && at(1, "common")
                    && key.eq_ignore_ascii_case("type")
                {
                    return Some(value.trim().to_lowercase());
                }
            }
            0x02 | 0x03 | 0x04 | 0x06 => r.skip(4)?,
            0x07 | 0x0a => r.skip(8)?,
            0x05 => r.wide()?,
            _ => return None,
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4)
            .map(|s| i32::from_le_bytes(s.try_into().unwrap()))
    }
    fn i64(&mut self) -> Option<i64> {
        self.take(8)
            .map(|s| i64::from_le_bytes(s.try_into().unwrap()))
    }
    fn cstr(&mut self) -> Option<String> {
        let rest = self.b.get(self.at..)?;
        let n = rest.iter().position(|&c| c == 0)?;
        let s = String::from_utf8_lossy(&rest[..n]).into_owned();
        self.at += n + 1;
        Some(s)
    }
    fn wide(&mut self) -> Option<()> {
        loop {
            if self.take(2)? == [0, 0] {
                return Some(());
            }
        }
    }
}

/// The folders whose change means Steam installed or removed something: each library's
/// `steamapps` (its manifests) and Steam's own, which holds `libraryfolders.vdf`.
pub fn watched(found: &Found) -> Vec<PathBuf> {
    let mut dirs = vec![found.dir.join("steamapps")];
    for lib in &found.libraries {
        let d = lib.join("steamapps");
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    dirs.retain(|d| d.is_dir());
    dirs
}

/// Sends once a few seconds after the last change in these folders, until cancelled.
pub async fn watch(
    dirs: Vec<PathBuf>,
    quiet: Duration,
    tx: mpsc::UnboundedSender<()>,
    cancel: CancellationToken,
) {
    use notify::Watcher;
    let (wake_tx, mut wake) = mpsc::unbounded_channel::<()>();
    let made = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        if res.is_ok() {
            let _ = wake_tx.send(());
        }
    });
    let Ok(mut w) = made else {
        return;
    };
    for d in &dirs {
        if let Err(e) = w.watch(d, notify::RecursiveMode::NonRecursive) {
            tracing::warn!(dir = %d.display(), error = %e, "Steam's folder is not watched");
        }
    }
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            got = wake.recv() => if got.is_none() { return },
        }
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(quiet) => break,
                got = wake.recv() => if got.is_none() { return },
            }
        }
        if tx.send(()).is_err() {
            return;
        }
    }
}

#[cfg(test)]
pub mod made_up {
    //! A Steam folder of made up apps, for the tests here and the app's.
    use std::path::Path;

    pub struct App<'a> {
        pub id: u32,
        pub name: &'a str,
        pub dir: &'a str,
        /// Its `common.type` in appinfo.vdf; None leaves it out of the file.
        pub kind: Option<&'a str>,
        pub installed: bool,
        /// Its files under `steamapps/music`, as a soundtrack's are.
        pub music: bool,
    }

    pub fn app<'a>(id: u32, name: &'a str, kind: Option<&'a str>) -> App<'a> {
        App {
            id,
            name,
            dir: name,
            kind,
            installed: true,
            music: false,
        }
    }

    /// Steam in `steam`, its first library there and a second in `second`.
    pub fn steam(steam: &Path, second: &Path, first: &[App], other: &[App]) {
        let vdf = format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
            steam.display(),
            second.display()
        );
        for (lib, apps) in [(steam, first), (second, other)] {
            let sa = lib.join("steamapps");
            std::fs::create_dir_all(sa.join("common")).unwrap();
            for a in apps {
                let flags = if a.installed { 4 } else { 1026 };
                std::fs::write(
                    sa.join(format!("appmanifest_{}.acf", a.id)),
                    format!(
                        "\"AppState\"\n{{\n\t\"appid\"\t\t\"{}\"\n\t\"name\"\t\t\"{}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"installdir\"\t\t\"{}\"\n}}\n",
                        a.id, a.name, a.dir
                    ),
                )
                .unwrap();
                let at = if a.music { "music" } else { "common" };
                std::fs::create_dir_all(sa.join(at).join(a.dir)).unwrap();
            }
        }
        std::fs::write(steam.join("steamapps").join("libraryfolders.vdf"), vdf).unwrap();
    }

    /// Programs added to Steam by hand, in one user's `shortcuts.vdf`, as Steam writes it.
    pub fn shortcuts(steam: &Path, items: &[(u32, &str, &str)]) {
        let mut b = b"\x00shortcuts\x00".to_vec();
        for (i, (id, name, exe)) in items.iter().enumerate() {
            b.push(0x00);
            b.extend(i.to_string().as_bytes());
            b.push(0);
            b.extend(b"\x02appid\x00");
            b.extend(id.to_le_bytes());
            b.extend(b"\x01AppName\x00");
            b.extend(name.as_bytes());
            b.extend(b"\x00\x01Exe\x00\"");
            b.extend(exe.as_bytes());
            b.extend(b"\"\x00\x01StartDir\x00\x00\x00tags\x00\x08\x08");
        }
        b.extend(b"\x08\x08");
        let dir = steam.join("userdata").join("123123123").join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("shortcuts.vdf"), b).unwrap();
    }

    /// appinfo.vdf of `version` (40 or 41, or any other number for an unknown one).
    pub fn appinfo(version: u32, apps: &[(u32, &str)]) -> Vec<u8> {
        let mut strings: Vec<String> = Vec::new();
        let mut key = |out: &mut Vec<u8>, k: &str| {
            if version >= 41 {
                let i = strings.iter().position(|s| s == k).unwrap_or_else(|| {
                    strings.push(k.to_string());
                    strings.len() - 1
                });
                out.extend((i as i32).to_le_bytes());
            } else {
                out.extend(k.as_bytes());
                out.push(0);
            }
        };
        let mut body = Vec::new();
        for (id, kind) in apps {
            let mut vdf = Vec::new();
            vdf.push(0x00);
            key(&mut vdf, "appinfo");
            vdf.push(0x02);
            key(&mut vdf, "appid");
            vdf.extend((*id as i32).to_le_bytes());
            vdf.push(0x00);
            key(&mut vdf, "common");
            vdf.push(0x01);
            key(&mut vdf, "name");
            vdf.extend(b"x\0");
            vdf.push(0x07);
            key(&mut vdf, "gameid");
            vdf.extend(u64::from(*id).to_le_bytes());
            vdf.push(0x01);
            key(&mut vdf, "type");
            vdf.extend(kind.as_bytes());
            vdf.push(0);
            vdf.push(0x08);
            vdf.push(0x00);
            key(&mut vdf, "extended");
            vdf.push(0x01);
            key(&mut vdf, "type");
            vdf.extend(b"not this one\0");
            vdf.push(0x08);
            vdf.push(0x08);
            vdf.push(0x08);
            body.extend(id.to_le_bytes());
            body.extend(((60 + vdf.len()) as u32).to_le_bytes());
            body.extend([0u8; 60]);
            body.extend(vdf);
        }
        body.extend(0u32.to_le_bytes());
        let mut out = Vec::new();
        out.extend((0x0756_4400 | version).to_le_bytes());
        out.extend(1u32.to_le_bytes());
        if version >= 41 {
            let at = 8 + 8 + body.len();
            out.extend((at as i64).to_le_bytes());
        }
        out.extend(body);
        if version >= 41 {
            out.extend((strings.len() as u32).to_le_bytes());
            for s in &strings {
                out.extend(s.as_bytes());
                out.push(0);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::made_up::{app, appinfo, shortcuts, steam, App};
    use super::*;

    fn library() -> (tempfile::TempDir, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let s = t.path().join("Steam");
        let second = t.path().join("SteamLibrary");
        steam(
            &s,
            &second,
            &[
                app(10, "Cinder Road", Some("Game")),
                app(1628350, "Steam Linux Runtime 3.0 (sniper)", Some("Tool")),
                App {
                    music: true,
                    ..app(30, "Cinder Road Soundtrack", Some("Music"))
                },
            ],
            &[
                app(40, "Desk Painter", Some("Application")),
                app(50, "Lantern Bay Demo", Some("demo")),
                App {
                    installed: false,
                    ..app(60, "Half There", Some("Game"))
                },
                App {
                    dir: "Proton 9.0",
                    ..app(70, "Proton 9.0", Some("Tool"))
                },
                app(80, "Tidewater", None),
                app(90, "Odd One", Some("Beta")),
            ],
        );
        (t, s)
    }

    fn names(f: &Found, k: SteamKind) -> Vec<String> {
        let mut n: Vec<String> = f
            .apps
            .iter()
            .filter(|a| a.kind == k)
            .map(|a| a.name.clone())
            .collect();
        n.sort();
        n
    }

    #[test]
    fn the_types_from_appinfo_40_and_41() {
        for version in [40, 41] {
            let (_t, s) = library();
            std::fs::create_dir_all(s.join("appcache")).unwrap();
            let kinds = [
                (10, "Game"),
                (1628350, "Tool"),
                (30, "Music"),
                (40, "Application"),
                (50, "demo"),
                (60, "Game"),
                (70, "Tool"),
                (90, "Beta"),
            ];
            std::fs::write(
                s.join("appcache").join("appinfo.vdf"),
                appinfo(version, &kinds),
            )
            .unwrap();
            let f = read(&s).unwrap();
            assert_eq!(
                f.types,
                Types::AppInfo {
                    version,
                    missing: 1
                },
                "Tidewater is not in it"
            );
            assert_eq!(f.libraries.len(), 2);
            assert_eq!(
                names(&f, SteamKind::Game),
                ["Cinder Road", "Lantern Bay Demo", "Tidewater"],
                "version {version}: a game, a demo, and one not in the file by the fallback"
            );
            assert_eq!(
                names(&f, SteamKind::Application),
                ["Desk Painter", "Odd One"]
            );
            let mut out = f.out.clone();
            out.sort();
            assert_eq!(
                out,
                [
                    "Cinder Road Soundtrack",
                    "Proton 9.0",
                    "Steam Linux Runtime 3.0 (sniper)"
                ]
            );
            assert_eq!(f.installing, 1, "Half There is not fully installed");
            let cinder = f.apps.iter().find(|a| a.app_id == 10).unwrap();
            assert!(cinder.path.ends_with("steamapps/common/Cinder Road"));
            assert!(
                f.log_line()
                    .contains(&format!("appinfo.vdf version {version}")),
                "{}",
                f.log_line()
            );
            assert!(
                f.log_line().contains("3 as Game, 2 as Not sorted yet"),
                "{}",
                f.log_line()
            );
        }
    }

    #[test]
    fn an_unknown_version_or_no_appinfo_falls_back_to_the_rules() {
        for (bytes, why) in [
            (
                Some(appinfo(42, &[(10, "Game"), (40, "Application")])),
                "version 42",
            ),
            (Some(b"not appinfo at all".to_vec()), "is not one"),
            (
                Some(appinfo(40, &[(10, "Game")])[..30].to_vec()),
                "ends early",
            ),
            (None, "is not there"),
        ] {
            let (_t, s) = library();
            if let Some(b) = bytes {
                std::fs::create_dir_all(s.join("appcache")).unwrap();
                std::fs::write(s.join("appcache").join("appinfo.vdf"), b).unwrap();
            }
            let f = read(&s).unwrap();
            match &f.types {
                Types::Fallback(w) => assert!(w.contains(why), "{w}"),
                other => panic!("{other:?}"),
            }
            assert_eq!(
                names(&f, SteamKind::Game),
                [
                    "Cinder Road",
                    "Desk Painter",
                    "Lantern Bay Demo",
                    "Odd One",
                    "Tidewater"
                ],
                "without the types the rest comes in as Game"
            );
            let mut out = f.out.clone();
            out.sort();
            assert_eq!(
                out,
                [
                    "Cinder Road Soundtrack",
                    "Proton 9.0",
                    "Steam Linux Runtime 3.0 (sniper)"
                ],
                "the tool by its id, Proton by its folder, the soundtrack by where its files are"
            );
            assert!(
                f.log_line().contains("the fallback rules"),
                "{}",
                f.log_line()
            );
        }
    }

    #[test]
    fn a_program_added_to_steam_by_hand_comes_in_as_its_program() {
        let (t, s) = library();
        let exe = t
            .path()
            .join("GOG Games")
            .join("Moonlit Racer")
            .join("racer_x64");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "").unwrap();
        shortcuts(
            &s,
            &[
                (3_000_000_001, "Moonlit Racer", &exe.display().to_string()),
                (7, "No path", "racer"),
            ],
        );
        let f = read(&s).unwrap();
        let cut: Vec<&SteamApp> = f
            .apps
            .iter()
            .filter(|a| a.kind == SteamKind::Shortcut)
            .collect();
        assert_eq!(cut.len(), 1, "one with no full path is left out: {cut:?}");
        assert_eq!(cut[0].name, "Moonlit Racer");
        assert_eq!(cut[0].path, std::fs::canonicalize(&exe).unwrap());
        assert!(
            f.log_line().contains("1 of them added to Steam by hand"),
            "{}",
            f.log_line()
        );
    }

    #[test]
    fn a_short_or_broken_entry_is_refused_not_read_past() {
        let mut b = appinfo(41, &[(10, "Game")]);
        b[16 + 4..16 + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(app_types(&b, &HashSet::from([10])).is_err());
        let mut b = appinfo(41, &[(10, "Game")]);
        let n = b.len();
        b[8..16].copy_from_slice(&(n as i64 + 100).to_le_bytes());
        assert!(app_types(&b, &HashSet::from([10])).is_err());
        let (v, m) = app_types(
            &appinfo(40, &[(10, "Game"), (11, "Tool")]),
            &HashSet::from([11]),
        )
        .unwrap();
        assert_eq!(
            (v, m.len(), m.get(&11).map(String::as_str)),
            (40, 1, Some("tool")),
            "only the wanted"
        );
    }

    #[test]
    fn steam_is_found_from_the_override_and_watched_where_it_changes() {
        let (t, s) = library();
        std::env::set_var("REEL_STEAM_DIR", &s);
        assert_eq!(locate(), Some(s.clone()));
        std::env::set_var("REEL_STEAM_DIR", t.path().join("none"));
        assert_eq!(locate(), None, "a folder without Steam");
        std::env::remove_var("REEL_STEAM_DIR");
        let f = read(&s).unwrap();
        let w = watched(&f);
        assert_eq!(w.len(), 2);
        assert!(w.iter().all(|d| d.ends_with("steamapps")));
    }

    #[tokio::test]
    async fn a_change_is_said_once_after_the_quiet() {
        let t = tempfile::tempdir().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let task = tokio::spawn(watch(
            vec![t.path().to_path_buf()],
            Duration::from_millis(300),
            tx,
            cancel.clone(),
        ));
        tokio::time::sleep(Duration::from_millis(200)).await;
        for i in 0..3 {
            std::fs::write(t.path().join(format!("appmanifest_{i}.acf")), "x").unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(600), rx.recv())
                .await
                .is_err(),
            "once"
        );
        cancel.cancel();
        task.await.unwrap();
    }
}
