use anyhow::Context;
use clap::{Parser, Subcommand};
use reel::seed;
use reel_core::events::new_bus;
use reel_core::settings;
use reel_media::{hw, Ffmpeg};
use reel_pipeline::{Ctx, Layers};
use reel_store::Store;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "reel",
    version,
    about = "a clip vault on your own server: share links that play in link previews, and the files themselves"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the server, the incoming watcher and the workers
    Serve,
    /// Check ffmpeg, the hardware path and the directories, then exit
    Doctor,
    /// Drop synthetic test clips into the incoming folder (demo only)
    Seed {
        #[arg(long, default_value_t = 6)]
        count: usize,
    },
    /// Set a user's password from REEL_PASSWORD (built-in accounts): the way back in when the
    /// admin is locked out, and what the demo uses. Every session of that user ends.
    Passwd {
        name: String,
        /// Make the user as an admin when there is none of that name
        #[arg(long)]
        make_admin: bool,
    },
    /// The users, and the way across when the sign-in method changes
    Users {
        #[command(subcommand)]
        cmd: UsersCmd,
    },
}

#[derive(Subcommand)]
enum UsersCmd {
    /// Every user with their role, their state and how they sign in
    List,
    /// Print a one-time link: whoever opens it and signs in at the identity provider is that user
    Link { name: String },
    /// Take a user's identity at the provider away; every session of theirs ends
    Unlink { name: String },
    /// Rename a user, such as to the name a sign-in proxy sends for them
    Rename { name: String, new_name: String },
}

const QUIET_CRATES: &str = "sqlx=warn,tower_http=info,hyper=warn";

fn init_logs() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let fixed = std::env::var("RUST_LOG").is_ok();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("info,{QUIET_CRATES}").into());
    let (filter, handle) = tracing_subscriber::reload::Layer::new(filter);
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .with(reel_pipeline::logging::RingLayer)
        .init();
    if !fixed {
        reel_pipeline::logging::install_level_setter(move |level| {
            let f = tracing_subscriber::EnvFilter::try_new(format!("{level},{QUIET_CRATES}"))
                .map_err(|e| e.to_string())?;
            handle.reload(f).map_err(|e| e.to_string())
        });
    }
}

fn layers() -> anyhow::Result<Layers> {
    let l = settings::load_layers().context("config")?;
    #[allow(unused_mut)]
    let mut base = l.base;
    #[cfg(feature = "style-page")]
    if std::env::var_os("REEL_DEMO_STANDALONE").is_some_and(|v| v == "1") {
        base.make_standalone();
    }
    Ok(Layers {
        base,
        file: l.file,
        env: true,
    })
}

