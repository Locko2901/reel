pub mod anim;
pub mod app_update;
pub mod cap;
pub mod cut;
pub mod derive;
pub mod derived;
pub mod discord;
pub mod disks;
pub mod embed;
pub mod encode;
pub mod enqueue;
pub mod external;
pub mod hash;
pub mod housekeeping;
pub mod ingest;
pub mod jobs;
pub mod lane;
pub mod level;
pub mod library;
pub mod logging;
pub mod montage;
pub mod notify;
pub mod poster_image;
pub mod retitle;
pub mod s3;
pub mod updates;
pub mod watcher;
pub mod web;
pub mod worker;

use arc_swap::ArcSwap;
use reel_core::config::ConfigError;
use reel_core::settings::{self, Effective};
use reel_core::{Bus, Config, Event};
use reel_media::{Ffmpeg, HwStatus};
use reel_store::Store;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// The layers under the database: the base config, config.toml, and whether the environment counts.
pub struct Layers {
    pub base: Config,
    pub file: Option<toml::Value>,
    pub env: bool,
}

impl Layers {
    pub fn of(base: Config) -> Self {
        Self {
            base,
            file: None,
            env: false,
        }
    }
}

#[derive(Clone)]
pub struct Ctx {
    settings: Arc<ArcSwap<Effective>>,
    layers: Arc<Layers>,
    pub store: Store,
    pub ff: Arc<Ffmpeg>,
    pub hw: Arc<RwLock<HwStatus>>,
    pub bus: Bus,
    pub shutdown: CancellationToken,
    pub cancels: Arc<tokio::sync::Mutex<std::collections::HashMap<String, CancellationToken>>>,
    pub updates: Arc<RwLock<updates::UpdateState>>,
    pub started_at: jiff::Timestamp,
    /// The disks under the Disk warning now, by device: a note when one falls under it.
    pub disk_low: Arc<std::sync::Mutex<std::collections::HashSet<u64>>>,
    /// Set by the desktop app while a game has the screen: the heavy jobs (re-encodes, the
    /// loop and the strip, the copies) wait for it to clear; the light ones go on.
    pub hold_heavy: Arc<std::sync::atomic::AtomicBool>,
    pub external: Arc<external::State>,
    pub lane: Arc<lane::Lane>,
    /// Whether the Library folder is there to be written to.
    pub library: Arc<library::State>,
}

impl Ctx {
    pub async fn new(
        layers: Layers,
        store: Store,
        ff: Arc<Ffmpeg>,
        hw: HwStatus,
        bus: Bus,
    ) -> Result<Self, ConfigError> {
        let eff = effective_of(&layers, &store).await?;
        reel_core::format::set_timezone(&eff.config.timezone);
        let ctx = Self {
            settings: Arc::new(ArcSwap::from_pointee(eff)),
            layers: Arc::new(layers),
            store,
            ff,
            hw: Arc::new(RwLock::new(hw)),
            bus,
            shutdown: CancellationToken::new(),
            cancels: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            updates: Arc::new(RwLock::new(updates::UpdateState::default())),
            started_at: jiff::Timestamp::now(),
            disk_low: Arc::default(),
            hold_heavy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            external: Arc::new(external::State::default()),
            lane: Arc::new(lane::Lane::default()),
            library: Arc::new(library::State::default()),
        };
        library::check(&ctx).await;
        Ok(ctx)
    }

