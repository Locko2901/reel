//! A stand-in OBS for the end to end run; `fail-next`, `write-ms` and `next-clip` steer its saves.
//! Usage: fake_obs --port 4455 --dir <record folder> --clip <file to hand out as the replay>
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

fn send(v: Value) -> Message {
    Message::text(v.to_string())
}

#[tokio::main]
async fn main() {
    let port: u16 = arg("--port").parse().expect("a port");
    let dir = PathBuf::from(arg("--dir"));
    let clip = PathBuf::from(arg("--clip"));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("the port");
    println!("fake obs listening on {port}");
    let mut saved = 0u32;
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else {
            continue;
        };
        let _ = ws
            .send(send(
                json!({ "op": 0, "d": { "obsWebSocketVersion": "5.5.2", "rpcVersion": 1 } }),
            ))
            .await;
        let Some(Ok(_identify)) = ws.next().await else {
            continue;
        };
        let _ = ws
            .send(send(json!({ "op": 2, "d": { "negotiatedRpcVersion": 1 } })))
            .await;
        println!("fake obs: a client identified");
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
                "GetRecordDirectory" => json!({ "recordDirectory": dir.to_string_lossy() }),
                "GetReplayBufferStatus" => json!({ "outputActive": true }),
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
                "GetRecordStatus" => json!({ "outputActive": false }),
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
