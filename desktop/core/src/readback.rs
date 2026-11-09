//! OBS's settings read back in the setup's terms, and reel's changes merged into OBS's files.
use crate::bundled::{
    app_hotkey, ini_remove, ini_set, volume_mul, Noise, ObsSetup, Platform, Quality, RateMode,
    VideoMode, FULL, TRACKS,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// What reel writes into OBS's files for a setup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Written {
    pub profile: String,
    pub encoder: Option<Value>,
    pub collection: Option<Value>,
}

/// The sources reel writes, by name.
pub const SOURCES: [&str; 6] = [
    "Screen",
    "Game",
    "Desktop audio",
    "Microphone",
    "Game audio",
    "Discord",
];

/// Written whatever OBS has: without them there is no replay buffer, and OBS's own Save
/// hotkey would save a second clip beside the app's.
const ENFORCED: &[(&str, &str)] = &[
    ("AdvOut", "RecRB"),
    ("SimpleOutput", "RecRB"),
    ("AdvOut", "RecType"),
    ("Hotkeys", "ReplayBuffer"),
];

/// The replay buffer's keys in the advanced output mode, by their names in the simple one.
const SIMPLE: &[(&str, &str)] = &[
    ("RecRB", "RecRB"),
    ("RecRBTime", "RecRBTime"),
    ("RecRBSize", "RecRBSize"),
    ("RecRBPrefix", "RecRBPrefix"),
    ("RecFilePath", "FilePath"),
    ("RecFormat2", "RecFormat2"),
];

/// Moves the replay buffer's keys to where OBS's simple output mode reads them.
pub fn in_mode(ini: &str, simple: bool) -> String {
    if !simple {
        return ini.to_string();
    }
    let mut out = String::new();
    for ((section, key), value) in ini_map(ini) {
        let (section, key) = match SIMPLE
            .iter()
            .find(|(a, _)| section == "AdvOut" && *a == key)
        {
            Some((_, s)) => ("SimpleOutput".to_string(), s.to_string()),
            None => (section, key),
        };
        out = ini_set(&out, &section, &key, &value);
    }
    out
}

/// Asked over the WebSocket (GetProfileParameter), as OBS holds them before it saves.
pub const PROFILE_KEYS: &[(&str, &str)] = &[
    ("Output", "Mode"),
    ("AdvOut", "RecRBTime"),
    ("AdvOut", "RecFilePath"),
    ("AdvOut", "RecFormat2"),
    ("AdvOut", "RecEncoder"),
    ("AdvOut", "RecTracks"),
    ("AdvOut", "Track1Name"),
    ("AdvOut", "Track2Name"),
    ("AdvOut", "Track3Name"),
    ("AdvOut", "Track4Name"),
    ("AdvOut", "Track5Name"),
    ("AdvOut", "Track6Name"),
    ("SimpleOutput", "RecRBTime"),
    ("SimpleOutput", "FilePath"),
    ("Video", "BaseCX"),
    ("Video", "BaseCY"),
    ("Video", "OutputCX"),
    ("Video", "OutputCY"),
    ("Video", "FPSType"),
    ("Video", "FPSCommon"),
    ("Video", "FPSInt"),
    ("Video", "FPSNum"),
    ("Video", "FPSDen"),
    ("Hotkeys", "ReplayBuffer"),
];

/// The answers to PROFILE_KEYS as a profile's text, for `observe`.
pub fn profile_text(values: &[(&str, &str, String)]) -> String {
    let mut out = String::new();
    for (section, key, value) in values {
        out = ini_set(&out, section, key, value);
    }
    out
}

fn ini_map(text: &str) -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim_start_matches('\u{feff}').trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
        } else if let Some((k, v)) = line.split_once('=') {
            out.entry((section.clone(), k.trim().to_string()))
                .or_insert_with(|| v.trim().to_string());
        }
    }
    out
}

/// Writes into OBS's file only the keys that differ between `base` and `ours`; without a
/// `base`, OBS's values stand.
pub fn merge_ini3(base: Option<&str>, ours: &str, theirs: &str) -> String {
    let o = ini_map(ours);
    let b = base.map(ini_map).unwrap_or_else(|| o.clone());
    let t = ini_map(theirs);
    let mut out = theirs.to_string();
    for ((s, k), v) in &o {
        let key = (s.clone(), k.clone());
        let enforced = ENFORCED.contains(&(s.as_str(), k.as_str()));
        let reels = enforced || b.get(&key) != Some(v) || !t.contains_key(&key);
        if reels && t.get(&key) != Some(v) {
            out = ini_set(&out, s, k, v);
        }
    }
    for ((s, k), v) in &b {
        let key = (s.clone(), k.clone());
        if !o.contains_key(&key) && t.get(&key) == Some(v) {
            out = ini_remove(&out, s, k);
        }
    }
    out
}

fn named(a: &[Value]) -> bool {
    a.iter()
        .all(|v| v.get("name").and_then(Value::as_str).is_some())
}

fn by_name<'a>(a: Option<&'a Value>, name: &str) -> Option<&'a Value> {
    a?.as_array()?.iter().find(|v| v["name"] == name)
}

