//! The requests the app sends a running OBS to set the microphone's filters, for a check to
//! send to a real one: each request as two lines, its type, then its data.
//! Usage: mic_filters <off|speex|rnnoise> <gate|no-gate> < GetSourceFilterList's answer
use reel_desktop_core::bundled::{filter_requests, Noise, ObsSetup};
use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let noise: Noise = serde_json::from_value(serde_json::Value::String(args[1].clone()))
        .expect("off, speex or rnnoise");
    let mut answer = String::new();
    std::io::stdin().read_to_string(&mut answer).unwrap();
    let answer: serde_json::Value =
        serde_json::from_str(&answer).expect("GetSourceFilterList's answer");
    let current = answer["filters"].as_array().cloned().unwrap_or_default();
    let setup = ObsSetup {
        mic: Some("default".into()),
        noise,
        gate: args[2] == "gate",
        ..ObsSetup::default()
    };
    for (kind, data) in filter_requests("Microphone", &current, &setup.mic_filters()) {
        println!("{kind}\n{data}");
    }
}
