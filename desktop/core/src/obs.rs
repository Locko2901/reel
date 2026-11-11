//! An obs-websocket 5 client.
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

pub const SUBSCRIBE_OUTPUTS: u64 = 64;
/// Scenes, inputs and scene items too: a switched scene, a retargeted game capture or one
/// turned on or off changes the clips' game.
pub const SUBSCRIBE: u64 = SUBSCRIBE_OUTPUTS | 4 | 8 | 128;
/// The inputs' volume meters, about twenty times a second: on only while a page shows them.
pub const SUBSCRIBE_METERS: u64 = 1 << 16;

#[derive(Debug, Clone, PartialEq)]
pub struct ObsConfig {
    pub host: String,
    pub port: u16,
    pub password: String,
}

impl From<&crate::settings::ObsSettings> for ObsConfig {
    fn from(s: &crate::settings::ObsSettings) -> Self {
        ObsConfig {
            host: s.host.clone(),
            port: s.port,
            password: s.password.clone(),
        }
    }
}

impl ObsConfig {
    pub fn url(&self) -> String {
        format!("ws://{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObsEvent {
    Connected {
        obs_version: String,
        ws_version: String,
    },
    Disconnected {
        reason: String,
        /// What the app waits for, for an OBS it does not run.
        words: String,
    },
    ReplayBufferSaved {
        path: String,
    },
    ReplayBufferState {
        active: bool,
    },
    /// A recording started or stopped; `started` is the file it writes once started, `saved`
    /// its file once stopped.
    RecordState {
        active: bool,
        started: Option<String>,
        saved: Option<String>,
    },
    /// OBS split the recording: it writes to `path` now, the file before is whole.
    RecordFile {
        path: String,
    },
    /// The program scene or an input changed; the game the clips are of is read again.
    SceneChanged,
    /// The game OBS's game capture source targets, when it targets a window by name.
    Game {
        game: Option<String>,
    },
    /// Each input's loudest channel's peak in dB (-60 for silence), while the meters are on.
    Meters {
        levels: Vec<(String, f32)>,
    },
}

/// One choice of an input's list property, as OBS names it and stores it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Item {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Version {
    pub obs_version: String,
    pub ws_version: String,
}

pub enum ObsCommand {
    SaveReplay(oneshot::Sender<Result<(), ObsError>>),
    StartReplayBuffer(oneshot::Sender<Result<(), ObsError>>),
    StopReplayBuffer(oneshot::Sender<Result<(), ObsError>>),
    RecordDirectory(oneshot::Sender<Result<String, ObsError>>),
    ReplayBufferActive(oneshot::Sender<Result<bool, ObsError>>),
    StartRecord(oneshot::Sender<Result<(), ObsError>>),
    /// The file OBS wrote, when it says.
    StopRecord(oneshot::Sender<Result<Option<String>, ObsError>>),
    RecordStatus(oneshot::Sender<Result<RecordStatus, ObsError>>),
    State(oneshot::Sender<Result<ObsState, ObsError>>),
    Apply(ObsChange, oneshot::Sender<Result<Vec<String>, ObsError>>),
    /// The choices of an input's list property: the audio devices, the monitors.
    Items {
        input: String,
        property: String,
        reply: oneshot::Sender<Result<Vec<Item>, ObsError>>,
    },
    /// Changes an input's settings now, over what it has.
    SetInput {
        input: String,
        settings: Value,
        reply: oneshot::Sender<Result<(), ObsError>>,
    },
    Meters {
        on: bool,
        reply: oneshot::Sender<Result<(), ObsError>>,
    },
    /// Any other request of obs-websocket, its response data back: the filters, the dialogs.
    Request {
        kind: String,
        data: Value,
        reply: oneshot::Sender<Result<Value, ObsError>>,
    },
}

/// What OBS is set to, read over the WebSocket.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ObsState {
    pub obs_version: String,
    pub ws_version: String,
    /// The profile in use, whose settings these are.
    #[serde(default)]
    pub profile: String,
    /// Advanced output mode keeps the encoder's settings in a file the WebSocket does not reach.
    pub advanced: bool,
    pub record_dir: String,
    /// mkv, mp4, hybrid_mp4, fragmented_mp4, mov, fragmented_mov, ts, flv.
    pub format: String,
    #[serde(default)]
    pub format_name: String,
    pub encoder: String,
    /// The encoder as OBS names it in its own list.
    #[serde(default)]
    pub encoder_name: String,
    /// Simple mode with the recording quality "Same as stream": the stream bitrate in kbit/s.
    pub video_kbit: Option<u32>,
    /// Simple mode with another quality: its name (Small, HQ, Lossless).
    pub quality: Option<String>,
    pub audio_kbit: Option<u32>,
    /// The tracks the recording takes, from the bitmask.
    pub audio_tracks: Vec<u8>,
    pub replay_enabled: bool,
    pub replay_seconds: u32,
    pub replay_active: bool,
    pub recording: bool,
    pub base: (u32, u32),
    pub output: (u32, u32),
    pub fps: f64,
}

/// Whether OBS records, and for how long by its own clock.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RecordStatus {
    pub active: bool,
    pub duration_ms: u64,
    pub bytes: u64,
}

/// What the app may change in OBS; each field is only touched when set.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct ObsChange {
    pub record_dir: Option<String>,
    pub replay_seconds: Option<u32>,
    pub format: Option<String>,
    pub video_kbit: Option<u32>,
    pub replay_enabled: Option<bool>,
}

/// The formats OBS 30 and 31 know under Recording Format.
pub const FORMATS: [&str; 8] = [
    "mkv",
    "mp4",
    "hybrid_mp4",
    "fragmented_mp4",
    "mov",
    "fragmented_mov",
    "ts",
    "flv",
];

fn tracks_of(mask: u32) -> Vec<u8> {
    (0..6u8)
        .filter(|i| mask & (1 << i) != 0)
        .map(|i| i + 1)
        .collect()
}

/// An encoder id as OBS's own list words it; an unknown id still reads as words.
pub fn encoder_word(id: &str) -> String {
    let w = match id {
        "x264" | "obs_x264" => "x264 (CPU)",
        "obs_nvenc_h264_tex" | "jim_nvenc" | "ffmpeg_nvenc" | "obs_nvenc_h264_cuda" => {
            "NVIDIA NVENC H.264"
        }
        "obs_nvenc_hevc_tex" | "jim_hevc_nvenc" | "obs_nvenc_hevc_cuda" => "NVIDIA NVENC HEVC",
        "obs_nvenc_av1_tex" | "jim_av1_nvenc" | "obs_nvenc_av1_cuda" => "NVIDIA NVENC AV1",
        "obs_qsv11_v2" | "obs_qsv11" => "Intel Quick Sync H.264",
        "obs_qsv11_hevc" => "Intel Quick Sync HEVC",
        "obs_qsv11_av1" => "Intel Quick Sync AV1",
        "h264_texture_amf" | "amd_amf_h264" => "AMD AMF H.264",
        "h265_texture_amf" | "amd_amf_h265" => "AMD AMF HEVC",
        "av1_texture_amf" => "AMD AMF AV1",
        "ffmpeg_vaapi_tex" | "ffmpeg_vaapi" => "VAAPI H.264",
        "hevc_ffmpeg_vaapi_tex" | "hevc_ffmpeg_vaapi" => "VAAPI HEVC",
        "av1_ffmpeg_vaapi_tex" | "av1_ffmpeg_vaapi" => "VAAPI AV1",
        "ffmpeg_svt_av1" => "SVT-AV1 (CPU)",
        "ffmpeg_aom_av1" => "AOM AV1 (CPU)",
        "obs_x265" | "ffmpeg_hevc" => "x265 (CPU)",
        "com.apple.videotoolbox.videoencoder.ave.avc" => "Apple VT H.264",
        "com.apple.videotoolbox.videoencoder.ave.hevc" => "Apple VT HEVC",
        other => return other.replace(['_', '.'], " "),
    };
    w.to_string()
}

