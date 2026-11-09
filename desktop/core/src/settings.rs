//! Every field has a default, so a settings file from an older version still loads.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Connected: the clips go to a reel server; standalone: the library is on this PC.
    pub mode: Mode,
    pub standalone: StandaloneSettings,
    /// https only; plain http only for localhost, so a token never crosses a network in clear.
    pub server_url: String,
    pub device_name: String,
    pub obs: ObsSettings,
    /// While OBS is connected the clips folder is its recording folder; off, `clips_dir` is.
    pub clips_follow_obs: bool,
    pub clips_dir: PathBuf,
    pub extensions: Vec<String>,
    /// A file counts as a clip once its size has held still this long and it opens.
    pub stable_seconds: u64,
    pub upload: UploadSettings,
    pub cache: CacheSettings,
    pub hotkeys: HotkeySettings,
    pub sounds: SoundSettings,
    pub startup: StartupSettings,
    /// In colour, or the mark alone in one colour for a tray whose icons are all one colour.
    pub tray_icon: TrayIcon,
    pub updates: UpdateMode,
    /// The version that last ran, so the start after an update can say from which.
    pub last_version: String,
    /// The app version that last asked to update the copy of OBS; only another version asks again.
    pub copy_update_asked: String,
    pub log_level: String,
    /// WebView2's own audio process, for a PC where the default fails; a start with
    /// `--webview-audio=service` turns it on and `=browser` off.
    pub webview_audio_fallback: bool,
}

/// Where WebView2 plays the window's sound on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WebviewAudio {
    /// The browser process, a child of the app, where a capture of the app's window finds it.
    #[default]
    Browser,
    /// Chromium's own audio process, a child of the browser process.
    Service,
}

