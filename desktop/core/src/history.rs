//! This PC's settings history: each change of a setting a page of the app shows, kept beside the
//! settings in `settings-history.jsonl`, one change a line, for Advanced's Settings history in the
//! app's window.
use crate::programs::Entry;
use crate::settings::Settings;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The changes kept, the number the server's table shows.
pub const KEEP: usize = 500;
/// The file is trimmed back to `KEEP` once it holds this many.
pub const TRIM_AT: usize = 600;

/// Settings whose values never go into the file nor to a page: a change says "changed" alone.
pub const SECRETS: &[&str] = &["obs.password", "obs.setup.password"];

/// Where a change came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A page of the app's Settings.
    #[default]
    Settings,
    /// The setup, or Run the setup again.
    Setup,
    /// Revert in Settings history.
    Revert,
    /// The app changed it itself.
    Reel,
    /// A start of the app with a flag that sets it.
    CommandLine,
}

impl Source {
    pub fn words(self) -> &'static str {
        match self {
            Source::Settings => "Settings",
            Source::Setup => "the setup",
            Source::Revert => "Revert",
            Source::Reel => "reel",
            Source::CommandLine => "the command line",
        }
    }

    /// What a page may say it saves from; the others are the app's own.
    pub fn of_page(word: Option<&str>) -> Source {
        match word {
            Some("setup") => Source::Setup,
            _ => Source::Settings,
        }
    }
}

/// One change as the file keeps it. A secret's values are never written.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub id: String,
    pub at: String,
    pub key: String,
    #[serde(default)]
    pub old: Value,
    #[serde(default)]
    pub new: Value,
    #[serde(default)]
    pub from: Source,
    /// A program's name when it changed, for its rows under Games and apps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
}

/// A change before it is kept: what changed, from what to what.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub key: String,
    pub old: Value,
    pub new: Value,
    pub from: Source,
    pub program: Option<String>,
}

