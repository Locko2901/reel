//! Standalone mode: reel's own server in this process on the loopback, with the library under
//! the app's data folder and the ffmpeg next to the app.
use reel_core::config::{Config, Hwaccel};
use reel_pipeline::{Ctx, Layers};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What the app tells the local server; the rest of the config is reel's default.
#[derive(Debug, Clone)]
pub struct LocalSettings {
    pub data_dir: PathBuf,
    pub hwaccel: String,
    pub cap_gb: u64,
    pub cap_keeps_unsent: bool,
    pub copies_on_hardware: bool,
}

/// The running local server and what the hardware probe found.
pub struct Local {
    pub ctx: Ctx,
    /// The server's state, for what the app asks reel in process.
    pub state: reel_server::AppState,
    pub addr: SocketAddr,
    pub url: String,
    pub backend: String,
    pub ffmpeg_version: Option<String>,
    pub notes: Vec<String>,
    handles: Vec<tokio::task::JoinHandle<()>>,
    server: tokio::task::JoinHandle<()>,
}

/// A sidecar next to the executable, where Tauri puts them, else the tool on the PATH.
pub fn sidecar(name: &str) -> PathBuf {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(&file);
            if p.is_file() {
                return p;
            }
        }
    }
    PathBuf::from(name)
}

fn free_port() -> anyhow::Result<u16> {
    let l = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(l.local_addr()?.port())
}

fn hwaccel_of(s: &str) -> Hwaccel {
    match s {
        "nvenc" => Hwaccel::Nvenc,
        "amf" => Hwaccel::Amf,
        "qsv" => Hwaccel::Qsv,
        "vaapi" => Hwaccel::Vaapi,
        "none" => Hwaccel::None,
        _ => Hwaccel::Auto,
    }
}

#[allow(clippy::field_reassign_with_default)]
pub fn config_for(s: &LocalSettings) -> Config {
    let mut cfg = Config::default();
    cfg.make_standalone();
    cfg.site_name = "reel desktop".into();
    cfg.data_dir = s.data_dir.clone();
    cfg.ingest.incoming_dir = s.data_dir.join("incoming");
    cfg.ingest.originals_dir = s.data_dir.join("originals");
    cfg.media.ffmpeg = sidecar("ffmpeg").display().to_string();
    cfg.media.ffprobe = sidecar("ffprobe").display().to_string();
    cfg.media.hwaccel = hwaccel_of(&s.hwaccel);
    if cfg!(windows) {
        cfg.media.render_device = String::new();
    }
    cfg.media.copies_on_hardware = s.copies_on_hardware;
    cfg.storage.cap_gb = s.cap_gb;
    cfg.storage.cap_keeps_unsent = s.cap_keeps_unsent;
    cfg
}

impl Local {
    pub async fn start(s: &LocalSettings) -> anyhow::Result<Local> {
        let port = free_port()?;
        let mut cfg = config_for(s);
        cfg.listen = format!("127.0.0.1:{port}");
        cfg.public_url = format!("http://127.0.0.1:{port}");
        cfg.ensure_dirs()?;
        let store = reel_store::Store::open(&cfg.db_path()).await?;
        store
            .set_setting("setup_done", "by the desktop app")
            .await?;
        let n = reel_pipeline::library::relative_originals(&store, &cfg).await?;
        if n > 0 {
            tracing::info!(
                n,
                "standalone: kept originals now by a path relative to the library"
            );
        }
        let ff = Arc::new(reel_media::Ffmpeg::new(
            &cfg.media.ffmpeg,
            &cfg.media.ffprobe,
            cfg.pipeline.nice,
            cfg.pipeline.job_timeout_minutes,
        ));
        let hw = reel_media::hw::detect(&ff, &cfg.media).await;
        let backend = hw.backend.name().to_string();
        let ffmpeg_version = hw.ffmpeg_version.clone();
        let notes = hw.notes.clone();
        tracing::info!(backend, ffmpeg = ?ffmpeg_version, notes = ?notes, "standalone: hardware probed");
        let listen = cfg.listen.clone();
        let url = cfg.public_url.clone();
        let ctx = Ctx::new(Layers::of(cfg), store, ff, hw, reel_core::events::new_bus()).await?;
        let handles = reel_pipeline::spawn_all(ctx.clone());
        let state = reel_server::AppState::new(ctx.clone()).await?;
        let (addr, server) =
            reel_server::bind(state.clone(), &listen, ctx.shutdown.clone()).await?;
        tracing::info!(%addr, "standalone: reel serves on the loopback");
        Ok(Local {
            ctx,
            state,
            addr,
            url,
            backend,
            ffmpeg_version,
            notes,
            handles,
            server,
        })
    }