/// The same merge over JSON: objects by key, lists of named objects by name.
pub fn merge3(base: Option<&Value>, ours: Option<&Value>, theirs: Option<&Value>) -> Option<Value> {
    match (ours, theirs) {
        (None, None) => None,
        (None, Some(t)) => (base != Some(t)).then(|| t.clone()),
        (Some(o), None) => (base.is_none() || base != Some(o)).then(|| o.clone()),
        (Some(Value::Object(o)), Some(Value::Object(t))) => {
            let b = base.and_then(Value::as_object);
            let mut out = Map::new();
            for k in t.keys().chain(o.keys().filter(|k| !t.contains_key(*k))) {
                if out.contains_key(k) {
                    continue;
                }
                let bk = match b {
                    Some(b) => b.get(k),
                    None => o.get(k),
                };
                if let Some(v) = merge3(bk, o.get(k), t.get(k)) {
                    out.insert(k.clone(), v);
                }
            }
            Some(Value::Object(out))
        }
        (Some(Value::Array(o)), Some(Value::Array(t)))
            if named(o) && named(t) && (!o.is_empty() || !t.is_empty()) =>
        {
            let b = base.filter(|b| b.as_array().is_some_and(|a| named(a)));
            let base_of = |name: &str| match b {
                Some(_) => by_name(b, name),
                None => o.iter().find(|v| v["name"] == name),
            };
            let mut out = Vec::new();
            for tv in t {
                let name = tv["name"].as_str().unwrap_or_default();
                let ov = o.iter().find(|v| v["name"] == name);
                if let Some(v) = merge3(base_of(name), ov, Some(tv)) {
                    out.push(v);
                }
            }
            for ov in o {
                let name = ov["name"].as_str().unwrap_or_default();
                if t.iter().any(|v| v["name"] == name) {
                    continue;
                }
                if let Some(v) = merge3(base_of(name), Some(ov), None) {
                    out.push(v);
                }
            }
            Some(Value::Array(out))
        }
        (Some(o), Some(t)) => Some(if base == Some(o) || base.is_none() {
            t.clone()
        } else {
            o.clone()
        }),
    }
}

/// Renumbers scene items, as one reel adds may share an id with one added in OBS.
pub fn merge_collection(base: Option<&Value>, ours: &Value, theirs: &Value) -> Value {
    let mut c = merge3(base, Some(ours), Some(theirs)).unwrap_or_else(|| ours.clone());
    if let Some(sources) = c["sources"].as_array_mut() {
        for s in sources.iter_mut().filter(|s| s["id"] == "scene") {
            let mut seen = std::collections::BTreeSet::new();
            let mut top = s["settings"]["items"]
                .as_array()
                .map(|a| a.iter().filter_map(|i| i["id"].as_u64()).max().unwrap_or(0))
                .unwrap_or(0);
            if let Some(items) = s["settings"]["items"].as_array_mut() {
                for i in items.iter_mut() {
                    let id = i["id"].as_u64().unwrap_or(0);
                    if id == 0 || !seen.insert(id) {
                        top += 1;
                        i["id"] = json!(top);
                        seen.insert(top);
                    }
                }
            }
            let counter = s["settings"]["id_counter"].as_u64().unwrap_or(0);
            s["settings"]["id_counter"] = json!(counter.max(top));
        }
    }
    c
}

/// A Save hotkey of OBS's the app has no name for, kept in OBS.
const UNNAMED: &str = "unnamed:";

/// A source as reel reads it back.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Seen {
    pub settings: Map<String, Value>,
    /// Tenths of a dB.
    pub volume: i64,
    pub muted: bool,
    pub filters: Vec<Value>,
    pub mixers: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Rate {
    Bitrate(u32),
    Level(u32),
    Other(String),
}

/// What OBS's files say, in the setup's terms.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Observed {
    pub simple: bool,
    pub clip_seconds: Option<u32>,
    pub rec_path: Option<String>,
    pub format: Option<String>,
    pub encoder: Option<String>,
    pub tracks: Option<u32>,
    pub titles: Vec<String>,
    pub video: Option<VideoMode>,
    pub rate: Option<Rate>,
    /// OBS's Save Replay hotkey as the app writes keys; None when it has none.
    pub save_key: Option<String>,
    pub sources: BTreeMap<String, Seen>,
    /// Sources and scenes added in OBS.
    pub added: Vec<String>,
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// OBS's frame rate as a fraction; its common 29.97 is 30000/1001.
fn fps_of(ini: &BTreeMap<(String, String), String>) -> Option<(u32, u32)> {
    let get = |k: &str| ini.get(&("Video".to_string(), k.to_string()));
    let n = |k: &str| get(k).and_then(|v| v.parse::<u32>().ok());
    let (num, den) = match get("FPSType").map(String::as_str).unwrap_or("0") {
        "1" => (n("FPSInt")?, 1),
        "2" => (n("FPSNum")?, n("FPSDen")?),
        _ => match get("FPSCommon").map(String::as_str).unwrap_or("30") {
            "59.94" => (60000, 1001),
            "29.97" => (30000, 1001),
            "23.976" => (24000, 1001),
            c => (c.parse::<f64>().ok()?.round() as u32, 1),
        },
    };
    if num == 0 || den == 0 {
        return None;
    }
    let g = gcd(num, den);
    Some((num / g, den / g))
}

fn path_word(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_string()
}