async fn make_ctx(layers: Layers, detect: bool) -> anyhow::Result<Ctx> {
    let boot = settings::build(
        &layers.base,
        layers.file.as_ref(),
        &[],
        &settings::process_env,
    )
    .context("config")?
    .config;
    boot.ensure_dirs().context("data directories")?;
    let store = Store::open(&boot.db_path())
        .await
        .context("open database")?;
    let table = store.settings().await.context("read settings")?;
    let config = settings::build(
        &layers.base,
        layers.file.as_ref(),
        &table,
        &settings::process_env,
    )
    .context("config")?
    .config;
    let ff = Arc::new(Ffmpeg::new(
        &config.media.ffmpeg,
        &config.media.ffprobe,
        config.pipeline.nice,
        config.pipeline.job_timeout_minutes,
    ));
    let status = if detect {
        hw::detect(&ff, &config.media).await
    } else {
        reel_media::HwStatus::software(&config.media.render_device, ff.version().await, vec![])
    };
    Ok(Ctx::new(layers, store, ff, status, new_bus()).await?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logs();
    let cli = Cli::parse();
    let layers = layers()?;
    match cli.cmd {
        Cmd::Serve => serve(layers).await,
        Cmd::Doctor => doctor(layers).await,
        Cmd::Seed { count } => {
            let ctx = make_ctx(layers, false).await?;
            seed::run(&ctx, count).await
        }
        Cmd::Passwd { name, make_admin } => {
            let ctx = make_ctx(layers, false).await?;
            passwd(&ctx, &name, make_admin).await
        }
        Cmd::Users { cmd } => {
            let ctx = make_ctx(layers, false).await?;
            users(&ctx, cmd).await
        }
    }
}

async fn serve(layers: Layers) -> anyhow::Result<()> {
    let ctx = make_ctx(layers, true).await?;
    {
        let hw = ctx.hw.read().await;
        tracing::info!(backend = hw.backend.name(), ffmpeg = ?hw.ffmpeg_version, notes = ?hw.notes, "media");
    }
    let cfg = ctx.config();
    if let Some(kind) = reel_pipeline::disks::network_share(&cfg.data_dir) {
        tracing::warn!(dir = %cfg.data_dir.display(), "{}", reel_pipeline::disks::share_warning(kind));
    }
    tracing::info!(data = %cfg.data_dir.display(), library = %cfg.library_dir().display(), ready = ctx.library.ready(), "folders");
    let requeued = ctx.store.requeue_stuck_jobs().await?;
    if requeued > 0 {
        tracing::info!(requeued, "jobs requeued after restart");
    }
    let handles = reel_pipeline::spawn_all(ctx.clone());
    let state = reel_server::AppState::new(ctx.clone()).await?;
    tokio::spawn(reel_server::expire_sessions(state.sessions.clone()));
    if let Some(code) = ctx.store.setting(reel_server::auth::SETUP_CODE_KEY).await? {
        if reel_server::accounts::wants_setup_code(&ctx.store, &cfg).await? {
            if cfg.auth.mode == reel_core::config::AuthMode::Oidc {
                println!("no admin yet: sign in through the identity provider, then type the setup code {code}");
            } else {
                println!(
                    "no user yet: the first admin signs up at /signin with the setup code {code}"
                );
            }
        }
    }
    if let Some(line) = reel_server::accounts::admin_way_in(&ctx.store, &cfg).await? {
        tracing::warn!("{line}");
    }
    let shutdown = ctx.shutdown.clone();
    let listen = ctx.config().listen.clone();
    let server = tokio::spawn(async move { reel_server::serve(state, &listen, shutdown).await });
    if let Err(e) = reel_pipeline::logging::set_level(&ctx.config().advanced.log_level) {
        tracing::debug!(error = %e, "log level from the settings not applied");
    }
    let shutdown = ctx.shutdown.clone();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("shutting down"),
        _ = shutdown.cancelled() => tracing::info!("stopping as asked"),
    }
    shutdown.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        for h in handles {
            let _ = h.await;
        }
        let _ = server.await;
    })
    .await;
    Ok(())
}

async fn doctor(layers: Layers) -> anyhow::Result<()> {
    let ctx = make_ctx(layers, true).await?;
    let config = ctx.config();
    let hw = ctx.hw.read().await;
    println!("reel {}", reel_core::VERSION);
    println!(
        "ffmpeg:   {}",
        hw.ffmpeg_version
            .clone()
            .unwrap_or_else(|| format!("NOT FOUND ({})", config.media.ffmpeg))
    );
    println!("ffprobe:  {}", config.media.ffprobe);
    println!(
        "hwaccel:  {} (config {:?}, device {})",
        hw.backend.name(),
        config.media.hwaccel,
        hw.device
    );
    for n in &hw.notes {
        println!("          {n}");
    }
    println!("decode:   {}", hw.decoders_line());
    println!("animated: {}", hw.anim_line());
    println!("data:     {}", config.data_dir.display());
    println!(
        "library:  {} ({})",
        config.library_dir().display(),
        match ctx.library.away() {
            Some(why) => format!("NOT USABLE: {why}"),
            None => "marked as this reel's".into(),
        }
    );
    if let Some(kind) = reel_pipeline::disks::network_share(&config.data_dir) {
        println!("          {}", reel_pipeline::disks::share_warning(kind));
    }
    println!(
        "incoming: {} ({})",
        config.ingest.incoming_dir.display(),
        if config.ingest.incoming_dir.is_dir() {
            "readable"
        } else {
            "MISSING"
        }
    );
    println!(
        "originals:{} ({})",
        config.ingest.originals_dir.display(),
        if config.ingest.originals_dir.is_dir() {
            "readable"
        } else {
            "missing, created on first use"
        }
    );
    let (n, bytes) = ctx.store.clip_count_and_size().await?;
    let (q, r) = ctx.store.job_counts().await?;
    println!("clips:    {n}, {}", reel_core::format::size(bytes as u64));
    println!("jobs:     {q} queued, {r} running");
    println!(
        "auth:     {:?}, public url {}",
        config.auth.mode, config.public_url
    );
    let waits = reel_server::accounts::wants_setup_code(&ctx.store, &config).await?;
    let code = ctx.store.setting(reel_server::auth::SETUP_CODE_KEY).await?;
    match (config.auth.mode, code) {
        (reel_core::config::AuthMode::Local, Some(code)) if waits => println!(
            "setup:    no user yet; the first admin needs the setup code {code} at /signin"
        ),
        (reel_core::config::AuthMode::Oidc, Some(code)) if waits => println!(
            "setup:    no admin yet; sign in through the identity provider, then type the setup code {code}"
        ),
        (reel_core::config::AuthMode::Local | reel_core::config::AuthMode::Oidc, _) => {
            println!("users:    {}", ctx.store.user_count().await?)
        }
        _ => {}
    }
    if let Some(line) = reel_server::accounts::admin_way_in(&ctx.store, &config).await? {
        println!("sign-in:  {line}");
    }
    Ok(())
}

