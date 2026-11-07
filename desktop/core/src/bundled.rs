//! The OBS that comes with the app, and what the app writes into its settings.
use crate::readback::{
    in_mode, merge3, merge_collection, merge_ini3, observe, overlay, take, Live, Taken, Written,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The bundled OBS's WebSocket port: not 4455, so it never meets a user's own OBS.
pub const PORT: u16 = 4466;
/// The profile's and the scene collection's name.
pub const NAME: &str = "reel";
/// OBS's Quick Sync encoder keeps its bitrate in 16 bits: more wraps around to far less.
const QSV_MAX_KBIT: u32 = 65535;
const PROFILE: &str = "obs-studio/basic/profiles/reel/basic.ini";
const ENCODER: &str = "obs-studio/basic/profiles/reel/recordEncoder.json";
const SCENES: &str = "obs-studio/basic/scenes/reel.json";
const WEBSOCKET: &str = "obs-studio/plugin_config/obs-websocket/config.json";
const PINNED: &str = include_str!("../../obs/pinned.json");
/// How much of OBS's own log goes into the app's log when OBS exits or does not answer.
pub const OBS_LOG_LINES: usize = 30;

/// The pinned OBS: its version and where its source is, for the GPL's naming.
#[derive(Debug, Clone, Deserialize)]
pub struct Pinned {
    pub version: String,
    pub source: String,
    pub sources_url: String,
    pub license: String,
}

pub fn pinned() -> Pinned {
    serde_json::from_str(PINNED).expect("desktop/obs/pinned.json parses")
}

/// OBS's version as libobs counts it; as LastVersion in global.ini it keeps OBS's
/// auto-configuration wizard closed.
fn api_version(version: &str) -> u64 {
    let mut parts = version.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    let (a, b, c) = (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    );
    (a << 24) | (b << 16) | c
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    /// 1080p at 60 frames a second.
    #[default]
    Smooth,
    /// 1440p at 60.
    Sharp,
    /// The screen's own size and refresh rate.
    Native,
    /// What was set in OBS's own Video settings, read back into `custom_video`.
    Custom,
}

/// OBS's Video settings: the canvas, the output and the frame rate as a fraction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VideoMode {
    pub base: (u32, u32),
    pub output: (u32, u32),
    pub fps: (u32, u32),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RateMode {
    /// A constant bitrate: the replay buffer's memory and the clips' sizes are known.
    #[default]
    Bitrate,
    /// A constant quality level instead (CQP, or CRF with x264): smaller still scenes, larger busy ones.
    Quality,
}

/// The screen the setup captures, as the app reads it from Windows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Screen {
    /// OBS's monitor_id: the device interface name Windows gives the monitor.
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub hz: u32,
    pub main: bool,
}

/// OBS's mixer tracks by mask, and their names, which OBS writes as each audio stream's title.
pub const FULL: u32 = 1;
pub const GAME: u32 = 1 << 1;
pub const MIC: u32 = 1 << 2;
pub const DISCORD: u32 = 1 << 3;
pub const DESKTOP: u32 = 1 << 4;
pub const TRACKS: [&str; 6] = [
    "Full mix",
    "Game",
    "Microphone",
    "Discord",
    "Desktop",
    "Track 6",
];

/// What the setup chose; every field has the default "Next, Next, Done" gives.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ObsSetup {
    pub done: bool,
    pub clip_seconds: u32,
    pub quality: Quality,
    pub rate: RateMode,
    /// The bitrate in kbit/s; 0 is the preset's.
    pub kbit: u32,
    /// The quality level for RateMode::Quality (lower is better).
    pub level: u32,
    /// OBS's encoder id, picked by the test encode; empty until then (x264).
    pub encoder: String,
    /// Capture the game in front (game capture of any fullscreen application) over the screen.
    pub game: bool,
    pub screen: Screen,
    /// The output device for the desktop audio; "default" follows Windows.
    pub desktop_audio: String,
    pub mic: Option<String>,
    /// Desktop audio, what the output device plays, as a source.
    pub desktop: bool,
    /// The game alone as a source: "" off, "front" the game in front (its game capture's own
    /// audio), or a program as OBS names a window ("title:class:exe").
    pub game_audio: String,
    /// Discord alone as a source.
    pub discord: bool,
    /// Each source on a track of its own beside the full mix.
    pub own_tracks: bool,
    /// The recording format: hybrid MP4 plays everywhere and survives a crash.
    pub format: String,
    /// Capture games that run as administrator: OBS, never the app, gets the highest rights.
    pub elevated: bool,
    /// The WebSocket password the app made; never shown.
    pub password: String,
    pub noise: Noise,
    /// OBS's noise gate on the microphone as well, after the noise suppression.
    pub gate: bool,
    /// The filters copied from the user's own OBS's microphone for Noise::Own, and their source.
    pub own_filters: Vec<Value>,
    pub own_filters_from: String,
    /// Each audio source's volume in dB, by its name; 0 where not set.
    pub volumes: std::collections::BTreeMap<String, f64>,
    /// The video set in OBS, for Quality::Custom.
    pub custom_video: Option<VideoMode>,
    /// The tracks a recording carries as set in OBS (its mask), when no choice of reel's
    /// gives them; 0 when own_tracks says.
    pub rec_tracks: u32,
    /// The tracks' names as set in OBS, in order; empty for reel's own (TRACKS).
    pub titles: Vec<String>,
}

/// The microphone's noise suppression: OBS's own filters, set on the Microphone source.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Noise {
    Off,
    /// Speex: little CPU, the steady hiss and hum.
    Speex,
    /// RNNoise: a small neural network on the CPU; also keys and noise between words.
    #[default]
    Rnnoise,
    /// NVIDIA's noise removal on an RTX card, when OBS reports it.
    Nvidia,
    /// The filters of the user's own OBS's microphone, as they are.
    Own,
}

impl Default for ObsSetup {
    fn default() -> Self {
        ObsSetup {
            done: false,
            clip_seconds: 60,
            quality: Quality::Smooth,
            rate: RateMode::Bitrate,
            kbit: 0,
            level: 20,
            encoder: String::new(),
            game: true,
            screen: Screen::default(),
            desktop_audio: "default".into(),
            mic: None,
            desktop: true,
            game_audio: String::new(),
            discord: false,
            own_tracks: false,
            format: "hybrid_mp4".into(),
            elevated: false,
            password: String::new(),
            noise: Noise::Rnnoise,
            gate: false,
            own_filters: Vec::new(),
            own_filters_from: String::new(),
            volumes: std::collections::BTreeMap::new(),
            custom_video: None,
            rec_tracks: 0,
            titles: Vec::new(),
        }
    }
}

/// OBS keeps a source's volume as a multiplier; the app's page and the WebSocket in dB.
pub fn volume_mul(db: f64) -> f64 {
    10f64.powf(db.clamp(-60.0, 26.0) / 20.0)
}

/// A filter as OBS saves it in a scene collection.
fn filter(name: &str, id: &str, version: Option<u32>, settings: Value) -> Value {
    json!({
        "name": name,
        "uuid": uuid_of(&format!("filter {name}")),
        "id": id,
        "versioned_id": match version { Some(v) => format!("{id}_v{v}"), None => id.to_string() },
        "settings": settings,
        "enabled": true,
        "hotkeys": {},
        "private_settings": {},
    })
}

impl ObsSetup {
    /// The noise suppression picked, then the gate when it is on; none without a microphone.
    pub fn mic_filters(&self) -> Vec<Value> {
        if self.mic.is_none() {
            return Vec::new();
        }
        let mut out = match self.noise {
            Noise::Off => Vec::new(),
            Noise::Speex => vec![filter(
                "Noise suppression",
                "noise_suppress_filter",
                Some(2),
                json!({ "method": "speex", "suppress_level": -30 }),
            )],
            Noise::Rnnoise => vec![filter(
                "Noise suppression",
                "noise_suppress_filter",
                Some(2),
                json!({ "method": "rnnoise" }),
            )],
            Noise::Nvidia => vec![filter(
                "NVIDIA noise removal",
                "nvidia_audiofx_filter",
                None,
                json!({ "method": "denoiser", "intensity": 1.0 }),
            )],
            Noise::Own => return self.own_filters.clone(),
        };
        if self.gate {
            out.push(filter(
                "Noise gate",
                "noise_gate_filter",
                None,
                json!({ "open_threshold": -26.0, "close_threshold": -32.0, "attack_time": 25, "hold_time": 200, "release_time": 150 }),
            ));
        }
        out
    }
}

/// The requests that replace every filter of a running OBS's input with the wanted ones.
pub fn filter_requests(input: &str, current: &[Value], wanted: &[Value]) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for f in current {
        if let Some(name) = f["filterName"].as_str() {
            out.push((
                "RemoveSourceFilter".to_string(),
                json!({ "sourceName": input, "filterName": name }),
            ));
        }
    }
    for f in wanted {
        let name = f["name"].as_str().unwrap_or("Filter");
        let kind = f["versioned_id"]
            .as_str()
            .filter(|k| !k.is_empty())
            .or(f["id"].as_str())
            .unwrap_or_default();
        out.push((
            "CreateSourceFilter".to_string(),
            json!({ "sourceName": input, "filterName": name, "filterKind": kind, "filterSettings": f["settings"].clone() }),
        ));
        if f["enabled"].as_bool() == Some(false) {
            out.push((
                "SetSourceFilterEnabled".to_string(),
                json!({ "sourceName": input, "filterName": name, "filterEnabled": false }),
            ));
        }
    }
    out
}

/// A microphone with filters in the user's own OBS's current scene collection, never written.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OwnMic {
    pub name: String,
    pub collection: String,
    pub filters: Vec<Value>,
}

const MIC_KINDS: &[&str] = &[
    "wasapi_input_capture",
    "pulse_input_capture",
    "coreaudio_input_capture",
];

/// `config`: %APPDATA%\obs-studio, or obs-studio under XDG_CONFIG_HOME on Linux.
pub fn own_mic(config: &Path) -> Option<OwnMic> {
    let wanted = ["user.ini", "global.ini"].iter().find_map(|f| {
        let text = std::fs::read_to_string(config.join(f)).ok()?;
        ini_value(&text, "Basic", "SceneCollectionFile")
    });
    let scenes = config.join("basic").join("scenes");
    let file = match wanted.map(|w| scenes.join(format!("{w}.json"))) {
        Some(f) if f.is_file() => f,
        _ => {
            std::fs::read_dir(&scenes)
                .ok()?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
                .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
                .max_by_key(|(t, _)| *t)?
                .1
        }
    };
    let c: Value = serde_json::from_str(&std::fs::read_to_string(&file).ok()?).ok()?;
    let collection = c["name"].as_str().unwrap_or_default().to_string();
    let globals = (1..=4).map(|i| c[format!("AuxAudioDevice{i}")].clone());
    let listed = c["sources"].as_array().cloned().unwrap_or_default();
    globals
        .chain(listed)
        .filter(|s| {
            let id = s["versioned_id"]
                .as_str()
                .or(s["id"].as_str())
                .unwrap_or("");
            MIC_KINDS.contains(&id)
        })
        .find_map(|s| {
            let filters = s["filters"].as_array().filter(|f| !f.is_empty())?.clone();
            Some(OwnMic {
                name: s["name"].as_str().unwrap_or("the microphone").to_string(),
                collection: collection.clone(),
                filters,
            })
        })
}

pub fn own_obs_config() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("obs-studio"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|c| c.join("obs-studio"))
    }
}

/// The OBS installed on this Linux PC, as reel's second OBS: `obs` on the PATH, else Flathub's.
pub fn installed(
    path: Option<&std::ffi::OsStr>,
    home: &Path,
    data_dir: &Path,
    flatpak_dirs: &[PathBuf],
) -> Option<Bundled> {
    let on_path = path
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .map(|d| d.join("obs"))
        .find(|p| runnable(p));
    if let Some(exe) = on_path {
        let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
        return Some(Bundled::linux(exe, data_dir));
    }
    flatpak_dirs
        .iter()
        .any(|d| d.join(FLATPAK_ID).is_dir())
        .then(|| Bundled::flatpak(home))
}

/// Where Flatpak keeps its apps: the user's, then the system's.
pub fn flatpak_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local")
            .join("share")
            .join("flatpak")
            .join("app"),
        PathBuf::from("/var/lib/flatpak/app"),
    ]
}

fn runnable(p: &Path) -> bool {
    let Ok(m) = std::fs::metadata(p) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        m.is_file() && m.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        m.is_file()
    }
}

pub(crate) fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}').trim();
        if line.starts_with('[') {
            inside = line == format!("[{section}]");
        } else if inside {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// What a setup comes to: the canvas, the output, the bitrate, the replay buffer's memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Video {
    pub base: (u32, u32),
    pub output: (u32, u32),
    pub fps: u32,
    pub kbit: u32,
    pub buffer_mb: u32,
}

