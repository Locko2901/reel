//! A stand-in OBS for the end to end run; `fail-next`, `write-ms` and `next-clip` steer its saves.
//! A recording's file is the one `record-clip` names, else the clip, written when it stops;
//! `fail-record` refuses the next start.
//! Usage: fake_obs --port 4455 --dir <record folder> --clip <file to hand out as the replay>
//! [--config <XDG_CONFIG_HOME>] [--password <its WebSocket's>] [--profile <name>]. With --config
//! it logs its encoders as OBS does (`encoders` in the record folder, x264 without), and its
//! replay buffer runs only with an encoder `starts` names (all listed ones without); with one it
//! does not list it never answers, as OBS held by its "Missing Encoders" dialog.
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio_tungstenite::tungstenite::Message;

fn arg(name: &str) -> String {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| panic!("{name} is needed"))
}

fn opt(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn words(path: &std::path::Path) -> Option<Vec<String>> {
    std::fs::read_to_string(path)
        .ok()
        .map(|t| t.split_whitespace().map(String::from).collect())
}

/// The profile's encoder, its log written as OBS writes it; whether its replay buffer runs.
fn start_as_obs(config: &std::path::Path, dir: &std::path::Path) -> bool {
    let has = words(&dir.join("encoders")).unwrap_or_else(|| vec!["obs_x264".into()]);
    let starts = words(&dir.join("starts")).unwrap_or_else(|| has.clone());
    let obs = config.join("obs-studio");
    let profile =
        std::fs::read_to_string(obs.join("basic/profiles/reel/basic.ini")).unwrap_or_default();
    let enc = profile
        .lines()
        .find_map(|l| l.strip_prefix("RecEncoder="))
        .unwrap_or("obs_x264")
        .trim()
        .to_string();
    if !has.contains(&enc) {
        println!("fake obs: no {enc}, held as by OBS's Missing Encoders dialog");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
    let mut log =
        String::from("00:00:00.000: Available Encoders:\n00:00:00.000:   Video Encoders:\n");
    for e in &has {
        log.push_str(&format!("00:00:00.000: \t- {e} ({e})\n"));
    }
    log.push_str("00:00:00.000:   Audio Encoders:\n00:00:00.000: \t- ffmpeg_aac (FFmpeg AAC)\n");
    let runs = starts.contains(&enc);
    if !runs {
        log.push_str(&format!(
            "00:00:00.100: [stand-in encoder: 'advanced_video_recording'] {enc} failed to initialize: no device here\n"
        ));
    }
    let logs = obs.join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let _ = std::fs::write(logs.join(format!("fake {stamp}.txt")), log);
    println!("fake obs: records with {enc}; its replay buffer runs: {runs}");
    runs
}

fn send(v: Value) -> Message {
    Message::text(v.to_string())
}

#[tokio::main]
async fn main() {
    let port: u16 = arg("--port").parse().expect("a port");
    let dir = PathBuf::from(arg("--dir"));
    let clip = PathBuf::from(arg("--clip"));
    let runs = match opt("--config") {
        Some(c) => start_as_obs(std::path::Path::new(&c), &dir),
        None => true,
    };
    let password = opt("--password");
    let profile = opt("--profile").unwrap_or_else(|| "Untitled".into());
    let (salt, challenge) = ("standin-salt", "standin-challenge");
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("the port");
    println!("fake obs listening on {port}");
    let mut saved = 0u32;
    let recording: std::sync::Arc<std::sync::Mutex<Option<(PathBuf, std::time::Instant)>>> =
        Default::default();
    let mut recorded = 0u32;
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
            continue;
        };
        let mut hello =
            json!({ "op": 0, "d": { "obsWebSocketVersion": "5.5.2", "rpcVersion": 1 } });
        if password.is_some() {
            hello["d"]["authentication"] = json!({ "challenge": challenge, "salt": salt });
        }
        let _ = ws.send(send(hello)).await;
        let Some(Ok(identify)) = ws.next().await else {
            continue;
        };
        if let Some(pw) = &password {
            let given = serde_json::from_str::<Value>(identify.to_text().unwrap_or("{}"))
                .ok()
                .and_then(|v| v["d"]["authentication"].as_str().map(String::from));
            if given != Some(reel_desktop_core::obs::auth_string(pw, salt, challenge)) {
                println!("fake obs: a wrong password or none");
                let _ = ws.close(None).await;
                continue;
            }
        }
        let _ = ws
            .send(send(json!({ "op": 2, "d": { "negotiatedRpcVersion": 1 } })))
            .await;
        println!("fake obs: a client identified");
        if runs {
            let _ = ws
                .send(send(json!({ "op": 5, "d": { "eventType": "ReplayBufferStateChanged", "eventIntent": 64, "eventData": { "outputActive": true, "outputState": "OBS_WEBSOCKET_OUTPUT_STARTED" } } })))
                .await;
        }
        let (mut sink, mut ws) = ws.split();
        let (out, mut outgoing) = tokio::sync::mpsc::unbounded_channel::<Message>();
        tokio::spawn(async move {
            while let Some(m) = outgoing.recv().await {
                if sink.send(m).await.is_err() {
                    break;
                }
            }
        });
        while let Some(Ok(msg)) = ws.next().await {
            let Ok(text) = msg.to_text() else { continue };
            let Ok(req) = serde_json::from_str::<Value>(text) else {
                continue;
            };
            let kind = req["d"]["requestType"].as_str().unwrap_or("").to_string();
            let id = req["d"]["requestId"].clone();
            println!("fake obs: {kind}");
            let mut event = None;
            let data = match kind.as_str() {
                "GetVersion" => json!({ "obsVersion": "31.0.0", "obsWebSocketVersion": "5.5.2" }),
                "GetProfileList" => {
                    json!({ "currentProfileName": profile, "profiles": [profile] })
                }
                "GetRecordDirectory" => json!({ "recordDirectory": dir.to_string_lossy() }),
                "GetReplayBufferStatus" => json!({ "outputActive": runs }),
                "GetCurrentProgramScene" => {
                    json!({ "sceneName": "Game", "currentProgramSceneName": "Game" })
                }
                "GetSceneItemList" => json!({ "sceneItems": [
                    { "sourceName": "Mic", "inputKind": "wasapi_input_capture", "sceneItemEnabled": true },
                    { "sourceName": "Game Capture", "inputKind": "game_capture", "sceneItemEnabled": true },
                ] }),
                "GetInputSettings" => json!({ "inputName": "Game Capture", "inputSettings": {
                    "capture_mode": "window",
                    "window": "VALORANT  #3A ranked:UnrealWindow:VALORANT-Win64-Shipping.exe",
                } }),
                "GetRecordStatus" => match recording.lock().unwrap().as_ref() {
                    Some((_, at)) => {
                        json!({ "outputActive": true, "outputDuration": at.elapsed().as_millis() as u64, "outputBytes": 1000 })
                    }
                    None => json!({ "outputActive": false, "outputDuration": 0, "outputBytes": 0 }),
                },
                "StartRecord"
                    if recording.lock().unwrap().is_some() || dir.join("fail-record").exists() =>
                {
                    let _ = std::fs::remove_file(dir.join("fail-record"));
                    let refused = json!({ "op": 7, "d": { "requestType": kind, "requestId": id, "requestStatus": { "result": false, "code": 500, "comment": "The output is already active, or could not start." } } });
                    let _ = out.send(send(refused));
                    continue;
                }
                "StartRecord" => {
                    recorded += 1;
                    let file = dir.join(format!("Session fake {recorded}.mp4"));
                    *recording.lock().unwrap() = Some((file.clone(), std::time::Instant::now()));
                    let state = |active: bool, state: &str, path: Option<&PathBuf>| {
                        send(
                            json!({ "op": 5, "d": { "eventType": "RecordStateChanged", "eventIntent": 64, "eventData": { "outputActive": active, "outputState": state, "outputPath": path.map(|p| p.to_string_lossy()) } } }),
                        )
                    };
                    let _ = out.send(state(false, "OBS_WEBSOCKET_OUTPUT_STARTING", None));
                    let _ = out.send(state(true, "OBS_WEBSOCKET_OUTPUT_STARTED", Some(&file)));
                    println!("fake obs: recording {}", file.display());
                    json!({})
                }
                "StopRecord" => {
                    let Some((file, _)) = recording.lock().unwrap().take() else {
                        let refused = json!({ "op": 7, "d": { "requestType": kind, "requestId": id, "requestStatus": { "result": false, "code": 501, "comment": "The output is not active." } } });
                        let _ = out.send(send(refused));
                        continue;
                    };
                    let part = dir.join(format!(".session {recorded}.part"));
                    let from = std::fs::read_to_string(dir.join("record-clip"))
                        .ok()
                        .map(|t| PathBuf::from(t.trim()))
                        .unwrap_or_else(|| clip.clone());
                    if std::fs::copy(&from, &part).is_ok() {
                        let _ = std::fs::rename(&part, &file);
                    }
                    let state = |active: bool, state: &str, path: Option<&PathBuf>| {
                        send(
                            json!({ "op": 5, "d": { "eventType": "RecordStateChanged", "eventIntent": 64, "eventData": { "outputActive": active, "outputState": state, "outputPath": path.map(|p| p.to_string_lossy()) } } }),
                        )
                    };
                    let _ = out.send(state(false, "OBS_WEBSOCKET_OUTPUT_STOPPING", None));
                    let _ = out.send(state(false, "OBS_WEBSOCKET_OUTPUT_STOPPED", Some(&file)));
                    println!("fake obs: recording stopped, {}", file.display());
                    json!({ "outputPath": file.to_string_lossy() })
                }
                "GetVideoSettings" => {
                    json!({ "baseWidth": 1280, "baseHeight": 720, "outputWidth": 1280, "outputHeight": 720, "fpsNumerator": 30, "fpsDenominator": 1 })
                }
                "GetProfileParameter" => {
                    let value = match req["d"]["requestData"]["parameterName"].as_str() {
                        Some("Mode") => Some("Simple"),
                        Some("RecFormat2") => Some("mp4"),
                        Some("RecEncoder") => Some("x264"),
                        Some("RecQuality") => Some("Stream"),
                        Some("VBitrate") => Some("3000"),
                        Some("ABitrate") => Some("160"),
                        Some("RecTracks") => Some("1"),
                        Some("RecRB") => Some("true"),
                        Some("RecRBTime") => Some("180"),
                        _ => None,
                    };
                    json!({ "parameterValue": value })
                }
                "SaveReplayBuffer" | "StartReplayBuffer" if !runs => {
                    let refused = json!({ "op": 7, "d": { "requestType": kind, "requestId": id, "requestStatus": { "result": false, "code": 501, "comment": "The replay buffer is not active." } } });
                    let _ = out.send(send(refused));
                    continue;
                }
                "SaveReplayBuffer" if dir.join("fail-next").exists() => {
                    let _ = std::fs::remove_file(dir.join("fail-next"));
                    let refused = json!({ "op": 7, "d": { "requestType": kind, "requestId": id, "requestStatus": { "result": false, "code": 604, "comment": "The replay buffer is not active." } } });
                    let _ = out.send(send(refused));
                    println!("fake obs: a save refused");
                    continue;
                }
                "SaveReplayBuffer" => {
                    saved += 1;
                    let ms = std::fs::read_to_string(dir.join("write-ms"))
                        .ok()
                        .and_then(|t| t.trim().parse::<u64>().ok())
                        .unwrap_or(0);
                    let next = dir.join("next-clip");
                    let clip = match std::fs::read_to_string(&next) {
                        Ok(named) => {
                            let mut lines = named.lines().map(str::trim).filter(|l| !l.is_empty());
                            let first = lines.next().unwrap_or_default().to_string();
                            let rest: Vec<&str> = lines.collect();
                            if rest.is_empty() {
                                let _ = std::fs::remove_file(&next);
                            } else {
                                let _ = std::fs::write(&next, rest.join("\n"));
                            }
                            PathBuf::from(first)
                        }
                        Err(_) => clip.clone(),
                    };
                    let (dir, out) = (dir.clone(), out.clone());
                    let write = async move {
                        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                        let file = dir.join(format!("Replay fake {saved}.mp4"));
                        let part = dir.join(format!(".fake {saved}.part"));
                        if std::fs::copy(&clip, &part).is_ok()
                            && std::fs::rename(&part, &file).is_ok()
                        {
                            let _ = out.send(send(json!({ "op": 5, "d": { "eventType": "ReplayBufferSaved", "eventIntent": 64, "eventData": { "savedReplayPath": file.to_string_lossy() } } })));
                            println!("fake obs: replay saved");
                        }
                    };
                    if ms == 0 {
                        event = Some(write);
                    } else {
                        tokio::spawn(write);
                    }
                    json!({})
                }
                _ => json!({}),
            };
            let _ = out.send(send(json!({ "op": 7, "d": { "requestType": kind, "requestId": id, "requestStatus": { "result": true, "code": 100 }, "responseData": data } })));
            if let Some(write) = event {
                write.await;
            }
        }
        println!("fake obs: the client left");
    }
}
