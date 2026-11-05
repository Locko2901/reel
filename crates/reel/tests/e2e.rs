use reel_core::config::{AuthMode, Config};
use reel_core::events::new_bus;
use reel_media::synth::Synth;
use reel_media::{Ffmpeg, HwBackend, HwStatus};
use reel_pipeline::{Ctx, Layers};
use reel_store::models::Clip;
use reel_store::Store;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn ffmpeg_paths() -> Option<(String, String)> {
    let home = std::env::var("HOME").unwrap_or_default();
    let ff = std::env::var("REEL_FFMPEG").unwrap_or(format!("{home}/.local/ffmpeg/ffmpeg"));
    let fp = std::env::var("REEL_FFPROBE").unwrap_or(format!("{home}/.local/ffmpeg/ffprobe"));
    let runs = |bin: &str| {
        std::process::Command::new(bin)
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    if runs(&ff) && runs(&fp) {
        Some((ff, fp))
    } else {
        None
    }
}

struct Env {
    ctx: Ctx,
    base: String,
    client: reqwest::Client,
    _dir: tempfile::TempDir,
    _server: tokio::task::JoinHandle<()>,
}

impl Env {
    fn incoming(&self) -> PathBuf {
        self.ctx.config().ingest.incoming_dir.clone()
    }
    fn store(&self) -> &Store {
        &self.ctx.store
    }
    async fn synth(&self, name: &str, spec: Synth) -> PathBuf {
        let dir = self.incoming();
        self.synth_at(&dir, name, spec).await
    }
    async fn synth_at(&self, dir: &std::path::Path, name: &str, spec: Synth) -> PathBuf {
        let out = dir.join(name);
        let part = reel_media::ffmpeg::part_path(&out);
        self.ctx
            .ff
            .run(&spec.args(&part), spec.seconds, None)
            .await
            .expect("synthetic clip");
        tokio::fs::rename(&part, &out).await.unwrap();
        out
    }
    async fn wait_ready(&self, timeout: Duration) -> Clip {
        self.wait_clip(|c| c.state == "ready", timeout).await
    }
    async fn wait_clip(&self, pred: impl Fn(&Clip) -> bool, timeout: Duration) -> Clip {
        let timeout = timeout * SLOW;
        let start = Instant::now();
        loop {
            for row in self
                .store()
                .clips("", reel_store::Sort::Newest)
                .await
                .unwrap()
            {
                if pred(&row.clip) {
                    return row.clip;
                }
            }
            assert!(start.elapsed() < timeout, "timed out waiting for a clip");
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
    async fn wait_job_done(&self, job_id: &str, timeout: Duration) -> reel_store::models::Job {
        let timeout = timeout * SLOW;
        let start = Instant::now();
        loop {
            let j = self.store().job(job_id).await.unwrap().expect("job row");
            if j.state == "done" || j.state == "failed" {
                return j;
            }
            assert!(
                start.elapsed() < timeout,
                "timed out waiting for job {job_id}"
            );
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
    async fn wait_no_jobs(&self, timeout: Duration) {
        let timeout = timeout * SLOW;
        let start = Instant::now();
        loop {
            let (q, r) = self.store().job_counts().await.unwrap();
            if q == 0 && r == 0 {
                return;
            }
            assert!(start.elapsed() < timeout, "timed out waiting for the queue");
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
}

/// A wait's limit is stretched by this: on GitHub's four core runners the suite takes twenty
/// minutes, and the longest waits come near their own limits at that pace.
const SLOW: u32 = 3;

async fn start(mode: AuthMode, trusted: Vec<&str>) -> Option<Env> {
    start_with(mode, trusted, |_| {}).await
}

async fn start_with(
    mode: AuthMode,
    trusted: Vec<&str>,
    tweak: impl FnOnce(&mut Config),
) -> Option<Env> {
    start_layered(mode, trusted, tweak, None, true).await
}

async fn start_layered(
    mode: AuthMode,
    trusted: Vec<&str>,
    tweak: impl FnOnce(&mut Config),
    file: Option<toml::Value>,
    set_up: bool,
) -> Option<Env> {
    start_in(
        tempfile::tempdir().unwrap(),
        mode,
        trusted,
        tweak,
        file,
        set_up,
    )
    .await
}

async fn restart(env: Env, tweak: impl FnOnce(&mut Config)) -> Env {
    env.ctx.shutdown.cancel();
    let _ = env._server.await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    start_in(env._dir, AuthMode::Single, vec![], tweak, None, true)
        .await
        .unwrap()
}

/// Points every test server's reads of GitHub at port 1, where nothing listens (bind(0) never
/// hands out a port below 1024): a stray read fails at once instead of spending the 60
/// requests an hour GitHub allows an address without a token. A test that reads a release
/// names a stand-in.
fn no_github() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| std::env::set_var("REEL_GITHUB_API", "http://127.0.0.1:1"));
}

async fn start_in(
    dir: tempfile::TempDir,
    mode: AuthMode,
    trusted: Vec<&str>,
    tweak: impl FnOnce(&mut Config),
    file: Option<toml::Value>,
    set_up: bool,
) -> Option<Env> {
    no_github();
    let Some((ff, fp)) = ffmpeg_paths() else {
        eprintln!("skipping: no ffmpeg (set REEL_FFMPEG and REEL_FFPROBE)");
        return None;
    };
    let mut cfg = Config {
        data_dir: dir.path().join("data"),
        ..Config::default()
    };
    cfg.ingest.incoming_dir = dir.path().join("incoming");
    cfg.ingest.originals_dir = dir.path().join("originals");
    cfg.ingest.poll_seconds = 1;
    cfg.ingest.stable_polls = 1;
    cfg.media.ffmpeg = ff;
    cfg.media.ffprobe = fp;
    cfg.media.hwaccel = reel_core::config::Hwaccel::None;
    cfg.share.embed_mb = 95;
    cfg.share.discord_limits_mb = vec![1, 10];
    cfg.share.discord_limit_mb = 1;
    cfg.pipeline.nice = 0;
    cfg.advanced.update_check = false;
    cfg.device.app_updates = false;
    cfg.auth.mode = mode;
    cfg.auth.trusted_proxies = trusted.into_iter().map(String::from).collect();
    tweak(&mut cfg);
    cfg.ensure_dirs().unwrap();
    std::fs::create_dir_all(&cfg.ingest.incoming_dir).unwrap();
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = held.local_addr().unwrap().port();
    cfg.listen = format!("127.0.0.1:{port}");
    cfg.public_url = format!("http://127.0.0.1:{port}");
    let store = Store::open(&cfg.db_path()).await.unwrap();
    let ffm = Arc::new(Ffmpeg::new(&cfg.media.ffmpeg, &cfg.media.ffprobe, 0, 10));
    let backend = match cfg.media.hwaccel {
        reel_core::config::Hwaccel::Qsv => HwBackend::Qsv,
        reel_core::config::Hwaccel::Vaapi => HwBackend::Vaapi,
        _ => HwBackend::Software,
    };
    let hw = HwStatus {
        backend,
        device: cfg.media.render_device.clone(),
        ffmpeg_version: ffm.version().await,
        notes: vec![],
        decoders: vec![],
        anim_webp: ffm.encoders().await.iter().any(|e| e == "libwebp_anim"),
    };
    let listen = cfg.listen.clone();
    let layers = Layers {
        base: cfg,
        file,
        env: false,
    };
    let ctx = Ctx::new(layers, store, ffm, hw, new_bus())
        .await
        .expect("a valid test config");
    reel_pipeline::spawn_all(ctx.clone());
    if set_up {
        ctx.store
            .set_setting("setup_done", "by the test harness")
            .await
            .unwrap();
    }
    let state = reel_server::AppState::new(ctx.clone()).await.unwrap();
    held.set_nonblocking(true).unwrap();
    let listener = tokio::net::TcpListener::from_std(held).unwrap();
    let _ = listen;
    let (addr, server) =
        reel_server::serve_listener(state, listener, ctx.shutdown.clone()).unwrap();
    let base = format!("http://{addr}");
    Some(Env {
        ctx,
        base,
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
        _dir: dir,
        _server: server,
    })
}

fn og(html: &str) -> std::collections::HashMap<String, String> {
    let doc = scraper::Html::parse_document(html);
    let sel = scraper::Selector::parse("meta[property]").unwrap();
    doc.select(&sel)
        .filter_map(|m| {
            Some((
                m.value().attr("property")?.to_string(),
                m.value().attr("content")?.to_string(),
            ))
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn ingest_share_embed_and_ranges() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let now = jiff::Zoned::now();
    let stamp = now.strftime("%Y.%m.%d - %H.%M.%S").to_string();
    env.synth(
        &format!("Overwatch 2 {stamp}.02.DVR.mp4"),
        Synth::h264_1080p(4.0, "ace"),
    )
    .await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    assert_eq!(
        clip.title,
        format!("Overwatch 2, {}", now.strftime("%b %-d, %H:%M")),
        "the game and the moment from a ShadowPlay name"
    );
    assert_eq!(clip.game.as_deref(), Some("Overwatch 2"));
    assert!(clip.recorded_at.is_some(), "recorded_at from the file name");
    assert_eq!(clip.recorded_source.as_deref(), Some("name"));
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        master.video_copied, 1,
        "share friendly H.264 is copied: {:?}",
        clip.plan_reason
    );
    assert_eq!(master.vcodec.as_deref(), Some("h264"));
    assert_eq!(master.acodec.as_deref(), Some("aac"));
    let video = env.ctx.config().abs(master.path.as_deref().unwrap());
    let d = reel_pipeline::derived::paths(&video);
    assert!(d.poster.exists(), "the poster is there at publish");
    env.wait_no_jobs(Duration::from_secs(120)).await;
    assert!(
        d.hover.exists() && d.strip.exists(),
        "the loop and the strip follow in the derive job"
    );
    assert!(
        !env.incoming()
            .join(format!("Overwatch 2 {stamp}.02.DVR.mp4"))
            .exists(),
        "copied raw file is removed"
    );
    let bytes = std::fs::read(&video).unwrap();
    let moov = bytes.windows(4).position(|w| w == b"moov").unwrap();
    let mdat = bytes.windows(4).position(|w| w == b"mdat").unwrap();
    assert!(moov < mdat, "faststart: moov before mdat");
    let s: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/shares", env.base, clip.id))
        .json(&serde_json::json!({ "label": "test", "expires_in": "7d", "allow_download": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = s["token"].as_str().unwrap().to_string();
    assert_eq!(token.len(), 22);
    let page = env
        .client
        .get(format!("{}/s/{token}", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    let html = page.text().await.unwrap();
    let tags = og(&html);
    assert_eq!(tags["og:type"], "video.other");
    assert_eq!(tags["og:video:type"], "video/mp4");
    assert_eq!(tags["og:title"], clip.title);
    assert!(
        tags["og:video"].starts_with(&format!("{}/s/{token}/video.mp4", env.base)),
        "{}",
        tags["og:video"]
    );
    assert_eq!(tags["og:video:secure_url"], tags["og:video"]);
    assert_eq!(tags["og:video:width"], "1920");
    assert_eq!(tags["og:video:height"], "1080");
    assert!(tags["og:image"].starts_with(&format!("{}/s/{token}/poster.jpg", env.base)));
    assert!(html.contains("theme-color"));
    assert!(
        !html.contains("Library"),
        "the share page has no admin chrome"
    );
    assert!(
        html.contains("full quality") && !html.contains("Discord"),
        "one full quality download"
    );
    assert!(
        html.contains("expires in 7 days") || html.contains("expires in 6 days"),
        "the expiry notice"
    );
    assert!(
        html.contains("--stage-tint: rgb("),
        "a flat tint from the poster"
    );
    assert!(
        html.contains("data-fps=\"60\"")
            && html.contains(&format!(r#"src="/assets/{}-"#, reel_core::VERSION))
            && html.contains(r#"/share.js"></script>"#)
            && html.contains(" loop "),
        "the clip player"
    );
    let url = tags["og:video"].clone();
    let full = env.client.get(&url).send().await.unwrap();
    assert_eq!(full.status(), 200);
    assert_eq!(full.headers()["content-type"], "video/mp4");
    assert_eq!(full.headers()["accept-ranges"], "bytes");
    let len: u64 = full.headers()["content-length"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    let r = env
        .client
        .get(&url)
        .header("range", "bytes=0-99")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(
        r.headers()["content-range"].to_str().unwrap(),
        format!("bytes 0-99/{len}")
    );
    assert_eq!(r.bytes().await.unwrap().len(), 100);
    let r = env
        .client
        .get(&url)
        .header("range", format!("bytes={}-", len - 10))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(r.bytes().await.unwrap().len(), 10);
    let r = env
        .client
        .get(&url)
        .header("range", format!("bytes={}-{}", len + 5, len + 50))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 416);
    let h = env.client.head(&url).send().await.unwrap();
    assert_eq!(h.status(), 200);
    assert_eq!(
        h.headers()["content-length"].to_str().unwrap(),
        len.to_string()
    );
    let poster = env.client.get(&tags["og:image"]).send().await.unwrap();
    assert_eq!(poster.status(), 200);
    assert_eq!(poster.headers()["content-type"], "image/jpeg");
    let dl = env
        .client
        .get(format!("{}/s/{token}/download", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(dl.status(), 200);
    assert!(dl.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .starts_with("attachment; filename=\"Overwatch 2"));
    let views = env
        .store()
        .share_by_token(&token)
        .await
        .unwrap()
        .unwrap()
        .views;
    assert_eq!(views, 1);
    let r = env
        .client
        .get(format!("{}/media/clips/{}/current.mp4", env.base, clip.id))
        .header("range", "bytes=10-19")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 206);
    env.client
        .delete(format!(
            "{}/api/shares/{}",
            env.base,
            s["id"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        env.client
            .get(format!("{}/s/{token}", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(env.client.get(&url).send().await.unwrap().status(), 404);
    assert_eq!(
        env.client
            .get(format!("{}/s/{token}/download", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        env.client
            .get(format!("{}/s/not-a-token", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let lib = env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(lib.contains(&clip.title));
    assert!(
        lib.contains("<span>Today</span>"),
        "a day divider when sorted by date"
    );
    assert!(
        lib.contains("Clips ") && !lib.contains("Disk "),
        "the line leads with what the clips take"
    );
    let by_title = env
        .client
        .get(format!("{}/?sort=title", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !by_title.contains("class=\"day\""),
        "no dividers for other sorts"
    );
    let disk: serde_json::Value = env
        .client
        .get(format!("{}/api/disk?refresh=1", env.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        disk["free"].as_u64().unwrap() > 0 && disk["library"].as_u64().unwrap() > 0,
        "{disk}"
    );
    let master_size = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap()
        .size
        .unwrap() as u64;
    assert_eq!(
        disk["clips"].as_u64().unwrap(),
        master_size,
        "the clips total is the sum of the library's versions"
    );
    assert!(disk["line"].as_str().unwrap().starts_with("Clips "));
    assert!(
        disk["detail"].as_str().unwrap().contains("free of"),
        "free space moved into the detail"
    );
    let health: serde_json::Value = env
        .client
        .get(format!("{}/healthz", env.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["ok"], true);
    assert_eq!(health["hwaccel"], "none");
}

#[tokio::test(flavor = "multi_thread")]
async fn hevc_is_reencoded_and_the_original_kept() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("2026-10-06 21-14-03.mkv", Synth::hevc_1440p(3.0, "bridge"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(180)).await;
    assert_eq!(clip.recorded_source.as_deref(), Some("name"));
    assert!(
        clip.recorded_at
            .as_deref()
            .unwrap()
            .starts_with("2026-10-06T"),
        "{:?}",
        clip.recorded_at
    );
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(master.video_copied, 0);
    assert_eq!(master.vcodec.as_deref(), Some("h264"));
    assert_eq!(master.height, Some(1440), "re-encodes keep up to 1440p now");
    assert!(
        clip.plan_reason.as_deref().unwrap().contains("not H.264"),
        "{:?}",
        clip.plan_reason
    );
    let orig = clip
        .original_path
        .clone()
        .expect("the original of a re-encoded clip is kept");
    assert!(
        PathBuf::from(&orig).exists() && orig.ends_with(".mkv"),
        "{orig}"
    );
    assert!(!env.incoming().join("2026-10-06 21-14-03.mkv").exists());
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !page.contains(
            &env.ctx
                .config()
                .ingest
                .originals_dir
                .to_string_lossy()
                .to_string()
        ),
        "no server path on the page"
    );
    assert!(page.contains("kept, ") && page.contains("Download the original, "));
    let raw = env
        .client
        .get(format!("{}/media/clips/{}/original", env.base, clip.id))
        .send()
        .await
        .unwrap();
    assert_eq!(raw.status(), 200);
    assert!(raw.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .contains("raw.mkv"));
    assert_eq!(
        raw.headers()["content-length"]
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap(),
        std::fs::metadata(&orig).unwrap().len()
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn high_bitrate_is_reencoded() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| c.media.copy_max_mbit = 15).await
    else {
        return;
    };
    env.synth(
        "Desktop 2026.10.05 - 23.41.09.mkv",
        Synth::h264_high_bitrate(3.0, "hot"),
    )
    .await;
    let clip = env.wait_ready(Duration::from_secs(120)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(master.video_copied, 0, "{:?}", clip.plan_reason);
    assert!(
        master.vbitrate.unwrap() < 12_000_000,
        "re-encoded near 10 Mbit/s: {:?}",
        master.vbitrate
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn unreadable_file_is_parked_and_not_retried() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    std::fs::write(env.incoming().join("broken.mkv"), b"not a video at all").unwrap();
    let clip = env
        .wait_clip(|c| c.state == "failed", Duration::from_secs(60))
        .await;
    assert!(
        clip.error.as_deref().unwrap().contains("could not read"),
        "{:?}",
        clip.error
    );
    assert!(env.incoming().join("failed").join("broken.mkv").exists());
    assert!(env
        .incoming()
        .join("failed")
        .join("broken.mkv.txt")
        .exists());
    assert!(!env.incoming().join("broken.mkv").exists());
    tokio::time::sleep(Duration::from_secs(3)).await;
    let f = env
        .store()
        .ingest_file("broken.mkv")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(f.state, "failed");
    let (q, r) = env.store().job_counts().await.unwrap();
    assert_eq!((q, r), (0, 0), "no retry loop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_out_of_sight_for_a_poll_is_taken_when_it_is_back() {
    // the watcher's own poll runs once at the start; the test polls by hand
    let Some(env) = start_with(AuthMode::Single, vec![], |c| {
        c.ingest.poll_seconds = 3600;
        c.ingest.stable_polls = 2;
    })
    .await
    else {
        return;
    };
    assert_eq!(
        reel_pipeline::library::check(&env.ctx).await,
        reel_pipeline::library::Status::Ready
    );
    let staged = env._dir.path().join("staged");
    std::fs::create_dir_all(&staged).unwrap();
    let name = "back again.mp4";
    let (away, here) = (staged.join(name), env.incoming().join(name));
    env.synth_at(&staged, name, Synth::small(2.0, "back again"))
        .await;
    let row = || async { env.store().ingest_file(name).await.unwrap().unwrap() };
    let poll = || async { reel_pipeline::watcher::poll_once(&env.ctx).await.unwrap() };
    std::fs::rename(&away, &here).unwrap();
    poll().await;
    assert_eq!(row().await.state, "seen");
    // moved out and back, or a network folder that listed nothing for a moment
    std::fs::rename(&here, &away).unwrap();
    poll().await;
    assert_eq!(row().await.state, "gone");
    std::fs::rename(&away, &here).unwrap();
    for _ in 0..=env.ctx.config().ingest.stable_polls {
        poll().await;
    }
    let f = row().await;
    assert!(
        f.clip_id.is_some(),
        "taken after the stable polls once it is back: {f:?}"
    );
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    assert_eq!(clip.source_name, name);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_synthetic_clips_of_one_spec_are_two_clips() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let staged = env._dir.path().join("staged");
    std::fs::create_dir_all(&staged).unwrap();
    let mut files = Vec::new();
    for name in ["twin 0.mp4", "twin 1.mp4"] {
        let out = staged.join(name);
        let mut args = Synth::small(2.0, "twin").args(&out);
        // one x264 thread makes the same bytes every time, as a busy machine's threads can
        let at = args.len() - 1;
        args.splice(at..at, ["-threads".to_string(), "1".to_string()]);
        env.ctx
            .ff
            .run(&args, 2.0, None)
            .await
            .expect("synthetic clip");
        files.push(out);
    }
    assert!(
        std::fs::read(&files[0]).unwrap() != std::fs::read(&files[1]).unwrap(),
        "two synthetic clips of one spec are two files"
    );
    for f in &files {
        std::fs::rename(f, env.incoming().join(f.file_name().unwrap())).unwrap();
    }
    let mut ids = Vec::new();
    for _ in &files {
        let clip = env
            .wait_clip(
                |c| c.state == "ready" && !ids.contains(&c.id),
                Duration::from_secs(90),
            )
            .await;
        ids.push(clip.id);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn cuts_copy_on_keyframes_reencode_between_and_mute() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("cut me.mp4", Synth::h264_1080p(6.0, "cut")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    assert_eq!(
        clip.recorded_source.as_deref(),
        Some("file time"),
        "no date in the name, no metadata in the mp4: the file time, marked"
    );
    assert!(clip.recorded_at.is_some());
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("file time, the last resort"));
    let master_id = clip.master_id.clone().unwrap();
    let master_path = env.ctx.config().abs(
        env.store()
            .rendition(&master_id)
            .await
            .unwrap()
            .unwrap()
            .path
            .as_deref()
            .unwrap(),
    );
    let master_size = std::fs::metadata(&master_path).unwrap().len();
    let post = |body: serde_json::Value| {
        env.client
            .post(format!("{}/api/clips/{}/cuts", env.base, clip.id))
            .json(&body)
    };
    let d: serde_json::Value = post(serde_json::json!({ "start": 2.0, "end": 5.0 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let j = env
        .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(60))
        .await;
    assert_eq!(j.state, "done", "{:?}", j.error);
    let cut1 = env
        .store()
        .rendition(d["rendition_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cut1.video_copied, 1, "start on a keyframe is a stream copy");
    assert!(
        (cut1.duration.unwrap() - 3.0).abs() < 0.3,
        "{:?}",
        cut1.duration
    );
    let fresh = env.store().clip(&clip.id).await.unwrap().unwrap();
    assert_eq!(
        fresh.current_id.as_deref(),
        Some(cut1.id.as_str()),
        "a cut becomes current"
    );
    let d: serde_json::Value = post(serde_json::json!({ "start": 1.3, "end": 4.0, "mute": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let j = env
        .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(120))
        .await;
    assert_eq!(j.state, "done", "{:?}", j.error);
    let cut2 = env
        .store()
        .rendition(d["rendition_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cut2.video_copied, 0, "between keyframes means a re-encode");
    assert!(cut2.acodec.is_none(), "muted");
    assert_eq!(cut2.seq, 2);
    for (rid, label) in [
        (&master_id, "original"),
        (&cut1.id, "cut 1"),
        (&cut2.id, "cut 2"),
    ] {
        assert_eq!(
            env.client
                .get(format!("{}/media/renditions/{rid}.mp4", env.base))
                .send()
                .await
                .unwrap()
                .status(),
            200,
            "{label} plays on its own"
        );
        let dl = env
            .client
            .get(format!("{}/media/renditions/{rid}/download", env.base))
            .send()
            .await
            .unwrap();
        assert_eq!(dl.status(), 200);
        assert!(
            dl.headers()["content-disposition"]
                .to_str()
                .unwrap()
                .contains(label),
            "{label} downloads on its own"
        );
    }
    let r = env
        .client
        .post(format!("{}/api/clips/{}/current", env.base, clip.id))
        .json(&serde_json::json!({ "rendition_id": master_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let fresh = env.store().clip(&clip.id).await.unwrap().unwrap();
    assert_eq!(
        fresh.current_id.as_deref(),
        Some(master_id.as_str()),
        "back to the original"
    );
    let del = |rid: String| {
        env.client
            .delete(format!("{}/api/renditions/{rid}", env.base))
    };
    assert_eq!(
        del(master_id.clone()).send().await.unwrap().status(),
        409,
        "the original cannot be deleted on its own"
    );
    let s: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/shares", env.base, clip.id))
        .json(&serde_json::json!({ "rendition_id": cut1.id }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        del(cut1.id.clone()).send().await.unwrap().status(),
        409,
        "a pinned link protects its cut"
    );
    env.client
        .delete(format!(
            "{}/api/shares/{}",
            env.base,
            s["id"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(del(cut1.id.clone()).send().await.unwrap().status(), 200);
    assert!(env.store().rendition(&cut1.id).await.unwrap().is_none());
    assert!(
        !env.ctx.config().abs(cut1.path.as_deref().unwrap()).exists(),
        "the cut's file is gone"
    );
    assert_eq!(
        std::fs::metadata(&master_path).unwrap().len(),
        master_size,
        "the original is untouched"
    );
    let r = env
        .client
        .post(format!("{}/api/clips/{}/current", env.base, clip.id))
        .json(&serde_json::json!({ "rendition_id": cut2.id }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        del(cut2.id.clone()).send().await.unwrap().status(),
        409,
        "the current version cannot be deleted"
    );
    let bad = post(serde_json::json!({ "start": 4.0, "end": 2.0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400);
    let r = env
        .client
        .post(format!("{}/api/clips/{}/poster", env.base, clip.id))
        .json(&serde_json::json!({ "time": 1.0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        env.store()
            .clip(&clip.id)
            .await
            .unwrap()
            .unwrap()
            .poster_time,
        Some(1.0)
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn a_cut_can_become_its_own_clip() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("source.mp4", Synth::h264_1080p(6.0, "source"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let d: serde_json::Value = env.client.post(format!("{}/api/clips/{}/cuts", env.base, clip.id)).json(&serde_json::json!({ "start": 2.0, "end": 4.0, "as_new_clip": true, "title": "the good part" })).send().await.unwrap().json().await.unwrap();
    let new_id = d["clip_id"].as_str().expect("a new clip id").to_string();
    let j = env
        .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(60))
        .await;
    assert_eq!(j.state, "done", "{:?}", j.error);
    let new_clip = env.store().clip(&new_id).await.unwrap().unwrap();
    assert_eq!(new_clip.state, "ready");
    assert_eq!(new_clip.title, "the good part");
    assert_eq!(new_clip.source_clip_id.as_deref(), Some(clip.id.as_str()));
    assert_eq!(new_clip.source, "cut");
    let master = env
        .store()
        .rendition(new_clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(master.kind, "master");
    assert_eq!(master.video_copied, 1);
    assert!(
        (master.duration.unwrap() - 2.0).abs() < 0.3,
        "{:?}",
        master.duration
    );
    let source = env.store().clip(&clip.id).await.unwrap().unwrap();
    assert_eq!(
        source.current_id, clip.master_id,
        "the source clip is untouched"
    );
    assert_eq!(
        env.store().renditions_of(&clip.id).await.unwrap().len(),
        1,
        "no new version on the source"
    );
    let page = env
        .client
        .get(format!("{}/clips/{new_id}", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("cut from") && page.contains(&clip.title));
    let source_page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(source_page.contains("saved as its own clip") && source_page.contains("the good part"));
    assert!(env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
        .contains("the good part"));
    env.client
        .delete(format!("{}/api/clips/{new_id}", env.base))
        .send()
        .await
        .unwrap();
    assert!(env
        .ctx
        .config()
        .abs(
            env.store()
                .rendition(clip.master_id.as_deref().unwrap())
                .await
                .unwrap()
                .unwrap()
                .path
                .as_deref()
                .unwrap()
        )
        .exists());
}
#[tokio::test(flavor = "multi_thread")]
async fn shares_follow_current_or_pin_to_a_version() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("pin.mp4", Synth::h264_1080p(6.0, "pin")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let master_id = clip.master_id.clone().unwrap();
    let d: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/cuts", env.base, clip.id))
        .json(&serde_json::json!({ "start": 2.0, "end": 4.0 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    env.wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(60))
        .await;
    let cut = env
        .store()
        .rendition(d["rendition_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    let master = env.store().rendition(&master_id).await.unwrap().unwrap();
    let share = |body: serde_json::Value| {
        env.client
            .post(format!("{}/api/clips/{}/shares", env.base, clip.id))
            .json(&body)
    };
    let follows: serde_json::Value = share(serde_json::json!({ "allow_download": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let pinned: serde_json::Value =
        share(serde_json::json!({ "allow_download": true, "rendition_id": master_id }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    assert_eq!(follows["serves"], "current, now cut 1");
    assert_eq!(pinned["serves"], "pinned to original");
    assert_eq!(
        share(serde_json::json!({ "rendition_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV" }))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let client = &env.client;
    let base = env.base.clone();
    let size_of = |token: &str| {
        let url = format!("{base}/s/{token}/download");
        async move {
            client.get(url).send().await.unwrap().headers()["content-length"]
                .to_str()
                .unwrap()
                .parse::<i64>()
                .unwrap()
        }
    };
    assert_eq!(
        size_of(follows["token"].as_str().unwrap()).await,
        cut.size.unwrap(),
        "a link without a pin follows the current version"
    );
    assert_eq!(
        size_of(pinned["token"].as_str().unwrap()).await,
        master.size.unwrap(),
        "a pinned link serves its version"
    );
    let page = env
        .client
        .get(format!(
            "{}/s/{}",
            env.base,
            pinned["token"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(og(&page)["og:video"].contains(&format!("r={master_id}")));
    let page = env
        .client
        .get(format!(
            "{}/s/{}",
            env.base,
            follows["token"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(og(&page)["og:video"].contains(&format!("r={}", cut.id)));
    env.client
        .post(format!("{}/api/clips/{}/current", env.base, clip.id))
        .json(&serde_json::json!({ "rendition_id": master_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        size_of(follows["token"].as_str().unwrap()).await,
        master.size.unwrap(),
        "the link follows the switch"
    );
    let overview = env
        .client
        .get(format!("{}/shares", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(overview.contains("pinned to original") && overview.contains("current, now original"));
    let clip_page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(clip_page.contains("pinned to original"));
}
#[tokio::test(flavor = "multi_thread")]
async fn the_clip_page_marks_the_links_that_still_play() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("two links.mp4", Synth::small(2.0, "links")).await;
    let clip = env.wait_ready(Duration::from_secs(60)).await;
    let mut ids = Vec::new();
    for expiry in ["never", "1d"] {
        let (s, v) = post_json_body(
            &env,
            &format!("/api/clips/{}/shares", clip.id),
            serde_json::json!({ "expires_in": expiry }),
        )
        .await;
        assert_eq!(s, 200, "{v}");
        ids.push(v["id"].as_str().unwrap().to_string());
    }
    let (live, expired) = (&ids[0], &ids[1]);
    let at = |hours: i64| {
        jiff::Timestamp::now()
            .checked_sub(jiff::SignedDuration::from_hours(hours))
            .unwrap()
            .to_string()
    };
    for (id, made, ends) in [(live, at(48), None), (expired, at(30), Some(at(6)))] {
        sqlx::query("UPDATE share SET created_at = ?, expires_at = ? WHERE id = ?")
            .bind(made)
            .bind(ends)
            .bind(id)
            .execute(env.store().pool())
            .await
            .unwrap();
    }
    let (s, html) = page_text(&env, &format!("/clips/{}", clip.id)).await;
    assert_eq!(s, 200);
    let rows: Vec<&str> = html
        .split(r#"<div class="linkrow" "#)
        .skip(1)
        .map(|r| r.split('>').next().unwrap())
        .collect();
    assert_eq!(
        rows,
        vec![
            format!(r#"data-sid="{expired}""#),
            format!(r#"data-sid="{live}" data-active"#),
        ],
        "the newest row is the expired link, listed but not live"
    );
    let (_, d) = get_json(&env, &format!("/api/clips/{}", clip.id)).await;
    assert!(
        d["link"].as_str().unwrap().ends_with(&format!(
            "/s/{}",
            env.store().share(live).await.unwrap().unwrap().token
        )),
        "the card's menu takes the same link: {d}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn discord_copy_lands_under_the_limit() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("big.mp4", Synth::h264_1080p(5.0, "big")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(
        master.size.unwrap() > 1_000_000,
        "the master is over 1 MB: {:?}",
        master.size
    );
    let url = format!("{}/media/clips/{}/discord.mp4?limit=1", env.base, clip.id);
    let first = env.client.get(&url).send().await.unwrap();
    assert_eq!(first.status(), 202);
    let info: serde_json::Value = first.json().await.unwrap();
    let j = env
        .wait_job_done(info["job_id"].as_str().unwrap(), Duration::from_secs(120))
        .await;
    assert_eq!(j.state, "done", "{:?}", j.error);
    let again = env.client.get(&url).send().await.unwrap();
    assert_eq!(again.status(), 200);
    assert!(again.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .contains("1mb"));
    let bytes = again.bytes().await.unwrap();
    assert!(bytes.len() <= 1_000_000, "{} bytes", bytes.len());
    assert!(bytes.len() > 200_000, "not a stub: {} bytes", bytes.len());
    let bad = env
        .client
        .get(format!(
            "{}/media/clips/{}/discord.mp4?limit=7",
            env.base, clip.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400);
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let s: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/shares", env.base, clip.id))
        .json(&serde_json::json!({ "allow_download": true }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        env.store().active_jobs().await.unwrap().is_empty(),
        "a share starts no encode"
    );
    let token = s["token"].as_str().unwrap();
    assert_eq!(
        env.client
            .get(format!("{}/s/{token}/discord.mp4", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        404,
        "no Discord copy on the public page"
    );
    let page = env
        .client
        .get(format!("{}/s/{token}", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(
        page.matches("class=\"btn\"").count(),
        1,
        "one download only"
    );
    assert!(page.contains("full quality") && !page.contains("Discord"));
    let s2: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/shares", env.base, clip.id))
        .json(&serde_json::json!({ "allow_download": false }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let page2 = env
        .client
        .get(format!("{}/s/{}", env.base, s2["token"].as_str().unwrap()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !page2.contains("Download"),
        "no download button when the link forbids it"
    );
    assert_eq!(
        env.client
            .get(format!(
                "{}/s/{}/download",
                env.base,
                s2["token"].as_str().unwrap()
            ))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn upload_goes_through_the_pipeline() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let tmp = env.incoming().join("..").join("up.mp4");
    let spec = Synth::small(2.0, "up");
    env.ctx.ff.run(&spec.args(&tmp), 2.0, None).await.unwrap();
    let bytes = std::fs::read(&tmp).unwrap();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(bytes)
            .file_name("from the browser.mp4")
            .mime_str("video/mp4")
            .unwrap(),
    );
    let r = env
        .client
        .post(format!("{}/upload", env.base))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "{}", r.text().await.unwrap());
    let clip = env.wait_ready(Duration::from_secs(60)).await;
    assert_eq!(clip.title, "from the browser");
    assert_eq!(clip.source, "upload");
    let bad = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"x".to_vec()).file_name("notes.txt"),
    );
    assert_eq!(
        env.client
            .post(format!("{}/upload", env.base))
            .multipart(bad)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
}

/// No idle connections: a piece answered before its body is read may close a pooled
/// connection, and the next request on it fails with hyper's IncompleteMessage.
fn pieces_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(0)
            .build()
            .unwrap()
    })
}

async fn put_piece(env: &Env, id: &str, offset: usize, bytes: &[u8]) -> (u16, serde_json::Value) {
    let r = pieces_client()
        .put(format!("{}/api/uploads/{id}?offset={offset}", env.base))
        .header("sec-fetch-site", "same-origin")
        .body(bytes.to_vec())
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    (status, r.json().await.unwrap_or(serde_json::Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upload_from_the_browser_goes_in_pieces_and_resumes() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    // no idle connection, as in pieces_client
    let env = Env {
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(0)
            .build()
            .unwrap(),
        ..env
    };
    let tmp = env.incoming().join("..").join("pieces.mp4");
    let spec = Synth::small(2.0, "pieces");
    env.ctx.ff.run(&spec.args(&tmp), 2.0, None).await.unwrap();
    let bytes = std::fs::read(&tmp).unwrap();
    let sha = reel_pipeline::hash::sha256_of_bytes(&bytes);
    let new = serde_json::json!({ "file_name": "from the browser in pieces.mp4", "size": bytes.len(), "file_key": "1760000000000" });
    let (s, v) = post_json_body(&env, "/api/uploads", new.clone()).await;
    assert_eq!(s, 201, "{v}");
    assert!(v["chunk_bytes"].as_u64().unwrap() <= 90 * 1024 * 1024);
    let id = v["upload_id"].as_str().unwrap().to_string();
    let half = bytes.len() / 2;
    let (s, v) = put_piece(&env, &id, 0, &bytes[..half]).await;
    assert_eq!((s, v["offset"].as_u64()), (200, Some(half as u64)), "{v}");
    let (s, v) = put_piece(&env, &id, 0, &bytes[..half]).await;
    assert_eq!((s, v["offset"].as_u64()), (409, Some(half as u64)), "{v}");
    let (s, v) = post_json_body(&env, "/api/uploads", new.clone()).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(
        (
            v["status"].as_str(),
            v["upload_id"].as_str(),
            v["offset"].as_u64()
        ),
        (Some("resumed"), Some(id.as_str()), Some(half as u64)),
        "the same file picked again goes on where it stopped: {v}"
    );
    let (s, v) = put_piece(&env, &id, half, &bytes[half..]).await;
    assert_eq!(
        (s, v["offset"].as_u64()),
        (200, Some(bytes.len() as u64)),
        "{v}"
    );
    let wrong = "0".repeat(64);
    let (s, v) = post_json_body(
        &env,
        &format!("/api/uploads/{id}/finish"),
        serde_json::json!({ "sha256": wrong }),
    )
    .await;
    assert_eq!((s, v["offset"].as_u64()), (422, Some(0)), "{v}");
    let (s, v) = put_piece(&env, &id, 0, &bytes).await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = post_json_body(
        &env,
        &format!("/api/uploads/{id}/finish"),
        serde_json::json!({ "sha256": sha }),
    )
    .await;
    assert_eq!((s, v["status"].as_str()), (200, Some("created")), "{v}");
    let first = v["clip"]["id"].as_str().unwrap().to_string();
    assert!(v["clip"]["job_id"].is_string(), "{v}");
    let clip = env.wait_ready(Duration::from_secs(60)).await;
    assert_eq!(
        (clip.id.as_str(), clip.title.as_str(), clip.source.as_str()),
        (first.as_str(), "from the browser in pieces", "upload")
    );
    let again = serde_json::json!({ "file_name": "the same again.mp4", "size": bytes.len(), "file_key": "other" });
    let (s, v) = post_json_body(&env, "/api/uploads", again).await;
    assert_eq!(s, 201, "{v}");
    let id2 = v["upload_id"].as_str().unwrap().to_string();
    assert_eq!(put_piece(&env, &id2, 0, &bytes).await.0, 200);
    let (s, v) = post_json_body(
        &env,
        &format!("/api/uploads/{id2}/finish"),
        serde_json::json!({ "sha256": sha }),
    )
    .await;
    assert_eq!(
        (s, v["status"].as_str(), v["clip"]["id"].as_str()),
        (200, Some("exists"), Some(first.as_str())),
        "the same file twice stays one clip: {v}"
    );
    let (s, v) = get_json(&env, &format!("/api/uploads/{id2}")).await;
    assert_eq!((s, v["state"].as_str()), (200, Some("done")), "{v}");
    let r = env
        .client
        .post(format!("{}/api/uploads", env.base))
        .header("sec-fetch-site", "cross-site")
        .json(&new)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        403,
        "another site's page cannot start an upload"
    );
    let (s, _) = post_json_body(
        &env,
        "/api/uploads",
        serde_json::json!({ "file_name": "notes.txt", "size": 3 }),
    )
    .await;
    assert_eq!(s, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_phone_installs_reel_and_shares_a_video_to_it() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let r = env
        .client
        .get(format!("{}/assets/manifest.webmanifest", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(r.headers()["content-type"]
        .to_str()
        .unwrap()
        .contains("manifest+json"));
    let m: serde_json::Value = r.json().await.unwrap();
    assert_eq!(m["display"], "standalone");
    assert_eq!(m["share_target"]["action"], "/share-target");
    assert_eq!(m["share_target"]["method"], "POST");
    assert_eq!(m["share_target"]["enctype"], "multipart/form-data");
    assert_eq!(m["share_target"]["params"]["files"][0]["name"], "clip");
    for icon in m["icons"].as_array().unwrap() {
        let path = icon["src"].as_str().unwrap();
        let r = env
            .client
            .get(format!("{}{path}", env.base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "{path}");
        assert_eq!(r.headers()["content-type"], "image/png");
    }
    let sw = env
        .client
        .get(format!("{}/assets/sw.js", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(sw.headers()["service-worker-allowed"], "/");
    let page = env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains(r#"<link rel="manifest" href="/assets/manifest.webmanifest">"#));

    let landing = env
        .client
        .get(format!("{}/share-target", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(landing.contains(&format!(r#"data-limit="{}""#, 8u64 << 30)));

    let tmp = env.incoming().join("..").join("phone.mp4");
    let spec = Synth::small(2.0, "phone");
    env.ctx.ff.run(&spec.args(&tmp), 2.0, None).await.unwrap();
    let form = reqwest::multipart::Form::new().text("title", "").part(
        "clip",
        reqwest::multipart::Part::bytes(std::fs::read(&tmp).unwrap())
            .file_name("from my phone.mp4")
            .mime_str("video/mp4")
            .unwrap(),
    );
    let r = env
        .client
        .post(format!("{}/share-target", env.base))
        .header("sec-fetch-site", "none")
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303);
    let to = r.headers()["location"].to_str().unwrap().to_string();
    let clip = env.wait_ready(Duration::from_secs(60)).await;
    assert_eq!(to, format!("/clips/{}", clip.id));
    assert_eq!(clip.source, "upload");
    assert_eq!(clip.title, "from my phone");

    let bad = reqwest::multipart::Form::new().part(
        "clip",
        reqwest::multipart::Part::bytes(b"x".to_vec()).file_name("notes.txt"),
    );
    let r = env
        .client
        .post(format!("{}/share-target", env.base))
        .multipart(bad)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400);
    assert!(r
        .text()
        .await
        .unwrap()
        .contains("notes.txt: not a clip we take"));

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let host = env.base.trim_start_matches("http://");
    let mut s = tokio::net::TcpStream::connect(host).await.unwrap();
    s.write_all(
        format!(
            "POST /share-target HTTP/1.1\r\nHost: {host}\r\nContent-Type: multipart/form-data; boundary=b\r\nContent-Length: {}\r\nConnection: close\r\n\r\n--b\r\n",
            9u64 << 30
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let mut answer = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), s.read_to_end(&mut answer)).await;
    let answer = String::from_utf8_lossy(&answer);
    assert!(answer.starts_with("HTTP/1.1 413"), "{answer}");
    assert!(answer.contains("an upload over 8 GB is refused; the limit is under Settings, Imports"));
    assert_eq!(
        env.store()
            .clips("", reel_store::Sort::Newest)
            .await
            .unwrap()
            .len(),
        1
    );
}

fn video_src(html: &str) -> String {
    let at = html.find(r#"<video id="video""#).expect("a player");
    let tag = &html[at..at + html[at..].find('>').unwrap()];
    let src = &tag[tag.find(r#" src=""#).unwrap() + 6..];
    src[..src.find('"').unwrap()].to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_page_plays_a_sharper_web_copy_and_drops_it_with_the_link() {
    use std::sync::atomic::Ordering;
    let Some(env) = start_with(AuthMode::Single, vec![], |c| {
        c.share.embed_mb = 5;
        c.share.web_mbit = 4;
    })
    .await
    else {
        return;
    };
    env.synth("tall 1440.mp4", Synth::h264_1440p(8.0, "tall"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(180)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(master.height, Some(1440));
    let deadline = Instant::now() + Duration::from_secs(180);
    while reel_pipeline::enqueue::embed_grade(&env.ctx, &master)
        .await
        .unwrap()
        .is_none_or(|e| e.id == master.id)
    {
        assert!(Instant::now() < deadline, "the preview video was not made");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    env.ctx.hold_heavy.store(true, Ordering::SeqCst);
    let (code, share) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", clip.id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(code, 200, "{share}");
    let token = share["token"].as_str().unwrap().to_string();
    let webs = env.store().web_renditions().await.unwrap();
    assert_eq!(webs.len(), 1, "a web copy is queued with the link");
    assert_eq!(webs[0].derived_from.as_deref(), Some(master.id.as_str()));
    let page = |env: &Env| {
        let url = format!("{}/s/{token}", env.base);
        let client = env.client.clone();
        async move { client.get(url).send().await.unwrap().text().await.unwrap() }
    };
    let html = page(&env).await;
    let tags = og(&html);
    let preview_src = video_src(&html);
    assert!(
        preview_src.contains(&format!("/s/{token}/video.mp4")),
        "the preview plays meanwhile: {preview_src}"
    );
    assert!(html.contains(&format!(r#"data-web="/s/{token}/web.json""#)));
    assert!(tags["og:video"].contains("/video.mp4"));
    assert!(
        html.contains("p preview</span>"),
        "the meta line names the preview"
    );
    assert!(
        html.contains("full quality, 1440p, "),
        "the download names the full file's height"
    );
    let (code, state) = get_json(&env, &format!("/s/{token}/web.json")).await;
    assert_eq!((code, state["ready"].as_bool()), (200, Some(false)));
    let r = env
        .client
        .get(format!("{}/s/{token}/web.mp4", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);

    env.ctx.hold_heavy.store(false, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(240);
    let web = loop {
        let w = env.store().web_renditions().await.unwrap().remove(0);
        if w.is_ready() {
            break w;
        }
        assert_ne!(w.state, "failed", "{:?}", w.error);
        assert!(Instant::now() < deadline, "the web copy was not made");
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    let rate = web.size.unwrap() as f64 * 8.0 / web.duration.unwrap();
    assert_eq!(web.height, Some(1440), "the version's own height");
    assert!(
        (3_000_000.0..4_200_000.0).contains(&rate),
        "near 4 Mbit/s, not over it: {rate}"
    );
    assert_eq!(
        (web.vcodec.as_deref(), web.acodec.as_deref()),
        (Some("h264"), Some("aac"))
    );
    let file = env.ctx.config().abs(web.path.as_deref().unwrap());
    assert!(file.starts_with(env.ctx.config().web_cache_dir()));

    let html = page(&env).await;
    assert_eq!(
        video_src(&html),
        format!("/s/{token}/web.mp4?r={}", web.id),
        "then the web copy plays"
    );
    assert!(!html.contains("data-web="));
    assert!(
        og(&html)["og:video"].contains("/video.mp4"),
        "og:video stays the preview"
    );
    assert!(html.contains("<span>1440p</span>"));
    let (_, state) = get_json(&env, &format!("/s/{token}/web.json")).await;
    assert_eq!(
        (state["ready"].as_bool(), state["height"].as_i64()),
        (Some(true), Some(1440))
    );
    let r = env
        .client
        .get(format!("{}/s/{token}/web.mp4", env.base))
        .header("range", "bytes=0-99")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 206);
    assert_eq!(r.bytes().await.unwrap().len(), 100);

    let r = env
        .client
        .delete(format!(
            "{}/api/shares/{}",
            env.base,
            share["id"].as_str().unwrap()
        ))
        .header("sec-fetch-site", "same-origin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !env.store().web_renditions().await.unwrap().is_empty() {
        assert!(Instant::now() < deadline, "the web copy outlived its link");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(!file.exists(), "and its file went with it");

    let (code, set) = post_json_body(
        &env,
        "/api/collections",
        serde_json::json!({ "name": "Best of" }),
    )
    .await;
    assert_eq!(code, 200);
    let set = set["id"].as_str().unwrap().to_string();
    let r = env
        .client
        .put(format!(
            "{}/api/collections/{set}/clips/{}",
            env.base, clip.id
        ))
        .header("sec-fetch-site", "same-origin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let (code, _) = post_json_body(
        &env,
        &format!("/api/collections/{set}/link"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(code, 200);
    assert_eq!(
        env.store().web_renditions().await.unwrap().len(),
        1,
        "a collection's link queues the web copies of its clips"
    );

    env.synth("small.mp4", Synth::small(3.0, "small")).await;
    let small = env
        .wait_clip(
            |c| c.state == "ready" && c.id != clip.id,
            Duration::from_secs(120),
        )
        .await;
    let (code, share) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", small.id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(code, 200);
    let token = share["token"].as_str().unwrap();
    assert_eq!(
        env.store().web_renditions().await.unwrap().len(),
        1,
        "a small version makes no web copy"
    );
    let html = env
        .client
        .get(format!("{}/s/{token}", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(
        video_src(&html),
        format!(
            "/s/{token}/web.mp4?r={}",
            small.current_id.as_deref().unwrap()
        ),
        "it is its own web copy"
    );
    let r = env
        .client
        .get(format!("{}/s/{token}/web.mp4", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_app_that_is_no_game_names_no_game_and_the_old_ones_are_cleared_once() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth(
        "Zen 2026.10.10 - 21.51.00.DVR.mp4",
        Synth::small(2.0, "zen"),
    )
    .await;
    let clip = env.wait_ready(Duration::from_secs(60)).await;
    assert_eq!(clip.game, None, "a browser's name is no game");
    assert!(
        clip.title.starts_with("Oct 10, ") && !clip.title.contains("Zen"),
        "nor leads the title: {}",
        clip.title
    );
    for game in ["Zen", "Valorant", "Visual Studio Code"] {
        let c = Clip::new(
            reel_core::ids::new_id(),
            format!("{game} clip"),
            "upload",
            format!("{game}.mp4"),
            None,
        );
        env.store().insert_clip(&c).await.unwrap();
        env.store().set_clip_game(&c.id, Some(game)).await.unwrap();
    }
    env.store()
        .delete_setting("not_games_cleared")
        .await
        .unwrap();
    let cleared = reel_pipeline::housekeeping::clear_not_games_once(&env.ctx)
        .await
        .unwrap();
    assert_eq!(cleared, 2, "Zen and Visual Studio Code, not Valorant");
    let games: Vec<_> = env
        .store()
        .games()
        .await
        .unwrap()
        .into_iter()
        .filter_map(|(g, _)| g)
        .collect();
    assert_eq!(games, vec!["Valorant".to_string()]);
    env.store()
        .set_clip_game(&clip.id, Some("Zen"))
        .await
        .unwrap();
    assert_eq!(
        reel_pipeline::housekeeping::clear_not_games_once(&env.ctx)
            .await
            .unwrap(),
        0,
        "once"
    );

    let at = "2026-10-10T19:51:00Z";
    let moment = reel_core::format::title_time(at);
    let other = reel_core::format::title_time("2026-10-10T19:52:00Z");
    let mut ids = Vec::new();
    for title in [
        format!("Zen, {moment}"),
        format!("Zen, {moment}, the clutch"),
        format!("Valorant, {moment}"),
        format!("Zen, {other}"),
    ] {
        let c = Clip::new(
            reel_core::ids::new_id(),
            title,
            "upload",
            "zen.mp4".into(),
            Some(at.into()),
        );
        env.store().insert_clip(&c).await.unwrap();
        ids.push(c.id);
    }
    env.store()
        .delete_setting("not_game_titles_remade")
        .await
        .unwrap();
    let remade = reel_pipeline::housekeeping::remake_not_game_titles_once(&env.ctx)
        .await
        .unwrap();
    assert_eq!(remade, 1, "only the title made from Zen and its own moment");
    let titles: Vec<String> = titles_of(&env, &ids).await;
    assert_eq!(
        titles,
        vec![
            moment.clone(),
            format!("Zen, {moment}, the clutch"),
            format!("Valorant, {moment}"),
            format!("Zen, {other}"),
        ]
    );
    env.store()
        .update_clip_text(&ids[1], Some(&format!("Zen, {moment}")), None)
        .await
        .unwrap();
    assert_eq!(
        reel_pipeline::housekeeping::remake_not_game_titles_once(&env.ctx)
            .await
            .unwrap(),
        0,
        "once"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_title_made_in_utc_is_made_again_in_the_pages_zone_once() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let at = "2026-07-14T19:59:00Z";
    let t: jiff::Timestamp = at.parse().unwrap();
    let berlin = jiff::tz::TimeZone::get("Europe/Berlin").unwrap();
    let year = reel_core::names::this_year();
    let utc = reel_core::names::title_at_in(None, t, &jiff::tz::TimeZone::UTC, year);
    let local = reel_core::names::title_at_in(None, t, &berlin, year);
    assert_ne!(utc, local);
    let cases: [(String, Option<&str>, &str, String); 6] = [
        (
            format!("Ember Pass, {utc}"),
            Some("Ember Pass"),
            "clip.mp4",
            format!("Ember Pass, {local}"),
        ),
        (utc.clone(), None, "clip.mp4", local.clone()),
        (
            format!("Ember Pass, {utc}"),
            None,
            "MedalTVEmberPass20260714195900.mp4",
            format!("Ember Pass, {local}"),
        ),
        (
            format!("Ember Pass, {utc}, the clutch"),
            Some("Ember Pass"),
            "clip.mp4",
            format!("Ember Pass, {utc}, the clutch"),
        ),
        (
            format!("Best of the night, {utc}"),
            Some("Ember Pass"),
            "clip.mp4",
            format!("Best of the night, {utc}"),
        ),
        (
            format!("Ember Pass, {local}"),
            Some("Ember Pass"),
            "clip.mp4",
            format!("Ember Pass, {local}"),
        ),
    ];
    let mut ids = Vec::new();
    for (title, game, name, _) in &cases {
        let c = Clip::new(
            reel_core::ids::new_id(),
            title.clone(),
            "upload",
            name.to_string(),
            Some(at.into()),
        );
        env.store().insert_clip(&c).await.unwrap();
        env.store().set_clip_game(&c.id, *game).await.unwrap();
        ids.push(c.id);
    }
    env.store()
        .delete_setting("utc_titles_remade")
        .await
        .unwrap();
    let remade = reel_pipeline::housekeeping::remake_utc_titles_once_in(&env.ctx, &berlin)
        .await
        .unwrap();
    assert_eq!(remade, 3, "the three titles made in UTC, no other");
    let want: Vec<String> = cases.iter().map(|c| c.3.clone()).collect();
    assert_eq!(titles_of(&env, &ids).await, want);
    env.store()
        .update_clip_text(&ids[3], Some(&format!("Ember Pass, {utc}")), None)
        .await
        .unwrap();
    assert_eq!(
        reel_pipeline::housekeeping::remake_utc_titles_once_in(&env.ctx, &berlin)
            .await
            .unwrap(),
        0,
        "once"
    );
}

async fn titles_of(env: &Env, ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        out.push(env.store().clip(id).await.unwrap().unwrap().title);
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn the_clip_page_goes_back_to_the_page_it_was_opened_from() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth(
        "VRChat 2026.10.01 - 20.00.00.DVR.mp4",
        Synth::small(2.0, "back"),
    )
    .await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let set = env
        .store()
        .collections()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.collection.game.as_deref() == Some("vrchat"))
        .expect("the game's collection")
        .collection
        .id;
    let back = |referer: Option<String>| {
        let env = &env;
        let id = clip.id.clone();
        async move {
            let mut r = env.client.get(format!("{}/clips/{id}", env.base));
            if let Some(from) = referer {
                r = r.header("referer", from);
            }
            let page = r.send().await.unwrap().text().await.unwrap();
            let at = page.find(r#"id="back""#).expect("a back button");
            let tag = &page[at..at + page[at..].find("</a>").unwrap()];
            let href = tag
                .split(r#"href=""#)
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_string();
            let label = tag
                .rsplit("<span>")
                .next()
                .unwrap()
                .trim_end_matches("</span>")
                .to_string();
            (href, label, tag.contains("data-history"))
        }
    };
    assert_eq!(
        back(Some(format!("{}/?q=vr&game=vrchat", env.base))).await,
        ("/?q=vr&#38;game=vrchat".into(), "Library".into(), true),
        "the library with its search and pick, through the history"
    );
    assert_eq!(
        back(Some(format!("{}/collections/{set}", env.base))).await,
        (format!("/collections/{set}"), "VRChat".into(), true),
        "a collection by its name"
    );
    assert_eq!(
        back(Some(format!("{}/trash", env.base))).await,
        ("/trash".into(), "Trash".into(), true)
    );
    for elsewhere in [
        None,
        Some("https://example.com/collections/x".to_string()),
        Some(format!("{}/signin?next=/", env.base)),
    ] {
        assert_eq!(
            back(elsewhere.clone()).await,
            ("/".into(), "Library".into(), false),
            "from {elsewhere:?}, the library and no history"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clip_makes_an_animated_gif_and_webp_once() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("short.mp4", Synth::small(3.0, "short")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(90)).await;
    let get = |path: String| {
        let env = &env;
        async move {
            env.client
                .get(format!("{}{path}", env.base))
                .send()
                .await
                .unwrap()
        }
    };
    let gif = format!("/media/clips/{}/anim.gif?w=480&fps=12", clip.id);
    let r = get(gif.clone()).await;
    assert_eq!(r.status(), 202, "made first, as a job");
    let d: serde_json::Value = r.json().await.unwrap();
    let job = env
        .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(90))
        .await;
    assert_eq!(job.kind, "anim");
    assert_eq!(job.priority, reel_store::models::PRIORITY_ANIM);
    let r = get(gif.clone()).await;
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-type"], "image/gif");
    assert!(r.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .ends_with(".gif\""));
    let bytes = r.bytes().await.unwrap();
    assert!(bytes.starts_with(b"GIF89a"), "a GIF");
    assert_eq!(&bytes[6..8], &480u16.to_le_bytes(), "480 px wide");
    let jobs_before = env.store().jobs_of_clip(&clip.id).await.unwrap().len();
    assert_eq!(get(gif.clone()).await.status(), 200);
    assert_eq!(
        env.store().jobs_of_clip(&clip.id).await.unwrap().len(),
        jobs_before,
        "made once and kept"
    );

    let webp = format!("/media/clips/{}/anim.webp?w=720&fps=24", clip.id);
    if env.ctx.hw.read().await.anim_webp {
        let d: serde_json::Value = get(webp.clone()).await.json().await.unwrap();
        env.wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(90))
            .await;
        let r = get(webp).await;
        assert_eq!(r.headers()["content-type"], "image/webp");
        let bytes = r.bytes().await.unwrap();
        assert!(bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP");
    } else {
        assert_eq!(
            get(webp).await.status(),
            400,
            "no WebP where ffmpeg has none"
        );
    }
    assert_eq!(
        get(format!("/media/clips/{}/anim.gif?w=1080", clip.id))
            .await
            .status(),
        400
    );
    let (_, page) = text_of(&env, &format!("/clips/{}", clip.id)).await;
    assert!(page.contains(r#"id="anim-format""#) && page.contains(r#""gif-480-12":["#));

    let (gone, _) = reel_pipeline::housekeeping::trim_discord_copies(&env.ctx, 0)
        .await
        .unwrap();
    assert!(gone >= 1, "kept and cleared with the copies under a size");
    assert_eq!(get(gif).await.status(), 202, "and made again on demand");

    env.synth("long.mp4", Synth::small(17.0, "long")).await;
    let long = env
        .wait_clip(
            |c| c.state == "ready" && c.source_name.starts_with("long"),
            Duration::from_secs(120),
        )
        .await;
    let r = get(format!("/media/clips/{}/anim.gif", long.id)).await;
    assert_eq!(r.status(), 400);
    assert!(r
        .text()
        .await
        .unwrap()
        .contains("15 s at most: trim it in the editor first"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_clip_page_lists_the_animated_pictures_made_of_it() {
    use std::sync::atomic::Ordering;
    let Some(env) = start(AuthMode::Local, vec![]).await else {
        return;
    };
    let code = setup_code(&env).await;
    let robin = cookie_of(&first_admin(&env, "robin", &code).await);
    let r = env
        .client
        .post(format!("{}/api/users", env.base))
        .header("cookie", &robin)
        .json(&serde_json::json!({ "name": "mira", "password": "mira has a password", "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let mira = cookie_of(
        &env.client
            .post(format!("{}/signin", env.base))
            .form(&[("name", "mira"), ("password", "mira has a password")])
            .send()
            .await
            .unwrap(),
    );
    env.synth("made.mp4", Synth::small(3.0, "made")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let list = |cookie: String| {
        let env = &env;
        let id = clip.id.clone();
        async move {
            let (s, d) = as_user(
                env,
                reqwest::Method::GET,
                &format!("/api/clips/{id}/anims"),
                &cookie,
            )
            .await;
            assert_eq!(s, 200, "{d}");
            serde_json::from_str::<serde_json::Value>(&d).unwrap()
        }
    };
    let held_until = |job: String, words: &'static str| {
        let env = &env;
        async move {
            let deadline = Instant::now() + Duration::from_secs(60);
            while env
                .store()
                .job(&job)
                .await
                .unwrap()
                .unwrap()
                .route
                .as_deref()
                != Some(words)
            {
                assert!(Instant::now() < deadline, "the picture waits: {words}");
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    };

    let d = list(robin.clone()).await;
    assert_eq!(d["making"], false);
    assert!(
        !d["html"].as_str().unwrap().contains(r#"class="made""#),
        "none yet: {d}"
    );
    let (_, page) = as_user(
        &env,
        reqwest::Method::GET,
        &format!("/clips/{}", clip.id),
        &robin,
    )
    .await;
    assert!(
        page.contains(r#"id="anim-made" hidden>"#),
        "quiet when there are none"
    );

    env.ctx.hold_heavy.store(true, Ordering::SeqCst);
    let (s, d) = as_user(
        &env,
        reqwest::Method::GET,
        &format!("/media/clips/{}/anim.gif?w=480&fps=12", clip.id),
        &robin,
    )
    .await;
    assert_eq!(s, 202, "{d}");
    let job = serde_json::from_str::<serde_json::Value>(&d).unwrap()["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    held_until(job.clone(), "waiting while a game has the screen").await;
    let d = list(robin.clone()).await;
    let html = d["html"].as_str().unwrap();
    assert_eq!(d["making"], true);
    assert!(
        html.contains("data-making")
            && html.contains("<b>GIF</b>")
            && html.contains("480 px, 12 fps, of the original, current")
            && html.contains("waiting while a game has the screen")
            && !html.contains("Download"),
        "the picture being made, with how far it is: {html}"
    );
    let rid = env
        .store()
        .job(&job)
        .await
        .unwrap()
        .unwrap()
        .rendition_id
        .unwrap();
    let (s, d) = as_user(
        &env,
        reqwest::Method::DELETE,
        &format!("/api/renditions/{rid}"),
        &robin,
    )
    .await;
    assert_eq!(s, 409, "one being made is left to its job: {d}");
    env.ctx.hold_heavy.store(false, Ordering::SeqCst);
    env.wait_job_done(&job, Duration::from_secs(90)).await;

    let d = list(robin.clone()).await;
    let html = d["html"].as_str().unwrap();
    assert_eq!(d["making"], false);
    assert!(
        html.contains("480 px, 12 fps, ")
            && html.contains("B, of the original, current")
            && html.contains(&format!(r#"href="/media/renditions/{rid}/download""#))
            && html.contains("anim-delete"),
        "made: its weight, Download and Delete: {html}"
    );
    let html = list(mira.clone()).await["html"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        html.contains("Download") && !html.contains("anim-delete"),
        "a member who may not change the clip downloads it, and deletes nothing: {html}"
    );
    let r = env
        .client
        .get(format!("{}/media/renditions/{rid}/download", env.base))
        .header("cookie", &mira)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-type"], "image/gif");
    let named = r.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        named.ends_with(".gif\"") && !named.contains("original"),
        "{named}"
    );
    assert!(r.bytes().await.unwrap().starts_with(b"GIF89a"));

    let r = env
        .client
        .post(format!("{}/api/clips/{}/cuts", env.base, clip.id))
        .header("cookie", &robin)
        .header("sec-fetch-site", "same-origin")
        .json(&serde_json::json!({ "start": 1.0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let d: serde_json::Value = r.json().await.unwrap();
    env.wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(90))
        .await;
    let html = list(robin.clone()).await["html"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        html.contains(", of the original</span>"),
        "a cut current, the picture is of the original, no longer the current version: {html}"
    );
    let r = env
        .client
        .get(format!("{}/media/renditions/{rid}/download", env.base))
        .header("cookie", &robin)
        .send()
        .await
        .unwrap();
    let named = r.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        named.ends_with(" original.gif\""),
        "named with its version: {named}"
    );

    let (s, _) = as_user(
        &env,
        reqwest::Method::DELETE,
        &format!("/api/renditions/{rid}"),
        &mira,
    )
    .await;
    assert_eq!(s, 403, "only who may change the clip deletes it");
    let file = env.ctx.config().abs(
        env.store()
            .rendition(&rid)
            .await
            .unwrap()
            .unwrap()
            .path
            .as_deref()
            .unwrap(),
    );
    assert!(file.exists());
    let (s, d) = as_user(
        &env,
        reqwest::Method::DELETE,
        &format!("/api/renditions/{rid}"),
        &robin,
    )
    .await;
    assert_eq!(s, 200, "{d}");
    assert!(
        !file.exists() && env.store().rendition(&rid).await.unwrap().is_none(),
        "gone with its file"
    );
    assert!(!list(robin.clone()).await["html"]
        .as_str()
        .unwrap()
        .contains(r#"class="made""#));
    let r = env
        .client
        .get(format!("{}/media/renditions/{rid}/download", env.base))
        .header("cookie", &robin)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn picked_clips_download_as_one_zip_with_their_details() {
    use std::io::Read;
    let Some(env) = start(AuthMode::Local, vec![]).await else {
        return;
    };
    let code = setup_code(&env).await;
    let robin = cookie_of(&first_admin(&env, "robin", &code).await);
    let r = env
        .client
        .post(format!("{}/api/users", env.base))
        .header("cookie", &robin)
        .json(&serde_json::json!({ "name": "mira", "password": "mira has a password", "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let mira = cookie_of(
        &env.client
            .post(format!("{}/signin", env.base))
            .form(&[("name", "mira"), ("password", "mira has a password")])
            .send()
            .await
            .unwrap(),
    );
    let mut ids = Vec::new();
    for n in 0..4 {
        env.synth(&format!("zipped {n}.mp4"), Synth::small(2.0, "zipped"))
            .await;
        let clip = env
            .wait_clip(
                |c| c.state == "ready" && !ids.contains(&c.id),
                Duration::from_secs(90),
            )
            .await;
        ids.push(clip.id);
    }
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let send =
        |method: reqwest::Method, path: String, cookie: &str, body: Option<serde_json::Value>| {
            let mut r = env
                .client
                .request(method, format!("{}{path}", env.base))
                .header("cookie", cookie.to_string())
                .header("sec-fetch-site", "same-origin");
            if let Some(b) = body {
                r = r.json(&b);
            }
            r.send()
        };
    for (id, title) in [(&ids[0], "Same title"), (&ids[2], "same title")] {
        let r = send(
            reqwest::Method::PATCH,
            format!("/api/clips/{id}"),
            &robin,
            Some(serde_json::json!({ "title": title })),
        )
        .await
        .unwrap();
        assert_eq!(r.status(), 200);
    }
    let r = send(
        reqwest::Method::POST,
        format!("/api/clips/{}/cuts", ids[1]),
        &robin,
        Some(serde_json::json!({ "start": 0.5 })),
    )
    .await
    .unwrap();
    let cut: serde_json::Value = r.json().await.unwrap();
    env.wait_job_done(cut["job_id"].as_str().unwrap(), Duration::from_secs(90))
        .await;
    let r = send(
        reqwest::Method::POST,
        format!("/api/clips/{}/shares", ids[1]),
        &robin,
        Some(serde_json::json!({ "label": "for the group" })),
    )
    .await
    .unwrap();
    let link = r.json::<serde_json::Value>().await.unwrap()["url"]
        .as_str()
        .unwrap()
        .to_string();
    let r = send(
        reqwest::Method::DELETE,
        format!("/api/clips/{}", ids[3]),
        &robin,
        None,
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 200, "the fourth in the trash");

    let r = env
        .client
        .post(format!("{}/api/bulk/download", env.base))
        .header("cookie", &mira)
        .header("sec-fetch-site", "cross-site")
        .json(&serde_json::json!({ "ids": ids }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403, "the same-site check");
    let r = send(
        reqwest::Method::POST,
        "/api/bulk/download".into(),
        &mira,
        Some(serde_json::json!({ "ids": ids })),
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 200);
    let asked: serde_json::Value = r.json().await.unwrap();
    assert_eq!(asked["clips"], 3, "{asked}");
    assert_eq!(asked["skipped"][0]["id"], ids[3].as_str());
    assert_eq!(asked["skipped"][0]["why"], "not there any more");
    assert_eq!(asked["choices"]["current"]["files"], 3);
    assert_eq!(asked["choices"]["original"]["files"], 3);
    assert_eq!(asked["choices"]["every"]["files"], 4);
    assert_eq!(asked["choices"]["every"]["over"], false);
    let token = asked["token"].as_str().unwrap().to_string();

    let r = send(
        reqwest::Method::GET,
        format!("/api/bulk/download/{token}?which=every"),
        &robin,
        None,
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 404, "a pick's token is its user's alone");
    let r = send(
        reqwest::Method::GET,
        format!("/api/bulk/download/{token}?which=every"),
        &mira,
        None,
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.headers()["content-type"], "application/zip");
    let length: u64 = r.headers()["content-length"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    let named = r.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        named.starts_with("attachment; filename=\"reel 3 clips ") && named.ends_with(".zip\""),
        "{named}"
    );
    let bytes = r.bytes().await.unwrap();
    assert_eq!(
        bytes.len() as u64,
        length,
        "the length said before the first byte"
    );
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).unwrap();
    let names: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().unwrap().to_string())
        .collect();
    assert_eq!(names[0], "README.txt", "{names:?}");
    let b = env.store().clip(&ids[1]).await.unwrap().unwrap();
    let mut want = vec![
        "README.txt".to_string(),
        "Same title/original.mp4".into(),
        "Same title/clip.json".into(),
        format!("{}/original.mp4", b.title),
        format!("{}/cut 1.mp4", b.title),
        format!("{}/clip.json", b.title),
        "same title (2)/original.mp4".into(),
        "same title (2)/clip.json".into(),
    ];
    want.sort();
    let mut got = names.clone();
    got.sort();
    assert_eq!(
        got, want,
        "a folder per clip, two alike numbered, the trashed one left out"
    );
    let read = |z: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str| {
        let mut f = z.by_name(name).unwrap();
        assert_eq!(f.compression(), zip::CompressionMethod::Stored);
        let mut v = Vec::new();
        f.read_to_end(&mut v).unwrap();
        v
    };
    let readme = String::from_utf8(read(&mut z, "README.txt")).unwrap();
    assert!(
        readme.starts_with("reel: 3 clips, every version of each\nMade "),
        "{readme}"
    );
    assert!(
        readme.contains(" by mira from ") && readme.contains("Left out: 1 not there any more."),
        "{readme}"
    );
    let rs = env.store().renditions_of(&ids[1]).await.unwrap();
    let cut_r = rs.iter().find(|r| r.kind == "cut").unwrap();
    let on_disk = std::fs::read(env.ctx.config().abs(cut_r.path.as_deref().unwrap())).unwrap();
    assert!(
        read(&mut z, &format!("{}/cut 1.mp4", b.title)) == on_disk,
        "the cut's file as it is"
    );
    let json: serde_json::Value =
        serde_json::from_slice(&read(&mut z, &format!("{}/clip.json", b.title))).unwrap();
    assert_eq!(json["title"], b.title.as_str());
    assert_eq!(json["id"], ids[1].as_str());
    assert!(
        json["recorded"].is_string()
            && json["added"].is_string()
            && json.get("game").is_some()
            && json.get("owner").is_some()
            && json["description"].is_string(),
        "{json}"
    );
    let vs = json["versions"].as_array().unwrap();
    assert_eq!(vs.len(), 2);
    assert_eq!(vs[0]["label"], "original");
    assert_eq!(vs[0]["file"], "original.mp4");
    assert_eq!(vs[0]["current"], false);
    assert_eq!(vs[1]["label"], "cut 1");
    assert_eq!(vs[1]["current"], true, "the cut is current");
    assert_eq!(vs[1]["bytes"], on_disk.len() as u64);
    assert!(
        vs[1]["seconds"].as_f64().unwrap() > 1.0
            && vs[1]["width"].as_i64().unwrap() > 0
            && vs[1]["video_codec"] == "h264",
        "{json}"
    );
    assert_eq!(json["links"][0]["url"], link.as_str());
    assert_eq!(json["links"][0]["label"], "for the group");
    assert_eq!(json["links"][0]["plays"], "the current version");

    let r = send(
        reqwest::Method::GET,
        format!("/api/bulk/download/{token}"),
        &mira,
        None,
    )
    .await
    .unwrap();
    let mut z =
        zip::ZipArchive::new(std::io::Cursor::new(r.bytes().await.unwrap().to_vec())).unwrap();
    assert_eq!(z.len(), 7, "the current version by default");
    assert!(
        z.by_name(&format!("{}/cut 1.mp4", b.title)).is_ok()
            && z.by_name(&format!("{}/original.mp4", b.title)).is_err()
    );
    let r = send(
        reqwest::Method::GET,
        format!("/api/bulk/download/{token}?which=original"),
        &mira,
        None,
    )
    .await
    .unwrap();
    let mut z =
        zip::ZipArchive::new(std::io::Cursor::new(r.bytes().await.unwrap().to_vec())).unwrap();
    assert!(
        z.by_name(&format!("{}/original.mp4", b.title)).is_ok()
            && z.by_name(&format!("{}/cut 1.mp4", b.title)).is_err()
    );

    let huge = reel_core::ProbeInfo {
        size: 60_000_000_000,
        duration: cut_r.duration.unwrap_or(1.0),
        width: cut_r.width.unwrap_or(0) as u32,
        height: cut_r.height.unwrap_or(0) as u32,
        vcodec: cut_r.vcodec.clone(),
        acodec: cut_r.acodec.clone(),
        fps: cut_r.fps.unwrap_or(30.0),
        ..Default::default()
    };
    env.store()
        .rendition_ready(&cut_r.id, cut_r.path.as_deref().unwrap(), &huge, false)
        .await
        .unwrap();
    let r = send(
        reqwest::Method::POST,
        "/api/bulk/download".into(),
        &mira,
        Some(serde_json::json!({ "ids": ids })),
    )
    .await
    .unwrap();
    let asked: serde_json::Value = r.json().await.unwrap();
    assert_eq!(asked["choices"]["current"]["over"], true, "{asked}");
    assert_eq!(asked["choices"]["original"]["over"], false);
    assert_eq!(asked["most"], "50.0 GB");
    let token = asked["token"].as_str().unwrap();
    let r = send(
        reqwest::Method::GET,
        format!("/api/bulk/download/{token}"),
        &mira,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        r.status(),
        400,
        "over what one download may weigh: refused before a byte"
    );
    assert!(r
        .text()
        .await
        .unwrap()
        .contains("is over the 50.0 GB one download may weigh"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_montage_part_snaps_to_its_own_versions_keyframes() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let mut ids = Vec::new();
    for n in 0..2 {
        env.synth(&format!("snap {n}.mp4"), Synth::small(6.0, "snap"))
            .await;
        let clip = env
            .wait_clip(
                |c| c.state == "ready" && !ids.contains(&c.id),
                Duration::from_secs(90),
            )
            .await;
        ids.push(clip.id);
    }
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let clip = env.store().clip(&ids[0]).await.unwrap().unwrap();
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let kept = env
        .ctx
        .config()
        .abs(master.path.as_deref().unwrap())
        .with_extension("kf.json");
    let _ = std::fs::remove_file(&kept);
    let (s, d) = get_json(&env, &format!("/api/renditions/{}/keyframes", master.id)).await;
    assert_eq!(s, 200, "{d}");
    let kf: Vec<f64> = serde_json::from_value(d["keyframes"].clone()).unwrap();
    assert_eq!(kf.len(), 3, "a 6 s clip with a keyframe every 2 s: {kf:?}");
    for (k, want) in kf.iter().zip([0.0, 2.0, 4.0]) {
        assert!((k - want).abs() < 0.05, "{kf:?}");
    }
    assert!((d["fps"].as_f64().unwrap() - 30.0).abs() < 0.5, "{d}");
    assert!(
        kept.exists(),
        "kept next to the file, as the clip page keeps them"
    );
    let (_, page) = text_of(&env, &format!("/clips/{}", clip.id)).await;
    let on_page = page
        .split(r#"<script type="application/json" id="kf-json">"#)
        .nth(1)
        .and_then(|s| s.split("</script>").next())
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<f64>>(on_page).unwrap(),
        kf,
        "the same keyframes as the clip page's"
    );

    let (s, d) = post_json_body(
        &env,
        &format!("/api/clips/{}/cuts", clip.id),
        serde_json::json!({ "start": 1.0, "end": 4.0 }),
    )
    .await;
    assert_eq!(s, 200, "{d}");
    let cut = env
        .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(90))
        .await;
    let cut_id = cut.rendition_id.unwrap();
    let (s, d) = get_json(&env, &format!("/api/renditions/{cut_id}/keyframes")).await;
    assert_eq!(s, 200, "{d}");
    let cut_kf: Vec<f64> = serde_json::from_value(d["keyframes"].clone()).unwrap();
    assert!(
        cut_kf.len() == 2 && cut_kf[0].abs() < 0.05 && (cut_kf[1] - 2.0).abs() < 0.05,
        "the 3 s cut's own keyframes, from its start: {cut_kf:?}"
    );
    let cut_r = env.store().rendition(&cut_id).await.unwrap().unwrap();
    assert!(env
        .ctx
        .config()
        .abs(cut_r.path.as_deref().unwrap())
        .with_extension("kf.json")
        .exists());

    let (s, _) = get_json(&env, "/api/renditions/01ZZZZZZZZZZZZZZZZZZZZZZZZ/keyframes").await;
    assert_eq!(s, 404);
    let (_, d) = get_json(
        &env,
        &format!("/media/clips/{}/anim.gif?w=480&fps=12", ids[1]),
    )
    .await;
    let anim = d["rendition_id"].as_str().unwrap().to_string();
    env.wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(90))
        .await;
    let (s, _) = get_json(&env, &format!("/api/renditions/{anim}/keyframes")).await;
    assert_eq!(s, 404, "an animated picture is no version");

    let (s, page) = text_of(&env, &format!("/montage?clips={},{}", ids[0], ids[1])).await;
    assert_eq!(s, 200);
    assert!(
        page.contains(&format!(r#"data-id="{}" data-rid="{cut_id}""#, ids[0])),
        "a part names its version, the cut current"
    );
    assert!(
        page.contains(r#"id="m-snap""#)
            && page.contains(r#"aria-checked="true" id="m-snap""#)
            && page.contains(r#"id="m-keyframes""#),
        "the switch, on, and the ticks' row"
    );
}

async fn ask(
    env: &Env,
    method: &str,
    path: &str,
    body: Option<serde_json::Value>,
) -> (u16, String) {
    let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
    let mut r = env
        .client
        .request(m, format!("{}{path}", env.base))
        .header("sec-fetch-site", "same-origin")
        .header("authorization", "Bearer a-token-of-no-device");
    if let Some(b) = body {
        r = r.json(&b);
    }
    let r = r.send().await.unwrap();
    (r.status().as_u16(), r.text().await.unwrap_or_default())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_standalone_library_offers_nothing_of_a_servers() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| {
        c.make_standalone();
        c.storage.disk_warn_gb = 0;
    })
    .await
    else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let file = env
        .synth_at(
            dir.path(),
            "Overwatch 2 2026.11.02 - 20.00.00.DVR.mp4",
            Synth::small(2.0, "standalone"),
        )
        .await;
    let (made, _) = reel_pipeline::enqueue::create_clip_for_file(
        &env.ctx,
        reel_pipeline::enqueue::NewClip::new(&file, "desktop"),
    )
    .await
    .unwrap();
    let clip = env
        .wait_clip(
            |c| c.id == made.id && c.state == "ready",
            Duration::from_secs(90),
        )
        .await;
    let id = clip.id.clone();
    let (status, set) = post_json_body(
        &env,
        "/api/collections",
        serde_json::json!({ "name": "Best" }),
    )
    .await;
    assert_eq!(status, 200);
    let set = set["id"].as_str().unwrap().to_string();
    let (status, _) = ask(
        &env,
        "PUT",
        &format!("/api/collections/{set}/clips/{id}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "a collection belongs on a PC");

    let refused: Vec<(&str, String, Option<serde_json::Value>)> = vec![
        (
            "POST",
            format!("/api/clips/{id}/shares"),
            Some(serde_json::json!({})),
        ),
        ("GET", format!("/api/clips/{id}/link-wait"), None),
        (
            "DELETE",
            "/api/shares/01M6BCN0TDHAX3FE17KH9KW306".into(),
            None,
        ),
        ("POST", format!("/api/collections/{set}/link"), None),
        ("DELETE", format!("/api/collections/{set}/link"), None),
        (
            "POST",
            "/api/bulk/links".into(),
            Some(serde_json::json!({ "ids": [id] })),
        ),
        ("GET", "/shares".into(), None),
        ("GET", "/s/a-token".into(), None),
        ("GET", "/s/a-token/video.mp4".into(), None),
        ("GET", "/s/a-token/download".into(), None),
        ("GET", "/c/a-token".into(), None),
        ("GET", format!("/c/a-token/{id}"), None),
        ("GET", "/api/users".into(), None),
        (
            "POST",
            "/api/users".into(),
            Some(serde_json::json!({ "name": "mira", "role": "member" })),
        ),
        (
            "PATCH",
            "/api/users/someone".into(),
            Some(serde_json::json!({ "role": "admin" })),
        ),
        ("GET", "/api/signin-attempts".into(), None),
        ("POST", "/api/sessions/signout-all".into(), None),
        ("POST", "/signin/first".into(), None),
        ("GET", "/oidc/start".into(), None),
        ("GET", "/oidc/callback".into(), None),
        ("POST", "/api/devices/pair-code".into(), None),
        ("GET", "/api/devices".into(), None),
        ("DELETE", "/api/devices/a-device".into(), None),
        ("GET", "/app/installer/reel".into(), None),
        (
            "POST",
            "/api/device/pair".into(),
            Some(serde_json::json!({ "code": "abcd-2345", "name": "PC" })),
        ),
        ("GET", "/api/device/whoami".into(), None),
        (
            "POST",
            "/api/device/uploads".into(),
            Some(serde_json::json!({})),
        ),
        (
            "POST",
            "/api/device/share".into(),
            Some(serde_json::json!({})),
        ),
        (
            "GET",
            "/api/device/update?current=1.0.0&target=windows&arch=x86_64".into(),
            None,
        ),
        ("GET", "/setup".into(), None),
        ("GET", "/api/setup/checks".into(), None),
        (
            "POST",
            "/api/setup/mkdir".into(),
            Some(serde_json::json!({ "which": "incoming" })),
        ),
        ("POST", "/api/setup/url-check".into(), None),
        ("POST", "/api/setup/done".into(), None),
        ("POST", "/api/notify/test".into(), None),
        ("POST", "/api/external/test".into(), None),
        ("POST", "/api/external/check".into(), None),
        ("POST", "/api/storage/clear-web".into(), None),
        ("POST", "/api/storage/remake-embeds".into(), None),
        ("POST", "/api/restart".into(), None),
        ("GET", "/api/export/config".into(), None),
        ("GET", "/api/logs".into(), None),
        ("GET", "/api/logs/download".into(), None),
        ("GET", "/share-target".into(), None),
        ("POST", "/share-target".into(), None),
        ("GET", "/settings/access".into(), None),
        ("GET", "/settings/devices".into(), None),
        ("GET", "/settings/notifications".into(), None),
        (
            "PATCH",
            "/api/settings".into(),
            Some(serde_json::json!({ "public_url": "https://clips.example.org" })),
        ),
        (
            "PATCH",
            "/api/settings".into(),
            Some(serde_json::json!({ "storage.cap_gb": 5 })),
        ),
        (
            "PATCH",
            "/api/settings".into(),
            Some(serde_json::json!({ "media.hwaccel": "none" })),
        ),
        (
            "PATCH",
            "/api/settings".into(),
            Some(serde_json::json!({ "notify.kind": "off" })),
        ),
        (
            "PATCH",
            "/api/settings".into(),
            Some(serde_json::json!({ "share.expiry_default": "7d" })),
        ),
    ];
    for (method, path, body) in refused {
        let (status, text) = ask(&env, method, &path, body).await;
        assert_eq!(status, 404, "{method} {path} is a server's: {text}");
        assert!(
            text.contains("Not in standalone"),
            "{method} {path} says why: {text}"
        );
    }
    let (_, text) = ask(
        &env,
        "POST",
        &format!("/api/clips/{id}/shares"),
        Some(serde_json::json!({})),
    )
    .await;
    assert!(text.contains("makes no share links"), "{text}");
    assert_eq!(
        env.store().shares_of(&id).await.unwrap().len(),
        0,
        "no link was made"
    );
    assert!(
        env.store().users().await.unwrap().len() <= 1,
        "no second user"
    );

    for (method, path, body) in [
        (
            "PATCH",
            "/api/settings",
            Some(serde_json::json!({ "share.trash_days": 9 })),
        ),
        ("POST", "/api/storage/clear-discord", None),
        ("GET", "/api/export/database", None),
        ("GET", "/api/disk", None),
    ] {
        let (status, text) = ask(&env, method, path, body).await;
        assert_eq!(status, 200, "{method} {path} belongs on a PC: {text}");
    }
    assert_eq!(env.ctx.config().share.trash_days, 9);

    let marks = [
        "href=\"/shares\"",
        "id=\"new-link\"",
        "id=\"link-dialog\"",
        "share-version",
        "data-act=\"link\"",
        "data-bulk=\"links\"",
        "Shared first",
        "id=\"set-link\"",
        "Most watched links",
        "incoming folder",
        "id=\"app-save-share\"",
        "id=\"app-share\"",
        "Save and share",
        "<dt>Server</dt>",
        "/settings/access",
        "/settings/devices",
        "/settings/notifications",
        "Preview video",
        "the links stop",
        "Sign out",
        "public URL",
        "Public URL",
        "Listen address",
        "data-action=\"/api/restart\"",
        "/api/export/config",
        "/setup",
        "/api/storage/clear-web",
        "/api/storage/remake-embeds",
        "Link previews from external storage",
    ];
    let pages = [
        "/".to_string(),
        format!("/clips/{id}"),
        "/collections".into(),
        format!("/collections/{set}"),
        "/stats".into(),
        "/jobs".into(),
        "/status".into(),
        "/trash".into(),
        "/upload".into(),
        format!("/montage?clips={id}"),
        "/settings".into(),
        "/settings/general".into(),
        "/settings/storage".into(),
        "/settings/ingest".into(),
        "/settings/sharing".into(),
        "/settings/imports".into(),
        "/settings/appearance".into(),
        "/settings/advanced".into(),
    ];
    for page in &pages {
        let (status, html) = ask(&env, "GET", page, None).await;
        assert_eq!(status, 200, "{page}");
        for m in marks {
            assert!(!html.contains(m), "{page} has {m}");
        }
        assert!(
            html.contains("<dt>Library</dt>")
                && html.contains("<b>on this PC</b>")
                && html.contains("class=\"dot ok\" id=\"chip-dot\""),
            "{page}: the Library row, green: {}",
            html.lines()
                .find(|l| l.contains("part-server"))
                .unwrap_or("none")
        );
        assert!(html.contains(&format!(
            "the library is in {}",
            env.ctx.config().library_dir().display()
        )));
    }
    let (_, sharing) = ask(&env, "GET", "/settings/sharing", None).await;
    assert!(sharing.contains("Copies under a size") && sharing.contains("Limit list"));
    let (_, storage) = ask(&env, "GET", "/settings/storage", None).await;
    assert!(
        storage.contains(&format!(
            "The library is in {} on this PC; its cap is under This app, Server.",
            env.ctx.config().library_dir().display()
        )),
        "the Storage line names the library's folder"
    );
    assert!(
        !storage.contains("Where the library lives"),
        "and sends nobody to a setting the app does not have"
    );
    let (_, status_page) = ask(&env, "GET", "/status", None).await;
    for gone in [
        "<dt>incoming</dt>",
        "<dt>access</dt>",
        "<dt>health</dt>",
        "<dt>app update</dt>",
    ] {
        assert!(!status_page.contains(gone), "the Status page has {gone}");
    }
    assert!(
        status_page.contains("on this PC, in"),
        "the library check says where"
    );

    std::fs::remove_dir_all(env.incoming()).unwrap();
    let (_, html) = ask(&env, "GET", "/", None).await;
    assert!(
        html.contains("class=\"dot ok\" id=\"chip-dot\""),
        "no incoming folder is no failing check"
    );
    std::fs::create_dir_all(env.incoming()).unwrap();
    env.synth(
        "Dropped 2026.11.02 - 21.00.00.DVR.mp4",
        Synth::small(1.0, "dropped"),
    )
    .await;
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(
        env.store()
            .clips("", reel_store::Sort::Newest)
            .await
            .unwrap()
            .len(),
        1,
        "nothing watches an incoming folder"
    );
    assert!(
        !env.ctx.config().data_dir.join("app-updates").exists(),
        "no installers fetched for paired apps"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_stats_page_counts_games_months_and_watched_links() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let mut ids = Vec::new();
    for name in [
        "VRChat 2026.09.01 - 20.00.00.DVR.mp4",
        "VRChat 2026.11.02 - 20.00.00.DVR.mp4",
        "Overwatch 2 2026.11.03 - 20.00.00.DVR.mp4",
    ] {
        env.synth(name, Synth::small(2.0, "stats")).await;
        let clip = env
            .wait_clip(
                |c| c.state == "ready" && !ids.contains(&c.id),
                Duration::from_secs(90),
            )
            .await;
        ids.push(clip.id);
    }
    let r = env
        .client
        .post(format!("{}/api/clips/{}/shares", env.base, ids[0]))
        .json(&serde_json::json!({ "label": "for the group" }))
        .send()
        .await
        .unwrap();
    let token = r.json::<serde_json::Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    for _ in 0..3 {
        env.client
            .get(format!("{}/s/{token}", env.base))
            .header(
                "user-agent",
                "Mozilla/5.0 (X11; Linux x86_64) Firefox/140.0",
            )
            .send()
            .await
            .unwrap();
    }
    let (status, page) = text_of(&env, "/stats").await;
    assert_eq!(status, 200);
    assert!(page.contains("3 clips,"), "the totals");
    let vr = page
        .find(r#"<a href="/?game=vrchat">VRChat</a>"#)
        .expect("VRChat's row, opening the library on it");
    assert!(page[vr..].find(r#"data-l="clips">2<"#).is_some());
    assert!(page.contains(r#"<a href="/?game=overwatch%202">Overwatch 2</a>"#));
    assert!(
        page.contains(">Nov 2026<") && page.contains(">Oct 2026<") && page.contains(">Sep 2026<"),
        "every month from the first to the last"
    );
    let views = env.store().most_watched(10).await.unwrap();
    if views.first().is_some_and(|l| l.views > 0) {
        assert!(page.contains("for the group"), "the watched link's row");
    }
    let (_, lib) = text_of(&env, "/").await;
    assert!(lib.contains(r#"href="/stats""#), "the library leads there");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_montage_puts_clips_of_two_sizes_in_one_new_clip() {
    use std::sync::atomic::Ordering;
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let mut quiet = Synth::small(2.0, "quiet");
    quiet.audio = false;
    let mut ids = Vec::new();
    for (name, spec) in [
        (
            "VRChat 2026.10.01 - 20.00.00.DVR.mp4",
            Synth::small(3.0, "small"),
        ),
        ("VRChat 2026.10.02 - 20.00.00.DVR.mp4", quiet),
        (
            "VRChat 2026.10.03 - 20.00.00.DVR.mp4",
            Synth::h264_1080p(2.0, "big"),
        ),
    ] {
        env.synth(name, spec).await;
        let clip = env
            .wait_clip(
                |c| c.state == "ready" && !ids.contains(&c.id),
                Duration::from_secs(120),
            )
            .await;
        ids.push(clip.id);
    }
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let before: Vec<_> = futures::future::join_all(ids.iter().map(|id| env.store().clip(id)))
        .await
        .into_iter()
        .map(|c| c.unwrap().unwrap().current_id)
        .collect();
    let post = |body: serde_json::Value| {
        let env = &env;
        async move {
            let r = env
                .client
                .post(format!("{}/api/montage", env.base))
                .json(&body)
                .send()
                .await
                .unwrap();
            (
                r.status().as_u16(),
                r.json::<serde_json::Value>().await.unwrap(),
            )
        }
    };
    let (status, d) = post(serde_json::json!({ "clips": [ids[0]] })).await;
    assert_eq!(status, 400, "{d}");
    assert!(d["error"].as_str().unwrap().contains("two clips or more"));
    let (status, _) =
        post(serde_json::json!({ "clips": [ids[0], "01HZZZZZZZZZZZZZZZZZZZZZZZ"] })).await;
    assert_eq!(status, 400, "a clip that is not there");
    let elsewhere = env
        .client
        .post(format!("{}/api/montage", env.base))
        .header("sec-fetch-site", "cross-site")
        .json(&serde_json::json!({ "clips": [ids[0], ids[1]] }))
        .send()
        .await
        .unwrap();
    assert_eq!(elsewhere.status().as_u16(), 403, "the same-site check");

    env.ctx.hold_heavy.store(true, Ordering::SeqCst);
    let (status, d) = post(serde_json::json!({ "clips": [ids[0], ids[1]] })).await;
    assert_eq!(status, 200, "{d}");
    let held = d["job_id"].as_str().unwrap().to_string();
    let deadline = Instant::now() + Duration::from_secs(60);
    while env
        .store()
        .job(&held)
        .await
        .unwrap()
        .unwrap()
        .route
        .as_deref()
        != Some("waiting while a game has the screen")
    {
        assert!(
            Instant::now() < deadline,
            "the montage waits while a game has the screen"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let (code, _) = post_json_body(
        &env,
        &format!("/api/jobs/{held}/cancel"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(code, 200);
    let deadline = Instant::now() + Duration::from_secs(30);
    while env.store().job(&held).await.unwrap().unwrap().state != "cancelled" {
        assert!(Instant::now() < deadline, "the montage is cancelled");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(
        env.store()
            .clip(d["clip_id"].as_str().unwrap())
            .await
            .unwrap()
            .is_none(),
        "a cancelled montage leaves no clip behind"
    );
    env.ctx.hold_heavy.store(false, Ordering::SeqCst);

    let order = [ids[2].clone(), ids[0].clone(), ids[1].clone()];
    let (status, d) = post(serde_json::json!({ "clips": order })).await;
    assert_eq!(status, 200, "{d}");
    let job = env
        .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(180))
        .await;
    assert_eq!(job.state, "done", "{job:?}");
    let made = env
        .store()
        .clip(d["clip_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(made.state, "ready");
    assert!(made.title.starts_with("Montage, "), "{}", made.title);
    assert_eq!(
        made.game.as_deref(),
        Some("VRChat"),
        "the game all its clips share"
    );
    assert_eq!(made.source, "montage");
    let master = env
        .store()
        .rendition(made.current_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let file = env.ctx.config().abs(master.path.as_deref().unwrap());
    let probe = env.ctx.ff.probe(&file).await.unwrap();
    assert_eq!(
        (probe.width, probe.height),
        (1920, 1080),
        "the tallest frame of the three"
    );
    assert!(
        (probe.duration - 7.0).abs() < 0.4,
        "hard cuts, nothing lost: {}",
        probe.duration
    );
    assert_eq!(
        probe.acodec.as_deref(),
        Some("aac"),
        "sound, with silence for the quiet clip"
    );
    let after: Vec<_> = futures::future::join_all(ids.iter().map(|id| env.store().clip(id)))
        .await
        .into_iter()
        .map(|c| c.unwrap().unwrap().current_id)
        .collect();
    assert_eq!(before, after, "the sources are untouched");
    let (_, page) = text_of(&env, &format!("/clips/{}", made.id)).await;
    let listed = page.find("a montage of ").expect("its sources on its page");
    let first = page[listed..].find(&format!("/clips/{}", ids[2])).unwrap();
    let last = page[listed..].find(&format!("/clips/{}", ids[1])).unwrap();
    assert!(first < last, "in the montage's order");
    let set = reel_store::models::Collection::new("trip", None);
    env.store().insert_collection(&set).await.unwrap();
    for id in &ids {
        env.store().add_to_collection(&set.id, id).await.unwrap();
    }
    let (_, set_page) = text_of(&env, &format!("/collections/{}", set.id)).await;
    assert!(set_page.contains(&format!(r#"href="/montage?set={}""#, set.id)));
    let (status, montage_page) = text_of(&env, &format!("/montage?set={}", set.id)).await;
    assert_eq!(status, 200);
    assert_eq!(montage_page.matches("class=\"montage-part\"").count(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_montage_editor_trims_fades_frames_and_replaces() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let mut ids = Vec::new();
    for (name, seconds) in [
        ("Valorant 2026.11.01 - 20.00.00.DVR.mp4", 4.0),
        ("Valorant 2026.11.01 - 20.05.00.DVR.mp4", 3.0),
        ("Valorant 2026.11.01 - 20.10.00.DVR.mp4", 3.0),
    ] {
        env.synth(name, Synth::small(seconds, name)).await;
        let clip = env
            .wait_clip(
                |c| c.state == "ready" && !ids.contains(&c.id),
                Duration::from_secs(120),
            )
            .await;
        ids.push(clip.id);
    }
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let post = |body: serde_json::Value| {
        let env = &env;
        async move {
            let r = env
                .client
                .post(format!("{}/api/montage", env.base))
                .json(&body)
                .send()
                .await
                .unwrap();
            (
                r.status().as_u16(),
                r.json::<serde_json::Value>().await.unwrap(),
            )
        }
    };
    let parts = serde_json::json!([
        { "clip": ids[0], "start": 1.0, "end": 3.5 },
        { "clip": ids[1], "volume": 0 },
        { "clip": ids[2], "start": 0.5 },
    ]);
    let (status, d) = post(serde_json::json!({
        "parts": [{ "clip": ids[0], "start": 1.0, "end": 1.2 }, { "clip": ids[1] }],
    }))
    .await;
    assert_eq!(status, 400, "{d}");
    assert!(
        d["error"].as_str().unwrap().contains("half a second"),
        "{d}"
    );
    let (status, d) = post(serde_json::json!({
        "parts": [{ "clip": ids[0] }, { "clip": ids[1], "start": 1.0 }, { "clip": ids[2] }],
        "join": "black", "join_seconds": 2.0,
    }))
    .await;
    assert_eq!(
        status, 400,
        "a clip in the middle shorter than its two fades: {d}"
    );
    assert!(
        d["error"]
            .as_str()
            .unwrap()
            .contains("too short for its fades"),
        "{d}"
    );
    let (status, d) = post(serde_json::json!({ "parts": parts, "join": "spin" })).await;
    assert_eq!(status, 400, "{d}");

    let (status, d) = post(serde_json::json!({
        "parts": parts, "title": "Night one", "join": "fade", "join_seconds": 0.5,
        "shape": "tall", "fill": true, "even_loudness": false,
    }))
    .await;
    assert_eq!(status, 200, "{d}");
    let job = env
        .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(180))
        .await;
    assert_eq!(job.state, "done", "{job:?}");
    let made = env
        .store()
        .clip(d["clip_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(made.title, "Night one");
    let master = env
        .store()
        .rendition(made.current_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let recipe: reel_core::recipe::MontageRecipe =
        serde_json::from_str(master.recipe.as_deref().unwrap()).unwrap();
    assert_eq!(recipe.parts[0].start, Some(1.0));
    assert_eq!(recipe.parts[1].volume, 0.0);
    assert_eq!(recipe.join.kind, reel_core::recipe::JoinKind::Fade);
    assert!(!recipe.even_loudness);
    let file = env.ctx.config().abs(master.path.as_deref().unwrap());
    let probe = env.ctx.ff.probe(&file).await.unwrap();
    assert_eq!((probe.width, probe.height), (360, 640), "tall, filled");
    // 2.5 + 3 + 2.5, less two crossfades of half a second
    assert!(
        (probe.duration - 7.0).abs() < 0.4,
        "the parts, their overlaps taken off: {}",
        probe.duration
    );

    let (status, page) = text_of(&env, &format!("/montage?from={}", made.id)).await;
    assert_eq!(status, 200);
    assert!(page.contains("Edit the montage"));
    assert!(page.contains(r#"data-join="fade""#) && page.contains(r#"data-shape="tall""#));
    assert!(page.contains(r#"data-fill="true""#) && page.contains(r#"data-even="false""#));
    assert!(page.contains(r#"data-start="1""#) && page.contains(r#"data-end="3.5""#));
    assert!(page.contains(&format!(r#"id="m-replace" data-id="{}""#, made.id)));
    let (_, clip_page) = text_of(&env, &format!("/clips/{}", made.id)).await;
    assert!(clip_page.contains(&format!(r#"href="/montage?from={}""#, made.id)));

    let (status, d) =
        post(serde_json::json!({ "clips": [ids[0], ids[1]], "replaces": ids[2] })).await;
    assert_eq!(status, 400, "only a montage is replaced: {d}");
    let (status, d) = post(serde_json::json!({
        "parts": [{ "clip": ids[2] }, { "clip": ids[0] }], "replaces": made.id,
    }))
    .await;
    assert_eq!(status, 200, "{d}");
    assert!(
        env.store()
            .clip(&made.id)
            .await
            .unwrap()
            .is_some_and(|c| c.deleted_at.is_none()),
        "the old montage stays until the new one is made"
    );
    let job = env
        .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(180))
        .await;
    assert_eq!(job.state, "done", "{job:?}");
    let old = env.store().trashed_clip(&made.id).await.unwrap();
    assert!(old.is_some(), "the replaced montage is in the trash");
    assert!(old.unwrap().deleted_by.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cut_is_framed_tall_or_square_and_fades_in_and_out() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth(
        "Valorant 2026.11.02 - 01.00.00.DVR.mp4",
        Synth::small(6.0, "frame"),
    )
    .await;
    let clip = env
        .wait_clip(|c| c.state == "ready", Duration::from_secs(120))
        .await;
    let mut quiet = Synth::small(4.0, "quiet");
    quiet.audio = false;
    env.synth("Valorant 2026.11.02 - 01.05.00.DVR.mp4", quiet)
        .await;
    let silent = env
        .wait_clip(
            |c| c.state == "ready" && c.id != clip.id,
            Duration::from_secs(120),
        )
        .await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let cut = |id: String, body: serde_json::Value| {
        let env = &env;
        async move {
            let r = env
                .client
                .post(format!("{}/api/clips/{id}/cuts", env.base))
                .json(&body)
                .send()
                .await
                .unwrap();
            (
                r.status().as_u16(),
                r.json::<serde_json::Value>().await.unwrap(),
            )
        }
    };
    let made = |d: serde_json::Value| {
        let env = &env;
        async move {
            let j = env
                .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(120))
                .await;
            assert_eq!(j.state, "done", "{:?}", j.error);
            env.store()
                .rendition(d["rendition_id"].as_str().unwrap())
                .await
                .unwrap()
                .unwrap()
        }
    };

    let (status, d) = cut(
        clip.id.clone(),
        serde_json::json!({ "start": 1.0, "end": 2.0, "fade_in": 1, "fade_out": 1 }),
    )
    .await;
    assert_eq!(status, 400, "{d}");
    assert!(
        d["error"].as_str().unwrap().contains("longer than the cut"),
        "{d}"
    );
    let (status, _) = cut(clip.id.clone(), serde_json::json!({ "fade_in": 9 })).await;
    assert_eq!(status, 400, "a fade of nine seconds");

    let (status, d) = cut(
        clip.id.clone(),
        serde_json::json!({ "start": 1.0, "end": 5.0, "reframe": { "shape": "tall", "x": 0.25 }, "fade_in": 0.5, "fade_out": 0.5 }),
    )
    .await;
    assert_eq!(status, 200, "{d}");
    let tall = made(d).await;
    assert_eq!(tall.video_copied, 0, "a framed, faded cut is encoded again");
    assert_eq!(
        (tall.width, tall.height),
        (Some(202), Some(360)),
        "9:16 of a 640 by 360 picture"
    );
    assert!(
        (tall.duration.unwrap() - 4.0).abs() < 0.3,
        "{:?}",
        tall.duration
    );
    assert_eq!(tall.acodec.as_deref(), Some("aac"), "its sound, faded");
    let recipe: reel_core::CutRecipe =
        serde_json::from_str(tall.recipe.as_deref().unwrap()).unwrap();
    assert_eq!(recipe.reframe.unwrap().x, 0.25);
    let (_, page) = text_of(&env, &format!("/clips/{}", clip.id)).await;
    assert!(
        page.contains(r#""reframe":{"shape":"tall","x":0.25}"#),
        "the cut's frame for the editor"
    );
    assert!(page.contains(r#""fade_in":0.5"#) && page.contains(r#""fade_out":0.5"#));
    assert!(page.contains(r#"id="reframe""#) && page.contains(r#"id="reframe-window""#));

    let (status, d) = cut(
        clip.id.clone(),
        serde_json::json!({ "reframe": { "shape": "square" } }),
    )
    .await;
    assert_eq!(status, 200, "the whole clip, square: {d}");
    let square = made(d).await;
    assert_eq!((square.width, square.height), (Some(360), Some(360)));

    let (status, d) = cut(silent.id.clone(), serde_json::json!({ "fade_out": 1.0 })).await;
    assert_eq!(status, 200, "a picture fades without any sound: {d}");
    let faded = made(d).await;
    assert_eq!(faded.video_copied, 0);
    assert!(faded.acodec.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_game_is_a_collection_by_itself_and_its_link_follows_the_game() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let ingest = |name: &'static str| {
        let env = &env;
        async move {
            let before: Vec<String> = env
                .store()
                .clips("", reel_store::Sort::Newest)
                .await
                .unwrap()
                .into_iter()
                .map(|r| r.clip.id)
                .collect();
            env.synth(name, Synth::small(2.0, name)).await;
            env.wait_clip(
                |c| c.state == "ready" && !before.contains(&c.id),
                Duration::from_secs(90),
            )
            .await
        }
    };
    let get = |path: String| {
        let env = &env;
        async move {
            let r = env
                .client
                .get(format!("{}{path}", env.base))
                .send()
                .await
                .unwrap();
            (r.status().as_u16(), r.text().await.unwrap())
        }
    };
    let first = ingest("VRChat 2026.10.01 - 20.00.00.DVR.mp4").await;
    let other = ingest("Valorant 2026.10.02 - 20.00.00.DVR.mp4").await;
    assert_eq!(first.game.as_deref(), Some("VRChat"));
    let game_set = |key: &'static str| {
        let env = &env;
        async move {
            env.store()
                .collections()
                .await
                .unwrap()
                .into_iter()
                .find(|c| c.collection.game.as_deref() == Some(key))
        }
    };
    let vr = game_set("vrchat")
        .await
        .expect("VRChat is a collection by itself");
    assert_eq!((vr.collection.name.as_str(), vr.clips), ("VRChat", 1));
    let vr_id = vr.collection.id.clone();
    let (_, list) = get("/collections".into()).await;
    assert!(
        list.contains(&format!(r#"href="/collections/{vr_id}""#))
            && list.contains(r#"<span class="badge">automatic</span>"#)
    );
    let (_, page) = get(format!("/collections/{vr_id}")).await;
    assert!(
        page.contains(&format!(r#"data-id="{}""#, first.id)),
        "its page lists the clip with the game"
    );
    assert!(
        !page.contains(r#"id="set-name""#) && !page.contains(r#"id="set-delete""#),
        "no renaming or deleting by hand"
    );
    assert!(
        page.contains("new ones included"),
        "the link's words say it follows the game"
    );
    let (_, clip_page) = get(format!("/clips/{}", first.id)).await;
    assert!(clip_page.contains(&format!(
        r#"Also in <a href="/collections/{vr_id}">VRChat</a>"#
    )));
    assert!(
        !clip_page.contains(&format!(r#"data-set="{vr_id}""#)),
        "no tick box for it on the clip page"
    );

    for (method, path) in [
        (
            "PUT",
            format!("/api/collections/{vr_id}/clips/{}", other.id),
        ),
        (
            "DELETE",
            format!("/api/collections/{vr_id}/clips/{}", first.id),
        ),
        ("DELETE", format!("/api/collections/{vr_id}")),
    ] {
        let r = env
            .client
            .request(method.parse().unwrap(), format!("{}{path}", env.base))
            .header("sec-fetch-site", "same-origin")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400, "{method} {path} by hand");
    }
    let r = env
        .client
        .patch(format!("{}/api/collections/{vr_id}", env.base))
        .header("sec-fetch-site", "same-origin")
        .json(&serde_json::json!({ "name": "Mine" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400, "no renaming");

    let (code, link) = post_json_body(
        &env,
        &format!("/api/collections/{vr_id}/link"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(code, 200, "{link}");
    let url = link["url"].as_str().unwrap();
    let path = url[url.find("/c/").unwrap()..].to_string();
    let tiles = |html: &str| html.matches(r#"class="card""#).count();
    let (code, public) = get(path.clone()).await;
    assert_eq!((code, tiles(&public)), (200, 1));

    let later = ingest("VRChat 2026.10.03 - 20.00.00.DVR.mp4").await;
    let (_, public) = get(path.clone()).await;
    assert_eq!(
        tiles(&public),
        2,
        "a clip added after the link was made is on it"
    );
    assert!(public.contains(&format!("{path}/{}", later.id)));
    let (code, _) = get(format!("{path}/{}", later.id)).await;
    assert_eq!(code, 200, "and plays under it");

    env.store()
        .set_clip_game(&first.id, Some("Valorant"))
        .await
        .unwrap();
    assert_eq!(
        game_set("vrchat").await.unwrap().clips,
        1,
        "its game changed and it left VRChat"
    );
    assert_eq!(game_set("valorant").await.unwrap().clips, 2, "for Valorant");
    let (code, _) = get(format!("{path}/{}", first.id)).await;
    assert_eq!(code, 404, "and no longer plays under VRChat's link");

    env.store().set_clip_game(&later.id, None).await.unwrap();
    assert!(
        game_set("vrchat").await.is_none(),
        "a game whose last clip went drops out"
    );
    let (_, list) = get("/collections".into()).await;
    assert!(!list.contains(&format!("/collections/{vr_id}")));
    let (code, _) = get(path.clone()).await;
    assert_eq!(code, 404, "its link answers as a revoked one does");
    env.store()
        .set_clip_game(&later.id, Some("VRChat"))
        .await
        .unwrap();
    let (code, public) = get(path.clone()).await;
    assert_eq!(
        (code, tiles(&public)),
        (200, 1),
        "and comes back with the game, the same link"
    );
    assert_eq!(game_set("vrchat").await.unwrap().collection.id, vr_id);
}

fn relative_imports(js: &str) -> Vec<String> {
    let mut out = Vec::new();
    for lead in ["from '", "import '", "import('", "from \"", "import \""] {
        let mut rest = js;
        while let Some(at) = rest.find(lead) {
            rest = &rest[at + lead.len()..];
            let quote = if lead.ends_with('"') { '"' } else { '\'' };
            if let Some(end) = rest.find(quote) {
                let spec = &rest[..end];
                if spec.starts_with("./") || spec.starts_with("../") {
                    out.push(spec.to_string());
                }
            }
        }
    }
    out
}

fn resolve_from(base: &str, spec: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();
    for seg in spec.split('/') {
        match seg {
            "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

#[tokio::test(flavor = "multi_thread")]
async fn every_module_a_page_loads_carries_the_pages_build() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let page = env
        .client
        .get(format!("{}/upload", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let at = page
        .find(r#"<script type="module" src="/assets/"#)
        .expect("the page's script");
    let src = &page[at + r#"<script type="module" src=""#.len()..];
    let src = &src[..src.find('"').unwrap()];
    let mark = src.split('/').nth(2).unwrap().to_string();
    assert!(
        src.ends_with("/reel.js") && mark.starts_with(reel_core::VERSION),
        "{src}"
    );
    assert!(
        page.contains(&format!(r#"href="/assets/{mark}/reel.css""#)),
        "the stylesheet too"
    );
    let mut queue = vec![src.to_string(), format!("/assets/{mark}/share.js")];
    let mut seen = std::collections::HashSet::new();
    while let Some(url) = queue.pop() {
        if !seen.insert(url.clone()) {
            continue;
        }
        assert!(
            url.starts_with(&format!("/assets/{mark}/")),
            "{url} is outside the page's build"
        );
        let r = env
            .client
            .get(format!("{}{url}", env.base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "{url}");
        assert!(
            r.headers()["cache-control"]
                .to_str()
                .unwrap()
                .contains("immutable"),
            "{url} is kept for good"
        );
        let js = r.text().await.unwrap();
        for spec in relative_imports(&js) {
            queue.push(resolve_from(&url, &spec));
        }
    }
    let mut modules: Vec<String> =
        std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/assets/js"))
            .unwrap()
            .map(|e| {
                format!(
                    "/assets/{mark}/js/{}",
                    e.unwrap().file_name().to_string_lossy()
                )
            })
            .collect();
    modules.sort();
    let mut reached: Vec<String> = seen
        .iter()
        .filter(|u| u.contains("/js/"))
        .cloned()
        .collect();
    reached.sort();
    assert_eq!(
        reached, modules,
        "the crawl reaches every module under web/assets/js, each of the page's build"
    );
    let css = env
        .client
        .get(format!("{}/assets/{mark}/reel.css", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let font = css
        .split("url(")
        .nth(1)
        .map(|u| {
            u.split(')')
                .next()
                .unwrap()
                .trim_matches(['"', '\''])
                .to_string()
        })
        .expect("a font");
    let r = env
        .client
        .get(format!(
            "{}{}",
            env.base,
            resolve_from(&format!("/assets/{mark}/reel.css"), &font)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        200,
        "the fonts follow the stylesheet's build: {font}"
    );
    let r = env
        .client
        .get(format!("{}/assets/js/lib.js", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        200,
        "a page from before the marks still gets its modules"
    );
    assert_eq!(r.headers()["cache-control"], "no-cache");
    let r = env
        .client
        .get(format!(
            "{}/assets/0.9.0-0000000000-20260101/js/lib.js",
            env.base
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        404,
        "another build's mark gets nothing of this one"
    );
    let r = env
        .client
        .get(format!("{}/assets/{mark}/sw.js", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.headers()["service-worker-allowed"], "/");
}

#[cfg(not(feature = "style-page"))]
#[tokio::test(flavor = "multi_thread")]
async fn a_release_build_has_no_demo_pages() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    for path in [
        "/style",
        "/__shot/clip",
        "/__shot/share",
        "/__shot/collection",
        "/__shot/clip-tracks",
        "/__shot/collection-link",
    ] {
        let r = env
            .client
            .get(format!("{}{path}", env.base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404, "{path}");
    }
    let r = env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn trash_kills_links_and_restore_brings_them_back() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("bye.mp4", Synth::small(2.0, "bye")).await;
    let clip = env.wait_ready(Duration::from_secs(60)).await;
    let s: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/shares", env.base, clip.id))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = s["token"].as_str().unwrap().to_string();
    assert_eq!(
        env.client
            .get(format!("{}/s/{token}", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        env.client
            .delete(format!("{}/api/clips/{}", env.base, clip.id))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        env.client
            .get(format!("{}/s/{token}", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert!(env.ctx.config().trash_dir().join(&clip.id).exists());
    assert!(!env.ctx.config().clip_dir(&clip.id).exists());
    assert!(!env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
        .contains("bye"));
    assert_eq!(
        env.client
            .post(format!("{}/api/clips/{}/restore", env.base, clip.id))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        env.client
            .get(format!("{}/s/{token}", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert!(env.ctx.config().clip_dir(&clip.id).exists());
    env.client
        .delete(format!("{}/api/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap();
    reel_pipeline::housekeeping::once(&env.ctx).await.unwrap();
    assert!(
        env.store().clip(&clip.id).await.unwrap().is_some(),
        "a fresh trash entry stays"
    );
    assert!(env.ctx.config().trash_dir().join(&clip.id).exists());
    let old = jiff::Timestamp::now()
        .checked_sub(jiff::Span::new().hours(24 * 8))
        .unwrap()
        .to_string();
    sqlx::query("UPDATE clip SET deleted_at = ? WHERE id = ?")
        .bind(&old)
        .bind(&clip.id)
        .execute(env.store().pool())
        .await
        .unwrap();
    reel_pipeline::housekeeping::once(&env.ctx).await.unwrap();
    assert!(
        env.store().clip(&clip.id).await.unwrap().is_none(),
        "purged after trash_days"
    );
    assert!(!env.ctx.config().trash_dir().join(&clip.id).exists());
    assert!(
        env.store().share_by_token(&token).await.unwrap().is_none(),
        "its shares go with it"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn proxy_auth_trusts_only_the_proxy() {
    let Some(env) = start(AuthMode::Proxy, vec!["127.0.0.1/32"]).await else {
        return;
    };
    let r = env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    assert!(r.text().await.unwrap().contains("sign-in proxy"));
    assert_eq!(
        env.client
            .get(format!("{}/api/settings", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let r = env
        .client
        .get(format!("{}/", env.base))
        .header("x-authentik-username", "robin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(r.text().await.unwrap().contains("robin"));
    assert_eq!(
        env.client
            .get(format!("{}/healthz", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        env.client
            .get(format!("{}/s/8xQvT1n4wKpR9sLm2cHd7A", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        env.client
            .get(format!("{}/upload", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let Some(strict) = start(AuthMode::Proxy, vec!["10.99.0.0/16"]).await else {
        return;
    };
    let r = strict
        .client
        .get(format!("{}/", strict.base))
        .header("x-authentik-username", "robin")
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        401,
        "a header from an untrusted address is ignored"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn clips_brought_in_from_the_old_host_render_as_imported() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("Desktop 2026.10.05 - 23.41.09.mkv", Synth::obs(2.0, "old"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(120)).await;
    sqlx::query("UPDATE clip SET source = 'imported', recorded_source = 'imported' WHERE id = ?")
        .bind(&clip.id)
        .execute(env.store().pool())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE job SET payload = json_set(payload, '$.source', 'imported') WHERE clip_id = ?",
    )
    .bind(&clip.id)
    .execute(env.store().pool())
    .await
    .unwrap();
    let library = env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(library.contains("imported"), "the card says imported");
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 200);
    let page = page.text().await.unwrap();
    assert!(
        page.contains("from the imported library"),
        "the date's source is named"
    );
    assert!(
        page.contains("not kept, the stream was copied"),
        "no original, no special case"
    );
    let jobs = env
        .client
        .get(format!("{}/jobs?show=history", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(jobs.status(), 200, "the finished import job still renders");
}

#[tokio::test(flavor = "multi_thread")]
async fn embed_copy_serves_the_share_when_the_master_is_too_big() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| {
        c.share.embed_mb = 2;
    })
    .await
    else {
        return;
    };
    env.synth("huge.mp4", Synth::h264_1080p(5.0, "huge")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(
        master.size.unwrap() > 2_000_000,
        "the master is over the embed limit: {:?}",
        master.size
    );
    let embed = env
        .store()
        .derived_rendition(&master.id, "embed", None)
        .await
        .unwrap()
        .expect("an embed copy was made at ingest");
    assert!(embed.is_ready(), "{:?}", embed.error);
    assert_eq!(
        embed.height,
        Some(480),
        "the ladder's rung for 5 s aimed under 2 MB"
    );
    assert!(embed.size.unwrap() <= 2_000_000, "{:?}", embed.size);
    let s: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/shares", env.base, clip.id))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let html = env
        .client
        .get(format!("{}/s/{}", env.base, s["token"].as_str().unwrap()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let tags = og(&html);
    assert_eq!(
        tags["og:video:width"], "854",
        "the tags describe the embed copy"
    );
    assert_eq!(tags["og:video:height"], "480");
    assert!(tags["og:video"].contains(&format!("r={}", embed.id)));
    let v = env.client.get(&tags["og:video"]).send().await.unwrap();
    assert_eq!(v.status(), 200);
    let len: u64 = v.headers()["content-length"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        len,
        embed.size.unwrap() as u64,
        "the share serves the embed copy, not the master"
    );
    let dl = env
        .client
        .get(format!(
            "{}/s/{}/download",
            env.base,
            s["token"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    let dlen: u64 = dl.headers()["content-length"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        dlen,
        master.size.unwrap() as u64,
        "the download is still the full current rendition"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn gpu_routes_fall_back_to_the_cpu_per_clip() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| {
        c.media.hwaccel = reel_core::config::Hwaccel::Qsv;
        c.media.render_device = "/dev/dri/renderD999".into();
    })
    .await
    else {
        return;
    };
    env.synth("gpu.mkv", Synth::hevc_1440p(2.0, "gpu")).await;
    let clip = env.wait_ready(Duration::from_secs(180)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(master.vcodec.as_deref(), Some("h264"));
    assert_eq!(master.height, Some(1440), "re-encodes keep up to 1440p now");
    let reason = clip.plan_reason.clone().unwrap();
    assert!(
        reason.contains("(libx264)"),
        "both QSV routes fail without a device and the clip still lands: {reason}"
    );
    let d: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/cuts", env.base, clip.id))
        .json(&serde_json::json!({ "start": 0.7, "end": 1.8 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let j = env
        .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(120))
        .await;
    assert_eq!(
        j.state, "done",
        "a re-encoded cut takes the same ladder: {:?}",
        j.error
    );
    let health: serde_json::Value = env
        .client
        .get(format!("{}/healthz", env.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["hwaccel"], "qsv");
    assert!(health["hw_decode"].is_array());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_obs_profile_is_remuxed_with_both_streams_copied() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let kept = env.incoming().join("..").join("obs-source.mkv");
    let spec = Synth::obs(3.0, "obs");
    env.ctx.ff.run(&spec.args(&kept), 3.0, None).await.unwrap();
    tokio::fs::copy(&kept, env.incoming().join("Replay_2026-10-06_21-14-03.mkv"))
        .await
        .unwrap();
    let source = env.ctx.ff.probe(&kept).await.unwrap();
    assert_eq!(source.acodec.as_deref(), Some("aac"));
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(master.video_copied, 1);
    let reason = clip.plan_reason.clone().unwrap();
    assert!(
        reason.starts_with("video copied: H.264 1080p") && reason.ends_with("audio copied: AAC"),
        "{reason}"
    );
    let master_path = env.ctx.config().abs(master.path.as_deref().unwrap());
    let out = env.ctx.ff.probe(&master_path).await.unwrap();
    assert_eq!(out.vcodec.as_deref(), Some("h264"));
    assert_eq!(out.profile.as_deref(), Some("High"));
    assert_eq!(out.acodec.as_deref(), Some("aac"));
    assert!(
        (out.fps - 60.0).abs() < 0.1,
        "60 fps within a tolerance for container timestamps: {}",
        out.fps
    );
    let source_size = source.size as f64;
    let master_size = master.size.unwrap() as f64;
    assert!(
        (master_size - source_size).abs() / source_size < 0.02,
        "a remux keeps the size: {master_size} vs {source_size}"
    );
    for stream in ["0:v:0", "0:a:0"] {
        let before = env.ctx.ff.stream_md5(&kept, stream).await.unwrap();
        let after = env.ctx.ff.stream_md5(&master_path, stream).await.unwrap();
        assert_eq!(
            before, after,
            "the packets of {stream} are the source's, byte for byte"
        );
    }
    assert!((out.duration - source.duration).abs() < 0.1);
    assert!(
        clip.original_path.is_none(),
        "nothing was re-encoded, so no raw copy is kept"
    );
    assert!(!env
        .incoming()
        .join("Replay_2026-10-06_21-14-03.mkv")
        .exists());
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("kbit/s, copied"), "the audio row says copied");
}

async fn patch_setting(env: &Env, body: serde_json::Value) -> (u16, serde_json::Value) {
    let r = env
        .client
        .patch(format!("{}/api/settings", env.base))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    (status, r.json().await.unwrap_or(serde_json::Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_save_live_with_undo_history_and_locks() {
    let file: toml::Value = toml::from_str("[share]\ntrash_days = 3\n").unwrap();
    let Some(env) = start_layered(AuthMode::Single, vec![], |_| {}, Some(file), true).await else {
        return;
    };
    let r = env
        .client
        .get(format!("{}/settings", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        200,
        "the bare route is the index of the sections"
    );
    let index = r.text().await.unwrap();
    assert!(
        index.contains(r#"data-section="general" data-index"#),
        "General beside the list on a wide screen, the list alone on a phone"
    );
    let page = env
        .client
        .get(format!("{}/settings/ingest", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("data-key=\"ingest.poll_seconds\"") && page.contains("Poll every"));
    assert!(
        page.contains("set in config.toml"),
        "a key from the file shows as locked"
    );
    assert!(
        page.contains("after a restart"),
        "restart-only keys are marked"
    );
    assert!(
        page.contains("aria-current=\"page\">Ingest"),
        "the section is current"
    );

    let (status, d) = patch_setting(&env, serde_json::json!({ "ingest.poll_seconds": 9 })).await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(d["value"], "9");
    assert_eq!(d["old"], "1");
    assert_eq!(d["changed"], true);
    assert_eq!(
        env.ctx.config().ingest.poll_seconds,
        9,
        "the change applies live"
    );
    let page = env
        .client
        .get(format!("{}/settings/ingest", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("changed, was 1"), "the row shows the change");

    let (status, d) = patch_setting(&env, serde_json::json!({ "ingest.poll_seconds": 0 })).await;
    assert_eq!(status, 400);
    assert!(
        d["error"].as_str().unwrap().contains("between 1 and 3600"),
        "{d}"
    );
    assert_eq!(
        env.ctx.config().ingest.poll_seconds,
        9,
        "a refused value changes nothing"
    );

    let (status, d) = patch_setting(&env, serde_json::json!({ "share.embed_mb": 99 })).await;
    assert_eq!(status, 400);
    assert!(
        d["error"].as_str().unwrap().contains("between 5 and 95"),
        "{d}"
    );

    let (status, d) = patch_setting(&env, serde_json::json!({ "share.trash_days": 30 })).await;
    assert_eq!(status, 409, "{d}");
    assert!(d["error"].as_str().unwrap().contains("set in config.toml"));
    assert_eq!(env.ctx.config().share.trash_days, 3);

    let (status, _) = patch_setting(&env, serde_json::json!({ "listen": "1.2.3.4:1" })).await;
    assert_eq!(status, 409, "a fixed key is never written");
    let (status, _) = patch_setting(&env, serde_json::json!({ "nope": 1 })).await;
    assert_eq!(status, 400);

    let (status, d) = patch_setting(&env, serde_json::json!({ "ingest.poll_seconds": null })).await;
    assert_eq!(status, 200);
    assert_eq!(d["value"], "1");
    assert_eq!(d["changed"], false);
    assert_eq!(
        env.ctx.config().ingest.poll_seconds,
        1,
        "null goes back under the table"
    );

    let (status, _) =
        patch_setting(&env, serde_json::json!({ "share.expiry_default": "7d" })).await;
    assert_eq!(status, 200);
    let api: serde_json::Value = env
        .client
        .get(format!("{}/api/settings", env.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(api["settings"]["share_expiry_default"], "7d");
    let row = api["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "share.trash_days")
        .unwrap();
    assert_eq!(row["source"], "file");
    assert_eq!(row["value"], "3");

    let history: serde_json::Value = env
        .client
        .get(format!("{}/api/settings/history", env.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let entries = history["history"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "{history}");
    assert_eq!(entries[0]["key"], "share.expiry_default");
    assert_eq!(entries[1]["key"], "ingest.poll_seconds");
    assert!(
        entries[1]["new"].is_null(),
        "the reset is recorded as going back"
    );
    assert_eq!(entries[2]["new"], "9");
    assert_eq!(entries[2]["user"], "you");
}

async fn get_json(env: &Env, path: &str) -> (u16, serde_json::Value) {
    let r = env
        .client
        .get(format!("{}{path}", env.base))
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    (status, r.json().await.unwrap_or(serde_json::Value::Null))
}

async fn post_json_body(
    env: &Env,
    path: &str,
    body: serde_json::Value,
) -> (u16, serde_json::Value) {
    let r = env
        .client
        .post(format!("{}{path}", env.base))
        .header("sec-fetch-site", "same-origin")
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    (status, r.json().await.unwrap_or(serde_json::Value::Null))
}

async fn post_json(env: &Env, path: &str) -> (u16, serde_json::Value) {
    let r = env
        .client
        .post(format!("{}{path}", env.base))
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    (status, r.json().await.unwrap_or(serde_json::Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn jobs_page_with_retry_dismiss_and_cancel() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    std::fs::write(env.incoming().join("broken.mkv"), b"not a video at all").unwrap();
    env.wait_clip(|c| c.state == "failed", Duration::from_secs(60))
        .await;
    let start_at = Instant::now();
    let failed = loop {
        let failed = env.store().failed_jobs().await.unwrap();
        if !failed.is_empty() || start_at.elapsed() > Duration::from_secs(10) {
            break failed;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(
        failed.len(),
        1,
        "the job is marked failed just after its clip"
    );
    let job = failed[0].clone();
    let page = env
        .client
        .get(format!("{}/jobs?show=failed", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains(&job.id) && page.contains("Retry") && page.contains("could not read"),
        "the failed job with its reason"
    );
    let counts: serde_json::Value = env
        .client
        .get(format!("{}/api/jobs/counts", env.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(counts["failed"], 1);
    assert!(env.incoming().join("failed").join("broken.mkv").exists());

    let (status, d) = post_json(&env, &format!("/api/jobs/{}/retry", job.id)).await;
    assert_eq!(status, 200, "{d}");
    let again = d["job_id"].as_str().unwrap().to_string();
    let done = env.wait_job_done(&again, Duration::from_secs(60)).await;
    assert_eq!(done.state, "failed", "the file is still broken");
    assert_eq!(done.attempts, 2, "the attempt count carries on");
    let failed = env.store().failed_jobs().await.unwrap();
    assert_eq!(
        failed.len(),
        1,
        "the retried job replaces the old one in the list"
    );
    assert_eq!(failed[0].id, again);
    assert!(
        env.incoming().join("failed").join("broken.mkv").exists(),
        "parked again"
    );

    let (status, _) = post_json(&env, &format!("/api/jobs/{again}/dismiss")).await;
    assert_eq!(status, 200);
    assert!(env.store().failed_jobs().await.unwrap().is_empty());

    env.synth(
        "2026-10-06 21-14-03.mkv",
        Synth::hevc_1440p(8.0, "cancel me"),
    )
    .await;
    let start = Instant::now();
    let running = loop {
        let now = env.store().jobs_now().await.unwrap();
        if let Some(j) = now
            .iter()
            .find(|j| j.state == "running" && j.kind == "ingest")
        {
            break j.clone();
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "the ingest never ran"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let (status, _) = post_json(&env, &format!("/api/jobs/{}/cancel", running.id)).await;
    assert_eq!(status, 200);
    let start = Instant::now();
    loop {
        let j = env.store().job(&running.id).await.unwrap().unwrap();
        if j.state == "cancelled" {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the job did not cancel: {}",
            j.state
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        env.incoming().join("2026-10-06 21-14-03.mkv").exists(),
        "a cancelled ingest leaves the raw file in the incoming folder"
    );
    assert!(
        env.store()
            .clip(running.clip_id.as_deref().unwrap())
            .await
            .unwrap()
            .is_none(),
        "the half-made clip is gone"
    );
    let back = env.wait_ready(Duration::from_secs(600)).await;
    assert_eq!(
        back.source_name, "2026-10-06 21-14-03.mkv",
        "the next poll takes the file again"
    );
    let history = env
        .client
        .get(format!("{}/jobs?show=history", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        history.contains("cancelled") && history.contains(">done<"),
        "the history lists both"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_ingest_is_discarded_with_its_file_and_clip_even_when_the_file_is_gone() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    std::fs::write(
        env.incoming().join("junk.mp4"),
        b"the relay cut this one off",
    )
    .unwrap();
    let clip = env
        .wait_clip(|c| c.state == "failed", Duration::from_secs(60))
        .await;
    let parked = env.incoming().join("failed").join("junk.mp4");
    assert!(parked.exists());
    let page = env
        .client
        .get(format!("{}/jobs?show=failed", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("data-act=\"discard\""),
        "the failed ingest offers Discard"
    );
    let clip_page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        clip_page.contains("id=\"discard\"") && !clip_page.contains("id=\"delete\""),
        "the failed clip offers Discard instead of the trash"
    );
    let job = env.store().failed_jobs().await.unwrap()[0].clone();
    let (status, d) = post_json(&env, &format!("/api/jobs/{}/discard", job.id)).await;
    assert_eq!(status, 200, "{d}");
    assert!(!parked.exists(), "the parked file is gone");
    assert!(
        !env.incoming().join("failed").join("junk.mp4.txt").exists(),
        "and its note"
    );
    assert!(env.store().clip(&clip.id).await.unwrap().is_none());
    assert!(env.store().job(&job.id).await.unwrap().is_none());
    assert!(env.store().ingest_file("junk.mp4").await.unwrap().is_none());
    assert!(env.store().failed_jobs().await.unwrap().is_empty());
    assert_eq!(
        env.client
            .get(format!("{}/clips/{}", env.base, clip.id))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    std::fs::write(
        env.incoming().join("more junk.mp4"),
        b"audio only, says the relay",
    )
    .unwrap();
    let clip = env
        .wait_clip(|c| c.state == "failed", Duration::from_secs(60))
        .await;
    std::fs::remove_file(env.incoming().join("failed").join("more junk.mp4")).unwrap();
    let (status, d) = post_json(&env, &format!("/api/clips/{}/discard", clip.id)).await;
    assert_eq!(
        status, 200,
        "a parked file deleted by hand is no obstacle: {d}"
    );
    assert!(env.store().clip(&clip.id).await.unwrap().is_none());
    assert!(env.store().failed_jobs().await.unwrap().is_empty());

    let elsewhere = env.ctx.config().data_dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let bystander = elsewhere.join("pruned.mp4");
    std::fs::write(&bystander, b"a file of that name somewhere else").unwrap();
    std::fs::write(env.incoming().join("pruned.mp4"), b"junk again").unwrap();
    let clip = env
        .wait_clip(
            |c| c.state == "failed" && c.source_name == "pruned.mp4",
            Duration::from_secs(60),
        )
        .await;
    sqlx::query("DELETE FROM job WHERE clip_id = ?")
        .bind(&clip.id)
        .execute(env.store().pool())
        .await
        .unwrap();
    sqlx::query("UPDATE clip SET source_name = ? WHERE id = ?")
        .bind(bystander.to_string_lossy().into_owned())
        .bind(&clip.id)
        .execute(env.store().pool())
        .await
        .unwrap();
    let (status, d) = post_json(&env, &format!("/api/clips/{}/discard", clip.id)).await;
    assert_eq!(status, 200, "with its job rows pruned: {d}");
    assert!(
        !env.incoming().join("failed").join("pruned.mp4").exists(),
        "the parked file, by its name"
    );
    assert!(!env
        .incoming()
        .join("failed")
        .join("pruned.mp4.txt")
        .exists());
    assert!(
        bystander.exists(),
        "no path is built from the clip's source name"
    );
    assert!(env.store().clip(&clip.id).await.unwrap().is_none());
    assert!(env
        .store()
        .ingest_file("pruned.mp4")
        .await
        .unwrap()
        .is_none());

    env.synth("2026-10-06 21-14-03.mkv", Synth::small(2.0, "fine"))
        .await;
    let ready = env.wait_ready(Duration::from_secs(120)).await;
    let (status, d) = post_json(&env, &format!("/api/clips/{}/discard", ready.id)).await;
    assert_eq!(status, 400, "only a failed clip: {d}");
    assert!(env.store().clip(&ready.id).await.unwrap().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_header_dot_says_how_reel_is_and_opens_status() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| c.storage.disk_warn_gb = 0).await
    else {
        return;
    };
    std::fs::create_dir_all(&env.ctx.config().ingest.originals_dir).unwrap();
    let library = || async {
        env.client
            .get(format!("{}/", env.base))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
    };
    let page = library().await;
    assert!(
        page.contains(r#"<i class="dot ok" id="chip-dot"></i>"#)
            && page.contains("<b>ok</b> <span class=\"muted\">Status: all is well</span>")
            && page.contains(r#"<a class="btn small quiet" href="/status">Status page</a>"#),
        "a green dot in the chip, the words in its popover, and the Status page from there: {}",
        page.lines()
            .find(|l| l.contains("id=\"chip-dot\""))
            .unwrap_or("no dot")
    );
    *env.ctx.updates.write().await = reel_pipeline::updates::UpdateState {
        checked_at: Some(reel_core::format::now()),
        latest: Some(reel_pipeline::updates::Release {
            version: "99.0.0".into(),
            url: "https://example.org/reel/releases/v99.0.0".into(),
            published_at: "2026-10-14T10:00:00Z".into(),
            ..Default::default()
        }),
        error: None,
    };
    let page = library().await;
    assert!(
        page.contains(r#"class="dot warn" id="chip-dot""#) && page.contains("an update is out"),
        "amber for an update"
    );
    std::fs::write(env.incoming().join("broken.mkv"), b"not a video at all").unwrap();
    env.wait_clip(|c| c.state == "failed", Duration::from_secs(60))
        .await;
    let page = library().await;
    assert!(
        page.contains(r#"class="dot warn" id="chip-dot""#)
            && page.contains("jobs, an update is out"),
        "the tooltip names what to look at"
    );
    std::fs::remove_dir_all(env.incoming()).unwrap();
    let page = library().await;
    assert!(
        page.contains(r#"class="dot err" id="chip-dot""#)
            && page.contains("a check fails: incoming"),
        "red when a check fails"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn status_page_shows_the_build_the_doctor_and_an_update() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let page = env
        .client
        .get(format!("{}/status", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains(&format!("reel {}", reel_core::VERSION)));
    assert!(page.contains(reel_core::GIT_SHA) && page.contains("ffmpeg"));
    let pin: serde_json::Value =
        serde_json::from_str(include_str!("../../../desktop/obs/pinned.json")).unwrap();
    let obs = format!("OBS Studio {}", pin["version"].as_str().unwrap());
    assert!(
        page.contains(&format!("{obs} in the app&#x27;s Windows installer reel,"))
            || page.contains(&format!("{obs} in the app's Windows installer reel,")),
        "the OBS the app ships is named next to ffmpeg, with the package that carries it"
    );
    assert!(
        page.contains("every re-encode on the CPU"),
        "the software backend is named"
    );
    assert!(
        page.contains("checking is off"),
        "the test config has the update check off"
    );
    assert!(page.contains("Changelog"));

    *env.ctx.updates.write().await = reel_pipeline::updates::UpdateState {
        checked_at: Some(reel_core::format::now()),
        latest: Some(reel_pipeline::updates::Release {
            version: "99.0.0".into(),
            url: "https://example.org/reel/releases/v99.0.0".into(),
            published_at: "2026-10-14T10:00:00Z".into(),
            ..Default::default()
        }),
        error: None,
    };
    let page = env
        .client
        .get(format!("{}/status", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("99.0.0 is out since 2026-10-14")
            && page.contains("https://example.org/reel/releases/v99.0.0"),
        "a newer release shows with its notes"
    );

    let (status, d) = post_json(&env, "/api/status/doctor").await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(d["hardware"]["backend"], "none");
    assert_eq!(env.ctx.hw.read().await.backend, HwBackend::Software);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_changelog_on_status_shows_code_as_code_and_no_marks() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let (status, page) = page_text(&env, "/status").await;
    assert_eq!(status, 200);
    let from = page.find(r#"<div class="panel changelog">"#).unwrap();
    let panel = &page[from..from + page[from..].find("</section>").unwrap()];
    for mark in ["`", "**", "]("] {
        assert!(!panel.contains(mark), "{mark} shows: {panel}");
    }
    let newest_five: String = reel_core::CHANGELOG
        .split("\n## ")
        .skip(1)
        .take(5)
        .collect();
    assert_eq!(
        panel.matches("<code>").count(),
        newest_five.matches('`').count() / 2,
        "{panel}"
    );
}

fn cookie_of(r: &reqwest::Response) -> String {
    r.headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("reel_session="))
        .map(|v| v.split(';').next().unwrap().to_string())
        .expect("a session cookie")
}

fn cookie_flags(r: &reqwest::Response) -> String {
    r.headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("reel_session="))
        .unwrap_or("")
        .to_string()
}

async fn setup_code(env: &Env) -> String {
    env.store()
        .setting(reel_server::auth::SETUP_CODE_KEY)
        .await
        .unwrap()
        .expect("a setup code waits for the first admin")
}

async fn first_admin(env: &Env, name: &str, code: &str) -> reqwest::Response {
    env.client
        .post(format!("{}/signin/first", env.base))
        .form(&[
            ("name", name),
            ("password", "correct horse battery"),
            ("code", code),
        ])
        .send()
        .await
        .unwrap()
}

fn jpeg_with_exif(env: &Env, colour: &str, marker: &str) -> Vec<u8> {
    let out = env.ctx.config().data_dir.join(format!("{colour}.jpg"));
    let ok = std::process::Command::new(&env.ctx.config().media.ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("color={colour}:s=800x600,format=yuvj420p"))
        .args(["-frames:v", "1"])
        .arg(&out)
        .status()
        .unwrap();
    assert!(ok.success());
    let plain = std::fs::read(&out).unwrap();
    let mut tiff = b"II*\0\x08\0\0\0\x01\0\x0e\x01\x02\0".to_vec();
    tiff.extend_from_slice(&(marker.len() as u32 + 1).to_le_bytes());
    tiff.extend_from_slice(&26u32.to_le_bytes());
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(marker.as_bytes());
    tiff.push(0);
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&tiff);
    let mut jpeg = plain[..2].to_vec();
    jpeg.extend_from_slice(&[0xFF, 0xE1]);
    jpeg.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
    jpeg.extend_from_slice(&app1);
    jpeg.extend_from_slice(&plain[2..]);
    jpeg
}

#[tokio::test(flavor = "multi_thread")]
async fn a_signed_in_browser_has_the_servers_version_in_the_header_menu() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let row = |html: &str| {
        html.split(r#"id="part-version">"#)
            .nth(1)
            .and_then(|r| r.split("</dd>").next())
            .map(str::to_string)
            .unwrap_or_default()
    };
    let (s, html) = page_text(&env, "/").await;
    assert_eq!(s, 200);
    let r = row(&html);
    assert!(
        r.contains(&format!("<b>{}</b>", reel_core::VERSION))
            && r.contains(r#"<i class="dot "></i>"#)
            && r.contains("the check for new releases is off"),
        "the test config has the check off: {r}"
    );
    let release = |version: &str| reel_pipeline::updates::UpdateState {
        checked_at: Some(reel_core::format::now()),
        latest: Some(reel_pipeline::updates::Release {
            version: version.into(),
            url: format!("https://example.org/reel/releases/v{version}"),
            published_at: "2026-10-14T10:00:00Z".into(),
            ..Default::default()
        }),
        error: None,
    };
    *env.ctx.updates.write().await = release("99.0.0");
    let r = row(&page_text(&env, "/collections").await.1);
    assert!(
        r.contains(r#"<i class="dot active"></i>"#) && r.contains("reel 99.0.0 is out"),
        "a newer release, in the app's pink: {r}"
    );
    *env.ctx.updates.write().await = release(reel_core::VERSION);
    let r = row(&page_text(&env, "/status").await.1);
    assert!(
        r.contains(&format!(
            "up to date: {} is the newest release",
            reel_core::VERSION
        )),
        "{r}"
    );
    let (s, gone) = page_text(&env, "/s/AAAAAAAAAAAAAAAAAAAAAA").await;
    assert_eq!(s, 404);
    assert!(!gone.contains("part-version"), "not on a link's page");

    let Some(local) = start(AuthMode::Local, vec![]).await else {
        return;
    };
    let (s, _) = page_text(&local, "/").await;
    assert_eq!(s, 303, "nobody is signed in: to the sign in page");
    let (s, signin) = page_text(&local, "/signin").await;
    assert_eq!(s, 200);
    assert!(
        !signin.contains("part-version") && !signin.contains(r#"id="chip-pop""#),
        "nothing of it without a session"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_uploaded_image_becomes_the_poster_without_its_metadata() {
    let Some(env) = start(AuthMode::Local, vec![]).await else {
        return;
    };
    let code = setup_code(&env).await;
    let robin = cookie_of(&first_admin(&env, "robin", &code).await);
    let r = env
        .client
        .post(format!("{}/api/users", env.base))
        .header("cookie", &robin)
        .json(&serde_json::json!({ "name": "mira", "password": "mira has a password", "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let mira = cookie_of(
        &env.client
            .post(format!("{}/signin", env.base))
            .form(&[("name", "mira"), ("password", "mira has a password")])
            .send()
            .await
            .unwrap(),
    );
    env.synth("posted.mp4", Synth::small(4.0, "posted")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(90)).await;
    let id = clip.id.clone();
    let upload = |bytes: Vec<u8>, cookie: &str, site: &'static str| {
        env.client
            .post(format!("{}/api/clips/{id}/poster/image", env.base))
            .header("cookie", cookie.to_string())
            .header("sec-fetch-site", site)
            .body(bytes)
            .send()
    };
    let red = jpeg_with_exif(&env, "red", "GPS-SECRET-MARK");
    assert_eq!(
        upload(red.clone(), &mira, "same-origin")
            .await
            .unwrap()
            .status(),
        403,
        "a member's poster on another's clip"
    );
    assert_eq!(
        upload(red.clone(), &robin, "cross-site")
            .await
            .unwrap()
            .status(),
        403,
        "an upload from another site"
    );
    let r = upload(
        b"just text, named poster.jpg".to_vec(),
        &robin,
        "same-origin",
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 400);
    assert!(r
        .text()
        .await
        .unwrap()
        .contains("not a JPEG, PNG or WebP image"));
    let mut bomb = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    bomb.extend_from_slice(&30_000u32.to_be_bytes());
    bomb.extend_from_slice(&30_000u32.to_be_bytes());
    bomb.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    let r = upload(bomb, &robin, "same-origin").await.unwrap();
    assert_eq!(
        r.status(),
        400,
        "a header that unpacks to 900 megapixels is never decoded"
    );
    let words = r.text().await.unwrap();
    assert!(
        words.contains("pixels") || words.contains("could not be read"),
        "{words}"
    );
    // two bytes over: a larger body still sending could see the connection close first
    let mut over = vec![0xFFu8, 0xD8, 0xFF, 0xE0];
    over.resize(25 * 1024 * 1024 + 2, 0);
    assert_eq!(
        upload(over, &robin, "same-origin").await.unwrap().status(),
        413,
        "over the size limit"
    );

    let r = upload(red, &robin, "same-origin").await.unwrap();
    assert_eq!(r.status(), 200);
    let poster = r.json::<serde_json::Value>().await.unwrap()["poster"]
        .as_str()
        .unwrap()
        .to_string();
    let after = env.store().clip(&id).await.unwrap().unwrap();
    let key = after
        .poster_image
        .clone()
        .expect("the clip says its poster is an image");
    assert!(poster.ends_with(&format!("?t={key}")), "{poster}");
    let served = env
        .client
        .get(format!("{}{poster}", env.base))
        .header("cookie", &robin)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert!(served.starts_with(&[0xFF, 0xD8]));
    let has = |b: &[u8], s: &[u8]| b.windows(s.len()).any(|w| w == s);
    assert!(
        !has(&served, b"GPS-SECRET-MARK") && !has(&served, b"Exif"),
        "no metadata survives"
    );
    let source = env.ctx.config().clip_dir(&id).join("poster-source.jpg");
    assert!(!has(&std::fs::read(&source).unwrap(), b"GPS-SECRET-MARK"));
    let current = env
        .store()
        .rendition(after.current_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let file =
        reel_pipeline::derived::paths(&env.ctx.config().abs(current.path.as_deref().unwrap()))
            .poster;
    let colour = env.ctx.ff.average_color(&file).await.unwrap();
    assert!(
        colour[0] > 200 && colour[1] < 60 && colour[2] < 60,
        "the poster is the image: {colour:?}"
    );
    let probe = env.ctx.ff.probe(&file).await.unwrap();
    let want =
        reel_pipeline::poster_image::poster_size(current.width.unwrap(), current.height.unwrap());
    assert_eq!(
        (probe.width, probe.height),
        want,
        "cropped to the version's frame"
    );
    let lib = env
        .client
        .get(format!("{}/", env.base))
        .header("cookie", &robin)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        lib.contains(&format!(".jpg?t={key}")),
        "the card's poster follows it"
    );

    let r = env
        .client
        .post(format!("{}/api/clips/{id}/cuts", env.base))
        .header("cookie", &robin)
        .json(&serde_json::json!({ "start": 1.0, "end": 3.0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let job = r.json::<serde_json::Value>().await.unwrap()["job_id"]
        .as_str()
        .unwrap()
        .to_string();
    env.wait_job_done(&job, Duration::from_secs(90)).await;
    let cut = env.store().clip(&id).await.unwrap().unwrap();
    assert_ne!(
        cut.current_id, after.current_id,
        "the cut is the current version"
    );
    let cut_r = env
        .store()
        .rendition(cut.current_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let cut_poster =
        reel_pipeline::derived::paths(&env.ctx.config().abs(cut_r.path.as_deref().unwrap())).poster;
    let colour = env.ctx.ff.average_color(&cut_poster).await.unwrap();
    assert!(
        colour[0] > 200 && colour[1] < 60,
        "a new version keeps the uploaded image: {colour:?}"
    );

    let r = env
        .client
        .post(format!("{}/api/clips/{id}/poster", env.base))
        .header("cookie", &robin)
        .json(&serde_json::json!({ "time": 0.5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(
        env.store()
            .clip(&id)
            .await
            .unwrap()
            .unwrap()
            .poster_image
            .is_none(),
        "a frame again"
    );
    let colour = env.ctx.ff.average_color(&cut_poster).await.unwrap();
    assert!(
        !(colour[0] > 200 && colour[1] < 60),
        "the frame, not the image: {colour:?}"
    );
}

async fn poster_colour(env: &Env, version_id: &str) -> [u8; 3] {
    let r = env.store().rendition(version_id).await.unwrap().unwrap();
    let video = env.ctx.config().abs(r.path.as_deref().unwrap());
    env.ctx
        .ff
        .average_color(&reel_pipeline::derived::paths(&video).poster)
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chosen_poster_stays_when_another_version_becomes_current() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let file = pc.join("red then blue.mp4");
    let ok = std::process::Command::new(&env.ctx.config().media.ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("color=c=red:s=640x360:r=30:d=10")
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=10", "-vf"])
        .arg("drawbox=x=0:y=0:w=iw:h=ih:color=blue:t=fill:enable='gte(t,6)'")
        .args(["-c:v", "libx264", "-g", "30", "-pix_fmt", "yuv420p"])
        .args(["-c:a", "aac", "-shortest"])
        .arg(&file)
        .status()
        .unwrap();
    assert!(ok.success());
    std::fs::copy(&file, env.incoming().join("red then blue.mp4")).unwrap();
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let id = clip.id.clone();
    let original = clip.master_id.clone().unwrap();
    let red = |c: [u8; 3]| c[0] > 200 && c[2] < 60;
    let blue = |c: [u8; 3]| c[2] > 200 && c[0] < 60;
    let post = |path: String, body: serde_json::Value| {
        env.client
            .post(format!("{}{path}", env.base))
            .header("sec-fetch-site", "same-origin")
            .json(&body)
            .send()
    };
    let now = || async { env.store().clip(&id).await.unwrap().unwrap() };

    let r = post(
        format!("/api/clips/{id}/poster"),
        serde_json::json!({ "time": 8.0 }),
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 200);
    assert!(blue(poster_colour(&env, &original).await));

    let holds_it = cut_with(&env, &id, serde_json::json!({ "start": 2.0, "end": 9.0 }))
        .await
        .id;
    let c = now().await;
    assert_eq!(c.current_id.as_deref(), Some(holds_it.as_str()));
    let at = c.poster_time.unwrap();
    assert!(
        (at - 6.0).abs() < 0.05,
        "the chosen frame, 8 s into the original, is 6 s into the cut: {at}"
    );
    let colour = poster_colour(&env, &holds_it).await;
    assert!(
        blue(colour),
        "the cut's poster is the frame chosen: {colour:?}"
    );

    let r = post(
        format!("/api/clips/{id}/current"),
        serde_json::json!({ "rendition_id": original }),
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 200);
    let at = now().await.poster_time.unwrap();
    assert!((at - 8.0).abs() < 0.05, "back on the original: {at}");
    assert!(blue(poster_colour(&env, &original).await));

    let without = cut_with(&env, &id, serde_json::json!({ "end": 5.0 })).await;
    let c = now().await;
    assert_eq!(c.current_id.as_deref(), Some(without.id.as_str()));
    assert_eq!(
        c.poster_time,
        Some(reel_pipeline::derived::default_poster_time(
            without.duration.unwrap()
        )),
        "a version without the chosen moment gets its own automatic frame"
    );
    assert!(red(poster_colour(&env, &without.id).await));

    let r = post(
        format!("/api/clips/{id}/current"),
        serde_json::json!({ "rendition_id": holds_it }),
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 200);
    let length = env
        .store()
        .rendition(&holds_it)
        .await
        .unwrap()
        .unwrap()
        .duration
        .unwrap();
    assert_eq!(
        now().await.poster_time,
        Some(reel_pipeline::derived::default_poster_time(length)),
        "the choice ended with the version that did not hold it"
    );
    let colour = poster_colour(&env, &holds_it).await;
    assert!(
        red(colour),
        "its poster is its automatic frame again: {colour:?}"
    );

    let green = jpeg_with_exif(&env, "green", "none");
    let r = env
        .client
        .post(format!("{}/api/clips/{id}/poster/image", env.base))
        .header("sec-fetch-site", "same-origin")
        .body(green)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "{}", r.text().await.unwrap());
    let r = post(
        format!("/api/clips/{id}/current"),
        serde_json::json!({ "rendition_id": original }),
    )
    .await
    .unwrap();
    assert_eq!(r.status(), 200);
    assert!(now().await.poster_image.is_some());
    let colour = poster_colour(&env, &original).await;
    assert!(
        colour[1] > 90 && colour[0] < 60 && colour[2] < 60,
        "an uploaded image stays the poster too: {colour:?}"
    );
    let without_again = cut_with(&env, &id, serde_json::json!({ "end": 4.0 }))
        .await
        .id;
    let colour = poster_colour(&env, &without_again).await;
    assert!(
        colour[1] > 90 && colour[0] < 60 && colour[2] < 60,
        "on a new cut as well: {colour:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn each_member_stars_clips_of_their_own() {
    let Some(env) = start(AuthMode::Local, vec![]).await else {
        return;
    };
    let code = setup_code(&env).await;
    let robin = cookie_of(&first_admin(&env, "robin", &code).await);
    let r = env
        .client
        .post(format!("{}/api/users", env.base))
        .header("cookie", &robin)
        .json(&serde_json::json!({ "name": "mira", "password": "mira has a password", "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let r = env
        .client
        .post(format!("{}/signin", env.base))
        .form(&[("name", "mira"), ("password", "mira has a password")])
        .send()
        .await
        .unwrap();
    let mira = cookie_of(&r);
    env.synth("starred.mp4", Synth::small(2.0, "star")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let id = clip.id.clone();
    let send = |method: &'static str, path: String, cookie: &str, site: &'static str| {
        env.client
            .request(method.parse().unwrap(), format!("{}{path}", env.base))
            .header("cookie", cookie.to_string())
            .header("sec-fetch-site", site)
            .send()
    };
    let card = |cookie: String| {
        let env = &env;
        let id = id.clone();
        async move {
            let page = env
                .client
                .get(format!("{}/", env.base))
                .header("cookie", cookie)
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
            let at = page.find(&format!(r#"data-id="{id}""#)).expect("the card");
            let start = page[..at].rfind("<div").unwrap();
            page[start..at + page[at..].find('>').unwrap()].to_string()
        }
    };
    let starred = |cookie: String| {
        let env = &env;
        let id = id.clone();
        async move {
            let d: serde_json::Value = env
                .client
                .get(format!("{}/api/clips/{id}", env.base))
                .header("cookie", cookie)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            d["starred"].as_bool().unwrap()
        }
    };

    let r = send("PUT", format!("/api/clips/{id}/star"), &mira, "same-origin")
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "a member stars a clip they may not change");
    assert_eq!(
        r.json::<serde_json::Value>().await.unwrap()["starred"],
        true
    );
    assert!(
        card(mira.clone()).await.contains(" data-star"),
        "mira's card is starred"
    );
    assert!(
        !card(robin.clone()).await.contains(" data-star"),
        "robin's is not"
    );
    assert!(starred(mira.clone()).await && !starred(robin.clone()).await);
    let page = send("GET", format!("/clips/{id}"), &mira, "same-origin")
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains(&format!(
            r#"id="star" type="button" data-clip="{id}" aria-pressed="true""#
        )),
        "the clip page's Star is pressed for mira"
    );
    assert_eq!(
        send("PUT", format!("/api/clips/{id}/star"), &robin, "cross-site")
            .await
            .unwrap()
            .status(),
        403,
        "a star from another site is refused"
    );
    assert_eq!(
        send(
            "PUT",
            "/api/clips/01HZZZZZZZZZZZZZZZZZZZZZZZ/star".into(),
            &mira,
            "same-origin"
        )
        .await
        .unwrap()
        .status(),
        404
    );
    assert_eq!(
        send(
            "DELETE",
            format!("/api/clips/{id}/star"),
            &mira,
            "same-origin"
        )
        .await
        .unwrap()
        .status(),
        200
    );
    assert!(
        !card(mira.clone()).await.contains(" data-star") && !starred(mira).await,
        "unstarred"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn picked_clips_are_acted_on_in_one_request_each() {
    let Some(env) = start(AuthMode::Local, vec![]).await else {
        return;
    };
    let code = setup_code(&env).await;
    let robin = cookie_of(&first_admin(&env, "robin", &code).await);
    let r = env
        .client
        .post(format!("{}/api/users", env.base))
        .header("cookie", &robin)
        .json(&serde_json::json!({ "name": "mira", "password": "mira has a password", "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let mira = cookie_of(
        &env.client
            .post(format!("{}/signin", env.base))
            .form(&[("name", "mira"), ("password", "mira has a password")])
            .send()
            .await
            .unwrap(),
    );
    let mut ids = Vec::new();
    for n in 0..3 {
        env.synth(&format!("picked {n}.mp4"), Synth::small(2.0, "picked"))
            .await;
        let clip = env
            .wait_clip(
                |c| c.state == "ready" && !ids.contains(&c.id),
                Duration::from_secs(90),
            )
            .await;
        ids.push(clip.id);
    }
    let post = |path: &'static str, cookie: &str, body: serde_json::Value| {
        env.client
            .post(format!("{}{path}", env.base))
            .header("cookie", cookie.to_string())
            .header("sec-fetch-site", "same-origin")
            .json(&body)
            .send()
    };
    let json = |r: reqwest::Response| async move {
        assert_eq!(r.status(), 200);
        r.json::<serde_json::Value>().await.unwrap()
    };
    let not_yours = "not yours: only who uploaded a clip, or an admin, changes it";

    let d = json(
        post("/api/bulk/delete", &mira, serde_json::json!({ "ids": ids }))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        d["done"].as_array().unwrap().len(),
        0,
        "a member deletes none of the admin's"
    );
    assert_eq!(d["skipped"].as_array().unwrap().len(), 3);
    assert_eq!(d["skipped"][0]["why"], not_yours);
    let r = env
        .client
        .post(format!("{}/api/bulk/delete", env.base))
        .header("cookie", &robin)
        .header("sec-fetch-site", "cross-site")
        .json(&serde_json::json!({ "ids": ids }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403, "a delete from another site is refused");

    let d = json(
        post("/api/bulk/links", &mira, serde_json::json!({ "ids": ids }))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(d["links"].as_array().unwrap().len(), 0);
    assert_eq!(d["skipped"][0]["why"], "no link, and not yours to make one");
    let d = json(
        post(
            "/api/bulk/links",
            &robin,
            serde_json::json!({ "ids": [ids[0], ids[1], "01HZZZZZZZZZZZZZZZZZZZZZZZ"] }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(
        (d["links"].as_array().unwrap().len(), d["made"].as_i64()),
        (2, Some(2))
    );
    assert_eq!(d["skipped"][0]["why"], "not there any more");
    let again = json(
        post(
            "/api/bulk/links",
            &mira,
            serde_json::json!({ "ids": [ids[0]] }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(
        again["links"][0]["url"], d["links"][0]["url"],
        "the clip's live link, for anyone, and no second one made"
    );

    let d = json(
        post(
            "/api/bulk/collect",
            &robin,
            serde_json::json!({ "ids": ids, "name": "Picked" }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(d["added"], 3);
    let set = d["collection"]["id"].as_str().unwrap().to_string();
    let r = post(
        "/api/bulk/collect",
        &mira,
        serde_json::json!({ "ids": ids, "collection": set }),
    )
    .await
    .unwrap();
    assert_eq!(
        r.status(),
        403,
        "a member puts nothing in a collection that is not theirs"
    );
    let list: serde_json::Value = env
        .client
        .get(format!("{}/api/collections", env.base))
        .header("cookie", &mira)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let picked = list["collections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == set.as_str())
        .unwrap()
        .clone();
    assert_eq!(
        (picked["clips"].as_i64(), picked["open"].as_bool()),
        (Some(3), Some(false))
    );

    let stars = |user: &'static str| {
        let env = &env;
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM star s JOIN user u ON u.id = s.user_id WHERE u.name = ?",
            )
            .bind(user)
            .fetch_one(env.store().pool())
            .await
            .unwrap()
        }
    };
    let mut asked = ids.clone();
    asked.push("01HZZZZZZZZZZZZZZZZZZZZZZZ".into());
    let d = json(
        post(
            "/api/bulk/star",
            &mira,
            serde_json::json!({ "ids": asked, "on": true }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(
        (
            d["done"].as_array().unwrap().len(),
            d["skipped"][0]["why"].as_str()
        ),
        (3, Some("not there any more")),
        "a member stars anyone's clips, for herself"
    );
    assert_eq!((stars("mira").await, stars("robin").await), (3, 0));
    let d = json(
        post(
            "/api/bulk/star",
            &mira,
            serde_json::json!({ "ids": [ids[0]], "on": false }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(d["done"].as_array().unwrap().len(), 1);
    assert_eq!(stars("mira").await, 2, "Unstar takes hers off");

    let d = json(
        post(
            "/api/bulk/delete",
            &robin,
            serde_json::json!({ "ids": ids }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(d["done"].as_array().unwrap().len(), 3);
    for id in &ids {
        assert!(env
            .store()
            .clip(id)
            .await
            .unwrap()
            .unwrap()
            .deleted_at
            .is_some());
    }
    let d = json(
        post(
            "/api/bulk/restore",
            &robin,
            serde_json::json!({ "ids": [ids[0], ids[1]] }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(d["done"].as_array().unwrap().len(), 2, "one Undo for all");
    assert!(env
        .store()
        .clip(&ids[0])
        .await
        .unwrap()
        .unwrap()
        .deleted_at
        .is_none());
    let d = json(
        post(
            "/api/bulk/purge",
            &mira,
            serde_json::json!({ "ids": [ids[2]] }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(
        d["skipped"][0]["why"], not_yours,
        "a member deletes none of the admin's for good"
    );
    let d = json(
        post(
            "/api/bulk/purge",
            &robin,
            serde_json::json!({ "ids": [ids[2], ids[0]] }),
        )
        .await
        .unwrap(),
    )
    .await;
    assert_eq!(d["done"], serde_json::json!([ids[2]]));
    assert_eq!(
        d["skipped"][0]["why"], "not there any more",
        "a clip not in the trash is not deleted for good"
    );
    assert!(env.store().clip(&ids[2]).await.unwrap().is_none());
    assert!(env.store().clip(&ids[0]).await.unwrap().is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn built_in_accounts_sign_in_and_members_edit_their_own_clips() {
    let Some(env) = start_with(AuthMode::Local, vec![], |c| c.auth.session_days = 2).await else {
        return;
    };
    let get = |path: &str, cookie: &str| {
        env.client
            .get(format!("{}{path}", env.base))
            .header("cookie", cookie.to_string())
            .send()
    };
    let r = get("/", "").await.unwrap();
    assert_eq!(r.status(), 303);
    assert!(r.headers()["location"]
        .to_str()
        .unwrap()
        .starts_with("/signin?next="));
    assert_eq!(
        get("/api/settings", "").await.unwrap().status(),
        401,
        "the API says sign in"
    );
    let page = get("/signin", "").await.unwrap().text().await.unwrap();
    assert!(
        page.contains("The first admin") && page.contains("name=\"code\""),
        "no user yet: the code is asked for"
    );

    let code = setup_code(&env).await;
    let r = first_admin(&env, "robin", &code).await;
    assert_eq!(r.status(), 303);
    let flags = cookie_flags(&r);
    assert!(
        flags.contains("HttpOnly") && flags.contains("SameSite=Lax"),
        "{flags}"
    );
    assert!(
        !flags.contains("Secure"),
        "plain http in the test: no Secure flag"
    );
    let admin = cookie_of(&r);
    assert_eq!(get("/", &admin).await.unwrap().status(), 200);
    assert_eq!(get("/settings/access", &admin).await.unwrap().status(), 200);
    assert_eq!(
        first_admin(&env, "again", &code).await.status(),
        409,
        "the first admin is made once"
    );

    let r = env
        .client
        .post(format!("{}/api/users", env.base))
        .header("cookie", &admin)
        .json(&serde_json::json!({ "name": "mira", "password": "mira has a password", "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let r = env
        .client
        .post(format!("{}/api/users", env.base))
        .header("cookie", &admin)
        .header("sec-fetch-site", "cross-site")
        .json(&serde_json::json!({ "name": "evil", "password": "twelve characters", "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403, "a change from another site is refused");
    let r = env
        .client
        .post(format!("{}/api/users", env.base))
        .header("cookie", &admin)
        .header("origin", "https://evil.example.net")
        .json(&serde_json::json!({ "name": "evil", "password": "twelve characters", "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403, "an Origin that is not ours is refused too");

    let r = env
        .client
        .post(format!("{}/signin", env.base))
        .form(&[
            ("name", "mira"),
            ("password", "mira has a password"),
            ("next", "/shares"),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303);
    assert_eq!(r.headers()["location"], "/shares");
    let mira = cookie_of(&r);
    assert_eq!(
        get("/settings/general", &mira).await.unwrap().status(),
        403,
        "settings are the admins'"
    );
    let lib = get("/", &mira).await.unwrap().text().await.unwrap();
    assert!(
        !lib.contains("tab-settings"),
        "no Settings tab for a member"
    );
    assert!(lib.contains("member"), "the menu says member");

    let bytes = {
        let path = env.synth("mine.mp4", Synth::small(3.0, "mine")).await;
        let b = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        b
    };
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(bytes).file_name("mine.mp4"),
    );
    let r = env
        .client
        .post(format!("{}/upload", env.base))
        .header("cookie", &mira)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let mine = env.wait_ready(Duration::from_secs(90)).await;
    let mira_row = env.store().user_by_name("mira").await.unwrap().unwrap();
    assert_eq!(
        mine.owner_id.as_deref(),
        Some(mira_row.id.as_str()),
        "the uploader owns the clip"
    );
    let r = env
        .client
        .patch(format!("{}/api/clips/{}", env.base, mine.id))
        .header("cookie", &mira)
        .json(&serde_json::json!({ "title": "mine, renamed" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "a member edits their own clip");

    env.synth("theirs.mp4", Synth::small(3.0, "theirs")).await;
    let theirs = env
        .wait_clip(
            |c| c.state == "ready" && c.source_name == "theirs.mp4",
            Duration::from_secs(90),
        )
        .await;
    assert!(
        theirs.owner_id.is_none(),
        "the incoming folder's clips belong to the admins"
    );
    let r = env
        .client
        .patch(format!("{}/api/clips/{}", env.base, theirs.id))
        .header("cookie", &mira)
        .json(&serde_json::json!({ "title": "not mine" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 403, "a member cannot edit someone else's clip");
    let page = get(&format!("/clips/{}", theirs.id), &mira)
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !page.contains("id=\"edit\"") && !page.contains("id=\"new-link\""),
        "no editor, no new link for a clip that is not theirs"
    );
    let lib = get("/", &admin).await.unwrap().text().await.unwrap();
    assert!(
        lib.contains("by mira"),
        "cards name the uploader once there is more than one user"
    );

    let r = env
        .client
        .post(format!("{}/api/users", env.base))
        .header("cookie", &admin)
        .json(&serde_json::json!({ "name": "nobody", "password": "short" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 400, "the password rule holds");

    let r = env
        .client
        .post(format!("{}/signout", env.base))
        .header("cookie", &mira)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303);
    assert_eq!(
        get("/", &mira).await.unwrap().status(),
        303,
        "the old cookie is dead"
    );
    let r = env
        .client
        .patch(format!("{}/api/users/{}", env.base, mira_row.id))
        .header("cookie", &admin)
        .json(&serde_json::json!({ "password": "a brand new password" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let access = get("/settings/access", &admin)
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        access.contains("Sign out everywhere") && access.contains("mira"),
        "the Access section lists the users"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_session_id_rotates_on_sign_in_and_dies_with_the_store_expiry() {
    let Some(env) = start(AuthMode::Local, vec![]).await else {
        return;
    };
    let code = setup_code(&env).await;
    let r = first_admin(&env, "robin", &code).await;
    assert_eq!(r.status(), 303);
    let first = cookie_of(&r);
    let r = env
        .client
        .get(format!("{}/signin", env.base))
        .send()
        .await
        .unwrap();
    let anonymous = cookie_of(&r);
    assert_ne!(
        anonymous, first,
        "a visit to the sign-in page gets its own anonymous session"
    );
    let r = env
        .client
        .post(format!("{}/signin", env.base))
        .header("cookie", &anonymous)
        .form(&[("name", "robin"), ("password", "correct horse battery")])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303);
    let signed = cookie_of(&r);
    assert_ne!(signed, anonymous, "the session id is rotated at sign-in");
    let status = |cookie: &str| {
        env.client
            .get(format!("{}/", env.base))
            .header("cookie", cookie.to_string())
            .send()
    };
    assert_eq!(status(&signed).await.unwrap().status(), 200);
    assert_eq!(
        status(&anonymous).await.unwrap().status(),
        303,
        "the old id is gone from the store"
    );

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tower_sessions")
        .fetch_one(env.store().pool())
        .await
        .unwrap();
    assert!(rows >= 1);
    let clear_id = signed.trim_start_matches("reel_session=").to_string();
    let hit: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tower_sessions WHERE id = ?")
        .bind(&clear_id)
        .fetch_one(env.store().pool())
        .await
        .unwrap();
    assert_eq!(
        hit, 0,
        "the table holds a hash of the id, never the cookie's value"
    );

    sqlx::query("UPDATE tower_sessions SET expiry_date = 1")
        .execute(env.store().pool())
        .await
        .unwrap();
    assert_eq!(
        status(&signed).await.unwrap().status(),
        303,
        "an expired session is not loaded"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sign_in_attempts_are_limited_per_address_and_a_strangers_forwarded_for_is_ignored() {
    let Some(env) = start(AuthMode::Local, vec![]).await else {
        return;
    };
    let code = setup_code(&env).await;
    assert_eq!(first_admin(&env, "robin", &code).await.status(), 303);
    for n in 0..4 {
        let r = env
            .client
            .post(format!("{}/signin", env.base))
            .header("x-forwarded-for", format!("198.51.100.{n}"))
            .form(&[
                ("name", format!("ghost{n}")),
                ("password", "wrong wrong wrong".into()),
            ])
            .send()
            .await
            .unwrap();
        assert_eq!(
            r.status(),
            401,
            "attempt {n} from an untrusted peer counts against the peer, whatever the header"
        );
    }
    let r = env
        .client
        .post(format!("{}/signin", env.base))
        .header("x-forwarded-for", "198.51.100.99")
        .form(&[("name", "robin"), ("password", "correct horse battery")])
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        429,
        "five sign-in requests from one address in two minutes, the sixth waits"
    );
    assert!(r.headers().get("retry-after").is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn behind_a_trusted_proxy_the_limits_follow_the_client_and_the_name() {
    let Some(env) = start(AuthMode::Local, vec!["127.0.0.1/32"]).await else {
        return;
    };
    let code = setup_code(&env).await;
    assert_eq!(first_admin(&env, "robin", &code).await.status(), 303);
    let attempt = |name: &str, password: &str, from: &str| {
        env.client
            .post(format!("{}/signin", env.base))
            .header("x-forwarded-for", from.to_string())
            .form(&[
                ("name", name.to_string()),
                ("password", password.to_string()),
            ])
            .send()
    };
    for n in 0..5 {
        let r = attempt(
            &format!("ghost{n}"),
            "wrong wrong wrong",
            "203.0.113.9, 127.0.0.1",
        )
        .await
        .unwrap();
        assert_eq!(
            r.status(),
            401,
            "the proxy's own address at the end of the list is skipped"
        );
    }
    let r = attempt("robin", "correct horse battery", "203.0.113.9")
        .await
        .unwrap();
    assert_eq!(r.status(), 429, "the client behind the proxy is limited");
    let r = attempt("robin", "correct horse battery", "203.0.113.10")
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        303,
        "another client behind the same proxy is not"
    );
    let admin = cookie_of(&r);

    for n in 0..5 {
        let r = attempt(
            "mira",
            "wrong wrong wrong",
            &format!("203.0.113.{}", 20 + n),
        )
        .await
        .unwrap();
        assert_eq!(r.status(), 401, "try {n} on the name is still answered");
    }
    let r = attempt("mira", "wrong wrong wrong", "203.0.113.30")
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        429,
        "five tries on one name from anywhere, the sixth waits"
    );
    assert!(
        r.headers()["retry-after"]
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            >= 1
    );
    let r = attempt("someone else", "wrong wrong wrong", "203.0.113.31")
        .await
        .unwrap();
    assert_eq!(r.status(), 401, "another name is not held up");
    let attempts: serde_json::Value = env
        .client
        .get(format!("{}/api/signin-attempts", env.base))
        .header("cookie", &admin)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        attempts["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["ip"] == "203.0.113.9"),
        "the attempts carry the client address: {attempts}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_admin_needs_the_setup_code_and_is_made_once() {
    let Some(env) = start(AuthMode::Local, vec![]).await else {
        return;
    };
    let code = setup_code(&env).await;
    assert_eq!(code.len(), 11);
    let page = env
        .client
        .get(format!("{}/signin", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("name=\"code\"") && !page.contains(&code),
        "the page asks for the code and never shows it"
    );
    assert_eq!(
        first_admin(&env, "robin", "").await.status(),
        403,
        "no code, no admin"
    );
    assert_eq!(
        first_admin(&env, "robin", "wrong-code1").await.status(),
        403
    );
    assert_eq!(env.store().user_count().await.unwrap(), 0);
    let mut tries = Vec::new();
    for i in 0..3 {
        let e = &env;
        let c = code.clone();
        tries.push(async move {
            first_admin(e, &format!("admin{i}"), &c)
                .await
                .status()
                .as_u16()
        });
    }
    let results = futures::future::join_all(tries).await;
    let made = results.iter().filter(|s| **s == 303).count();
    assert_eq!(
        made, 1,
        "exactly one of three at once became the admin: {results:?}"
    );
    assert!(
        results.iter().all(|s| *s == 303 || *s == 409),
        "{results:?}"
    );
    assert_eq!(env.store().user_count().await.unwrap(), 1);
    assert!(
        env.store()
            .setting(reel_server::auth::SETUP_CODE_KEY)
            .await
            .unwrap()
            .is_none(),
        "the code is spent"
    );
}

fn throwaway_signing_pem() -> &'static str {
    static PEM: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PEM.get_or_init(|| {
        use rsa::pkcs1::EncodeRsaPrivateKey;
        let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).expect("a throwaway key");
        key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
            .expect("pem")
            .to_string()
    })
}

#[derive(Clone)]
struct Persona {
    sub: String,
    username: String,
    email: Option<String>,
    email_verified: Option<bool>,
    groups: Vec<String>,
}

impl Persona {
    fn mira() -> Self {
        Persona {
            sub: "sub-mira".into(),
            username: "mira".into(),
            email: None,
            email_verified: None,
            groups: vec!["players".into(), "reel-admins".into()],
        }
    }
}

type Personas = Arc<tokio::sync::Mutex<Persona>>;

fn persona(p: Persona) -> Personas {
    Arc::new(tokio::sync::Mutex::new(p))
}

async fn mock_issuer(persona: Personas) -> String {
    use axum::extract::{Form, Query, State};
    use axum::response::{IntoResponse, Redirect};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use openidconnect::core::{
        CoreGenderClaim, CoreJsonWebKeySet, CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm, CoreProviderMetadata, CoreResponseType, CoreRsaPrivateSigningKey,
        CoreSubjectIdentifierType, CoreTokenType,
    };
    use openidconnect::{
        AccessToken, Audience, AuthUrl, EmptyAdditionalProviderMetadata, EmptyExtraTokenFields,
        EndUserEmail, EndUserUsername, IdToken, IdTokenClaims, IdTokenFields, IssuerUrl,
        JsonWebKeyId, JsonWebKeySetUrl, Nonce, PrivateSigningKey, ResponseTypes, StandardClaims,
        StandardTokenResponse, SubjectIdentifier, TokenUrl,
    };
    use reel_server::accounts::ExtraClaims;
    use std::collections::HashMap;

    #[derive(Clone)]
    struct Issuer {
        base: String,
        key: Arc<CoreRsaPrivateSigningKey>,
        nonce: Arc<tokio::sync::Mutex<Option<String>>>,
        persona: Personas,
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let issuer = Issuer {
        base: base.clone(),
        key: Arc::new(
            CoreRsaPrivateSigningKey::from_pem(
                throwaway_signing_pem(),
                Some(JsonWebKeyId::new("test".into())),
            )
            .expect("the test key"),
        ),
        nonce: Arc::new(tokio::sync::Mutex::new(None)),
        persona,
    };
    let app = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(|State(s): State<Issuer>| async move {
                let md = CoreProviderMetadata::new(
                    IssuerUrl::new(s.base.clone()).unwrap(),
                    AuthUrl::new(format!("{}/authorize", s.base)).unwrap(),
                    JsonWebKeySetUrl::new(format!("{}/jwks", s.base)).unwrap(),
                    vec![ResponseTypes::new(vec![CoreResponseType::Code])],
                    vec![CoreSubjectIdentifierType::Public],
                    vec![CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256],
                    EmptyAdditionalProviderMetadata {},
                )
                .set_token_endpoint(Some(TokenUrl::new(format!("{}/token", s.base)).unwrap()));
                Json(serde_json::to_value(md).unwrap())
            }),
        )
        .route(
            "/jwks",
            get(|State(s): State<Issuer>| async move {
                Json(
                    serde_json::to_value(CoreJsonWebKeySet::new(vec![s.key.as_verification_key()]))
                        .unwrap(),
                )
            }),
        )
        .route(
            "/authorize",
            get(
                |State(s): State<Issuer>, Query(q): Query<HashMap<String, String>>| async move {
                    *s.nonce.lock().await = q.get("nonce").cloned();
                    Redirect::to(&format!(
                        "{}?code=code-1&state={}",
                        q["redirect_uri"], q["state"]
                    ))
                    .into_response()
                },
            ),
        )
        .route(
            "/token",
            post(
                |State(s): State<Issuer>, Form(f): Form<HashMap<String, String>>| async move {
                    assert_eq!(f.get("code").map(String::as_str), Some("code-1"));
                    let nonce = s.nonce.lock().await.clone().expect("authorize ran first");
                    let who = s.persona.lock().await.clone();
                    let mut extra = serde_json::Map::new();
                    extra.insert("groups".into(), serde_json::json!(who.groups));
                    let mut standard = StandardClaims::new(SubjectIdentifier::new(who.sub.clone()))
                        .set_preferred_username(Some(EndUserUsername::new(who.username.clone())));
                    if let Some(email) = &who.email {
                        standard = standard
                            .set_email(Some(EndUserEmail::new(email.clone())))
                            .set_email_verified(who.email_verified);
                    }
                    let claims = IdTokenClaims::<ExtraClaims, CoreGenderClaim>::new(
                        IssuerUrl::new(s.base.clone()).unwrap(),
                        vec![Audience::new("reel".into())],
                        chrono::Utc::now() + chrono::Duration::minutes(5),
                        chrono::Utc::now(),
                        standard,
                        ExtraClaims(extra),
                    )
                    .set_nonce(Some(Nonce::new(nonce)));
                    let access = AccessToken::new("access-1".into());
                    let id_token = IdToken::<
                        ExtraClaims,
                        CoreGenderClaim,
                        CoreJweContentEncryptionAlgorithm,
                        CoreJwsSigningAlgorithm,
                    >::new(
                        claims,
                        &*s.key,
                        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
                        Some(&access),
                        None,
                    )
                    .unwrap();
                    let resp = StandardTokenResponse::new(
                        access,
                        CoreTokenType::Bearer,
                        IdTokenFields::new(Some(id_token), EmptyExtraTokenFields {}),
                    );
                    Json(serde_json::to_value(resp).unwrap())
                },
            ),
        )
        .with_state(issuer);
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    base
}

#[tokio::test(flavor = "multi_thread")]
async fn an_identity_provider_signs_in_and_its_claim_makes_the_admin() {
    std::env::set_var("REEL_OIDC_CLIENT_SECRET", "secret-1");
    let issuer = mock_issuer(persona(Persona::mira())).await;
    let i = issuer.clone();
    let Some(env) = start_with(AuthMode::Oidc, vec![], move |c| {
        c.auth.oidc_issuer = i;
        c.auth.oidc_client_id = "reel".into();
        c.auth.oidc_role_claim = "groups".into();
        c.auth.oidc_admin_value = "reel-admins".into();
    })
    .await
    else {
        return;
    };
    let page = env
        .client
        .get(format!("{}/signin", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("Sign in with 127.0.0.1") && page.contains("/oidc/start"),
        "{page}"
    );
    assert_eq!(
        env.client
            .get(format!("{}/", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        303
    );

    let r = env
        .client
        .get(format!("{}/oidc/start", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303);
    let to_issuer = r.headers()["location"].to_str().unwrap().to_string();
    assert!(
        to_issuer.starts_with(&format!("{issuer}/authorize?")),
        "{to_issuer}"
    );
    assert!(
        to_issuer.contains("code_challenge=") && to_issuer.contains("nonce="),
        "pkce and a nonce: {to_issuer}"
    );
    let pending = cookie_of(&r);
    let r = env.client.get(&to_issuer).send().await.unwrap();
    assert_eq!(r.status(), 303);
    let back = r.headers()["location"].to_str().unwrap().to_string();
    assert!(
        back.starts_with(&format!("{}/oidc/callback?", env.base)),
        "{back}"
    );
    let r = env
        .client
        .get(&back)
        .header("cookie", &pending)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303, "{}", r.text().await.unwrap_or_default());
    assert_eq!(r.headers()["location"], "/");
    let signed = cookie_of(&r);
    assert_ne!(signed, pending, "the session id is rotated at sign-in");
    let lib = env
        .client
        .get(format!("{}/", env.base))
        .header("cookie", &signed)
        .send()
        .await
        .unwrap();
    assert_eq!(lib.status(), 200);
    let mira = env
        .store()
        .user_by_name("mira")
        .await
        .unwrap()
        .expect("a row on first sight");
    assert!(mira.is_admin(), "the groups claim made an admin");
    assert!(
        mira.password_hash.is_none(),
        "no password for an identity the provider vouches for"
    );
    let r = env
        .client
        .get(&back)
        .header("cookie", &signed)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status(),
        400,
        "a callback without a pending sign-in is refused"
    );
    let r = env
        .client
        .post(format!("{}/signout", env.base))
        .header("cookie", &signed)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303);
    assert_eq!(
        env.client
            .get(format!("{}/", env.base))
            .header("cookie", &signed)
            .send()
            .await
            .unwrap()
            .status(),
        303
    );
}

/// Each call comes from its own forwarded address, so the sign-in limit does not count a
/// test's many visitors together.
async fn oidc_sign_in(env: &Env) -> (u16, String) {
    static VISITOR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    let n = VISITOR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let r = env
        .client
        .get(format!("{}/oidc/start", env.base))
        .header("x-forwarded-for", format!("203.0.113.{}", n % 250 + 1))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303);
    let to_issuer = r.headers()["location"].to_str().unwrap().to_string();
    let pending = cookie_of(&r);
    let r = env.client.get(&to_issuer).send().await.unwrap();
    let back = r.headers()["location"].to_str().unwrap().to_string();
    let r = env
        .client
        .get(&back)
        .header("cookie", &pending)
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    let cookie = if status == 303 {
        cookie_of(&r)
    } else {
        String::new()
    };
    (status, cookie)
}

async fn oidc_env(persona: Personas, tweak: impl FnOnce(&mut Config)) -> Option<(Env, String)> {
    std::env::set_var("REEL_OIDC_CLIENT_SECRET", "secret-1");
    let issuer = mock_issuer(persona).await;
    let i = issuer.clone();
    let env = start_with(AuthMode::Oidc, vec!["127.0.0.1/32"], move |c| {
        c.auth.oidc_issuer = i;
        c.auth.oidc_client_id = "reel".into();
        tweak(c);
    })
    .await?;
    Some((env, issuer))
}

#[tokio::test(flavor = "multi_thread")]
async fn an_identity_is_bound_to_its_subject_not_its_name() {
    let who = persona(Persona {
        sub: "sub-1".into(),
        username: "mira".into(),
        groups: vec![],
        ..Persona::mira()
    });
    let Some((env, issuer)) = oidc_env(who.clone(), |c| {
        c.auth.oidc_role_claim = "groups".into();
        c.auth.oidc_admin_value = "reel-admins".into();
        c.auth.admins = vec!["pat".into()];
    })
    .await
    else {
        return;
    };
    let local = reel_store::models::User::new("mira", "admin", Some("hash".into()));
    env.store().insert_user(&local).await.unwrap();

    let (status, cookie) = oidc_sign_in(&env).await;
    assert_eq!(status, 303);
    assert_eq!(env.store().users().await.unwrap().len(), 2);
    let row = env
        .store()
        .user_by_oidc(&issuer, "sub-1")
        .await
        .unwrap()
        .expect("a row bound to issuer and subject");
    assert_eq!(
        row.name, "mira 2",
        "the local mira keeps her row and her name"
    );
    assert!(!row.is_admin(), "no claim, not listed, not the first user");
    let local_now = env.store().user(&local.id).await.unwrap().unwrap();
    assert_eq!(local_now.password_hash.as_deref(), Some("hash"));
    assert!(local_now.is_admin());
    let lib = env
        .client
        .get(format!("{}/", env.base))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(lib.contains("mira 2"), "the header names the new row");

    who.lock().await.username = "someone else".into();
    assert_eq!(oidc_sign_in(&env).await.0, 303);
    assert_eq!(env.store().users().await.unwrap().len(), 2);
    let same = env
        .store()
        .user_by_oidc(&issuer, "sub-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (same.id, same.name),
        (row.id.clone(), "mira 2".into()),
        "a renamed person is the same row"
    );

    {
        let mut p = who.lock().await;
        p.sub = "sub-2".into();
        p.username = "mira".into();
    }
    assert_eq!(oidc_sign_in(&env).await.0, 303);
    let other = env
        .store()
        .user_by_oidc(&issuer, "sub-2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        other.name, "mira 3",
        "another subject with the same name is another row"
    );
    assert!(!other.is_admin());

    {
        let mut p = who.lock().await;
        p.sub = "sub-1".into();
        p.groups = vec!["reel-admins".into()];
    }
    assert_eq!(oidc_sign_in(&env).await.0, 303);
    assert!(
        env.store().user(&row.id).await.unwrap().unwrap().is_admin(),
        "the claim promotes"
    );
    env.store().set_role(&local.id, "member").await.unwrap();
    who.lock().await.groups = vec![];
    assert_eq!(oidc_sign_in(&env).await.0, 303);
    assert!(
        env.store().user(&row.id).await.unwrap().unwrap().is_admin(),
        "the last admin is not demoted"
    );
    env.store().set_role(&local.id, "admin").await.unwrap();
    assert_eq!(oidc_sign_in(&env).await.0, 303);
    assert!(
        !env.store().user(&row.id).await.unwrap().unwrap().is_admin(),
        "the claim demotes once another admin exists"
    );

    {
        let mut p = who.lock().await;
        p.sub = "sub-pat".into();
        p.username = "pat".into();
        p.groups = vec![];
    }
    assert_eq!(oidc_sign_in(&env).await.0, 303);
    let pat = env
        .store()
        .user_by_oidc(&issuer, "sub-pat")
        .await
        .unwrap()
        .unwrap();
    assert!(pat.is_admin(), "the Admins list applies at first sight");
    assert_eq!(oidc_sign_in(&env).await.0, 303);
    assert!(
        env.store().user(&pat.id).await.unwrap().unwrap().is_admin(),
        "and keeps the listed name an admin without the claim"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unverified_email_is_not_a_name() {
    let who = persona(Persona {
        sub: "sub-9".into(),
        username: "nine".into(),
        email: Some("nine@example.org".into()),
        email_verified: Some(false),
        groups: vec![],
    });
    let Some((env, issuer)) = oidc_env(who.clone(), |c| {
        c.auth.oidc_username_claim = "email".into();
    })
    .await
    else {
        return;
    };
    assert_eq!(oidc_sign_in(&env).await.0, 303);
    let row = env
        .store()
        .user_by_oidc(&issuer, "sub-9")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.name, "sub-9",
        "an unverified email falls back to the subject"
    );
    {
        let mut p = who.lock().await;
        p.sub = "sub-10".into();
        p.email_verified = None;
    }
    assert_eq!(oidc_sign_in(&env).await.0, 303);
    assert_eq!(
        env.store()
            .user_by_oidc(&issuer, "sub-10")
            .await
            .unwrap()
            .unwrap()
            .name,
        "sub-10",
        "no word on it counts as unverified"
    );
    {
        let mut p = who.lock().await;
        p.sub = "sub-11".into();
        p.email_verified = Some(true);
    }
    assert_eq!(oidc_sign_in(&env).await.0, 303);
    assert_eq!(
        env.store()
            .user_by_oidc(&issuer, "sub-11")
            .await
            .unwrap()
            .unwrap()
            .name,
        "nine@example.org"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pending_sign_in_expires_in_minutes_and_start_is_limited() {
    let Some((env, _issuer)) = oidc_env(persona(Persona::mira()), |_| {}).await else {
        return;
    };
    let minutes_left = || async {
        sqlx::query_scalar::<_, f64>(
            "SELECT (julianday(expiry_date) - julianday('now')) * 1440.0 FROM tower_sessions ORDER BY expiry_date DESC LIMIT 1",
        )
        .fetch_one(env.store().pool())
        .await
        .unwrap()
    };
    let r = env
        .client
        .get(format!("{}/oidc/start", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303);
    let pending = cookie_of(&r);
    let waiting = minutes_left().await;
    assert!(
        waiting > 0.0 && waiting <= 10.5,
        "a sign-in that waits for the provider lives ten minutes: {waiting}"
    );
    let to_issuer = r.headers()["location"].to_str().unwrap().to_string();
    let back = env.client.get(&to_issuer).send().await.unwrap().headers()["location"]
        .to_str()
        .unwrap()
        .to_string();
    let r = env
        .client
        .get(&back)
        .header("cookie", &pending)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303);
    let signed = minutes_left().await;
    assert!(
        signed > 29.0 * 1440.0,
        "signed in, the session gets its full expiry back: {signed}"
    );
    let mut last = 0;
    for _ in 0..6 {
        last = env
            .client
            .get(format!("{}/oidc/start", env.base))
            .send()
            .await
            .unwrap()
            .status()
            .as_u16();
        if last == 429 {
            break;
        }
    }
    assert_eq!(last, 429, "oidc start sits under the sign-in limit");
}

#[tokio::test(flavor = "multi_thread")]
async fn first_run_gates_admins_until_it_is_done() {
    let Some(env) = start_layered(AuthMode::Single, vec![], |_| {}, None, false).await else {
        return;
    };
    let r = env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 303);
    assert_eq!(r.headers()["location"], "/setup");
    let r = env
        .client
        .get(format!("{}/api/settings", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "the API is not gated");
    let page = env
        .client
        .get(format!("{}/setup", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("Welcome") && page.contains("data-key=\"site_name\""));
    let (status, d) = patch_setting(&env, serde_json::json!({ "site_name": "my vault" })).await;
    assert_eq!(status, 200, "{d}");
    let folders: serde_json::Value = env
        .client
        .get(format!("{}/api/setup/checks", env.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let incoming = folders["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["key"] == "ingest.incoming_dir")
        .unwrap();
    assert_eq!(incoming["ok"], true, "{folders}");
    let page = env
        .client
        .get(format!("{}/setup?step=3", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("every re-encode on the CPU"),
        "doctor is in the hardware step"
    );
    let (status, d) = post_json(&env, "/api/setup/url-check").await;
    assert_eq!(status, 200);
    assert_eq!(d["ok"], true, "the test server reaches itself: {d}");
    let page = env
        .client
        .get(format!("{}/setup?step=6", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("my vault"),
        "the summary shows the saved name"
    );
    let (status, _) = post_json(&env, "/api/setup/done").await;
    assert_eq!(status, 200);
    assert_eq!(
        env.client
            .get(format!("{}/", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        200,
        "the gate lifts"
    );
    let again = env
        .client
        .get(format!("{}/setup", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 200, "the wizard stays reachable");
    assert!(again.text().await.unwrap().contains("first run, again"));
}

type Notes = Arc<tokio::sync::Mutex<Vec<(String, String, serde_json::Value)>>>;

/// Authorization header.
async fn mock_receiver() -> (String, Notes) {
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    let got: Notes = Arc::new(tokio::sync::Mutex::new(vec![]));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let auth = |h: &HeaderMap| {
        h.get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    };
    let app = Router::new()
        .route(
            "/",
            post(
                move |State(got): State<Notes>,
                      h: HeaderMap,
                      Json(body): Json<serde_json::Value>| async move {
                    got.lock().await.push(("/".into(), auth(&h), body));
                    "ok"
                },
            ),
        )
        .route(
            "/hook",
            post(
                move |State(got): State<Notes>,
                      h: HeaderMap,
                      Json(body): Json<serde_json::Value>| async move {
                    got.lock().await.push(("/hook".into(), auth(&h), body));
                    "ok"
                },
            ),
        )
        .with_state(got.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (base, got)
}

async fn wait_note(
    got: &Notes,
    pred: impl Fn(&(String, String, serde_json::Value)) -> bool,
) -> (String, String, serde_json::Value) {
    let start = Instant::now();
    loop {
        if let Some(n) = got.lock().await.iter().find(|n| pred(n)).cloned() {
            return n;
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "no such note among {:?}",
            got.lock()
                .await
                .iter()
                .map(|n| n.2.clone())
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn notes_reach_a_webhook_and_an_ntfy_topic() {
    let (target, got) = mock_receiver().await;
    let t = target.clone();
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.notify.kind = "webhook".into();
        c.notify.webhook_url = format!("{t}/hook");
        c.storage.disk_warn_gb = 0;
    })
    .await
    else {
        return;
    };
    let (status, d) = post_json(&env, "/api/notify/test").await;
    assert_eq!(status, 200, "{d}");
    assert!(d["line"].as_str().unwrap().contains("200"), "{d}");
    let n = wait_note(&got, |n| n.2["event"] == "test").await;
    assert_eq!(n.0, "/hook");
    assert_eq!(n.2["site"], "reel");
    assert!(n.2["body"].as_str().unwrap().contains("Sent by"));

    std::fs::write(env.incoming().join("broken.mkv"), b"not a video at all").unwrap();
    let n = wait_note(&got, |n| n.2["event"] == "file_unreadable").await;
    assert!(
        n.2["body"].as_str().unwrap().contains("broken.mkv"),
        "{}",
        n.2
    );
    assert!(n.2["link"].as_str().unwrap().ends_with("/jobs?show=failed"));

    for (k, v) in [
        ("notify.ntfy_url", target.as_str()),
        ("notify.ntfy_topic", "reel"),
        ("notify.ntfy_token", "tok-1"),
        ("notify.kind", "ntfy"),
    ] {
        let (status, d) = patch_setting(&env, serde_json::json!({ k: v })).await;
        assert_eq!(status, 200, "{k}: {d}");
    }
    let (status, d) = post_json(&env, "/api/notify/test").await;
    assert_eq!(status, 200, "{d}");
    assert!(d["line"].as_str().unwrap().contains("topic reel"), "{d}");
    let n = wait_note(&got, |n| n.0 == "/" && n.2["topic"] == "reel").await;
    assert_eq!(n.1, "Bearer tok-1", "the token goes as a bearer");
    assert!(n.2["title"].as_str().unwrap().contains("a test"));
    assert_eq!(n.2["tags"][0], "white_check_mark");

    let (status, d) = patch_setting(
        &env,
        serde_json::json!({ "notify.events": "update_available" }),
    )
    .await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(
        d["words"], "an update is available",
        "the answer says the value in words"
    );
    assert_eq!(
        d["default_text"], "a job failed, a file could not be read, the disk is low, the external storage needs a look, the library cap moved clips",
        "and the default in words, never the stored values"
    );
    env.ctx.emit(reel_core::Event::Update {
        version: "99.0.0".into(),
        url: "https://example.org/reel/releases/v99.0.0".into(),
    });
    let n = wait_note(&got, |n| {
        n.2["title"].as_str().unwrap_or("").contains("99.0.0")
    })
    .await;
    assert_eq!(n.2["click"], "https://example.org/reel/releases/v99.0.0");

    let (status, _) = patch_setting(&env, serde_json::json!({ "notify.events": "disk_low" })).await;
    assert_eq!(status, 200);
    let (status, d) =
        patch_setting(&env, serde_json::json!({ "storage.disk_warn_gb": 10000 })).await;
    assert_eq!(status, 200, "{d}");
    reel_pipeline::housekeeping::once(&env.ctx).await.unwrap();
    wait_note(&got, |n| {
        n.2["title"]
            .as_str()
            .unwrap_or("")
            .contains("under 10000 GB")
    })
    .await;
    reel_pipeline::housekeeping::once(&env.ctx).await.unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    let disk_notes = got
        .lock()
        .await
        .iter()
        .filter(|n| {
            n.2["title"]
                .as_str()
                .unwrap_or("")
                .contains("under 10000 GB")
        })
        .count();
    assert_eq!(disk_notes, 1, "the disk note goes once per crossing");

    let (status, d) = patch_setting(
        &env,
        serde_json::json!({ "notify.webhook_url": "http://example.org/hook" }),
    )
    .await;
    assert_eq!(status, 400, "plain http to the internet is refused: {d}");
    let (status, _) = patch_setting(
        &env,
        serde_json::json!({ "notify.webhook_url": "https://example.org/hook" }),
    )
    .await;
    assert_eq!(status, 200);
    let (status, d) = patch_setting(&env, serde_json::json!({ "notify.events": "nope" })).await;
    assert_eq!(status, 400, "{d}");
    let page = env
        .client
        .get(format!("{}/settings/notifications", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let events = page
        .split("data-key=\"notify.events\"")
        .nth(1)
        .and_then(|rest| rest.split("class=\"panel").next())
        .unwrap_or_default();
    assert!(
        events.contains("data-opt=\"link_made\"")
            && events.contains("A link was made")
            && events.contains("data-opt=\"library_cap\"")
            && events.matches("role=\"switch\"").count() == 7,
        "each event is a row of its own with a switch"
    );
    assert!(
        !events.contains("type=\"checkbox\""),
        "no checkbox in a settings row"
    );
    assert!(
        events.contains("changed from the defaults") && !events.contains("was update_available"),
        "the changed badge says it in words"
    );
}

async fn scan_folder(env: &Env, path: &str) -> (u16, serde_json::Value) {
    scan_folder_as(env, path, "").await
}

async fn scan_folder_as(env: &Env, path: &str, mode: &str) -> (u16, serde_json::Value) {
    let r = env
        .client
        .post(format!("{}/api/imports/scan", env.base))
        .json(&serde_json::json!({ "path": path, "mode": mode }))
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    (
        status,
        r.json::<serde_json::Value>().await.unwrap_or_default(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_scan_copies_its_clips_skips_them_next_time_and_moves_on_request() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let other = tempfile::tempdir().unwrap();
    env.synth_at(
        other.path(),
        "2026-10-01 20-00-00.mkv",
        Synth::small(2.0, "one"),
    )
    .await;
    env.synth_at(
        other.path(),
        "2026-10-02 20-00-00.mkv",
        Synth::small(2.0, "two"),
    )
    .await;
    std::fs::write(other.path().join("notes.txt"), b"not a clip").unwrap();
    std::fs::create_dir(other.path().join("deeper")).unwrap();
    let (status, d) = scan_folder(&env, &other.path().to_string_lossy()).await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(
        (d["queued"].as_u64(), d["skipped"].as_u64()),
        (Some(2), Some(2)),
        "{d}"
    );
    let start = Instant::now();
    loop {
        let ready = env
            .store()
            .clips("", reel_store::Sort::Newest)
            .await
            .unwrap()
            .into_iter()
            .filter(|c| c.clip.state == "ready" && c.clip.source == "imported")
            .count();
        if ready == 2 {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "both clips land"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let left = std::fs::read_dir(other.path()).unwrap().count();
    assert_eq!(
        left, 4,
        "the folder stays as it was: the clips were copied in"
    );
    let (status, d) = scan_folder(&env, &other.path().to_string_lossy()).await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(
        (d["queued"].as_u64(), d["already"].as_u64()),
        (Some(0), Some(2)),
        "a second scan skips what it imported before: {d}"
    );
    let lib = env
        .client
        .get(format!("{}/", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(lib.contains("imported"), "the cards say imported");

    let taken = tempfile::tempdir().unwrap();
    env.synth_at(
        taken.path(),
        "2026-10-03 20-00-00.mkv",
        Synth::small(2.0, "three"),
    )
    .await;
    let (status, d) = scan_folder_as(&env, &taken.path().to_string_lossy(), "move").await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(d["queued"].as_u64(), Some(1));
    assert!(
        d["line"].as_str().unwrap().contains("moved into reel"),
        "{d}"
    );
    let start = Instant::now();
    loop {
        let ready = env
            .store()
            .clips("", reel_store::Sort::Newest)
            .await
            .unwrap()
            .into_iter()
            .filter(|c| c.clip.state == "ready" && c.clip.source == "imported")
            .count();
        if ready == 3 {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "the moved clip lands"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    assert_eq!(
        std::fs::read_dir(taken.path()).unwrap().count(),
        0,
        "Move into reel takes the file"
    );

    let (status, d) = scan_folder(&env, &env.incoming().to_string_lossy()).await;
    assert_eq!(status, 400, "the incoming folder is watched already: {d}");
    let (status, _) = scan_folder(&env, "/nowhere/at/all").await;
    assert_eq!(status, 400);
    let (status, _) = scan_folder(&env, "").await;
    assert_eq!(status, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_library_search_marks_cards_for_the_browser_and_filters_on_the_server() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("2026-10-01 20-00-00.mkv", Synth::small(2.0, "one"))
        .await;
    let a = env.wait_ready(Duration::from_secs(120)).await;
    env.synth("2026-10-02 20-00-00.mkv", Synth::small(2.0, "two"))
        .await;
    let b = env
        .wait_clip(
            |c| c.state == "ready" && c.id != a.id,
            Duration::from_secs(120),
        )
        .await;
    for (id, title, description) in [(&a.id, "Hot Game", "Night RAID"), (&b.id, "Calm Walk", "")] {
        let r = env
            .client
            .patch(format!("{}/api/clips/{id}", env.base))
            .json(&serde_json::json!({ "title": title, "description": description }))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
    }
    let library = |q: &str| {
        let path = if q.is_empty() {
            "/".to_string()
        } else {
            format!("/?q={q}")
        };
        let url = format!("{}{path}", env.base);
        let client = env.client.clone();
        async move { client.get(url).send().await.unwrap().text().await.unwrap() }
    };
    let page = library("").await;
    assert!(
        page.contains("data-text=\"hot game\nnight raid\""),
        "every card carries its lowercased title and description"
    );
    assert!(page.contains("data-text=\"calm walk\n\""));
    let page = library("RAID").await;
    assert!(
        page.contains(&a.id) && !page.contains(&b.id),
        "the server filters without JavaScript"
    );
    assert!(page.contains("value=\"RAID\""), "the box carries the query");
    assert!(
        page.contains("1 match for &#34;RAID&#34;")
            || page.contains("1 match for &quot;RAID&quot;"),
        "the count line says how many match"
    );
    let page = library("m_w").await;
    assert!(
        page.contains("No clip matches") && !page.contains(&b.id),
        "an underscore is a character, not a wildcard"
    );
    let page = library("m%20w").await;
    assert!(page.contains(&b.id) && !page.contains(&a.id));
    let page = library("night%20raid&sort=title").await;
    assert!(
        page.contains(&a.id) && page.contains("value=\"title\" selected"),
        "the sort stays"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_download_select_offers_the_original_and_only_the_limits_it_does_not_fit() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("tiny.mp4", Synth::small(2.0, "tiny")).await;
    let tiny = env.wait_ready(Duration::from_secs(90)).await;
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, tiny.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("<select id=\"quality\""),
        "one select next to Download"
    );
    assert!(
        page.contains("<option value=\"original\" data-ready=\"1\">original as it is, "),
        "the original first, with its size"
    );
    assert!(
        !page.contains("Under 1 MB") && !page.contains("Under 10 MB"),
        "a limit the original fits under is not offered"
    );
    assert!(page.contains("already under 1 MB"), "and the hint says so");
    assert!(!page.contains("Download for Discord"));

    env.synth("big.mp4", Synth::h264_1080p(5.0, "big")).await;
    let big = env
        .wait_clip(
            |c| c.state == "ready" && c.id != tiny.id,
            Duration::from_secs(90),
        )
        .await;
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, big.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("<option value=\"1\">Under 1 MB</option>"),
        "{page}"
    );
    assert!(
        !page.contains("Under 10 MB") && page.contains("already under 10 MB"),
        "the original covers 10 MB"
    );
    let url = format!("{}/media/clips/{}/discord.mp4?limit=1", env.base, big.id);
    let first = env.client.get(&url).send().await.unwrap();
    assert_eq!(first.status(), 202);
    let info: serde_json::Value = first.json().await.unwrap();
    env.wait_job_done(info["job_id"].as_str().unwrap(), Duration::from_secs(120))
        .await;
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, big.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("<option value=\"1\" data-ready=\"1\">Under 1 MB, ready, "),
        "a copy that exists is marked"
    );
}

fn audio_md5(path: &std::path::Path) -> String {
    let (ff, _) = ffmpeg_paths().expect("ffmpeg");
    let out = std::process::Command::new(ff)
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-c", "copy", "-f", "md5", "-"])
        .output()
        .expect("ffmpeg runs");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn copies_keep_the_aac_audio_whole_and_follow_the_ladder() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| {
        c.media.x264_preset = "faster".into();
    })
    .await
    else {
        return;
    };
    env.synth("2026-10-05 21-00-00.mkv", Synth::obs(12.0, "obs"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(120)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(master.size.unwrap() > 10_000_000, "{:?}", master.size);
    let master_path = env.ctx.config().abs(master.path.as_deref().unwrap());
    let source_audio = audio_md5(&master_path);
    assert!(source_audio.starts_with("MD5="), "{source_audio}");

    let url = format!("{}/media/clips/{}/discord.mp4?limit=10", env.base, clip.id);
    let first = env.client.get(&url).send().await.unwrap();
    assert_eq!(first.status(), 202);
    let info: serde_json::Value = first.json().await.unwrap();
    let j = env
        .wait_job_done(info["job_id"].as_str().unwrap(), Duration::from_secs(300))
        .await;
    assert_eq!(j.state, "done", "{:?}", j.error);
    let copy = env
        .store()
        .rendition(info["rendition_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(copy.size.unwrap() <= 10_000_000, "{:?}", copy.size);
    assert_eq!(
        copy.height,
        Some(720),
        "the ladder's rung for 12 s under 10 MB"
    );
    assert!((copy.fps.unwrap() - 60.0).abs() < 0.5, "{:?}", copy.fps);
    assert_eq!(copy.acodec.as_deref(), Some("aac"));
    let copy_path = env.ctx.config().abs(copy.path.as_deref().unwrap());
    assert_eq!(
        audio_md5(&copy_path),
        source_audio,
        "the AAC stream is copied bit for bit"
    );
    let reason = copy.reason.clone().unwrap_or_default();
    assert!(
        reason.starts_with("720p60 at ") && reason.contains("audio copied at "),
        "{reason}"
    );
    assert!(
        j.route.as_deref().unwrap_or("").contains("faster"),
        "{:?}",
        j.route
    );
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("Under 10 MB: 720p60 at "),
        "the clip page shows the copy's reason"
    );

    let url = format!("{}/media/clips/{}/discord.mp4?limit=1", env.base, clip.id);
    let first = env.client.get(&url).send().await.unwrap();
    assert_eq!(
        first.status(),
        202,
        "{}",
        first.text().await.unwrap_or_default()
    );
    let info: serde_json::Value = first.json().await.unwrap();
    let j = env
        .wait_job_done(info["job_id"].as_str().unwrap(), Duration::from_secs(300))
        .await;
    assert_eq!(j.state, "done", "{:?}", j.error);
    let small = env
        .store()
        .rendition(info["rendition_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(small.size.unwrap() <= 1_000_000, "{:?}", small.size);
    assert_eq!(
        (small.height, small.fps.map(|f| f.round() as u32)),
        (Some(480), Some(30))
    );
    let reason = small.reason.clone().unwrap_or_default();
    assert!(
        reason.ends_with("; audio lowered to 128 kbit/s to fit 1 MB"),
        "{reason}"
    );

    env.synth("2026-10-06 21-00-00.mkv", Synth::obs(30.0, "long"))
        .await;
    let long = env
        .wait_clip(
            |c| c.state == "ready" && c.id != clip.id,
            Duration::from_secs(120),
        )
        .await;
    let refused = env
        .client
        .get(format!(
            "{}/media/clips/{}/discord.mp4?limit=1",
            env.base, long.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 400);
    let why: serde_json::Value = refused.json().await.unwrap();
    assert!(
        why["error"]
            .as_str()
            .unwrap()
            .contains("does not fit under 1 MB"),
        "{why}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn storage_actions_purge_clear_rebuild_and_refresh() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("big.mp4", Synth::h264_1080p(5.0, "big")).await;
    let big = env.wait_ready(Duration::from_secs(90)).await;
    env.synth("gone.mp4", Synth::small(2.0, "gone")).await;
    let gone = env
        .wait_clip(
            |c| c.state == "ready" && c.id != big.id,
            Duration::from_secs(90),
        )
        .await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let url = format!("{}/media/clips/{}/discord.mp4?limit=1", env.base, big.id);
    let info: serde_json::Value = env
        .client
        .get(&url)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    env.wait_job_done(info["job_id"].as_str().unwrap(), Duration::from_secs(120))
        .await;
    let copy = env
        .store()
        .rendition(info["rendition_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    let copy_path = env.ctx.config().abs(copy.path.as_deref().unwrap());
    assert!(copy_path.exists());
    let (status, d) = post_json(&env, "/api/storage/clear-discord").await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(d["gone"], 1);
    assert!(!copy_path.exists(), "the copy's file is gone");
    assert!(env.store().discord_renditions().await.unwrap().is_empty());

    let r = env
        .client
        .delete(format!("{}/api/clips/{}", env.base, gone.id))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(
        env.store().clip(&gone.id).await.unwrap().is_some(),
        "in the trash, still a row"
    );
    let (status, d) = post_json(&env, "/api/storage/purge-trash").await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(d["purged"], 1);
    assert!(
        env.store().clip(&gone.id).await.unwrap().is_none(),
        "purged for good"
    );
    assert!(!env.ctx.config().trash_dir().join(&gone.id).exists());

    let master = env
        .store()
        .rendition(big.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let video = env.ctx.config().abs(master.path.as_deref().unwrap());
    let poster = reel_pipeline::derived::paths(&video).poster;
    let before = std::fs::metadata(&poster).unwrap().modified().unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let (status, d) = post_json(&env, "/api/storage/rebuild-derived").await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(d["queued"], 1, "one job per ready clip");
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let after = std::fs::metadata(&poster).unwrap().modified().unwrap();
    assert!(after > before, "the poster was made again");
    let derive = env.store().finished_jobs(10).await.unwrap();
    assert!(
        derive
            .iter()
            .any(|j| j.kind == "derive" && j.state == "done"),
        "{:?}",
        derive
            .iter()
            .map(|j| (&j.kind, &j.state))
            .collect::<Vec<_>>()
    );
    let (status, d) = post_json(&env, "/api/storage/disk-refresh").await;
    assert_eq!(status, 200);
    assert!(d["line"].as_str().unwrap().contains("data disk"), "{d}");
    let page = env
        .client
        .get(format!("{}/settings/storage", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("The trash")
            && page.contains("Purge now")
            && page.contains("Rebuild")
            && page.contains("data disk"),
        "the Storage section carries the actions and the disk line"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn advanced_has_the_logs_the_history_the_exports_and_a_restart() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    // without the server's own filter every crate's trace lines fill the ring
    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            "info,sqlx=warn,tower_http=info,hyper=warn",
        ))
        .with(reel_pipeline::logging::RingLayer)
        .try_init();
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    // The ring keeps the newest 2000 lines and the tests beside this one log into it too, so
    // each read looks for a line logged just before it and tries again until it holds it.
    let mut lines = Vec::new();
    let mut held = false;
    for n in 0..50 {
        let line = format!("a line from the test, {n}");
        tracing::info!(clip = "c-1", "{line}");
        let d: serde_json::Value = env
            .client
            .get(format!("{}/api/logs?lines=2000", env.base))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        lines = d["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect();
        held = lines
            .iter()
            .any(|l| l.contains("INFO") && l.contains(&line) && l.contains("clip=\"c-1\""));
        if held {
            break;
        }
    }
    assert!(
        held,
        "the last lines never held the one logged just before: {lines:?}"
    );
    let mut held = false;
    for n in 0..50 {
        let line = format!("a line for the hour, {n}");
        tracing::info!("{line}");
        let hour = env
            .client
            .get(format!("{}/api/logs/download", env.base))
            .send()
            .await
            .unwrap();
        assert!(hour.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .contains("reel-log-"));
        held = hour.text().await.unwrap().contains(&line);
        if held {
            break;
        }
    }
    assert!(
        held,
        "the last hour never held the line logged just before it"
    );

    let (status, d) =
        patch_setting(&env, serde_json::json!({ "advanced.log_level": "debug" })).await;
    assert_eq!(status, 200, "{d}");
    let (status, _) = patch_setting(&env, serde_json::json!({ "share.trash_days": 3 })).await;
    assert_eq!(status, 200);
    let h: serde_json::Value = env
        .client
        .get(format!("{}/api/settings/history", env.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let entry = h["history"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["key"] == "share.trash_days")
        .expect("the change is in the history");
    assert_eq!(entry["new"], "3");
    let old = entry["old"].clone();
    let (status, d) = patch_setting(&env, serde_json::json!({ "share.trash_days": old })).await;
    assert_eq!(status, 200, "revert: {d}");
    assert_eq!(
        env.ctx.config().share.trash_days,
        7,
        "reverted to the value under the table"
    );

    let db = env
        .client
        .get(format!("{}/api/export/database", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(db.status(), 200);
    assert!(db.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .contains("reel-"));
    let bytes = db.bytes().await.unwrap();
    assert!(
        bytes.starts_with(b"SQLite format 3"),
        "a real database file"
    );
    assert!(
        std::fs::read_dir(env.ctx.config().cache_dir())
            .map(|rd| rd
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().starts_with(".export-")))
            .unwrap_or(true),
        "the temporary copy is gone"
    );
    let cfg = env
        .client
        .get(format!("{}/api/export/config", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        cfg.contains("[auth]") && cfg.contains("ntfy_token = \"\""),
        "{cfg}"
    );

    let (status, d) = post_json(&env, "/api/restart").await;
    assert_eq!(status, 200, "{d}");
    let start = Instant::now();
    while !env.ctx.shutdown.is_cancelled() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "the restart asks the process to stop"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_preview_size_decides_which_versions_get_a_copy_and_remake_fills_the_gaps() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("big.mp4", Synth::h264_high_bitrate(5.0, "big"))
        .await;
    let big = env.wait_ready(Duration::from_secs(90)).await;
    env.synth("tiny.mp4", Synth::small(2.0, "tiny")).await;
    let tiny = env
        .wait_clip(
            |c| c.state == "ready" && c.id != big.id,
            Duration::from_secs(90),
        )
        .await;
    env.wait_no_jobs(Duration::from_secs(60)).await;
    let master = env
        .store()
        .rendition(big.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(master.size.unwrap() > 5_000_000 && master.size.unwrap() < 20_000_000);
    assert!(
        env.store()
            .derived_rendition(&master.id, "embed", None)
            .await
            .unwrap()
            .is_none(),
        "under 20 MB, no copy"
    );
    let share: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/shares", env.base, big.id))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = share["token"].as_str().unwrap().to_string();
    let page_of = |t: String| {
        let client = env.client.clone();
        let base = env.base.clone();
        async move {
            client
                .get(format!("{base}/s/{t}"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        }
    };
    let tags = og(&page_of(token.clone()).await);
    assert!(
        tags["og:video"].contains(&format!("r={}", master.id)),
        "the version itself: {}",
        tags["og:video"]
    );

    let (status, d) = patch_setting(&env, serde_json::json!({ "share.embed_mb": 5 })).await;
    assert_eq!(status, 200, "{d}");
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, big.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("Preview video") && page.contains("none yet"),
        "the page says a copy is missing"
    );
    let tags = og(&page_of(token.clone()).await);
    assert!(
        tags["og:video"].contains(&format!("r={}", master.id)),
        "until a copy exists the link serves the version itself"
    );

    let (status, d) = post_json(&env, "/api/storage/remake-embeds").await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(
        (
            d["queued"].clone(),
            d["missing"].clone(),
            d["stale"].clone()
        ),
        (1.into(), 1.into(), 0.into()),
        "{d}"
    );
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let copy = env
        .store()
        .derived_rendition(&master.id, "embed", None)
        .await
        .unwrap()
        .expect("a copy under the new size");
    assert!(
        copy.is_ready() && copy.size.unwrap() <= 5_000_000,
        "{:?}",
        copy.error
    );
    let tags = og(&page_of(token.clone()).await);
    assert!(
        tags["og:video"].contains(&format!("r={}", copy.id)),
        "the tags point at the copy: {}",
        tags["og:video"]
    );
    assert_eq!(tags["og:video:height"], copy.height.unwrap().to_string());
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, big.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains(&format!("{}p", copy.height.unwrap())),
        "the page shows the preview's height"
    );
    let tiny_page = env
        .client
        .get(format!("{}/clips/{}", env.base, tiny.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        tiny_page.contains("sent as it is"),
        "a version under the size needs none"
    );
    assert!(env
        .store()
        .derived_rendition(tiny.master_id.as_deref().unwrap(), "embed", None)
        .await
        .unwrap()
        .is_none());

    let (status, _) = patch_setting(&env, serde_json::json!({ "share.embed_mb": 6 })).await;
    assert_eq!(status, 200);
    let (status, d) = post_json(&env, "/api/storage/remake-embeds").await;
    assert_eq!(status, 200, "{d}");
    assert_eq!(
        (
            d["queued"].clone(),
            d["missing"].clone(),
            d["stale"].clone()
        ),
        (1.into(), 0.into(), 1.into()),
        "a copy made for another size is stale: {d}"
    );
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let again = env
        .store()
        .derived_rendition(&master.id, "embed", None)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(again.id, copy.id, "a new copy");
    assert!(again.recipe.as_deref().unwrap().contains("\"size_mb\":6"));
    assert!(
        env.store().rendition(&copy.id).await.unwrap().is_none(),
        "the old one is gone"
    );
    let (status, d) = post_json(&env, "/api/storage/remake-embeds").await;
    assert_eq!(status, 200);
    assert_eq!(d["queued"], 0, "nothing left to do: {d}");
}

async fn pair_code(env: &Env) -> String {
    let r = env
        .client
        .post(format!("{}/api/devices/pair-code", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let v: serde_json::Value = r.json().await.unwrap();
    assert_eq!(v["expires_in"], 600);
    v["code"].as_str().unwrap().to_string()
}

async fn try_pair(env: &Env, code: &str, name: &str) -> (u16, serde_json::Value) {
    let r = env
        .client
        .post(format!("{}/api/device/pair", env.base))
        .json(&serde_json::json!({ "code": code, "name": name }))
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    let v: serde_json::Value = r.json().await.unwrap_or(serde_json::Value::Null);
    (status, v)
}

async fn pair(env: &Env, name: &str) -> (String, String) {
    let code = pair_code(env).await;
    let (status, v) = try_pair(env, &code, name).await;
    assert_eq!(status, 201, "{v}");
    (
        v["token"].as_str().unwrap().to_string(),
        v["device_id"].as_str().unwrap().to_string(),
    )
}

async fn dev_json(
    env: &Env,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> (u16, serde_json::Value) {
    let mut req = env
        .client
        .request(method, format!("{}{path}", env.base))
        .bearer_auth(token);
    if let Some(b) = body {
        req = req.json(&b);
    }
    let r = req.send().await.unwrap();
    let status = r.status().as_u16();
    let v: serde_json::Value = r.json().await.unwrap_or(serde_json::Value::Null);
    (status, v)
}

async fn put_chunk(
    env: &Env,
    token: &str,
    id: &str,
    offset: usize,
    bytes: &[u8],
) -> (u16, serde_json::Value) {
    let r = pieces_client()
        .put(format!(
            "{}/api/device/uploads/{id}?offset={offset}",
            env.base
        ))
        .bearer_auth(token)
        .body(bytes.to_vec())
        .send()
        .await
        .unwrap();
    let status = r.status().as_u16();
    let v: serde_json::Value = r.json().await.unwrap_or(serde_json::Value::Null);
    (status, v)
}

async fn upload_whole(
    env: &Env,
    token: &str,
    path: &std::path::Path,
    chunk: usize,
    extra: serde_json::Value,
) -> (u16, serde_json::Value) {
    let bytes = std::fs::read(path).unwrap();
    let sha = reel_pipeline::hash::sha256_of_bytes(&bytes);
    let mut body = serde_json::json!({
        "file_name": path.file_name().unwrap().to_str().unwrap(),
        "size": bytes.len(),
        "sha256": sha,
    });
    if let serde_json::Value::Object(m) = extra {
        for (k, v) in m {
            body[k] = v;
        }
    }
    let (status, v) = dev_json(
        env,
        token,
        reqwest::Method::POST,
        "/api/device/uploads",
        Some(body),
    )
    .await;
    if v["status"] == "exists" {
        return (status, v);
    }
    assert!(status == 201 || status == 200, "{v}");
    let id = v["upload_id"].as_str().unwrap().to_string();
    let mut offset = v["offset"].as_u64().unwrap() as usize;
    while offset < bytes.len() {
        let end = (offset + chunk).min(bytes.len());
        let (s, r) = put_chunk(env, token, &id, offset, &bytes[offset..end]).await;
        assert_eq!(s, 200, "{r}");
        offset = r["offset"].as_u64().unwrap() as usize;
    }
    dev_json(
        env,
        token,
        reqwest::Method::POST,
        &format!("/api/device/uploads/{id}/finish"),
        None,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn devices_pair_with_a_code_that_is_spent_once_expires_and_is_limited() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let code = pair_code(&env).await;
    assert_eq!(code.len(), 9, "{code}");
    let (s, v) = try_pair(&env, "zzzz-zzzz", "wrong").await;
    assert_eq!(s, 403, "{v}");
    let (s, v) = try_pair(&env, &code.to_uppercase(), " gaming PC ").await;
    assert_eq!(s, 201, "{v}");
    let token = v["token"].as_str().unwrap().to_string();
    assert_eq!(token.len(), 43);
    assert_eq!(v["name"], "gaming PC");
    assert_eq!(v["user"], "you");
    let device_id = v["device_id"].as_str().unwrap().to_string();
    let (s, v) = try_pair(&env, &code, "again").await;
    assert_eq!(s, 403, "a code works once: {v}");

    let (s, who) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        "/api/device/whoami",
        None,
    )
    .await;
    assert_eq!(s, 200, "{who}");
    assert_eq!(who["device_id"], device_id);
    assert_eq!(who["chunk_bytes"], 32 * 1024 * 1024);
    assert_eq!(who["max_chunk_bytes"], 90 * 1024 * 1024);
    let (s, v) = dev_json(
        &env,
        "nope",
        reqwest::Method::GET,
        "/api/device/whoami",
        None,
    )
    .await;
    assert_eq!(s, 401, "{v}");
    let (s, v) = dev_json(
        &env,
        &"A".repeat(43),
        reqwest::Method::GET,
        "/api/device/whoami",
        None,
    )
    .await;
    assert_eq!(s, 401, "{v}");
    let r = env
        .client
        .get(format!("{}/api/device/whoami", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401, "no cookie counts on the device routes");

    let stored: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM device")
        .fetch_all(env.store().pool())
        .await
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_ne!(stored[0], token, "the token is stored as a hash");
    assert_eq!(stored[0].len(), 64);

    let expired = pair_code(&env).await;
    sqlx::query("UPDATE pair_code SET expires_at = '2000-01-01T00:00:00Z' WHERE used_at IS NULL")
        .execute(env.store().pool())
        .await
        .unwrap();
    let (s, v) = try_pair(&env, &expired, "late").await;
    assert_eq!(s, 403, "an expired code: {v}");

    let (s, _) = try_pair(&env, "zzzz-zzz2", "x").await;
    assert_eq!(s, 403);
    let r = env
        .client
        .post(format!("{}/api/device/pair", env.base))
        .json(&serde_json::json!({ "code": "zzzz-zzz3", "name": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 429, "the sixth try from one address waits");
    assert!(r.headers().get("retry-after").is_some());
    let v: serde_json::Value = r.json().await.unwrap();
    assert!(v["error"].as_str().unwrap().contains("wait"));

    let (s, list) = {
        let r = env
            .client
            .get(format!("{}/api/devices", env.base))
            .send()
            .await
            .unwrap();
        (
            r.status().as_u16(),
            r.json::<serde_json::Value>().await.unwrap(),
        )
    };
    assert_eq!(s, 200);
    assert_eq!(list["devices"].as_array().unwrap().len(), 1);
    assert_eq!(list["devices"][0]["name"], "gaming PC");
    assert_eq!(list["devices"][0]["revoked"], false);
    assert_ne!(list["devices"][0]["last_seen"], "never");

    let r = env
        .client
        .delete(format!("{}/api/devices/{device_id}", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        "/api/device/whoami",
        None,
    )
    .await;
    assert_eq!(s, 401, "{v}");
    assert!(v["error"].as_str().unwrap().contains("revoked"));

    let page = env
        .client
        .get(format!("{}/settings/devices", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("Pair a device"));
    assert!(page.contains("gaming PC"));
    assert!(page.contains("revoked"));
    assert!(page.contains("device.chunk_mb"));
    let (s, v) = patch_setting(&env, serde_json::json!({ "device.chunk_mb": 91 })).await;
    assert_eq!(s, 400, "{v}");
    let (s, _) = patch_setting(&env, serde_json::json!({ "device.chunk_mb": 90 })).await;
    assert_eq!(s, 200);
    assert_eq!(env.ctx.config().device_chunk_bytes(), 90 * 1024 * 1024);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_uploads_in_chunks_resumes_after_a_cut_and_a_bad_checksum_starts_over() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let (token, device_id) = pair(&env, "pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let file = env
        .synth_at(
            &pc,
            "Replay_2026-10-08_21-14-03.mkv",
            Synth::h264_1080p(3.0, "ace"),
        )
        .await;
    let bytes = std::fs::read(&file).unwrap();
    assert!(
        bytes.len() > 600_000,
        "a clip worth chunking: {}",
        bytes.len()
    );
    let sha = reel_pipeline::hash::sha256_of_bytes(&bytes);
    let create = serde_json::json!({
        "file_name": "Replay_2026-10-08_21-14-03.mkv", "size": bytes.len(), "sha256": sha,
        "recorded_at": "2026-10-08T19:14:03Z", "game": "Valorant", "title": "Valorant, Oct 8, 21:14",
    });
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        "/api/device/uploads",
        Some(create.clone()),
    )
    .await;
    assert_eq!(s, 201, "{v}");
    assert_eq!(v["status"], "created");
    assert_eq!(v["offset"], 0);
    let id = v["upload_id"].as_str().unwrap().to_string();
    let chunk = 200_000;

    let (s, v) = put_chunk(&env, &token, &id, 0, &bytes[..chunk]).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["offset"], chunk);
    let (s, v) = put_chunk(&env, &token, &id, 0, &bytes[..chunk]).await;
    assert_eq!(
        s, 409,
        "a chunk at the wrong offset is refused with the real end: {v}"
    );
    assert_eq!(v["offset"], chunk);
    let (s, v) = put_chunk(&env, &token, &id, chunk, &bytes[chunk..chunk + 77_777]).await;
    assert_eq!(s, 200, "a cut chunk: what arrived is kept {v}");
    let (s, st) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        &format!("/api/device/uploads/{id}"),
        None,
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(st["offset"], chunk + 77_777);
    assert_eq!(st["state"], "open");
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        "/api/device/uploads",
        Some(create.clone()),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(
        v["status"], "resumed",
        "the same hash resumes the open upload"
    );
    assert_eq!(v["upload_id"], id);
    assert_eq!(v["offset"], chunk + 77_777);
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        &format!("/api/device/uploads/{id}/finish"),
        None,
    )
    .await;
    assert_eq!(s, 409, "finish before the end: {v}");
    assert_eq!(v["offset"], chunk + 77_777);
    let (s, v) = put_chunk(
        &env,
        &token,
        &id,
        chunk + 77_777,
        &bytes[chunk + 77_777..chunk + 77_777 + 1],
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let mut offset = chunk + 77_777 + 1;
    while offset < bytes.len() {
        let end = (offset + chunk).min(bytes.len());
        let (s, r) = put_chunk(&env, &token, &id, offset, &bytes[offset..end]).await;
        assert_eq!(s, 200, "{r}");
        offset = r["offset"].as_u64().unwrap() as usize;
    }
    let (s, v) = put_chunk(&env, &token, &id, offset, b"extra").await;
    assert_eq!(s, 400, "past the announced size: {v}");
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        &format!("/api/device/uploads/{id}/finish"),
        None,
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["state"], "processing");
    let clip_id = v["clip_id"].as_str().unwrap().to_string();
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    assert_eq!(clip.id, clip_id);
    assert_eq!(clip.source, "device");
    assert_eq!(clip.title, "Valorant, Oct 8, 21:14");
    assert_eq!(clip.game.as_deref(), Some("Valorant"));
    assert_eq!(clip.device_id.as_deref(), Some(device_id.as_str()));
    assert_eq!(clip.source_hash.as_deref(), Some(sha.as_str()));
    assert_eq!(clip.recorded_at.as_deref(), Some("2026-10-08T19:14:03Z"));
    assert_eq!(clip.recorded_source.as_deref(), Some("device"));
    assert!(clip.owner_id.is_some());
    assert!(file.exists(), "the PC keeps its own file");
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        &format!("/api/device/clips/{clip_id}"),
        None,
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["ready"], true);
    assert_eq!(v["state"], "ready");
    assert_eq!(v["preview_ready"], true, "{v}");
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        "/api/device/clips",
        None,
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(v["clips"].as_array().unwrap().len(), 1);
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        &format!("/api/device/uploads/{id}/finish"),
        None,
    )
    .await;
    assert_eq!(s, 200, "finish again answers the clip: {v}");
    assert_eq!(v["clip_id"], clip_id);
    assert!(
        !env.ctx
            .config()
            .device_uploads_dir()
            .join(&id)
            .join("Replay_2026-10-08_21-14-03.mkv")
            .exists(),
        "the raw file went through the ingest"
    );

    let other = env
        .synth_at(
            &pc,
            "2026-10-08 21-20-00.mkv",
            Synth::h264_1080p(2.0, "two"),
        )
        .await;
    let other_bytes = std::fs::read(&other).unwrap();
    let wrong = serde_json::json!({ "file_name": "2026-10-08 21-20-00.mkv", "size": other_bytes.len(), "sha256": "0".repeat(64) });
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        "/api/device/uploads",
        Some(wrong),
    )
    .await;
    assert_eq!(s, 201, "{v}");
    let bad = v["upload_id"].as_str().unwrap().to_string();
    let (s, v) = put_chunk(&env, &token, &bad, 0, &other_bytes).await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        &format!("/api/device/uploads/{bad}/finish"),
        None,
    )
    .await;
    assert_eq!(s, 422, "{v}");
    assert_eq!(v["offset"], 0);
    let (_, st) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        &format!("/api/device/uploads/{bad}"),
        None,
    )
    .await;
    assert_eq!(st["offset"], 0, "the upload starts over");
    let (s, _) = dev_json(
        &env,
        &token,
        reqwest::Method::DELETE,
        &format!("/api/device/uploads/{bad}"),
        None,
    )
    .await;
    assert_eq!(s, 200);
    let (s, _) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        &format!("/api/device/uploads/{bad}"),
        None,
    )
    .await;
    assert_eq!(s, 404);

    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        "/api/device/uploads",
        Some(serde_json::json!({ "file_name": "x.exe", "size": 10, "sha256": "0".repeat(64) })),
    )
    .await;
    assert_eq!(s, 400, "{v}");

    let stale =
        serde_json::json!({ "file_name": "stale.mkv", "size": 10, "sha256": "1".repeat(64) });
    let (_, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        "/api/device/uploads",
        Some(stale),
    )
    .await;
    let stale_id = v["upload_id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE device_upload SET updated_at = '2000-01-01T00:00:00Z' WHERE id = ?")
        .bind(&stale_id)
        .execute(env.store().pool())
        .await
        .unwrap();
    let gone = reel_pipeline::housekeeping::expire_device_uploads(&env.ctx, "2001-01-01T00:00:00Z")
        .await
        .unwrap();
    assert_eq!(gone, 1);
    assert!(!env
        .ctx
        .config()
        .device_uploads_dir()
        .join(&stale_id)
        .exists());
    let (s, _) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        &format!("/api/device/uploads/{stale_id}"),
        None,
    )
    .await;
    assert_eq!(s, 404);
    let list: serde_json::Value = env
        .client
        .get(format!("{}/api/devices", env.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["devices"][0]["uploads"], 1, "{list}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_sees_only_its_own_clips() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let (a, _) = pair(&env, "a").await;
    let (b, _) = pair(&env, "b").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let file = env
        .synth_at(&pc, "mine.mp4", Synth::h264_1080p(2.0, "a"))
        .await;
    let (s, v) = upload_whole(&env, &a, &file, 300_000, serde_json::json!({})).await;
    assert_eq!(s, 200, "{v}");
    let clip_id = v["clip_id"].as_str().unwrap().to_string();
    env.wait_ready(Duration::from_secs(90)).await;
    let (s, _) = dev_json(
        &env,
        &a,
        reqwest::Method::GET,
        &format!("/api/device/clips/{clip_id}"),
        None,
    )
    .await;
    assert_eq!(s, 200);
    let (s, v) = dev_json(
        &env,
        &b,
        reqwest::Method::GET,
        &format!("/api/device/clips/{clip_id}"),
        None,
    )
    .await;
    assert_eq!(s, 404, "{v}");
    let (_, v) = dev_json(&env, &b, reqwest::Method::GET, "/api/device/clips", None).await;
    assert_eq!(v["clips"].as_array().unwrap().len(), 0);
    let (s, v) = dev_json(&env, &b, reqwest::Method::POST, "/api/device/share", None).await;
    assert_eq!(s, 404, "no clip of its own to share: {v}");
    let (s, v) = dev_json(
        &env,
        &b,
        reqwest::Method::POST,
        "/api/device/share",
        Some(serde_json::json!({ "clip_id": clip_id })),
    )
    .await;
    assert_eq!(s, 404, "{v}");
    let (s, v) = dev_json(&env, &a, reqwest::Method::POST, "/api/device/share", None).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["status"], "ready");
    assert_eq!(v["clip_id"], clip_id);
    let url = v["url"].as_str().unwrap().to_string();
    assert!(url.starts_with(&format!("{}/s/", env.base)), "{url}");
    let (s, again) = dev_json(&env, &a, reqwest::Method::POST, "/api/device/share", None).await;
    assert_eq!(s, 200);
    assert_eq!(again["url"], url, "the link is reused");
    assert_eq!(env.store().shares_of(&clip_id).await.unwrap().len(), 1);
    let r = env.client.get(&url).send().await.unwrap();
    assert_eq!(r.status(), 200);
    let (_, v) = dev_json(
        &env,
        &a,
        reqwest::Method::GET,
        &format!("/api/device/clips/{clip_id}"),
        None,
    )
    .await;
    assert_eq!(v["share_url"], url);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_same_file_through_the_relay_and_the_app_is_one_clip() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let (token, device_id) = pair(&env, "pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();

    let relayed = env
        .synth_at(
            &pc,
            "Replay_2026-10-08_20-00-00.mkv",
            Synth::h264_1080p(2.0, "relay"),
        )
        .await;
    std::fs::copy(
        &relayed,
        env.incoming().join("Replay_2026-10-08_20-00-00.mkv"),
    )
    .unwrap();
    let first = env.wait_ready(Duration::from_secs(90)).await;
    assert_eq!(first.source, "incoming");
    let sha = reel_pipeline::hash::sha256_file(&relayed).await.unwrap();
    assert_eq!(
        first.source_hash.as_deref(),
        Some(sha.as_str()),
        "the incoming folder's clips carry their hash too"
    );
    assert!(first.device_id.is_none());
    let (s, v) = upload_whole(&env, &token, &relayed, 300_000, serde_json::json!({})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["status"], "exists", "the app's copy is not taken: {v}");
    assert_eq!(v["clip_id"], first.id);
    let adopted = env.store().clip(&first.id).await.unwrap().unwrap();
    assert_eq!(
        adopted.device_id.as_deref(),
        Some(device_id.as_str()),
        "the device adopts the relay's clip"
    );
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        "/api/device/share",
        None,
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["clip_id"], first.id);

    let uploaded = env
        .synth_at(
            &pc,
            "Replay_2026-10-08_20-30-00.mkv",
            Synth::h264_1080p(2.0, "app"),
        )
        .await;
    let (s, v) = upload_whole(&env, &token, &uploaded, 300_000, serde_json::json!({})).await;
    assert_eq!(s, 200, "{v}");
    let second_id = v["clip_id"].as_str().unwrap().to_string();
    env.wait_clip(
        |c| c.id == second_id && c.state == "ready",
        Duration::from_secs(90),
    )
    .await;
    let before = env
        .store()
        .clips("", reel_store::Sort::Newest)
        .await
        .unwrap()
        .len();
    assert_eq!(before, 2);
    let late = env.incoming().join("Replay_2026-10-08_20-30-00.mkv");
    std::fs::copy(&uploaded, &late).unwrap();
    let start_at = Instant::now();
    loop {
        let f = env
            .store()
            .ingest_file("Replay_2026-10-08_20-30-00.mkv")
            .await
            .unwrap();
        if let Some(f) = f {
            if f.state == "done" {
                assert_eq!(
                    f.clip_id.as_deref(),
                    Some(second_id.as_str()),
                    "the relay's copy is booked as the app's clip"
                );
                break;
            }
        }
        assert!(
            start_at.elapsed() < Duration::from_secs(90),
            "the duplicate was not resolved"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    env.wait_no_jobs(Duration::from_secs(30)).await;
    assert!(!late.exists(), "the duplicate raw file is removed");
    let after = env
        .store()
        .clips("", reel_store::Sort::Newest)
        .await
        .unwrap();
    assert_eq!(
        after.len(),
        2,
        "no second clip: {:?}",
        after
            .iter()
            .map(|c| c.clip.title.clone())
            .collect::<Vec<_>>()
    );
    assert!(after.iter().all(|c| c.clip.state == "ready"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_app_paired_again_gets_the_clip_the_library_has_and_its_link() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let (old, _) = pair(&env, "gaming pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let file = env
        .synth_at(
            &pc,
            "Replay_2026-11-01_20-38-40.mkv",
            Synth::h264_1080p(2.0, "sent"),
        )
        .await;
    let (s, v) = upload_whole(&env, &old, &file, 300_000, serde_json::json!({})).await;
    assert_eq!(s, 200, "{v}");
    let clip_id = v["clip_id"].as_str().unwrap().to_string();
    env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;

    let (new, new_id) = pair(&env, "gaming pc").await;
    for _ in 0..2 {
        let (s, v) = upload_whole(&env, &new, &file, 300_000, serde_json::json!({})).await;
        assert_eq!(s, 200, "{v}");
        assert_eq!(v["status"], "exists", "{v}");
        assert_eq!(v["clip_id"], clip_id);
    }
    let (s, v) = dev_json(
        &env,
        &new,
        reqwest::Method::GET,
        &format!("/api/device/clips/{clip_id}"),
        None,
    )
    .await;
    assert_eq!(s, 200, "the app paired again follows the clip: {v}");
    assert_eq!(v["ready"], true);
    let (_, v) = dev_json(&env, &new, reqwest::Method::GET, "/api/device/clips", None).await;
    assert_eq!(v["clips"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(v["clips"][0]["id"], clip_id);
    let (s, v) = dev_json(
        &env,
        &new,
        reqwest::Method::POST,
        "/api/device/share",
        Some(serde_json::json!({ "clip_id": clip_id })),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["status"], "ready");
    let url = v["url"].as_str().unwrap();
    assert!(url.starts_with(&format!("{}/s/", env.base)), "{url}");
    let (s, _) = dev_json(
        &env,
        &old,
        reqwest::Method::GET,
        &format!("/api/device/clips/{clip_id}"),
        None,
    )
    .await;
    assert_eq!(s, 200, "the first device keeps it");
    let list: serde_json::Value = env
        .client
        .get(format!("{}/api/devices", env.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let row = list["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["id"] == new_id)
        .unwrap()
        .clone();
    assert_eq!(
        row["uploads"], 1,
        "the same file twice is one upload: {row}"
    );

    let (late, _) = pair(&env, "laptop").await;
    let second = env
        .synth_at(
            &pc,
            "Replay_2026-11-01_21-00-00.mkv",
            Synth::h264_1080p(2.0, "late"),
        )
        .await;
    let bytes = std::fs::read(&second).unwrap();
    let (s, v) = dev_json(
        &env,
        &late,
        reqwest::Method::POST,
        "/api/device/uploads",
        Some(serde_json::json!({
            "file_name": "Replay_2026-11-01_21-00-00.mkv",
            "size": bytes.len(),
            "sha256": reel_pipeline::hash::sha256_of_bytes(&bytes),
        })),
    )
    .await;
    assert_eq!(s, 201, "{v}");
    let upload = v["upload_id"].as_str().unwrap().to_string();
    let (s, v) = upload_whole(&env, &old, &second, 300_000, serde_json::json!({})).await;
    assert_eq!(s, 200, "{v}");
    let second_id = v["clip_id"].as_str().unwrap().to_string();
    env.wait_clip(
        |c| c.id == second_id && c.state == "ready",
        Duration::from_secs(90),
    )
    .await;
    let (s, v) = put_chunk(&env, &late, &upload, 0, &bytes).await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = dev_json(
        &env,
        &late,
        reqwest::Method::POST,
        &format!("/api/device/uploads/{upload}/finish"),
        None,
    )
    .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["clip_id"], second_id, "the library had it by the finish");
    let (s, v) = dev_json(
        &env,
        &late,
        reqwest::Method::GET,
        &format!("/api/device/clips/{second_id}"),
        None,
    )
    .await;
    assert_eq!(s, 200, "{v}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_that_is_another_users_clip_is_not_handed_to_a_device() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let (token, _) = pair(&env, "gaming pc").await;
    let mira = reel_store::models::User::new("mira", "member", None);
    env.store().insert_user(&mira).await.unwrap();
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let file = env
        .synth_at(
            &pc,
            "Replay_2026-11-01_20-38-40.mkv",
            Synth::h264_1080p(2.0, "hers"),
        )
        .await;
    std::fs::copy(&file, env.incoming().join("hers.mkv")).unwrap();
    let hers = env.wait_ready(Duration::from_secs(90)).await;
    env.store()
        .set_clip_owner(&hers.id, Some(&mira.id))
        .await
        .unwrap();
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let bytes = std::fs::read(&file).unwrap();
    let new = serde_json::json!({
        "file_name": "Replay_2026-11-01_20-38-40.mkv",
        "size": bytes.len(),
        "sha256": reel_pipeline::hash::sha256_of_bytes(&bytes),
    });
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        "/api/device/uploads",
        Some(new),
    )
    .await;
    assert_eq!(s, 403, "{v}");
    assert!(v["clip_id"].is_null() && v["clip"].is_null(), "{v}");
    assert!(
        v["error"].as_str().unwrap().contains("another user's clip"),
        "{v}"
    );
    let (s, _) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        &format!("/api/device/clips/{}", hers.id),
        None,
    )
    .await;
    assert_eq!(s, 404);
    let (s, _) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        "/api/device/share",
        Some(serde_json::json!({ "clip_id": hers.id })),
    )
    .await;
    assert_eq!(s, 404);
    let (_, v) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        "/api/device/clips",
        None,
    )
    .await;
    assert_eq!(v["clips"].as_array().unwrap().len(), 0, "{v}");

    let later = env
        .synth_at(
            &pc,
            "Replay_2026-11-01_21-00-00.mkv",
            Synth::h264_1080p(2.0, "later"),
        )
        .await;
    let bytes = std::fs::read(&later).unwrap();
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        "/api/device/uploads",
        Some(serde_json::json!({
            "file_name": "Replay_2026-11-01_21-00-00.mkv",
            "size": bytes.len(),
            "sha256": reel_pipeline::hash::sha256_of_bytes(&bytes),
        })),
    )
    .await;
    assert_eq!(s, 201, "{v}");
    let upload = v["upload_id"].as_str().unwrap().to_string();
    std::fs::copy(&later, env.incoming().join("hers too.mkv")).unwrap();
    let theirs = env
        .wait_clip(
            |c| c.id != hers.id && c.state == "ready",
            Duration::from_secs(90),
        )
        .await;
    env.store()
        .set_clip_owner(&theirs.id, Some(&mira.id))
        .await
        .unwrap();
    let (s, v) = put_chunk(&env, &token, &upload, 0, &bytes).await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        &format!("/api/device/uploads/{upload}/finish"),
        None,
    )
    .await;
    assert_eq!(s, 403, "{v}");
    assert_eq!(
        env.store().upload(&upload).await.unwrap().unwrap().state,
        "failed"
    );
    for id in [&hers.id, &theirs.id] {
        let c = env.store().clip(id).await.unwrap().unwrap();
        assert!(c.device_id.is_none(), "not adopted");
        assert_eq!(c.owner_id.as_deref(), Some(mira.id.as_str()));
    }
    assert_eq!(
        env.store()
            .clips("", reel_store::Sort::Newest)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_share_route_waits_for_the_preview() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| c.share.embed_mb = 1).await else {
        return;
    };
    let (token, _) = pair(&env, "pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let file = env
        .synth_at(&pc, "big.mp4", Synth::h264_1080p(4.0, "big"))
        .await;
    assert!(std::fs::metadata(&file).unwrap().len() > 1_000_000);
    let share = || async {
        dev_json(
            &env,
            &token,
            reqwest::Method::POST,
            "/api/device/share",
            None,
        )
        .await
    };
    env.ctx
        .hold_heavy
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let (s, v) = upload_whole(&env, &token, &file, 400_000, serde_json::json!({})).await;
    assert_eq!(s, 200, "{v}");
    let clip_id = v["clip_id"].as_str().unwrap().to_string();
    let mut seen_preparing = Vec::new();
    let start_at = Instant::now();
    loop {
        let (s, v) = share().await;
        assert_eq!(s, 202, "{v}: no link before the copy is made");
        assert_eq!(v["status"], "preparing");
        assert_eq!(v["clip_id"], clip_id);
        seen_preparing.push(v["step"].as_str().unwrap().to_string());
        let clip = env.store().clip(&clip_id).await.unwrap().unwrap();
        if clip.state == "ready" {
            break;
        }
        assert!(
            start_at.elapsed() < Duration::from_secs(120),
            "no publish in time"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let clip = env.store().clip(&clip_id).await.unwrap().unwrap();
    let current = env
        .store()
        .rendition(clip.current_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let copy = env
        .store()
        .derived_rendition(&current.id, "embed", None)
        .await
        .unwrap()
        .expect("the copy is queued before the clip is published");
    assert!(!copy.is_ready(), "the copy waits with the pipeline held");
    assert!(
        reel_pipeline::enqueue::embed_grade(&env.ctx, &current)
            .await
            .unwrap()
            .is_none(),
        "a version over the limit does not embed on its own"
    );
    for _ in 0..3 {
        let (s, v) = share().await;
        assert_eq!(s, 202, "{v}: published, but the copy is not there yet");
        assert_eq!(v["step"], "preview");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    env.ctx
        .hold_heavy
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let mut url = None;
    let start_at = Instant::now();
    while url.is_none() {
        let (s, v) = share().await;
        match s {
            202 => {
                assert_eq!(v["step"], "preview", "{v}");
            }
            200 => {
                assert_eq!(v["status"], "ready");
                let grade = reel_pipeline::enqueue::embed_grade(&env.ctx, &current)
                    .await
                    .unwrap()
                    .expect("a link is handed out only once the page embeds");
                assert_eq!(grade.kind, "embed", "the copy is what the page serves");
                url = Some(v["url"].as_str().unwrap().to_string());
            }
            other => panic!("{other}: {v}"),
        }
        assert!(
            start_at.elapsed() < Duration::from_secs(120),
            "no link in time"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        seen_preparing.contains(&"ingest".to_string())
            || seen_preparing.contains(&"preview".to_string())
    );
    let url = url.unwrap();
    let html = env
        .client
        .get(&url)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let tags = og(&html);
    let clip = env.store().clip(&clip_id).await.unwrap().unwrap();
    let current = env
        .store()
        .rendition(clip.current_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let copy = env
        .store()
        .derived_rendition(&current.id, "embed", None)
        .await
        .unwrap()
        .unwrap();
    assert!(copy.is_ready());
    assert!(
        tags["og:video"].ends_with(&format!("?r={}", copy.id)),
        "{}",
        tags["og:video"]
    );
    let (_, v) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        &format!("/api/device/clips/{clip_id}"),
        None,
    )
    .await;
    assert_eq!(v["preview_ready"], true);
    assert_eq!(v["share_url"], url);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_opened_before_its_preview_is_made_says_it_is_being_prepared() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| c.share.embed_mb = 1).await else {
        return;
    };
    env.ctx
        .hold_heavy
        .store(true, std::sync::atomic::Ordering::SeqCst);
    env.synth("big.mp4", Synth::h264_1080p(4.0, "big")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let current = env
        .store()
        .rendition(clip.current_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let copy = env
        .store()
        .derived_rendition(&current.id, "embed", None)
        .await
        .unwrap()
        .expect("a version over the limit gets a preview made for it");
    assert!(!copy.is_ready(), "the preview waits with the pipeline held");
    let (s, link) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", clip.id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(s, 200, "{link}");
    let page = |token: String| {
        let env = &env;
        async move {
            let r = env
                .client
                .get(format!("{}/s/{token}", env.base))
                .send()
                .await
                .unwrap();
            (r.status().as_u16(), r.text().await.unwrap())
        }
    };
    let token = link["token"].as_str().unwrap().to_string();
    let (s, html) = page(token.clone()).await;
    assert_eq!(s, 503);
    assert!(
        html.contains("<strong>This clip is still being prepared.</strong>")
            && html.contains("Try again in a minute."),
        "{html}"
    );
    assert!(
        !html.contains("retired"),
        "a fresh link is not retired: {html}"
    );

    env.ctx
        .hold_heavy
        .store(false, std::sync::atomic::Ordering::SeqCst);
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let (s, _) = page(token.clone()).await;
    assert_eq!(s, 200, "the page plays once the preview is made");
    let r = env
        .client
        .delete(format!(
            "{}/api/shares/{}",
            env.base,
            link["id"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let (s, html) = page(token).await;
    assert_eq!(s, 404);
    assert!(
        html.contains("<strong>This link has been retired.</strong>")
            && html.contains("Ask whoever sent it for a fresh one."),
        "{html}"
    );
    assert!(!html.contains("being prepared"), "{html}");
}

#[tokio::test]
async fn a_clip_is_in_the_library_before_its_loop_and_filmstrip() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("fast.mp4", Synth::h264_1080p(4.0, "fast")).await;
    let clip = env.wait_ready(Duration::from_secs(120)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let video = env.ctx.config().abs(master.path.as_deref().unwrap());
    let d = reel_pipeline::derived::paths(&video);
    assert!(
        d.poster.exists(),
        "the poster is there when the clip is published"
    );
    let mut jobs = env.store().active_jobs().await.unwrap();
    jobs.extend(env.store().finished_jobs(20).await.unwrap());
    assert!(
        jobs.iter().any(|j| j.kind == "derive"
            && j.clip_id.as_deref() == Some(clip.id.as_str())
            && j.payload.contains("\"poster\":false")),
        "{:?}",
        jobs.iter()
            .map(|j| (&j.kind, &j.state, &j.payload))
            .collect::<Vec<_>>()
    );
    env.wait_no_jobs(Duration::from_secs(120)).await;
    assert!(
        d.hover.exists() && d.strip.exists(),
        "the loop and the filmstrip follow in the derive job"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cap_on_a_server_spares_collections_and_takes_linked_clips_last() {
    let (target, got) = mock_receiver().await;
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.notify.kind = "webhook".into();
        c.notify.webhook_url = format!("{target}/hook");
        c.storage.disk_warn_gb = 0;
    })
    .await
    else {
        return;
    };
    let mut ids = Vec::new();
    for name in ["a", "b", "c", "d"] {
        env.synth(&format!("{name}.mp4"), Synth::small(2.0, name))
            .await;
        let clip = env
            .wait_clip(
                |c| c.state == "ready" && c.source_name.starts_with(name),
                Duration::from_secs(60),
            )
            .await;
        ids.push(clip.id);
        tokio::time::sleep(Duration::from_millis(1100)).await;
    }
    env.wait_no_jobs(Duration::from_secs(60)).await;
    let set = reel_store::models::Collection::new("keep", None);
    env.store().insert_collection(&set).await.unwrap();
    env.store()
        .add_to_collection(&set.id, &ids[0])
        .await
        .unwrap();
    let r = env
        .client
        .post(format!("{}/api/clips/{}/shares", env.base, ids[1]))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let token = r.json::<serde_json::Value>().await.unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let who: String = sqlx::query_scalar("SELECT id FROM user LIMIT 1")
        .fetch_one(env.store().pool())
        .await
        .unwrap();
    assert!(env.store().star(&who, &ids[3]).await.unwrap());

    let cfg = env.ctx.config();
    assert!(
        cfg.storage.cap_keeps_unsent && !cfg.storage.standalone,
        "a server with the defaults: the unsent rule on, and no clip of it ever uploaded elsewhere"
    );
    let p = reel_pipeline::cap::preview(&env.ctx, 1).await.unwrap();
    let order: Vec<&str> = p.go.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        order,
        vec![ids[2].as_str(), ids[1].as_str()],
        "the oldest first, the linked one last, the collected and the starred one never"
    );
    assert_eq!(
        (
            p.holds.clips,
            p.in_sets.clips,
            p.starred.clips,
            p.unsent.clips
        ),
        (4, 1, 1, 0)
    );
    let line = |gb: u64| {
        let env = &env;
        async move {
            let d: serde_json::Value = env
                .client
                .get(format!("{}/api/storage/cap?gb={gb}", env.base))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            d["line"].as_str().unwrap().to_string()
        }
    };
    assert!(line(0).await.contains("in 4 clips, and no cap."));
    assert!(
        line(1000)
            .await
            .contains("under the 1.0 TB cap: nothing to move."),
        "{}",
        line(1000).await
    );
    let (_, page) = text_of(&env, "/settings/storage").await;
    assert!(
        page.contains("The library holds") && page.contains("and no cap."),
        "the cap's line on the settings page"
    );
    assert!(
        !page.contains("The cap spares unsent clips"),
        "a server's settings leave out the unsent rule"
    );

    let done = reel_pipeline::cap::enforce(&env.ctx, 1, false)
        .await
        .unwrap();
    assert_eq!(done.moved.clips, 2);
    let live: Vec<String> = env
        .store()
        .clips("", reel_store::Sort::Newest)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.clip.id)
        .collect();
    assert_eq!(
        live,
        vec![ids[3].clone(), ids[0].clone()],
        "the starred and the collected clips stay"
    );
    let r = env
        .client
        .get(format!("{}/s/{token}", env.base))
        .send()
        .await
        .unwrap();
    assert_ne!(r.status(), 200, "a trashed clip's link stops");
    let (_, trash) = text_of(&env, "/trash").await;
    assert_eq!(trash.matches("by the library cap").count(), 2);
    let n = wait_note(&got, |n| n.2["event"] == "library_cap").await;
    assert!(
        n.2["title"]
            .as_str()
            .unwrap()
            .ends_with("the library cap moved 2 clips to the trash"),
        "{}",
        n.2
    );
    assert!(n.2["link"].as_str().unwrap().ends_with("/trash"));
    assert_eq!(
        got.lock()
            .await
            .iter()
            .filter(|n| n.2["event"] == "library_cap")
            .count(),
        1,
        "one note a run"
    );
}

#[tokio::test]
async fn the_library_cap_trashes_the_oldest_and_spares_what_is_nowhere_else() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let mut ids = Vec::new();
    for name in ["first", "second", "third"] {
        env.synth(&format!("{name}.mp4"), Synth::small(2.0, name))
            .await;
        let clip = env
            .wait_clip(
                |c| c.state == "ready" && c.source_name.starts_with(name),
                Duration::from_secs(60),
            )
            .await;
        ids.push(clip.id);
        tokio::time::sleep(Duration::from_millis(1100)).await;
    }
    env.wait_no_jobs(Duration::from_secs(60)).await;
    env.store()
        .mark_uploaded(&ids[0], "https://clips.example.org")
        .await
        .unwrap();
    env.store()
        .mark_uploaded(&ids[1], "https://clips.example.org")
        .await
        .unwrap();
    let gone = reel_pipeline::housekeeping::enforce_cap(&env.ctx, 1, true)
        .await
        .unwrap();
    assert_eq!(
        gone, 2,
        "the two clips on a server go, oldest first; the third is nowhere else"
    );
    let live: Vec<String> = env
        .store()
        .clips("", reel_store::Sort::Newest)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.clip.id)
        .collect();
    assert_eq!(live, vec![ids[2].clone()]);
    let gone = reel_pipeline::housekeeping::enforce_cap(&env.ctx, 1, false)
        .await
        .unwrap();
    assert_eq!(gone, 1, "without the rule the last one goes too");
    assert_eq!(
        reel_pipeline::housekeeping::enforce_cap(&env.ctx, 0, false)
            .await
            .unwrap(),
        0,
        "0 is no cap"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_title_sweep_names_file_named_clips_and_keeps_typed_ones() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let names = [
        "2025_05_02_19_30_00-clp.mp4",
        "MedalTVVRChat20260502193000123.mp4",
        "2025-05-02 19-31-00-ses.mp4",
        "MedalTVBeatSaber20240502193200.mp4",
        "Replay_2026-05-02_19-33-00.mp4",
        "MedalTVOverwatch220240502193400.mp4",
        "downloaded-clip-trim-1777750500141.mp4",
    ];
    let mut ids = Vec::new();
    for n in names {
        let stem = n.rsplit_once('.').unwrap().0.to_string();
        let c = reel_store::models::Clip::new(
            reel_core::ids::new_id(),
            stem,
            "folder",
            n.to_string(),
            None,
        );
        env.store().insert_clip(&c).await.unwrap();
        ids.push(c.id);
    }
    let typed = reel_store::models::Clip::new(
        reel_core::ids::new_id(),
        "my best round".into(),
        "folder",
        "Replay_2026-05-02_19-33-00.mp4".into(),
        None,
    );
    env.store().insert_clip(&typed).await.unwrap();
    let words = reel_store::models::Clip::new(
        reel_core::ids::new_id(),
        "the bridge jump".into(),
        "folder",
        "the bridge jump.mp4".into(),
        Some(reel_core::format::now()),
    );
    env.store().insert_clip(&words).await.unwrap();
    let dvr = reel_store::models::Clip::new(
        reel_core::ids::new_id(),
        "Overwatch 2 2024.10.03 - 18.40.01".into(),
        "folder",
        "Overwatch 2 2024.10.03 - 18.40.01.02.DVR.mp4".into(),
        None,
    );
    env.store().insert_clip(&dvr).await.unwrap();

    let n = reel_pipeline::retitle::sweep(&env.ctx).await.unwrap();
    assert_eq!(
        n, 8,
        "the seven file-named clips and the ShadowPlay one, nothing else"
    );
    let store = env.store();
    let clip = |id: &str| {
        let id = id.to_string();
        async move { store.clip(&id).await.unwrap().unwrap() }
    };
    let want: [(&str, Option<&str>); 7] = [
        ("May 2, 2025, 19:30", None),
        ("VRChat, May 2", Some("VRChat")),
        ("May 2, 2025, 19:31", None),
        ("Beat Saber, May 2, 2024, 19:32", Some("Beat Saber")),
        ("May 2", None),
        ("Overwatch 2, May 2, 2024, 19:34", Some("Overwatch 2")),
        ("May ", None),
    ];
    for (i, (title, game)) in want.iter().enumerate() {
        let c = clip(&ids[i]).await;
        assert!(
            c.title.starts_with(title),
            "{}: {} should start with {title}",
            names[i],
            c.title
        );
        assert_ne!(
            c.title,
            names[i].rsplit_once('.').unwrap().0,
            "{}: retitled",
            names[i]
        );
        assert_eq!(c.game.as_deref(), *game, "{}: the game", names[i]);
    }
    let c = clip(&dvr.id).await;
    assert_eq!(
        c.title, "Overwatch 2, Oct 3, 2024, 18:40",
        "ShadowPlay's .02.DVR tail is not part of the name"
    );
    assert_eq!(c.game.as_deref(), Some("Overwatch 2"));
    assert_eq!(
        clip(&typed.id).await.title,
        "my best round",
        "a typed title stays"
    );
    assert_eq!(
        clip(&words.id).await.title,
        "the bridge jump",
        "a name with words stays"
    );
    assert_eq!(
        reel_pipeline::retitle::sweep(&env.ctx).await.unwrap(),
        0,
        "the sweep is one-time: a retitled clip no longer matches its stem"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_open_library_gets_a_new_clip_by_its_events_and_its_card() {
    use futures::StreamExt;
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let mut events = env
        .client
        .get(format!("{}/api/events", env.base))
        .send()
        .await
        .unwrap()
        .bytes_stream();

    env.synth("Replay_2026-10-06_21-14-03.mkv", Synth::small(2.0, "live"))
        .await;
    let mut buf = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let id = loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(left, events.next()).await {
            Ok(Some(Ok(b))) => buf.push_str(&String::from_utf8_lossy(&b)),
            other => panic!("no clip event: {other:?}\n{buf}"),
        }
        if let Some(line) = buf
            .lines()
            .find(|l| l.contains(r#""type":"clip""#) && l.contains(r#""state":"ready""#))
        {
            let v: serde_json::Value =
                serde_json::from_str(line.trim_start_matches("data:").trim()).unwrap();
            break v["id"].as_str().unwrap().to_string();
        }
    };
    let (s, d) = get_json(&env, &format!("/api/library/card/{id}")).await;
    assert_eq!(s, 200, "{d}");
    let html = d["html"].as_str().unwrap();
    assert!(
        html.contains(&format!(r#"data-id="{id}""#)) && html.contains("Oct 6, 21:14"),
        "the card as the library draws it: {html}"
    );
    assert_eq!(d["day"]["key"].as_str().unwrap().len(), 10, "{d}");
    assert!(
        d["summary"].as_str().unwrap().starts_with("1 clip, "),
        "{d}"
    );
    let (s, d) = get_json(&env, &format!("/api/library/card/{id}?sort=title")).await;
    assert_eq!(s, 200);
    assert_eq!(d["day"]["key"], "", "no day groups when sorted by title");
    let r = env
        .client
        .delete(format!("{}/api/clips/{id}", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while !buf.contains(r#""state":"trashed""#) {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(left, events.next()).await {
            Ok(Some(Ok(b))) => buf.push_str(&String::from_utf8_lossy(&b)),
            other => panic!("no trashed event: {other:?}"),
        }
    }
    let (s, _) = get_json(&env, &format!("/api/library/card/{id}")).await;
    assert_eq!(s, 404, "a trashed clip has no card");
    let (_, d) = get_json(&env, "/api/library/summary").await;
    assert!(
        d["summary"].as_str().unwrap().starts_with("0 clips, "),
        "{d}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_live_updates_tell_a_proxy_not_to_hold_them_back() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let r = env
        .client
        .get(format!("{}/api/events", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let h = r.headers().clone();
    drop(r);
    assert_eq!(h["content-type"], "text/event-stream", "{h:?}");
    assert_eq!(
        h.get("x-accel-buffering").and_then(|v| v.to_str().ok()),
        Some("no"),
        "{h:?}"
    );
    assert_eq!(h["cache-control"], "no-cache", "{h:?}");
    for path in ["/", "/api/jobs", "/healthz"] {
        let r = env
            .client
            .get(format!("{}{path}", env.base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "{path}");
        assert!(
            r.headers().get("x-accel-buffering").is_none(),
            "{path}: only the event stream asks for no buffering"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_device_api_hands_over_the_shape_of_the_settings_pages() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let code = pair_code(&env).await;
    let (s, v) = try_pair(&env, &code, "gaming PC").await;
    assert_eq!(s, 201, "{v}");
    let token = v["token"].as_str().unwrap().to_string();
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::GET,
        "/api/device/sections",
        None,
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let ids: Vec<&str> = v["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "general",
            "storage",
            "ingest",
            "sharing",
            "imports",
            "notifications",
            "access",
            "devices",
            "appearance",
            "advanced"
        ],
        "reel's sections in reel's order"
    );
    assert_eq!(v["sections"][0]["label"], "General");
    assert!(v["sections"][0]["line"].as_str().unwrap().len() > 10);
    assert!(v["sections"][0]["changed"].is_u64());
    assert_eq!(v["who"]["name"], "you");
    assert_eq!(v["who"]["admin"], true);
    assert!(["ok", "warn", "err"].contains(&v["health"]["state"].as_str().unwrap()));
    assert!(v["jobs"]["queued"].is_u64() && v["jobs"]["failed"].is_u64());
    let (s, _) = dev_json(
        &env,
        "nope",
        reqwest::Method::GET,
        "/api/device/sections",
        None,
    )
    .await;
    assert_eq!(s, 401, "the shape needs a paired device");
}

#[tokio::test]
async fn the_apps_parts_in_the_header_stay_hidden_in_a_browser() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    for path in ["/", "/settings/general"] {
        let html = env
            .client
            .get(format!("{}{path}", env.base))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            html.contains(r#"id="app-share" type="button" aria-label="Share the last clip" hidden"#)
                && html.contains(r#"id="part-app" hidden"#)
                && html.contains(r#"<label class="toggle app-part" hidden>Pause uploads <button type="button" class="switch" role="switch" aria-checked="false" id="app-pause">"#),
            "{path}: the app's parts are in the header and the status chip, hidden until the app unhides them"
        );
        assert_eq!(
            html.matches("class=\"wordmark\"").count(),
            1,
            "{path}: one header, one wordmark"
        );
    }
    let html = env
        .client
        .get(format!("{}/settings/general", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        html.contains(r#"<span class="sections-head app-link" hidden>This app</span>"#)
            && html.contains(r##"<a class="app-link" href="#" data-app-section="status" hidden>Status</a>"##)
            && html.contains(r##"<a class="app-link" href="#" data-app-section="log" hidden>Log</a>"##),
        "the group This app with the app's nine pages sits under the sections, hidden until the app unhides it"
    );
    let js = env
        .client
        .get(format!("{}/assets/js/app-shell.js", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(
        js.status(),
        200,
        "the shell script is served with the others"
    );
}

async fn mean_volume(ff: &Ffmpeg, path: &std::path::Path, track: usize) -> f64 {
    let log = ff
        .analyse(&[
            "-i".into(),
            path.to_string_lossy().into_owned(),
            "-map".into(),
            format!("0:a:{track}"),
            "-af".into(),
            "volumedetect".into(),
            "-f".into(),
            "null".into(),
            "-".into(),
        ])
        .await
        .unwrap();
    let line = log
        .lines()
        .find(|l| l.contains("mean_volume:"))
        .expect("volumedetect prints the mean");
    line.split("mean_volume:")
        .nth(1)
        .unwrap()
        .trim()
        .trim_end_matches(" dB")
        .parse()
        .unwrap()
}

fn packet_md5s(ffmpeg: &str, path: &std::path::Path, stream: &str) -> Vec<String> {
    let out = std::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-map", stream, "-c", "copy", "-f", "framemd5", "-"])
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.rsplit(',').next().map(|h| h.trim().to_string()))
        .collect()
}

async fn cut_with(
    env: &Env,
    clip_id: &str,
    body: serde_json::Value,
) -> reel_store::models::Rendition {
    let r = env
        .client
        .post(format!("{}/api/clips/{clip_id}/cuts", env.base))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "{body}");
    let d: serde_json::Value = r.json().await.unwrap();
    let j = env
        .wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(120))
        .await;
    assert_eq!(j.state, "done", "{body}: {:?}", j.error);
    env.store()
        .rendition(d["rendition_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_volume_makes_a_version_with_the_video_copied_and_the_level_measured() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("a quiet tone.mp4", Synth::small(6.0, "quiet"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let master_path = env.ctx.config().abs(master.path.as_deref().unwrap());
    assert!(master.audio_measured_at.is_some(), "measured at ingest");
    let peak = master.audio_peak.expect("a tone has a peak");
    let loudness = master.audio_loudness.expect("and a loudness");
    assert!(
        (-19.0..-14.0).contains(&peak),
        "lavfi's sine peaks near -18 dBFS: {peak}"
    );
    assert!(loudness < peak, "{loudness} under {peak}");

    sqlx::query("UPDATE rendition SET audio_peak = NULL, audio_loudness = NULL, audio_measured_at = NULL WHERE id = ?")
        .bind(&master.id)
        .execute(env.store().pool())
        .await
        .unwrap();
    let page = |id: String| {
        let env = &env;
        async move {
            env.client
                .get(format!("{}/clips/{id}", env.base))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        }
    };
    let html = page(clip.id.clone()).await;
    assert!(
        html.contains("\"measured\":false"),
        "a version from before is not measured yet"
    );
    assert!(
        html.contains("id=\"gain\"")
            && html.contains("id=\"normalize\"")
            && html.contains("id=\"level-line\"")
    );
    let level: serde_json::Value = env
        .client
        .get(format!("{}/api/renditions/{}/level", env.base, master.id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(level["audio"], true);
    assert!(
        (level["peak"].as_f64().unwrap() - peak).abs() < 0.05,
        "{level}"
    );
    let html = page(clip.id.clone()).await;
    assert!(
        html.contains("\"measured\":true"),
        "measured on the first open, kept"
    );
    assert!(
        html.contains(&format!("\"peak\":{peak}")),
        "the editor gets the peak"
    );

    let video = env.ctx.ff.stream_md5(&master_path, "0:v:0").await.unwrap();
    let before = mean_volume(&env.ctx.ff, &master_path, 0).await;
    let up = cut_with(&env, &clip.id, serde_json::json!({ "gain_db": 6.0 })).await;
    let up_path = env.ctx.config().abs(up.path.as_deref().unwrap());
    let after = mean_volume(&env.ctx.ff, &up_path, 0).await;
    assert!(
        (after - before - 6.0).abs() <= 0.5,
        "+6 dB: {before} to {after}"
    );
    assert_eq!(up.video_copied, 1, "an audio change keeps the video copied");
    assert_eq!(
        env.ctx.ff.stream_md5(&up_path, "0:v:0").await.unwrap(),
        video,
        "the video stream is the original's, bit for bit"
    );
    assert_eq!(up.acodec.as_deref(), Some("aac"));
    assert!(up.audio_measured_at.is_some() && up.audio_peak.unwrap() > peak + 5.0);
    assert!(up.recipe.as_deref().unwrap().contains("\"gain_db\":6.0"));
    let fresh = env.store().clip(&clip.id).await.unwrap().unwrap();
    assert_eq!(
        fresh.current_id.as_deref(),
        Some(up.id.as_str()),
        "a new version, made current"
    );
    assert!(master_path.exists(), "the original stays");
    let html = page(clip.id.clone()).await;
    assert!(
        html.contains("+6 dB, video copied"),
        "the versions say what changed"
    );

    let down = cut_with(&env, &clip.id, serde_json::json!({ "gain_db": -6.5 })).await;
    let down_after = mean_volume(
        &env.ctx.ff,
        &env.ctx.config().abs(down.path.as_deref().unwrap()),
        0,
    )
    .await;
    assert!(
        (down_after - before + 6.5).abs() <= 0.5,
        "-6.5 dB: {before} to {down_after}"
    );

    let target = env.ctx.config().media.loudness_target as f64;
    let norm = cut_with(
        &env,
        &clip.id,
        serde_json::json!({ "normalize": true, "gain_db": 3.0 }),
    )
    .await;
    assert!(
        norm.recipe
            .as_deref()
            .unwrap()
            .contains("\"loudness\":-16.0")
            && !norm.recipe.as_deref().unwrap().contains("gain_db")
    );
    let lufs = norm.audio_loudness.unwrap();
    assert!(
        (lufs - target).abs() <= 1.0,
        "normalised to {target} LUFS: {lufs}"
    );
    assert_eq!(norm.video_copied, 1);

    let trim = cut_with(
        &env,
        &clip.id,
        serde_json::json!({ "end": 4.0, "gain_db": 0.0, "normalize": false }),
    )
    .await;
    let ffmpeg = env.ctx.config().media.ffmpeg.clone();
    let kept = packet_md5s(
        &ffmpeg,
        &env.ctx.config().abs(trim.path.as_deref().unwrap()),
        "0:a:0",
    );
    let all = packet_md5s(&ffmpeg, &master_path, "0:a:0");
    assert!(kept.len() > 100 && kept.len() < all.len());
    assert_eq!(
        kept[..],
        all[..kept.len()],
        "gain 0 copies the audio packets as they were"
    );

    let muted = cut_with(
        &env,
        &clip.id,
        serde_json::json!({ "mute": true, "gain_db": 6.0, "normalize": true }),
    )
    .await;
    assert!(muted.acodec.is_none(), "mute wins");
    assert_eq!(
        muted.recipe.as_deref(),
        Some(r#"{"start":null,"end":null,"mute":true}"#)
    );

    for (body, why) in [
        (serde_json::json!({ "gain_db": 0.3 }), "half-decibel steps"),
        (serde_json::json!({ "gain_db": 20.5 }), "within the range"),
        (
            serde_json::json!({ "gain_db": 0.0 }),
            "gain 0 on the whole clip changes nothing",
        ),
    ] {
        let r = env
            .client
            .post(format!("{}/api/clips/{}/cuts", env.base, clip.id))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400, "{why}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_boost_on_a_loud_clip_stays_under_the_ceiling() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let loud = Synth {
        tone_db: 15.0,
        ..Synth::small(6.0, "loud")
    };
    env.synth("a loud tone.mp4", loud).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(
        master.audio_peak.unwrap() > -4.0,
        "a loud clip: {:?}",
        master.audio_peak
    );
    let master_path = env.ctx.config().abs(master.path.as_deref().unwrap());
    let ceiling = reel_media::args::PEAK_CEILING;
    for (body, copied) in [
        (serde_json::json!({ "gain_db": 20.0 }), 1),
        (
            serde_json::json!({ "start": 1.3, "end": 5.0, "gain_db": 20.0 }),
            0,
        ),
    ] {
        let r = cut_with(&env, &clip.id, body.clone()).await;
        assert_eq!(r.video_copied, copied, "{body}");
        let path = env.ctx.config().abs(r.path.as_deref().unwrap());
        let level = env.ctx.ff.audio_level(&path).await.unwrap();
        let tp = level.peak.unwrap();
        assert!(tp < ceiling, "{body}: true peak {tp} dBTP under {ceiling}");
        assert!(
            level.loudness.unwrap() > master.audio_loudness.unwrap() + 1.0,
            "louder all the same"
        );
        let probe = env.ctx.ff.probe(&path).await.unwrap();
        let len = if copied == 1 {
            master.duration.unwrap()
        } else {
            3.7
        };
        assert!(
            (probe.duration - len).abs() < 0.2,
            "{body}: {} s",
            probe.duration
        );
    }
    assert!(master_path.exists());
}

async fn band_volume(ff: &Ffmpeg, path: &std::path::Path, track: usize, freq: u32) -> f64 {
    let log = ff
        .analyse(&[
            "-i".into(),
            path.to_string_lossy().into_owned(),
            "-map".into(),
            format!("0:a:{track}"),
            "-af".into(),
            format!(
                "{},volumedetect",
                vec![format!("bandpass=f={freq}:width_type=h:w=40"); 4].join(",")
            ),
            "-f".into(),
            "null".into(),
            "-".into(),
        ])
        .await
        .unwrap();
    let line = log.lines().find(|l| l.contains("mean_volume:")).unwrap();
    line.split("mean_volume:")
        .nth(1)
        .unwrap()
        .trim()
        .trim_end_matches(" dB")
        .parse()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_clip_with_three_tracks_keeps_them_and_a_cut_mixes_them_as_set() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let spec = Synth {
        audio_tracks: 3,
        track_titles: vec!["Game".into(), "Mic".into(), "Discord".into()],
        ..Synth::small(6.0, "three tracks")
    };
    env.synth("three tracks.mkv", spec).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let master_path = env.ctx.config().abs(master.path.as_deref().unwrap());
    assert_eq!(
        env.ctx.ff.audio_streams(&master_path).await.unwrap(),
        3,
        "the original keeps every track"
    );
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains(r#""tracks":["Game","Mic","Discord"]"#),
        "the editor gets the tracks by name: {}",
        page.lines()
            .find(|l| l.contains("level-json"))
            .unwrap_or("")
    );

    let cut = cut_with(&env, &clip.id, serde_json::json!({ "end": 4.0 })).await;
    let cut_path = env.ctx.config().abs(cut.path.as_deref().unwrap());
    assert_eq!(
        env.ctx.ff.audio_streams(&cut_path).await.unwrap(),
        1,
        "a version plays one track"
    );
    let ffmpeg = env.ctx.config().media.ffmpeg.clone();
    let kept = packet_md5s(&ffmpeg, &cut_path, "0:a:0");
    assert_eq!(
        kept[..],
        packet_md5s(&ffmpeg, &master_path, "0:a:0")[..kept.len()],
        "untouched, the first track alone, copied"
    );

    let mix = cut_with(&env, &clip.id, serde_json::json!({ "tracks": [{ "on": true }, { "on": false }, { "on": true, "gain_db": -6.0 }] })).await;
    let mix_path = env.ctx.config().abs(mix.path.as_deref().unwrap());
    assert_eq!(
        env.ctx.ff.audio_streams(&mix_path).await.unwrap(),
        1,
        "the tracks mixed into one"
    );
    assert_eq!(mix.video_copied, 1);
    let ff = &env.ctx.ff;
    let (game, mic, discord) = (
        band_volume(ff, &master_path, 0, 440).await,
        band_volume(ff, &master_path, 1, 660).await,
        band_volume(ff, &master_path, 2, 880).await,
    );
    let (m440, m660, m880) = (
        band_volume(ff, &mix_path, 0, 440).await,
        band_volume(ff, &mix_path, 0, 660).await,
        band_volume(ff, &mix_path, 0, 880).await,
    );
    assert!(
        (m440 - game).abs() <= 1.0,
        "Game as it was: {game} and {m440}"
    );
    assert!(
        m660 < mic - 30.0,
        "the muted Mic is not in the mix: {mic} and {m660}"
    );
    assert!(
        (m880 - (discord - 6.0)).abs() <= 1.0,
        "Discord 6 dB down: {discord} and {m880}"
    );
    let html = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        html.contains("track 1 and track 3 -6 dB"),
        "the version says which tracks"
    );
    let track = env
        .client
        .get(format!(
            "{}/media/renditions/{}/track/2.m4a",
            env.base, master.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        track.status(),
        200,
        "one track of the original for the editor's preview"
    );
    let tmp = env._dir.path().join("track2.m4a");
    std::fs::write(&tmp, track.bytes().await.unwrap()).unwrap();
    assert_eq!(env.ctx.ff.audio_streams(&tmp).await.unwrap(), 1);
    assert!(
        band_volume(ff, &tmp, 0, 880).await > band_volume(ff, &tmp, 0, 440).await + 30.0,
        "the third track's own sound"
    );
    let bad = env
        .client
        .post(format!("{}/api/clips/{}/cuts", env.base, clip.id))
        .json(&serde_json::json!({ "tracks": [{ "on": true, "gain_db": 0.3 }] }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400, "a track's gain in half decibels too");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_mp4_without_track_titles_takes_the_names_its_device_gave() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let (token, _) = pair(&env, "pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let file = env
        .synth_at(
            &pc,
            "replay.mp4",
            Synth {
                audio_tracks: 3,
                ..Synth::small(4.0, "three untitled tracks")
            },
        )
        .await;
    let (s, v) = upload_whole(
        &env,
        &token,
        &file,
        400_000,
        serde_json::json!({ "audio_names": ["Full mix", "Microphone", "Desktop"] }),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let clip_id = v["clip_id"].as_str().unwrap().to_string();
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    assert_eq!(clip.id, clip_id);
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains(r#""tracks":["Full mix","Microphone","Desktop"]"#),
        "the editor names the tracks as the device did: {}",
        page.lines()
            .find(|l| l.contains("level-json"))
            .unwrap_or("")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_replay_that_asks_for_its_last_seconds_is_published_with_them_current() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let (token, _) = pair(&env, "pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let file = env
        .synth_at(&pc, "replay.mp4", Synth::small(20.0, "replay"))
        .await;
    let bytes = std::fs::read(&file).unwrap();
    let (s, v) = dev_json(
        &env,
        &token,
        reqwest::Method::POST,
        "/api/device/uploads",
        Some(serde_json::json!({
            "file_name": "replay.mp4",
            "size": bytes.len(),
            "sha256": reel_pipeline::hash::sha256_of_bytes(&bytes),
            "keep_last": 3,
        })),
    )
    .await;
    assert_eq!(s, 400, "{v}: under 5 s is refused");
    let (s, v) = upload_whole(
        &env,
        &token,
        &file,
        400_000,
        serde_json::json!({ "keep_last": 7 }),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let clip_id = v["clip_id"].as_str().unwrap().to_string();
    let start_at = Instant::now();
    let url = loop {
        let (s, v) = dev_json(
            &env,
            &token,
            reqwest::Method::POST,
            "/api/device/share",
            None,
        )
        .await;
        let clip = env.store().clip(&clip_id).await.unwrap().unwrap();
        if clip.state == "ready" {
            assert_ne!(
                clip.current_id, clip.master_id,
                "published with the trim current, never the whole file first"
            );
        }
        if s == 200 {
            break v["url"].as_str().unwrap().to_string();
        }
        assert_eq!(s, 202, "{v}");
        assert!(
            start_at.elapsed() < Duration::from_secs(120),
            "no link in time"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let clip = env.store().clip(&clip_id).await.unwrap().unwrap();
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let cut = env
        .store()
        .rendition(clip.current_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(
        (master.duration.unwrap() - 20.0).abs() < 0.2,
        "the whole file stays the original"
    );
    assert_eq!(cut.kind, "cut");
    assert_eq!(cut.video_copied, 1, "started on a keyframe: a stream copy");
    let recipe: reel_core::CutRecipe =
        serde_json::from_str(cut.recipe.as_deref().unwrap()).unwrap();
    assert_eq!(recipe.start, Some(12.0), "the keyframe at or before 13 s");
    assert!(
        (cut.duration.unwrap() - 8.0).abs() < 0.2,
        "{:?}",
        cut.duration
    );
    let page = env
        .client
        .get(&url)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains(&format!("?r={}", cut.id)) || page.contains(&cut.id),
        "the link serves the trim"
    );
    let html = env
        .client
        .get(format!("{}/clips/{clip_id}", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        html.contains("data-seconds=\"15\"") && html.contains("Keep the last 60 s"),
        "the editor's quick trims"
    );
    assert!(
        html.contains("0:12.00 to 0:20.00"),
        "the version says what it keeps"
    );
}

async fn mock_github_release(version: &'static str) -> String {
    use axum::extract::Path;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::get;
    let allowed = |repo: &str, h: &HeaderMap| match repo {
        "pub" => true,
        "r" => h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer t0ken"),
        _ => false,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let assets_base = base.clone();
    let app = axum::Router::new()
        .route(
            &format!("/repos/o/{{repo}}/releases/tags/v{version}"),
            get(move |Path(repo): Path<String>, h: HeaderMap| {
                let base = format!("{}/{repo}", assets_base);
                async move {
                    if !allowed(&repo, &h) {
                        return StatusCode::NOT_FOUND.into_response();
                    }
                    axum::Json(serde_json::json!({ "assets": [
                        { "name": format!("reel-light_{version}_x64-setup.exe"), "url": format!("{base}/assets/1") },
                        { "name": format!("reel-light_{version}_x64-setup.exe.sig"), "url": format!("{base}/assets/2") },
                        { "name": format!("reel-light_{version}_x64-setup.exe.sha256"), "url": format!("{base}/assets/3") },
                        { "name": format!("reel_{version}_x64-setup.exe"), "url": format!("{base}/assets/4") },
                        { "name": format!("reel_{version}_x64-setup.exe.sig"), "url": format!("{base}/assets/5") },
                        { "name": format!("reel_{version}_amd64.AppImage"), "url": format!("{base}/assets/6") },
                        { "name": format!("reel_{version}_amd64.AppImage.sig"), "url": format!("{base}/assets/7") },
                        { "name": format!("reel_{version}_amd64.deb"), "url": format!("{base}/assets/8") },
                        { "name": format!("reel_{version}_amd64.deb.sig"), "url": format!("{base}/assets/9") },
                    ] }))
                    .into_response()
                }
            }),
        )
        .route(
            "/{repo}/assets/{n}",
            get(move |Path((repo, n)): Path<(String, u32)>, h: HeaderMap| async move {
                let octets = h.get("accept").and_then(|v| v.to_str().ok()) == Some("application/octet-stream");
                if !allowed(&repo, &h) || !octets {
                    return StatusCode::NOT_FOUND.into_response();
                }
                match n {
                    1 => b"MZ reel light".to_vec().into_response(),
                    2 => b"dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==\n".to_vec().into_response(),
                    4 => b"MZ reel, OBS included".to_vec().into_response(),
                    5 => b"d2l0aCBvYnM=\n".to_vec().into_response(),
                    6 => b"\x7fELF the AppImage".to_vec().into_response(),
                    7 => b"YXBwaW1hZ2U=\n".to_vec().into_response(),
                    8 => b"!<arch> the deb".to_vec().into_response(),
                    9 => b"ZGVi\n".to_vec().into_response(),
                    _ => StatusCode::NOT_FOUND.into_response(),
                }
            }),
        );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

#[tokio::test(flavor = "multi_thread")]
async fn a_paired_app_is_offered_the_signed_app_of_this_version() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| {
        c.device.app_updates = true;
    })
    .await
    else {
        return;
    };
    assert_eq!(
        reel_pipeline::app_update::github_api(),
        "http://127.0.0.1:1",
        "the server's own fetch of the release asks nobody"
    );
    let (token, _) = pair(&env, "pc").await;
    let v = reel_core::VERSION;
    let ask = |query: String, bearer: Option<String>| {
        let mut r = env
            .client
            .get(format!("{}/api/device/update{query}", env.base));
        if let Some(b) = bearer {
            r = r.bearer_auth(b);
        }
        async move {
            let r = r.send().await.unwrap();
            let status = r.status().as_u16();
            (status, r.bytes().await.unwrap().to_vec())
        }
    };
    let older = "?current=0.1.0&target=windows&arch=x86_64".to_string();
    assert_eq!(
        ask(older.clone(), Some(token.clone())).await.0,
        204,
        "nothing to offer before the installer is there"
    );
    assert_eq!(
        ask(older.clone(), None).await.0,
        401,
        "the device token, as the rest of the device API"
    );

    let gh = mock_github_release(reel_core::VERSION).await;
    let dir = reel_pipeline::app_update::dir(&env.ctx.config());
    use reel_pipeline::app_update::Flavour;
    assert!(
        !reel_pipeline::app_update::fetch_from_release(
            &gh,
            "o/r",
            v,
            "wrong",
            &dir,
            Flavour::Plain
        )
        .await
        .unwrap(),
        "a token the repository refuses finds nothing"
    );
    assert!(
        !reel_pipeline::app_update::fetch_from_release(&gh, "o/r", v, "", &dir, Flavour::Plain)
            .await
            .unwrap(),
        "a private fork's release without a token: nothing"
    );
    let open_dir = dir.join("public");
    for flavour in Flavour::ALL {
        assert!(
            reel_pipeline::app_update::fetch_from_release(&gh, "o/pub", v, "", &open_dir, flavour)
                .await
                .unwrap(),
            "a public repository's release needs no token"
        );
    }
    assert_eq!(
        std::fs::read(open_dir.join(format!("reel-light_{v}_x64-setup.exe"))).unwrap(),
        b"MZ reel light"
    );
    assert_eq!(
        std::fs::read(open_dir.join(format!("reel_{v}_x64-setup.exe"))).unwrap(),
        b"MZ reel, OBS included"
    );
    std::fs::remove_dir_all(&open_dir).unwrap();
    for flavour in Flavour::ALL {
        assert!(reel_pipeline::app_update::fetch_from_release(
            &gh, "o/r", v, "t0ken", &dir, flavour
        )
        .await
        .unwrap());
    }
    assert!(
        !reel_pipeline::app_update::fetch_from_release(
            &gh,
            "o/r",
            "9.9.9",
            "t0ken",
            &dir,
            Flavour::Plain
        )
        .await
        .unwrap(),
        "no such release"
    );

    let (s, body) = ask(older.clone(), Some(token.clone())).await;
    assert_eq!(s, 200);
    let m: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(m["version"], v);
    assert_eq!(m["signature"], "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==");
    let url = m["url"].as_str().unwrap();
    assert!(url.ends_with("/api/device/update/installer"), "{url}");
    assert!(m["pub_date"].as_str().unwrap().contains('T'));
    assert_eq!(
        ask(
            format!("?current={v}&target=windows-x86_64"),
            Some(token.clone())
        )
        .await
        .0,
        204,
        "the same version"
    );
    assert_eq!(
        ask(
            format!("?current={v}-dev.abc1234&target=windows-x86_64"),
            Some(token.clone())
        )
        .await
        .0,
        200,
        "a dev build of it"
    );
    let (s, body) = ask(
        "?current=0.1.0&target=linux&arch=x86_64".into(),
        Some(token.clone()),
    )
    .await;
    assert_eq!(s, 200);
    let m: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        (
            m["signature"].as_str(),
            m["url"]
                .as_str()
                .map(|u| u.ends_with("/api/device/update/installer?flavour=appimage"))
        ),
        (Some("YXBwaW1hZ2U="), Some(true)),
        "a Linux app is offered the AppImage, with its own signature"
    );
    assert_eq!(
        ask(
            "?current=0.1.0&target=darwin&arch=x86_64".into(),
            Some(token.clone())
        )
        .await
        .0,
        204,
        "Windows and Linux only"
    );

    let installer = |bearer: Option<String>| {
        let mut r = env
            .client
            .get(format!("{}/api/device/update/installer", env.base));
        if let Some(b) = bearer {
            r = r.bearer_auth(b);
        }
        async move {
            let r = r.send().await.unwrap();
            (r.status().as_u16(), r.bytes().await.unwrap().to_vec())
        }
    };
    let (s, bytes) = installer(Some(token.clone())).await;
    assert_eq!((s, bytes.as_slice()), (200, b"MZ reel light".as_slice()));
    assert_eq!(installer(None).await.0, 401);

    let apps = ["1.9.1", "1.10.0"]
        .into_iter()
        .filter(|c| reel_pipeline::app_update::offers_to(c));
    for current in apps {
        for (flavour, signature, file, bytes) in [
            (
                "&flavour=with-obs",
                "d2l0aCBvYnM=",
                format!("reel_{v}_x64-setup.exe"),
                b"MZ reel, OBS included".as_slice(),
            ),
            (
                "",
                "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==",
                format!("reel-light_{v}_x64-setup.exe"),
                b"MZ reel light".as_slice(),
            ),
        ] {
            let (s, body) = ask(
                format!("?current={current}&target=windows&arch=x86_64{flavour}"),
                Some(token.clone()),
            )
            .await;
            assert_eq!(s, 200, "an app of {current}{flavour}");
            let m: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(m["signature"], signature, "{current}{flavour}: {file}'s");
            let url = m["url"].as_str().unwrap();
            let r = env
                .client
                .get(format!(
                    "{}{}",
                    env.base,
                    &url[url.find("/api/").unwrap()..]
                ))
                .bearer_auth(&token)
                .send()
                .await
                .unwrap();
            let named = r
                .headers()
                .get("content-disposition")
                .and_then(|h| h.to_str().ok())
                .map(str::to_string);
            assert_eq!(
                (named, r.bytes().await.unwrap().as_ref()),
                (Some(format!("attachment; filename=\"{file}\"")), bytes),
                "an app of {current} with OBS gets reel, one without gets reel light ({flavour})"
            );
        }
    }
    let appimage = env
        .client
        .get(format!(
            "{}/api/device/update/installer?flavour=appimage",
            env.base
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(
        appimage.bytes().await.unwrap().as_ref(),
        b"\x7fELF the AppImage"
    );
    for (id, bytes) in [
        ("reel", b"MZ reel, OBS included".as_slice()),
        ("reel-light", b"MZ reel light".as_slice()),
        ("reel-appimage", b"\x7fELF the AppImage".as_slice()),
        ("reel-deb", b"!<arch> the deb".as_slice()),
    ] {
        let r = env
            .client
            .get(format!("{}/app/installer/{id}", env.base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "{id} for a signed-in user");
        assert_eq!(r.bytes().await.unwrap().as_ref(), bytes);
    }
    let devices = env
        .client
        .get(format!("{}/settings/devices", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        devices.contains("href=\"/app/installer/reel\"")
            && devices.contains("href=\"/app/installer/reel-light\"")
            && devices.contains("<b>reel, for Windows</b>")
            && devices.contains("<b>reel light, for Windows</b>")
            && devices.contains("href=\"/app/installer/reel-appimage\"")
            && devices.contains("href=\"/app/installer/reel-deb\"")
            && devices.contains("it does not update itself"),
        "Devices offers every installer, and says the .deb does not update itself"
    );

    let (s, _) = patch_setting(&env, serde_json::json!({ "device.app_updates": false })).await;
    assert_eq!(s, 200);
    assert_eq!(
        ask(older, Some(token)).await.0,
        204,
        "the switch under Devices"
    );
    let status = env
        .client
        .get(format!("{}/status", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(status.contains("app update"));
}

type Asked = Arc<std::sync::Mutex<Vec<(String, String, bool)>>>;

/// GitHub's latest release of `o/r`, reel light's installer without its signature; `o/spent`
/// answers as GitHub does once an address's hourly limit is spent.
async fn mock_latest_release(version: &'static str, asked: Asked) -> String {
    use axum::extract::Path;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::get;
    use reel_pipeline::app_update::{installer_name, Flavour};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let note = |asked: &Asked, path: String, h: &HeaderMap| {
        let ua = h
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        asked
            .lock()
            .unwrap()
            .push((path, ua, h.contains_key("authorization")));
    };
    let mut assets = Vec::new();
    for f in Flavour::ALL {
        let name = installer_name(version, f);
        assets.push(name.clone());
        if f != Flavour::Plain {
            assets.push(format!("{name}.sig"));
        }
    }
    let release = serde_json::json!({
        "tag_name": format!("v{version}"),
        "html_url": format!("{base}/o/r/releases/tag/v{version}"),
        "published_at": "2026-11-03T10:00:00Z",
        "body": "### Features\n\n* a standalone app installs its own updates",
        "assets": assets.iter().map(|n| serde_json::json!({
            "name": n,
            "browser_download_url": format!("{base}/o/r/releases/download/v{version}/{n}"),
            "url": format!("{base}/api-only/{n}"),
        })).collect::<Vec<_>>(),
    });
    let (a1, a2, a3) = (asked.clone(), asked.clone(), asked);
    let app = axum::Router::new()
        .route(
            "/repos/o/r/releases/latest",
            get(move |h: HeaderMap| async move {
                note(&a1, "/repos/o/r/releases/latest".into(), &h);
                axum::Json(release)
            }),
        )
        .route(
            "/repos/o/spent/releases/latest",
            get(move |h: HeaderMap| async move {
                note(&a2, "/repos/o/spent/releases/latest".into(), &h);
                (
                    StatusCode::FORBIDDEN,
                    [("x-ratelimit-remaining", "0")],
                    "API rate limit exceeded",
                )
                    .into_response()
            }),
        )
        .route(
            "/o/r/releases/download/{tag}/{name}",
            get(
                move |Path((_tag, name)): Path<(String, String)>, h: HeaderMap| async move {
                    note(&a3, format!("/download/{name}"), &h);
                    match name.strip_suffix(".sig") {
                        Some(of) => format!("the signature of {of}\n").into_response(),
                        None => b"the installer".to_vec().into_response(),
                    }
                },
            ),
        );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    base
}

#[tokio::test(flavor = "multi_thread")]
async fn the_standalone_library_hands_its_app_the_newer_release_of_its_kind() {
    use reel_pipeline::app_update::{installer_name, Flavour};
    let Some(env) = start_with(AuthMode::Single, vec![], |c| {
        c.make_standalone();
        c.advanced.update_repo = "o/r".into();
    })
    .await
    else {
        return;
    };
    let ask = |query: &str| {
        let r = env
            .client
            .get(format!("{}/api/app/update{query}", env.base));
        async move {
            let r = r.send().await.unwrap();
            let status = r.status().as_u16();
            let body: serde_json::Value = r.json().await.unwrap_or_default();
            (status, body)
        }
    };
    let with_obs = "?current=0.1.0&target=windows&arch=x86_64&flavour=with-obs";
    assert_eq!(
        ask(with_obs).await.0,
        204,
        "nothing before a read of the releases"
    );

    let asked: Asked = Default::default();
    let gh = mock_latest_release("99.0.0", asked.clone()).await;
    let read = reel_pipeline::updates::check_at(&env.ctx, &gh).await;
    assert_eq!(
        read.latest.as_ref().map(|r| r.version.as_str()),
        Some("99.0.0"),
        "{:?}",
        read.error
    );

    let (s, m) = ask(with_obs).await;
    assert_eq!(s, 200, "{m}");
    let exe = installer_name("99.0.0", Flavour::WithObs);
    assert_eq!(m["version"], "99.0.0");
    assert_eq!(
        m["url"],
        format!("{gh}/o/r/releases/download/v99.0.0/{exe}"),
        "the installer straight from the release, at its download address"
    );
    assert_eq!(m["signature"], format!("the signature of {exe}"));
    assert_eq!(m["pub_date"], "2026-11-03T10:00:00Z");
    assert!(m["notes"]
        .as_str()
        .unwrap()
        .contains("installs its own updates"));
    let (s, m) = ask("?current=0.1.0&target=linux&arch=x86_64").await;
    assert_eq!(s, 200);
    assert_eq!(
        m["url"],
        format!(
            "{gh}/o/r/releases/download/v99.0.0/{}",
            installer_name("99.0.0", Flavour::AppImage)
        ),
        "a Linux app is offered the AppImage"
    );
    assert_eq!(
        ask("?current=0.1.0&target=windows&arch=x86_64").await.0,
        204,
        "reel light's installer is on the release without its signature: not offered"
    );
    assert_eq!(
        ask("?current=99.0.0&target=windows&arch=x86_64&flavour=with-obs")
            .await
            .0,
        204,
        "an app as new as the release"
    );
    assert_eq!(
        ask("?current=0.1.0&target=darwin&arch=x86_64").await.0,
        204,
        "not an app the release has"
    );
    assert_eq!(ask(with_obs).await.0, 200);
    {
        let asked = asked.lock().unwrap();
        assert_eq!(
            asked
                .iter()
                .filter(|(p, _, _)| p == &format!("/download/{exe}.sig"))
                .count(),
            1,
            "a signature is read from the release once: {asked:?}"
        );
        assert!(
            !asked
                .iter()
                .any(|(p, _, _)| p.ends_with(".exe") || p.ends_with(".AppImage")),
            "reel on this PC downloads no installer itself: {asked:?}"
        );
        for (path, ua, auth) in asked.iter() {
            assert_eq!(ua, &format!("reel/{}", reel_core::VERSION), "{path}");
            assert!(!auth, "{path} carried an authorisation");
        }
    }

    env.ctx.updates.write().await.latest = Some(reel_pipeline::updates::Release {
        version: "99.1.0-rc.1".into(),
        ..read.latest.clone().unwrap()
    });
    assert_eq!(ask(with_obs).await.0, 204, "a test build is not offered");

    let (s, body) = patch_setting(
        &env,
        serde_json::json!({ "advanced.update_repo": "o/spent" }),
    )
    .await;
    assert_eq!(s, 200, "{body}");
    let read = reel_pipeline::updates::check_at(&env.ctx, &gh).await;
    assert!(
        read.error
            .as_deref()
            .unwrap_or("")
            .contains("60 questions an hour"),
        "{:?}",
        read.error
    );

    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let nobody = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let read = reel_pipeline::updates::check_at(&env.ctx, &nobody).await;
    assert!(
        read.error
            .as_deref()
            .unwrap_or("")
            .starts_with("GitHub could not be reached ("),
        "{:?}",
        read.error
    );

    let Some(server) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let r = server
        .client
        .get(format!("{}/api/app/update{with_obs}", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status().as_u16(),
        404,
        "a server's apps ask its device API, with their token"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn normalise_new_clips_publishes_a_levelled_version_and_keeps_the_original() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| c.media.normalize_new = true).await
    else {
        return;
    };
    let staged = env._dir.path().join("staged");
    std::fs::create_dir_all(&staged).unwrap();
    let file = env
        .synth_at(&staged, "a quiet tone.mp4", Synth::small(6.0, "quiet"))
        .await;
    let source_audio = env.ctx.ff.stream_md5(&file, "0:a:0").await.unwrap();
    std::fs::rename(&file, env.incoming().join("a quiet tone.mp4")).unwrap();
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    assert_ne!(
        clip.current_id, clip.master_id,
        "published with the levelled version current"
    );
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let current = env
        .store()
        .rendition(clip.current_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let target = env.ctx.config().media.loudness_target as f64;
    assert!(
        (current.audio_loudness.unwrap() - target).abs() <= 1.0,
        "{:?} against {target}",
        current.audio_loudness
    );
    assert_eq!(
        current.video_copied, 1,
        "the video copied, only the audio encoded again"
    );
    assert!(current
        .recipe
        .as_deref()
        .unwrap()
        .contains("\"loudness\":-16.0"));
    let master_path = env.ctx.config().abs(master.path.as_deref().unwrap());
    assert_eq!(
        env.ctx.ff.stream_md5(&master_path, "0:a:0").await.unwrap(),
        source_audio,
        "the original as it came"
    );
    assert!(
        (master.audio_loudness.unwrap() - target).abs() > 3.0,
        "the original keeps its own level"
    );

    let (token, _) = pair(&env, "pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let replay = env
        .synth_at(&pc, "replay.mp4", Synth::small(20.0, "replay"))
        .await;
    let (s, v) = upload_whole(
        &env,
        &token,
        &replay,
        400_000,
        serde_json::json!({ "keep_last": 7 }),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let id = v["clip_id"].as_str().unwrap().to_string();
    let replay_clip = env
        .wait_clip(
            |c| c.id == id && c.state == "ready",
            Duration::from_secs(120),
        )
        .await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let cuts: Vec<_> = env
        .store()
        .renditions_of(&id)
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.kind == "cut")
        .collect();
    assert_eq!(cuts.len(), 1, "the last seconds and the level in one cut");
    let cut = &cuts[0];
    assert_eq!(replay_clip.current_id.as_deref(), Some(cut.id.as_str()));
    let recipe: reel_core::CutRecipe =
        serde_json::from_str(cut.recipe.as_deref().unwrap()).unwrap();
    assert_eq!((recipe.start, recipe.loudness), (Some(12.0), Some(target)));
    assert!((cut.duration.unwrap() - 8.0).abs() < 0.2);
    assert!((cut.audio_loudness.unwrap() - target).abs() <= 1.0);
    let cut_jobs =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM job WHERE clip_id = ? AND kind = 'cut'")
            .bind(&id)
            .fetch_one(env.store().pool())
            .await
            .unwrap();
    assert_eq!(cut_jobs, 0, "made inside the ingest, no second job");
}

#[tokio::test(flavor = "multi_thread")]
async fn games_filter_the_library_and_a_collection_has_a_page_and_a_link() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let (token, _) = pair(&env, "pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let mut ids = Vec::new();
    for (name, game) in [
        ("one.mp4", Some("Valorant")),
        ("two.mp4", Some("Valorant")),
        ("three.mp4", None),
    ] {
        let f = env.synth_at(&pc, name, Synth::small(3.0, name)).await;
        let extra = match game {
            Some(g) => serde_json::json!({ "game": g }),
            None => serde_json::json!({}),
        };
        let (s, v) = upload_whole(&env, &token, &f, 400_000, extra).await;
        assert_eq!(s, 200, "{v}");
        ids.push(v["clip_id"].as_str().unwrap().to_string());
    }
    for id in &ids {
        env.wait_clip(
            |c| &c.id == id && c.state == "ready",
            Duration::from_secs(90),
        )
        .await;
    }
    let get = |path: String| {
        let env = &env;
        async move {
            let r = env
                .client
                .get(format!("{}{path}", env.base))
                .send()
                .await
                .unwrap();
            (r.status().as_u16(), r.text().await.unwrap())
        }
    };
    let (_, library) = get("/".into()).await;
    assert!(
        library.contains(
            r#"data-game="valorant"> <span>Valorant</span> <span class="count">2</span>"#
        ),
        "the game with its count"
    );
    assert!(
        library.contains(r#"data-game=""> <span>No game</span> <span class="count">1</span>"#),
        "and No game"
    );
    assert_eq!(
        library
            .matches(r#"data-game="valorant" data-sets="""#)
            .count(),
        2,
        "the cards carry their game"
    );

    let api = |method: reqwest::Method, path: String, body: Option<serde_json::Value>| {
        let env = &env;
        async move {
            let mut r = env.client.request(method, format!("{}{path}", env.base));
            if let Some(b) = body {
                r = r.json(&b);
            }
            let r = r.send().await.unwrap();
            let s = r.status().as_u16();
            (
                s,
                r.json::<serde_json::Value>()
                    .await
                    .unwrap_or(serde_json::Value::Null),
            )
        }
    };
    assert_eq!(
        api(
            reqwest::Method::POST,
            "/api/collections".into(),
            Some(serde_json::json!({ "name": "  " }))
        )
        .await
        .0,
        400
    );
    let (s, set) = api(
        reqwest::Method::POST,
        "/api/collections".into(),
        Some(serde_json::json!({ "name": "Best of" })),
    )
    .await;
    assert_eq!(s, 200);
    let set_id = set["id"].as_str().unwrap().to_string();
    for id in [&ids[0], &ids[2]] {
        assert_eq!(
            api(
                reqwest::Method::PUT,
                format!("/api/collections/{set_id}/clips/{id}"),
                None
            )
            .await
            .0,
            200
        );
    }
    let (_, library) = get("/".into()).await;
    assert!(
        library.contains(&format!(
            r#"data-set="{set_id}"> <span>Best of</span> <span class="count">2</span>"#
        )),
        "the collection as a pick"
    );
    assert_eq!(
        library.matches(&format!(r#"data-sets="{set_id}""#)).count(),
        2,
        "the cards carry their collections"
    );
    let (_, clip_page) = get(format!("/clips/{}", ids[0])).await;
    assert!(
        clip_page.contains(&format!(
            r#"data-set="{set_id}" checked> <span>Best of</span>"#
        )),
        "ticked on the clip page"
    );
    let (s, page) = get(format!("/collections/{set_id}")).await;
    assert_eq!(s, 200);
    assert!(
        page.contains(&format!("/clips/{}", ids[0]))
            && !page.contains(&format!("/clips/{}", ids[1])),
        "its own page lists its clips"
    );
    assert!(get("/collections".into()).await.1.contains("2 clips"));

    let (s, link) = api(
        reqwest::Method::POST,
        format!("/api/collections/{set_id}/link"),
        Some(serde_json::json!({})),
    )
    .await;
    assert_eq!(s, 200);
    let url = link["url"].as_str().unwrap().to_string();
    let path = url
        .split_once("/c/")
        .map(|(_, t)| format!("/c/{t}"))
        .unwrap();
    let public = reqwest::Client::new();
    let (s, list) = {
        let r = public
            .get(format!("{}{path}", env.base))
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.text().await.unwrap())
    };
    assert_eq!(s, 200);
    assert!(
        list.contains(&format!(r#"href="{path}/{}""#, ids[0]))
            && list.contains(&format!(r#"href="{path}/{}""#, ids[2])),
        "the link lists the clips"
    );
    assert!(
        og(&list)["og:image"].contains(&format!("{path}/")),
        "the first clip's poster for the preview"
    );
    let (s, one) = {
        let r = public
            .get(format!("{}{path}/{}", env.base, ids[0]))
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.text().await.unwrap())
    };
    assert_eq!(s, 200);
    let tags = og(&one);
    assert!(
        tags["og:video"].contains(&format!("{path}/{}/video.mp4", ids[0])),
        "plays as its share page, under the collection's link"
    );
    let video = tags["og:video"]
        .split_once("/c/")
        .map(|(_, t)| format!("/c/{t}"))
        .unwrap();
    let r = public
        .get(format!("{}{video}", env.base))
        .header("range", "bytes=0-99")
        .send()
        .await
        .unwrap();
    assert_eq!(
        (r.status().as_u16(), r.bytes().await.unwrap().len()),
        (206, 100),
        "range requests"
    );
    assert_eq!(
        public
            .get(format!("{}{path}/{}/poster.jpg", env.base, ids[0]))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        public
            .get(format!("{}{path}/{}", env.base, ids[1]))
            .send()
            .await
            .unwrap()
            .status(),
        404,
        "a clip outside the collection"
    );
    assert_eq!(
        public
            .get(format!("{}{path}/{}/video.mp4", env.base, ids[1]))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );

    let (_, again) = api(
        reqwest::Method::POST,
        format!("/api/collections/{set_id}/link"),
        None,
    )
    .await;
    assert_eq!(
        public
            .get(format!("{}{path}", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        404,
        "a new link retires the one before"
    );
    let path2 = again["url"]
        .as_str()
        .unwrap()
        .split_once("/c/")
        .map(|(_, t)| format!("/c/{t}"))
        .unwrap();
    assert_eq!(
        public
            .get(format!("{}{path2}", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        api(
            reqwest::Method::DELETE,
            format!("/api/clips/{}", ids[0]),
            None
        )
        .await
        .0,
        200
    );
    let list = public
        .get(format!("{}{path2}", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !list.contains(&ids[0]) && list.contains(&ids[2]),
        "a clip in the trash leaves the link's list"
    );
    assert_eq!(
        api(
            reqwest::Method::DELETE,
            format!("/api/collections/{set_id}/link"),
            None
        )
        .await
        .0,
        200
    );
    assert_eq!(
        public
            .get(format!("{}{path2}", env.base))
            .send()
            .await
            .unwrap()
            .status(),
        404,
        "revoked"
    );
    assert_eq!(
        api(
            reqwest::Method::PATCH,
            format!("/api/collections/{set_id}"),
            Some(serde_json::json!({ "name": "Best of 2026" }))
        )
        .await
        .0,
        200
    );
    assert!(get("/collections".into()).await.1.contains("Best of 2026"));
    assert_eq!(
        api(
            reqwest::Method::DELETE,
            format!("/api/collections/{set_id}"),
            None
        )
        .await
        .0,
        200
    );
    assert_eq!(get(format!("/collections/{set_id}")).await.0, 404);
    assert!(
        env.store().clip(&ids[2]).await.unwrap().is_some(),
        "the clips stay"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_knows_when_it_was_posted_and_how_often_it_was_opened() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("seen.mp4", Synth::small(3.0, "seen")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let (code, s) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", clip.id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(code, 200);
    assert_eq!(s["activity"], "not posted, not opened");
    let token = s["token"].as_str().unwrap().to_string();
    let public = reqwest::Client::new();
    let visit = |ua: &'static str, path: String| {
        let c = public.clone();
        let url = format!("{}{path}", env.base);
        async move {
            let r = c.get(url).header("user-agent", ua).send().await.unwrap();
            assert_eq!(r.status(), 200, "{ua}");
        }
    };
    const DISCORD: &str = "Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)";
    const BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0";
    const GOOGLE: &str = "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)";
    let share = || async { env.store().share_by_token(&token).await.unwrap().unwrap() };

    visit(DISCORD, format!("/s/{token}")).await;
    let s = share().await;
    assert!(
        s.last_posted_at.is_some(),
        "Discord building the preview is a post"
    );
    assert_eq!(s.views, 0, "and not a view");
    visit(BROWSER, format!("/s/{token}")).await;
    visit(GOOGLE, format!("/s/{token}")).await;
    let s = share().await;
    assert_eq!(
        s.views, 1,
        "a browser opens the link, a search robot does not"
    );
    assert!(s.last_viewed_at.is_some());
    let page = env
        .client
        .get(format!("{}/shares", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains(r#"<span class="word">posted </span><span class="short">just now</span>"#)
            && page
                .contains(r#"<span class="word">opened </span>1<span class="word"> time</span>"#),
        "the Shares page says both"
    );
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("posted just now, opened once"),
        "and the clip page"
    );

    let (_, set) = post_json_body(
        &env,
        "/api/collections",
        serde_json::json!({ "name": "Seen" }),
    )
    .await;
    let set = set["id"].as_str().unwrap().to_string();
    let r = env
        .client
        .put(format!(
            "{}/api/collections/{set}/clips/{}",
            env.base, clip.id
        ))
        .header("sec-fetch-site", "same-origin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let (code, l) = post_json_body(
        &env,
        &format!("/api/collections/{set}/link"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(code, 200);
    let ctoken = l["url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_string();
    let link = || async {
        env.store()
            .collection_link_by_token(&ctoken)
            .await
            .unwrap()
            .unwrap()
    };
    visit(DISCORD, format!("/c/{ctoken}/{}", clip.id)).await;
    let l = link().await;
    assert!(
        l.last_posted_at.is_some(),
        "a clip of a collection's link posted is the link posted"
    );
    assert_eq!(l.views, 0);
    visit(BROWSER, format!("/c/{ctoken}")).await;
    visit(BROWSER, format!("/c/{ctoken}/{}", clip.id)).await;
    let l = link().await;
    assert_eq!(l.views, 2, "the page and a clip of it opened");
    assert!(l.last_viewed_at.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_storage_secret_shows_nowhere() {
    const SECRET: &str = "wJalrXUtnFEMI-reel-test-secret-0123456789";
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let mut seen: Vec<(String, String)> = Vec::new();
    for (key, value) in [
        ("external.on", "true"),
        ("external.endpoint", "http://127.0.0.1:9"),
        ("external.bucket", "previews"),
        ("external.access_key", "AKIA-reel-test"),
        ("external.public_url", "http://127.0.0.1:9/previews"),
        ("external.secret_key", SECRET),
    ] {
        let (code, d) = patch_setting(&env, serde_json::json!({ key: value })).await;
        assert_eq!(code, 200, "{key}: {d}");
        seen.push((format!("the answer to saving {key}"), d.to_string()));
    }
    let (code, d) = patch_setting(
        &env,
        serde_json::json!({ "external.secret_key": format!("{SECRET}x") }),
    )
    .await;
    assert_eq!(code, 200);
    seen.push(("the answer to changing the secret".into(), d.to_string()));
    let (code, d) = patch_setting(&env, serde_json::json!({ "external.secret_key": SECRET })).await;
    assert_eq!(code, 200);
    seen.push(("the answer to changing it back".into(), d.to_string()));
    assert_eq!(
        env.ctx.config().external.secret_key,
        SECRET,
        "the secret is in force"
    );
    let mut page = String::new();
    for path in [
        "/settings/sharing",
        "/status",
        "/api/settings",
        "/api/settings/history",
        "/api/export/config",
    ] {
        let r = env
            .client
            .get(format!("{}{path}", env.base))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "{path}");
        let text = r.text().await.unwrap();
        if path == "/settings/sharing" {
            page = text.clone();
        }
        seen.push((path.into(), text));
    }
    assert!(
        page.contains("placeholder=\"set, type to replace\""),
        "the page says the secret is set"
    );
    let db = env
        .client
        .get(format!("{}/api/export/database", env.base))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert!(
        db.starts_with(b"SQLite format 3"),
        "the export is a database"
    );
    seen.push((
        "the database's export".into(),
        String::from_utf8_lossy(&db).into_owned(),
    ));
    let (_, d) = post_json(&env, "/api/status/doctor").await;
    seen.push(("doctor's answer".into(), d.to_string()));
    let (code, d) = post_json(&env, "/api/external/test").await;
    assert_eq!(code, 400, "nothing answers on port 9: {d}");
    seen.push(("the Test's answer".into(), d.to_string()));
    let cfg = env.ctx.config();
    seen.push(("the config's Debug".into(), format!("{cfg:?}")));
    seen.push((
        "the bucket's Debug".into(),
        format!("{:?}", reel_pipeline::external::bucket_of(&cfg)),
    ));
    for (place, text) in &seen {
        assert!(!text.contains(SECRET), "the secret is in {place}");
    }

    let file: toml::Value = toml::from_str(&format!(
        "[external]\non = true\nsecret_key = \"{SECRET}\"\n"
    ))
    .unwrap();
    let Some(locked) = start_layered(AuthMode::Single, vec![], |_| {}, Some(file), true).await
    else {
        return;
    };
    assert_eq!(locked.ctx.config().external.secret_key, SECRET);
    for path in ["/settings/sharing", "/api/settings", "/api/export/config"] {
        let text = locked
            .client
            .get(format!("{}{path}", locked.base))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            !text.contains(SECRET),
            "a secret from config.toml is in {path}"
        );
    }
}

struct FakeS3 {
    base: String,
    objects: Arc<std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>>,
    down: Arc<std::sync::atomic::AtomicBool>,
    put_ms: Arc<std::sync::atomic::AtomicU64>,
}

impl FakeS3 {
    fn keys(&self) -> Vec<String> {
        self.objects.lock().unwrap().keys().cloned().collect()
    }
}

async fn fake_s3() -> FakeS3 {
    use axum::http::{Method, StatusCode};
    use axum::response::IntoResponse;
    type Objects = Arc<std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>>;
    let objects: Objects = Arc::default();
    let down = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let put_ms = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let (o, d, p) = (objects.clone(), down.clone(), put_ms.clone());
    let app = axum::Router::new().fallback(move |req: axum::extract::Request| {
        let (objects, down, put_ms) = (o.clone(), d.clone(), p.clone());
        async move {
            let error = |status: StatusCode, code: &str| {
                (status, format!("<Error><Code>{code}</Code><Message>{code} from the test's storage</Message></Error>")).into_response()
            };
            if down.load(std::sync::atomic::Ordering::SeqCst) {
                return error(StatusCode::SERVICE_UNAVAILABLE, "ServiceUnavailable");
            }
            let method = req.method().clone();
            let path = req.uri().path().trim_start_matches('/').to_string();
            let query = req.uri().query().unwrap_or("").to_string();
            let range = req
                .headers()
                .get("range")
                .and_then(|v| v.to_str().ok())
                .map(String::from);
            let signed = req
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.starts_with("AWS4-HMAC-SHA256 Credential=test/"));
            let body = axum::body::to_bytes(req.into_body(), usize::MAX).await.unwrap();
            match method {
                Method::GET if query.contains("list-type=2") => {
                    if !signed {
                        return error(StatusCode::FORBIDDEN, "AccessDenied");
                    }
                    let bucket = path.trim_end_matches('/');
                    let items: String = objects
                        .lock()
                        .unwrap()
                        .iter()
                        .filter_map(|(k, v)| k.strip_prefix(&format!("{bucket}/")).map(|k| (k.to_string(), v.len())))
                        .map(|(k, n)| format!("<Contents><Key>{k}</Key><Size>{n}</Size></Contents>"))
                        .collect();
                    format!("<ListBucketResult><IsTruncated>false</IsTruncated>{items}</ListBucketResult>").into_response()
                }
                Method::GET | Method::HEAD => {
                    let Some(b) = objects.lock().unwrap().get(&path).cloned() else {
                        return error(StatusCode::NOT_FOUND, "NoSuchKey");
                    };
                    let len = b.len();
                    let range = range
                        .as_deref()
                        .and_then(|r| r.strip_prefix("bytes="))
                        .and_then(|r| r.split_once('-'))
                        .and_then(|(a, z)| Some((a.parse::<usize>().ok()?, z.parse::<usize>().ok()?.min(len - 1))));
                    let (status, body, extra) = match range {
                        Some((a, z)) if a <= z => (StatusCode::PARTIAL_CONTENT, b[a..=z].to_vec(), Some(format!("bytes {a}-{z}/{len}"))),
                        _ => (StatusCode::OK, b, None),
                    };
                    let n = body.len();
                    let mut r = (status, if method == Method::HEAD { Vec::new() } else { body }).into_response();
                    r.headers_mut().insert("content-length", n.into());
                    if let Some(cr) = extra {
                        r.headers_mut().insert("content-range", cr.parse().unwrap());
                    }
                    r
                }
                Method::PUT if signed => {
                    let ms = put_ms.load(std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                    objects.lock().unwrap().insert(path, body.to_vec());
                    StatusCode::OK.into_response()
                }
                Method::DELETE if signed => {
                    objects.lock().unwrap().remove(&path);
                    StatusCode::NO_CONTENT.into_response()
                }
                _ => error(StatusCode::FORBIDDEN, "AccessDenied"),
            }
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    FakeS3 {
        base,
        objects,
        down,
        put_ms,
    }
}

async fn object_behind(og: &str) -> (u16, Option<String>) {
    let r = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
        .get(og)
        .send()
        .await
        .unwrap();
    let to = r
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    (r.status().as_u16(), to)
}

#[tokio::test(flavor = "multi_thread")]
async fn link_previews_go_to_the_external_storage_and_follow_the_links() {
    let s3 = fake_s3().await;
    let public = format!("{}/previews", s3.base);
    let (endpoint, public_url) = (s3.base.clone(), public.clone());
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.external.on = true;
        c.external.endpoint = endpoint;
        c.external.bucket = "previews".into();
        c.external.access_key = "test".into();
        c.external.secret_key = "test-secret".into();
        c.external.public_url = public_url;
    })
    .await
    else {
        return;
    };
    env.synth("stored.mp4", Synth::h264_1080p(6.0, "stored"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let master_id = clip.master_id.clone().unwrap();
    let shares_path = format!("/api/clips/{}/shares", clip.id);
    let make = |pin: Option<&str>| {
        let body = match pin {
            Some(r) => serde_json::json!({ "rendition_id": r }),
            None => serde_json::json!({}),
        };
        post_json_body(&env, &shares_path, body)
    };
    let tags_of = |token: String| {
        let url = format!("{}/s/{token}", env.base);
        let c = env.client.clone();
        async move {
            let r = c.get(url).send().await.unwrap();
            assert_eq!(r.status(), 200);
            og(&r.text().await.unwrap())
        }
    };
    let token = |v: &serde_json::Value| v["token"].as_str().unwrap().to_string();

    let (code, follows) = make(None).await;
    assert_eq!(code, 200, "{follows}");
    assert_eq!(
        follows["preview"], "preview from storage",
        "the link waited for its upload"
    );
    assert_eq!(follows["preview_note"], "");
    assert_eq!(s3.keys().len(), 1);
    let video = tags_of(token(&follows)).await["og:video"].clone();
    assert_eq!(
        video,
        format!("{}/s/{}/video.mp4?r={master_id}", env.base, token(&follows)),
        "og:video is reel's own address for the version"
    );
    let (code, object) = object_behind(&video).await;
    let object = object.unwrap_or_default();
    assert_eq!(code, 302);
    assert!(
        object.starts_with(&format!("{public}/reel/{master_id}-")),
        "which sends the chat app to the object: {object}"
    );
    let got = reqwest::get(&video).await.unwrap();
    assert_eq!(got.status(), 200);
    let master = env.store().rendition(&master_id).await.unwrap().unwrap();
    assert_eq!(
        got.bytes().await.unwrap().len() as i64,
        master.size.unwrap(),
        "a version under the storage's size goes as it is"
    );
    let head = reqwest::Client::new().head(&video).send().await.unwrap();
    assert_eq!(
        (
            head.status().as_u16(),
            head.headers()["content-length"]
                .to_str()
                .unwrap()
                .to_string()
        ),
        (200, master.size.unwrap().to_string()),
        "a HEAD follows the redirect"
    );
    let part = reqwest::Client::new()
        .get(&video)
        .header("range", "bytes=0-99")
        .send()
        .await
        .unwrap();
    assert_eq!(part.status(), 206, "and so does a range");
    assert_eq!(part.bytes().await.unwrap().len(), 100);

    let (_, pinned) = make(Some(&master_id)).await;
    assert_eq!(pinned["preview"], "preview from storage");
    assert_eq!(
        s3.keys().len(),
        1,
        "two links of one version share its object"
    );

    let d: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/cuts", env.base, clip.id))
        .json(&serde_json::json!({ "start": 1.0, "end": 4.0 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cut_id = d["rendition_id"].as_str().unwrap().to_string();
    env.wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(60))
        .await;
    let deadline = Instant::now() + Duration::from_secs(60);
    let moved = loop {
        let v = tags_of(token(&follows)).await["og:video"].clone();
        let (_, to) = object_behind(&v).await;
        if v.ends_with(&format!("?r={cut_id}"))
            && to
                .as_deref()
                .is_some_and(|t| t.starts_with(&format!("{public}/reel/{cut_id}-")))
        {
            break v;
        }
        assert!(
            Instant::now() < deadline,
            "the following link did not move to the cut's preview: {v} {to:?}"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    };
    assert_eq!(reqwest::get(&moved).await.unwrap().status(), 200);
    let pinned_video = tags_of(token(&pinned)).await["og:video"].clone();
    assert_eq!(
        object_behind(&pinned_video).await.1.as_deref(),
        Some(object.as_str()),
        "the pinned link keeps its object"
    );
    assert_eq!(s3.keys().len(), 2);

    let r = env
        .client
        .delete(format!(
            "{}/api/shares/{}",
            env.base,
            pinned["id"].as_str().unwrap()
        ))
        .header("sec-fetch-site", "same-origin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let deadline = Instant::now() + Duration::from_secs(30);
    while s3.keys().len() != 1 {
        assert!(
            Instant::now() < deadline,
            "the revoked link's object stayed: {:?}",
            s3.keys()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(
        reqwest::get(&object).await.unwrap().status(),
        404,
        "the original's object is gone"
    );
    let old = reqwest::get(&video).await.unwrap();
    assert_eq!(
        old.status(),
        200,
        "and a message posted before the edit still plays: its og:video leads to the link's preview now"
    );
    let page = env
        .client
        .get(format!("{}/shares", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains(r#"title="preview from storage"><span class="short">storage</span><span class="long">from storage</span>"#),
        "the Shares page says where the preview comes from: a word, and its sentence"
    );

    s3.down.store(true, std::sync::atomic::Ordering::SeqCst);
    let (code, fallen) = make(Some(&master_id)).await;
    assert_eq!(code, 200);
    assert_eq!(
        fallen["preview"],
        "preview from your server: the upload failed"
    );
    assert_eq!(
        fallen["preview_note"],
        "Its preview comes from your server: the upload failed."
    );
    let fallen_video = tags_of(token(&fallen)).await["og:video"].clone();
    assert_eq!(
        object_behind(&fallen_video).await,
        (200, None),
        "the link's preview is served from here: {fallen_video}"
    );
    let page = env
        .client
        .get(format!("{}/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("preview from your server: the upload failed"),
        "the clip page says why"
    );

    s3.down.store(false, std::sync::atomic::Ordering::SeqCst);
    env.ctx.external.kick();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let (_, to) = object_behind(&fallen_video).await;
        if to
            .as_deref()
            .is_some_and(|t| t.starts_with(&format!("{public}/reel/{master_id}-")))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the storage came back and the preview did not go up again: {to:?}"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let r = env
        .client
        .delete(format!("{}/api/clips/{}", env.base, clip.id))
        .header("sec-fetch-site", "same-origin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !s3.keys().is_empty() || !env.store().external_objects().await.unwrap().is_empty() {
        assert!(
            Instant::now() < deadline,
            "a trashed clip's objects or their records stayed: {:?}",
            s3.keys()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_quick_presses_while_a_link_waits_for_its_preview_make_one_link() {
    let s3 = fake_s3().await;
    s3.put_ms.store(2500, std::sync::atomic::Ordering::SeqCst);
    let (endpoint, public_url) = (s3.base.clone(), format!("{}/previews", s3.base));
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.external.on = true;
        c.external.endpoint = endpoint;
        c.external.bucket = "previews".into();
        c.external.access_key = "test".into();
        c.external.secret_key = "test-secret".into();
        c.external.public_url = public_url;
        c.external.embed_mb = 1;
    })
    .await
    else {
        return;
    };
    env.synth("presses.mp4", Synth::h264_1080p(4.0, "presses"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let press = |body: serde_json::Value| {
        let (base, client, id) = (env.base.clone(), env.client.clone(), clip.id.clone());
        tokio::spawn(async move {
            let r = client
                .post(format!("{base}/api/clips/{id}/shares"))
                .header("sec-fetch-site", "same-origin")
                .json(&body)
                .send()
                .await
                .unwrap();
            (
                r.status().as_u16(),
                r.json::<serde_json::Value>().await.unwrap(),
            )
        })
    };
    // one request first: the user is made on first sight, and two at once both make it
    let r = env
        .client
        .get(format!("{}/api/disk", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let first = press(serde_json::json!({}));
    let deadline = Instant::now() + Duration::from_secs(20);
    let waits = loop {
        let w: serde_json::Value = env
            .client
            .get(format!("{}/api/clips/{}/link-wait", env.base, clip.id))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if w["step"] == "upload" {
            break w;
        }
        assert!(
            Instant::now() < deadline,
            "the button never heard the preview was going up: {w}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert!(
        waits["ready"] == false && (0.0..=1.0).contains(&waits["progress"].as_f64().unwrap()),
        "what the button says while the link waits: {waits}"
    );
    let second = press(serde_json::json!({}));
    let other = press(serde_json::json!({ "label": "for the team" }));
    assert!(
        !first.is_finished(),
        "the first press still waits for its preview"
    );
    let (a, b, c) = (
        first.await.unwrap(),
        second.await.unwrap(),
        other.await.unwrap(),
    );
    assert!(a.0 == 200 && b.0 == 200 && c.0 == 200, "{a:?} {b:?} {c:?}");
    assert_eq!(a.1["id"], b.1["id"], "two quick presses gave one link");
    assert_eq!(a.1["url"], b.1["url"]);
    assert_eq!(b.1["preview"], "preview from storage", "{}", b.1);
    assert_ne!(c.1["id"], a.1["id"], "a different ask is a link of its own");
    let shares = env.store().shares_of(&clip.id).await.unwrap();
    assert_eq!(shares.len(), 2, "one link per ask: {shares:?}");
    let (s, again) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", clip.id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(s, 200, "{again}");
    assert_ne!(
        again["id"], a.1["id"],
        "a press after the link was handed out makes a new one"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_apps_link_waits_for_its_preview_in_the_external_storage() {
    let s3 = fake_s3().await;
    s3.put_ms.store(1500, std::sync::atomic::Ordering::SeqCst);
    let public = format!("{}/previews", s3.base);
    let (endpoint, public_url) = (s3.base.clone(), public.clone());
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.external.on = true;
        c.external.endpoint = endpoint;
        c.external.bucket = "previews".into();
        c.external.access_key = "test".into();
        c.external.secret_key = "test-secret".into();
        c.external.public_url = public_url;
        c.external.embed_mb = 1;
    })
    .await
    else {
        return;
    };
    let (token, _) = pair(&env, "pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let file = env
        .synth_at(&pc, "one-key.mp4", Synth::h264_1080p(4.0, "one key"))
        .await;
    assert!(std::fs::metadata(&file).unwrap().len() > 1_000_000);
    let (s, v) = upload_whole(&env, &token, &file, 400_000, serde_json::json!({})).await;
    assert_eq!(s, 200, "{v}");
    let mut steps = Vec::new();
    let start_at = Instant::now();
    let url = loop {
        let (s, v) = dev_json(
            &env,
            &token,
            reqwest::Method::POST,
            "/api/device/share",
            None,
        )
        .await;
        if s == 200 {
            break v["url"].as_str().unwrap().to_string();
        }
        assert_eq!(s, 202, "{v}");
        let step = v["step"].as_str().unwrap().to_string();
        if steps.last() != Some(&step) {
            steps.push(step);
        }
        assert!(
            start_at.elapsed() < Duration::from_secs(180),
            "no link in time: {steps:?}"
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    };
    assert!(
        steps.contains(&"preview".to_string()),
        "the storage's copy was made first: {steps:?}"
    );
    assert_eq!(
        steps.last().map(String::as_str),
        Some("upload"),
        "then the link waited for its upload: {steps:?}"
    );
    let page = env
        .client
        .get(&url)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let video = og(&page)["og:video"].clone();
    let (code, to) = object_behind(&video).await;
    assert!(
        code == 302
            && to
                .as_deref()
                .is_some_and(|t| t.starts_with(&format!("{public}/reel/"))),
        "og:video leads to the object: {video} {code} {to:?}"
    );
    let got = reqwest::get(&video).await.unwrap().bytes().await.unwrap();
    assert!(
        got.len() < 1_300_000,
        "the copy at the storage's size went up, {} bytes",
        got.len()
    );
    let clip = env
        .store()
        .clips("", reel_store::Sort::Newest)
        .await
        .unwrap()
        .remove(0)
        .clip;
    let current = clip.current_id.unwrap();
    assert!(
        env.store()
            .derived_rendition(&current, "embed", None)
            .await
            .unwrap()
            .is_none(),
        "no copy at the server's size while the storage serves the link"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_waiting_for_its_preview_from_here_raises_its_copy_and_gets_it() {
    let s3 = fake_s3().await;
    s3.down.store(true, std::sync::atomic::Ordering::SeqCst);
    let (endpoint, public_url) = (s3.base.clone(), format!("{}/previews", s3.base));
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.external.on = true;
        c.external.endpoint = endpoint;
        c.external.bucket = "previews".into();
        c.external.access_key = "test".into();
        c.external.secret_key = "test-secret".into();
        c.external.public_url = public_url;
        c.share.embed_mb = 1;
        c.share.web_mbit = 2;
        c.pipeline.workers = 1;
    })
    .await
    else {
        return;
    };
    env.synth("fallback.mp4", Synth::h264_1080p(4.0, "fallback"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    env.ctx
        .hold_heavy
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let (code, _) = post_json(&env, "/api/storage/rebuild-derived").await;
    assert_eq!(code, 200);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !env
        .store()
        .active_jobs()
        .await
        .unwrap()
        .iter()
        .any(|j| j.kind == "derive" && j.state == "running")
    {
        assert!(Instant::now() < deadline, "the worker took no held job");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (base, client, id) = (env.base.clone(), env.client.clone(), clip.id.clone());
    let made = tokio::spawn(async move {
        let r = client
            .post(format!("{base}/api/clips/{id}/shares"))
            .header("sec-fetch-site", "same-origin")
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        (
            r.status().as_u16(),
            r.json::<serde_json::Value>().await.unwrap(),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let copy_job = loop {
        let jobs = env.store().active_jobs().await.unwrap();
        let web = jobs.iter().find(|j| j.kind == "web" && j.state == "queued");
        let copy = jobs
            .iter()
            .find(|j| j.kind == "embed" && matches!(j.state.as_str(), "queued" | "running"));
        if let (Some(j), Some(w)) = (copy, web) {
            if j.priority == reel_store::models::PRIORITY_EMBED && j.priority < w.priority {
                break j.clone();
            }
        }
        assert!(
            Instant::now() < deadline,
            "the server's copy a link waits for did not rise to a link's priority: {:?}",
            jobs.iter()
                .map(|j| (&j.kind, &j.state, j.priority))
                .collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert!(
        !made.is_finished(),
        "the link waits for its preview from here"
    );
    env.ctx
        .hold_heavy
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let (code, share) = tokio::time::timeout(Duration::from_secs(120), made)
        .await
        .expect("the link came")
        .unwrap();
    assert_eq!(code, 200, "{share}");
    assert_eq!(
        share["preview"],
        "preview from your server: the upload failed"
    );
    assert_eq!(
        share["preview_note"],
        "Its preview comes from your server: the upload failed."
    );
    let page = env
        .client
        .get(
            share["url"]
                .as_str()
                .unwrap()
                .replace(&env.ctx.config().public_url, &env.base),
        )
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let copy = env
        .store()
        .rendition(copy_job.rendition_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(
        copy.is_ready(),
        "the server's copy is made before the link is handed out"
    );
    let video = og(&page)["og:video"].clone();
    let (code, to) = object_behind(&video).await;
    let served = reqwest::get(&video).await.unwrap().bytes().await.unwrap();
    let files: Vec<_> = env
        .store()
        .renditions_of(&clip.id)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.kind, r.state, r.size, r.id))
        .collect();
    assert!(
        code == 200 && to.is_none() && served.len() as i64 == copy.size.unwrap(),
        "and og:video plays it from here: {video} {code} {to:?}, {} bytes, the copy {:?} {:?}; {files:?}",
        served.len(),
        copy.id,
        copy.size
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_storage_makes_no_copy_for_a_link_it_has_no_room_for() {
    let s3 = fake_s3().await;
    let (endpoint, public_url) = (s3.base.clone(), format!("{}/previews", s3.base));
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.external.endpoint = endpoint;
        c.external.bucket = "previews".into();
        c.external.access_key = "test".into();
        c.external.secret_key = "test-secret".into();
        c.external.public_url = public_url;
        c.external.embed_mb = 1;
        c.external.limit_gb = 0;
    })
    .await
    else {
        return;
    };
    env.synth("no-room.mp4", Synth::h264_1080p(4.0, "no room"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let (code, d) = patch_setting(&env, serde_json::json!({ "external.on": "true" })).await;
    assert_eq!(code, 200, "{d}");
    let (code, share) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", clip.id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(code, 200, "{share}");
    assert_eq!(
        share["preview"],
        "preview from your server: the storage's limit was reached"
    );
    let current = clip.current_id.unwrap();
    assert!(
        env.store()
            .derived_rendition(&current, "embed_ext", None)
            .await
            .unwrap()
            .is_none(),
        "no copy at the storage's size for a preview that has no room there"
    );
    assert!(s3.keys().is_empty());
}

type ObjectsAre = Box<dyn Fn(&[reel_store::models::ExternalObject]) -> bool + Send>;

#[tokio::test(flavor = "multi_thread")]
async fn previews_made_ahead_go_up_before_a_link_exists() {
    let s3 = fake_s3().await;
    let (endpoint, public_url) = (s3.base.clone(), format!("{}/previews", s3.base));
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.external.on = true;
        c.external.endpoint = endpoint;
        c.external.bucket = "previews".into();
        c.external.access_key = "test".into();
        c.external.secret_key = "test-secret".into();
        c.external.public_url = public_url;
        c.external.ahead = "edited_new".into();
    })
    .await
    else {
        return;
    };
    let objects = || async { env.store().external_objects().await.unwrap() };
    let wait_for = |what: &'static str, pred: ObjectsAre| {
        let store = env.store().clone();
        let ctx = env.ctx.clone();
        async move {
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                let o = store.external_objects().await.unwrap();
                if pred(&o) {
                    return o;
                }
                assert!(
                    Instant::now() < deadline,
                    "{what}: {:?}",
                    o.iter()
                        .map(|o| (&o.version_id, &o.class, &o.state))
                        .collect::<Vec<_>>()
                );
                ctx.external.kick();
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
    };
    env.synth("ahead.mp4", Synth::h264_1080p(5.0, "ahead"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let master = clip.master_id.clone().unwrap();
    let m = master.clone();
    wait_for(
        "a new clip's preview goes up with no link",
        Box::new(move |o| {
            o.iter()
                .any(|x| x.version_id == m && x.class == "new" && x.state == "stored")
        }),
    )
    .await;
    let d: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/cuts", env.base, clip.id))
        .json(&serde_json::json!({ "start": 1.0, "end": 4.0 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cut = d["rendition_id"].as_str().unwrap().to_string();
    env.wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(60))
        .await;
    let c = cut.clone();
    let m = master.clone();
    wait_for(
        "the edit's preview takes the original's place",
        Box::new(move |o| o.len() == 1 && o[0].version_id == c && o[0].class == "edited"),
    )
    .await;
    let started = Instant::now();
    let (code, share) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", clip.id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(code, 200);
    assert_eq!(share["preview"], "preview from storage");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a link to a preview made ahead is instant: {:?}",
        started.elapsed()
    );
    let _ = m;
    let c = cut.clone();
    wait_for(
        "the edited preview is a shared one now",
        Box::new(move |o| o.len() == 1 && o[0].version_id == c && o[0].class == "shared"),
    )
    .await;
    let (code, d) = post_json(&env, "/api/external/check").await;
    assert_eq!(code, 200);
    let line = d["line"].as_str().unwrap().to_string();
    assert!(
        line.contains(" of 5.0 GB (0 %): shared ") && line.contains(" in 1"),
        "{line}"
    );
    let status = env
        .client
        .get(format!("{}/status", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        status.contains("link storage"),
        "Status has the storage's line"
    );

    env.synth("ahead-2.mp4", Synth::h264_1080p(5.0, "ahead two"))
        .await;
    let other = env
        .wait_clip(
            |c| c.state == "ready" && c.id != clip.id,
            Duration::from_secs(120),
        )
        .await;
    let om = other.master_id.clone().unwrap();
    let o2 = om.clone();
    wait_for(
        "the second new clip's preview goes up",
        Box::new(move |o| o.iter().any(|x| x.version_id == o2 && x.class == "new")),
    )
    .await;
    let (code, _) = patch_setting(&env, serde_json::json!({ "external.ahead": "off" })).await;
    assert_eq!(code, 200);
    let c = cut.clone();
    wait_for(
        "with Made ahead off only the linked preview stays",
        Box::new(move |o| o.len() == 1 && o[0].version_id == c),
    )
    .await;
    assert_eq!(objects().await.len(), 1);
    assert_eq!(s3.keys().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cleanup_revokes_only_links_never_shared() {
    let s3 = fake_s3().await;
    let (endpoint, public_url) = (s3.base.clone(), format!("{}/previews", s3.base));
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.external.on = true;
        c.external.endpoint = endpoint;
        c.external.bucket = "previews".into();
        c.external.access_key = "test".into();
        c.external.secret_key = "test-secret".into();
        c.external.public_url = public_url;
        c.share.cleanup = true;
        c.share.cleanup_days = 30;
    })
    .await
    else {
        return;
    };
    env.synth("lonely.mp4", Synth::small(3.0, "lonely")).await;
    let lonely = env.wait_ready(Duration::from_secs(90)).await;
    env.synth("talked-about.mp4", Synth::small(3.0, "talked about"))
        .await;
    let talked = env
        .wait_clip(
            |c| c.state == "ready" && c.id != lonely.id,
            Duration::from_secs(90),
        )
        .await;
    let make = |clip: String, body: serde_json::Value| {
        let env = &env;
        async move {
            let (code, s) = post_json_body(env, &format!("/api/clips/{clip}/shares"), body).await;
            assert_eq!(code, 200, "{s}");
            (
                s["id"].as_str().unwrap().to_string(),
                s["token"].as_str().unwrap().to_string(),
            )
        }
    };
    let (never, _) = make(lonely.id.clone(), serde_json::json!({})).await;
    let (posted, posted_token) = make(talked.id.clone(), serde_json::json!({})).await;
    let (opened, opened_token) = make(talked.id.clone(), serde_json::json!({})).await;
    let (expiring, _) = make(
        talked.id.clone(),
        serde_json::json!({ "expires_in": "30d" }),
    )
    .await;
    let (legacy, _) = make(talked.id.clone(), serde_json::json!({})).await;
    let (young, _) = make(talked.id.clone(), serde_json::json!({})).await;
    let public = reqwest::Client::new();
    let r = public
        .get(format!("{}/s/{posted_token}", env.base))
        .header(
            "user-agent",
            "Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let r = public
        .get(format!("{}/s/{opened_token}", env.base))
        .header(
            "user-agent",
            "Mozilla/5.0 (X11; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let ago = |days: i64| {
        jiff::Timestamp::now()
            .checked_sub(jiff::SignedDuration::from_hours(24 * days))
            .unwrap()
            .to_string()
    };
    for id in [&never, &posted, &opened, &expiring, &legacy] {
        sqlx::query("UPDATE share SET created_at = ? WHERE id = ?")
            .bind(ago(40))
            .bind(id)
            .execute(env.store().pool())
            .await
            .unwrap();
    }
    // an hour short of ten days: the page counts whole days left, and a second's tick between
    // here and the page would make 20 days 19
    let young_at = jiff::Timestamp::now()
        .checked_sub(jiff::SignedDuration::from_hours(24 * 10 - 1))
        .unwrap()
        .to_string();
    sqlx::query("UPDATE share SET created_at = ? WHERE id = ?")
        .bind(young_at)
        .bind(&young)
        .execute(env.store().pool())
        .await
        .unwrap();
    sqlx::query("UPDATE share SET last_posted_at = ? WHERE id = ?")
        .bind(ago(35))
        .bind(&posted)
        .execute(env.store().pool())
        .await
        .unwrap();
    sqlx::query("UPDATE share SET views = 3, last_posted_at = NULL WHERE id = ?")
        .bind(&legacy)
        .execute(env.store().pool())
        .await
        .unwrap();

    let page = env
        .client
        .get(format!("{}/shares", env.base))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        page.contains("never shared, revoked at the next cleanup"),
        "Shares says which link goes next"
    );
    assert!(
        page.contains("never shared, revoked in 20 days"),
        "and when a younger one would"
    );
    let lonely_version = lonely.current_id.clone().unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !env
        .store()
        .external_objects()
        .await
        .unwrap()
        .iter()
        .any(|o| o.version_id == lonely_version && o.state == "stored")
    {
        assert!(
            Instant::now() < deadline,
            "the lonely link's preview never went up"
        );
        env.ctx.external.kick();
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let gone = reel_pipeline::housekeeping::cleanup_links(&env.ctx)
        .await
        .unwrap();
    assert_eq!(gone, 1, "one link never shared and old enough");
    let state = |id: String| {
        let store = env.store().clone();
        async move { store.share(&id).await.unwrap().unwrap() }
    };
    assert!(
        state(never.clone()).await.revoked_at.is_some(),
        "the link never posted nor opened is revoked"
    );
    for (id, why) in [
        (
            &posted,
            "posted once, quiet for longer than the days, stays",
        ),
        (&opened, "opened once stays"),
        (&expiring, "a link with its own expiry is left to it"),
        (&legacy, "an older link with views counts as shared"),
        (&young, "a link younger than the days stays"),
    ] {
        assert!(state(id.clone()).await.is_active(), "{why}");
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while env
        .store()
        .external_objects()
        .await
        .unwrap()
        .iter()
        .any(|o| o.version_id == lonely_version)
    {
        assert!(
            Instant::now() < deadline,
            "the revoked link's preview stayed in the storage"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(
        reel_pipeline::housekeeping::cleanup_links(&env.ctx)
            .await
            .unwrap(),
        0,
        "nothing more goes"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_or_expired_links_preview_leaves_the_storage_even_when_made_ahead_wants_it() {
    let s3 = fake_s3().await;
    let (endpoint, public_url) = (s3.base.clone(), format!("{}/previews", s3.base));
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.external.on = true;
        c.external.endpoint = endpoint;
        c.external.bucket = "previews".into();
        c.external.access_key = "test".into();
        c.external.secret_key = "test-secret".into();
        c.external.public_url = public_url;
        c.external.ahead = "edited_new".into();
    })
    .await
    else {
        return;
    };
    env.synth("revoked.mp4", Synth::small(5.0, "revoked")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let d: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/cuts", env.base, clip.id))
        .json(&serde_json::json!({ "start": 1.0, "end": 4.0 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cut = d["rendition_id"].as_str().unwrap().to_string();
    env.wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(60))
        .await;
    let key_of = |cut: String, not: Option<String>| {
        let store = env.store().clone();
        let ctx = env.ctx.clone();
        async move {
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                let o = store.external_objects().await.unwrap();
                if let Some(x) = o.iter().find(|x| {
                    x.version_id == cut && x.state == "stored" && Some(&x.key) != not.as_ref()
                }) {
                    return x.key.clone();
                }
                assert!(Instant::now() < deadline, "no object for the edit: {o:?}");
                ctx.external.kick();
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    };
    let first = key_of(cut.clone(), None).await;
    let (code, link) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", clip.id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(code, 200, "{link}");
    let page = env
        .client
        .get(format!(
            "{}/s/{}",
            env.base,
            link["token"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let (code, to) = object_behind(&og(&page)["og:video"]).await;
    assert!(
        code == 302 && to.as_deref().is_some_and(|t| t.ends_with(&first)),
        "{code} {to:?}"
    );
    let r = env
        .client
        .delete(format!(
            "{}/api/shares/{}",
            env.base,
            link["id"].as_str().unwrap()
        ))
        .header("sec-fetch-site", "same-origin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let second = key_of(cut.clone(), Some(first.clone())).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    while s3.keys().iter().any(|k| k.ends_with(&first)) {
        assert!(
            Instant::now() < deadline,
            "the revoked link's key is still in the storage"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(
        reqwest::get(format!("{}/previews/{first}", s3.base))
            .await
            .unwrap()
            .status(),
        404,
        "the address the revoked link handed out answers nothing"
    );
    assert_ne!(
        first, second,
        "and Made ahead put the edit up again under a new key"
    );

    let (code, expiring) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", clip.id),
        serde_json::json!({ "expires_in": "1d" }),
    )
    .await;
    assert_eq!(code, 200, "{expiring}");
    let page = env
        .client
        .get(format!(
            "{}/s/{}",
            env.base,
            expiring["token"].as_str().unwrap()
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let (code, to) = object_behind(&og(&page)["og:video"]).await;
    assert!(
        code == 302 && to.as_deref().is_some_and(|t| t.ends_with(&second)),
        "{code} {to:?}"
    );
    let soon = jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_secs(2))
        .unwrap()
        .to_string();
    sqlx::query("UPDATE share SET expires_at = ? WHERE id = ?")
        .bind(&soon)
        .bind(expiring["id"].as_str().unwrap())
        .execute(env.store().pool())
        .await
        .unwrap();
    env.ctx.external.kick();
    let deadline = Instant::now() + Duration::from_secs(20);
    while s3.keys().iter().any(|k| k.ends_with(&second)) {
        assert!(
            Instant::now() < deadline,
            "the expired link's key is still in the storage"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let third = key_of(cut.clone(), Some(second.clone())).await;
    assert_ne!(
        third, second,
        "the expired link's key went at its expiry, the edit up again under a new one"
    );
}

async fn job_for(env: &Env, rendition_id: &str) -> reel_store::models::Job {
    sqlx::query_as::<_, reel_store::models::Job>("SELECT * FROM job WHERE rendition_id = ?")
        .bind(rendition_id)
        .fetch_one(env.store().pool())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_someone_waits_for_runs_at_once_and_pauses_the_background_one() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| {
        c.pipeline.workers = 1;
        c.share.web_mbit = 2;
    })
    .await
    else {
        return;
    };
    env.synth("long.mp4", Synth::h264_1440p(30.0, "background"))
        .await;
    let clip = env.wait_ready(Duration::from_secs(120)).await;
    env.wait_no_jobs(Duration::from_secs(180)).await;
    let (s, v) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", clip.id),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let web = env
        .store()
        .renditions_of(&clip.id)
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.kind == "web")
        .expect("a web copy to make");
    let deadline = Instant::now() + Duration::from_secs(60);
    while job_for(&env, &web.id).await.state != "running"
        || job_for(&env, &web.id).await.progress <= 0.0
    {
        assert!(
            Instant::now() < deadline,
            "the web copy did not start: {:?}",
            job_for(&env, &web.id).await
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (s, v) = post_json_body(
        &env,
        &format!("/api/clips/{}/cuts", clip.id),
        serde_json::json!({ "start": 1.0, "end": 5.0 }),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let cut_job = v["job_id"].as_str().unwrap().to_string();
    let deadline = Instant::now() + Duration::from_secs(60);
    let cut = loop {
        let j = env.store().job(&cut_job).await.unwrap().unwrap();
        if j.state == "done" {
            break j;
        }
        assert!(j.state != "failed", "{j:?}");
        assert!(Instant::now() < deadline, "the cut waited: {j:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let background = job_for(&env, &web.id).await;
    assert_eq!(
        background.state, "running",
        "the cut ran beside the web copy, not after it: {cut:?} {background:?}"
    );
    assert!(
        env.ctx.lane.pauses() >= 1 || cfg!(not(unix)),
        "the web copy's ffmpeg was paused while the cut ran"
    );
    env.wait_no_jobs(Duration::from_secs(300)).await;
    assert_eq!(
        job_for(&env, &web.id).await.state,
        "done",
        "and went on after it"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn with_keep_last_the_link_comes_first_and_the_original_after() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| {
        c.share.embed_mb = 1;
    })
    .await
    else {
        return;
    };
    let (token, _) = pair(&env, "pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let replay = env
        .synth_at(&pc, "replay.mp4", Synth::h264_1080p(12.0, "order"))
        .await;
    let (s, v) = upload_whole(
        &env,
        &token,
        &replay,
        4_000_000,
        serde_json::json!({ "keep_last": 5 }),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let id = v["clip_id"].as_str().unwrap().to_string();
    let clip = env
        .wait_clip(
            |c| c.id == id && c.state == "ready",
            Duration::from_secs(120),
        )
        .await;
    env.wait_no_jobs(Duration::from_secs(180)).await;
    let renditions = env.store().renditions_of(&id).await.unwrap();
    let master = renditions.iter().find(|r| r.kind == "master").unwrap();
    let cut = renditions.iter().find(|r| r.kind == "cut").unwrap();
    assert_eq!(clip.current_id.as_deref(), Some(cut.id.as_str()));
    let jobs = sqlx::query_as::<_, reel_store::models::Job>(
        "SELECT * FROM job WHERE clip_id = ? ORDER BY created_at",
    )
    .bind(&id)
    .fetch_all(env.store().pool())
    .await
    .unwrap();
    let preview_of = |r: &reel_store::models::Rendition| {
        let copy = renditions
            .iter()
            .find(|c| c.kind == "embed" && c.derived_from.as_deref() == Some(r.id.as_str()))
            .unwrap_or_else(|| panic!("no preview of the {}", r.kind));
        jobs.iter()
            .find(|j| j.rendition_id.as_deref() == Some(copy.id.as_str()))
            .unwrap()
            .clone()
    };
    let (for_cut, for_master) = (preview_of(cut), preview_of(master));
    assert_eq!(
        for_cut.priority,
        reel_store::models::PRIORITY_EMBED,
        "the link's preview at a link's priority"
    );
    assert_eq!(
        for_master.priority,
        reel_store::models::PRIORITY_DERIVE,
        "the original's behind every link's work"
    );
    let ingest = jobs.iter().find(|j| j.kind == "ingest").unwrap();
    let derive = jobs.iter().find(|j| j.kind == "derive").unwrap();
    assert!(
        derive.payload.contains("\"whole\":true"),
        "the original's poster and loudness left for the derive job: {}",
        derive.payload
    );
    assert!(
        derive.finished_at >= ingest.finished_at,
        "after the ingest: {jobs:?}"
    );
    let master_file = env.ctx.config().abs(master.path.as_deref().unwrap());
    assert!(
        master_file.with_extension("jpg").exists(),
        "the derive job made the original's poster"
    );
    assert!(
        master.audio_measured_at.is_some(),
        "and measured the original's loudness"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn keep_last_makes_no_storage_copy_of_the_original() {
    let s3 = fake_s3().await;
    let (endpoint, public_url) = (s3.base.clone(), format!("{}/previews", s3.base));
    let Some(env) = start_with(AuthMode::Single, vec![], move |c| {
        c.external.on = true;
        c.external.endpoint = endpoint;
        c.external.bucket = "previews".into();
        c.external.access_key = "test".into();
        c.external.secret_key = "test-secret".into();
        c.external.public_url = public_url;
        c.external.embed_mb = 1;
    })
    .await
    else {
        return;
    };
    let (token, _) = pair(&env, "pc").await;
    let pc = env._dir.path().join("pc");
    std::fs::create_dir_all(&pc).unwrap();
    let file = env
        .synth_at(&pc, "replay.mp4", Synth::h264_1080p(12.0, "replay"))
        .await;
    let (s, v) = upload_whole(
        &env,
        &token,
        &file,
        4_000_000,
        serde_json::json!({ "keep_last": 6 }),
    )
    .await;
    assert_eq!(s, 200, "{v}");
    let clip_id = v["clip_id"].as_str().unwrap().to_string();
    let clip = env
        .wait_clip(
            |c| c.id == clip_id && c.state == "ready",
            Duration::from_secs(120),
        )
        .await;
    let (master, current) = (clip.master_id.unwrap(), clip.current_id.unwrap());
    assert_ne!(
        master, current,
        "the kept last seconds are the current version"
    );
    env.wait_no_jobs(Duration::from_secs(120)).await;
    assert!(
        env.store()
            .derived_rendition(&current, "embed_ext", None)
            .await
            .unwrap()
            .is_some(),
        "the current version has its copy at the storage's size"
    );
    assert!(
        env.store()
            .derived_rendition(&master, "embed_ext", None)
            .await
            .unwrap()
            .is_none(),
        "the original, current never, has none"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_posted_before_an_edit_offers_its_link_for_the_new_version() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("posted.mp4", Synth::small(5.0, "posted")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let (_, link) = post_json_body(
        &env,
        &format!("/api/clips/{}/shares", clip.id),
        serde_json::json!({}),
    )
    .await;
    let token = link["token"].as_str().unwrap().to_string();
    let public = reqwest::Client::new();
    let fetch = |ua: &'static str, path: String| {
        let c = public.clone();
        let url = format!("{}{path}", env.base);
        async move {
            let r = c.get(url).header("user-agent", ua).send().await.unwrap();
            (r.status().as_u16(), r.text().await.unwrap())
        }
    };
    const DISCORD: &str = "Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)";
    const BROWSER: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0";
    fetch(DISCORD, format!("/s/{token}")).await;
    let clip_page = || async {
        env.client
            .get(format!("{}/clips/{}", env.base, clip.id))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
    };
    assert!(
        !clip_page().await.contains("Copy link for this version"),
        "nothing to offer before an edit"
    );
    let d: serde_json::Value = env
        .client
        .post(format!("{}/api/clips/{}/cuts", env.base, clip.id))
        .json(&serde_json::json!({ "start": 1.0, "end": 4.0 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    env.wait_job_done(d["job_id"].as_str().unwrap(), Duration::from_secs(60))
        .await;
    let page = clip_page().await;
    let versioned = format!("{}/s/{token}?v=1", env.ctx.config().public_url);
    assert!(
        page.contains(&format!(
            r#"data-copy="{versioned}">Copy link for this version<"#
        )),
        "posted, then edited: the link for cut 1 is offered"
    );
    let (code, plain) = fetch(BROWSER, format!("/s/{token}")).await;
    assert_eq!(code, 200);
    let (code, page) = fetch(BROWSER, format!("/s/{token}?v=1")).await;
    assert_eq!(code, 200);
    let (tags, plain_tags) = (og(&page), og(&plain));
    assert_eq!(tags["og:url"], versioned, "its address is the one posted");
    assert_eq!(
        tags["og:video"], plain_tags["og:video"],
        "and its preview the link's current version"
    );
    let (_, other) = fetch(BROWSER, format!("/s/{token}?v=7")).await;
    assert_eq!(
        og(&other)["og:video"],
        plain_tags["og:video"],
        "a version the link no longer serves gives the current one"
    );
    let (_, odd) = fetch(BROWSER, format!("/s/{token}?v=x1")).await;
    assert_eq!(
        og(&odd)["og:url"],
        format!("{}/s/{token}", env.ctx.config().public_url),
        "a suffix that names no version is dropped"
    );
    let share = env.store().share_by_token(&token).await.unwrap().unwrap();
    assert_eq!(
        share.views, 4,
        "the same link: its views count every address"
    );
    let r = env
        .client
        .delete(format!(
            "{}/api/shares/{}",
            env.base,
            link["id"].as_str().unwrap()
        ))
        .header("sec-fetch-site", "same-origin")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        fetch(BROWSER, format!("/s/{token}?v=1")).await.0,
        404,
        "and a revoke ends it too"
    );
}

fn nas_of(cfg: &Config) -> PathBuf {
    cfg.data_dir.parent().unwrap().join("nas")
}

#[cfg(unix)]
fn inode(path: &std::path::Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).unwrap().ino()
}

async fn page_text(env: &Env, path: &str) -> (u16, String) {
    let r = env
        .client
        .get(format!("{}{path}", env.base))
        .send()
        .await
        .unwrap();
    (r.status().as_u16(), r.text().await.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_library_elsewhere_holds_the_clips_and_the_trash_is_a_rename() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| c.library_dir = nas_of(c)).await
    else {
        return;
    };
    let cfg = env.ctx.config();
    let nas = nas_of(&cfg);
    assert!(env.ctx.library.ready());
    assert!(
        std::fs::read_to_string(nas.join(".reel-library"))
            .unwrap()
            .contains("instance "),
        "a new library is marked"
    );
    assert!(!cfg.data_dir.join("library").exists());
    assert!(cfg.db_path().exists());
    env.synth(
        "Valorant 2026.10.29 - 20.15.00.01.mp4",
        Synth::small(2.0, "nas"),
    )
    .await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    let master = env
        .store()
        .rendition(clip.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        master.path.as_deref(),
        Some(format!("library/{}/master.mp4", clip.id).as_str()),
        "the database holds the same relative path as ever"
    );
    let video = cfg.abs(master.path.as_deref().unwrap());
    assert_eq!(video, nas.join(&clip.id).join("master.mp4"));
    assert!(video.exists());
    assert!(reel_pipeline::derived::paths(&video).poster.exists());
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let (s, _) = page_text(&env, &format!("/media/clips/{}/current.mp4", clip.id)).await;
    assert_eq!(s, 200);

    let tmp = env.incoming().join("..").join("up.mp4");
    let spec = Synth::small(2.0, "up");
    env.ctx.ff.run(&spec.args(&tmp), 2.0, None).await.unwrap();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(std::fs::read(&tmp).unwrap())
            .file_name("uploaded.mp4")
            .mime_str("video/mp4")
            .unwrap(),
    );
    let r = env
        .client
        .post(format!("{}/upload", env.base))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let up = env
        .wait_clip(
            |c| c.title == "uploaded" && c.state == "ready",
            Duration::from_secs(90),
        )
        .await;
    assert!(nas.join(&up.id).join("master.mp4").exists());

    #[cfg(unix)]
    let before = inode(&video);
    let r = env
        .client
        .delete(format!("{}/api/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let trashed = nas.join(".trash").join(&clip.id).join("master.mp4");
    assert!(trashed.exists() && !video.exists());
    #[cfg(unix)]
    assert_eq!(
        inode(&trashed),
        before,
        "renamed into the trash, not copied"
    );
    assert!(!cfg.data_dir.join("trash").exists());
    let (_, disk) = get_json(&env, "/api/disk?refresh=1").await;
    assert!(disk["trash"].as_u64().unwrap() > 0, "{disk}");
    let line = disk["detail"].as_str().unwrap();
    assert!(
        line.contains("data and library disk "),
        "one disk under the test's folders, named for both: {line}"
    );
    assert_eq!(disk["disks"].as_array().unwrap().len(), 1, "{disk}");
    let (s, _) = post_json(&env, &format!("/api/clips/{}/restore", clip.id)).await;
    assert_eq!(s, 200);
    assert!(video.exists() && !trashed.exists());
    #[cfg(unix)]
    assert_eq!(inode(&video), before, "and back by a rename");

    env.client
        .delete(format!("{}/api/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap();
    let old = jiff::Timestamp::now()
        .checked_sub(jiff::Span::new().hours(24 * 8))
        .unwrap()
        .to_string();
    sqlx::query("UPDATE clip SET deleted_at = ? WHERE id = ?")
        .bind(&old)
        .bind(&clip.id)
        .execute(env.store().pool())
        .await
        .unwrap();
    reel_pipeline::housekeeping::once(&env.ctx).await.unwrap();
    assert!(env.store().clip(&clip.id).await.unwrap().is_none());
    assert!(!nas.join(".trash").join(&clip.id).exists());
    assert!(nas.join(&up.id).exists(), "the other clip stays");

    let (s, settings) = page_text(&env, "/settings/storage").await;
    assert_eq!(s, 200);
    assert!(settings.contains("Library folder"));
    assert!(settings.contains(&nas.display().to_string()));
    assert!(
        settings.contains("on the data disk, "),
        "the library's line names its disk"
    );
    let (_, status) = page_text(&env, "/status").await;
    assert!(status.contains("in its own folder, marked as this reel"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_library_that_is_not_there_is_not_written_to() {
    let Some(env) = start_with(AuthMode::Single, vec![], |c| c.library_dir = nas_of(c)).await
    else {
        return;
    };
    let nas = nas_of(&env.ctx.config());
    env.synth("first.mp4", Synth::small(2.0, "one")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;

    let away = nas.with_file_name("nas-away");
    std::fs::rename(&nas, &away).unwrap();
    std::fs::create_dir(&nas).unwrap();
    let status = reel_pipeline::library::check(&env.ctx).await;
    assert!(
        matches!(&status, reel_pipeline::library::Status::Away(w) if w.contains("is empty")),
        "{status:?}"
    );

    let r = env
        .client
        .get(format!("{}/healthz", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 503);
    let health: serde_json::Value = r.json().await.unwrap();
    assert_eq!(health["ok"], false);
    assert!(health["library"].as_str().unwrap().contains("empty"));

    let r = env
        .client
        .delete(format!("{}/api/clips/{}", env.base, clip.id))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 503);
    let why: serde_json::Value = r.json().await.unwrap();
    assert!(
        why["error"].as_str().unwrap().contains("not usable"),
        "{why}"
    );
    assert!(env
        .store()
        .clip(&clip.id)
        .await
        .unwrap()
        .unwrap()
        .deleted_at
        .is_none());
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"not read".to_vec()).file_name("later.mp4"),
    );
    let r = env
        .client
        .post(format!("{}/upload", env.base))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 503);
    let (s, _) = post_json(&env, &format!("/api/clips/{}/restore", clip.id)).await;
    assert_eq!(s, 503);

    env.synth("second.mp4", Synth::small(2.0, "two")).await;
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(
        env.store()
            .ingest_file("second.mp4")
            .await
            .unwrap()
            .is_none(),
        "the incoming file waits"
    );
    reel_pipeline::housekeeping::once(&env.ctx).await.unwrap();
    assert_eq!(
        std::fs::read_dir(&nas).unwrap().count(),
        0,
        "nothing was written on the bare mount point"
    );
    let (_, home) = page_text(&env, "/").await;
    assert!(home.contains("library away") && home.contains("is empty"));
    let (_, status) = page_text(&env, "/status").await;
    assert!(status.contains("nothing is taken in or moved until it is back"));
    let (_, settings) = page_text(&env, "/settings/storage").await;
    assert!(settings.contains("not usable: "));

    std::fs::remove_dir(&nas).unwrap();
    std::fs::rename(&away, &nas).unwrap();
    let start = Instant::now();
    while !env.ctx.library.ready() {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "picked up by itself"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    let second = env
        .wait_clip(
            |c| c.title == "second" && c.state == "ready",
            Duration::from_secs(90),
        )
        .await;
    assert!(nas.join(&second.id).join("master.mp4").exists());
    let r = env
        .client
        .get(format!("{}/healthz", env.base))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_existing_library_moves_out_and_its_old_trash_follows() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    let cfg = env.ctx.config();
    assert_eq!(cfg.library_dir(), cfg.data_dir.join("library"));
    assert!(cfg.library_marker().exists());
    env.synth("keep.mp4", Synth::small(2.0, "keep")).await;
    let keep = env.wait_ready(Duration::from_secs(90)).await;
    env.synth("bin.mp4", Synth::small(2.0, "bin")).await;
    let bin = env
        .wait_clip(
            |c| c.title == "bin" && c.state == "ready",
            Duration::from_secs(90),
        )
        .await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let r = env
        .client
        .delete(format!("{}/api/clips/{}", env.base, bin.id))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(cfg.library_dir().join(".trash").join(&bin.id).exists());
    std::fs::remove_file(cfg.library_marker()).unwrap();
    let instance = env.store().setting("instance_id").await.unwrap().unwrap();
    let env = restart(env, |_| {}).await;
    assert!(
        env.ctx.library.ready(),
        "the library has the clips' folders: marked on its first start"
    );
    assert!(std::fs::read_to_string(cfg.library_marker())
        .unwrap()
        .contains(&instance));

    let (nas, library) = (nas_of(&cfg), cfg.library_dir());
    let env = {
        env.ctx.shutdown.cancel();
        tokio::time::sleep(Duration::from_millis(500)).await;
        std::fs::create_dir_all(cfg.data_dir.join("trash")).unwrap();
        std::fs::rename(
            library.join(".trash").join(&bin.id),
            cfg.data_dir.join("trash").join(&bin.id),
        )
        .unwrap();
        std::fs::rename(&library, &nas).unwrap();
        restart(env, |c| c.library_dir = nas_of(c)).await
    };
    assert!(env.ctx.library.ready(), "the marker moved with the library");
    let cfg = env.ctx.config();
    assert!(cfg.library_elsewhere());
    let master = env
        .store()
        .rendition(keep.master_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        cfg.abs(master.path.as_deref().unwrap()),
        nas.join(&keep.id).join("master.mp4")
    );
    let (s, _) = page_text(&env, &format!("/media/clips/{}/current.mp4", keep.id)).await;
    assert_eq!(
        s, 200,
        "a path from the database resolves in the moved library"
    );
    let start = Instant::now();
    while cfg.data_dir.join("trash").exists() {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the old trash moves over"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    assert!(nas.join(".trash").join(&bin.id).join("master.mp4").exists());
    let (s, _) = post_json(&env, &format!("/api/clips/{}/restore", bin.id)).await;
    assert_eq!(s, 200);
    assert!(nas.join(&bin.id).join("master.mp4").exists());
}

async fn as_user(env: &Env, method: reqwest::Method, path: &str, cookie: &str) -> (u16, String) {
    let r = env
        .client
        .request(method, format!("{}{path}", env.base))
        .header("cookie", cookie)
        .header("sec-fetch-site", "same-origin")
        .send()
        .await
        .unwrap();
    (r.status().as_u16(), r.text().await.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn the_trash_page_lists_a_clip_and_deletes_it_for_good() {
    let Some(env) = start(AuthMode::Local, vec![]).await else {
        return;
    };
    let code = setup_code(&env).await;
    let admin = cookie_of(&first_admin(&env, "robin", &code).await);
    let r = env
        .client
        .post(format!("{}/api/users", env.base))
        .header("cookie", &admin)
        .json(&serde_json::json!({ "name": "mira", "password": "mira has a password", "role": "member" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let r = env
        .client
        .post(format!("{}/signin", env.base))
        .form(&[("name", "mira"), ("password", "mira has a password")])
        .send()
        .await
        .unwrap();
    let mira = cookie_of(&r);
    env.synth("bin me.mp4", Synth::small(2.0, "bin")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;

    let (_, lib) = as_user(&env, reqwest::Method::GET, "/", &admin).await;
    assert!(lib.contains(r#"class="card-more""#) && lib.contains(r#"id="card-menu""#));
    assert!(lib.contains(&format!(r#"data-id="{}""#, clip.id)) && lib.contains(" data-edit"));
    let (_, lib) = as_user(&env, reqwest::Method::GET, "/", &mira).await;
    assert!(
        lib.contains(r#"class="card-more""#) && !lib.contains(" data-edit"),
        "a member's menu leaves out what only the owner or an admin may do"
    );
    let (_, d) = as_user(
        &env,
        reqwest::Method::GET,
        &format!("/api/clips/{}", clip.id),
        &admin,
    )
    .await;
    assert!(serde_json::from_str::<serde_json::Value>(&d).unwrap()["link"].is_null());
    let r = env
        .client
        .post(format!("{}/api/clips/{}/shares", env.base, clip.id))
        .header("cookie", &admin)
        .header("sec-fetch-site", "same-origin")
        .json(&serde_json::json!({ "label": "", "expires_in": "never", "allow_download": true, "rendition_id": "" }))
        .send()
        .await
        .unwrap();
    let made: serde_json::Value = r.json().await.unwrap();
    let (_, d) = as_user(
        &env,
        reqwest::Method::GET,
        &format!("/api/clips/{}", clip.id),
        &mira,
    )
    .await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&d).unwrap()["link"],
        made["url"],
        "the newest live link, the one the clip page copies"
    );

    let (s, _) = as_user(
        &env,
        reqwest::Method::DELETE,
        &format!("/api/trash/{}", clip.id),
        &admin,
    )
    .await;
    assert_eq!(s, 404, "only a clip in the trash is deleted from it");
    let (s, _) = as_user(
        &env,
        reqwest::Method::DELETE,
        &format!("/api/clips/{}", clip.id),
        &admin,
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(
        env.store()
            .clip(&clip.id)
            .await
            .unwrap()
            .unwrap()
            .deleted_by
            .as_deref(),
        Some("robin")
    );
    let (s, page) = as_user(&env, reqwest::Method::GET, "/trash", &mira).await;
    assert_eq!(s, 200);
    assert!(
        page.contains("bin me") && page.contains("by robin"),
        "{page}"
    );
    assert!(page.contains("7 days left") && page.contains("1 clip, "));
    assert!(
        !page.contains("Empty the trash"),
        "emptying it is the admins'"
    );
    let rid = clip.current_id.as_deref().unwrap();
    let (s, _) = as_user(
        &env,
        reqwest::Method::GET,
        &format!("/media/renditions/{rid}.jpg"),
        &mira,
    )
    .await;
    assert_eq!(s, 200, "the poster of a clip in the trash");
    let (s, _) = as_user(
        &env,
        reqwest::Method::DELETE,
        &format!("/api/trash/{}", clip.id),
        &mira,
    )
    .await;
    assert_eq!(
        s, 403,
        "a member deletes for good only the clips they uploaded"
    );
    let (_, page) = as_user(&env, reqwest::Method::GET, "/trash", &admin).await;
    assert!(page.contains("Empty the trash") && page.contains("trash-delete"));
    let (_, settings) = as_user(&env, reqwest::Method::GET, "/settings/storage", &admin).await;
    assert!(settings.contains(r#"href="/trash""#) && settings.contains("1 clip, "));

    let (s, _) = as_user(
        &env,
        reqwest::Method::DELETE,
        &format!("/api/trash/{}", clip.id),
        &admin,
    )
    .await;
    assert_eq!(s, 200);
    assert!(env.store().clip(&clip.id).await.unwrap().is_none());
    assert!(!env.ctx.config().trash_dir().join(&clip.id).exists());
    let (_, page) = as_user(&env, reqwest::Method::GET, "/trash", &admin).await;
    assert!(page.contains("The trash is empty."));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cap_signs_what_it_sends_to_the_trash() {
    let Some(env) = start(AuthMode::Single, vec![]).await else {
        return;
    };
    env.synth("old.mp4", Synth::small(2.0, "old")).await;
    let clip = env.wait_ready(Duration::from_secs(90)).await;
    env.wait_no_jobs(Duration::from_secs(120)).await;
    let n = reel_pipeline::housekeeping::enforce_cap(&env.ctx, 1, false)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let (_, page) = text_of(&env, "/trash").await;
    assert!(page.contains("by the library cap"), "{page}");
    assert!(page.contains(&clip.title));
}

async fn text_of(env: &Env, path: &str) -> (u16, String) {
    let r = env
        .client
        .get(format!("{}{path}", env.base))
        .send()
        .await
        .unwrap();
    (r.status().as_u16(), r.text().await.unwrap())
}