fn even(x: f64) -> u32 {
    ((x / 2.0).round() as u32).max(1) * 2
}

impl ObsSetup {
    /// The screen's size, 1920x1080 at 60 Hz when the app could not read it.
    fn screen_mode(&self) -> (u32, u32, u32) {
        let s = &self.screen;
        if s.width == 0 || s.height == 0 {
            (1920, 1080, 60)
        } else {
            (s.width, s.height, if s.hz == 0 { 60 } else { s.hz })
        }
    }

    pub fn video(&self) -> Video {
        let mode = self.video_mode();
        let (w, h) = mode.base;
        let output = mode.output;
        let fps = self.fps();
        let preset = output.0 as f64 * output.1 as f64 * fps as f64 * 0.2 / 1000.0;
        let kbit = if self.kbit > 0 {
            self.kbit
        } else {
            ((preset / 500.0).round() as u32).max(1) * 500
        };
        let audio_kbit = 160 * self.tracks(Platform::Windows).count_ones();
        let buffer_mb =
            ((kbit + audio_kbit) as u64 * self.clip_seconds.max(1) as u64 * 5 / 4 / 8 / 1000)
                .max(64) as u32;
        Video {
            base: (w, h),
            output,
            fps,
            kbit,
            buffer_mb,
        }
    }

    /// The preset's for the screen, or what was set in OBS.
    pub fn video_mode(&self) -> VideoMode {
        if let (Quality::Custom, Some(v)) = (self.quality, self.custom_video) {
            if v.base.0 > 0 && v.output.0 > 0 && v.fps.0 > 0 && v.fps.1 > 0 {
                return v;
            }
        }
        let (w, h, hz) = self.screen_mode();
        let (height, fps) = match self.quality {
            Quality::Sharp => (1440, 60),
            Quality::Native => (h, hz.min(240)),
            Quality::Smooth | Quality::Custom => (1080, 60),
        };
        let height = height.min(h);
        VideoMode {
            base: (w, h),
            output: (even(w as f64 * height as f64 / h as f64), height),
            fps: (fps, 1),
        }
    }

    fn fps(&self) -> u32 {
        let (n, d) = self.video_mode().fps;
        ((n as f64 / d.max(1) as f64).round() as u32).max(1)
    }

    /// The mixer tracks a recording carries: the full mix always, and with own_tracks each
    /// source's own.
    pub fn tracks(&self, platform: Platform) -> u32 {
        if self.rec_tracks != 0 {
            return self.rec_tracks & 0x3f;
        }
        self.audio(platform).iter().fold(FULL, |t, s| t | s.mixers)
    }

    /// Desktop audio already holds the game and Discord, so with it on those never go into the
    /// full mix a second time.
    pub fn audio(&self, platform: Platform) -> Vec<AudioSource> {
        let own = |t: u32| if self.own_tracks { t } else { 0 };
        let apart = |t: u32| {
            if self.desktop {
                t
            } else {
                FULL | own(t)
            }
        };
        let mut out = Vec::new();
        let mut push = |name: &str, kind: &str, mixers: u32| {
            out.push(AudioSource {
                name: name.into(),
                kind: kind.into(),
                mixers,
            })
        };
        if self.desktop {
            push("Desktop audio", "desktop", FULL | own(DESKTOP));
        }
        if self.mic.is_some() {
            push("Microphone", "mic", FULL | own(MIC));
        }
        match self.game_audio.as_str() {
            "" => {}
            "front" if self.game && platform == Platform::Windows => {
                push("Game", "game", apart(GAME))
            }
            "front" => {}
            _ => push("Game audio", "game", apart(GAME)),
        }
        if self.discord {
            push("Discord", "discord", apart(DISCORD));
        }
        out
    }

    /// Where each source goes, a line each, for the page.
    pub fn audio_words(&self, platform: Platform) -> Vec<String> {
        let list = self.audio(platform);
        let word = |kind: &str| match kind {
            "desktop" => "desktop audio",
            "mic" => "the microphone",
            "game" => "the game",
            _ => "Discord",
        };
        let mut out = Vec::new();
        let full: Vec<&str> = list
            .iter()
            .filter(|s| s.mixers & FULL != 0)
            .map(|s| word(&s.kind))
            .collect();
        out.push(if full.is_empty() {
            "nothing is chosen: a clip has no sound".to_string()
        } else {
            format!("The full mix, which links play: {}", full.join(" and "))
        });
        for s in &list {
            let apart = s.mixers & !FULL != 0;
            let line = match (s.kind.as_str(), self.desktop, apart) {
                ("game" | "discord", true, _) => format!(
                    "{} is in the full mix through desktop audio, and on a track of its own",
                    capital(word(&s.kind))
                ),
                (_, _, true) => format!("{} also on a track of its own", capital(word(&s.kind))),
                _ => continue,
            };
            out.push(line);
        }
        out
    }

    /// The recorded tracks' names in order, for the server, as an MP4 drops the stream titles.
    pub fn track_names(&self, platform: Platform) -> Vec<String> {
        let mask = self.tracks(platform);
        self.track_titles()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, n)| n)
            .collect()
    }

    /// The six tracks' names the profile gives: reel's, or those set in OBS.
    pub fn track_titles(&self) -> Vec<String> {
        (0..6)
            .map(|i| match self.titles.get(i) {
                Some(t) if !t.is_empty() => t.clone(),
                _ => TRACKS[i].to_string(),
            })
            .collect()
    }

    pub fn encoder_id(&self) -> &str {
        if self.encoder.is_empty() {
            "obs_x264"
        } else {
            &self.encoder
        }
    }

    /// A keyframe every second so the editor cuts close to where it is asked. VAAPI takes the
    /// profile as FFmpeg's number and its quantizer as qp.
    pub fn encoder_settings(&self) -> Value {
        let v = self.video();
        let id = self.encoder_id();
        let mut s = json!({ "keyint_sec": 1, "profile": "high" });
        let quality = self.rate == RateMode::Quality;
        match id {
            "obs_x264" => {
                s["preset"] = json!("veryfast");
                if quality {
                    s["rate_control"] = json!("CRF");
                    s["crf"] = json!(self.level);
                } else {
                    s["rate_control"] = json!("CBR");
                    s["bitrate"] = json!(v.kbit);
                }
            }
            "ffmpeg_vaapi_tex" => {
                s["profile"] = json!(100);
                if quality {
                    s["rate_control"] = json!("CQP");
                    s["qp"] = json!(self.level);
                } else {
                    s["rate_control"] = json!("CBR");
                    s["bitrate"] = json!(v.kbit);
                }
            }
            _ => {
                match id {
                    "obs_nvenc_h264_tex" => s["preset"] = json!("p5"),
                    "h264_texture_amf" => s["preset"] = json!("quality"),
                    "obs_qsv11_v2" => s["target_usage"] = json!("TU4"),
                    _ => {}
                }
                if quality {
                    s["rate_control"] = json!("CQP");
                    s["cqp"] = json!(self.level);
                } else {
                    s["rate_control"] = json!("CBR");
                    s["bitrate"] = json!(match id {
                        "obs_qsv11_v2" => v.kbit.min(QSV_MAX_KBIT),
                        _ => v.kbit,
                    });
                }
            }
        }
        s
    }
}

/// A source of sound: `kind` is desktop, mic, game or discord; `mixers` a mask.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AudioSource {
    pub name: String,
    pub kind: String,
    pub mixers: u32,
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The screen's id under PipeWire (Wayland): the desktop asks which screen, OBS keeps a token.
pub const PIPEWIRE: &str = "pipewire";

/// A program's name as PipeWire knows it, also from a Windows window (title:class:exe).
fn program_of(spec: &str) -> String {
    match spec.rsplit_once(':') {
        Some((_, exe)) => exe.strip_suffix(".exe").unwrap_or(exe).to_string(),
        None => spec.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    /// The OBS installed on a Linux PC, run as a second OBS with settings of its own.
    Linux,
}

/// A setup with the clips folder and the Save hotkey it goes with: what reel writes from.
pub type Chosen<'a> = (&'a ObsSetup, &'a Path, &'a str);

/// The bundled OBS on disk; its settings by portable mode on Windows, XDG_CONFIG_HOME on Linux.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundled {
    pub exe: PathBuf,
    pub config_root: PathBuf,
    pub platform: Platform,
    pub flatpak: bool,
}

pub const FLATPAK_ID: &str = "com.obsproject.Studio";

/// OBS's own obs64.exe renamed: Discord turns Streamer Mode on while a process of that name
/// runs, and reel's OBS runs all the time.
pub const OBS_EXE: &str = "reel-capture.exe";
/// The program's name in an install or a copy an older version made.
pub const OBS_EXE_BEFORE: &str = "obs64.exe";

/// A copy, not a link, of the installed program: Discord reads /proc/<pid>/exe, which a link
/// would leave at the installed `obs`.
pub const LINUX_NAME: &str = "reel-capture";

/// Where the copy runs from on Linux: its own `bin` beside links to the installed prefix's folders.
const PROGRAM: &str = "program";

/// A copy of `exe` named LINUX_NAME, or `exe` when it is no ELF file or the copy fails.
/// OBS finds its helpers beside /proc/self/exe and from 33 its plugins by `..`: both get links.
pub fn own_name(exe: &Path, dir: &Path) -> PathBuf {
    let root = dir.join(PROGRAM);
    let bin = root.join("bin");
    let to = bin.join(LINUX_NAME);
    let Ok(program) = std::fs::read(exe) else {
        return exe.to_path_buf();
    };
    if !program.starts_with(b"\x7fELF") {
        return exe.to_path_buf();
    }
    forget_first_copy(dir);
    if !std::fs::read(&to).is_ok_and(|had| had == program) {
        let new = bin.join(format!("{LINUX_NAME}.new"));
        let made = std::fs::create_dir_all(&bin)
            .and_then(|_| std::fs::write(&new, &program))
            .and_then(|_| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
                }
                std::fs::rename(&new, &to)
            });
        if let Err(e) = made {
            tracing::warn!(error = %e, "no copy of OBS under reel's name; it runs as installed");
            return exe.to_path_buf();
        }
        tracing::info!(from = %exe.display(), to = %to.display(), "reel's OBS runs under a name of its own");
    }
    match link_helpers(exe, &bin).and_then(|mut linked| {
        linked.extend(link_prefix(exe, &root)?);
        Ok(linked)
    }) {
        Ok(linked) => {
            if !linked.is_empty() {
                tracing::info!(?linked, "OBS's helpers and folders beside reel's copy");
            }
            to
        }
        Err(e) => {
            tracing::warn!(error = %e, "OBS's helpers or folders are not beside reel's copy; it runs as installed");
            exe.to_path_buf()
        }
    }
}

/// Links in `root` to the installed prefix's folders, so `..` from the copy's `bin` reaches
/// the same; stale links go. The names of those made.
fn link_prefix(exe: &Path, root: &Path) -> std::io::Result<Vec<String>> {
    let mut linked = Vec::new();
    let Some(prefix) = exe.parent().and_then(Path::parent) else {
        return Ok(linked);
    };
    let mut names = std::collections::HashSet::new();
    for e in std::fs::read_dir(prefix)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_string();
        let target = e.path();
        if name == "bin" || !target.is_dir() {
            continue;
        }
        names.insert(name.clone());
        let link = root.join(&name);
        if std::fs::read_link(&link).is_ok_and(|t| t == target) {
            continue;
        }
        let _ = std::fs::remove_file(&link);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)?;
        linked.push(name);
    }
    for e in std::fs::read_dir(root)? {
        let e = e?;
        if e.file_type()?.is_symlink() && !names.contains(&*e.file_name().to_string_lossy()) {
            std::fs::remove_file(e.path())?;
        }
    }
    Ok(linked)
}

/// Removes a copy an older version made in the OBS folder itself, keeping `obs-studio`.
fn forget_first_copy(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(t) = e.file_type() else {
            continue;
        };
        let copy = t.is_file() && (name == LINUX_NAME || name == format!("{LINUX_NAME}.new"));
        let helper = t.is_symlink() && name.starts_with("obs-") && name != "obs-studio";
        if copy || helper {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Links in `dir` to the obs- programs beside `exe`; the names of those made.
fn link_helpers(exe: &Path, dir: &Path) -> std::io::Result<Vec<String>> {
    let mut linked = Vec::new();
    let Some(from) = exe.parent() else {
        return Ok(linked);
    };
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().to_string();
        let target = e.path();
        if !name.starts_with("obs-") || !runnable(&target) {
            continue;
        }
        let link = dir.join(&name);
        if std::fs::read_link(&link).is_ok_and(|t| t == target) {
            continue;
        }
        let _ = std::fs::remove_file(&link);
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link)?;
        linked.push(name);
    }
    Ok(linked)
}

/// Whether a process's file name is one of reel's OBS on Windows, under either name.
pub fn is_obs_exe(name: &str) -> bool {
    name.eq_ignore_ascii_case(OBS_EXE) || name.eq_ignore_ascii_case(OBS_EXE_BEFORE)
}

