//! What is in the foreground: whether a fullscreen app has it, and which process it is.
use jiff::Timestamp;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub exe_name: String,
    pub title: String,
    pub path: PathBuf,
}

pub trait Foreground: Send + Sync {
    fn fullscreen_app(&self) -> bool;
    fn foreground_process(&self) -> Option<ProcessInfo>;
}

/// `waits` is called once if a fullscreen app is in front; whether it had to wait.
pub fn wait_while_fullscreen(
    fg: &dyn Foreground,
    every: std::time::Duration,
    waits: impl FnOnce(),
) -> bool {
    if !fg.fullscreen_app() {
        return false;
    }
    waits();
    while fg.fullscreen_app() {
        std::thread::sleep(every);
    }
    true
}

#[derive(Default)]
pub struct Fake {
    pub fullscreen: std::sync::atomic::AtomicBool,
    pub process: std::sync::Mutex<Option<ProcessInfo>>,
}

impl Foreground for Fake {
    fn fullscreen_app(&self) -> bool {
        self.fullscreen.load(std::sync::atomic::Ordering::Relaxed)
    }
    fn foreground_process(&self) -> Option<ProcessInfo> {
        self.process.lock().unwrap().clone()
    }
}

#[cfg(not(windows))]
pub struct System;

#[cfg(not(windows))]
impl Foreground for System {
    fn fullscreen_app(&self) -> bool {
        false
    }
    fn foreground_process(&self) -> Option<ProcessInfo> {
        None
    }
}

#[cfg(windows)]
pub struct System;

#[cfg(windows)]
impl Foreground for System {
    fn fullscreen_app(&self) -> bool {
        use windows::Win32::UI::Shell::{
            SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE,
            QUNS_RUNNING_D3D_FULL_SCREEN,
        };
        match unsafe { SHQueryUserNotificationState() } {
            Ok(state) => {
                state == QUNS_BUSY
                    || state == QUNS_RUNNING_D3D_FULL_SCREEN
                    || state == QUNS_PRESENTATION_MODE
            }
            Err(_) => false,
        }
    }

    fn foreground_process(&self) -> Option<ProcessInfo> {
        use windows::core::PWSTR;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
        };
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let mut title_buf = [0u16; 512];
            let n = GetWindowTextW(hwnd, &mut title_buf);
            let title = String::from_utf16_lossy(&title_buf[..n.max(0) as usize]);
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut path_buf = [0u16; 1024];
            let mut len = path_buf.len() as u32;
            let got = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                PWSTR(path_buf.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(handle);
            got.ok()?;
            let full = String::from_utf16_lossy(&path_buf[..len as usize]);
            let exe_name = full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string();
            Some(ProcessInfo {
                exe_name,
                title,
                path: PathBuf::from(full),
            })
        }
    }
}

/// A program that runs now, for Add a program.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Program {
    pub path: PathBuf,
    /// Its window's title; none on Linux.
    pub title: String,
}

/// On Windows the programs with a window; on Linux, where no window says its program, the
/// programs this user runs from outside the system's own folders. reel itself and the programs
/// reel knows are no game are left out.
pub fn running_now() -> Vec<Program> {
    let mut out = sys_running();
    out.retain(|p| {
        let stem = p
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        !stem.is_empty() && !stem.starts_with("reel") && !reel_core::games::is_not_a_game(&stem)
    });
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.path.clone()));
    out.sort_by_key(|p| name_of(&p.path, &p.title).to_lowercase());
    out
}

/// The name a program gets from its file, or its file's own name when it gives none.
pub fn name_of(path: &Path, title: &str) -> String {
    let exe = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    game_name(&exe, title).unwrap_or_else(|| {
        path.file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    })
}

