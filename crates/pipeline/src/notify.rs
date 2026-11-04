//! Notes to an ntfy topic or a webhook, for the events switched on under Notifications.
use crate::Ctx;
use anyhow::{anyhow, bail};
use reel_core::format::now;
use reel_core::{Config, Event};
use std::time::Duration;

pub const EVENTS: [&str; 7] = [
    "job_failed",
    "file_unreadable",
    "disk_low",
    "storage",
    "library_cap",
    "update_available",
    "link_made",
];

pub struct Note {
    pub event: &'static str,
    pub title: String,
    pub body: String,
    pub link: String,
}

impl Note {
    pub fn new(
        event: &'static str,
        title: impl Into<String>,
        body: impl Into<String>,
        link: impl Into<String>,
    ) -> Self {
        Note {
            event,
            title: title.into(),
            body: body.into(),
            link: link.into(),
        }
    }
}

fn tag(event: &str) -> &'static str {
    match event {
        "job_failed" | "file_unreadable" | "disk_low" | "storage" => "warning",
        "update_available" => "arrow_up",
        "link_made" => "link",
        "library_cap" => "wastebasket",
        _ => "white_check_mark",
    }
}

pub fn free_and_total(path: &std::path::Path) -> (u64, u64) {
    #[cfg(unix)]
    {
        match nix::sys::statvfs::statvfs(path) {
            Ok(v) => {
                let frag = v.fragment_size();
                (v.blocks_available() * frag, v.blocks() * frag)
            }
            Err(_) => (0, 0),
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let (mut free, mut total, mut total_free) = (0u64, 0u64, 0u64);
        let ok =
            unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, &mut total_free) };
        if ok != 0 {
            (free, total)
        } else {
            (0, 0)
        }
    }
}

/// Sends a note when its event is ticked and a target is set; a test always goes. The words
/// say where it went.
pub async fn send(ctx: &Ctx, note: Note) -> anyhow::Result<String> {
    let cfg = ctx.config();
    if note.event != "test" && !cfg.notifies(note.event) {
        return Ok("not sent, the event is not ticked".into());
    }
    deliver(&cfg, &note).await
}

pub async fn deliver(cfg: &Config, note: &Note) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(format!("reel/{}", reel_core::VERSION))
        .build()?;
    match cfg.notify.kind.as_str() {
        "ntfy" => {
            if cfg.notify.ntfy_url.is_empty() || cfg.notify.ntfy_topic.is_empty() {
                bail!("ntfy needs a server and a topic");
            }
            let mut req = client.post(&cfg.notify.ntfy_url).json(&serde_json::json!({
                "topic": cfg.notify.ntfy_topic,
                "title": note.title,
                "message": note.body,
                "click": note.link,
                "tags": [tag(note.event)],
            }));
            if !cfg.notify.ntfy_token.is_empty() {
                req = req.bearer_auth(&cfg.notify.ntfy_token);
            }
            let r = req
                .send()
                .await
                .map_err(|e| anyhow!("ntfy at {}: {e}", cfg.notify.ntfy_url))?;
            if !r.status().is_success() {
                bail!("ntfy answered {}", r.status());
            }
            Ok(format!("sent to the ntfy topic {}", cfg.notify.ntfy_topic))
        }
        "webhook" => {
            if cfg.notify.webhook_url.is_empty() {
                bail!("the webhook needs a URL");
            }
            let r = client
                .post(&cfg.notify.webhook_url)
                .json(&serde_json::json!({
                    "event": note.event,
                    "title": note.title,
                    "body": note.body,
                    "link": note.link,
                    "site": cfg.site_name,
                    "at": now(),
                }))
                .send()
                .await
                .map_err(|e| anyhow!("the webhook: {e}"))?;
            if !r.status().is_success() {
                bail!("the webhook answered {}", r.status());
            }
            Ok(format!("the webhook answered {}", r.status().as_u16()))
        }
        _ => bail!("notifications are off"),
    }
}

/// Sends without making the caller wait; what goes wrong lands in the log.
pub fn send_later(ctx: &Ctx, note: Note) {
    let c = ctx.clone();
    tokio::spawn(async move {
        let event = note.event;
        match send(&c, note).await {
            Ok(went) => tracing::debug!(event, went, "notification"),
            Err(e) => tracing::warn!(event, error = %e, "notification not sent"),
        }
    });
}

/// Housekeeping's disk check, for each disk that holds reel's data: one note when its free
/// space falls under the warning, none until it has been above it again.
pub async fn disk_check(ctx: &Ctx) {
    let cfg = ctx.config();
    let c = cfg.clone();
    let disks = tokio::task::spawn_blocking(move || crate::disks::disks(&c))
        .await
        .unwrap_or_default();
    let low: Vec<_> = disks
        .iter()
        .filter(|d| d.low(cfg.storage.disk_warn_gb))
        .collect();
    let newly: Vec<String> = {
        let mut held = ctx.disk_low.lock().unwrap();
        let newly = low
            .iter()
            .filter(|d| !held.contains(&d.id))
            .map(|d| d.line(cfg.library_elsewhere()))
            .collect();
        *held = low.iter().map(|d| d.id).collect();
        newly
    };
    if newly.is_empty() {
        return;
    }
    send_later(
        ctx,
        Note::new(
            "disk_low",
            format!(
                "{}: a disk is under {} GB",
                cfg.site_name, cfg.storage.disk_warn_gb
            ),
            newly.join("; "),
            format!("{}/status", cfg.public_url),
        ),
    );
}

pub async fn run(ctx: Ctx) {
    let mut rx = ctx.bus.subscribe();
    loop {
        let event = tokio::select! {
            r = rx.recv() => match r {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
            _ = ctx.shutdown.cancelled() => break,
        };
        let cfg = ctx.config();
        match event {
            Event::Job {
                kind,
                clip_id,
                state,
                error,
                ..
            } if state == "failed" => {
                let reason = cfg.hide_paths(&error.unwrap_or_default());
                let clip = match &clip_id {
                    Some(id) => ctx.store.clip(id).await.ok().flatten(),
                    None => None,
                };
                let name = clip
                    .as_ref()
                    .map(|c| c.title.clone())
                    .unwrap_or_else(|| "a clip".into());
                let link = format!("{}/jobs?show=failed", cfg.public_url);
                let note = if kind == "ingest" && reason.contains("could not read") {
                    let file = clip.as_ref().map(|c| c.source_name.clone()).unwrap_or(name);
                    Note::new(
                        "file_unreadable",
                        format!("{}: a file could not be read", cfg.site_name),
                        format!("{file}: {reason}"),
                        link,
                    )
                } else {
                    Note::new(
                        "job_failed",
                        format!("{}: a job failed", cfg.site_name),
                        format!("{kind} on {name}: {reason}"),
                        link,
                    )
                };
                send_later(&ctx, note);
            }
            Event::Update { version, url } => send_later(
                &ctx,
                Note::new(
                    "update_available",
                    format!("{}: reel {version} is out", cfg.site_name),
                    format!("you run {}", reel_core::VERSION),
                    url,
                ),
            ),
            _ => {}
        }
    }
}