    /// Heavy work waits while the pipeline is held; a cancelled job stops waiting.
    pub async fn wait_while_held(&self, job: &reel_store::models::Job) -> anyhow::Result<()> {
        use std::sync::atomic::Ordering;
        let mut told = false;
        while self.hold_heavy.load(Ordering::SeqCst) {
            if self.is_cancelled(&job.id).await {
                anyhow::bail!("cancelled while waiting");
            }
            if !told {
                told = true;
                tracing::info!(job = %job.id, kind = %job.kind, "the job waits while a game has the screen");
                let _ = self
                    .store
                    .job_route(&job.id, "waiting while a game has the screen")
                    .await;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        if told {
            let _ = self.store.job_route(&job.id, "").await;
        }
        Ok(())
    }

    /// The config in force right now; a setting saved a moment ago is already in it.
    pub fn config(&self) -> Arc<Config> {
        self.settings.load().config.clone()
    }

    pub fn effective(&self) -> Arc<Effective> {
        self.settings.load_full()
    }

    pub fn layers(&self) -> &Layers {
        &self.layers
    }

    /// Rebuilds the config from the table under the file and the environment.
    pub async fn reload(&self) -> Result<Arc<Effective>, ConfigError> {
        let eff = Arc::new(effective_of(&self.layers, &self.store).await?);
        reel_core::format::set_timezone(&eff.config.timezone);
        self.settings.store(eff.clone());
        Ok(eff)
    }

    pub fn emit(&self, e: Event) {
        let _ = self.bus.send(e);
    }

    /// The token a running job's ffmpeg runs watch; cancelling it ends the job at its next run.
    pub async fn cancel_token(&self, job_id: &str) -> Option<CancellationToken> {
        self.cancels.lock().await.get(job_id).cloned()
    }

    pub async fn is_cancelled(&self, job_id: &str) -> bool {
        self.cancels
            .lock()
            .await
            .get(job_id)
            .map(|t| t.is_cancelled())
            .unwrap_or(false)
    }

    pub async fn backend(&self) -> reel_media::HwBackend {
        self.hw.read().await.backend
    }

    pub async fn encoder_for(
        &self,
        vcodec: Option<&str>,
        source: (u32, u32),
        fps: f64,
    ) -> reel_media::args::Encoder {
        let hw = self.hw.read().await;
        let cfg = self.config();
        reel_media::args::Encoder {
            hw: hw.backend,
            device: cfg.media.render_device.clone(),
            mbit: cfg.media.encode_mbit,
            fps,
            hw_decoder: hw.decoder_for(vcodec),
            source,
        }
    }
}

async fn effective_of(layers: &Layers, store: &Store) -> Result<Effective, ConfigError> {
    let table = store
        .settings()
        .await
        .map_err(|e| ConfigError::Invalid(format!("reading the settings: {e}")))?;
    let env: &dyn Fn(&str) -> Option<String> = if layers.env {
        &settings::process_env
    } else {
        &settings::no_env
    };
    settings::build(&layers.base, layers.file.as_ref(), &table, env)
}

pub fn spawn_all(ctx: Ctx) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::new();
    let workers = ctx.config().pipeline.workers.max(1);
    for i in 0..workers {
        let c = ctx.clone();
        handles.push(tokio::spawn(async move {
            worker::run_worker(c, i, false).await
        }));
    }
    let c = ctx.clone();
    handles.push(tokio::spawn(async move {
        worker::run_worker(c, workers, true).await
    }));
    let c = ctx.clone();
    handles.push(tokio::spawn(async move { library::run(c).await }));
    let c = ctx.clone();
    handles.push(tokio::spawn(async move { housekeeping::run(c).await }));
    let c = ctx.clone();
    handles.push(tokio::spawn(async move { updates::run(c).await }));
    // A server's alone: the app hands its clips over in process.
    if !ctx.config().standalone() {
        let c = ctx.clone();
        handles.push(tokio::spawn(async move { watcher::run_watcher(c).await }));
        let c = ctx.clone();
        handles.push(tokio::spawn(async move { app_update::run(c).await }));
        let c = ctx.clone();
        handles.push(tokio::spawn(async move { notify::run(c).await }));
        let c = ctx.clone();
        handles.push(tokio::spawn(async move { external::run(c).await }));
    }
    let c = ctx.clone();
    handles.push(tokio::spawn(async move {
        if let Err(e) = retitle::sweep(&c).await {
            tracing::warn!(error = %e, "the title sweep did not finish");
        }
    }));
    handles
}