/// How a row's values read in the table, as the row shows them.
#[derive(Debug, Clone, Copy)]
pub enum Words {
    OnOff,
    Choice(&'static [(&'static str, &'static str)]),
    Encoder,
    /// A number and its unit; the words for 0 where 0 means something else.
    Unit(&'static str, Option<&'static str>),
    /// 0 to 1, as the page's 0 to 100.
    Volume,
    Hotkey,
    Text,
    /// A WAV file; none plays the bundled one.
    Sound,
    List,
    Mic,
    GameAudio,
    Screen,
    LibraryDir,
    Secret,
}

/// Whether a row's Revert puts its old value back, or where it is changed instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Back {
    Yes,
    No(&'static str),
}

/// A setting a page of the app shows: its key (the row's `data-key`), its name as its row says
/// it, and, for a row without a key, the id of the row or the setup's label it is held to.
#[derive(Debug, Clone, Copy)]
pub struct Row {
    pub key: &'static str,
    pub name: &'static str,
    pub words: Words,
    pub back: Back,
    pub row: Option<&'static str>,
}

const fn row(key: &'static str, name: &'static str, words: Words) -> Row {
    Row {
        key,
        name,
        words,
        back: Back::Yes,
        row: None,
    }
}

const fn fixed(key: &'static str, name: &'static str, words: Words, at: &'static str) -> Row {
    Row {
        key,
        name,
        words,
        back: Back::No(at),
        row: None,
    }
}

const fn held(mut r: Row, at: &'static str) -> Row {
    r.row = Some(at);
    r
}

const ON: Words = Words::OnOff;
const SECS: Words = Words::Unit("s", None);

const HOTKEY: Words = Words::Hotkey;

/// Every setting the app's pages show, the setup's too. A key not here is the app's own
/// bookkeeping and is never recorded.
pub const ROWS: &[Row] = &[
    row(
        "obs.replay_buffer",
        "The replay buffer",
        Words::Choice(&[("always", "Always"), ("games", "While a game runs")]),
    ),
    row("obs.setup.clip_seconds", "Clip length", SECS),
    row(
        "obs.setup.quality",
        "Quality",
        Words::Choice(&[
            ("light", "Light"),
            ("smooth", "Smooth"),
            ("sharp", "Sharp"),
            ("native", "Native"),
            ("custom", "as set in OBS"),
        ]),
    ),
    held(
        row("obs.setup.mic", "Microphone", Words::Mic),
        "mic-app-row",
    ),
    row(
        "upload.mode",
        "Upload",
        Words::Choice(&[
            ("auto", "every clip"),
            ("hotkey", "the last, on its key"),
            ("manual", "by hand"),
        ]),
    ),
    row("startup.autostart", "Start with Windows", ON),
    row(
        "log_level",
        "Log level",
        Words::Choice(&[
            ("error", "errors only"),
            ("warn", "warnings"),
            ("info", "info"),
            ("debug", "debug, everything"),
        ]),
    ),
    fixed(
        "mode",
        "Mode",
        Words::Choice(&[("connected", "connected"), ("standalone", "standalone")]),
        "Connection",
    ),
    fixed("server_url", "Server URL", Words::Text, "Connection"),
    row("device_name", "Device name", Words::Text),
    held(
        fixed(
            "standalone.library_dir",
            "Library folder",
            Words::LibraryDir,
            "Library folder",
        ),
        "library-row",
    ),
    row(
        "standalone.cap_gb",
        "Library cap",
        Words::Unit("GB", Some("no cap")),
    ),
    row(
        "standalone.cap_keeps_unsent",
        "The cap spares unsent clips",
        ON,
    ),
    row(
        "standalone.hwaccel",
        "GPU encoder",
        Words::Choice(&[
            ("auto", "automatic"),
            ("nvenc", "NVIDIA NVENC"),
            ("amf", "AMD AMF"),
            ("qsv", "Intel Quick Sync"),
            ("vaapi", "VAAPI"),
            ("none", "none, the CPU"),
        ]),
    ),
    row("standalone.copies_on_hardware", "Copies on the GPU", ON),
    row("obs.start_replay", "Start it in OBS", ON),
    row(
        "upload.keep_last",
        "Keep the last",
        Words::Unit("s", Some("the whole clip")),
    ),
    row("hotkeys.save", "Save a clip", HOTKEY),
    row("hotkeys.save_share", "Save and share", HOTKEY),
    row("hotkeys.share_last", "Share the last clip", HOTKEY),
    row("hotkeys.upload_last", "Upload the last clip", HOTKEY),
    row("hotkeys.record", "Record a session", HOTKEY),
    row("sounds.captured.enabled", "Clip captured", ON),
    row(
        "sounds.captured.volume",
        "Clip captured, Volume",
        Words::Volume,
    ),
    row("sounds.captured.file", "Clip captured, Sound", Words::Sound),
    row("sounds.no_clip.enabled", "No clip", ON),
    row("sounds.no_clip.volume", "No clip, Volume", Words::Volume),
    row("sounds.no_clip.file", "No clip, Sound", Words::Sound),
    row("sounds.uploaded.enabled", "Clip uploaded", ON),
    row(
        "sounds.uploaded.volume",
        "Clip uploaded, Volume",
        Words::Volume,
    ),
    row("sounds.uploaded.file", "Clip uploaded, Sound", Words::Sound),
    row("sounds.link_copied.enabled", "Link copied", ON),
    row(
        "sounds.link_copied.volume",
        "Link copied, Volume",
        Words::Volume,
    ),
    row(
        "sounds.link_copied.file",
        "Link copied, Sound",
        Words::Sound,
    ),
    row("clips_dir", "Clips folder", Words::Text),
    row("cache.gb", "Keep up to", Words::Unit("GB", None)),
    row("cache.days", "Keep for", Words::Unit("days", None)),
    row(
        "session.record_by_itself",
        "Every game records by itself",
        ON,
    ),
    row(
        "session.stop_with_game",
        "Stop the session recording when its game closes",
        ON,
    ),
    row(
        "session.remind_minutes",
        "Remind every",
        Words::Unit("min", Some("never")),
    ),
    row("sounds.session.enabled", "Reminders, Play", ON),
    row("sounds.session.volume", "Reminders, Volume", Words::Volume),
    row("sounds.session.file", "Reminders, Sound", Words::Sound),
    fixed("obs.setup.encoder", "Encoder", Words::Encoder, "Capture"),
    row(
        "obs.setup.format",
        "Format",
        Words::Choice(&[("hybrid_mp4", "hybrid MP4"), ("mkv", "MKV")]),
    ),
    row("obs.setup.game", "The game in front", ON),
    row(
        "obs.setup.elevated",
        "Capture games that run as administrator",
        ON,
    ),
    row("obs.setup.desktop", "Desktop audio", ON),
    held(
        row("obs.setup.game_audio", "The game alone", Words::GameAudio),
        "game-app-row",
    ),
    row("obs.setup.discord", "Discord alone", ON),
    row(
        "obs.setup.noise",
        "Noise suppression",
        Words::Choice(&[
            ("off", "Off"),
            ("speex", "Light (Speex)"),
            ("rnnoise", "Strong (RNNoise)"),
            ("nvidia", "NVIDIA noise removal"),
            ("own", "My own OBS's filters"),
        ]),
    ),
    row("obs.setup.gate", "Noise gate", ON),
    row(
        "obs.setup.own_tracks",
        "Each source on a track of its own",
        ON,
    ),
    held(
        fixed(
            "obs.setup.screen",
            "Screen",
            Words::Screen,
            "Capture, Run the setup again",
        ),
        "Screen",
    ),
    held(
        row(
            "obs.setup.rate",
            "Rate control",
            Words::Choice(&[("bitrate", "a bitrate"), ("quality", "a quality level")]),
        ),
        "Rate control",
    ),
    held(
        row("obs.setup.kbit", "Bitrate", Words::Unit("kbit/s", None)),
        "Bitrate",
    ),
    held(
        row("obs.setup.level", "Quality level", Words::Unit("", None)),
        "Quality level",
    ),
    row("obs.bundled", "Use the OBS that came with reel", ON),
    row("obs.enabled", "Connect to OBS", ON),
    row("obs.host", "obs-websocket, Host", Words::Text),
    row("obs.port", "obs-websocket, Port", Words::Unit("", None)),
    fixed(
        "obs.password",
        "obs-websocket, Password",
        Words::Secret,
        "Capture",
    ),
    row("clips_follow_obs", "Follow OBS", ON),
    row("extensions", "Extensions", Words::List),
    row("stable_seconds", "Still for", SECS),
    row("games.steam", "Add my Steam games to the list", ON),
    row("discord.show", "Show on Discord", ON),
    row("discord.name_game", "Name the game", ON),
    row("discord.button", "Button to reel", ON),
    row("session.grace_seconds", "Grace period", SECS),
    row(
        "upload.wait_for_games",
        "Uploads wait while a game has the screen",
        ON,
    ),
    row(
        "upload.pause_only_on_wan",
        "Only on the internet, not on the LAN",
        ON,
    ),
    row(
        "upload.bandwidth_kbit",
        "Bandwidth limit",
        Words::Unit("kbit/s", Some("no limit")),
    ),
    row(
        "upload.chunk_mb",
        "Chunk size",
        Words::Unit("MB", Some("the server's")),
    ),
    row(
        "tray_icon",
        "Tray icon",
        Words::Choice(&[
            ("colour", "in colour"),
            ("white", "white"),
            ("black", "black"),
        ]),
    ),
    row("startup.elevated", "Start elevated at logon", ON),
    row(
        "updates",
        "New versions",
        Words::Choice(&[
            ("ask", "ask, then install"),
            ("on_quit", "install when the app quits"),
            ("off", "off"),
        ]),
    ),
];

/// Each audio source's level under Capture, by the source's name.
pub const VOLUMES: &str = "obs.setup.volumes.";

/// A program's own settings under Games and apps: `games/<program>.<field>`.
pub const PROGRAM: &str = "games/";

/// A program's rows as its page names them, and the words of their values.
pub const PROGRAM_ROWS: &[(&str, &str)] = &[
    ("name", "Name"),
    ("naming", "Clips are named"),
    ("class", "Class"),
    ("buffer", "Replay buffer"),
    ("record", "Record by itself"),
    ("stop", "Stop when it closes"),
    ("wait", "Uploads wait"),
];

const CLASS_WORDS: &[(&str, &str)] = &[
    ("game", "Game"),
    ("not_a_game", "Not a game"),
    ("not_sorted", "Not sorted yet"),
    ("ignored", "Ignored"),
];
const OWN_WORDS: &[(&str, &str)] = &[("as_class", "As its class"), ("on", "On"), ("off", "Off")];

pub fn find(key: &str) -> Option<&'static Row> {
    ROWS.iter().find(|r| r.key == key)
}