/// Drops the verbatim `\\?\` prefix: under it Windows does not resolve `..`, by which OBS finds
/// its data and portable settings.
pub fn plain(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        let b = rest.as_bytes();
        if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
            return PathBuf::from(rest);
        }
    }
    p.to_path_buf()
}

impl Bundled {
    /// The OBS the installer with OBS put next to the app; None without one.
    pub fn in_resources(resource_dir: &Path) -> Option<Bundled> {
        let b = Bundled::windows(resource_dir);
        b.exe.is_file().then_some(b)
    }

    /// Where the installer puts OBS under the app's resource folder, every path plain.
    pub fn windows(resource_dir: &Path) -> Bundled {
        let root = plain(resource_dir).join("obs");
        Bundled {
            exe: root.join("bin").join("64bit").join(OBS_EXE),
            config_root: root.join("config"),
            platform: Platform::Windows,
            flatpak: false,
        }
    }

    /// Its settings in reel's data folder, so the user's own OBS stays as it is.
    pub fn linux(exe: PathBuf, data_dir: &Path) -> Bundled {
        Bundled {
            exe,
            config_root: data_dir.join("obs"),
            platform: Platform::Linux,
            flatpak: false,
        }
    }

    /// Settings under ~/.var/app, the one place Flathub's sandbox lets OBS write.
    pub fn flatpak(home: &Path) -> Bundled {
        Bundled {
            exe: PathBuf::from("flatpak"),
            config_root: home.join(".var").join("app").join(FLATPAK_ID).join("reel"),
            platform: Platform::Linux,
            flatpak: true,
        }
    }

    /// Flathub's sandbox takes XDG_CONFIG_HOME from `--env`.
    pub fn command(&self) -> (PathBuf, Vec<String>) {
        if !self.flatpak {
            return (self.exe.clone(), self.args(false));
        }
        let mut a = vec![
            "run".to_string(),
            format!("--env=XDG_CONFIG_HOME={}", self.config_root.display()),
            FLATPAK_ID.to_string(),
        ];
        a.extend(self.args(false));
        (self.exe.clone(), a)
    }

    /// The user's own OBS settings folder of the same kind, Flathub's under ~/.var/app.
    pub fn own_config(&self) -> Option<PathBuf> {
        if self.flatpak {
            return self
                .config_root
                .parent()
                .map(|app| app.join("config").join("obs-studio"));
        }
        own_obs_config()
    }

    /// OBS finds user plugins under its settings folder, so reel's second OBS links to the
    /// user's plugins folder; nothing of the user's is written.
    pub fn link_plugins(&self) {
        if self.platform != Platform::Linux {
            return;
        }
        let Some(theirs) = self.own_config().map(|c| c.join("plugins")) else {
            return;
        };
        let ours = self.obs_dir().join("plugins");
        if !theirs.is_dir() || ours.symlink_metadata().is_ok() {
            return;
        }
        let _ = std::fs::create_dir_all(self.obs_dir());
        #[cfg(unix)]
        if let Err(e) = std::os::unix::fs::symlink(&theirs, &ours) {
            tracing::warn!(error = %e, "the link to the user's OBS plugins was not made");
        }
    }

    /// Whether Discord sees this OBS as OBS: Flathub's runs as /app/bin/obs, and a program
    /// that is no ELF file is not copied.
    pub fn shows_as_obs(&self) -> bool {
        self.flatpak
            || (self.platform == Platform::Linux
                && !std::fs::read(&self.exe)
                    .map(|b| b.starts_with(b"\x7fELF"))
                    .unwrap_or(false))
    }

    pub fn which(&self) -> String {
        if self.flatpak {
            "OBS from Flathub".to_string()
        } else if self.platform == Platform::Linux {
            format!("the OBS installed on this PC ({})", self.exe.display())
        } else {
            "the OBS that came with reel".to_string()
        }
    }

    /// OBS finds its data from the folder it starts in; Flathub's starts in its settings folder.
    pub fn workdir(&self) -> PathBuf {
        if self.flatpak {
            return self.config_root.clone();
        }
        self.exe
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// `tray` when OBS has the highest rights, as the app may not hide its window then; on Linux
    /// always, as no app may hide another's window on Wayland.
    pub fn args(&self, tray: bool) -> Vec<String> {
        let mut a = Vec::new();
        if self.platform == Platform::Windows {
            a.push("--portable".to_string());
        }
        if tray || self.platform == Platform::Linux {
            a.push("--minimize-to-tray".to_string());
        }
        for x in [
            "--multi",
            "--disable-updater",
            "--disable-missing-files-check",
            "--startreplaybuffer",
            "--profile",
            NAME,
            "--collection",
            NAME,
            "--scene",
            NAME,
        ] {
            a.push(x.to_string());
        }
        a
    }

    fn obs_dir(&self) -> PathBuf {
        self.config_root.join("obs-studio")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.obs_dir().join("logs")
    }

    /// Writes the whole setup, for an OBS without a profile of reel's yet.
    pub fn write(&self, setup: &ObsSetup, clips_dir: &Path, save_key: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(clips_dir)?;
        self.put(&self.whole(setup, clips_dir, save_key))?;
        self.clear_crash_marks();
        Ok(())
    }

    /// Writes each file under the settings folder, leaving alone one that would not change.
    pub fn put(&self, files: &[(String, String)]) -> std::io::Result<()> {
        for (rel, text) in files {
            let path = self.config_root.join(rel);
            if std::fs::read_to_string(&path).ok().as_deref() == Some(text.as_str()) {
                continue;
            }
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d)?;
            }
            write_atomic(&path, text)?;
        }
        Ok(())
    }

    /// What `write` writes.
    pub fn whole(
        &self,
        setup: &ObsSetup,
        clips_dir: &Path,
        save_key: &str,
    ) -> Vec<(String, String)> {
        let dir = self.obs_dir();
        let read = |p: &Path| std::fs::read_to_string(p).ok();
        let elevated = setup.elevated && self.platform == Platform::Windows;
        let mut w = self.render(setup, clips_dir, save_key);
        if let Some(c) = w.collection.as_mut() {
            keep_restore_token(read(&self.config_root.join(SCENES)).as_deref(), c);
        }
        vec![
            (
                "obs-studio/global.ini".to_string(),
                merge_ini(read(&dir.join("global.ini")), &global_ini()),
            ),
            (
                "obs-studio/user.ini".to_string(),
                merge_ini(
                    read(&dir.join("user.ini")),
                    &user_ini(elevated || self.platform == Platform::Linux),
                ),
            ),
            (PROFILE.to_string(), w.profile),
            (
                ENCODER.to_string(),
                serde_json::to_string_pretty(&w.encoder).unwrap_or_default(),
            ),
            (
                SCENES.to_string(),
                serde_json::to_string_pretty(&w.collection).unwrap_or_default(),
            ),
            (WEBSOCKET.to_string(), websocket_file(self, setup)),
        ]
    }

    /// Into what `theirs` has, only the keys the change touches; into an OBS without a profile
    /// of reel's, the whole setup.
    pub fn change(
        &self,
        theirs: &Bundled,
        from: Chosen<'_>,
        to: Chosen<'_>,
    ) -> Vec<(String, String)> {
        let (setup, clips_dir, save_key) = to;
        let Some(now) = theirs.read_files() else {
            return self.whole(setup, clips_dir, save_key);
        };
        let base = self.render(from.0, from.1, from.2);
        let ours = self.render(setup, clips_dir, save_key);
        let simple = ini_value(&now.profile, "Output", "Mode").as_deref() == Some("Simple");
        let profile = merge_ini3(
            Some(&in_mode(&base.profile, simple)),
            &in_mode(&ours.profile, simple),
            &now.profile,
        );
        let encoder = match &now.encoder {
            Some(t) => merge3(base.encoder.as_ref(), ours.encoder.as_ref(), Some(t)),
            None => ours.encoder.clone(),
        }
        .unwrap_or_else(|| setup.encoder_settings());
        let ours_collection = ours
            .collection
            .clone()
            .unwrap_or_else(|| collection(setup, self.platform));
        let scenes = match &now.collection {
            Some(t) => merge_collection(base.collection.as_ref(), &ours_collection, t),
            None => ours_collection,
        };
        let windows = self.platform == Platform::Windows;
        let dir = theirs.obs_dir();
        let read = |p: &Path| std::fs::read_to_string(p).ok().unwrap_or_default();
        let user = merge_ini3(
            Some(&user_ini(from.0.elevated && windows || !windows)),
            &user_ini(setup.elevated && windows || !windows),
            &read(&dir.join("user.ini")),
        );
        vec![
            (
                "obs-studio/global.ini".to_string(),
                merge_ini(Some(read(&dir.join("global.ini"))), &global_ini()),
            ),
            ("obs-studio/user.ini".to_string(), user),
            (PROFILE.to_string(), profile),
            (
                ENCODER.to_string(),
                serde_json::to_string_pretty(&encoder).unwrap_or_default(),
            ),
            (
                SCENES.to_string(),
                serde_json::to_string_pretty(&scenes).unwrap_or_default(),
            ),
            (WEBSOCKET.to_string(), websocket_file(theirs, setup)),
        ]
    }

    /// At a start, only what reel needs where it is missing; an OBS without a profile of
    /// reel's gets the whole setup.
    pub fn ensure(
        &self,
        setup: &ObsSetup,
        clips_dir: &Path,
        save_key: &str,
    ) -> std::io::Result<()> {
        let Some(profile) = std::fs::read_to_string(self.config_root.join(PROFILE)).ok() else {
            return self.write(setup, clips_dir, save_key);
        };
        let dir = self.obs_dir();
        let read = |p: &Path| std::fs::read_to_string(p).ok();
        let elevated = setup.elevated && self.platform == Platform::Windows;
        let simple = ini_value(&profile, "Output", "Mode").as_deref() == Some("Simple");
        let mut p = ini_set(
            &profile,
            if simple { "SimpleOutput" } else { "AdvOut" },
            "RecRB",
            "true",
        );
        if !simple {
            p = ini_set(&p, "AdvOut", "RecType", "Standard");
        }
        let folder = ini_value(
            &p,
            if simple { "SimpleOutput" } else { "AdvOut" },
            if simple { "FilePath" } else { "RecFilePath" },
        );
        if let Some(f) = folder.map(PathBuf::from).filter(|f| f.is_absolute()) {
            let _ = std::fs::create_dir_all(f);
        }
        self.put(&[
            (
                "obs-studio/global.ini".to_string(),
                merge_ini(read(&dir.join("global.ini")), &global_ini()),
            ),
            (
                "obs-studio/user.ini".to_string(),
                merge_ini(
                    read(&dir.join("user.ini")),
                    &user_ini_needed(elevated || self.platform == Platform::Linux),
                ),
            ),
            (PROFILE.to_string(), p),
            (WEBSOCKET.to_string(), websocket_file(self, setup)),
        ])?;
        self.clear_crash_marks();
        Ok(())
    }

    /// OBS's files as they are; None before OBS has a profile of reel's.
    pub fn read_files(&self) -> Option<Written> {
        let text = |p: &Path| {
            std::fs::read(p)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).to_string())
        };
        let json = |p: &Path| text(p).and_then(|t| serde_json::from_str::<Value>(&t).ok());
        Some(Written {
            profile: text(&self.config_root.join(PROFILE))?,
            encoder: json(&self.config_root.join(ENCODER)),
            collection: json(&self.config_root.join(SCENES)),
        })
    }

    /// What reel writes for a setup.
    pub fn render(&self, setup: &ObsSetup, clips_dir: &Path, save_key: &str) -> Written {
        let save = if setup.elevated && self.platform == Platform::Windows {
            obs_hotkey(save_key)
        } else {
            None
        };
        Written {
            profile: profile_ini(setup, self.platform, clips_dir, save),
            encoder: Some(setup.encoder_settings()),
            collection: Some(collection(setup, self.platform)),
        }
    }

    /// The setup as OBS has it, `profile` and `live` from the running OBS laid over its saved
    /// files; None while OBS has no profile of reel's.
    pub fn read(
        &self,
        setup: &ObsSetup,
        clips_dir: &Path,
        save_key: &str,
        profile: Option<&str>,
        live: Option<&Live>,
    ) -> Option<Taken> {
        let mut now = self.read_files()?;
        if let Some(p) = profile {
            now.profile = p.to_string();
        }
        if let (Some(c), Some(l)) = (now.collection.as_mut(), live) {
            overlay(c, l);
        }
        let ours = self.render(setup, clips_dir, save_key);
        let seen = |w: &Written| observe(&w.profile, w.encoder.as_ref(), w.collection.as_ref());
        Some(take(setup, self.platform, &seen(&ours), &seen(&now)))
    }

    /// The last lines of the log OBS wrote since `since`, with its path; None when it wrote
    /// none, which means it stopped before it found its own folders.
    pub fn log_tail(
        &self,
        since: std::time::SystemTime,
        lines: usize,
    ) -> Option<(PathBuf, Vec<String>)> {
        let newest = newest_txt(&self.logs_dir(), since)?;
        let text = String::from_utf8_lossy(&std::fs::read(&newest).ok()?).to_string();
        let all: Vec<&str> = text.lines().collect();
        let tail = all[all.len().saturating_sub(lines)..]
            .iter()
            .map(|l| l.to_string())
            .collect();
        Some((newest, tail))
    }

    /// The whole log of OBS's newest run written since `since`.
    pub fn log_since(&self, since: std::time::SystemTime) -> Option<String> {
        let newest = newest_txt(&self.logs_dir(), since)?;
        Some(String::from_utf8_lossy(&std::fs::read(newest).ok()?).into_owned())
    }

    /// The video encoders OBS listed at its newest start that got as far; None before any.
    /// A start held by a dialog lists none yet, so older logs answer for it.
    pub fn encoders_listed(&self) -> Option<Vec<String>> {
        let mut logs: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(self.logs_dir())
            .ok()?
            .flatten()
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .filter(|(_, p)| p.extension().is_some_and(|x| x == "txt"))
            .collect();
        logs.sort_by_key(|a| std::cmp::Reverse(a.0));
        logs.iter().find_map(|(_, p)| {
            crate::encoders::listed(&String::from_utf8_lossy(&std::fs::read(p).ok()?))
        })
    }

    /// The first lines of the crash report OBS wrote since `since`, with its path.
    pub fn crash_report(
        &self,
        since: std::time::SystemTime,
        lines: usize,
    ) -> Option<(PathBuf, Vec<String>)> {
        let newest = newest_txt(&self.obs_dir().join("crashes"), since)?;
        let text = String::from_utf8_lossy(&std::fs::read(&newest).ok()?).to_string();
        let head = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(lines)
            .map(str::to_string)
            .collect();
        Some((newest, head))
    }

    /// Puts what OBS wrote of a run started at `since` into the app's log after `what` (OBS
    /// exited, a save failed): the end of its newest log and the head of a crash report, as
    /// the user may not reach OBS's folder and a log they send should name the cause.
    pub fn log_what_obs_said(&self, since: std::time::SystemTime, what: &str) {
        match self.log_tail(since, OBS_LOG_LINES) {
            Some((path, lines)) => tracing::warn!(
                log = %path.display(),
                "{what}; the end of OBS's own log:\n{}",
                lines.join("\n")
            ),
            None => tracing::warn!("{what}; OBS wrote no log of this run"),
        }
        if let Some((path, lines)) = self.crash_report(since, 12) {
            tracing::warn!(
                report = %path.display(),
                "OBS wrote a crash report:\n{}",
                lines.join("\n")
            );
        }
    }

    /// OBS leaves a file per run in .sentinel and removes it on a clean exit; one left over
    /// makes the next start ask about safe mode.
    pub fn clear_crash_marks(&self) {
        let dir = self.obs_dir().join(".sentinel");
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                if e.file_name().to_string_lossy().starts_with("run_") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
}