pub fn format_word(id: &str) -> String {
    let w = match id {
        "mkv" => "MKV",
        "mp4" => "MP4",
        "hybrid_mp4" => "hybrid MP4",
        "fragmented_mp4" => "fragmented MP4",
        "mov" => "MOV",
        "fragmented_mov" => "fragmented MOV",
        "ts" => "MPEG-TS",
        "flv" => "FLV",
        other => return other.replace('_', " "),
    };
    w.to_string()
}

#[derive(Debug, thiserror::Error)]
pub enum ObsError {
    #[error("cannot reach OBS at {0}: {1}")]
    Connect(String, String),
    #[error("OBS refused the password")]
    Auth,
    #[error("OBS needs a password and none is set")]
    NeedsPassword,
    #[error("OBS said: {0}")]
    Request(String),
    #[error("{0}")]
    Busy(String),
    #[error("the connection to OBS ended: {0}")]
    Closed(String),
    #[error("{0}")]
    Protocol(String),
}

impl ObsError {
    /// What the Test button shows: what to do, and the OS or protocol line when there is one.
    pub fn explain(&self, host: &str, port: u16) -> (String, Option<String>) {
        match self {
            ObsError::Connect(url, cause) => {
                let c = cause.to_lowercase();
                let words = if c.contains("refused") || c.contains("10061") {
                    format!(
                        "Nothing answers at {url}. In OBS open Tools, WebSocket Server Settings, \
                         tick Enable WebSocket server and check that the port there is {port}."
                    )
                } else if c.contains("timed out") {
                    format!(
                        "No answer from {url}. Is OBS running on {host}, and does its firewall let \
                         port {port} through?"
                    )
                } else {
                    format!("Cannot reach OBS at {url}. Check the host and the port.")
                };
                (words, Some(cause.clone()))
            }
            ObsError::Auth => (
                "OBS refused the password. Copy it from Tools, WebSocket Server Settings.".into(),
                None,
            ),
            ObsError::NeedsPassword => (
                "OBS asks for a password: copy it from Tools, WebSocket Server Settings into the \
                 field above."
                    .into(),
                None,
            ),
            ObsError::Request(m) => (
                "OBS answered but refused the request.".into(),
                Some(m.clone()),
            ),
            ObsError::Busy(m) => (m.clone(), None),
            ObsError::Closed(m) => ("OBS closed the connection.".into(), Some(m.clone())),
            ObsError::Protocol(m) => (
                "That is not obs-websocket answering; is the port right?".into(),
                Some(m.clone()),
            ),
        }
    }
}

/// The line while the app waits for an OBS it does not run: what it waits for, or the error
/// when waiting alone does not help (a password).
pub fn waiting_words(e: &ObsError, host: &str, port: u16) -> String {
    let at = format!("{host}:{port}");
    match e {
        ObsError::Connect(_, cause)
            if cause.to_lowercase().contains("refused") || cause.contains("10061") =>
        {
            format!("waiting for OBS at {at}: it is not running, or its WebSocket server is off; reel connects once it answers")
        }
        ObsError::Connect(..) => {
            format!("waiting for OBS at {at}, which does not answer; reel connects once it does")
        }
        ObsError::Closed(why) => gone_words(why, host, port),
        other => format!("not connected: {other}"),
    }
}

/// The line once a connection to an OBS the app does not run ended.
fn gone_words(why: &str, host: &str, port: u16) -> String {
    format!("waiting for OBS at {host}:{port} to come back ({why}); reel connects once it answers")
}

/// obs-websocket's auth string: base64(sha256(base64(sha256(password + salt)) + challenge)).
pub fn auth_string(password: &str, salt: &str, challenge: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD;
    let secret = b64.encode(Sha256::digest(format!("{password}{salt}").as_bytes()));
    b64.encode(Sha256::digest(format!("{secret}{challenge}").as_bytes()))
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct Session {
    socket: Socket,
    events: VecDeque<ObsEvent>,
    next_id: u64,
    pub version: Version,
    /// Why the connection is no good any more: a request found it closed or got no answer.
    broken: Option<String>,
}

/// How long OBS has for its Hello and to take the password.
const HANDSHAKE: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 10 });
/// How long OBS has to answer a request.
const ANSWER: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 10 });
/// How long a connection may be quiet before OBS is asked something to prove it still answers.
const QUIET: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 15 });

/// The next message of the handshake; a frozen OBS takes the connection and says nothing.
async fn handshake_json(socket: &mut Socket, url: &str) -> Result<Value, ObsError> {
    tokio::time::timeout(HANDSHAKE, next_json(socket))
        .await
        .map_err(|_| ObsError::Connect(url.to_string(), "timed out in the handshake".into()))?
}

fn text(v: &Value) -> Message {
    Message::text(v.to_string())
}

async fn next_json(socket: &mut Socket) -> Result<Value, ObsError> {
    loop {
        let msg = match socket.next().await {
            Some(Ok(m)) => m,
            Some(Err(e)) => return Err(ObsError::Closed(e.to_string())),
            None => return Err(ObsError::Closed("OBS closed the socket".into())),
        };
        match msg {
            Message::Text(t) => {
                return serde_json::from_str(t.as_str())
                    .map_err(|e| ObsError::Protocol(format!("not JSON: {e}")))
            }
            Message::Binary(b) => {
                return serde_json::from_slice(&b)
                    .map_err(|e| ObsError::Protocol(format!("not JSON: {e}")))
            }
            Message::Ping(p) => {
                let _ = socket.send(Message::Pong(p)).await;
            }
            Message::Close(f) => {
                return Err(ObsError::Closed(
                    f.map(|f| f.reason.to_string()).unwrap_or_default(),
                ))
            }
            _ => {}
        }
    }
}