pub fn secret(key: &str) -> bool {
    SECRETS.contains(&key)
}

fn at<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(v, |v, part| v.get(part))
}

/// A setting's value by its key; nothing there reads as null.
pub fn value_of(settings: &Settings, key: &str) -> Value {
    let v = serde_json::to_value(settings).unwrap_or(Value::Null);
    let found = match key.strip_prefix(VOLUMES) {
        Some(source) => at(&v, VOLUMES.trim_end_matches('.')).and_then(|m| m.get(source)),
        None => at(&v, key),
    };
    found.cloned().unwrap_or(Value::Null)
}

/// What changed between two settings, a row at a time: only the settings a page shows, a
/// secret without its values. `from` says where each row's change was asked.
pub fn changes(old: &Settings, new: &Settings, from: &dyn Fn(&str) -> Source) -> Vec<Change> {
    let (a, b) = (
        serde_json::to_value(old).unwrap_or(Value::Null),
        serde_json::to_value(new).unwrap_or(Value::Null),
    );
    let mut out = Vec::new();
    let mut push = |key: String, was: Value, now: Value| {
        if was == now {
            return;
        }
        let (was, now) = if secret(&key) {
            (Value::Null, Value::Null)
        } else {
            (scrub(&key, was), scrub(&key, now))
        };
        out.push(Change {
            from: from(&key),
            key,
            old: was,
            new: now,
            program: None,
        });
    };
    for r in ROWS {
        let was = at(&a, r.key).cloned().unwrap_or(Value::Null);
        let now = at(&b, r.key).cloned().unwrap_or(Value::Null);
        push(r.key.to_string(), was, now);
    }
    let levels = |v: &Value| {
        at(v, VOLUMES.trim_end_matches('.'))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };
    let (la, lb) = (levels(&a), levels(&b));
    let mut names: Vec<&String> = la.keys().chain(lb.keys()).collect();
    names.sort();
    names.dedup();
    for n in names {
        let was = la.get(n).cloned().unwrap_or(Value::Null);
        let now = lb.get(n).cloned().unwrap_or(Value::Null);
        push(format!("{VOLUMES}{n}"), was, now);
    }
    out
}

/// A server URL without a name and password in it: those are never kept.
fn scrub(key: &str, v: Value) -> Value {
    let Some(raw) = v.as_str().filter(|_| key == "server_url") else {
        return v;
    };
    match url::Url::parse(raw) {
        Ok(mut u) if !u.username().is_empty() || u.password().is_some() => {
            let _ = u.set_username("");
            let _ = u.set_password(None);
            json!(u.as_str().trim_end_matches('/'))
        }
        Ok(_) => v,
        Err(_) if raw.contains('@') => json!("changed"),
        Err(_) => v,
    }
}