/// The newest .txt in `dir` written since `since`: OBS names its logs and crash reports by date.
fn newest_txt(dir: &Path, since: std::time::SystemTime) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(t, p)| *t >= since && p.extension().is_some_and(|x| x == "txt"))
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p)
}

/// Keeps PipeWire's restore token in a collection written whole, or the desktop asks for the
/// screen again at OBS's next start.
fn keep_restore_token(had: Option<&str>, collection: &mut Value) {
    let Some(had) = had.and_then(|t| serde_json::from_str::<Value>(t).ok()) else {
        return;
    };
    let token = sources_named(&had, "Screen")["settings"]["RestoreToken"].clone();
    if !token.as_str().is_some_and(|t| !t.is_empty()) {
        return;
    }
    if let Some(screen) = collection["sources"]
        .as_array_mut()
        .and_then(|s| s.iter_mut().find(|s| s["name"] == "Screen"))
        .filter(|s| s["id"] == "pipewire-screen-capture-source")
    {
        screen["settings"]["RestoreToken"] = token;
    }
}

fn websocket_file(b: &Bundled, setup: &ObsSetup) -> String {
    let mut w: Value = std::fs::read_to_string(b.config_root.join(WEBSOCKET))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    for (k, v) in websocket_config(setup).as_object().into_iter().flatten() {
        w[k] = v.clone();
    }
    serde_json::to_string_pretty(&w).unwrap_or_default()
}

fn websocket_config(setup: &ObsSetup) -> Value {
    json!({
        "first_load": false,
        "server_enabled": true,
        "server_port": PORT,
        "alerts_enabled": false,
        "auth_required": true,
        "server_password": setup.password,
    })
}

fn merge_ini(had: Option<String>, wanted: &str) -> String {
    let mut out = had.unwrap_or_default();
    let mut section = String::new();
    for line in wanted.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
        } else if let Some((k, v)) = line.split_once('=') {
            out = ini_set(&out, &section, k, v);
        }
    }
    out
}

/// Sets one key in its section, keeping every other line and a leading BOM, which OBS writes.
pub(crate) fn ini_set(text: &str, section: &str, key: &str, value: &str) -> String {
    let (bom, body) = match text.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", text),
    };
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    let header = format!("[{section}]");
    let entry = format!("{key}={value}");
    match lines.iter().position(|l| l.trim() == header) {
        Some(start) => {
            let end = lines[start + 1..]
                .iter()
                .position(|l| l.trim_start().starts_with('['))
                .map(|i| start + 1 + i)
                .unwrap_or(lines.len());
            match lines[start + 1..end].iter().position(|l| {
                l.split_once('=')
                    .map(|(k, _)| k.trim() == key)
                    .unwrap_or(false)
            }) {
                Some(i) => lines[start + 1 + i] = entry,
                None => {
                    let mut at = end;
                    while at > start + 1 && lines[at - 1].trim().is_empty() {
                        at -= 1;
                    }
                    lines.insert(at, entry);
                }
            }
        }
        None => {
            if lines.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
            lines.push(header);
            lines.push(entry);
        }
    }
    format!("{bom}{}\n", lines.join("\n"))
}

pub(crate) fn ini_remove(text: &str, section: &str, key: &str) -> String {
    let (bom, body) = match text.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", text),
    };
    let header = format!("[{section}]");
    let mut inside = false;
    let lines: Vec<&str> = body
        .lines()
        .filter(|l| {
            let t = l.trim();
            if t.starts_with('[') {
                inside = t == header;
                return true;
            }
            !(inside
                && t.split_once('=')
                    .map(|(k, _)| k.trim() == key)
                    .unwrap_or(false))
        })
        .collect();
    format!("{bom}{}\n", lines.join("\n"))
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let part = path.with_extension("reel-part");
    std::fs::write(&part, text)?;
    std::fs::rename(&part, path)
}

fn global_ini() -> String {
    format!(
        "[General]\nLastVersion={}\nEnableAutoUpdates=false\nInfoIncrement=-1\n",
        api_version(&pinned().version)
    )
}

/// What OBS needs to start with reel's profile and ask nothing; `tray` where OBS hides itself.
fn user_ini_needed(tray: bool) -> String {
    format!(
        "[General]\nFirstRun=true\nConfirmOnExit=false\nWarnBeforeStartingStream=false\nWarnBeforeStoppingStream=false\nWarnBeforeStoppingRecord=false\nHotkeyFocusType=NeverDisableHotkeys\n\n[Basic]\nProfile={NAME}\nProfileDir={NAME}\nSceneCollection={NAME}\nSceneCollectionFile={NAME}\n\n[BasicWindow]\nSysTrayEnabled={tray}\nSysTrayWhenStarted={tray}\nSysTrayMinimizeToTray={tray}\n"
    )
}

/// With OBS's window kept plain the first time.
fn user_ini(tray: bool) -> String {
    format!(
        "{}ShowTransitions=false\nShowListboxToolbars=false\nShowStatusBar=false\n",
        user_ini_needed(tray)
    )
}

/// The app's names for keys beside letters, digits, F1 to F24 and Numpad0 to 9, with OBS's.
const KEYS: &[(&str, &str)] = &[
    ("Up", "UP"),
    ("Down", "DOWN"),
    ("Left", "LEFT"),
    ("Right", "RIGHT"),
    ("Space", "SPACE"),
    ("Enter", "RETURN"),
    ("Tab", "TAB"),
    ("Backspace", "BACKSPACE"),
    ("Delete", "DELETE"),
    ("Insert", "INSERT"),
    ("Home", "HOME"),
    ("End", "END"),
    ("PageUp", "PAGEUP"),
    ("PageDown", "PAGEDOWN"),
    ("PrintScreen", "PRINT"),
    ("ScrollLock", "SCROLLLOCK"),
    ("Pause", "PAUSE"),
    ("NumpadAdd", "NUMPLUS"),
    ("NumpadPlus", "NUMPLUS"),
    ("NumpadSubtract", "NUMMINUS"),
    ("NumpadMultiply", "NUMASTERISK"),
    ("NumpadDivide", "NUMSLASH"),
    ("NumpadDecimal", "NUMPERIOD"),
    ("Minus", "MINUS"),
    ("Equal", "EQUAL"),
    ("Comma", "COMMA"),
    ("Period", "PERIOD"),
    ("Slash", "SLASH"),
    ("Backslash", "BACKSLASH"),
    ("Semicolon", "SEMICOLON"),
    ("Quote", "APOSTROPHE"),
    ("Backquote", "QUOTELEFT"),
    ("BracketLeft", "BRACKETLEFT"),
    ("BracketRight", "BRACKETRIGHT"),
];

/// A hotkey as the app writes it (`Alt+F10`) in OBS's form; None for a key OBS has no name for.
pub fn obs_hotkey(combo: &str) -> Option<Value> {
    let parts: Vec<&str> = combo
        .split('+')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    let (key, mods) = parts.split_last()?;
    let named = match *key {
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphanumeric()) => k.to_ascii_uppercase(),
        k if k.starts_with('F') && k[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => {
            k.to_string()
        }
        k if k.starts_with("Numpad") && k[6..].parse::<u8>().is_ok_and(|n| n <= 9) => {
            format!("NUM{}", &k[6..])
        }
        k => KEYS.iter().find(|(app, _)| *app == k)?.1.to_string(),
    };
    let has = |m: &str| mods.iter().any(|x| x.eq_ignore_ascii_case(m));
    Some(json!({
        "key": format!("OBS_KEY_{named}"),
        "shift": has("Shift"),
        "control": has("Ctrl") || has("Control"),
        "alt": has("Alt"),
        "command": has("Super") || has("Win"),
    }))
}

/// The other way; None for a key the app has no name for.
pub fn app_hotkey(v: &Value) -> Option<String> {
    let name = v["key"].as_str()?.strip_prefix("OBS_KEY_")?;
    let key = match name {
        n if n.len() == 1 && n.chars().all(|c| c.is_ascii_alphanumeric()) => n.to_string(),
        n if n.starts_with('F') && n[1..].parse::<u8>().is_ok_and(|x| (1..=24).contains(&x)) => {
            n.to_string()
        }
        n if n.starts_with("NUM") && n[3..].parse::<u8>().is_ok_and(|x| x <= 9) => {
            format!("Numpad{}", &n[3..])
        }
        n => KEYS.iter().find(|(_, obs)| *obs == n)?.0.to_string(),
    };
    let on = |k: &str| v[k].as_bool().unwrap_or(false);
    let mut parts: Vec<String> = [
        ("control", "Ctrl"),
        ("alt", "Alt"),
        ("shift", "Shift"),
        ("command", "Super"),
    ]
    .iter()
    .filter(|(k, _)| on(k))
    .map(|(_, w)| w.to_string())
    .collect();
    parts.push(key);
    Some(parts.join("+"))
}

fn profile_ini(
    setup: &ObsSetup,
    platform: Platform,
    clips_dir: &Path,
    save: Option<Value>,
) -> String {
    let v = setup.video();
    let (num, den) = setup.video_mode().fps;
    let fps = if den == 1 {
        format!("FPSType=1\nFPSInt={num}")
    } else {
        format!("FPSType=2\nFPSNum={num}\nFPSDen={den}")
    };
    let mut s = format!(
        "[General]\nName={NAME}\n\n[Video]\nBaseCX={}\nBaseCY={}\nOutputCX={}\nOutputCY={}\n{}\nScaleType=bicubic\nColorFormat=NV12\nColorSpace=709\nColorRange=Partial\n\n[Audio]\nSampleRate=48000\nChannelSetup=Stereo\n\n[Output]\nMode=Advanced\n\n[AdvOut]\nRecType=Standard\nRecFilePath={}\nRecFormat2={}\nRecEncoder={}\nRecTracks={}\nRecAudioEncoder=ffmpeg_aac\nRecRB=true\nRecRBTime={}\nRecRBSize={}\nRecRBPrefix=Replay\nRecFileNameWithoutSpace=false\nTrackIndex=1\n",
        v.base.0,
        v.base.1,
        v.output.0,
        v.output.1,
        fps,
        plain(clips_dir).display(),
        setup.format,
        setup.encoder_id(),
        setup.tracks(platform),
        setup.clip_seconds.max(1),
        v.buffer_mb,
    );
    for (i, n) in setup.track_titles().iter().enumerate() {
        s.push_str(&format!(
            "Track{}Name={n}\nTrack{}Bitrate=160\n",
            i + 1,
            i + 1
        ));
    }
    s.push_str(&format!(
        "\n[Hotkeys]\nReplayBuffer={}\n",
        save_hotkeys(save)
    ));
    s
}

