pub mod accounts;
pub mod auth;
pub mod back;
pub mod device;
pub mod disk;
pub mod error;
pub mod headers;
pub mod rights;
pub mod routes;
pub mod serve_file;
pub mod settings;
pub mod single;
pub mod standalone;
pub mod stats;
pub mod views;
pub mod zip;

use axum::extract::DefaultBodyLimit;
use axum::response::IntoResponse;
use axum::routing::{delete, get, patch, post, put};
use axum::Router;
use reel_core::config::AuthMode;
use reel_pipeline::Ctx;
use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;

/// A pick's body limit: 20,000 ids are about 600 KB, and the body is read in full before counting.
const PICK_BODY: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub ctx: Ctx,
    pub started_at: jiff::Timestamp,
    pub disk: disk::DiskCache,
    pub sessions: accounts::HashedIdStore<tower_sessions_sqlx_store::SqliteStore>,
    pub limiter: std::sync::Arc<accounts::NameLimiter>,
    /// Per address: the sign-in routes, and the pairing route on its own.
    pub signin_limits: accounts::AddressLimits,
    pub pair_limits: accounts::AddressLimits,
    pub provider: Option<std::sync::Arc<accounts::Provider>>,
    pub setup_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub uploads_busy: std::sync::Arc<tokio::sync::Mutex<std::collections::HashSet<String>>>,
    pub hashing: std::sync::Arc<routes::pieces::Hashing>,
    pub links_making: std::sync::Arc<routes::api::LinksMaking>,
    pub zips: std::sync::Arc<routes::zips::Pending>,
    /// The standalone library's key, which only the app is handed.
    pub app_key: Option<single::AppKey>,
}

impl AppState {
    pub async fn new(ctx: Ctx) -> anyhow::Result<Self> {
        let cfg = ctx.config();
        let inner = tower_sessions_sqlx_store::SqliteStore::new(ctx.store.pool().clone());
        inner.migrate().await?;
        let sessions = accounts::HashedIdStore(inner);
        reel_pipeline::housekeeping::clear_not_games_once(&ctx).await?;
        reel_pipeline::housekeeping::remake_utc_titles_once(&ctx).await?;
        reel_pipeline::housekeeping::remake_not_game_titles_once(&ctx).await?;
        reel_pipeline::housekeeping::mark_others_footage_once(&ctx).await?;
        {
            let ctx = ctx.clone();
            tokio::spawn(async move {
                if let Err(e) = reel_pipeline::housekeeping::remove_personal_tags_once(&ctx).await {
                    tracing::warn!(error = %e, "the files' tags were not looked through");
                }
            });
        }
        if let Some(code) = auth::ensure_setup_code(&ctx).await? {
            if cfg.auth.mode == AuthMode::Oidc {
                tracing::warn!(
                    code,
                    "no admin yet: sign in through the identity provider, then type this setup code"
                );
            } else {
                tracing::warn!(
                    code,
                    "no user yet: the first admin needs this setup code at /signin"
                );
            }
        }
        let provider = if cfg.auth.mode == AuthMode::Oidc {
            let p = std::sync::Arc::new(accounts::Provider::new(cfg.clone()));
            p.refresh().await;
            tokio::spawn(p.clone().keep(ctx.shutdown.clone()));
            Some(p)
        } else {
            None
        };
        let limiter = std::sync::Arc::new(accounts::NameLimiter::new());
        let signin_limits = accounts::address_limits(&cfg);
        let pair_limits = accounts::address_limits(&cfg);
        tokio::spawn(accounts::prune_limits(
            limiter.clone(),
            vec![signin_limits.clone(), pair_limits.clone()],
            ctx.shutdown.clone(),
        ));
        // the app's standalone library has no first run
        let done = ctx.config().standalone() || routes::setup::is_done(&ctx).await;
        let app_key = ctx.config().standalone().then(single::AppKey::new);
        Ok(Self {
            ctx,
            started_at: jiff::Timestamp::now(),
            disk: disk::DiskCache::default(),
            sessions,
            limiter,
            signin_limits,
            pair_limits,
            provider,
            setup_done: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(done)),
            uploads_busy: std::sync::Arc::new(tokio::sync::Mutex::new(
                std::collections::HashSet::new(),
            )),
            hashing: Default::default(),
            links_making: Default::default(),
            zips: Default::default(),
            app_key,
        })
    }

    /// The identity provider as a person reads of it: the admin's name for it, else its host.
    pub fn provider_name(&self) -> String {
        let cfg = self.ctx.config();
        match &self.provider {
            _ if !cfg.auth.oidc_name.is_empty() => cfg.auth.oidc_name.clone(),
            Some(p) => p.host(),
            None => "the identity provider".into(),
        }
    }
}