fn program_value(e: &Entry, field: &str) -> Value {
    match field {
        "name" => json!(e.name),
        "naming" => serde_json::to_value(e.naming).unwrap_or(Value::Null),
        "class" => serde_json::to_value(e.class).unwrap_or(Value::Null),
        "buffer" => serde_json::to_value(e.own.buffer).unwrap_or(Value::Null),
        "record" => serde_json::to_value(e.own.record).unwrap_or(Value::Null),
        "stop" => serde_json::to_value(e.own.stop).unwrap_or(Value::Null),
        "wait" => serde_json::to_value(e.own.wait).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// What changed of one program's own settings.
pub fn program_changes(old: &Entry, new: &Entry, from: Source) -> Vec<Change> {
    PROGRAM_ROWS
        .iter()
        .filter_map(|(field, _)| {
            let (was, now) = (program_value(old, field), program_value(new, field));
            (was != now).then(|| Change {
                key: format!("{PROGRAM}{}.{field}", new.id),
                old: was,
                new: now,
                from,
                program: Some(old.name.clone()),
            })
        })
        .collect()
}

/// A program's key: its id and the field.
pub fn program_key(key: &str) -> Option<(&str, &str)> {
    let rest = key.strip_prefix(PROGRAM)?;
    let (id, field) = rest.rsplit_once('.')?;
    PROGRAM_ROWS
        .iter()
        .any(|(f, _)| *f == field)
        .then_some((id, field))
}

/// What a page needs to name rows as the app's pages do now.
#[derive(Debug, Clone, Default)]
pub struct Ctx {
    pub standalone: bool,
    pub paired: bool,
    pub windows: bool,
    /// The programs under Games and apps now, by id: their names.
    pub programs: std::collections::HashMap<String, String>,
}

fn choice(list: &[(&str, &str)], v: &Value) -> String {
    let s = v.as_str().unwrap_or_default();
    list.iter()
        .find(|(k, _)| *k == s)
        .map(|(_, w)| w.to_string())
        .unwrap_or_else(|| raw(v))
}

fn raw(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "none".into(),
        other => other.to_string(),
    }
}

fn number(v: &Value) -> String {
    match v.as_f64() {
        Some(n) if n.fract() == 0.0 => format!("{}", n as i64),
        Some(n) => format!("{n}"),
        None => raw(v),
    }
}

/// A row's value in the words its row shows it.
pub fn words(w: Words, v: &Value) -> String {
    match w {
        Words::OnOff => if v.as_bool() == Some(true) {
            "on"
        } else {
            "off"
        }
        .into(),
        Words::Choice(list) => choice(list, v),
        Words::Encoder => match v.as_str() {
            Some("") | None => "x264 on the CPU".into(),
            Some(id) => crate::obs::encoder_word(id),
        },
        Words::Unit(unit, zero) => match zero {
            Some(z) if v.as_f64() == Some(0.0) => z.into(),
            _ if unit.is_empty() => number(v),
            _ => format!("{} {unit}", number(v)),
        },
        Words::Volume => format!(
            "{}",
            (v.as_f64().unwrap_or(1.0) * 100.0)
                .round()
                .clamp(0.0, 100.0) as i64
        ),
        Words::Hotkey => match v.as_str() {
            Some("") | None => "none".into(),
            Some(k) => k.into(),
        },
        Words::Text => match v.as_str() {
            Some("") => "empty".into(),
            _ => raw(v),
        },
        Words::Sound => match v.as_str() {
            Some(f) if !f.is_empty() => f.into(),
            _ => "the bundled one".into(),
        },
        Words::List => match v.as_array() {
            Some(a) if !a.is_empty() => a.iter().map(raw).collect::<Vec<_>>().join(", "),
            _ => "none".into(),
        },
        Words::Mic => match v.as_str() {
            None => "off".into(),
            Some("default") | Some("") => "on, the default device".into(),
            Some(_) => "on, a device of its own".into(),
        },
        Words::GameAudio => match v.as_str() {
            Some("") | None => "off".into(),
            Some("front") => "the game in front".into(),
            Some(_) => "a program's window".into(),
        },
        Words::Screen => {
            let name = v.get("name").and_then(Value::as_str).unwrap_or_default();
            let (w, h) = (
                v.get("width").and_then(Value::as_u64).unwrap_or(0),
                v.get("height").and_then(Value::as_u64).unwrap_or(0),
            );
            match (name.is_empty(), w > 0 && h > 0) {
                (true, _) => "the main screen".into(),
                (false, true) => format!("{name}, {w}x{h}"),
                (false, false) => name.into(),
            }
        }
        Words::LibraryDir => match v.as_str() {
            Some("") | None => "in the app's data folder".into(),
            Some(p) => p.into(),
        },
        Words::Secret => "changed".into(),
    }
}

fn level_words(v: &Value) -> String {
    format!("{} dB", if v.is_null() { "0".into() } else { number(v) })
}

/// A row's name as the app's pages say it now.
pub fn name(r: &Record, ctx: &Ctx) -> String {
    if let Some((id, field)) = program_key(&r.key) {
        let program = ctx
            .programs
            .get(id)
            .cloned()
            .or_else(|| r.program.clone())
            .unwrap_or_else(|| "A program".into());
        let row = match field {
            "wait" if ctx.standalone && ctx.paired => "Uploads and encoding wait",
            "wait" if ctx.standalone => "Encoding waits",
            f => PROGRAM_ROWS
                .iter()
                .find(|(k, _)| *k == f)
                .map(|(_, n)| *n)
                .unwrap_or(f),
        };
        return format!("{program}, {row}");
    }
    if let Some(source) = r.key.strip_prefix(VOLUMES) {
        return format!("{source} level");
    }
    match r.key.as_str() {
        "startup.autostart" if !ctx.windows => "Start at logon".into(),
        "sounds.uploaded.enabled" if ctx.standalone => "Clip in the library".into(),
        "sounds.uploaded.volume" if ctx.standalone => "Clip in the library, Volume".into(),
        "sounds.uploaded.file" if ctx.standalone => "Clip in the library, Sound".into(),
        "upload.wait_for_games" if ctx.standalone && ctx.paired => {
            "Uploads and encoding wait while a game has the screen".into()
        }
        "upload.wait_for_games" if ctx.standalone => {
            "Encoding waits while a game has the screen".into()
        }
        k => find(k)
            .map(|r| r.name.to_string())
            .unwrap_or_else(|| k.into()),
    }
}

/// A value of a row in its row's words; a secret's never.
pub fn value_words(key: &str, v: &Value) -> String {
    if secret(key) {
        return "changed".into();
    }
    if let Some((_, field)) = program_key(key) {
        return match field {
            "class" => choice(CLASS_WORDS, v),
            "naming" if v.as_str() == Some("window") => "after its window's title".into(),
            "naming" => "after its name".into(),
            "name" => raw(v),
            _ => choice(OWN_WORDS, v),
        };
    }
    if key.starts_with(VOLUMES) {
        return level_words(v);
    }
    match find(key) {
        Some(r) => words(r.words, v),
        None => raw(v),
    }
}

/// Whether a row goes back with Revert, and where it is changed when it does not.
pub fn back(key: &str) -> Back {
    if let Some((_, field)) = program_key(key) {
        return if field == "name" {
            Back::No("Games and apps")
        } else {
            Back::Yes
        };
    }
    if secret(key) {
        return Back::No("Capture");
    }
    if key.starts_with(VOLUMES) {
        return Back::Yes;
    }
    find(key)
        .map(|r| r.back)
        .unwrap_or(Back::No("the app's own pages"))
}

/// A row as Advanced's table shows it: its name, the values in its row's words, where from and
/// when; never a value a row does not show.
pub fn view(r: &Record, ctx: &Ctx) -> Value {
    let note = match back(&r.key) {
        Back::Yes => None,
        Back::No(at) => Some(format!("change it under {at}")),
    };
    json!({
        "id": r.id,
        "name": name(r, ctx),
        "was": value_words(&r.key, &r.old),
        "now": value_words(&r.key, &r.new),
        "from": r.from.words(),
        "at": r.at,
        "revert": note.is_none(),
        "note": note,
    })
}

/// Why a Revert is refused.
pub fn check_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 20 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("that is not a change of this PC's settings history".into());
    }
    Ok(())
}

