//! What is in the foreground: whether a fullscreen app has it, and which process it is.
use jiff::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub exe_name: String,
    pub title: String,
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
            Some(ProcessInfo { exe_name, title })
        }
    }
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
        });
        assert_eq!(
            game_name(&f.foreground_process().unwrap().exe_name, "").as_deref(),
            Some("Counter-Strike 2")
        );
        assert!(!System.fullscreen_app() || cfg!(windows));
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