pub async fn expire_sessions(
    store: accounts::HashedIdStore<tower_sessions_sqlx_store::SqliteStore>,
) {
    use tower_sessions::ExpiredDeletion;
    let _ = store
        .continuously_delete_expired(std::time::Duration::from_secs(600))
        .await;
}

/// The routes, for reel listening on `bound`.
pub fn router(state: AppState, bound: SocketAddr) -> Router {
    use standalone::Part;
    let guard = |part: Part| {
        axum::middleware::from_fn_with_state(
            standalone::Guard::new(&state, part),
            standalone::guard,
        )
    };
    let shared = Router::new()
        .route("/s/{token}", get(routes::share::page))
        .route("/s/{token}/video.mp4", get(routes::share::video))
        .route("/s/{token}/web.mp4", get(routes::share::web))
        .route("/s/{token}/web.json", get(routes::share::web_json))
        .route("/s/{token}/poster.jpg", get(routes::share::poster))
        .route("/s/{token}/download", get(routes::share::download))
        .route("/c/{token}", get(routes::collections::public_page))
        .route("/c/{token}/{clip}", get(routes::collections::public_clip))
        .route(
            "/c/{token}/{clip}/video.mp4",
            get(routes::collections::public_video),
        )
        .route(
            "/c/{token}/{clip}/poster.jpg",
            get(routes::collections::public_poster),
        )
        .route(
            "/c/{token}/{clip}/web.mp4",
            get(routes::collections::public_web),
        )
        .route(
            "/c/{token}/{clip}/web.json",
            get(routes::collections::public_web_json),
        )
        .route(
            "/c/{token}/{clip}/download",
            get(routes::collections::public_download),
        )
        .route_layer(guard(Part::Public));
    let public = Router::new()
        .route("/healthz", get(routes::health::healthz))
        .route("/signin", get(routes::users::signin_page))
        .route("/assets/{*path}", get(routes::assets::asset))
        .route("/app/open", get(single::open))
        .merge(shared)
        .merge(
            Router::new()
                .route("/oidc/callback", get(routes::users::oidc_callback))
                .route_layer(guard(Part::Accounts)),
        );

    let private = Router::new()
        .route("/", get(routes::pages::library))
        .route("/clips/{id}", get(routes::pages::clip))
        .route("/trash", get(routes::trash::page))
        .route("/stats", get(routes::stats::page))
        .route("/montage", get(routes::montage::page))
        .route(
            "/api/montage",
            post(routes::montage::create).layer(DefaultBodyLimit::max(PICK_BODY)),
        )
        .route("/api/trash/{id}", delete(routes::trash::purge))
        .route("/collections", get(routes::collections::list_page))
        .route("/collections/{id}", get(routes::collections::page))
        .route(
            "/api/collections",
            get(routes::collections::list).post(routes::collections::create),
        )
        .route(
            "/api/collections/{id}",
            patch(routes::collections::rename).delete(routes::collections::delete),
        )
        .route(
            "/api/collections/{id}/clips/{clip}",
            put(routes::collections::add_clip).delete(routes::collections::remove_clip),
        )
        .route(
            "/upload",
            get(routes::pages::upload_page).post(routes::upload::upload),
        )
        .route("/api/uploads", post(routes::upload::create_pieces))
        .route(
            "/api/uploads/{id}",
            get(routes::upload::pieces_status)
                .put(routes::upload::put_piece)
                .delete(routes::upload::abandon_pieces),
        )
        .route(
            "/api/uploads/{id}/finish",
            post(routes::upload::finish_pieces),
        )
        .route("/media/clips/{id}/current.mp4", get(routes::media::current))
        .route("/media/clips/{id}/download", get(routes::media::download))
        .route("/media/clips/{id}/original", get(routes::media::original))
        .route("/media/clips/{id}/discord.mp4", get(routes::media::discord))
        .route("/media/clips/{id}/anim.gif", get(routes::media::anim_gif))
        .route("/media/clips/{id}/anim.webp", get(routes::media::anim_webp))
        .route(
            "/media/renditions/{rid}/download",
            get(routes::media::rendition_download),
        )
        .route(
            "/media/renditions/{file}",
            get(routes::media::rendition_file),
        )
        .route(
            "/media/renditions/{rid}/track/{file}",
            get(routes::media::rendition_track),
        )
        .route(
            "/api/clips/{id}",
            get(routes::api::clip_json)
                .patch(routes::api::patch_clip)
                .delete(routes::api::delete_clip),
        )
        .route(
            "/api/clips/{id}/star",
            put(routes::api::star_clip).delete(routes::api::unstar_clip),
        )
        .route(
            "/api/bulk/delete",
            post(routes::bulk::delete).layer(DefaultBodyLimit::max(PICK_BODY)),
        )
        .route(
            "/api/bulk/restore",
            post(routes::bulk::restore).layer(DefaultBodyLimit::max(PICK_BODY)),
        )
        .route(
            "/api/bulk/purge",
            post(routes::bulk::purge).layer(DefaultBodyLimit::max(PICK_BODY)),
        )
        .route(
            "/api/bulk/star",
            post(routes::bulk::star).layer(DefaultBodyLimit::max(PICK_BODY)),
        )
        .route(
            "/api/bulk/collect",
            post(routes::bulk::collect).layer(DefaultBodyLimit::max(PICK_BODY)),
        )
        .route(
            "/api/bulk/download",
            post(routes::zips::ask).layer(DefaultBodyLimit::max(PICK_BODY)),
        )
        .route("/api/bulk/download/{token}", get(routes::zips::download))
        .route("/api/clips/{id}/restore", post(routes::api::restore_clip))
        .route("/api/clips/{id}/discard", post(routes::api::discard_clip))
        .route("/api/clips/{id}/cuts", post(routes::api::create_cut))
        .route("/api/clips/{id}/current", post(routes::api::set_current))
        .route("/api/clips/{id}/poster", post(routes::api::set_poster))
        .route(
            "/api/clips/{id}/poster/image",
            post(routes::api::upload_poster).layer(axum::extract::DefaultBodyLimit::max(
                reel_pipeline::poster_image::MAX_BYTES + 1,
            )),
        )
        .route("/api/clips/{id}/anims", get(routes::anims::list))
        .route("/api/renditions/{rid}", delete(routes::api::delete_cut))
        .route(
            "/api/renditions/{rid}/level",
            get(routes::api::rendition_level),
        )
        .route(
            "/api/renditions/{rid}/keyframes",
            get(routes::montage::keyframes),
        )
        .route("/jobs", get(routes::jobs::page))
        .route("/status", get(routes::status::page))
        .route("/api/status/doctor", post(routes::status::doctor))
        .route("/api/jobs", get(routes::jobs::list))
        .route("/api/jobs/counts", get(routes::jobs::counts))
        .route("/api/jobs/{id}", get(routes::api::job_json))
        .route("/api/jobs/{id}/cancel", post(routes::jobs::cancel))
        .route("/api/jobs/{id}/retry", post(routes::jobs::retry))
        .route("/api/jobs/{id}/dismiss", post(routes::jobs::dismiss))
        .route("/api/jobs/{id}/discard", post(routes::jobs::discard))
        .route("/api/events", get(routes::events::events))
        .route("/signout", post(routes::users::signout))
        .route("/api/disk", get(routes::api::disk))
        .route("/api/library/card/{id}", get(routes::pages::library_card))
        .route("/api/library/summary", get(routes::pages::library_summary))
        .route("/api/health", get(routes::health::healthz))
        .route("/api/app/update", get(routes::device::release_update))
        .merge(
            Router::new()
                .route("/shares", get(routes::pages::shares))
                .route(
                    "/api/collections/{id}/link",
                    post(routes::collections::make_link).delete(routes::collections::revoke_link),
                )
                .route(
                    "/api/bulk/links",
                    post(routes::bulk::links).layer(DefaultBodyLimit::max(PICK_BODY)),
                )
                .route("/api/clips/{id}/shares", post(routes::api::create_share))
                .route("/api/clips/{id}/link-wait", get(routes::api::link_wait))
                .route("/api/shares/{id}", delete(routes::api::revoke_share))
                .route_layer(guard(Part::Links)),
        )
        .merge(
            Router::new()
                .route(
                    "/app/installer/{flavour}",
                    get(routes::device::installer_download),
                )
                .route_layer(guard(Part::Devices)),
        )
        .merge(
            Router::new()
                .route(
                    "/api/sessions/signout-all",
                    post(routes::users::signout_everywhere),
                )
                .route_layer(guard(Part::Accounts)),
        )
        .merge(
            Router::new()
                .route(
                    "/share-target",
                    get(routes::upload::share_target).post(routes::upload::share_target_post),
                )
                .route_layer(guard(Part::Server)),
        );
    #[cfg(feature = "style-page")]
    let private = private
        .route("/style", get(routes::style::page))
        .route("/__shot/clip", get(routes::shots::clip))
        .route("/__shot/share", get(routes::shots::share))
        .route("/__shot/collection", get(routes::shots::collection))
        .route("/__shot/montage", get(routes::shots::montage))
        .route("/__shot/clip-tracks", get(routes::shots::clip_tracks))
        .route(
            "/__shot/collection-link",
            get(routes::shots::collection_link),
        );
    let admin = Router::new()
        .route("/settings", get(routes::settings::index))
        .route("/settings/{section}", get(routes::settings::page))
        .route(
            "/api/settings",
            get(routes::settings::get_api).patch(routes::settings::patch_api),
        )
        .route("/api/settings/history", get(routes::settings::history))
        .route(
            "/api/status/update-check",
            post(routes::status::update_check),
        )
        .route("/api/imports/scan", post(routes::imports::scan))
        .route(
            "/api/storage/purge-trash",
            post(routes::storage::purge_trash),
        )
        .route(
            "/api/storage/clear-discord",
            post(routes::storage::clear_discord),
        )
        .route("/api/storage/cap", get(routes::storage::cap_preview))
        .route(
            "/api/storage/rebuild-derived",
            post(routes::storage::rebuild_derived),
        )
        .route(
            "/api/storage/remake-posters",
            post(routes::storage::remake_posters),
        )
        .route(
            "/api/storage/disk-refresh",
            post(routes::storage::disk_refresh),
        )
        .route(
            "/api/export/database",
            get(routes::advanced::export_database),
        )
        .merge(
            Router::new()
                .route("/api/devices/pair-code", post(routes::device::pair_code))
                .route("/api/devices", get(routes::device::list_devices))
                .route("/api/devices/{id}", delete(routes::device::revoke_device))
                .route_layer(guard(Part::Devices)),
        )
        .merge(
            Router::new()
                .route(
                    "/api/users",
                    get(routes::users::list_users).post(routes::users::create_user),
                )
                .route("/api/users/{id}", patch(routes::users::patch_user))
                .route(
                    "/api/users/{id}/link",
                    post(routes::users::make_link).delete(routes::users::unlink),
                )
                .route("/api/signin-attempts", get(routes::users::signin_attempts))
                .route_layer(guard(Part::Accounts)),
        )
        .merge(
            Router::new()
                .route("/api/notify/test", post(routes::settings::notify_test))
                .route("/api/external/test", post(routes::settings::external_test))
                .route(
                    "/api/external/check",
                    post(routes::settings::external_check),
                )
                .route("/api/storage/clear-web", post(routes::storage::clear_web))
                .route("/api/logs", get(routes::advanced::logs))
                .route("/api/logs/download", get(routes::advanced::logs_download))
                .route("/api/export/config", get(routes::advanced::export_config))
                .route("/api/restart", post(routes::advanced::restart))
                .route(
                    "/api/storage/remake-embeds",
                    post(routes::storage::remake_embeds),
                )
                .route("/setup", get(routes::setup::page))
                .route("/api/setup/checks", get(routes::setup::folder_checks))
                .route("/api/setup/mkdir", post(routes::setup::mkdir))
                .route("/api/setup/url-check", post(routes::setup::url_check))
                .route("/api/setup/done", post(routes::setup::done))
                .route_layer(guard(Part::Server)),
        )
        .layer(axum::middleware::from_fn(auth::require_admin));
    let private = private
        .merge(admin)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_user,
        ));

    let cfg = state.ctx.config();
    let limited = Router::new()
        .route("/signin", post(routes::users::signin))
        .route("/signin/first", post(routes::users::first_admin))
        .route("/signin/claim", post(routes::users::claim_admin))
        .route("/oidc/start", get(routes::users::oidc_start))
        .route("/oidc/link/{token}", get(routes::users::oidc_link))
        .route_layer(guard(Part::Accounts))
        .layer(DefaultBodyLimit::max(accounts::SIGNIN_BODY))
        .layer(
            tower_governor::GovernorLayer::new(state.signin_limits.clone()).error_handler(|e| {
                match e {
                    tower_governor::GovernorError::TooManyRequests { wait_time, .. } => {
                        let mut r = auth::error_page(
                            axum::http::StatusCode::TOO_MANY_REQUESTS,
                            "Too many sign-in attempts from this address.",
                            &format!("Wait {wait_time} s and try again."),
                        );
                        if let Ok(v) = axum::http::HeaderValue::from_str(&wait_time.to_string()) {
                            r.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
                        }
                        r
                    }
                    other => auth::error_page(
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        "The rate limiter could not read the request.",
                        &other.to_string(),
                    ),
                }
            }),
        );

    let pair = Router::new()
        .route("/api/device/pair", post(routes::device::pair))
        .route_layer(guard(Part::Devices))
        .route_layer(DefaultBodyLimit::max(accounts::SIGNIN_BODY))
        .route_layer(
            tower_governor::GovernorLayer::new(state.pair_limits.clone()).error_handler(|e| match e {
                tower_governor::GovernorError::TooManyRequests { wait_time, .. } => {
                    let mut r = (
                        axum::http::StatusCode::TOO_MANY_REQUESTS,
                        axum::Json(serde_json::json!({
                            "error": format!("too many pairing tries from this address; wait {wait_time} s"),
                            "retry_after": wait_time,
                        })),
                    )
                        .into_response();
                    if let Ok(v) = axum::http::HeaderValue::from_str(&wait_time.to_string()) {
                        r.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
                    }
                    r
                }
                other => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(serde_json::json!({ "error": other.to_string() })),
                )
                    .into_response(),
            }),
        );
    let device = Router::new()
        .route("/api/device/whoami", get(routes::device::whoami))
        .route("/api/device/sections", get(routes::device::sections))
        .route("/api/device/uploads", post(routes::device::create_upload))
        .route(
            "/api/device/uploads/{id}",
            get(routes::device::upload_status)
                .put(routes::device::put_chunk)
                .delete(routes::device::abandon_upload),
        )
        .route(
            "/api/device/uploads/{id}/finish",
            post(routes::device::finish_upload),
        )
        .route("/api/device/clips", get(routes::device::clips))
        .route("/api/device/clips/{id}", get(routes::device::clip))
        .route("/api/device/share", post(routes::device::share))
        .route("/api/device/update", get(routes::device::update))
        .route(
            "/api/device/update/installer",
            get(routes::device::update_installer),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            device::require_device,
        ))
        .route_layer(guard(Part::Devices));

    let session_layer = tower_sessions::SessionManagerLayer::new(state.sessions.clone())
        .with_name(accounts::session_cookie(&cfg.public_url))
        .with_http_only(true)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_secure(cfg.public_url.starts_with("https://"))
        .with_expiry(tower_sessions::Expiry::OnInactivity(time::Duration::days(
            cfg.auth.session_days.max(1) as i64,
        )));
    let backend = accounts::Backend::new(state.ctx.clone());
    let auth_layer = axum_login::AuthManagerLayerBuilder::new(backend, session_layer).build();

    let app = Router::new()
        .merge(public)
        .merge(limited)
        .merge(private)
        .merge(pair)
        .merge(device)
        .layer(auth_layer);
    let app = if cfg.auth.mode == AuthMode::Single {
        app.layer(axum::middleware::from_fn_with_state(
            single::Bound {
                st: state.clone(),
                addr: bound,
            },
            single::own_address,
        ))
    } else {
        app
    };
    app.layer(DefaultBodyLimit::max(64 * 1024 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            headers::add,
        ))
        .with_state(state)
}

pub async fn serve(
    state: AppState,
    listen: &str,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(listen).await?;
    let bound = listener.local_addr()?;
    tracing::info!(listen = %bound, "reel listening");
    axum::serve(
        listener,
        router(state, bound).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move { shutdown.cancelled().await })
    .await?;
    Ok(())
}

pub async fn bind(
    state: AppState,
    listen: &str,
    shutdown: CancellationToken,
) -> anyhow::Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    let listener = tokio::net::TcpListener::bind(listen).await?;
    serve_listener(state, listener, shutdown)
}

/// Serves on a listener already bound, so no other test can take a test's free port in between.
pub fn serve_listener(
    state: AppState,
    listener: tokio::net::TcpListener,
    shutdown: CancellationToken,
) -> anyhow::Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    let addr = listener.local_addr()?;
    let app = router(state, addr).into_make_service_with_connect_info::<SocketAddr>();
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .await;
    });
    Ok((addr, handle))
}