#[cfg(windows)]
fn sys_running() -> Vec<Program> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, GW_OWNER,
    };
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = &mut *(lparam.0 as *mut Vec<Program>);
        if !IsWindowVisible(hwnd).as_bool()
            || GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid())
        {
            return BOOL(1);
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf) as usize;
        let title = String::from_utf16_lossy(&buf[..n]);
        if title.trim().is_empty() {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return BOOL(1);
        };
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let got =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        if got.is_ok() {
            out.push(Program {
                path: PathBuf::from(String::from_utf16_lossy(&path[..len as usize])),
                title,
            });
        }
        BOOL(1)
    }
    let mut out: Vec<Program> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut out as *mut Vec<Program> as isize));
    }
    out
}

#[cfg(not(windows))]
fn sys_running() -> Vec<Program> {
    const SYSTEM: &[&str] = &[
        "/usr/",
        "/bin/",
        "/sbin/",
        "/lib",
        "/snap/",
        "/app/",
        "/proc/",
        "/nix/store/",
    ];
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    dir.filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .filter_map(|e| std::fs::read_link(e.path().join("exe")).ok())
        .filter(|p| {
            let s = p.to_string_lossy();
            !s.ends_with(" (deleted)") && !SYSTEM.iter().any(|d| s.starts_with(d))
        })
        .map(|path| Program {
            path,
            title: String::new(),
        })
        .collect()
}

/// The program a Windows shortcut (.lnk) starts; any other file is its own program.
pub fn program_of(file: &Path) -> Result<PathBuf, String> {
    let lnk = file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("lnk"));
    if !lnk {
        return Ok(file.to_path_buf());
    }
    shortcut_target(file)
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            "reel could not read where that shortcut points: pick the program itself".into()
        })
}

#[cfg(windows)]
fn shortcut_target(file: &Path) -> Option<PathBuf> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    let file = file.to_path_buf();
    // on a thread of its own, so COM is set up there and the caller's thread stays as it was
    std::thread::spawn(move || unsafe {
        let inited = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let got = (|| {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
            let persist: IPersistFile = link.cast().ok()?;
            persist
                .Load(&HSTRING::from(file.as_os_str()), STGM_READ)
                .ok()?;
            let mut buf = [0u16; 1024];
            link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
            let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
        })();
        if inited {
            CoUninitialize();
        }
        got
    })
    .join()
    .ok()
    .flatten()
}

#[cfg(not(windows))]
fn shortcut_target(_file: &Path) -> Option<PathBuf> {
    None
}

const KNOWN: &[(&str, &str)] = &[
    ("valorant-win64-shipping", "Valorant"),
    ("valorant", "Valorant"),
    ("overwatch", "Overwatch 2"),
    ("cs2", "Counter-Strike 2"),
    ("csgo", "Counter-Strike"),
    ("r5apex", "Apex Legends"),
    ("r5apex_dx12", "Apex Legends"),
    ("fortniteclient-win64-shipping", "Fortnite"),
    ("rocketleague", "Rocket League"),
    ("league of legends", "League of Legends"),
    ("javaw", "Minecraft"),
    ("minecraft", "Minecraft"),
    ("dota2", "Dota 2"),
    ("hunt", "Hunt: Showdown"),
    ("eldenring", "Elden Ring"),
    ("helldivers2", "Helldivers 2"),
    ("marvel-win64-shipping", "Marvel Rivals"),
    ("thefinals", "The Finals"),
    ("discovery", "The Finals"),
    ("deadlock", "Deadlock"),
    ("destiny2", "Destiny 2"),
    ("rainbowsix", "Rainbow Six Siege"),
    ("rainbowsix_dx11", "Rainbow Six Siege"),
    ("rainbowsix_be", "Rainbow Six Siege"),
    ("warframe.x64", "Warframe"),
    ("gta5", "GTA V"),
    ("gta5_enhanced", "GTA V"),
    ("bf6", "Battlefield 6"),
    ("bf2042", "Battlefield 2042"),
    ("cod", "Call of Duty"),
    ("modernwarfare", "Call of Duty"),
    ("pubg", "PUBG"),
    ("tslgame", "PUBG"),
    ("escapefromtarkov", "Escape from Tarkov"),
    ("splitgate", "Splitgate"),
    ("squadgame", "Squad"),
    ("haloinfinite", "Halo Infinite"),
    ("starcitizen", "Star Citizen"),
    ("palworld-win64-shipping", "Palworld"),
    ("rust", "Rust"),
    ("rustclient", "Rust"),
    ("dayz_x64", "DayZ"),
    ("arma3_x64", "Arma 3"),
    ("hl2", "Half-Life 2"),
    ("tf_win64", "Team Fortress 2"),
    ("tf", "Team Fortress 2"),
    ("left4dead2", "Left 4 Dead 2"),
    ("factorio", "Factorio"),
    ("satisfactory-win64-shipping", "Satisfactory"),
    ("ffxiv_dx11", "Final Fantasy XIV"),
    ("wow", "World of Warcraft"),
    ("diablo iv", "Diablo IV"),
];