impl WebviewAudio {
    pub fn word(&self) -> &'static str {
        match self {
            WebviewAudio::Browser => "browser",
            WebviewAudio::Service => "service",
        }
    }

    pub fn from_args(args: &[String]) -> Option<WebviewAudio> {
        args.iter()
            .find_map(|a| match a.strip_prefix("--webview-audio=")? {
                "browser" => Some(WebviewAudio::Browser),
                "service" => Some(WebviewAudio::Service),
                _ => None,
            })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrayIcon {
    White,
    Black,
    /// An icon of a later version reads as this one.
    #[default]
    #[serde(other)]
    Colour,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateMode {
    /// Downloads at once and installs when the app quits.
    OnQuit,
    Off,
    /// Says so in a notification, the tray and the Status page; installs on the click. A mode
    /// of a later version reads as this one: nothing installs without the click.
    #[default]
    #[serde(other)]
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ObsSettings {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub password: String,
    /// When OBS is open and its replay buffer is off, the app enables it and starts it.
    pub start_replay: bool,
    /// The app runs the OBS that came with it instead of connecting to the one above.
    pub bundled: bool,
    pub setup: crate::bundled::ObsSetup,
    /// The setup was skipped for now: the app opens on its pages.
    pub setup_later: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Connected,
    Standalone,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct StandaloneSettings {
    /// auto, nvenc, amf, qsv, vaapi (Linux alone) or none.
    pub hwaccel: String,
    /// The library is kept under this many GB, the oldest clips first; 0 is no cap.
    pub cap_gb: u64,
    /// The cap never trims a clip that is not on a server too.
    pub cap_keeps_unsent: bool,
    /// Size-limited copies on the GPU encoder instead of two-pass x264.
    pub copies_on_hardware: bool,
    /// reel on this PC's whole folder when the app moved it; empty is `library` in the data
    /// folder. Only a move or the use of another library sets it.
    pub library_dir: PathBuf,
}

impl Default for StandaloneSettings {
    fn default() -> Self {
        Self {
            hwaccel: "auto".into(),
            cap_gb: 100,
            cap_keeps_unsent: true,
            copies_on_hardware: true,
            library_dir: PathBuf::new(),
        }
    }
}

impl Settings {
    /// The folder reel on this PC runs from: the one the app moved it to, else its origin.
    pub fn library_dir(&self, paths: &Paths) -> PathBuf {
        if self.standalone.library_dir.as_os_str().is_empty() {
            paths.library_dir()
        } else {
            self.standalone.library_dir.clone()
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum UploadMode {
    #[default]
    Auto,
    Hotkey,
    /// A mode of a later version reads as this one: no clip goes up that the user did not send.
    #[serde(other)]
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UploadSettings {
    pub mode: UploadMode,
    /// 0 is unlimited.
    pub bandwidth_kbit: u64,
    pub pause_in_fullscreen: bool,
    /// Off by default: an upload on the LAN still costs the game disk and CPU time.
    pub pause_only_on_wan: bool,
    /// 0 takes the server's chunk size.
    pub chunk_mb: u64,
    /// Seconds: the server publishes each clip with its last seconds as the current version,
    /// the whole file kept as the original; 0 keeps the whole clip.
    pub keep_last: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CacheSettings {
    /// 0 deletes a clip right after the server confirmed it.
    pub gb: u64,
    pub days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HotkeySettings {
    pub share_last: String,
    /// OBS saves the replay, the app uploads it and copies its link.
    pub save_share: String,
    /// In the Hotkey upload mode, the last found clip goes up when it is pressed.
    pub upload_last: String,
    /// OBS saves the replay, and it goes up as any clip does.
    pub save: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    ShareLast,
    SaveShare,
    UploadLast,
    Save,
}

/// One hotkey: its setting as the pages name it, its name, what it does and its combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey<'a> {
    pub key: &'static str,
    pub label: &'static str,
    pub action: HotkeyAction,
    pub combo: &'a str,
}

impl HotkeySettings {
    /// Every hotkey there is, in the order they register: the one list the registration and
    /// the recorder's check both go by.
    pub fn all(&self) -> [Hotkey<'_>; 4] {
        [
            Hotkey {
                key: "hotkeys.share_last",
                label: "Share the last clip",
                action: HotkeyAction::ShareLast,
                combo: &self.share_last,
            },
            Hotkey {
                key: "hotkeys.save_share",
                label: "Save and share",
                action: HotkeyAction::SaveShare,
                combo: &self.save_share,
            },
            Hotkey {
                key: "hotkeys.upload_last",
                label: "Upload the last clip",
                action: HotkeyAction::UploadLast,
                combo: &self.upload_last,
            },
            Hotkey {
                key: "hotkeys.save",
                label: "Save a clip",
                action: HotkeyAction::Save,
                combo: &self.save,
            },
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CueSettings {
    pub enabled: bool,
    pub volume: f32,
    /// A WAV file of the user's choice; None plays the bundled cue.
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SoundSettings {
    pub captured: CueSettings,
    pub uploaded: CueSettings,
    pub link_copied: CueSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct StartupSettings {
    pub autostart: bool,
    /// Start through a scheduled task with the highest privileges, for hotkeys while an
    /// elevated game has the focus.
    pub elevated: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Connected,
            standalone: StandaloneSettings::default(),
            server_url: String::new(),
            device_name: default_device_name(),
            obs: ObsSettings::default(),
            clips_follow_obs: true,
            clips_dir: default_clips_dir(),
            extensions: ["mkv", "mp4", "mov"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            stable_seconds: 5,
            upload: UploadSettings::default(),
            cache: CacheSettings::default(),
            hotkeys: HotkeySettings::default(),
            sounds: SoundSettings::default(),
            startup: StartupSettings::default(),
            tray_icon: TrayIcon::Colour,
            updates: UpdateMode::Ask,
            last_version: String::new(),
            copy_update_asked: String::new(),
            log_level: "info".into(),
            webview_audio_fallback: false,
        }
    }
}

impl Default for ObsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".into(),
            port: 4455,
            password: String::new(),
            start_replay: true,
            bundled: false,
            setup: crate::bundled::ObsSetup::default(),
            setup_later: false,
        }
    }
}

impl Default for UploadSettings {
    fn default() -> Self {
        Self {
            mode: UploadMode::Auto,
            bandwidth_kbit: 0,
            pause_in_fullscreen: true,
            pause_only_on_wan: false,
            chunk_mb: 0,
            keep_last: 0,
        }
    }
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self { gb: 5, days: 14 }
    }
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            share_last: "Ctrl+Alt+S".into(),
            save_share: "Ctrl+Alt+Shift+S".into(),
            upload_last: "Ctrl+Alt+U".into(),
            save: String::new(),
        }
    }
}

impl Default for CueSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 1.0,
            file: None,
        }
    }
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            captured: CueSettings::default(),
            uploaded: CueSettings::default(),
            link_copied: CueSettings {
                enabled: true,
                volume: 0.5,
                file: None,
            },
        }
    }
}

impl Default for StartupSettings {
    fn default() -> Self {
        Self {
            autostart: true,
            elevated: false,
        }
    }
}

pub fn default_device_name() -> String {
    gethostname::gethostname()
        .to_string_lossy()
        .trim()
        .to_string()
        .chars()
        .take(40)
        .collect::<String>()
        .trim()
        .to_string()
        .replace(|c: char| c.is_control(), "")
}

pub fn default_clips_dir() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\Clips")
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Clips")
    }
}

