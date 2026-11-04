use reel_core::Config;
use serde::Serialize;

/// The few values the page scripts need: the link dialog's defaults and the Discord picker's start.
#[derive(Debug, Clone, Serialize)]
pub struct Client {
    pub discord_limit_mb: u64,
    pub share_download_default: bool,
    pub share_expiry_default: String,
    /// Link previews go to the external storage, so a new link waits for its upload.
    pub external: bool,
}

pub fn client(cfg: &Config) -> Client {
    Client {
        discord_limit_mb: cfg.share.discord_limit_mb,
        share_download_default: cfg.share.download_default,
        share_expiry_default: cfg.share.expiry_default.clone(),
        external: cfg.external_on(),
    }
}

/// The JSON every page puts on its body: the client values plus the job counts for the top bar.
pub async fn client_json(st: &crate::AppState) -> anyhow::Result<String> {
    let cfg = st.ctx.config();
    let (queued, running, failed) = st.ctx.store.job_counts_full().await?;
    let mut v = serde_json::to_value(client(&cfg))?;
    v["jobs"] = serde_json::json!({ "queued": queued, "running": running, "failed": failed });
    Ok(serde_json::to_string(&v)?)
}