/// The setting a Revert of `r` writes, and the value it puts back, or why it does not.
pub fn reverted(r: &Record) -> Result<(String, Value), String> {
    if let Back::No(at) = back(&r.key) {
        return Err(format!(
            "this one does not go back from here: change it under {at}"
        ));
    }
    Ok((r.key.clone(), r.old.clone()))
}

/// `settings` with `key` set to `value`: a level set to nothing is taken out.
pub fn put(settings: &Settings, key: &str, value: &Value) -> Result<Settings, String> {
    let mut v = serde_json::to_value(settings).map_err(|e| e.to_string())?;
    let (parents, last) = match key.strip_prefix(VOLUMES) {
        Some(source) => (VOLUMES.trim_end_matches('.').to_string(), source),
        None => match key.rsplit_once('.') {
            Some((p, l)) => (p.to_string(), l),
            None => (String::new(), key),
        },
    };
    let mut at = &mut v;
    if !parents.is_empty() {
        for part in parents.split('.') {
            at = at
                .get_mut(part)
                .ok_or_else(|| format!("{key} is not a setting"))?;
        }
    }
    let map = at
        .as_object_mut()
        .ok_or_else(|| format!("{key} is not a setting"))?;
    if key.starts_with(VOLUMES) && value.is_null() {
        map.remove(last);
    } else if map.contains_key(last) || key.starts_with(VOLUMES) {
        map.insert(last.to_string(), value.clone());
    } else {
        return Err(format!("{key} is not a setting"));
    }
    serde_json::from_value(v).map_err(|e| format!("the old value does not fit any more: {e}"))
}

/// The file, its next id and how many lines it holds.
#[derive(Debug)]
pub struct History {
    file: PathBuf,
    next: u64,
    lines: usize,
}