    /// Settings, Advanced, Check for updates of this library, as the app's setup leaves it. Off
    /// is standalone's own default, so off takes the library's row away.
    pub async fn set_update_check(&self, on: bool) -> anyhow::Result<()> {
        const KEY: &str = "advanced.update_check";
        let was = self.ctx.config().advanced.update_check;
        if on {
            self.ctx.store.set_setting(KEY, "true").await?;
        } else {
            self.ctx.store.delete_setting(KEY).await?;
        }
        self.ctx
            .reload()
            .await
            .map_err(|e| anyhow::anyhow!("the settings were not read again: {e}"))?;
        if was != on {
            let (old, new) = (was.to_string(), on.to_string());
            self.ctx
                .store
                .add_setting_history(KEY, Some(&old), Some(&new), "the app's setup")
                .await?;
        }
        tracing::info!(on, was, "standalone: the setup set Check for updates");
        Ok(())
    }

    /// This run's key, for the app's own requests that are not its window's.
    pub fn key(&self) -> &str {
        self.state
            .app_key
            .as_ref()
            .map(|k| k.secret())
            .unwrap_or_default()
    }

    /// Where the window opens a page of the library: the way in with a ticket of this run, which
    /// the library trades for the key as the window's cookie before it goes on to the page.
    pub fn page(&self, path: &str) -> String {
        let ticket = self
            .state
            .app_key
            .as_ref()
            .map(|k| k.ticket())
            .unwrap_or_default();
        match url::Url::parse(&format!("{}/app/open", self.url)) {
            Ok(mut u) => {
                u.query_pairs_mut()
                    .append_pair("ticket", &ticket)
                    .append_pair("to", path);
                u.to_string()
            }
            Err(_) => format!("{}{path}", self.url),
        }
    }

    /// Hands a clip OBS saved to the local pipeline; the file is taken into the library.
    pub async fn ingest(
        &self,
        path: &Path,
        title: Option<String>,
        recorded_at: Option<String>,
        game: Option<String>,
        keep_last: Option<f64>,
        audio_names: Option<Vec<String>>,
    ) -> anyhow::Result<String> {
        let mut new = reel_pipeline::enqueue::NewClip::new(path, "desktop");
        new.audio_names = audio_names;
        new.title = title;
        new.recorded_at = recorded_at;
        new.recorded_source = Some("desktop");
        new.game = game;
        new.keep_last = keep_last;
        let (clip, _job) = reel_pipeline::enqueue::create_clip_for_file(&self.ctx, new).await?;
        Ok(clip.id)
    }

    /// A clip's master file and title, for an upload to a server.
    pub async fn master_of(&self, clip_id: &str) -> anyhow::Result<Option<(PathBuf, String)>> {
        let Some(clip) = self.ctx.store.clip(clip_id).await? else {
            return Ok(None);
        };
        let Some(mid) = clip.master_id.as_deref() else {
            return Ok(None);
        };
        let Some(master) = self.ctx.store.rendition(mid).await? else {
            return Ok(None);
        };
        let Some(rel) = master.path.as_deref() else {
            return Ok(None);
        };
        Ok(Some((self.ctx.config().abs(rel), clip.title)))
    }

    pub async fn mark_uploaded(&self, clip_id: &str, server: &str) -> anyhow::Result<()> {
        self.ctx.store.mark_uploaded(clip_id, server).await?;
        Ok(())
    }

    /// Heavy jobs wait while this is set (a game has the screen).
    pub fn hold(&self, held: bool) {
        self.ctx
            .hold_heavy
            .store(held, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.ctx.shutdown.cancel();
        self.server.abort();
        for h in &self.handles {
            h.abort();
        }
        tracing::info!("standalone: reel stopped");
    }

    /// Stops it and closes its database, so its folder can move.
    pub async fn close(&self) {
        self.stop();
        self.ctx.store.close().await;
        // an ffmpeg ended with its job lets go of its files a moment later on Windows
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        tracing::info!("standalone: reel's database is closed");
    }
}

/// The pinned ffmpeg build, for the Status page.
pub fn ffmpeg_pin() -> serde_json::Value {
    serde_json::from_str(include_str!("../../ffmpeg/pinned.json")).unwrap_or_default()
}