/// Empty unless OBS has the highest rights and takes the key itself; else the app's hotkey saves.
fn save_hotkeys(save: Option<Value>) -> Value {
    json!({ "ReplayBuffer.Save": save.into_iter().collect::<Vec<_>>() })
}

fn source(name: &str, id: &str, settings: Value, mixers: u32) -> Value {
    json!({
        "name": name,
        "uuid": uuid_of(name),
        "id": id,
        "versioned_id": id,
        "settings": settings,
        "mixers": mixers,
        "sync": 0,
        "flags": 0,
        "volume": 1.0,
        "balance": 0.5,
        "enabled": true,
        "muted": false,
        "push-to-mute": false,
        "push-to-talk": false,
        "hotkeys": {},
        "deinterlace_mode": 0,
        "monitoring_type": 0,
        "private_settings": {},
        "filters": [],
    })
}

/// A stable UUID per source name, so a written collection is the same every time.
fn uuid_of(name: &str) -> String {
    let h = crate::hash::sha256_hex(name.as_bytes());
    format!(
        "{}-{}-4{}-a{}-{}",
        &h[0..8],
        &h[8..12],
        &h[13..16],
        &h[17..20],
        &h[20..32]
    )
}

fn item(name: &str, id: u32, w: u32, h: u32) -> Value {
    json!({
        "name": name,
        "source_uuid": uuid_of(name),
        "visible": true,
        "locked": true,
        "rot": 0.0,
        "pos": { "x": 0.0, "y": 0.0 },
        "scale": { "x": 1.0, "y": 1.0 },
        "align": 5,
        "bounds_type": 2,
        "bounds_align": 0,
        "bounds": { "x": w as f64, "y": h as f64 },
        "crop_left": 0, "crop_top": 0, "crop_right": 0, "crop_bottom": 0,
        "id": id,
        "group_item_backup": false,
        "scale_filter": "disable",
        "blend_method": "default",
        "blend_type": "normal",
        "show_transition": { "duration": 0 },
        "hide_transition": { "duration": 0 },
        "private_settings": {},
    })
}

/// One scene: the screen with the game in front over it, as game capture shows nothing until
/// a fullscreen game runs.
pub fn collection(setup: &ObsSetup, platform: Platform) -> Value {
    let v = setup.video();
    let (w, h) = v.base;
    let mut sources = Vec::new();
    let mut items = Vec::new();
    let screen = match platform {
        Platform::Windows => source(
            "Screen",
            "monitor_capture",
            json!({ "monitor_id": setup.screen.id, "method": 0, "capture_cursor": true }),
            0,
        ),
        Platform::Linux if setup.screen.id == PIPEWIRE => source(
            "Screen",
            "pipewire-screen-capture-source",
            json!({ "ShowCursor": true }),
            0,
        ),
        Platform::Linux => source(
            "Screen",
            "xshm_input",
            json!({ "screen": setup.screen.id.parse::<u32>().unwrap_or(0), "show_cursor": true }),
            0,
        ),
    };
    sources.push(screen);
    items.push(item("Screen", 1, w, h));
    let audio = setup.audio(platform);
    let mixers_of = |name: &str| audio.iter().find(|s| s.name == name).map(|s| s.mixers);
    if setup.game && platform == Platform::Windows {
        let heard = mixers_of("Game");
        sources.push(source(
            "Game",
            "game_capture",
            json!({
                "capture_mode": "any_fullscreen",
                "capture_cursor": false,
                "allow_transparency": false,
                "anti_cheat_hook": true,
                "capture_audio": heard.is_some(),
                "hook_rate": 1,
            }),
            heard.unwrap_or(0),
        ));
        items.push(item("Game", 2, w, h));
    }
    if setup.game && platform == Platform::Linux {
        sources.push(source(
            "Game",
            "vkcapture-source",
            json!({ "window": "", "show_cursor": false, "allow_transparency": false }),
            0,
        ));
        items.push(item("Game", 2, w, h));
    }
    let (output_id, input_id) = match platform {
        Platform::Windows => ("wasapi_output_capture", "wasapi_input_capture"),
        Platform::Linux => ("pulse_output_capture", "pulse_input_capture"),
    };
    if let Some(m) = mixers_of("Desktop audio") {
        sources.push(source(
            "Desktop audio",
            output_id,
            json!({ "device_id": setup.desktop_audio }),
            m,
        ));
    }
    if let (Some(m), Some(device)) = (mixers_of("Microphone"), &setup.mic) {
        let mut mic = source("Microphone", input_id, json!({ "device_id": device }), m);
        mic["filters"] = json!(setup.mic_filters());
        sources.push(mic);
    }
    let mut next = 3;
    for (name, window) in [
        ("Game audio", setup.game_audio.as_str()),
        ("Discord", "Discord:Chrome_WidgetWin_1:Discord.exe"),
    ] {
        if let Some(m) = mixers_of(name) {
            let s = match platform {
                Platform::Windows => source(
                    name,
                    "wasapi_process_output_capture",
                    json!({ "window": window, "priority": 2 }),
                    m,
                ),
                Platform::Linux => source(
                    name,
                    "pipewire_audio_application_capture",
                    json!({ "CaptureMode": 0, "MatchPriorty": 0, "TargetName": program_of(window) }),
                    m,
                ),
            };
            sources.push(s);
            items.push(item(name, next, w, h));
            next += 1;
        }
    }
    for src in sources.iter_mut() {
        let name = src["name"].as_str().unwrap_or_default().to_string();
        if let Some(db) = setup.volumes.get(&name) {
            src["volume"] = json!(volume_mul(*db));
        }
    }
    let scene = json!({
        "name": NAME,
        "uuid": uuid_of("scene"),
        "id": "scene",
        "versioned_id": "scene",
        "settings": { "id_counter": items.len(), "custom_size": false, "items": items },
        "mixers": 0,
        "sync": 0,
        "flags": 0,
        "volume": 1.0,
        "balance": 0.5,
        "enabled": true,
        "muted": false,
        "hotkeys": {},
        "private_settings": {},
        "filters": [],
    });
    sources.insert(0, scene);
    let mut c = json!({
        "name": NAME,
        "current_scene": NAME,
        "current_program_scene": NAME,
        "scene_order": [{ "name": NAME }],
        "sources": sources,
        "groups": [],
        "transitions": [],
        "current_transition": "Cut",
        "transition_duration": 0,
        "quick_transitions": [],
        "saved_projectors": [],
        "preview_locked": true,
        "scaling_enabled": false,
        "modules": {},
        "version": 2,
    });
    for (key, name) in [
        ("DesktopAudioDevice1", "Desktop audio"),
        ("AuxAudioDevice1", "Microphone"),
    ] {
        let s = sources_named(&c, name);
        if !s.is_null() {
            c[key] = s;
        }
    }
    c
}

fn sources_named(c: &Value, name: &str) -> Value {
    c["sources"]
        .as_array()
        .and_then(|a| a.iter().find(|s| s["name"] == name).cloned())
        .unwrap_or(Value::Null)
}