impl History {
    pub fn open(file: &Path) -> History {
        let mut h = History {
            file: file.to_path_buf(),
            next: 1,
            lines: 0,
        };
        let all = h.read();
        h.lines = all.len();
        h.next = all
            .iter()
            .filter_map(|r| r.id.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        h
    }

    /// Every change kept, the oldest first; a line that does not read is passed over.
    pub fn read(&self) -> Vec<Record> {
        let Ok(text) = std::fs::read_to_string(&self.file) else {
            return Vec::new();
        };
        text.lines()
            .filter_map(|l| serde_json::from_str::<Record>(l).ok())
            .map(|mut r| {
                if secret(&r.key) {
                    r.old = Value::Null;
                    r.new = Value::Null;
                }
                r
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<Record, String> {
        check_id(id)?;
        self.read()
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| "that change is no longer in this PC's settings history".into())
    }

    /// A Revert by id: the change, the setting it writes and the value it puts back, all from
    /// this file; or why not.
    pub fn revert_of(&self, id: &str) -> Result<(Record, String, Value), String> {
        let r = self.get(id)?;
        let (key, value) = reverted(&r)?;
        Ok((r, key, value))
    }

    /// Appends the changes, each with an id and the time; trims the file back to the last
    /// `KEEP` once it holds `TRIM_AT`.
    pub fn add(&mut self, changes: Vec<Change>) -> std::io::Result<Vec<Record>> {
        if changes.is_empty() {
            return Ok(Vec::new());
        }
        let now = Timestamp::now().to_string();
        let records: Vec<Record> = changes
            .into_iter()
            .map(|c| {
                let id = self.next.to_string();
                self.next += 1;
                let secret = secret(&c.key);
                Record {
                    id,
                    at: now.clone(),
                    key: c.key,
                    old: if secret { Value::Null } else { c.old },
                    new: if secret { Value::Null } else { c.new },
                    from: c.from,
                    program: c.program,
                }
            })
            .collect();
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut text = String::new();
        for r in &records {
            text.push_str(&serde_json::to_string(r)?);
            text.push('\n');
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)?;
        f.write_all(text.as_bytes())?;
        drop(f);
        self.lines += records.len();
        if self.lines >= TRIM_AT {
            self.trim()?;
        }
        Ok(records)
    }

    fn trim(&mut self) -> std::io::Result<()> {
        let all = self.read();
        let keep = &all[all.len().saturating_sub(KEEP)..];
        let mut text = String::new();
        for r in keep {
            text.push_str(&serde_json::to_string(r)?);
            text.push('\n');
        }
        let part = crate::settings::part_path(&self.file);
        std::fs::write(&part, text)?;
        std::fs::rename(&part, &self.file)?;
        self.lines = keep.len();
        Ok(())
    }

    /// The rows for Advanced's table, the newest first.
    pub fn page(&self, ctx: &Ctx) -> Vec<Value> {
        let mut rows: Vec<Value> = self.read().iter().map(|r| view(r, ctx)).collect();
        rows.reverse();
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ReplayBuffer;

    fn by(f: Source) -> impl Fn(&str) -> Source {
        move |_| f
    }

    #[test]
    fn a_change_is_one_line_with_both_values_and_the_file_keeps_the_last_500() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings-history.jsonl");
        let mut h = History::open(&file);
        let old = Settings::default();
        let mut new = old.clone();
        new.obs.replay_buffer = ReplayBuffer::Games;
        let kept = h.add(changes(&old, &new, &by(Source::Settings))).unwrap();
        assert_eq!(kept.len(), 1);
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(text.lines().count(), 1);
        let r: Record = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(
            (r.id.as_str(), r.key.as_str(), &r.old, &r.new, r.from),
            (
                "1",
                "obs.replay_buffer",
                &json!("always"),
                &json!("games"),
                Source::Settings
            )
        );
        assert!(r.at.parse::<Timestamp>().is_ok(), "{}", r.at);
        for n in 0..(TRIM_AT as u64) {
            let mut a = Settings::default();
            a.upload.keep_last = 5 + n % 500;
            let mut b = a.clone();
            b.upload.keep_last = 6 + n % 500;
            h.add(changes(&a, &b, &by(Source::Settings))).unwrap();
        }
        let all = h.read();
        assert!(all.len() <= TRIM_AT && all.len() >= KEEP, "{}", all.len());
        assert_eq!(all.last().unwrap().id, (TRIM_AT as u64 + 1).to_string());
        let again = History::open(&file);
        assert_eq!(again.next, TRIM_AT as u64 + 2, "ids go on after a start");
        assert!(again.get("1").is_err(), "the oldest went");
        std::fs::write(&file, "not json\n").unwrap();
        assert!(
            History::open(&file).read().is_empty(),
            "a line that does not read"
        );
    }

    #[test]
    fn a_secret_says_changed_and_its_values_go_nowhere() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("h.jsonl");
        let mut h = History::open(&file);
        let old = Settings::default();
        let mut new = old.clone();
        new.obs.password = "hunter22hunter".into();
        new.obs.setup.password = "made-by-the-app".into();
        let c = changes(&old, &new, &by(Source::Settings));
        assert_eq!(c.len(), 1, "the app's own password is no row: {c:?}");
        assert_eq!(
            (c[0].old.clone(), c[0].new.clone()),
            (Value::Null, Value::Null)
        );
        h.add(c).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("hunter22hunter") && !text.contains("made-by"));
        let row = &h.page(&Ctx::default())[0];
        assert_eq!(
            (row["was"].as_str(), row["now"].as_str()),
            (Some("changed"), Some("changed"))
        );
        assert_eq!(row["revert"], false);
        assert_eq!(row["note"], "change it under Capture");
        std::fs::write(
            &file,
            r#"{"id":"9","at":"2026-11-23T10:00:00Z","key":"obs.password","old":"a","new":"hunter22hunter","from":"settings"}"#,
        )
        .unwrap();
        let read = History::open(&file);
        assert_eq!(
            read.read()[0].new,
            Value::Null,
            "a value written by hand is not read"
        );
        assert!(!serde_json::to_string(&read.page(&Ctx::default()))
            .unwrap()
            .contains("hunter"));
        assert!(read.get("9").map(|r| reverted(&r)).unwrap().is_err());
    }

    /// Every setting that looks like a secret by its name, and every password box of the pages,
    /// is in SECRETS: a new one fails here until it is listed.
    #[test]
    fn every_secret_is_listed() {
        fn leaves(v: &Value, path: &mut Vec<String>, out: &mut Vec<String>) {
            match v {
                Value::Object(m) if !m.is_empty() => {
                    for (k, x) in m {
                        path.push(k.clone());
                        leaves(x, path, out);
                        path.pop();
                    }
                }
                _ => out.push(path.join(".")),
            }
        }
        let mut s = Settings::default();
        s.obs.setup.custom_video = Some(Default::default());
        let mut keys = Vec::new();
        leaves(
            &serde_json::to_value(&s).unwrap(),
            &mut Vec::new(),
            &mut keys,
        );
        assert!(keys.len() > 60, "{keys:?}");
        let looks = |k: &str| {
            let last = k.rsplit('.').next().unwrap_or(k).to_ascii_lowercase();
            ["password", "secret", "token", "credential", "passphrase"]
                .iter()
                .any(|w| last.contains(w))
                || last == "key"
                || last.ends_with("_key")
        };
        for k in keys.iter().filter(|k| looks(k)) {
            assert!(
                SECRETS.contains(&k.as_str()),
                "{k} looks secret: add it to SECRETS"
            );
        }
        for page in [
            include_str!("../../ui/index.html"),
            include_str!("../../ui/setup.html"),
        ] {
            for tag in page.split("<div class=\"srow").skip(1) {
                let tag = tag.split("</div></div>").next().unwrap_or("");
                let hidden =
                    tag.contains("type=\"password\"") || tag.contains("data-kind=\"password\"");
                if let (true, Some(key)) = (hidden, attr(tag, "data-key")) {
                    assert!(SECRETS.contains(&key.as_str()), "{key} is a password box");
                }
            }
        }
        for k in SECRETS {
            assert!(keys.contains(&k.to_string()), "{k} is a setting");
            if let Some(r) = find(k) {
                assert!(matches!(r.words, Words::Secret), "{k}");
            }
        }
    }

    fn attr(tag: &str, name: &str) -> Option<String> {
        tag.split(&format!(" {name}=\""))
            .nth(1)
            .and_then(|v| v.split('"').next())
            .map(str::to_string)
    }

    fn label(tag: &str) -> Option<String> {
        let b = tag.split("<div class=\"slabel\"><b").nth(1)?;
        let b = &b[b.find('>')? + 1..];
        Some(b.split('<').next()?.to_string())
    }

    /// The pages' rows: their key, their id, the words of their label and their markup.
    fn rows() -> Vec<(Option<String>, Option<String>, String, String)> {
        let mut out = Vec::new();
        for page in [
            include_str!("../../ui/index.html"),
            include_str!("../../ui/setup.html"),
        ] {
            for tag in page.split("<div class=\"srow").skip(1) {
                let head = tag.split('>').next().unwrap_or("");
                if let Some(b) = label(tag) {
                    out.push((attr(head, "data-key"), attr(head, "id"), b, tag.to_string()));
                }
            }
        }
        out
    }

    fn keyed() -> Vec<(String, String, String)> {
        rows()
            .into_iter()
            .filter_map(|(k, _, b, tag)| k.map(|k| (k, b, tag)))
            .collect()
    }

    #[test]
    fn every_row_of_the_pages_is_in_the_table_by_the_name_it_shows() {
        let rows = keyed();
        assert!(rows.len() > 60, "{}", rows.len());
        for (key, b, tag) in &rows {
            let r = find(key).unwrap_or_else(|| panic!("{key} ({b}) is not in history::ROWS"));
            assert!(
                r.name == b || r.name.ends_with(&format!(", {b}")),
                "{key}: its row says {b}, the table {}",
                r.name
            );
            if let Words::Choice(list) = r.words {
                for opt in tag.split("<option value=\"").skip(1) {
                    let (v, rest) = opt.split_once('"').unwrap();
                    let words = rest.split_once('>').unwrap().1.split('<').next().unwrap();
                    let w = words.replace("&#39;", "'");
                    assert_eq!(choice(list, &json!(v)), w, "{key} = {v}");
                }
                for card in tag.split("data-value=\"").skip(1) {
                    let (v, rest) = card.split_once('"').unwrap();
                    let words = rest.split("<b>").nth(1).unwrap().split('<').next().unwrap();
                    assert_eq!(choice(list, &json!(v)), words, "{key} = {v}");
                }
            }
        }
        let all = super::tests::rows();
        for r in ROWS {
            let shown = rows.iter().any(|(k, _, _)| k == r.key)
                || r.row.is_some_and(|at| {
                    all.iter().any(|(_, id, b, _)| {
                        (id.as_deref() == Some(at) && b == r.name) || (b == at && at == r.name)
                    })
                });
            assert!(shown, "{} ({}) is on no page", r.key, r.name);
        }
        let pages = [
            include_str!("../../ui/index.html"),
            include_str!("../../ui/setup.html"),
        ]
        .concat();
        let windows = Ctx {
            windows: true,
            ..Ctx::default()
        };
        let named = |key: &str, ctx: &Ctx| {
            name(
                &Record {
                    id: "1".into(),
                    at: String::new(),
                    key: key.into(),
                    old: Value::Null,
                    new: Value::Null,
                    from: Source::Settings,
                    program: Some("Dust Runner".into()),
                },
                ctx,
            )
        };
        assert_eq!(named("startup.autostart", &windows), "Start with Windows");
        assert_eq!(
            named("startup.autostart", &Ctx::default()),
            "Start at logon"
        );
        assert!(pages.contains("data-linux-text=\"Start at logon\""));
        let alone = Ctx {
            standalone: true,
            ..Ctx::default()
        };
        assert_eq!(
            named("sounds.uploaded.enabled", &alone),
            "Clip in the library"
        );
        assert!(pages.contains("data-standalone-text=\"Clip in the library\""));
        assert_eq!(
            named("obs.setup.volumes.Microphone", &alone),
            "Microphone level"
        );
        assert_eq!(named("games/steam-570.class", &alone), "Dust Runner, Class");
        assert_eq!(
            named("games/steam-570.wait", &alone),
            "Dust Runner, Encoding waits"
        );
    }

    #[test]
    fn a_programs_rows_are_named_as_its_page_names_them() {
        let js = include_str!("../../ui/games.js");
        for (_, n) in PROGRAM_ROWS {
            assert!(js.contains(&format!("'{n}'")), "{n} is not in games.js");
        }
        for (v, w) in CLASS_WORDS {
            assert!(js.contains(&format!("{v}: '{w}'")), "{v}");
        }
        for (v, w) in OWN_WORDS {
            assert!(js.contains(&format!("['{v}', '{w}']")), "{v}");
        }
        let old = Entry {
            id: "p-0123456789ab".into(),
            name: "Dust Runner".into(),
            ..Entry::default()
        };
        let mut new = old.clone();
        new.class = crate::programs::Class::Game;
        new.own.buffer = crate::programs::Own::Off;
        let c = program_changes(&old, &new, Source::Settings);
        let keys: Vec<&str> = c.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(
            keys,
            ["games/p-0123456789ab.class", "games/p-0123456789ab.buffer"]
        );
        assert_eq!(value_words(&c[0].key, &c[0].new), "Game");
        assert_eq!(value_words(&c[1].key, &c[1].old), "As its class");
        assert_eq!(
            program_key("games/p-0123456789ab.class"),
            Some(("p-0123456789ab", "class"))
        );
        assert_eq!(program_key("games/p-01.colour"), None);
        let mut renamed = old.clone();
        renamed.name = "Dust Runner II".into();
        let c = program_changes(&old, &renamed, Source::Settings);
        assert_eq!(back(&c[0].key), Back::No("Games and apps"));
    }

    #[test]
    fn the_apps_bookkeeping_is_left_out() {
        let old = Settings::default();
        let mut new = old.clone();
        new.obs.setup.done = true;
        new.obs.own_setup_due = true;
        new.obs.setup_later = true;
        new.last_version = "9.9.9".into();
        new.copy_update_asked = "9.9.9".into();
        new.obs.setup.own_filters = vec![json!({"name": "x"})];
        new.obs.setup.titles = vec!["Game".into()];
        new.obs.setup.rec_tracks = 3;
        new.webview_audio_fallback = true;
        assert_eq!(changes(&old, &new, &by(Source::Reel)), vec![]);
        new.obs.setup.volumes.insert("Microphone".into(), -6.0);
        new.obs.setup.screen.name = "DISPLAY2".into();
        new.obs.setup.screen.width = 2560;
        new.obs.setup.screen.height = 1440;
        let c = changes(&old, &new, &by(Source::Setup));
        let keys: Vec<&str> = c.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["obs.setup.screen", "obs.setup.volumes.Microphone"]);
        assert_eq!(value_words(&c[0].key, &c[0].new), "DISPLAY2, 2560x1440");
        assert_eq!(value_words(&c[1].key, &c[1].old), "0 dB");
        assert_eq!(value_words(&c[1].key, &c[1].new), "-6 dB");
    }