fn filter_seen(f: &Value) -> Value {
    json!({
        "name": f["name"],
        "kind": f["versioned_id"].as_str().filter(|k| !k.is_empty()).or(f["id"].as_str()),
        "settings": f["settings"].as_object().cloned().unwrap_or_default(),
        "enabled": f["enabled"].as_bool().unwrap_or(true),
    })
}

fn source_seen(s: &Value) -> Seen {
    let mul = s["volume"].as_f64().unwrap_or(1.0);
    let volume = if mul <= 0.0 {
        -1000
    } else {
        (20.0 * mul.log10() * 10.0).round() as i64
    };
    Seen {
        settings: s["settings"].as_object().cloned().unwrap_or_default(),
        volume,
        muted: s["muted"].as_bool().unwrap_or(false),
        filters: s["filters"]
            .as_array()
            .map(|a| a.iter().map(filter_seen).collect())
            .unwrap_or_default(),
        mixers: s["mixers"].as_u64().unwrap_or(0x3f),
    }
}

/// OBS keeps the global audio channels apart from the sources list.
fn collection_sources(c: &Value) -> Vec<&Value> {
    let channels = [
        "DesktopAudioDevice1",
        "DesktopAudioDevice2",
        "AuxAudioDevice1",
        "AuxAudioDevice2",
        "AuxAudioDevice3",
        "AuxAudioDevice4",
    ];
    let mut out: Vec<&Value> = c["sources"]
        .as_array()
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    for ch in channels {
        let s = &c[ch];
        if s["name"].is_string() && !out.iter().any(|o| o["name"] == s["name"]) {
            out.push(s);
        }
    }
    out
}

pub fn observe(profile: &str, encoder: Option<&Value>, collection: Option<&Value>) -> Observed {
    let ini = ini_map(profile);
    let get = |s: &str, k: &str| ini.get(&(s.to_string(), k.to_string())).cloned();
    let simple = get("Output", "Mode").as_deref() == Some("Simple");
    let out = if simple { "SimpleOutput" } else { "AdvOut" };
    let num = |s: &str, k: &str| get(s, k).and_then(|v| v.parse::<u32>().ok());
    let video = match (
        num("Video", "BaseCX"),
        num("Video", "BaseCY"),
        num("Video", "OutputCX"),
        num("Video", "OutputCY"),
        fps_of(&ini),
    ) {
        (Some(bw), Some(bh), Some(ow), Some(oh), Some(fps)) => Some(VideoMode {
            base: (bw, bh),
            output: (ow, oh),
            fps,
        }),
        _ => None,
    };
    let rate = encoder.map(|e| {
        let n = |k: &str, d: u32| e[k].as_u64().map(|v| v as u32).unwrap_or(d);
        match e["rate_control"]
            .as_str()
            .unwrap_or("CBR")
            .to_ascii_uppercase()
            .as_str()
        {
            "CBR" | "VBR" | "ABR" => Rate::Bitrate(n("bitrate", 2500)),
            "CRF" => Rate::Level(n("crf", 23)),
            "CQP" => Rate::Level(n("cqp", 20)),
            "ICQ" => Rate::Level(n("icq_quality", 23)),
            other => Rate::Other(other.to_string()),
        }
    });
    let save_key = get("Hotkeys", "ReplayBuffer")
        .and_then(|v| serde_json::from_str::<Value>(&v).ok())
        .and_then(|v| {
            v["ReplayBuffer.Save"]
                .as_array()
                .and_then(|a| a.first().cloned())
        })
        .map(|k| app_hotkey(&k).unwrap_or_else(|| format!("{UNNAMED}{k}")));
    let mut sources = BTreeMap::new();
    let mut added = Vec::new();
    if let Some(c) = collection {
        for s in collection_sources(c) {
            let name = s["name"].as_str().unwrap_or_default();
            if SOURCES.contains(&name) {
                sources.insert(name.to_string(), source_seen(s));
            } else if s["name"] != c["name"] {
                added.push(name.to_string());
            }
        }
    }
    Observed {
        simple,
        clip_seconds: num(out, "RecRBTime"),
        rec_path: get(out, if simple { "FilePath" } else { "RecFilePath" }).map(|p| path_word(&p)),
        format: get(out, "RecFormat2").filter(|_| !simple),
        encoder: get(out, "RecEncoder").filter(|_| !simple),
        tracks: num(out, "RecTracks").filter(|_| !simple),
        titles: (1..=6)
            .map(|i| get("AdvOut", &format!("Track{i}Name")).unwrap_or_default())
            .collect(),
        video,
        rate,
        save_key,
        sources,
        added,
    }
}

/// What was read from OBS: the setup with it, and the app's own settings it touches.
#[derive(Debug, Clone, PartialEq)]
pub struct Taken {
    pub setup: ObsSetup,
    pub clips_dir: Option<String>,
    pub save_key: Option<String>,
    /// What changed, in words, for the log and the page.
    pub words: Vec<String>,
}