async fn passwd(ctx: &Ctx, name: &str, make_admin: bool) -> anyhow::Result<()> {
    let password = std::env::var("REEL_PASSWORD").unwrap_or_default();
    reel_core::auth::password_ok(&password).map_err(|e| anyhow::anyhow!("REEL_PASSWORD: {e}"))?;
    let hash = reel_server::accounts::hash_password(&password);
    let user = match ctx.store.user_by_name(name).await? {
        Some(u) => u,
        None if make_admin => {
            let u = reel_store::models::User::new(name, "admin", None);
            ctx.store.insert_user(&u).await?;
            println!("made {name} as an admin");
            u
        }
        None => anyhow::bail!("no user named {name}; add --make-admin to make one"),
    };
    ctx.store.set_password(&user.id, &hash).await?;
    ctx.store.bump_session_version(&user.id).await?;
    let _ = ctx
        .store
        .delete_setting(reel_server::auth::SETUP_CODE_KEY)
        .await;
    println!("password set for {name}; every session of theirs has ended");
    Ok(())
}

async fn named(ctx: &Ctx, name: &str) -> anyhow::Result<reel_store::models::User> {
    ctx.store
        .user_by_name(name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no user named {name}; reel users list names them"))
}

/// How a user gets in under the sign-in mode in force.
fn signs_in(u: &reel_store::models::User, cfg: &reel_core::Config) -> &'static str {
    use reel_core::config::AuthMode;
    let waiting = u
        .link_until
        .as_deref()
        .is_some_and(|t| t > reel_core::format::now().as_str());
    match cfg.auth.mode {
        AuthMode::Local if u.password_hash.is_some() => "password",
        AuthMode::Local => "no password",
        AuthMode::Oidc if u.linked_to(&cfg.auth.oidc_issuer) => "linked",
        AuthMode::Oidc if waiting => "not linked, a link waits",
        AuthMode::Oidc => "not linked",
        AuthMode::Proxy | AuthMode::Single => "by name",
    }
}

async fn users(ctx: &Ctx, cmd: UsersCmd) -> anyhow::Result<()> {
    let cfg = ctx.config();
    match cmd {
        UsersCmd::List => {
            let all = ctx.store.users().await?;
            let width = all
                .iter()
                .map(|u| u.name.chars().count())
                .max()
                .unwrap_or(4)
                .max(4);
            let mode = format!("{:?}", cfg.auth.mode).to_lowercase();
            println!(
                "{:width$}  {:6}  {:8}  sign-in ({mode})",
                "name", "role", "state"
            );
            for u in &all {
                let state = if u.disabled_at.is_some() {
                    "disabled"
                } else {
                    "active"
                };
                println!(
                    "{:width$}  {:6}  {:8}  {}",
                    u.name,
                    u.role,
                    state,
                    signs_in(u, &cfg)
                );
            }
        }
        UsersCmd::Link { name } => {
            let user = named(ctx, &name).await?;
            let (url, until) =
                reel_server::accounts::make_link(&ctx.store, &cfg, &user, "reel users link")
                    .await?;
            println!("{url}");
            println!(
                "Whoever opens this link and signs in at the identity provider is {} from then on. It works once, until {}.",
                user.name,
                reel_core::format::datetime(&until)
            );
        }
        UsersCmd::Unlink { name } => {
            let user = named(ctx, &name).await?;
            reel_server::accounts::unlink(&ctx.store, &user, "reel users unlink").await?;
            println!(
                "{} is unlinked from the identity provider; every session of theirs has ended",
                user.name
            );
        }
        UsersCmd::Rename { name, new_name } => {
            let user = named(ctx, &name).await?;
            let to = reel_server::accounts::rename(&ctx.store, &user, &new_name).await?;
            println!("{} is {to} now", user.name);
        }
    }
    Ok(())
}