    #[test]
    fn values_read_as_their_rows_show_them() {
        let w = |k: &str, v: Value| value_words(k, &v);
        assert_eq!(w("obs.replay_buffer", json!("games")), "While a game runs");
        assert_eq!(w("obs.setup.clip_seconds", json!(60)), "60 s");
        assert_eq!(w("upload.keep_last", json!(0)), "the whole clip");
        assert_eq!(w("upload.keep_last", json!(30)), "30 s");
        assert_eq!(w("discord.show", json!(true)), "on");
        assert_eq!(w("sounds.captured.volume", json!(0.5)), "50");
        assert_eq!(w("sounds.captured.file", Value::Null), "the bundled one");
        assert_eq!(w("hotkeys.save", json!("")), "none");
        assert_eq!(w("extensions", json!(["mkv", "mp4"])), "mkv, mp4");
        assert_eq!(w("obs.setup.mic", Value::Null), "off");
        assert_eq!(w("obs.setup.encoder", json!("obs_x264")), "x264 (CPU)");
        assert_eq!(
            w("standalone.library_dir", json!("")),
            "in the app's data folder"
        );
        assert_eq!(w("session.remind_minutes", json!(0)), "never");
    }

    #[test]
    fn a_server_url_keeps_no_name_or_password() {
        let old = Settings::default();
        let mut new = old.clone();
        new.server_url = "https://sam:hunter22hunter@clips.example.com".into();
        let c = changes(&old, &new, &by(Source::CommandLine));
        assert_eq!(c[0].new, json!("https://clips.example.com"));
        new.server_url = "https://clips.example.com".into();
        let c = changes(&old, &new, &by(Source::CommandLine));
        assert_eq!(c[0].new, json!("https://clips.example.com"));
    }