pub fn game_name(exe_name: &str, title: &str) -> Option<String> {
    let stem = exe_name
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(exe_name)
        .trim_end_matches(".exe")
        .trim_end_matches(".EXE")
        .trim();
    if stem.is_empty() {
        return None;
    }
    let key = stem.to_ascii_lowercase();
    if reel_core::games::is_not_a_game(&key) {
        return None;
    }
    if let Some((_, name)) = KNOWN.iter().find(|(k, _)| *k == key) {
        return Some((*name).to_string());
    }
    let mut cleaned = key.clone();
    for suffix in [
        "-win64-shipping",
        "-win32-shipping",
        "_win64_shipping",
        "-wingdk-shipping",
        "_dx12",
        "_dx11",
        "_be",
        "_x64",
        "-x64",
        "x64",
        "_retail",
        "-retail",
        "client",
    ] {
        if let Some(s) = cleaned.strip_suffix(suffix) {
            if !s.is_empty() {
                cleaned = s.to_string();
            }
        }
    }
    let words: Vec<String> = cleaned
        .split(['-', '_', ' ', '.'])
        .filter(|w| !w.is_empty())
        .map(title_case)
        .collect();
    let from_exe = words.join(" ");
    if from_exe.chars().count() < 3 {
        let t: String = title.trim().chars().take(40).collect();
        return reel_core::games::clean(Some(&t));
    }
    reel_core::games::clean(Some(&from_exe))
}