impl Session {
    /// Connects and identifies; the Hello carries a challenge when OBS has a password.
    pub async fn connect(cfg: &ObsConfig) -> Result<Session, ObsError> {
        let url = cfg.url();
        let connect = tokio::time::timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(&url),
        );
        let (mut socket, _) = match connect.await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return Err(ObsError::Connect(url, e.to_string())),
            Err(_) => return Err(ObsError::Connect(url, "timed out".into())),
        };
        let hello = handshake_json(&mut socket, &url).await?;
        if hello["op"].as_u64() != Some(0) {
            return Err(ObsError::Protocol("expected a Hello".into()));
        }
        let ws_version = hello["d"]["obsWebSocketVersion"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let mut identify = json!({ "rpcVersion": 1, "eventSubscriptions": SUBSCRIBE });
        if let Some(auth) = hello["d"]["authentication"].as_object() {
            if cfg.password.is_empty() {
                return Err(ObsError::NeedsPassword);
            }
            let salt = auth.get("salt").and_then(|v| v.as_str()).unwrap_or("");
            let challenge = auth.get("challenge").and_then(|v| v.as_str()).unwrap_or("");
            identify["authentication"] = Value::String(auth_string(&cfg.password, salt, challenge));
        }
        socket
            .send(text(&json!({ "op": 1, "d": identify })))
            .await
            .map_err(|e| ObsError::Closed(e.to_string()))?;
        let identified = match handshake_json(&mut socket, &url).await {
            Ok(v) => v,
            Err(ObsError::Closed(_)) => return Err(ObsError::Auth),
            Err(e) => return Err(e),
        };
        if identified["op"].as_u64() != Some(2) {
            return Err(ObsError::Protocol("expected Identified".into()));
        }
        let mut s = Session {
            socket,
            events: VecDeque::new(),
            next_id: 1,
            broken: None,
            version: Version {
                obs_version: String::new(),
                ws_version,
            },
        };
        let v = s.request("GetVersion", json!({})).await?;
        s.version.obs_version = v["obsVersion"].as_str().unwrap_or("").to_string();
        if let Some(w) = v["obsWebSocketVersion"].as_str() {
            s.version.ws_version = w.to_string();
        }
        Ok(s)
    }

    fn event_of(v: &Value) -> Option<ObsEvent> {
        let d = &v["d"];
        match d["eventType"].as_str()? {
            "ReplayBufferSaved" => Some(ObsEvent::ReplayBufferSaved {
                path: d["eventData"]["savedReplayPath"].as_str()?.to_string(),
            }),
            "ReplayBufferStateChanged" => Some(ObsEvent::ReplayBufferState {
                active: d["eventData"]["outputActive"].as_bool()?,
            }),
            "RecordStateChanged" => {
                let state = &d["eventData"]["outputState"];
                let path = || d["eventData"]["outputPath"].as_str().map(str::to_string);
                Some(ObsEvent::RecordState {
                    active: d["eventData"]["outputActive"].as_bool()?,
                    started: (state == "OBS_WEBSOCKET_OUTPUT_STARTED")
                        .then(path)
                        .flatten(),
                    saved: (state == "OBS_WEBSOCKET_OUTPUT_STOPPED")
                        .then(path)
                        .flatten(),
                })
            }
            "RecordFileChanged" => Some(ObsEvent::RecordFile {
                path: d["eventData"]["newOutputPath"].as_str()?.to_string(),
            }),
            "CurrentProgramSceneChanged"
            | "InputSettingsChanged"
            | "SceneItemEnableStateChanged" => Some(ObsEvent::SceneChanged),
            "InputVolumeMeters" => Some(ObsEvent::Meters {
                levels: d["eventData"]["inputs"]
                    .as_array()?
                    .iter()
                    .map(|i| {
                        let peak = i["inputLevelsMul"]
                            .as_array()
                            .map(|chs| {
                                chs.iter()
                                    .filter_map(|c| c[1].as_f64())
                                    .fold(0.0_f64, f64::max)
                            })
                            .unwrap_or(0.0);
                        let db = if peak > 0.001 {
                            20.0 * peak.log10()
                        } else {
                            -60.0
                        };
                        (
                            i["inputName"].as_str().unwrap_or_default().to_string(),
                            db.max(-60.0) as f32,
                        )
                    })
                    .collect(),
            }),
            _ => None,
        }
    }

    /// Sends a request and waits for its answer, keeping any events that arrive meanwhile.
    pub async fn request(&mut self, kind: &str, data: Value) -> Result<Value, ObsError> {
        let answer = self.answer(kind, data).await;
        if let Err(ObsError::Closed(why)) = &answer {
            self.broken = Some(why.clone());
        }
        answer
    }

    async fn answer(&mut self, kind: &str, data: Value) -> Result<Value, ObsError> {
        let id = format!("r{}", self.next_id);
        self.next_id += 1;
        self.socket
            .send(text(&json!({ "op": 6, "d": { "requestType": kind, "requestId": id, "requestData": data } })))
            .await
            .map_err(|e| ObsError::Closed(e.to_string()))?;
        loop {
            let v = tokio::time::timeout(ANSWER, next_json(&mut self.socket))
                .await
                .map_err(|_| {
                    ObsError::Closed(format!("no answer to {kind} in {} s", ANSWER.as_secs()))
                })??;
            match v["op"].as_u64() {
                Some(5) => {
                    if let Some(e) = Self::event_of(&v) {
                        self.events.push_back(e);
                    }
                }
                Some(7) if v["d"]["requestId"].as_str() == Some(id.as_str()) => {
                    let st = &v["d"]["requestStatus"];
                    if st["result"].as_bool() == Some(true) {
                        return Ok(v["d"]["responseData"].clone());
                    }
                    return Err(ObsError::Request(format!(
                        "{} ({})",
                        st["comment"].as_str().unwrap_or("refused"),
                        st["code"]
                    )));
                }
                _ => {}
            }
        }
    }

    /// The next output event, buffered ones first.
    pub async fn next_event(&mut self) -> Result<ObsEvent, ObsError> {
        if let Some(e) = self.events.pop_front() {
            return Ok(e);
        }
        loop {
            let v = next_json(&mut self.socket).await?;
            if v["op"].as_u64() == Some(5) {
                if let Some(e) = Self::event_of(&v) {
                    return Ok(e);
                }
            }
        }
    }

    /// The game the program scene's first enabled game capture targets by window name; None
    /// when it captures any fullscreen app.
    pub async fn game_capture_target(&mut self) -> Result<Option<String>, ObsError> {
        let scene = self.request("GetCurrentProgramScene", json!({})).await?;
        let Some(name) = scene["sceneName"]
            .as_str()
            .or_else(|| scene["currentProgramSceneName"].as_str())
            .map(str::to_string)
        else {
            return Ok(None);
        };
        let items = self
            .request("GetSceneItemList", json!({ "sceneName": name }))
            .await?;
        for item in items["sceneItems"].as_array().into_iter().flatten() {
            let kind = item["inputKind"].as_str().unwrap_or("");
            if !kind.starts_with("game_capture") || item["sceneItemEnabled"] == json!(false) {
                continue;
            }
            let Some(source) = item["sourceName"].as_str() else {
                continue;
            };
            let s = self
                .request("GetInputSettings", json!({ "inputName": source }))
                .await?;
            let settings = &s["inputSettings"];
            if settings["capture_mode"].as_str() != Some("window") {
                continue;
            }
            if let Some(game) = settings["window"].as_str().and_then(game_of_window) {
                return Ok(Some(game));
            }
        }
        Ok(None)
    }

    pub async fn record_directory(&mut self) -> Result<String, ObsError> {
        let v = self.request("GetRecordDirectory", json!({})).await?;
        Ok(v["recordDirectory"].as_str().unwrap_or("").to_string())
    }

    pub async fn save_replay(&mut self) -> Result<(), ObsError> {
        self.request("SaveReplayBuffer", json!({}))
            .await
            .map(|_| ())
    }

    pub async fn items(&mut self, input: &str, property: &str) -> Result<Vec<Item>, ObsError> {
        let v = self
            .request(
                "GetInputPropertiesListPropertyItems",
                json!({ "inputName": input, "propertyName": property }),
            )
            .await?;
        Ok(v["propertyItems"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|i| i["itemEnabled"].as_bool().unwrap_or(true))
                    .map(|i| Item {
                        name: i["itemName"].as_str().unwrap_or_default().to_string(),
                        value: match &i["itemValue"] {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        },
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn set_input(&mut self, input: &str, settings: Value) -> Result<(), ObsError> {
        self.request(
            "SetInputSettings",
            json!({ "inputName": input, "inputSettings": settings, "overlay": true }),
        )
        .await
        .map(|_| ())
    }

    /// Identifies again with other subscriptions; OBS's answer needs no waiting for.
    pub async fn set_meters(&mut self, on: bool) -> Result<(), ObsError> {
        let subs = if on {
            SUBSCRIBE | SUBSCRIBE_METERS
        } else {
            SUBSCRIBE
        };
        self.socket
            .send(text(
                &json!({ "op": 3, "d": { "eventSubscriptions": subs } }),
            ))
            .await
            .map_err(|e| ObsError::Closed(e.to_string()))
    }

    pub async fn start_replay_buffer(&mut self) -> Result<(), ObsError> {
        self.request("StartReplayBuffer", json!({}))
            .await
            .map(|_| ())
    }

    pub async fn replay_buffer_active(&mut self) -> Result<bool, ObsError> {
        let v = self.request("GetReplayBufferStatus", json!({})).await?;
        Ok(v["outputActive"].as_bool().unwrap_or(false))
    }

    pub async fn stop_replay_buffer(&mut self) -> Result<(), ObsError> {
        self.request("StopReplayBuffer", json!({}))
            .await
            .map(|_| ())
    }

    pub async fn recording(&mut self) -> Result<bool, ObsError> {
        Ok(self.record_status().await?.active)
    }

    pub async fn record_status(&mut self) -> Result<RecordStatus, ObsError> {
        let v = self.request("GetRecordStatus", json!({})).await?;
        Ok(RecordStatus {
            active: v["outputActive"].as_bool().unwrap_or(false),
            duration_ms: v["outputDuration"].as_u64().unwrap_or(0),
            bytes: v["outputBytes"].as_u64().unwrap_or(0),
        })
    }

    pub async fn start_record(&mut self) -> Result<(), ObsError> {
        self.request("StartRecord", json!({})).await.map(|_| ())
    }

    /// Stops the recording; OBS 30 and later answer with its file.
    pub async fn stop_record(&mut self) -> Result<Option<String>, ObsError> {
        let v = self.request("StopRecord", json!({})).await?;
        Ok(v["outputPath"].as_str().map(str::to_string))
    }

    /// One value of the profile's basic.ini, None when the key is not set.
    pub async fn profile_parameter(
        &mut self,
        category: &str,
        name: &str,
    ) -> Result<Option<String>, ObsError> {
        let v = self
            .request(
                "GetProfileParameter",
                json!({ "parameterCategory": category, "parameterName": name }),
            )
            .await?;
        Ok(v["parameterValue"].as_str().map(|s| s.to_string()))
    }

    pub async fn set_profile_parameter(
        &mut self,
        category: &str,
        name: &str,
        value: &str,
    ) -> Result<(), ObsError> {
        self.request(
            "SetProfileParameter",
            json!({ "parameterCategory": category, "parameterName": name, "parameterValue": value }),
        )
        .await
        .map(|_| ())
    }

    /// Everything the OBS page shows, in one round of requests.
    pub async fn state(&mut self) -> Result<ObsState, ObsError> {
        let mut st = ObsState {
            obs_version: self.version.obs_version.clone(),
            ws_version: self.version.ws_version.clone(),
            ..ObsState::default()
        };
        st.profile = self
            .request("GetProfileList", json!({}))
            .await?
            .get("currentProfileName")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        st.advanced =
            self.profile_parameter("Output", "Mode").await?.as_deref() == Some("Advanced");
        let cat = if st.advanced {
            "AdvOut"
        } else {
            "SimpleOutput"
        };
        st.record_dir = self.record_directory().await?;
        st.format = self
            .profile_parameter(cat, "RecFormat2")
            .await?
            .unwrap_or_else(|| "mkv".into());
        st.encoder = self
            .profile_parameter(cat, "RecEncoder")
            .await?
            .unwrap_or_else(|| {
                if st.advanced {
                    "obs_x264".into()
                } else {
                    "x264".into()
                }
            });
        st.format_name = format_word(&st.format);
        st.encoder_name = encoder_word(&st.encoder);
        if cfg!(windows) {
            st.record_dir = st.record_dir.replace('/', "\\");
        }
        let num = |v: Option<String>| v.and_then(|s| s.trim().parse::<u32>().ok());
        if st.advanced {
            let mask = num(self.profile_parameter(cat, "RecTracks").await?).unwrap_or(1);
            st.audio_tracks = tracks_of(mask);
        } else {
            let quality = self
                .profile_parameter(cat, "RecQuality")
                .await?
                .unwrap_or_else(|| "Stream".into());
            if quality == "Stream" {
                st.video_kbit = num(self.profile_parameter(cat, "VBitrate").await?);
            } else {
                st.quality = Some(quality);
            }
            st.audio_kbit = num(self.profile_parameter(cat, "ABitrate").await?);
            let mask = num(self.profile_parameter(cat, "RecTracks").await?).unwrap_or(1);
            st.audio_tracks = tracks_of(mask);
        }
        st.replay_enabled = self
            .profile_parameter(cat, "RecRB")
            .await?
            .map(|v| v == "true")
            .unwrap_or(false);
        st.replay_seconds = num(self.profile_parameter(cat, "RecRBTime").await?).unwrap_or(20);
        st.replay_active = self.replay_buffer_active().await?;
        st.recording = self.recording().await?;
        let v = self.request("GetVideoSettings", json!({})).await?;
        st.base = (
            v["baseWidth"].as_u64().unwrap_or(0) as u32,
            v["baseHeight"].as_u64().unwrap_or(0) as u32,
        );
        st.output = (
            v["outputWidth"].as_u64().unwrap_or(0) as u32,
            v["outputHeight"].as_u64().unwrap_or(0) as u32,
        );
        let n = v["fpsNumerator"].as_f64().unwrap_or(0.0);
        let d = v["fpsDenominator"].as_f64().unwrap_or(1.0);
        st.fps = if d > 0.0 { n / d } else { 0.0 };
        Ok(st)
    }

    /// Refused while OBS records, as the outputs are in use; a running replay buffer is
    /// restarted so the change takes.
    pub async fn apply(&mut self, change: &ObsChange) -> Result<Vec<String>, ObsError> {
        if self.recording().await? {
            return Err(ObsError::Busy(
                "OBS is recording; the outputs are in use. Stop the recording in OBS first.".into(),
            ));
        }
        let advanced =
            self.profile_parameter("Output", "Mode").await?.as_deref() == Some("Advanced");
        let cat = if advanced { "AdvOut" } else { "SimpleOutput" };
        let mut done = Vec::new();
        if let Some(dir) = &change.record_dir {
            let dir = dir.trim();
            if dir.is_empty() {
                return Err(ObsError::Request("the folder cannot be empty".into()));
            }
            let set = self
                .request("SetRecordDirectory", json!({ "recordDirectory": dir }))
                .await;
            if set.is_err() {
                let key = if advanced { "RecFilePath" } else { "FilePath" };
                self.set_profile_parameter(cat, key, dir).await?;
            }
            done.push(format!("the folder is {dir}"));
        }
        if let Some(secs) = change.replay_seconds {
            if !(1..=3600).contains(&secs) {
                return Err(ObsError::Request(
                    "the replay length is between 1 and 3600 seconds".into(),
                ));
            }
            self.set_profile_parameter(cat, "RecRBTime", &secs.to_string())
                .await?;
            done.push(format!("the replay buffer keeps {secs} s"));
        }
        if let Some(fmt) = &change.format {
            if !FORMATS.contains(&fmt.as_str()) {
                return Err(ObsError::Request(format!(
                    "{fmt} is not a format OBS knows"
                )));
            }
            self.set_profile_parameter(cat, "RecFormat2", fmt).await?;
            done.push(format!("the format is {fmt}"));
        }
        if let Some(kbit) = change.video_kbit {
            if advanced {
                return Err(ObsError::Request(
                    "the bitrate sits in the encoder's own settings in advanced output mode; set it in OBS under Settings, Output, Recording".into(),
                ));
            }
            if !(500..=300_000).contains(&kbit) {
                return Err(ObsError::Request(
                    "the bitrate is between 500 and 300000 kbit/s".into(),
                ));
            }
            self.set_profile_parameter(cat, "RecQuality", "Stream")
                .await?;
            self.set_profile_parameter(cat, "VBitrate", &kbit.to_string())
                .await?;
            done.push(format!("the video bitrate is {kbit} kbit/s"));
        }
        if let Some(on) = change.replay_enabled {
            self.set_profile_parameter(cat, "RecRB", if on { "true" } else { "false" })
                .await?;
            done.push(if on {
                "the replay buffer is enabled".into()
            } else {
                "the replay buffer is disabled".into()
            });
        }
        if !done.is_empty() && self.replay_buffer_active().await? {
            self.stop_replay_buffer().await?;
            tokio::time::sleep(Duration::from_millis(400)).await;
            self.start_replay_buffer().await?;
            done.push("the replay buffer was restarted with them".into());
        }
        Ok(done)
    }
}

/// The Test button: connect, identify, read the versions and what OBS is set to, hang up.
/// An OBS that answers the versions and refuses the rest is still one that answers.
pub async fn test_connection(cfg: &ObsConfig) -> Result<(Version, Option<ObsState>), ObsError> {
    let mut s = Session::connect(cfg).await?;
    let st = s.state().await.ok();
    Ok((s.version.clone(), st))
}

/// OBS writes the window as "Title:Class:exe.exe", with `#3A` for a colon and `#22` for a hash.
pub fn game_of_window(window: &str) -> Option<String> {
    let mut parts = window.split(':');
    let title = parts.next().unwrap_or("");
    let _class = parts.next()?;
    let exe = parts.next()?;
    let undo = |s: &str| s.replace("#3A", ":").replace("#22", "#");
    crate::gaming::game_name(&undo(exe), &undo(title))
}

/// Resolves once reel's own OBS is asked to close; never without `ready`.
async fn closing(ready: &mut Option<tokio::sync::watch::Receiver<bool>>) {
    match ready {
        Some(r) => {
            if r.wait_for(|v| !*v).await.is_err() {
                std::future::pending::<()>().await;
            }
        }
        None => std::future::pending::<()>().await,
    }
}

/// Stays connected to OBS until cancelled. With `ready`, for reel's own OBS, it waits for an
/// OBS not asked to close and hangs up once it is.
pub async fn run(
    cfg: ObsConfig,
    tx: mpsc::Sender<ObsEvent>,
    mut commands: mpsc::Receiver<ObsCommand>,
    cancel: CancellationToken,
    mut ready: Option<tokio::sync::watch::Receiver<bool>>,
) {
    let mut wait = Duration::from_secs(2);
    loop {
        if cancel.is_cancelled() {
            return;
        }
        if let Some(r) = ready.as_mut() {
            let gone = tokio::select! {
                _ = cancel.cancelled() => return,
                up = r.wait_for(|v| *v) => up.is_err(),
            };
            if gone {
                ready = None;
            }
        }
        match Session::connect(&cfg).await {
            Ok(mut session) => {
                wait = Duration::from_secs(2);
                tracing::info!(obs = %session.version.obs_version, ws = %session.version.ws_version, "OBS connected");
                let _ = tx
                    .send(ObsEvent::Connected {
                        obs_version: session.version.obs_version.clone(),
                        ws_version: session.version.ws_version.clone(),
                    })
                    .await;
                let game = session.game_capture_target().await.unwrap_or(None);
                let _ = tx.send(ObsEvent::Game { game }).await;
                let quiet = tokio::time::sleep(QUIET);
                tokio::pin!(quiet);
                let reason = loop {
                    if let Some(why) = session.broken.take() {
                        break why;
                    }
                    quiet.as_mut().reset(tokio::time::Instant::now() + QUIET);
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = closing(&mut ready) => break "reel asked OBS to close".to_string(),
                        _ = &mut quiet => {
                            if let Err(e) = session.request("GetVersion", json!({})).await {
                                tracing::warn!(error = %e, "OBS did not answer on a quiet connection");
                            }
                        }
                        ev = session.next_event() => match ev {
                            Ok(ObsEvent::SceneChanged) => {
                                let game = session.game_capture_target().await.unwrap_or(None);
                                if tx.send(ObsEvent::Game { game }).await.is_err() { return; }
                            }
                            Ok(e @ ObsEvent::Meters { .. }) => {
                                if tx.send(e).await.is_err() { return; }
                            }
                            Ok(e) => {
                                tracing::info!(event = ?e, "OBS event");
                                if tx.send(e).await.is_err() { return; }
                            }
                            Err(e) => break e.to_string(),
                        },
                        cmd = commands.recv() => match cmd {
                            None => return,
                            Some(ObsCommand::SaveReplay(reply)) => { let _ = reply.send(session.save_replay().await); }
                            Some(ObsCommand::StartReplayBuffer(reply)) => { let _ = reply.send(session.start_replay_buffer().await); }
                            Some(ObsCommand::StopReplayBuffer(reply)) => { let _ = reply.send(session.stop_replay_buffer().await); }
                            Some(ObsCommand::RecordDirectory(reply)) => { let _ = reply.send(session.record_directory().await); }
                            Some(ObsCommand::ReplayBufferActive(reply)) => { let _ = reply.send(session.replay_buffer_active().await); }
                            Some(ObsCommand::StartRecord(reply)) => { let _ = reply.send(session.start_record().await); }
                            Some(ObsCommand::StopRecord(reply)) => { let _ = reply.send(session.stop_record().await); }
                            Some(ObsCommand::RecordStatus(reply)) => { let _ = reply.send(session.record_status().await); }
                            Some(ObsCommand::State(reply)) => { let _ = reply.send(session.state().await); }
                            Some(ObsCommand::Apply(change, reply)) => { let _ = reply.send(session.apply(&change).await); }
                            Some(ObsCommand::Items { input, property, reply }) => { let _ = reply.send(session.items(&input, &property).await); }
                            Some(ObsCommand::SetInput { input, settings, reply }) => { let _ = reply.send(session.set_input(&input, settings).await); }
                            Some(ObsCommand::Meters { on, reply }) => { let _ = reply.send(session.set_meters(on).await); }
                            Some(ObsCommand::Request { kind, data, reply }) => { let _ = reply.send(session.request(&kind, data).await); }
                        },
                    }
                };
                tracing::warn!(reason, "OBS disconnected");
                let words = gone_words(&reason, &cfg.host, cfg.port);
                let _ = tx.send(ObsEvent::Disconnected { reason, words }).await;
            }
            Err(e) => {
                tracing::debug!(error = %e, "OBS not reachable");
                let _ = tx
                    .send(ObsEvent::Disconnected {
                        reason: e.to_string(),
                        words: waiting_words(&e, &cfg.host, cfg.port),
                    })
                    .await;
            }
        }
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(wait) => {},
        }
        // A local OBS is retried every few seconds, so its start is seen soon.
        let most = if matches!(cfg.host.as_str(), "127.0.0.1" | "localhost" | "::1") {
            3
        } else {
            30
        };
        wait = (wait * 2).min(Duration::from_secs(most));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_event_reel_reads_is_in_a_category_it_subscribes_to() {
        // obs-websocket 5's event categories (its protocol's EventSubscription)
        let category = |event: &str| match event {
            "CurrentProgramSceneChanged" => Some(4),
            "InputSettingsChanged" => Some(8),
            "ReplayBufferSaved"
            | "ReplayBufferStateChanged"
            | "RecordStateChanged"
            | "RecordFileChanged" => Some(64),
            "SceneItemEnableStateChanged" => Some(128),
            "InputVolumeMeters" => Some(1 << 16),
            _ => None,
        };
        let code = include_str!("obs.rs");
        let body = code
            .split("fn event_of(")
            .nth(1)
            .and_then(|b| b.split("\n    }\n").next())
            .unwrap();
        let read: Vec<&str> = body
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|w| w.ends_with("Changed") || w.ends_with("Saved") || w.ends_with("Meters"))
            .collect();
        assert!(read.len() >= 6, "{read:?}");
        for event in read {
            let Some(bit) = category(event) else {
                panic!("{event}: which category is it in?");
            };
            let asked = if bit == SUBSCRIBE_METERS {
                SUBSCRIBE | SUBSCRIBE_METERS
            } else {
                SUBSCRIBE
            };
            assert_ne!(
                asked & bit,
                0,
                "{event} is read, its category never asked for"
            );
        }
    }

    #[test]
    fn the_meters_give_each_inputs_loudest_peak_in_db() {
        let ev = json!({ "op": 5, "d": { "eventType": "InputVolumeMeters", "eventData": { "inputs": [
            { "inputName": "Desktop audio", "inputLevelsMul": [[0.1, 0.25, 0.3], [0.1, 0.5, 0.6]] },
            { "inputName": "Microphone", "inputLevelsMul": [] },
        ] } } });
        match Session::event_of(&ev) {
            Some(ObsEvent::Meters { levels }) => {
                assert_eq!(levels[0].0, "Desktop audio");
                assert!((levels[0].1 - (-6.02)).abs() < 0.01, "{}", levels[0].1);
                assert_eq!(levels[1], ("Microphone".to_string(), -60.0));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_game_capture_window_names_the_game() {
        assert_eq!(
            game_of_window("VALORANT  #3A ranked:UnrealWindow:VALORANT-Win64-Shipping.exe")
                .as_deref(),
            Some("Valorant")
        );
        assert_eq!(
            game_of_window("Beat Saber:UnityWndClass:Beat Saber.exe").as_deref(),
            Some("Beat Saber")
        );
        assert_eq!(game_of_window("Zen:MozillaWindowClass:zen.exe"), None);
        assert_eq!(game_of_window("no colons here"), None);
    }

    #[test]
    fn a_recording_names_its_file_when_it_starts_when_it_splits_and_when_it_stops() {
        let event = |kind: &str, data: Value| {
            Session::event_of(
                &json!({ "op": 5, "d": { "eventType": kind, "eventIntent": 64, "eventData": data } }),
            )
        };
        let state = |active: bool, state: &str, path: Option<&str>| {
            event(
                "RecordStateChanged",
                json!({ "outputActive": active, "outputState": state, "outputPath": path }),
            )
        };
        // the events OBS 33.0.1 sent in the run with the real OBS
        assert_eq!(
            state(false, "OBS_WEBSOCKET_OUTPUT_STARTING", None),
            Some(ObsEvent::RecordState {
                active: false,
                started: None,
                saved: None
            })
        );
        assert_eq!(
            state(
                true,
                "OBS_WEBSOCKET_OUTPUT_STARTED",
                Some("/clips/2026-11-11 08-36-45.mkv")
            ),
            Some(ObsEvent::RecordState {
                active: true,
                started: Some("/clips/2026-11-11 08-36-45.mkv".into()),
                saved: None
            })
        );
        assert_eq!(
            state(
                false,
                "OBS_WEBSOCKET_OUTPUT_STOPPED",
                Some("/clips/2026-11-11 08-36-45.mkv")
            ),
            Some(ObsEvent::RecordState {
                active: false,
                started: None,
                saved: Some("/clips/2026-11-11 08-36-45.mkv".into())
            })
        );
        assert_eq!(
            state(true, "OBS_WEBSOCKET_OUTPUT_STARTED", None),
            Some(ObsEvent::RecordState {
                active: true,
                started: None,
                saved: None
            }),
            "an obs-websocket that names no file at the start"
        );
        assert_eq!(
            event(
                "RecordFileChanged",
                json!({ "newOutputPath": "/clips/2026-11-11 09-36-45.mkv" })
            ),
            Some(ObsEvent::RecordFile {
                path: "/clips/2026-11-11 09-36-45.mkv".into()
            })
        );
    }

    #[test]
    fn the_auth_string_matches_the_reference() {
        assert_eq!(
            auth_string(
                "supersecret",
                "UDavZ5kv3o9gEaHQ",
                "l8fovRtsW6AeHVrOhCkPaXKaPQXVCe7RXPJZ3yB4"
            ),
            "6utlAbQbAvgzWCb1dIh3sXcJ1AFFMERRmM54t+0D57w="
        );
    }

    async fn fake_obs(
        password: &'static str,
        salt: &'static str,
        challenge: &'static str,
    ) -> (u16, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let mut seen = Vec::new();
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            ws.send(text(&json!({ "op": 0, "d": { "obsWebSocketVersion": "5.5.2", "rpcVersion": 1, "authentication": { "challenge": challenge, "salt": salt } } }))).await.unwrap();
            let identify: Value =
                serde_json::from_str(ws.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
            assert_eq!(identify["op"], 1);
            assert_eq!(identify["d"]["eventSubscriptions"], SUBSCRIBE);
            if identify["d"]["authentication"].as_str()
                != Some(&auth_string(password, salt, challenge))
            {
                seen.push("bad auth".into());
                let _ = ws.close(None).await;
                return seen;
            }
            ws.send(text(
                &json!({ "op": 2, "d": { "negotiatedRpcVersion": 1 } }),
            ))
            .await
            .unwrap();
            loop {
                let Some(Ok(msg)) = ws.next().await else {
                    break;
                };
                let Ok(t) = msg.to_text() else { continue };
                let req: Value = serde_json::from_str(t).unwrap();
                let kind = req["d"]["requestType"].as_str().unwrap().to_string();
                let id = req["d"]["requestId"].as_str().unwrap().to_string();
                seen.push(kind.clone());
                let data = match kind.as_str() {
                    "GetVersion" => {
                        ws.send(text(&json!({ "op": 5, "d": { "eventType": "ReplayBufferSaved", "eventIntent": 64, "eventData": { "savedReplayPath": "C:/Clips/Replay 2026-10-08 21-14-03.mkv" } } }))).await.unwrap();
                        json!({ "obsVersion": "31.0.0", "obsWebSocketVersion": "5.5.2" })
                    }
                    "GetRecordDirectory" => json!({ "recordDirectory": "C:/Clips" }),
                    "SaveReplayBuffer" => {
                        ws.send(text(&json!({ "op": 7, "d": { "requestType": kind, "requestId": id, "requestStatus": { "result": false, "code": 500, "comment": "the replay buffer is not active" } } }))).await.unwrap();
                        ws.send(text(&json!({ "op": 5, "d": { "eventType": "RecordStateChanged", "eventIntent": 64, "eventData": { "outputActive": false, "outputState": "OBS_WEBSOCKET_OUTPUT_STOPPING", "outputPath": null } } }))).await.unwrap();
                        ws.send(text(&json!({ "op": 5, "d": { "eventType": "RecordStateChanged", "eventIntent": 64, "eventData": { "outputActive": false, "outputState": "OBS_WEBSOCKET_OUTPUT_STOPPED", "outputPath": "C:/Clips/2026-10-08 21-20-00.mkv" } } }))).await.unwrap();
                        let _ = ws.close(None).await;
                        break;
                    }
                    _ => json!({}),
                };
                ws.send(text(&json!({ "op": 7, "d": { "requestType": kind, "requestId": id, "requestStatus": { "result": true, "code": 100 }, "responseData": data } }))).await.unwrap();
                if seen.len() >= 3 {
                    let _ = ws.close(None).await;
                    break;
                }
            }
            seen
        });
        (port, task)
    }

    #[tokio::test]
    async fn the_handshake_the_events_and_the_requests_against_a_fake_obs() {
        let (port, server) = fake_obs("hunter22hunter", "salty", "challenging").await;
        let cfg = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: "hunter22hunter".into(),
        };
        let mut s = Session::connect(&cfg).await.unwrap();
        assert_eq!(s.version.obs_version, "31.0.0");
        assert_eq!(s.version.ws_version, "5.5.2");
        let first = s.next_event().await.unwrap();
        assert_eq!(
            first,
            ObsEvent::ReplayBufferSaved {
                path: "C:/Clips/Replay 2026-10-08 21-14-03.mkv".into()
            },
            "the event that arrived during GetVersion was kept"
        );
        assert_eq!(s.record_directory().await.unwrap(), "C:/Clips");
        let refused = s.save_replay().await.unwrap_err();
        assert!(
            matches!(refused, ObsError::Request(ref m) if m.contains("not active")),
            "{refused}"
        );
        let stopping = s.next_event().await.unwrap();
        assert_eq!(
            stopping,
            ObsEvent::RecordState {
                active: false,
                started: None,
                saved: None
            }
        );
        let stopped = s.next_event().await.unwrap();
        assert!(
            matches!(stopped, ObsEvent::RecordState { active: false, saved: Some(ref path), .. } if path.ends_with("21-20-00.mkv")),
            "obs-websocket 5 names the file in RecordStateChanged once stopped: {stopped:?}"
        );
        let seen = server.await.unwrap();
        assert_eq!(
            seen,
            vec!["GetVersion", "GetRecordDirectory", "SaveReplayBuffer"]
        );
    }

    async fn fake_obs_profile(advanced: bool) -> (u16, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let mut seen = Vec::new();
            let mut profile: std::collections::HashMap<String, String> = [
                ("Output/Mode", if advanced { "Advanced" } else { "Simple" }),
                ("SimpleOutput/FilePath", "C:/Clips"),
                ("SimpleOutput/RecFormat2", "mkv"),
                ("SimpleOutput/RecEncoder", "jim_nvenc"),
                ("SimpleOutput/RecQuality", "Stream"),
                ("SimpleOutput/VBitrate", "16000"),
                ("SimpleOutput/ABitrate", "320"),
                ("SimpleOutput/RecRB", "true"),
                ("SimpleOutput/RecRBTime", "60"),
                ("AdvOut/RecFilePath", "D:/Rec"),
                ("AdvOut/RecFormat2", "hybrid_mp4"),
                ("AdvOut/RecEncoder", "obs_qsv11_v2"),
                ("AdvOut/RecTracks", "3"),
                ("AdvOut/RecRB", "false"),
                ("AdvOut/RecRBTime", "30"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
            let mut replay_active = true;
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            ws.send(text(
                &json!({ "op": 0, "d": { "obsWebSocketVersion": "5.5.2", "rpcVersion": 1 } }),
            ))
            .await
            .unwrap();
            let _identify = ws.next().await.unwrap().unwrap();
            ws.send(text(
                &json!({ "op": 2, "d": { "negotiatedRpcVersion": 1 } }),
            ))
            .await
            .unwrap();
            loop {
                let Some(Ok(msg)) = ws.next().await else {
                    break;
                };
                let Ok(t) = msg.to_text() else { continue };
                let req: Value = serde_json::from_str(t).unwrap();
                let kind = req["d"]["requestType"].as_str().unwrap().to_string();
                let id = req["d"]["requestId"].as_str().unwrap().to_string();
                let rd = &req["d"]["requestData"];
                seen.push(kind.clone());
                let data = match kind.as_str() {
                    "GetVersion" => {
                        json!({ "obsVersion": "31.0.2", "obsWebSocketVersion": "5.5.2" })
                    }
                    "GetProfileList" => {
                        json!({ "currentProfileName": "Gaming", "profiles": ["Untitled", "Gaming"] })
                    }
                    "GetProfileParameter" => {
                        let key = format!(
                            "{}/{}",
                            rd["parameterCategory"].as_str().unwrap(),
                            rd["parameterName"].as_str().unwrap()
                        );
                        json!({ "parameterValue": profile.get(&key).cloned(), "defaultParameterValue": Value::Null })
                    }
                    "SetProfileParameter" => {
                        let key = format!(
                            "{}/{}",
                            rd["parameterCategory"].as_str().unwrap(),
                            rd["parameterName"].as_str().unwrap()
                        );
                        profile.insert(key, rd["parameterValue"].as_str().unwrap().to_string());
                        json!({})
                    }
                    "GetRecordDirectory" => {
                        let key = if profile["Output/Mode"] == "Advanced" {
                            "AdvOut/RecFilePath"
                        } else {
                            "SimpleOutput/FilePath"
                        };
                        json!({ "recordDirectory": profile[key] })
                    }
                    "SetRecordDirectory" => {
                        let key = if profile["Output/Mode"] == "Advanced" {
                            "AdvOut/RecFilePath"
                        } else {
                            "SimpleOutput/FilePath"
                        };
                        profile.insert(
                            key.into(),
                            rd["recordDirectory"].as_str().unwrap().to_string(),
                        );
                        json!({})
                    }
                    "GetRecordStatus" => {
                        json!({ "outputActive": profile.get("Test/Recording").map(|v| v == "true").unwrap_or(false), "outputPaused": false })
                    }
                    "GetReplayBufferStatus" => json!({ "outputActive": replay_active }),
                    "StopReplayBuffer" => {
                        replay_active = false;
                        json!({})
                    }
                    "StartReplayBuffer" => {
                        replay_active = true;
                        json!({})
                    }
                    "GetVideoSettings" => {
                        json!({ "baseWidth": 2560, "baseHeight": 1440, "outputWidth": 1920, "outputHeight": 1080, "fpsNumerator": 60, "fpsDenominator": 1 })
                    }
                    _ => json!({}),
                };
                ws.send(text(&json!({ "op": 7, "d": { "requestType": kind, "requestId": id, "requestStatus": { "result": true, "code": 100 }, "responseData": data } }))).await.unwrap();
            }
            seen
        });
        (port, task)
    }

    #[tokio::test]
    async fn the_state_is_read_from_the_profile_and_changes_are_written_back() {
        let (port, server) = fake_obs_profile(false).await;
        let cfg = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: String::new(),
        };
        let mut s = Session::connect(&cfg).await.unwrap();
        let st = s.state().await.unwrap();
        assert!(!st.advanced);
        assert_eq!(st.profile, "Gaming");
        assert_eq!(st.record_dir, "C:/Clips");
        assert_eq!(
            (st.format.as_str(), st.encoder.as_str()),
            ("mkv", "jim_nvenc")
        );
        assert_eq!(
            (st.video_kbit, st.audio_kbit, st.quality),
            (Some(16000), Some(320), None)
        );
        assert_eq!(st.audio_tracks, vec![1]);
        assert!(st.replay_enabled && st.replay_active && !st.recording);
        assert_eq!(st.replay_seconds, 60);
        assert_eq!(
            (st.base, st.output, st.fps),
            ((2560, 1440), (1920, 1080), 60.0)
        );
        let done = s
            .apply(&ObsChange {
                record_dir: Some("E:/Clips".into()),
                replay_seconds: Some(90),
                format: Some("hybrid_mp4".into()),
                video_kbit: Some(20000),
                replay_enabled: None,
            })
            .await
            .unwrap();
        assert_eq!(done.len(), 5, "{done:?}");
        assert!(done.last().unwrap().contains("restarted"), "{done:?}");
        let st = s.state().await.unwrap();
        assert_eq!(st.record_dir, "E:/Clips");
        assert_eq!(st.replay_seconds, 90);
        assert_eq!(st.format, "hybrid_mp4");
        assert_eq!(st.video_kbit, Some(20000));
        assert!(st.replay_active, "the buffer runs again after the restart");
        let bad = s
            .apply(&ObsChange {
                format: Some("avi".into()),
                ..ObsChange::default()
            })
            .await
            .unwrap_err();
        assert!(matches!(bad, ObsError::Request(ref m) if m.contains("not a format")));
        s.set_profile_parameter("Test", "Recording", "true")
            .await
            .unwrap();
        let busy = s
            .apply(&ObsChange {
                replay_seconds: Some(10),
                ..ObsChange::default()
            })
            .await
            .unwrap_err();
        assert!(
            matches!(busy, ObsError::Busy(ref m) if m.contains("recording")),
            "{busy}"
        );
        drop(s);
        let seen = server.await.unwrap();
        assert!(seen.iter().any(|k| k == "SetRecordDirectory"));
        assert!(seen.iter().filter(|k| *k == "StopReplayBuffer").count() == 1);
    }

    #[tokio::test]
    async fn the_test_reads_what_obs_is_set_to() {
        let (port, _server) = fake_obs_profile(false).await;
        let cfg = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: String::new(),
        };
        let (v, st) = test_connection(&cfg).await.unwrap();
        let st = st.unwrap();
        assert_eq!(v.obs_version, "31.0.2");
        assert_eq!(
            (
                st.profile.as_str(),
                st.record_dir.as_str(),
                st.replay_seconds
            ),
            ("Gaming", "C:/Clips", 60)
        );
        assert!(st.replay_enabled && st.replay_active);
    }

    #[tokio::test]
    async fn advanced_output_mode_keeps_the_bitrate_in_obs() {
        let (port, _server) = fake_obs_profile(true).await;
        let cfg = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: String::new(),
        };
        let mut s = Session::connect(&cfg).await.unwrap();
        let st = s.state().await.unwrap();
        assert!(st.advanced);
        assert_eq!(st.record_dir, "D:/Rec");
        assert_eq!(st.encoder, "obs_qsv11_v2");
        assert_eq!(st.audio_tracks, vec![1, 2]);
        assert_eq!(st.video_kbit, None);
        assert!(!st.replay_enabled);
        let refused = s
            .apply(&ObsChange {
                video_kbit: Some(12000),
                ..ObsChange::default()
            })
            .await
            .unwrap_err();
        assert!(matches!(refused, ObsError::Request(ref m) if m.contains("advanced output")));
        let (_, words) = refused.explain("127.0.0.1", port);
        assert!(words.unwrap().contains("Settings, Output, Recording"));
    }

    #[tokio::test]
    async fn a_wrong_password_and_a_missing_one_are_told_apart_and_test_connection_works() {
        let (port, _server) = fake_obs("right", "s", "c").await;
        let wrong = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: "wrong".into(),
        };
        assert!(matches!(
            Session::connect(&wrong).await,
            Err(ObsError::Auth)
        ));
        let (port, _server) = fake_obs("right", "s", "c").await;
        let none = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: String::new(),
        };
        assert!(matches!(
            Session::connect(&none).await,
            Err(ObsError::NeedsPassword)
        ));
        let (port, _server) = fake_obs("right", "s", "c").await;
        let ok = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: "right".into(),
        };
        let (v, st) = test_connection(&ok).await.unwrap();
        assert_eq!(v.obs_version, "31.0.0");
        assert!(
            st.is_none(),
            "this stand-in hangs up after three requests: the versions still stand"
        );
        let dead = ObsConfig {
            host: "127.0.0.1".into(),
            port: 1,
            password: String::new(),
        };
        assert!(matches!(
            test_connection(&dead).await,
            Err(ObsError::Connect(..))
        ));
    }

    #[tokio::test]
    async fn the_run_loop_reports_the_connection_and_runs_commands() {
        let (port, _server) = fake_obs("pw", "s", "c").await;
        let cfg = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: "pw".into(),
        };
        let (tx, mut rx) = mpsc::channel(16);
        let (ctx, crx) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let c = cancel.clone();
        let task = tokio::spawn(run(cfg, tx, crx, c, None));
        let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(first, ObsEvent::Connected { ref obs_version, .. } if obs_version == "31.0.0")
        );
        let game = rx.recv().await.unwrap();
        assert_eq!(
            game,
            ObsEvent::Game { game: None },
            "no game capture in this OBS"
        );
        let second = rx.recv().await.unwrap();
        assert!(matches!(second, ObsEvent::ReplayBufferSaved { .. }));
        let (reply, got) = oneshot::channel();
        ctx.send(ObsCommand::RecordDirectory(reply)).await.unwrap();
        assert_eq!(got.await.unwrap().unwrap(), "C:/Clips");
        cancel.cancel();
        task.await.unwrap();
    }

    /// An OBS that takes connections and then freezes: with `hello` after its Hello and two
    /// answers (GetVersion, the scene of the game capture); without, before its Hello.
    async fn frozen_obs(hello: bool) -> u16 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                if hello {
                    ws.send(text(&json!({ "op": 0, "d": { "obsWebSocketVersion": "5.5.2", "rpcVersion": 1 } }))).await.unwrap();
                    let _identify = ws.next().await;
                    ws.send(text(
                        &json!({ "op": 2, "d": { "negotiatedRpcVersion": 1 } }),
                    ))
                    .await
                    .unwrap();
                    for _ in 0..2 {
                        let req: Value = serde_json::from_str(
                            ws.next().await.unwrap().unwrap().to_text().unwrap(),
                        )
                        .unwrap();
                        ws.send(text(&json!({ "op": 7, "d": { "requestType": req["d"]["requestType"], "requestId": req["d"]["requestId"], "requestStatus": { "result": true, "code": 100 }, "responseData": { "obsVersion": "33.0.1" } } }))).await.unwrap();
                    }
                }
                held.push(ws);
            }
        });
        port
    }

    #[tokio::test]
    async fn an_obs_frozen_before_its_hello_is_not_waited_for_forever() {
        let port = frozen_obs(false).await;
        let cfg = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: String::new(),
        };
        let t = std::time::Instant::now();
        match Session::connect(&cfg).await {
            Err(ObsError::Connect(_, why)) => assert_eq!(why, "timed out in the handshake"),
            other => panic!("not a handshake that timed out: {:?}", other.err()),
        }
        assert!(t.elapsed() < HANDSHAKE * 2);
    }

    #[tokio::test]
    async fn a_connection_that_stops_answering_ends_and_says_why() {
        let port = frozen_obs(true).await;
        let cfg = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: String::new(),
        };
        let (tx, mut rx) = mpsc::channel(16);
        let (_ctx, crx) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(run(cfg, tx, crx, cancel.clone(), None));
        let mut seen = Vec::new();
        let reason = loop {
            match tokio::time::timeout(QUIET + ANSWER * 6, rx.recv()).await {
                Ok(Some(ObsEvent::Disconnected { reason, .. })) => break reason,
                Ok(Some(e)) => seen.push(e),
                other => panic!("the frozen connection did not end: {other:?}"),
            }
        };
        assert!(matches!(seen[0], ObsEvent::Connected { .. }), "{seen:?}");
        assert_eq!(
            reason,
            format!("no answer to GetVersion in {} s", ANSWER.as_secs()),
            "the quiet connection's question"
        );
        cancel.cancel();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn the_run_loop_waits_for_an_obs_not_asked_to_close() {
        let (port, _server) = fake_obs("pw", "s", "c").await;
        let cfg = ObsConfig {
            host: "127.0.0.1".into(),
            port,
            password: "pw".into(),
        };
        let (tx, mut rx) = mpsc::channel(16);
        let (_ctx, crx) = mpsc::channel(4);
        let cancel = CancellationToken::new();
        let ready = tokio::sync::watch::Sender::new(false);
        let task = tokio::spawn(run(cfg, tx, crx, cancel.clone(), Some(ready.subscribe())));
        assert!(
            tokio::time::timeout(Duration::from_millis(300), rx.recv())
                .await
                .is_err(),
            "nothing while OBS is asked to close"
        );
        ready.send_replace(true);
        let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(first, ObsEvent::Connected { .. }));
        ready.send_replace(false);
        let gone = loop {
            match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
                Ok(Some(ObsEvent::Disconnected { reason, .. })) => break reason,
                Ok(Some(_)) => continue,
                other => panic!("no end of the connection: {other:?}"),
            }
        };
        assert_eq!(gone, "reel asked OBS to close");
        assert!(
            tokio::time::timeout(Duration::from_millis(300), rx.recv())
                .await
                .is_err(),
            "not connected again"
        );
        cancel.cancel();
        task.await.unwrap();
    }

    #[test]
    fn an_obs_the_app_does_not_run_is_waited_for_in_words_that_say_for_what() {
        let refused = ObsError::Connect(
            "ws://127.0.0.1:4455".into(),
            "Connection refused (os error 111)".into(),
        );
        assert_eq!(
            waiting_words(&refused, "127.0.0.1", 4455),
            "waiting for OBS at 127.0.0.1:4455: it is not running, or its WebSocket server is off; reel connects once it answers"
        );
        let silent = ObsError::Connect("ws://pc:4455".into(), "timed out".into());
        assert_eq!(
            waiting_words(&silent, "pc", 4455),
            "waiting for OBS at pc:4455, which does not answer; reel connects once it does"
        );
        assert_eq!(
            waiting_words(&ObsError::Closed("OBS closed the socket".into()), "127.0.0.1", 4455),
            "waiting for OBS at 127.0.0.1:4455 to come back (OBS closed the socket); reel connects once it answers"
        );
        assert_eq!(
            waiting_words(&ObsError::NeedsPassword, "127.0.0.1", 4455),
            "not connected: OBS needs a password and none is set",
            "a password is not waited for"
        );
    }

    #[test]
    fn a_refused_connection_is_explained_in_plain_words() {
        let e = ObsError::Connect(
            "ws://127.0.0.1:4455".into(),
            "IO error: No connection could be made because the target machine actively refused it. (os error 10061)".into(),
        );
        let (words, detail) = e.explain("127.0.0.1", 4455);
        assert!(words.contains("WebSocket Server Settings"), "{words}");
        assert!(words.contains("4455"), "{words}");
        assert!(detail.unwrap().contains("10061"));
        let t = ObsError::Connect("ws://10.0.0.5:4455".into(), "timed out".into());
        assert!(t.explain("10.0.0.5", 4455).0.contains("firewall"));
        assert_eq!(ObsError::Auth.explain("h", 1).1, None);
    }
}
