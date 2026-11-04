//! Uploads one file with the app's own uploader and times each part, for checking long clips.
//! Usage: upload_file <server url> <device token file> <file> [chunk MB]
use reel_desktop_core::api::Api;
use reel_desktop_core::uploader::{upload_file, Control, Progress, UploadMeta};
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::{mpsc, watch};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (server, token_file, file) = match &args[..] {
        [s, t, f, ..] => (s.clone(), t.clone(), PathBuf::from(f)),
        _ => {
            eprintln!("usage: upload_file <server url> <device token file> <file> [chunk MB]");
            std::process::exit(2);
        }
    };
    let token = std::fs::read_to_string(&token_file)
        .expect("the token file")
        .trim()
        .to_string();
    let api = Api::new(&server, Some(token)).expect("the server's address");
    let (_pause, paused) = watch::channel(false);
    let mut ctl = Control::new(paused);
    if let Some(mb) = args.get(3).and_then(|m| m.parse::<u64>().ok()) {
        ctl.chunk_bytes = Some(mb * 1024 * 1024);
    }
    let (tx, mut rx) = mpsc::unbounded_channel();
    let started = Instant::now();
    let watcher = tokio::spawn(async move {
        let mut hashed_at = None;
        let mut last = 0u64;
        while let Some(p) = rx.recv().await {
            match p {
                Progress::Hashing { done, total } if done == total => {
                    hashed_at = Some(Instant::now());
                    println!(
                        "hashed {total} bytes in {:.1} s",
                        started.elapsed().as_secs_f64()
                    );
                }
                Progress::Uploading { sent, total }
                    if sent - last > total / 10 || sent == total =>
                {
                    last = sent;
                    let since = hashed_at.unwrap_or(started).elapsed().as_secs_f64();
                    println!("sent {sent} of {total} bytes, {:.1} s", since);
                }
                Progress::Retrying { reason, in_secs } => {
                    println!("retrying in {in_secs} s: {reason}")
                }
                Progress::Failed { reason } => println!("failed: {reason}"),
                _ => {}
            }
        }
    });
    let out = upload_file(&api, &file, UploadMeta::default(), &ctl, tx).await;
    let _ = watcher.await;
    match out {
        Ok(o) => println!(
            "uploaded as clip {} in {:.1} s (deduped: {})",
            o.clip_id,
            started.elapsed().as_secs_f64(),
            o.deduped
        ),
        Err(e) => {
            println!("upload failed: {e}");
            std::process::exit(1);
        }
    }
}