fn title_case(w: &str) -> String {
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// "Valorant, Oct 8, 21:14", in local time, with the year when it is not the current one.
pub fn clip_title(game: Option<&str>, recorded_at: Timestamp) -> String {
    let z = recorded_at.to_zoned(jiff::tz::TimeZone::system());
    reel_core::names::title_for(game, &z.datetime(), reel_core::names::this_year())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_games_odd_names_and_the_apps_that_are_not_games() {
        assert_eq!(
            game_name("VALORANT-Win64-Shipping.exe", "VALORANT").as_deref(),
            Some("Valorant")
        );
        assert_eq!(
            game_name("Overwatch.exe", "").as_deref(),
            Some("Overwatch 2")
        );
        assert_eq!(
            game_name("cs2.exe", "").as_deref(),
            Some("Counter-Strike 2")
        );
        assert_eq!(game_name("r5apex.exe", "").as_deref(), Some("Apex Legends"));
        assert_eq!(
            game_name("FortniteClient-Win64-Shipping.exe", "").as_deref(),
            Some("Fortnite")
        );
        assert_eq!(
            game_name("RocketLeague.exe", "").as_deref(),
            Some("Rocket League")
        );
        assert_eq!(
            game_name("javaw.exe", "Minecraft 1.21").as_deref(),
            Some("Minecraft")
        );
        assert_eq!(
            game_name(r"C:\Games\SomeGame-Win64-Shipping.exe", "").as_deref(),
            Some("Somegame")
        );
        assert_eq!(
            game_name("deep_rock_galactic.exe", "").as_deref(),
            Some("Deep Rock Galactic")
        );
        assert_eq!(
            game_name("xy.exe", "A Window Title").as_deref(),
            Some("A Window Title")
        );
        assert_eq!(game_name("xy.exe", ""), None);
        assert_eq!(game_name("explorer.exe", "Clips"), None);
        assert_eq!(game_name("obs64.exe", "OBS"), None);
        assert_eq!(game_name("reel-capture.exe", "OBS"), None);
        assert_eq!(game_name("Discord.exe", "Discord"), None);
        assert_eq!(game_name("zen.exe", "reel - Zen Browser"), None);
        assert_eq!(
            game_name(r"C:\Program Files\Mozilla Firefox\firefox.exe", ""),
            None
        );
        assert_eq!(game_name("Code.exe", "main.rs"), None);
        assert_eq!(game_name("ab.exe", "Zen"), None);
        assert_eq!(game_name("", ""), None);
    }

    #[test]
    fn the_title_names_the_game_and_the_moment() {
        let t = Timestamp::now();
        let z = t.to_zoned(jiff::tz::TimeZone::system());
        let month = z.strftime("%b").to_string();
        let expect = format!(
            "Valorant, {month} {}, {:02}:{:02}",
            z.day(),
            z.hour(),
            z.minute()
        );
        assert_eq!(
            clip_title(Some("Valorant"), t),
            expect,
            "this year: no year"
        );
        assert!(clip_title(None, t).starts_with(&month));
        assert!(clip_title(Some("  "), t).starts_with(&month));
        let old: Timestamp = "2024-10-24T15:40:00Z".parse().unwrap();
        assert!(
            clip_title(Some("Beat Saber"), old).contains(", 2024, "),
            "another year is named: {}",
            clip_title(Some("Beat Saber"), old)
        );
    }

    #[test]
    fn the_fake_is_settable() {
        let f = Fake::default();
        assert!(!f.fullscreen_app());
        f.fullscreen
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(f.fullscreen_app());
        *f.process.lock().unwrap() = Some(ProcessInfo {
            exe_name: "cs2.exe".into(),
            title: "Counter-Strike 2".into(),
            path: PathBuf::from(r"C:\Games\cs2.exe"),
        });
        assert_eq!(
            game_name(&f.foreground_process().unwrap().exe_name, "").as_deref(),
            Some("Counter-Strike 2")
        );
        assert!(!System.fullscreen_app() || cfg!(windows));
    }

    #[test]
    fn the_name_a_file_gives_and_the_programs_that_run_now() {
        assert_eq!(
            name_of(Path::new("/g/deep_rock_galactic.exe"), ""),
            "Deep Rock Galactic"
        );
        assert_eq!(
            name_of(Path::new("/g/xy.exe"), "Moonlit Racer"),
            "Moonlit Racer"
        );
        assert_eq!(
            name_of(Path::new("/g/xy"), ""),
            "xy",
            "its file's name when it gives none"
        );
        let now = running_now();
        assert!(
            now.iter()
                .all(|p| !p.path.to_string_lossy().starts_with("/usr/")),
            "the system's own programs stay out: {now:?}"
        );
        assert!(program_of(Path::new("/g/racer")).is_ok());
        assert!(
            program_of(Path::new("/g/Racer.lnk")).is_err() || cfg!(windows),
            "a shortcut that cannot be read says so"
        );
    }

    #[test]
    fn the_wait_lasts_while_a_fullscreen_app_is_in_front_and_says_so_once() {
        use std::sync::atomic::Ordering;
        use std::time::Duration;
        let fg = std::sync::Arc::new(Fake::default());
        let mut said = 0;
        assert!(!wait_while_fullscreen(
            &*fg,
            Duration::from_millis(5),
            || said += 1
        ));
        assert_eq!(said, 0, "nothing in front: no wait, nothing said");
        fg.fullscreen.store(true, Ordering::Relaxed);
        let off = fg.clone();
        let game = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            off.fullscreen.store(false, Ordering::Relaxed);
        });
        let t = std::time::Instant::now();
        assert!(wait_while_fullscreen(
            &*fg,
            Duration::from_millis(5),
            || said += 1
        ));
        assert!(
            t.elapsed() >= Duration::from_millis(80),
            "until the game is gone"
        );
        assert_eq!(said, 1);
        game.join().unwrap();
    }
}