/// REEL_DESKTOP_DIR puts both folders under one, for tests and for a run on another machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }
    pub fn ledger_file(&self) -> PathBuf {
        self.data_dir.join("ledger.json")
    }
    /// What the app knows of this PC's encoders for OBS.
    pub fn encoders_file(&self) -> PathBuf {
        self.data_dir.join("encoders.json")
    }
    /// Standalone: reel's data folder (its library, cache, trash and database) where it starts
    /// out; `Settings::library_dir` says where it is.
    pub fn library_dir(&self) -> PathBuf {
        self.data_dir.join("library")
    }
    pub fn cues_dir(&self) -> PathBuf {
        self.data_dir.join("cues")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }
    pub fn ensure(&self) -> std::io::Result<()> {
        for d in [
            &self.config_dir,
            &self.data_dir,
            &self.cues_dir(),
            &self.logs_dir(),
        ] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }
}

pub fn paths() -> Paths {
    if let Some(root) = std::env::var_os("REEL_DESKTOP_DIR").filter(|v| !v.is_empty()) {
        let root = PathBuf::from(root);
        return Paths {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
        };
    }
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("reel");
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("reel");
    Paths {
        config_dir,
        data_dir,
    }
}

/// The server URL rule: https anywhere, plain http only on this machine.
pub fn check_server_url(raw: &str) -> Result<Url, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("no server URL yet".into());
    }
    let url = Url::parse(raw).map_err(|_| format!("{raw} is not a URL"))?;
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    match url.scheme() {
        "https" => {}
        "http" => {
            if !matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]") {
                return Err("http only for localhost; a server on the network needs https".into());
            }
        }
        other => return Err(format!("{other}: a server URL starts with https://")),
    }
    if host.is_empty() {
        return Err("the URL has no host".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("a server URL has no query or fragment".into());
    }
    Ok(url)
}

pub fn server_base(raw: &str) -> Result<String, String> {
    let url = check_server_url(raw)?;
    Ok(url.as_str().trim_end_matches('/').to_string())
}

impl Settings {
    pub fn webview_audio(&self) -> WebviewAudio {
        if self.webview_audio_fallback {
            WebviewAudio::Service
        } else {
            WebviewAudio::Browser
        }
    }

    pub fn load(path: &Path) -> anyhow::Result<Settings> {
        Ok(Settings::load_or_fresh(path)?.0)
    }

    /// A new install starts standalone; a settings file without `mode` loads as connected.
    pub fn fresh() -> Settings {
        Settings {
            mode: Mode::Standalone,
            ..Settings::default()
        }
    }

    /// True when there was no settings file.
    pub fn load_or_fresh(path: &Path) -> anyhow::Result<(Settings, bool)> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok((serde_json::from_str(&text)?, false)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Settings::fresh(), true)),
            Err(e) => Err(e.into()),
        }
    }

    /// Written as a part file and renamed into place, so a crash leaves the old file.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part = part_path(path);
        std::fs::write(&part, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&part, path)?;
        Ok(())
    }

    pub fn accepts(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| self.extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
            .unwrap_or(false)
    }
}