/// A password for the bundled OBS's WebSocket: 256 bits from the OS.
pub fn new_password() -> String {
    reel_core::ids::new_device_token()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ini_key_is_set_in_its_section_and_nothing_else_moves() {
        let had = "\u{feff}[General]\nName=mine\n\n[AdvOut]\nRecEncoder=jim_nvenc\nRecRBTime=20\n\n[Video]\nBaseCX=2560\n";
        let out = ini_set(had, "AdvOut", "RecRBTime", "60");
        assert_eq!(out, "\u{feff}[General]\nName=mine\n\n[AdvOut]\nRecEncoder=jim_nvenc\nRecRBTime=60\n\n[Video]\nBaseCX=2560\n");
        let out = ini_set(&out, "AdvOut", "RecRB", "true");
        assert!(
            out.contains("RecRBTime=60\nRecRB=true\n\n[Video]"),
            "a new key at the end of its section: {out}"
        );
        let out = ini_set(&out, "Hotkeys", "ReplayBuffer", "{}");
        assert!(
            out.ends_with("BaseCX=2560\n\n[Hotkeys]\nReplayBuffer={}\n"),
            "{out}"
        );
        assert_eq!(
            ini_value(&out, "General", "Name").as_deref(),
            Some("mine"),
            "the byte order mark read past"
        );
    }

    #[test]
    fn a_start_writes_only_what_reel_needs() {
        let dir = tempfile::tempdir().unwrap();
        let b = Bundled {
            exe: dir.path().join("obs64.exe"),
            config_root: dir.path().to_path_buf(),
            platform: Platform::Windows,
            flatpak: false,
        };
        let s = ObsSetup {
            clip_seconds: 90,
            password: "pw".into(),
            ..ObsSetup::default()
        };
        let clips = dir.path().join("clips");
        b.ensure(&s, &clips, "Alt+F10").unwrap();
        let obs = dir.path().join("obs-studio");
        let profile = obs.join("basic/profiles/reel/basic.ini");
        let first = std::fs::read_to_string(&profile).unwrap();
        assert!(
            first.contains("RecRBTime=90") && first.contains("[Video]"),
            "the whole setup, the first time"
        );
        std::fs::write(&profile, "[General]\nName=reel\n\n[Output]\nMode=Simple\n\n[SimpleOutput]\nRecEncoder=nvenc\nRecRB=false\nRecRBTime=30\nFilePath=D:\\Mine\n").unwrap();
        let scenes = obs.join("basic/scenes/reel.json");
        std::fs::write(
            &scenes,
            r#"{"name":"reel","sources":[{"name":"My webcam"}]}"#,
        )
        .unwrap();
        std::fs::write(
            obs.join("plugin_config/obs-websocket/config.json"),
            r#"{"server_port":4455,"server_password":"x","alerts_enabled":true,"custom":1}"#,
        )
        .unwrap();
        std::fs::write(
            obs.join("user.ini"),
            "[General]\nFirstRun=true\n\n[BasicWindow]\nShowStatusBar=true\n",
        )
        .unwrap();
        b.ensure(
            &ObsSetup {
                clip_seconds: 120,
                ..s.clone()
            },
            &clips,
            "Alt+F10",
        )
        .unwrap();
        let p = std::fs::read_to_string(&profile).unwrap();
        assert!(
            p.contains("RecEncoder=nvenc")
                && p.contains("Mode=Simple")
                && p.contains("RecRBTime=30")
                && p.contains("FilePath=D:\\Mine"),
            "OBS's own settings stand, the clip length among them: {p}"
        );
        assert!(
            p.contains("RecRB=true") && !p.contains("RecRB=false"),
            "the replay buffer on, in the mode OBS is in: {p}"
        );
        assert!(
            !p.contains("[AdvOut]"),
            "nothing of the advanced mode added: {p}"
        );
        assert_eq!(
            std::fs::read_to_string(&scenes).unwrap(),
            r#"{"name":"reel","sources":[{"name":"My webcam"}]}"#,
            "the scenes untouched"
        );
        let w: Value = serde_json::from_str(
            &std::fs::read_to_string(obs.join("plugin_config/obs-websocket/config.json")).unwrap(),
        )
        .unwrap();
        assert!(
            w["server_port"] == PORT
                && w["server_password"] == "pw"
                && w["alerts_enabled"] == false
                && w["custom"] == 1,
            "{w}"
        );
        let u = std::fs::read_to_string(obs.join("user.ini")).unwrap();
        assert!(
            u.contains("ShowStatusBar=true")
                && u.contains("ConfirmOnExit=false")
                && u.contains("SceneCollection=reel"),
            "{u}"
        );
    }

    #[test]
    fn a_change_of_reels_writes_only_what_it_touches() {
        let dir = tempfile::tempdir().unwrap();
        let b = Bundled {
            exe: dir.path().join("obs64.exe"),
            config_root: dir.path().to_path_buf(),
            platform: Platform::Windows,
            flatpak: false,
        };
        let s = ObsSetup {
            screen: Screen {
                width: 2560,
                height: 1440,
                hz: 144,
                ..Default::default()
            },
            ..ObsSetup::default()
        };
        let clips = dir.path().join("clips");
        b.write(&s, &clips, "Alt+F10").unwrap();
        let obs = dir.path().join("obs-studio");
        let profile = obs.join("basic/profiles/reel/basic.ini");
        let scenes = obs.join("basic/scenes/reel.json");
        let t = std::fs::read_to_string(&profile).unwrap();
        std::fs::write(&profile, ini_set(&t, "AdvOut", "RecRBTime", "120")).unwrap();
        let mut c: Value =
            serde_json::from_str(&std::fs::read_to_string(&scenes).unwrap()).unwrap();
        c["sources"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "name": "Webcam", "id": "dshow_input" }));
        std::fs::write(&scenes, c.to_string()).unwrap();
        let sharp = ObsSetup {
            quality: Quality::Sharp,
            ..s.clone()
        };
        b.put(&b.change(&b, (&s, &clips, "Alt+F10"), (&sharp, &clips, "Alt+F10")))
            .unwrap();
        let p = std::fs::read_to_string(&profile).unwrap();
        assert!(
            p.contains("OutputCX=2560") && p.contains("RecRBTime=120"),
            "the quality written, OBS's clip length kept: {p}"
        );
        assert!(std::fs::read_to_string(&scenes).unwrap().contains("Webcam"));
        let longer = ObsSetup {
            clip_seconds: 90,
            ..sharp.clone()
        };
        b.put(&b.change(
            &b,
            (&sharp, &clips, "Alt+F10"),
            (&longer, &clips, "Alt+F10"),
        ))
        .unwrap();
        assert!(std::fs::read_to_string(&profile)
            .unwrap()
            .contains("RecRBTime=90"));
    }

    #[test]
    fn each_source_keeps_its_volume() {
        let mut s = ObsSetup {
            mic: Some("default".into()),
            ..Default::default()
        };
        s.volumes.insert("Microphone".into(), -6.0);
        s.volumes.insert("Desktop audio".into(), 0.0);
        let c = collection(&s, Platform::Windows);
        let mic = sources_named(&c, "Microphone")["volume"].as_f64().unwrap();
        assert!((mic - 0.501).abs() < 0.001, "-6 dB is about half: {mic}");
        assert_eq!(
            c["AuxAudioDevice1"]["volume"].as_f64(),
            Some(mic),
            "the global copy too"
        );
        assert_eq!(sources_named(&c, "Desktop audio")["volume"], 1.0);
    }

    #[test]
    fn the_microphone_gets_the_noise_suppression_picked() {
        let mut s = ObsSetup::default();
        assert!(s.mic_filters().is_empty(), "no microphone, no filter");
        s.mic = Some("{0.0.1.00000000}.{yeti}".into());
        let f = s.mic_filters();
        assert_eq!(f.len(), 1, "Strong by default once there is a microphone");
        assert_eq!(f[0]["versioned_id"], "noise_suppress_filter_v2");
        assert_eq!(f[0]["settings"]["method"], "rnnoise");
        s.noise = Noise::Speex;
        s.gate = true;
        let f = s.mic_filters();
        assert_eq!(f[0]["settings"]["method"], "speex");
        assert_eq!(f[1]["id"], "noise_gate_filter");
        s.noise = Noise::Nvidia;
        assert_eq!(s.mic_filters()[0]["versioned_id"], "nvidia_audiofx_filter");
        s.noise = Noise::Off;
        assert_eq!(s.mic_filters().len(), 1, "the gate alone");
        s.noise = Noise::Own;
        s.own_filters = vec![
            json!({ "name": "Gain", "id": "gain_filter", "versioned_id": "gain_filter", "settings": { "db": 6.0 }, "enabled": false }),
        ];
        assert_eq!(
            s.mic_filters(),
            s.own_filters,
            "the own filters as they are, nothing added"
        );
        let c = collection(&s, Platform::Windows);
        assert_eq!(c["AuxAudioDevice1"]["filters"][0]["name"], "Gain");
    }

    #[test]
    fn a_running_obs_gets_the_wanted_filters_in_order() {
        let current = vec![json!({ "filterName": "Noise suppression" })];
        let wanted = vec![
            json!({ "name": "Gain", "id": "gain_filter", "versioned_id": "gain_filter", "settings": { "db": 6.0 }, "enabled": false }),
            json!({ "name": "Noise suppression", "id": "noise_suppress_filter", "versioned_id": "noise_suppress_filter_v2", "settings": { "method": "rnnoise" } }),
        ];
        let r = filter_requests("Microphone", &current, &wanted);
        let kinds: Vec<&str> = r.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "RemoveSourceFilter",
                "CreateSourceFilter",
                "SetSourceFilterEnabled",
                "CreateSourceFilter"
            ]
        );
        assert_eq!(r[3].1["filterKind"], "noise_suppress_filter_v2");
        assert_eq!(r[1].1["filterSettings"]["db"], 6.0);
    }

    #[test]
    fn the_own_obs_microphone_filters_are_read_from_its_current_collection() {
        let dir = tempfile::tempdir().unwrap();
        let scenes = dir.path().join("basic").join("scenes");
        std::fs::create_dir_all(&scenes).unwrap();
        std::fs::write(dir.path().join("user.ini"), "[General]\nFirstRun=true\n\n[Basic]\nSceneCollection=Gaming\nSceneCollectionFile=Gaming\n").unwrap();
        std::fs::write(
            scenes.join("Untitled.json"),
            r#"{"name":"Untitled","sources":[]}"#,
        )
        .unwrap();
        std::fs::write(
            scenes.join("Gaming.json"),
            r#"{"name":"Gaming","sources":[{"name":"Game","id":"game_capture","filters":[{"name":"Crop","id":"crop_filter"}]}],
                "AuxAudioDevice1":{"name":"Mic/Aux","id":"wasapi_input_capture","versioned_id":"wasapi_input_capture",
                  "filters":[{"name":"Noise Suppression","id":"noise_suppress_filter","versioned_id":"noise_suppress_filter_v2","settings":{"method":"rnnoise"},"enabled":true},
                             {"name":"Compressor","id":"compressor_filter","versioned_id":"compressor_filter","settings":{"ratio":4.0},"enabled":true}]}}"#,
        )
        .unwrap();
        let before = std::fs::read(scenes.join("Gaming.json")).unwrap();
        let m = own_mic(dir.path()).unwrap();
        assert_eq!(
            (m.name.as_str(), m.collection.as_str()),
            ("Mic/Aux", "Gaming")
        );
        assert_eq!(m.filters.len(), 2);
        assert_eq!(m.filters[1]["name"], "Compressor");
        assert_eq!(
            std::fs::read(scenes.join("Gaming.json")).unwrap(),
            before,
            "read only"
        );
        std::fs::write(
            scenes.join("Gaming.json"),
            r#"{"name":"Gaming","sources":[]}"#,
        )
        .unwrap();
        assert!(
            own_mic(dir.path()).is_none(),
            "no microphone with filters, nothing offered"
        );
    }

    #[test]
    fn every_path_handed_to_obs_is_a_plain_drive_path() {
        let verbatim = |s: &str| s.contains(r"\\?\");
        let b = Bundled::windows(Path::new(r"\\?\C:\Users\Robin\AppData\Local\reel"));
        for p in [&b.exe, &b.workdir(), &b.config_root, &b.obs_dir()] {
            let p = p.to_string_lossy();
            assert!(p.starts_with(r"C:\Users\Robin\AppData\Local\reel"), "{p}");
        }
        assert!(b.args(true).iter().all(|a| !verbatim(a)));
        let task = crate::elevated::copy_args(&b).join(" ");
        assert!(!verbatim(&task), "{task}");
        let line = crate::elevated::helper_args(
            crate::elevated::Step::Install,
            &crate::elevated::obs_root(&b),
            Path::new(r"C:\Temp\a.txt"),
            r"PC\Robin",
        );
        assert!(!verbatim(&line), "{line}");
        let ini = profile_ini(
            &ObsSetup::default(),
            Platform::Windows,
            Path::new(r"\\?\D:\Clips\reel"),
            None,
        );
        assert!(ini.contains(r"RecFilePath=D:\Clips\reel") && !verbatim(&ini));
        assert_eq!(
            plain(Path::new(r"\\?\UNC\nas\clips")),
            PathBuf::from(r"\\nas\clips")
        );
        assert_eq!(plain(Path::new(r"C:\x")), PathBuf::from(r"C:\x"));
        assert_eq!(
            plain(Path::new(r"\\?\Volume{1}\x")),
            PathBuf::from(r"\\?\Volume{1}\x")
        );
    }

    fn screen(w: u32, h: u32, hz: u32) -> Screen {
        Screen {
            id: r"\\?\DISPLAY#GSM5B7F#5&1".into(),
            name: "LG 27GP850".into(),
            width: w,
            height: h,
            hz,
            main: true,
        }
    }

    #[test]
    fn hotkeys_in_obs_words() {
        assert_eq!(
            obs_hotkey("Ctrl+Alt+Shift+S").unwrap(),
            json!({ "key": "OBS_KEY_S", "shift": true, "control": true, "alt": true, "command": false })
        );
        assert_eq!(obs_hotkey("Shift+F8").unwrap()["key"], "OBS_KEY_F8");
        assert_eq!(obs_hotkey("Super+Numpad5").unwrap()["command"], true);
        assert_eq!(
            obs_hotkey("Ctrl+PrintScreen").unwrap()["key"],
            "OBS_KEY_PRINT"
        );
        assert!(obs_hotkey("").is_none() && obs_hotkey("Ctrl+MediaPlayPause").is_none());
        for combo in [
            "Alt+F10",
            "Ctrl+Alt+Shift+S",
            "Super+Numpad5",
            "Ctrl+PrintScreen",
            "F8",
            "Alt+NumpadAdd",
            "Ctrl+BracketLeft",
        ] {
            assert_eq!(
                app_hotkey(&obs_hotkey(combo).unwrap()).as_deref(),
                Some(combo),
                "read back as the app writes it"
            );
        }
        assert_eq!(
            app_hotkey(&json!({ "key": "OBS_KEY_F9", "alt": true })).as_deref(),
            Some("Alt+F9"),
            "OBS leaves out the modifiers that are off"
        );
        assert!(app_hotkey(&json!({ "key": "OBS_KEY_MEDIA_PLAY" })).is_none());
    }

    #[test]
    fn the_presets_come_to_what_they_say() {
        let mut s = ObsSetup {
            screen: screen(2560, 1440, 165),
            ..Default::default()
        };
        let v = s.video();
        assert_eq!((v.base, v.output, v.fps), ((2560, 1440), (1920, 1080), 60));
        assert_eq!(
            v.kbit, 25000,
            "about 0.2 bits a pixel, in steps of 500 kbit/s"
        );
        s.quality = Quality::Sharp;
        assert_eq!(s.video().output, (2560, 1440));
        s.quality = Quality::Native;
        let v = s.video();
        assert_eq!((v.output, v.fps), ((2560, 1440), 165));
        let ultrawide = ObsSetup {
            screen: screen(3440, 1440, 144),
            ..Default::default()
        };
        assert_eq!(
            ultrawide.video().output,
            (2580, 1080),
            "the screen's shape kept"
        );
        let small = ObsSetup {
            screen: screen(1280, 720, 60),
            quality: Quality::Sharp,
            ..Default::default()
        };
        assert_eq!(
            small.video().output,
            (1280, 720),
            "never larger than the screen"
        );
    }

    #[test]
    fn the_replay_buffer_holds_the_clip_length_at_the_bitrate() {
        let s = ObsSetup {
            screen: screen(1920, 1080, 60),
            clip_seconds: 120,
            ..Default::default()
        };
        let v = s.video();
        assert_eq!(v.buffer_mb, (25000 + 160) * 120 * 5 / 4 / 8 / 1000);
        let s = ObsSetup {
            kbit: 8000,
            clip_seconds: 30,
            ..s
        };
        assert_eq!(s.video().buffer_mb, 64, "never under 64 MB");
    }

    #[test]
    fn quick_sync_gets_its_highest_bitrate_instead_of_one_that_wraps_around() {
        let native = ObsSetup {
            screen: screen(2560, 1440, 165),
            quality: Quality::Native,
            encoder: "obs_qsv11_v2".into(),
            ..Default::default()
        };
        assert_eq!(native.video().kbit, 121500, "the preset's own");
        let e = native.encoder_settings();
        assert_eq!(
            (
                e["rate_control"].as_str(),
                e["bitrate"].as_u64(),
                e["target_usage"].as_str()
            ),
            (Some("CBR"), Some(65535), Some("TU4"))
        );
        let nvenc = ObsSetup {
            encoder: "obs_nvenc_h264_tex".into(),
            ..native
        };
        assert_eq!(
            nvenc.encoder_settings()["bitrate"],
            121500,
            "NVENC takes it whole"
        );
    }

    #[test]
    fn tracks_and_encoders() {
        let mut s = ObsSetup::default();
        assert_eq!(s.tracks(Platform::Windows), FULL, "the full mix alone");
        s.own_tracks = true;
        assert_eq!(
            s.tracks(Platform::Windows),
            FULL | DESKTOP,
            "only what was chosen"
        );
        s.mic = Some("default".into());
        s.game_audio = "front".into();
        s.discord = true;
        assert_eq!(
            s.tracks(Platform::Windows),
            0b11111,
            "the full mix, the game, the microphone, Discord, the desktop"
        );
        assert_eq!(
            s.tracks(Platform::Linux),
            FULL | MIC | DISCORD | DESKTOP,
            "Discord through the PipeWire audio capture; no game in front on Linux"
        );
        assert_eq!(
            s.track_names(Platform::Linux),
            ["Full mix", "Microphone", "Discord", "Desktop"]
        );
        assert_eq!(
            s.track_names(Platform::Windows),
            ["Full mix", "Game", "Microphone", "Discord", "Desktop"]
        );
        let ini = profile_ini(&s, Platform::Windows, Path::new("C:/clips"), None);
        assert!(
            ini.contains("RecTracks=31")
                && ini.contains("Track1Name=Full mix\nTrack1Bitrate=160\nTrack2Name=Game\n")
                && ini.contains("Track3Name=Microphone")
                && ini.contains("Track4Name=Discord")
                && ini.contains("Track5Name=Desktop"),
            "{ini}"
        );
        assert_eq!(s.encoder_settings()["rate_control"], "CBR");
        assert_eq!(s.encoder_id(), "obs_x264");
        s.encoder = "obs_nvenc_h264_tex".into();
        s.rate = RateMode::Quality;
        let e = s.encoder_settings();
        assert_eq!(
            (
                e["rate_control"].as_str(),
                e["cqp"].as_u64(),
                e["keyint_sec"].as_u64()
            ),
            (Some("CQP"), Some(20), Some(1))
        );
    }

    #[test]
    fn the_collection_puts_the_game_over_the_screen_and_routes_the_tracks() {
        let s = ObsSetup {
            screen: screen(2560, 1440, 165),
            own_tracks: true,
            mic: Some("{0.0.1.00000000}.{mic}".into()),
            game_audio: "front".into(),
            discord: true,
            ..Default::default()
        };
        let c = collection(&s, Platform::Windows);
        let names: Vec<&str> = c["sources"][0]["settings"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Screen", "Game", "Discord"], "bottom to top");
        let mixers = |n: &str| sources_named(&c, n)["mixers"].as_u64().unwrap();
        assert_eq!(
            (
                mixers("Desktop audio"),
                mixers("Game"),
                mixers("Discord"),
                mixers("Microphone")
            ),
            (
                (FULL | DESKTOP) as u64,
                GAME as u64,
                DISCORD as u64,
                (FULL | MIC) as u64
            ),
            "the game and Discord are in desktop audio: their own tracks only"
        );
        assert_eq!(sources_named(&c, "Game")["settings"]["capture_audio"], true);
        assert_eq!(c["DesktopAudioDevice1"]["id"], "wasapi_output_capture");
        assert_eq!(
            c["AuxAudioDevice1"]["settings"]["device_id"],
            "{0.0.1.00000000}.{mic}"
        );
        assert_eq!(
            sources_named(&c, "Screen")["settings"]["monitor_id"],
            s.screen.id
        );
        let program = ObsSetup {
            desktop: false,
            game_audio: "Valorant:UnrealWindow:VALORANT-Win64-Shipping.exe".into(),
            ..s.clone()
        };
        let c = collection(&program, Platform::Windows);
        assert!(
            sources_named(&c, "Desktop audio").is_null() && c.get("DesktopAudioDevice1").is_none(),
            "desktop audio off: not in OBS"
        );
        let ga = sources_named(&c, "Game audio");
        assert_eq!(
            (
                ga["id"].as_str(),
                ga["settings"]["window"].as_str(),
                ga["mixers"].as_u64()
            ),
            (
                Some("wasapi_process_output_capture"),
                Some("Valorant:UnrealWindow:VALORANT-Win64-Shipping.exe"),
                Some((FULL | GAME) as u64)
            ),
            "a chosen program, in the full mix without desktop audio"
        );
        assert_eq!(
            sources_named(&c, "Game")["settings"]["capture_audio"],
            false,
            "the game capture's own audio not twice"
        );
        let quiet = collection(&ObsSetup::default(), Platform::Windows);
        assert!(
            sources_named(&quiet, "Microphone").is_null() && quiet.get("AuxAudioDevice1").is_none(),
            "no microphone chosen: none in OBS"
        );
        let linux = collection(&ObsSetup::default(), Platform::Linux);
        assert_eq!(sources_named(&linux, "Screen")["id"], "xshm_input");
        assert_eq!(sources_named(&linux, "Screen")["settings"]["screen"], 0);
        assert_eq!(
            sources_named(&linux, "Game")["id"],
            "vkcapture-source",
            "a game through obs-vkcapture, any game"
        );
        assert_eq!(sources_named(&linux, "Game")["settings"]["window"], "");
        let screen_only = ObsSetup {
            game: false,
            ..ObsSetup::default()
        };
        assert!(sources_named(&collection(&screen_only, Platform::Linux), "Game").is_null());
    }

    #[test]
    fn linux_takes_the_screen_through_pipewire_on_wayland_and_programs_by_name() {
        let s = ObsSetup {
            game: false,
            screen: Screen {
                id: PIPEWIRE.into(),
                ..Screen::default()
            },
            desktop: false,
            game_audio: "FSD-Win64-Shipping.exe".into(),
            discord: true,
            ..ObsSetup::default()
        };
        let c = collection(&s, Platform::Linux);
        let screen = sources_named(&c, "Screen");
        assert_eq!(screen["id"], "pipewire-screen-capture-source");
        assert_eq!(screen["settings"]["ShowCursor"], true);
        assert!(
            screen["settings"].get("RestoreToken").is_none(),
            "OBS writes its own"
        );
        let game = sources_named(&c, "Game audio");
        assert_eq!(game["id"], "pipewire_audio_application_capture");
        assert_eq!(game["settings"]["TargetName"], "FSD-Win64-Shipping.exe");
        assert_eq!(game["settings"]["CaptureMode"], 0);
        assert_eq!(
            sources_named(&c, "Discord")["settings"]["TargetName"],
            "Discord"
        );

        let dir = tempfile::tempdir().unwrap();
        let b = Bundled::linux(PathBuf::from("/usr/bin/obs"), dir.path());
        let clips = dir.path().join("clips");
        b.write(&s, &clips, "").unwrap();
        let path = b.config_root.join(SCENES);
        let mut saved: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for src in saved["sources"].as_array_mut().unwrap() {
            if src["name"] == "Screen" {
                src["settings"]["RestoreToken"] = json!("a-token-of-the-portal");
            }
        }
        std::fs::write(&path, serde_json::to_string_pretty(&saved).unwrap()).unwrap();
        let longer = ObsSetup {
            clip_seconds: 90,
            ..s.clone()
        };
        b.put(&b.change(&b, (&s, &clips, ""), (&longer, &clips, "")))
            .unwrap();
        let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            sources_named(&after, "Screen")["settings"]["RestoreToken"],
            "a-token-of-the-portal",
            "the desktop's answer survives a change of reel's settings, so it asks once"
        );
        b.write(&longer, &clips, "").unwrap();
        let again: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            sources_named(&again, "Screen")["settings"]["RestoreToken"],
            "a-token-of-the-portal",
            "and the setup written whole again, as Run the setup again does"
        );
        let x11 = ObsSetup {
            screen: Screen {
                id: "0".into(),
                ..Screen::default()
            },
            ..longer.clone()
        };
        b.write(&x11, &clips, "").unwrap();
        let other: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(
            sources_named(&other, "Screen")["settings"]
                .get("RestoreToken")
                .is_none(),
            "a screen taken otherwise has no token of PipeWire's"
        );
    }

    #[cfg(unix)]
    #[test]
    fn reels_obs_on_linux_runs_from_a_copy_under_reels_name() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("usr/bin/obs");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"\x7fELF OBS 32").unwrap();
        let ours = dir.path().join("data/obs");
        let run = own_name(&exe, &ours);
        assert_eq!(run, ours.join("program/bin/reel-capture"));
        assert_eq!(std::fs::read(&run).unwrap(), b"\x7fELF OBS 32");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&run).unwrap().permissions().mode() & 0o111,
                0o111
            );
        }
        assert_eq!(
            own_name(&exe, &ours),
            run,
            "the same program: the copy as it is"
        );
        std::fs::write(&exe, b"\x7fELF OBS 33").unwrap();
        assert_eq!(
            std::fs::read(own_name(&exe, &ours)).unwrap(),
            b"\x7fELF OBS 33",
            "an update of OBS is copied again"
        );
        let mux = dir.path().join("usr/bin/obs-ffmpeg-mux");
        std::fs::write(&mux, b"\x7fELF the muxer").unwrap();
        std::fs::write(dir.path().join("usr/bin/obsidian-notes"), b"not OBS's").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for f in [&mux, &dir.path().join("usr/bin/obsidian-notes")] {
                std::fs::set_permissions(f, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        assert_eq!(own_name(&exe, &ours), run);
        assert_eq!(
            std::fs::read_link(ours.join("program/bin/obs-ffmpeg-mux")).unwrap(),
            mux,
            "OBS's muxer beside the copy, where OBS looks for it"
        );
        assert!(
            !ours.join("program/bin/obsidian-notes").exists(),
            "nothing else of the folder"
        );
        let script = dir.path().join("usr/bin/obs-wrapper");
        std::fs::write(&script, "#!/bin/sh\nexec obs \"$@\"\n").unwrap();
        assert_eq!(own_name(&script, &ours), script, "a script runs as it is");
        assert_eq!(
            own_name(&dir.path().join("missing"), &ours),
            dir.path().join("missing")
        );
    }

    #[cfg(unix)]
    #[test]
    fn reels_copy_on_linux_finds_obs_33s_plugins_by_its_place_and_the_first_copy_goes() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let usr = dir.path().join("usr");
        let exe = usr.join("bin/obs");
        for d in [
            "bin",
            "lib/x86_64-linux-gnu/obs-plugins",
            "share/obs/obs-modules/core/obs-x264",
        ] {
            std::fs::create_dir_all(usr.join(d)).unwrap();
        }
        std::fs::write(
            usr.join("lib/x86_64-linux-gnu/obs-plugins/obs-x264.so"),
            "x264",
        )
        .unwrap();
        std::fs::write(usr.join("README"), "a file, not a folder").unwrap();
        std::fs::write(&exe, b"\x7fELF OBS 33").unwrap();
        let mux = usr.join("bin/obs-ffmpeg-mux");
        std::fs::write(&mux, b"\x7fELF the muxer").unwrap();
        std::fs::set_permissions(&mux, std::fs::Permissions::from_mode(0o755)).unwrap();
        let ours = dir.path().join("data/obs");
        std::fs::create_dir_all(ours.join("obs-studio/basic")).unwrap();
        std::fs::write(ours.join("reel-capture"), b"\x7fELF OBS 32").unwrap();
        std::os::unix::fs::symlink(&mux, ours.join("obs-ffmpeg-mux")).unwrap();

        let run = own_name(&exe, &ours);
        assert_eq!(run, ours.join("program/bin/reel-capture"));
        let program = ours.join("program");
        assert_eq!(
            std::fs::read_link(program.join("lib")).unwrap(),
            usr.join("lib")
        );
        assert_eq!(
            std::fs::read_link(program.join("share")).unwrap(),
            usr.join("share")
        );
        assert!(
            std::fs::symlink_metadata(program.join("bin"))
                .unwrap()
                .is_dir(),
            "bin is the copy's own folder"
        );
        assert!(!program.join("README").exists(), "folders only");
        assert_eq!(
            std::fs::read_to_string(
                run.parent()
                    .unwrap()
                    .join("../lib/x86_64-linux-gnu/obs-plugins/obs-x264.so")
            )
            .unwrap(),
            "x264",
            "OBS 33's plugin found by .. from the copy's folder, as from the installed one's"
        );
        assert!(run
            .parent()
            .unwrap()
            .join("../share/obs/obs-modules/core/obs-x264")
            .is_dir());
        assert!(
            !ours.join("reel-capture").exists() && !ours.join("obs-ffmpeg-mux").exists(),
            "the first copy and its link gone"
        );
        assert!(
            ours.join("obs-studio/basic").is_dir(),
            "OBS's settings stay"
        );

        std::fs::remove_dir_all(usr.join("share")).unwrap();
        std::fs::create_dir_all(usr.join("lib64")).unwrap();
        assert_eq!(own_name(&exe, &ours), run);
        assert!(
            std::fs::symlink_metadata(program.join("share")).is_err(),
            "a folder no longer beside the installed program: its link gone"
        );
        assert_eq!(
            std::fs::read_link(program.join("lib64")).unwrap(),
            usr.join("lib64")
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_installed_obs_is_found_on_the_path_else_from_flathub() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let (bin, home, data) = (
            dir.path().join("bin"),
            dir.path().join("home"),
            dir.path().join("data"),
        );
        std::fs::create_dir_all(&bin).unwrap();
        let flatpaks = [dir.path().join("user-apps"), dir.path().join("system-apps")];
        let path = std::env::join_paths([dir.path().join("nothing"), bin.clone()]).unwrap();
        assert!(
            installed(Some(&path), &home, &data, &flatpaks).is_none(),
            "none at all"
        );

        std::fs::create_dir_all(flatpaks[1].join(FLATPAK_ID)).unwrap();
        let f = installed(Some(&path), &home, &data, &flatpaks).unwrap();
        assert!(f.flatpak);
        assert_eq!(
            f.config_root,
            home.join(".var/app/com.obsproject.Studio/reel"),
            "where the sandbox may write"
        );
        assert_eq!(
            f.own_config().unwrap(),
            home.join(".var/app/com.obsproject.Studio/config/obs-studio")
        );
        let (program, args) = f.command();
        assert_eq!(program, PathBuf::from("flatpak"));
        assert_eq!(
            args[..3],
            [
                "run".to_string(),
                format!("--env=XDG_CONFIG_HOME={}", f.config_root.display()),
                FLATPAK_ID.to_string()
            ]
        );
        assert!(
            args.contains(&"--multi".to_string())
                && args.contains(&"--minimize-to-tray".to_string())
        );
        assert_eq!(f.workdir(), f.config_root);
        assert_eq!(f.which(), "OBS from Flathub");

        let obs = bin.join("obs");
        std::fs::write(&obs, "#!/bin/sh\n").unwrap();
        assert!(
            installed(Some(&path), &home, &data, &flatpaks)
                .unwrap()
                .flatpak,
            "a file that does not run is no OBS"
        );
        std::fs::set_permissions(&obs, std::fs::Permissions::from_mode(0o755)).unwrap();
        let n = installed(Some(&path), &home, &data, &flatpaks).unwrap();
        assert!(!n.flatpak, "the PATH's OBS before Flathub's");
        assert_eq!(
            (n.exe.clone(), n.config_root.clone()),
            (obs.clone(), data.join("obs"))
        );
        let (program, args) = n.command();
        assert_eq!(program, obs);
        assert_eq!(
            args[0], "--minimize-to-tray",
            "no app may hide another's window on Wayland"
        );
        assert!(!args.contains(&"--portable".to_string()));
        assert!(n.which().contains(&obs.display().to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn reel_s_second_obs_sees_the_users_plugins_and_writes_nothing_of_theirs() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let f = Bundled::flatpak(&home);
        let theirs = f.own_config().unwrap();
        std::fs::create_dir_all(theirs.join("plugins").join("linux-pipewire-audio")).unwrap();
        std::fs::write(theirs.join("global.ini"), "[General]\nMaxLogs=10\n").unwrap();
        f.link_plugins();
        let link = f.config_root.join("obs-studio").join("plugins");
        assert_eq!(std::fs::read_link(&link).unwrap(), theirs.join("plugins"));
        assert!(link.join("linux-pipewire-audio").is_dir());
        f.link_plugins();
        assert_eq!(
            std::fs::read_to_string(theirs.join("global.ini")).unwrap(),
            "[General]\nMaxLogs=10\n"
        );
    }

    #[test]
    fn no_source_is_twice_in_the_full_mix_whatever_is_chosen() {
        let mut seen = 0;
        for desktop in [false, true] {
            for mic in [None, Some("default".to_string())] {
                for (game, game_audio) in [
                    (true, ""),
                    (true, "front"),
                    (false, "front"),
                    (true, "a:b:c.exe"),
                ] {
                    for discord in [false, true] {
                        for own in [false, true] {
                            let s = ObsSetup {
                                desktop,
                                mic: mic.clone(),
                                game,
                                game_audio: game_audio.into(),
                                discord,
                                own_tracks: own,
                                ..Default::default()
                            };
                            let list = s.audio(Platform::Windows);
                            let what = format!(
                                "{desktop} {mic:?} {game} {game_audio} {discord} {own}: {list:?}"
                            );
                            for src in &list {
                                let in_full = src.mixers & FULL != 0;
                                match src.kind.as_str() {
                                    "desktop" | "mic" => assert!(in_full, "{what}"),
                                    _ => {
                                        assert_eq!(in_full, !desktop, "in the full mix once, through desktop audio or itself: {what}");
                                        assert!(desktop || in_full, "{what}");
                                        let own_track =
                                            if src.kind == "game" { GAME } else { DISCORD };
                                        assert_eq!(
                                            src.mixers & own_track != 0,
                                            own || desktop,
                                            "its own track: {what}"
                                        );
                                    }
                                }
                                if src.kind == "desktop" {
                                    assert_eq!(src.mixers & DESKTOP != 0, own, "{what}");
                                }
                                if src.kind == "mic" {
                                    assert_eq!(src.mixers & MIC != 0, own, "{what}");
                                }
                            }
                            let kinds: Vec<&str> = list.iter().map(|x| x.kind.as_str()).collect();
                            assert_eq!(kinds.contains(&"desktop"), desktop, "{what}");
                            assert_eq!(kinds.contains(&"mic"), mic.is_some(), "{what}");
                            assert_eq!(
                                kinds.contains(&"game"),
                                !game_audio.is_empty() && (game || game_audio != "front"),
                                "{what}"
                            );
                            assert_eq!(kinds.contains(&"discord"), discord, "{what}");
                            assert_eq!(
                                s.tracks(Platform::Windows),
                                list.iter().fold(FULL, |m, x| m | x.mixers),
                                "{what}"
                            );
                            let c = collection(&s, Platform::Windows);
                            let audio_ids = [
                                "wasapi_output_capture",
                                "wasapi_input_capture",
                                "wasapi_process_output_capture",
                            ];
                            let in_obs = c["sources"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter(|x| audio_ids.contains(&x["id"].as_str().unwrap_or("")))
                                .count()
                                + usize::from(
                                    sources_named(&c, "Game")["settings"]["capture_audio"] == true,
                                );
                            assert_eq!(
                                in_obs,
                                list.len(),
                                "only the chosen sources exist in OBS: {what}"
                            );
                            seen += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(seen, 64);
        let s = ObsSetup {
            mic: Some("default".into()),
            discord: true,
            own_tracks: false,
            ..Default::default()
        };
        assert_eq!(
            s.audio_words(Platform::Windows),
            [
                "The full mix, which links play: desktop audio and the microphone",
                "Discord is in the full mix through desktop audio, and on a track of its own"
            ]
        );
        let s = ObsSetup {
            desktop: false,
            mic: Some("default".into()),
            discord: true,
            own_tracks: true,
            ..Default::default()
        };
        assert_eq!(
            s.audio_words(Platform::Windows),
            [
                "The full mix, which links play: the microphone and Discord",
                "The microphone also on a track of its own",
                "Discord also on a track of its own"
            ]
        );
    }

    #[test]
    fn written_settings_skip_the_wizard_the_prompts_and_the_tray() {
        let dir = tempfile::tempdir().unwrap();
        let b = Bundled {
            exe: dir.path().join("bin/64bit/obs64.exe"),
            config_root: dir.path().join("config"),
            platform: Platform::Windows,
            flatpak: false,
        };
        let sentinel = b.obs_dir().join(".sentinel");
        std::fs::create_dir_all(&sentinel).unwrap();
        std::fs::write(sentinel.join("run_1234"), "").unwrap();
        let s = ObsSetup {
            password: "pw".into(),
            ..Default::default()
        };
        b.write(&s, &dir.path().join("clips"), "Alt+F10").unwrap();
        let read = |p: &str| std::fs::read_to_string(b.obs_dir().join(p)).unwrap();
        assert!(
            read("global.ini").contains("LastVersion="),
            "no auto-configuration wizard"
        );
        let user = read("user.ini");
        assert!(
            user.contains("FirstRun=true")
                && user.contains("ConfirmOnExit=false")
                && user.contains("SysTrayEnabled=false")
        );
        let profile = read("basic/profiles/reel/basic.ini");
        assert!(
            profile.contains("RecRB=true")
                && profile.contains("RecRBTime=60")
                && profile.contains("Mode=Advanced")
        );
        let ws: Value =
            serde_json::from_str(&read("plugin_config/obs-websocket/config.json")).unwrap();
        assert_eq!(
            (ws["server_port"].as_u64(), ws["server_password"].as_str()),
            (Some(4466), Some("pw"))
        );
        assert!(
            !sentinel.join("run_1234").exists(),
            "no safe mode question after a crash"
        );
        assert_eq!(b.workdir(), dir.path().join("bin/64bit"));
        assert!(b.args(false).contains(&"--portable".to_string()));
        assert!(
            profile.contains(r#"ReplayBuffer={"ReplayBuffer.Save":[]}"#),
            "the app's own hotkey, none of OBS's"
        );
        let up = ObsSetup {
            elevated: true,
            password: "pw".into(),
            ..Default::default()
        };
        b.write(&up, &dir.path().join("clips"), "Alt+F10").unwrap();
        let user = read("user.ini");
        assert!(
            user.contains("SysTrayEnabled=true") && user.contains("SysTrayWhenStarted=true"),
            "OBS with the rights hides in its own tray"
        );
        assert!(
            read("basic/profiles/reel/basic.ini").contains(r#"ReplayBuffer={"ReplayBuffer.Save":[{"alt":true,"command":false,"control":false,"key":"OBS_KEY_F10","shift":false}]}"#),
            "and takes the Save hotkey itself"
        );
        assert!(b.args(true).contains(&"--minimize-to-tray".to_string()));
        assert_eq!(api_version("32.2.2"), 537001986);
    }

    #[test]
    fn obs_s_encoders_come_from_its_newest_log_that_lists_them() {
        use std::time::{Duration, SystemTime};
        let dir = tempfile::tempdir().unwrap();
        let b = Bundled::linux(dir.path().join("obs"), dir.path());
        assert_eq!(b.encoders_listed(), None, "OBS never started");
        std::fs::create_dir_all(b.logs_dir()).unwrap();
        let older = b.logs_dir().join("2026-11-06 09-00-00.txt");
        std::fs::write(
            &older,
            "09:00:00.100: Available Encoders:\n09:00:00.100:   Video Encoders:\n09:00:00.100: \t- obs_qsv11_v2 (QuickSync H.264)\n09:00:00.100: \t- obs_x264 (x264)\n09:00:00.100:   Audio Encoders:\n",
        )
        .unwrap();
        std::fs::File::options()
            .write(true)
            .open(&older)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(3600))
            .unwrap();
        std::fs::write(
            b.logs_dir().join("2026-11-07 10-00-00.txt"),
            "10:00:00.000: Command Line Arguments: --multi\n",
        )
        .unwrap();
        assert_eq!(
            b.encoders_listed().unwrap(),
            ["obs_qsv11_v2", "obs_x264"],
            "a start held by a dialog lists none: the run before answers"
        );
    }

    #[test]
    fn what_obs_wrote_of_a_run_is_its_newest_logs_end_and_its_crash_reports_head() {
        use std::time::{Duration, SystemTime};
        let dir = tempfile::tempdir().unwrap();
        let b = Bundled::linux(dir.path().join("obs"), dir.path());
        let crashes = b.obs_dir().join("crashes");
        std::fs::create_dir_all(b.logs_dir()).unwrap();
        std::fs::create_dir_all(&crashes).unwrap();
        let start = SystemTime::now() - Duration::from_secs(60);
        let before = b.logs_dir().join("2026-11-06 09-00-00.txt");
        std::fs::write(&before, "an earlier run\n").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&before)
            .unwrap()
            .set_modified(start - Duration::from_secs(3600))
            .unwrap();
        assert_eq!(b.log_tail(start, 30), None, "no log of this run");
        let lines: String = (1..=40).map(|i| format!("line {i}\n")).collect();
        std::fs::write(b.logs_dir().join("2026-11-07 10-00-00.txt"), lines).unwrap();
        let (path, tail) = b.log_tail(start, 30).unwrap();
        assert!(path.ends_with("2026-11-07 10-00-00.txt"));
        assert_eq!((tail.len(), tail[0].as_str()), (30, "line 11"));
        assert_eq!(b.crash_report(start, 12), None);
        std::fs::write(
            crashes.join("Crash 2026-11-07 10-00-05.txt"),
            "Unhandled exception: c0000005\n\nFault address: 7FFB1234 (amfrt64.dll)\n",
        )
        .unwrap();
        let (_, head) = b.crash_report(start, 12).unwrap();
        assert_eq!(
            head,
            [
                "Unhandled exception: c0000005",
                "Fault address: 7FFB1234 (amfrt64.dll)"
            ]
        );
    }
}