fn fps_word((n, d): (u32, u32)) -> String {
    if d == 1 {
        n.to_string()
    } else {
        let f = n as f64 / d as f64;
        let s = format!("{f:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// Takes OBS's value (`now`) wherever it differs from what reel's setup writes (`ours`).
pub fn take(setup: &ObsSetup, platform: Platform, ours: &Observed, now: &Observed) -> Taken {
    let mut s = setup.clone();
    let mut words = Vec::new();
    let mut clips_dir = None;
    let mut save_key = None;
    macro_rules! took {
        ($f:ident) => {
            now.$f.is_some() && now.$f != ours.$f
        };
    }
    if took!(clip_seconds) {
        if let Some(v) = now.clip_seconds {
            s.clip_seconds = v.clamp(5, 1800);
            words.push(format!("the clip length, {} s", s.clip_seconds));
        }
    }
    if took!(rec_path) {
        if let Some(p) = &now.rec_path {
            let p = if platform == Platform::Windows {
                p.replace('/', "\\")
            } else {
                p.clone()
            };
            words.push(format!("the clips folder, {p}"));
            clips_dir = Some(p);
        }
    }
    // Only an elevated OBS takes the Save key itself, so only then is its hotkey the app's.
    let elevated = setup.elevated && platform == Platform::Windows;
    let named = !now
        .save_key
        .as_deref()
        .is_some_and(|k| k.starts_with(UNNAMED));
    if elevated && now.save_key != ours.save_key && named {
        let k = now.save_key.clone().unwrap_or_default();
        words.push(if k.is_empty() {
            "the Save hotkey, none".into()
        } else {
            format!("the Save hotkey, {k}")
        });
        save_key = Some(k);
    }
    if now.simple {
        return Taken {
            setup: s,
            clips_dir,
            save_key,
            words,
        };
    }
    if took!(format) {
        if let Some(f) = &now.format {
            s.format = f.clone();
            words.push(format!("the format, {f}"));
        }
    }
    if took!(encoder) {
        if let Some(e) = &now.encoder {
            s.encoder = e.clone();
            words.push(format!("the encoder, {e}"));
        }
    }
    if took!(tracks) {
        if let Some(m) = now.tracks {
            let with = |own: bool| {
                let mut x = s.clone();
                x.own_tracks = own;
                x.rec_tracks = 0;
                x.tracks(platform)
            };
            if m == with(true) {
                (s.own_tracks, s.rec_tracks) = (true, 0);
            } else if m == FULL {
                (s.own_tracks, s.rec_tracks) = (false, 0);
            } else {
                s.rec_tracks = m;
            }
            let list: Vec<String> = (0..6)
                .filter(|i| m & (1 << i) != 0)
                .map(|i| (i + 1).to_string())
                .collect();
            words.push(format!("the recorded tracks, {}", list.join(", ")));
        }
    }
    if now.titles != ours.titles && now.titles.iter().any(|t| !t.is_empty()) {
        s.titles = if now.titles.iter().zip(TRACKS).all(|(a, b)| a == b) {
            Vec::new()
        } else {
            now.titles.clone()
        };
        words.push("the tracks' names".into());
    }
    if took!(video) {
        if let Some(v) = now.video {
            if s.rate == RateMode::Bitrate && s.kbit == 0 {
                s.kbit = setup.video().kbit;
            }
            let preset = [
                Quality::Smooth,
                Quality::Light,
                Quality::Sharp,
                Quality::Native,
            ]
            .into_iter()
            .find(|q| {
                let mut x = s.clone();
                x.quality = *q;
                x.video_mode() == v
            });
            match preset {
                Some(q) => (s.quality, s.custom_video) = (q, None),
                None => (s.quality, s.custom_video) = (Quality::Custom, Some(v)),
            }
            words.push(format!(
                "the video, {}x{} at {} fps",
                v.output.0,
                v.output.1,
                fps_word(v.fps)
            ));
        }
    }
    if took!(rate) {
        match &now.rate {
            Some(Rate::Bitrate(b)) => {
                let mut x = s.clone();
                x.kbit = 0;
                s.rate = RateMode::Bitrate;
                s.kbit = if x.video().kbit == *b { 0 } else { *b };
                words.push(format!("the bitrate, {b} kbit/s"));
            }
            Some(Rate::Level(l)) => {
                s.rate = RateMode::Quality;
                s.level = *l;
                words.push(format!("the quality level, {l}"));
            }
            _ => {}
        }
    }
    for name in SOURCES {
        let Some(o) = ours.sources.get(name) else {
            continue;
        };
        let Some(n) = now.sources.get(name) else {
            if name == "Game" && s.game {
                s.game = false;
                words.push("the game capture, removed in OBS".into());
            }
            continue;
        };
        if n.settings != o.settings {
            let device = n.settings.get("device_id").and_then(Value::as_str);
            match (name, device) {
                ("Desktop audio", Some(d)) if d != s.desktop_audio => {
                    s.desktop_audio = d.to_string();
                }
                ("Microphone", Some(d)) if s.mic.is_some() && s.mic.as_deref() != Some(d) => {
                    s.mic = Some(d.to_string());
                }
                _ => {}
            }
            if name == "Screen" {
                if let Some(id) = n.settings.get("monitor_id").and_then(Value::as_str) {
                    s.screen.id = id.to_string();
                }
            }
        }
        if n.volume != o.volume {
            let db = n.volume as f64 / 10.0;
            if n.volume == 0 {
                s.volumes.remove(name);
            } else {
                s.volumes.insert(name.to_string(), db.max(-60.0));
            }
            words.push(format!("the level of {name}, {db:.1} dB"));
        }
        if name == "Microphone" && n.muted != o.muted {
            if n.muted {
                s.mic = None;
                words.push("the microphone, muted in OBS: off".into());
            } else {
                let d = n
                    .settings
                    .get("device_id")
                    .and_then(Value::as_str)
                    .unwrap_or("default");
                s.mic = Some(d.to_string());
                words.push("the microphone, on in OBS".into());
            }
        }
        if name == "Microphone" && n.filters != o.filters && !n.muted {
            let choice = [Noise::Off, Noise::Speex, Noise::Rnnoise, Noise::Nvidia]
                .into_iter()
                .flat_map(|noise| [(noise, false), (noise, true)])
                .find(|(noise, gate)| {
                    let mut x = s.clone();
                    x.noise = *noise;
                    x.gate = *gate;
                    x.mic = Some(x.mic.unwrap_or_else(|| "default".into()));
                    x.mic_filters().iter().map(filter_seen).collect::<Vec<_>>() == n.filters
                });
            match choice {
                Some((noise, gate)) => {
                    s.noise = noise;
                    s.gate = gate;
                }
                None => {
                    s.noise = Noise::Own;
                    s.own_filters = n
                        .filters
                        .iter()
                        .map(|f| {
                            json!({
                                "name": f["name"],
                                "id": f["kind"],
                                "versioned_id": f["kind"],
                                "settings": f["settings"],
                                "enabled": f["enabled"],
                            })
                        })
                        .collect();
                    s.own_filters_from = "the microphone, as set in OBS".into();
                }
            }
            words.push("the microphone's filters".into());
        }
    }
    Taken {
        setup: s,
        clips_dir,
        save_key,
        words,
    }
}

/// A running OBS's sources over its saved collection: OBS saves a volume or mute on exit only.
#[derive(Debug, Clone, Default)]
pub struct Live {
    pub sources: BTreeMap<String, LiveSource>,
}

#[derive(Debug, Clone, Default)]
pub struct LiveSource {
    pub settings: Option<Value>,
    /// As GetSourceFilterList answers.
    pub filters: Option<Vec<Value>>,
    pub volume_db: Option<f64>,
    pub muted: Option<bool>,
}

pub fn overlay(collection: &mut Value, live: &Live) {
    let channels = [
        "DesktopAudioDevice1",
        "DesktopAudioDevice2",
        "AuxAudioDevice1",
        "AuxAudioDevice2",
        "AuxAudioDevice3",
        "AuxAudioDevice4",
    ];
    let apply = |s: &mut Value| {
        let Some(l) = s["name"].as_str().and_then(|n| live.sources.get(n)) else {
            return;
        };
        if let Some(v) = &l.settings {
            s["settings"] = v.clone();
        }
        if let Some(f) = &l.filters {
            s["filters"] = Value::Array(
                f.iter()
                    .map(|f| {
                        json!({
                            "name": f["filterName"],
                            "id": f["filterKind"],
                            "versioned_id": f["filterKind"],
                            "settings": f["filterSettings"],
                            "enabled": f["filterEnabled"],
                        })
                    })
                    .collect(),
            );
        }
        if let Some(db) = l.volume_db {
            s["volume"] = json!(if db <= -100.0 { 0.0 } else { volume_mul(db) });
        }
        if let Some(m) = l.muted {
            s["muted"] = json!(m);
        }
    };
    if let Some(a) = collection["sources"].as_array_mut() {
        a.iter_mut().for_each(apply);
    }
    for ch in channels {
        if collection[ch].is_object() {
            apply(&mut collection[ch]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundled::{collection, Bundled};
    use std::path::Path;

    #[test]
    fn an_ini_merge_keeps_what_obs_changed_and_writes_what_reel_changed() {
        let base =
            "[AdvOut]\nRecRBTime=60\nRecFormat2=mkv\nRecRB=true\n\n[Hotkeys]\nReplayBuffer=A\n";
        let ours = "[AdvOut]\nRecRBTime=60\nRecFormat2=hybrid_mp4\nRecRB=true\n\n[Hotkeys]\nReplayBuffer=A\n";
        let theirs = "\u{feff}[AdvOut]\nRecRBTime=90\nRecFormat2=mkv\nRecRB=false\nRecRBPrefix=Mine\n\n[Hotkeys]\nReplayBuffer=B\nOBSBasic.StartRecording=[]\n";
        let out = ini_map(&merge_ini3(Some(base), ours, theirs));
        let get = |s: &str, k: &str| out.get(&(s.into(), k.into())).cloned();
        assert_eq!(
            get("AdvOut", "RecRBTime").as_deref(),
            Some("90"),
            "OBS's change stands"
        );
        assert_eq!(
            get("AdvOut", "RecFormat2").as_deref(),
            Some("hybrid_mp4"),
            "reel's own change since is newer"
        );
        assert_eq!(
            get("AdvOut", "RecRB").as_deref(),
            Some("true"),
            "no replay buffer, no reel"
        );
        assert_eq!(
            get("Hotkeys", "ReplayBuffer").as_deref(),
            Some("A"),
            "nor two Save hotkeys"
        );
        assert_eq!(get("AdvOut", "RecRBPrefix").as_deref(), Some("Mine"));
        assert_eq!(
            get("Hotkeys", "OBSBasic.StartRecording").as_deref(),
            Some("[]")
        );
        assert!(merge_ini3(Some(base), ours, theirs).starts_with('\u{feff}'));
        let gone = merge_ini3(
            Some("[A]\nx=1\ny=2\n"),
            "[A]\nx=1\n",
            "[A]\nx=1\ny=2\nz=3\n",
        );
        assert!(!gone.contains("y=") && gone.contains("z=3"), "{gone}");
        let changed = merge_ini3(Some("[A]\nx=1\ny=2\n"), "[A]\nx=1\n", "[A]\nx=1\ny=5\n");
        assert!(changed.contains("y=5"), "{changed}");
        let first = merge_ini3(None, "[A]\nx=1\nn=2\n", "[A]\nx=9\n");
        assert!(first.contains("x=9") && first.contains("n=2"), "{first}");
    }

    fn named<'a>(c: &'a Value, name: &str) -> Option<&'a Value> {
        c["sources"].as_array()?.iter().find(|s| s["name"] == name)
    }

    fn items(c: &Value) -> Vec<(String, u64)> {
        named(c, "reel").unwrap()["settings"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| {
                (
                    i["name"].as_str().unwrap().to_string(),
                    i["id"].as_u64().unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn a_collection_merge_keeps_sources_added_in_obs_and_adds_and_removes_reels() {
        let s = ObsSetup {
            mic: Some("default".into()),
            ..ObsSetup::default()
        };
        let base = collection(&s, Platform::Windows);
        let mut theirs = base.clone();
        theirs["sources"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "name": "Webcam", "id": "dshow_input", "settings": { "video_device_id": "cam" } }));
        let scene = theirs["sources"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|x| x["name"] == "reel")
            .unwrap();
        scene["settings"]["items"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "name": "Webcam", "id": 3 }));
        scene["settings"]["id_counter"] = json!(3);
        theirs["sources"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|x| x["name"] == "Screen")
            .unwrap()["settings"]["capture_cursor"] = json!(false);
        let s2 = ObsSetup {
            own_tracks: true,
            discord: true,
            game: false,
            ..s.clone()
        };
        let ours = collection(&s2, Platform::Windows);
        let out = merge_collection(Some(&base), &ours, &theirs);
        assert_eq!(
            named(&out, "Webcam").unwrap()["settings"]["video_device_id"],
            "cam",
            "a source added in OBS stays"
        );
        assert!(
            named(&out, "Discord").is_some(),
            "reel's new source is added"
        );
        assert!(
            named(&out, "Game").is_none(),
            "and the one it no longer has removed"
        );
        assert_eq!(
            named(&out, "Screen").unwrap()["settings"]["capture_cursor"],
            false
        );
        assert_eq!(
            named(&out, "Screen").unwrap()["settings"]["monitor_id"],
            named(&base, "Screen").unwrap()["settings"]["monitor_id"]
        );
        assert_eq!(
            named(&out, "Desktop audio").unwrap()["mixers"],
            json!(FULL | crate::bundled::DESKTOP),
            "reel's change of the tracks is written"
        );
        let its = items(&out);
        let names: Vec<&str> = its.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Screen", "Webcam", "Discord"]);
        let mut ids: Vec<u64> = its.iter().map(|(_, i)| *i).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 3, "each item its own number: {its:?}");
        let counter = named(&out, "reel").unwrap()["settings"]["id_counter"]
            .as_u64()
            .unwrap();
        assert!(counter >= *ids.last().unwrap());
    }

    fn bundled(dir: &Path, platform: Platform) -> Bundled {
        Bundled {
            exe: dir.join("obs64.exe"),
            config_root: dir.join("config"),
            platform,
            flatpak: false,
        }
    }

    fn edit(path: &Path, f: impl FnOnce(String) -> String) {
        let t = std::fs::read_to_string(path).unwrap();
        std::fs::write(path, f(t)).unwrap();
    }

    fn edit_json(path: &Path, f: impl FnOnce(&mut Value)) {
        let mut v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        f(&mut v);
        std::fs::write(path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    }

    fn source_mut<'a>(c: &'a mut Value, name: &str) -> &'a mut Value {
        c["sources"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|s| s["name"] == name)
            .unwrap()
    }

    #[test]
    fn thirty_frames_set_in_obs_read_back_as_light() {
        let dir = tempfile::tempdir().unwrap();
        let b = bundled(dir.path(), Platform::Windows);
        let clips = dir.path().join("clips");
        let s = ObsSetup {
            screen: crate::bundled::Screen {
                id: "monitor-1".into(),
                width: 2560,
                height: 1440,
                hz: 144,
                ..Default::default()
            },
            ..ObsSetup::default()
        };
        b.write(&s, &clips, "Alt+F10").unwrap();
        let profile = dir
            .path()
            .join("config/obs-studio/basic/profiles/reel/basic.ini");
        edit(&profile, |t| ini_set(&t, "Video", "FPSInt", "30"));
        let a = b.read(&s, &clips, "Alt+F10", None, None).unwrap();
        assert_eq!(
            (a.setup.quality, a.setup.custom_video),
            (Quality::Light, None)
        );
        assert!(
            a.words
                .iter()
                .any(|w| w == "the video, 1920x1080 at 30 fps"),
            "{:?}",
            a.words
        );
    }

    #[test]
    fn what_obs_has_shows_in_the_setup_and_a_start_writes_over_none_of_it() {
        let dir = tempfile::tempdir().unwrap();
        let b = bundled(dir.path(), Platform::Windows);
        let clips = dir.path().join("clips");
        let s = ObsSetup {
            password: "pw".into(),
            mic: Some("mic-1".into()),
            screen: crate::bundled::Screen {
                id: "monitor-1".into(),
                width: 2560,
                height: 1440,
                hz: 144,
                ..Default::default()
            },
            ..ObsSetup::default()
        };
        b.write(&s, &clips, "Alt+F10").unwrap();
        let obs = dir.path().join("config/obs-studio");
        let profile = obs.join("basic/profiles/reel/basic.ini");
        let encoder = obs.join("basic/profiles/reel/recordEncoder.json");
        let scenes = obs.join("basic/scenes/reel.json");
        edit(&profile, |t| {
            let t = ini_set(&t, "AdvOut", "RecRBTime", "90");
            let t = ini_set(&t, "Video", "FPSType", "0");
            ini_set(&t, "Video", "FPSCommon", "29.97")
        });
        edit_json(&encoder, |e| {
            e["rate_control"] = json!("VBR");
            e["bitrate"] = json!(9000);
        });
        let speex = ObsSetup {
            noise: Noise::Speex,
            ..s.clone()
        }
        .mic_filters();
        edit_json(&scenes, |c| {
            source_mut(c, "Microphone")["filters"] = json!(speex);
            source_mut(c, "Desktop audio")["volume"] = json!(0.501187205314636_f64);
            source_mut(c, "Screen")["settings"]["capture_cursor"] = json!(false);
            c["sources"]
                .as_array_mut()
                .unwrap()
                .push(json!({ "name": "Webcam", "id": "dshow_input" }));
        });
        let a = b.read(&s, &clips, "Alt+F10", None, None).unwrap();
        let t = &a.setup;
        assert_eq!(t.clip_seconds, 90);
        assert_eq!(t.quality, Quality::Custom);
        assert_eq!(
            t.custom_video,
            Some(VideoMode {
                base: (2560, 1440),
                output: (1920, 1080),
                fps: (30000, 1001)
            })
        );
        assert_eq!((t.rate, t.kbit), (RateMode::Bitrate, 9000));
        assert_eq!(
            (t.noise, t.gate),
            (Noise::Speex, false),
            "a choice of reel's, found"
        );
        assert_eq!(t.volumes.get("Desktop audio"), Some(&-6.0));
        assert_eq!(t.screen.id, "monitor-1");
        assert_eq!(t.mic.as_deref(), Some("mic-1"));
        assert!(
            a.words.iter().any(|w| w == "the clip length, 90 s")
                && a.words
                    .iter()
                    .any(|w| w == "the video, 1920x1080 at 29.97 fps"),
            "{:?}",
            a.words
        );
        assert_eq!(a.clips_dir, None);
        b.ensure(&a.setup, &clips, "Alt+F10").unwrap();
        let read = |p: &Path| std::fs::read_to_string(p).unwrap();
        let json = |p: &Path| serde_json::from_str::<Value>(&read(p)).unwrap();
        let seen = observe(&read(&profile), Some(&json(&encoder)), Some(&json(&scenes)));
        assert_eq!(seen.clip_seconds, Some(90));
        assert_eq!(seen.video.unwrap().fps, (30000, 1001));
        assert_eq!(seen.rate, Some(Rate::Bitrate(9000)));
        assert_eq!(seen.sources["Screen"].settings["capture_cursor"], false);
        assert_eq!(seen.sources["Desktop audio"].volume, -60);
        assert_eq!(seen.sources["Microphone"].filters.len(), 1);
        assert_eq!(seen.added, vec!["Webcam".to_string()]);
        let again = b.read(&a.setup, &clips, "Alt+F10", None, None).unwrap();
        assert!(again.words.is_empty(), "{:?}", again.words);
        assert_eq!(again.setup, a.setup);
        let later = ObsSetup {
            clip_seconds: 120,
            ..a.setup.clone()
        };
        b.put(&b.change(
            &b,
            (&a.setup, &clips, "Alt+F10"),
            (&later, &clips, "Alt+F10"),
        ))
        .unwrap();
        let p = read(&profile);
        assert!(
            p.contains("RecRBTime=120") && p.contains("FPSCommon=29.97"),
            "{p}"
        );
        assert_eq!(
            json(&encoder)["rate_control"],
            "VBR",
            "OBS's own key stands"
        );
        assert_eq!(
            observe(&p, None, Some(&json(&scenes))).added,
            vec!["Webcam".to_string()]
        );
    }

    #[test]
    fn the_rest_of_what_obs_can_change_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let b = bundled(dir.path(), Platform::Windows);
        let clips = dir.path().join("clips");
        let s = ObsSetup {
            mic: Some("mic-1".into()),
            ..ObsSetup::default()
        };
        b.write(&s, &clips, "Alt+F10").unwrap();
        let obs = dir.path().join("config/obs-studio");
        let profile = obs.join("basic/profiles/reel/basic.ini");
        let scenes = obs.join("basic/scenes/reel.json");
        edit(&profile, |t| {
            let t = ini_set(&t, "AdvOut", "RecEncoder", "obs_nvenc_hevc_tex");
            let t = ini_set(&t, "AdvOut", "RecFormat2", "mkv");
            let t = ini_set(&t, "AdvOut", "RecTracks", "5");
            let t = ini_set(&t, "AdvOut", "RecFilePath", "D:/Clips/");
            let t = ini_set(&t, "AdvOut", "Track3Name", "Voice");
            ini_set(
                &t,
                "Hotkeys",
                "ReplayBuffer",
                r#"{"ReplayBuffer.Save":[{"key":"OBS_KEY_F9","control":true}]}"#,
            )
        });
        edit_json(&scenes, |c| {
            source_mut(c, "Microphone")["muted"] = json!(true);
            let a = c["sources"].as_array_mut().unwrap();
            a.retain(|s| s["name"] != "Game");
        });
        let a = b.read(&s, &clips, "Alt+F10", None, None).unwrap();
        let t = &a.setup;
        assert_eq!(t.encoder, "obs_nvenc_hevc_tex");
        assert_eq!(t.format, "mkv");
        assert_eq!(t.rec_tracks, 5, "tracks 1 and 3, no choice of reel's");
        assert_eq!(t.track_names(Platform::Windows), ["Full mix", "Voice"]);
        assert_eq!(a.clips_dir.as_deref(), Some("D:\\Clips"));
        assert_eq!(
            a.save_key, None,
            "OBS's own Save key is OBS's: the app's hotkey stays the app's"
        );
        assert_eq!(t.mic, None, "muted in OBS: off");
        assert!(!t.game, "the game capture removed in OBS");
        let up = ObsSetup {
            elevated: true,
            ..s.clone()
        };
        let a = b.read(&up, &clips, "Alt+F10", None, None).unwrap();
        assert_eq!(a.save_key.as_deref(), Some("Ctrl+F9"));
    }

    #[test]
    fn the_running_obs_counts_before_obs_saves_it() {
        let dir = tempfile::tempdir().unwrap();
        let b = bundled(dir.path(), Platform::Linux);
        let clips = dir.path().join("clips");
        let s = ObsSetup {
            mic: Some("default".into()),
            ..ObsSetup::default()
        };
        b.write(&s, &clips, "Alt+F10").unwrap();
        let mut live = Live::default();
        live.sources.insert(
            "Microphone".into(),
            LiveSource {
                volume_db: Some(-3.0),
                filters: Some(vec![json!({ "filterName": "Noise suppression", "filterKind": "noise_suppress_filter_v2", "filterSettings": { "method": "speex", "suppress_level": -30 }, "filterEnabled": true })]),
                ..Default::default()
            },
        );
        let profile = profile_text(&[("AdvOut", "RecRBTime", "45".to_string())]);
        let a = b
            .read(&s, &clips, "Alt+F10", Some(&profile), Some(&live))
            .unwrap();
        assert_eq!(a.setup.volumes.get("Microphone"), Some(&-3.0));
        assert_eq!(a.setup.noise, Noise::Speex);
        assert_eq!(
            a.setup.clip_seconds, 45,
            "the profile as the running OBS answers it"
        );
        assert_eq!(
            a.setup.quality, s.quality,
            "a key it did not answer is no change"
        );
        let none = b
            .read(&a.setup, &clips, "Alt+F10", Some(&profile), Some(&live))
            .unwrap();
        assert!(none.words.is_empty(), "taken once: {:?}", none.words);
    }

    #[test]
    fn in_the_simple_output_mode_a_change_of_the_clip_length_still_reaches_obs() {
        let dir = tempfile::tempdir().unwrap();
        let b = bundled(dir.path(), Platform::Windows);
        let clips = dir.path().join("clips");
        let s = ObsSetup::default();
        b.write(&s, &clips, "Alt+F10").unwrap();
        let profile = dir
            .path()
            .join("config/obs-studio/basic/profiles/reel/basic.ini");
        edit(&profile, |t| {
            let t = ini_set(&t, "Output", "Mode", "Simple");
            let t = ini_set(&t, "SimpleOutput", "RecQuality", "Small");
            ini_set(&t, "SimpleOutput", "RecRBTime", "30")
        });
        let a = b.read(&s, &clips, "Alt+F10", None, None).unwrap();
        assert_eq!(a.setup.clip_seconds, 30, "read where that mode keeps it");
        let longer = ObsSetup {
            clip_seconds: 45,
            ..a.setup.clone()
        };
        b.put(&b.change(
            &b,
            (&a.setup, &clips, "Alt+F10"),
            (&longer, &clips, "Alt+F10"),
        ))
        .unwrap();
        let p = std::fs::read_to_string(&profile).unwrap();
        let m = ini_map(&p);
        let get = |s: &str, k: &str| m.get(&(s.to_string(), k.to_string())).cloned();
        assert_eq!(
            get("SimpleOutput", "RecRBTime").as_deref(),
            Some("45"),
            "{p}"
        );
        assert_eq!(get("SimpleOutput", "RecRB").as_deref(), Some("true"));
        assert_eq!(get("SimpleOutput", "RecQuality").as_deref(), Some("Small"));
        assert_eq!(get("Output", "Mode").as_deref(), Some("Simple"));
    }
}