    #[test]
    fn a_revert_refuses_an_unknown_id_a_malformed_one_and_a_row_without_revert() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(&dir.path().join("h.jsonl"));
        let old = Settings::default();
        let mut new = old.clone();
        new.upload.keep_last = 30;
        new.mode = crate::settings::Mode::Standalone;
        h.add(changes(&old, &new, &by(Source::Settings))).unwrap();
        let ids: Vec<(String, String)> = h.read().into_iter().map(|r| (r.key, r.id)).collect();
        let id = |k: &str| ids.iter().find(|(key, _)| key == k).unwrap().1.clone();
        let (_, key, value) = h.revert_of(&id("upload.keep_last")).unwrap();
        assert_eq!((key.as_str(), value), ("upload.keep_last", json!(0)));
        let e = h.revert_of(&id("mode")).unwrap_err();
        assert!(e.contains("change it under Connection"), "{e}");
        let e = h.revert_of("999").unwrap_err();
        assert!(e.contains("no longer"), "an unknown id: {e}");
        for bad in ["", "x1", "1 OR 1", "../1", "1e3", "-2"] {
            let e = h.revert_of(bad).unwrap_err();
            assert!(e.contains("not a change"), "{bad:?}: {e}");
        }
    }

    #[test]
    fn a_revert_takes_the_id_alone_and_some_rows_do_not_go_back() {
        for bad in [
            "",
            "abc",
            "-1",
            "1.5",
            "1; 2",
            "999999999999999999999",
            " 1",
        ] {
            assert!(check_id(bad).is_err(), "{bad:?}");
        }
        assert!(check_id("17").is_ok());
        let record = |key: &str, old: Value| Record {
            id: "1".into(),
            at: String::new(),
            key: key.into(),
            old,
            new: Value::Null,
            from: Source::Settings,
            program: None,
        };
        for (key, at) in [
            ("mode", "Connection"),
            ("server_url", "Connection"),
            ("standalone.library_dir", "Library folder"),
            ("obs.setup.screen", "Capture, Run the setup again"),
            ("obs.setup.encoder", "Capture"),
            ("obs.password", "Capture"),
        ] {
            let e = reverted(&record(key, json!("x"))).unwrap_err();
            assert!(e.ends_with(&format!("change it under {at}")), "{key}: {e}");
        }
        let s = Settings::default();
        let (k, v) = reverted(&record("upload.keep_last", json!(30))).unwrap();
        assert_eq!(put(&s, &k, &v).unwrap().upload.keep_last, 30);
        let mut loud = s.clone();
        loud.obs.setup.volumes.insert("Microphone".into(), -6.0);
        let back = put(&loud, "obs.setup.volumes.Microphone", &Value::Null).unwrap();
        assert!(
            back.obs.setup.volumes.is_empty(),
            "no level before: taken out"
        );
        let set = put(&s, "obs.setup.volumes.Microphone", &json!(-3.0)).unwrap();
        assert_eq!(set.obs.setup.volumes["Microphone"], -3.0);
        assert!(put(&s, "obs.nothing", &json!(1)).is_err());
        assert!(put(&s, "upload.keep_last", &json!("thirty")).is_err());
    }
}