pub fn part_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".part");
    PathBuf::from(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_url_rule() {
        assert!(check_server_url("https://clips.example.org").is_ok());
        assert!(check_server_url("https://clips.example.org/").is_ok());
        assert!(check_server_url("http://localhost:8747").is_ok());
        assert!(check_server_url("http://127.0.0.1:8747/").is_ok());
        assert!(check_server_url("http://198.51.100.55:8747").is_err());
        assert!(check_server_url("http://clips.example.org").is_err());
        assert!(check_server_url("ftp://clips.example.org").is_err());
        assert!(check_server_url("clips.example.org").is_err());
        assert!(check_server_url("").is_err());
        assert!(check_server_url("https://clips.example.org/?x=1").is_err());
        assert_eq!(
            server_base("https://clips.example.org/").unwrap(),
            "https://clips.example.org"
        );
        assert_eq!(
            server_base("http://127.0.0.1:8747").unwrap(),
            "http://127.0.0.1:8747"
        );
    }

    fn page_keys(kind: Option<&str>) -> Vec<String> {
        let mut keys = Vec::new();
        for page in [
            include_str!("../../ui/index.html"),
            include_str!("../../ui/setup.html"),
        ] {
            for tag in page.split('<').filter(|t| t.contains("data-key=\"")) {
                let attr = |name: &str| {
                    tag.split(&format!(" {name}=\""))
                        .nth(1)
                        .and_then(|v| v.split('"').next())
                        .map(str::to_string)
                };
                if kind.is_none() || attr("data-kind").as_deref() == kind {
                    keys.extend(attr("data-key"));
                }
            }
        }
        keys
    }

    #[test]
    fn every_hotkey_row_of_the_pages_is_a_hotkey_the_app_knows() {
        let s = HotkeySettings::default();
        let known: Vec<&str> = s.all().iter().map(|h| h.key).collect();
        let rows = page_keys(Some("hotkey"));
        assert!(rows.len() >= 6, "the rows of both pages: {rows:?}");
        for row in &rows {
            assert!(known.contains(&row.as_str()), "{row} is not in the list");
        }
        let fields = serde_json::to_value(&s).unwrap();
        let fields = fields.as_object().unwrap();
        assert_eq!(fields.len(), known.len(), "one entry a setting");
        for f in fields.keys() {
            assert!(known.contains(&format!("hotkeys.{f}").as_str()), "{f}");
        }
        for h in s.all() {
            assert_eq!(fields[&h.key["hotkeys.".len()..]], h.combo, "{}", h.key);
        }
    }

    #[test]
    fn every_setting_a_page_sends_is_one_the_app_keeps() {
        let s = serde_json::to_value(Settings::default()).unwrap();
        let keys = page_keys(None);
        assert!(keys.len() > 40, "{keys:?}");
        for key in keys {
            let at = key
                .split('.')
                .try_fold(&s, |v, part| v.as_object().and_then(|o| o.get(part)));
            assert!(at.is_some(), "{key} is not a setting");
        }
    }

    #[test]
    fn every_choice_a_page_offers_is_one_the_settings_take() {
        let base = serde_json::to_value(Settings::default()).unwrap();
        let mut seen = 0;
        for page in [
            include_str!("../../ui/index.html"),
            include_str!("../../ui/setup.html"),
        ] {
            let mut row: Option<String> = None;
            for tag in page.split('<') {
                let value = |name: &str| {
                    tag.split(&format!(" {name}=\""))
                        .nth(1)
                        .and_then(|v| v.split('"').next())
                        .map(str::to_string)
                };
                if tag.starts_with("div ") && tag.contains("class=\"srow\"") {
                    row = value("data-key")
                        .filter(|_| value("data-kind").as_deref() == Some("select"));
                } else if let (Some(key), true) = (&row, tag.starts_with("option ")) {
                    let choice = value("value").unwrap();
                    let mut v = base.clone();
                    let mut at = &mut v;
                    for part in key.split('.') {
                        at = &mut at[part];
                    }
                    let kept = if at.is_number() {
                        serde_json::json!(choice.parse::<f64>().unwrap())
                    } else {
                        serde_json::json!(choice)
                    };
                    *at = kept.clone();
                    let back = serde_json::to_value(
                        serde_json::from_value::<Settings>(v)
                            .unwrap_or_else(|e| panic!("{key} = {choice}: {e}")),
                    )
                    .unwrap();
                    let mut got = &back;
                    for part in key.split('.') {
                        got = &got[part];
                    }
                    assert_eq!(got, &kept, "{key} = {choice} is not kept");
                    seen += 1;
                }
            }
        }
        assert!(seen > 20, "{seen} choices");
    }

    #[test]
    fn defaults_and_an_older_file() {
        let s = Settings::default();
        assert_eq!(s.cache.gb, 5);
        assert_eq!(s.cache.days, 14);
        assert_eq!(s.obs.port, 4455);
        assert_eq!(s.hotkeys.share_last, "Ctrl+Alt+S");
        assert_eq!(s.hotkeys.save_share, "Ctrl+Alt+Shift+S");
        assert_eq!(s.hotkeys.upload_last, "Ctrl+Alt+U");
        assert_eq!(s.sounds.link_copied.volume, 0.5);
        assert!(s.startup.autostart);
        assert_eq!(s.upload.mode, UploadMode::Auto);
        let older: Settings =
            serde_json::from_str(r#"{"server_url":"https://x.example","cache":{"gb":1}}"#).unwrap();
        assert_eq!(older.server_url, "https://x.example");
        assert_eq!(older.cache.gb, 1);
        assert_eq!(older.cache.days, 14, "a missing field takes its default");
        assert_eq!(older.upload.keep_last, 0, "the whole clip unless asked");
        assert_eq!(older.hotkeys.save_share, "Ctrl+Alt+Shift+S");
        assert_eq!(older.updates, UpdateMode::Ask);
        let quit: Settings = serde_json::from_str(r#"{"updates":"on_quit"}"#).unwrap();
        assert_eq!(quit.updates, UpdateMode::OnQuit);
        assert_eq!(older.webview_audio(), WebviewAudio::Browser, "1.2.0's file");
        for test_build in ["service", "browser"] {
            let json = format!(r#"{{"webview_audio":"{test_build}"}}"#);
            let s: Settings = serde_json::from_str(&json).unwrap();
            assert_eq!(
                s.webview_audio(),
                WebviewAudio::Browser,
                "the test build's {test_build}"
            );
        }
        let fallback: Settings =
            serde_json::from_str(r#"{"webview_audio_fallback":true}"#).unwrap();
        assert_eq!(fallback.webview_audio(), WebviewAudio::Service);
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            WebviewAudio::from_args(&args(&["reel", "--hidden", "--webview-audio=browser"])),
            Some(WebviewAudio::Browser)
        );
        assert_eq!(
            WebviewAudio::from_args(&args(&["reel", "--webview-audio=service"])),
            Some(WebviewAudio::Service)
        );
        assert_eq!(
            WebviewAudio::from_args(&args(&["reel", "--webview-audio=loud"])),
            None
        );
        assert_eq!(WebviewAudio::from_args(&args(&["reel", "--hidden"])), None);
        for w in [WebviewAudio::Service, WebviewAudio::Browser] {
            let flag = format!("--webview-audio={}", w.word());
            assert_eq!(WebviewAudio::from_args(&[flag]), Some(w));
        }
        assert!(s.accepts(Path::new("a.MKV")));
        assert!(!s.accepts(Path::new("a.txt")));
    }

    #[test]
    fn save_and_load_round_trip_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let s = Settings {
            server_url: "https://clips.example.org".into(),
            upload: UploadSettings {
                mode: UploadMode::Hotkey,
                ..UploadSettings::default()
            },
            ..Settings::default()
        };
        s.save(&file).unwrap();
        assert!(!part_path(&file).exists());
        let back = Settings::load(&file).unwrap();
        assert_eq!(back, s);
        assert_eq!(
            Settings::load(&dir.path().join("none.json")).unwrap(),
            Settings::fresh()
        );
    }

    #[test]
    fn a_file_of_a_later_version_loads_with_what_this_one_knows() {
        use crate::bundled::{Noise, Quality, RateMode};
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let mut s = Settings {
            mode: Mode::Standalone,
            server_url: "https://clips.example.org".into(),
            device_name: "PC".into(),
            stable_seconds: 7,
            tray_icon: TrayIcon::White,
            updates: UpdateMode::OnQuit,
            upload: UploadSettings {
                mode: UploadMode::Hotkey,
                keep_last: 30,
                ..UploadSettings::default()
            },
            log_level: "debug".into(),
            ..Settings::default()
        };
        s.obs.port = 4460;
        s.obs.bundled = true;
        s.obs.setup.done = true;
        s.obs.setup.quality = Quality::Smooth;
        s.obs.setup.rate = RateMode::Quality;
        s.obs.setup.level = 21;
        s.obs.setup.noise = Noise::Speex;
        s.save(&file).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        let newer = text.replacen(r#""quality": "smooth""#, r#""quality": "ultra""#, 1);
        assert_ne!(newer, text, "the file names its quality");
        std::fs::write(&file, &newer).unwrap();
        let back = Settings::load(&file).expect("a quality of a later version loads");
        assert_eq!(back.obs.setup.quality, Quality::Smooth);
        assert_eq!(back, s, "every other value as it was");

        let read = |json: &str| {
            std::fs::write(&file, json).unwrap();
            Settings::load(&file).unwrap_or_else(|e| panic!("{json}: {e}"))
        };
        assert_eq!(
            read(r#"{"tray_icon":"rainbow"}"#).tray_icon,
            TrayIcon::Colour
        );
        assert_eq!(read(r#"{"updates":"nightly"}"#).updates, UpdateMode::Ask);
        assert_eq!(
            read(r#"{"upload":{"mode":"on_wifi"}}"#).upload.mode,
            UploadMode::Manual,
            "no clip goes up by itself"
        );
        let setup = read(r#"{"obs":{"setup":{"rate":"vbr","noise":"deep","level":19}}}"#)
            .obs
            .setup;
        assert_eq!(
            (setup.rate, setup.noise, setup.level),
            (RateMode::Bitrate, Noise::Rnnoise, 19)
        );
    }

    #[test]
    fn the_light_quality_is_saved_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let mut s = Settings {
            server_url: "https://clips.example.org".into(),
            ..Settings::default()
        };
        s.obs.bundled = true;
        s.obs.setup.done = true;
        s.obs.setup.quality = crate::bundled::Quality::Light;
        s.save(&file).unwrap();
        assert!(std::fs::read_to_string(&file)
            .unwrap()
            .contains(r#""quality": "light""#));
        assert_eq!(Settings::load(&file).unwrap(), s);
    }

    #[test]
    fn a_new_install_starts_standalone_and_a_set_up_app_keeps_its_mode() {
        let dir = tempfile::tempdir().unwrap();
        let (s, fresh) = Settings::load_or_fresh(&dir.path().join("settings.json")).unwrap();
        assert!(fresh, "no settings file: a new install");
        assert_eq!(
            s.mode,
            Mode::Standalone,
            "a new install's setup starts on This PC only"
        );
        assert_eq!(
            Settings::default().mode,
            Mode::Connected,
            "the default a field missing from a file takes"
        );
        let kept = |json: &str| {
            let file = dir.path().join("kept.json");
            std::fs::write(&file, json).unwrap();
            let (s, fresh) = Settings::load_or_fresh(&file).unwrap();
            assert!(!fresh, "{json}");
            s.save(&file).unwrap();
            let (again, _) = Settings::load_or_fresh(&file).unwrap();
            assert_eq!(again.mode, s.mode, "saved and read again: {json}");
            s.mode
        };
        for (json, mode) in [
            (
                r#"{"server_url":"https://clips.example.org"}"#,
                Mode::Connected,
            ),
            (
                r#"{"server_url":"https://clips.example.org","device_name":"PC","last_version":"1.2.0"}"#,
                Mode::Connected,
            ),
            (r#"{"last_version":"1.0.0"}"#, Mode::Connected),
            ("{}", Mode::Connected),
            (
                r#"{"mode":"connected","server_url":"https://clips.example.org"}"#,
                Mode::Connected,
            ),
            (r#"{"mode":"connected"}"#, Mode::Connected),
            (r#"{"mode":"standalone"}"#, Mode::Standalone),
            (
                r#"{"mode":"standalone","server_url":"https://clips.example.org"}"#,
                Mode::Standalone,
            ),
        ] {
            assert_eq!(kept(json), mode, "{json}");
        }
    }

    #[test]
    fn the_paths_follow_the_override() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("REEL_DESKTOP_DIR", dir.path());
        let p = paths();
        assert_eq!(p.config_dir, dir.path().join("config"));
        assert_eq!(p.data_dir, dir.path().join("data"));
        p.ensure().unwrap();
        assert!(p.logs_dir().is_dir() && p.cues_dir().is_dir());
        std::env::remove_var("REEL_DESKTOP_DIR");
        let p = paths();
        assert!(p.config_dir.ends_with("reel"));
    }
}
