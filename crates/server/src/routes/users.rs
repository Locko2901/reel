use crate::accounts::{
    self, hash_password, Backend, Credentials, LinkRefused, OidcPending, SessionUser, OIDC_PENDING,
    OIDC_PENDING_MINUTES, SIGNED_IN_AT,
};
use crate::auth::{client_ip, error_page, same_origin, User, SETUP_CODE_KEY};
use crate::error::{AppError, AppResult};
use crate::routes::{html, html_status};
use crate::AppState;
use askama::Template;
use axum::extract::{ConnectInfo, Form, Path, Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{Extension, Json};
use axum_login::AuthSession;
use reel_core::auth::{password_ok, same_code};
use reel_core::config::AuthMode;
use reel_core::format;
use reel_store::models::User as UserRow;
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;

#[derive(Template)]
#[template(path = "signin.html")]
pub struct SigninPage {
    pub site: String,
    pub local: bool,
    pub oidc: Option<String>,
    pub first: bool,
    pub error: String,
    pub next: String,
    pub mode_line: String,
    /// The provider has not answered yet: the page asks again by itself.
    pub retry: bool,
    /// Behind an identity provider with no admin yet: the signed-in user's name, to whom the
    /// page offers the setup code's field.
    pub claim: Option<String>,
}

#[derive(Deserialize)]
pub struct SigninQuery {
    pub next: Option<String>,
    pub error: Option<String>,
}

/// Where a sign-in goes on to: a path that a browser reads as one on this site, else the library.
/// A browser takes a backslash for a slash and drops tabs and line breaks, so none may pass.
fn safe_next(next: Option<&str>, public_url: &str) -> String {
    let Some(n) = next else {
        return "/".into();
    };
    if !n.starts_with('/') || n.contains('\\') || n.chars().any(char::is_control) {
        return "/".into();
    }
    let Ok(base) = url::Url::parse(public_url) else {
        return "/".into();
    };
    match base.join(n) {
        Ok(to) if to.origin() == base.origin() => n.to_string(),
        _ => "/".into(),
    }
}

pub const RETRY_SECONDS: u32 = 20;

/// The sign-in page's error codes: an address carries a code, the words are reel's own.
const REFUSED: &str = "refused";

fn error_words(code: &str, provider: &str) -> String {
    match code {
        REFUSED => format!("{provider} did not sign you in. Try again, or ask an admin."),
        _ => String::new(),
    }
}

async fn signin_view(st: &AppState, next: &str, error: &str) -> SigninPage {
    let cfg = st.ctx.config();
    let local = cfg.auth.mode == AuthMode::Local;
    let first = local && st.ctx.store.user_count().await.unwrap_or(1) == 0;
    let waiting = match &st.provider {
        Some(p) if cfg.auth.mode == AuthMode::Oidc => p.for_sign_in().await.is_none(),
        _ => false,
    };
    SigninPage {
        site: cfg.site_name.clone(),
        local,
        oidc: (cfg.auth.mode == AuthMode::Oidc).then(|| st.provider_name()),
        first,
        error: error.into(),
        next: next.into(),
        mode_line: match cfg.auth.mode {
            AuthMode::Proxy => {
                "The proxy in front signs you in; there is nothing to type here.".into()
            }
            AuthMode::Single => {
                "This reel runs for a single user; there is nothing to sign in to.".into()
            }
            AuthMode::Oidc if waiting => format!(
                "{} is not reachable yet. reel keeps trying, and this page asks again every {RETRY_SECONDS} seconds.",
                st.provider_name()
            ),
            AuthMode::Local | AuthMode::Oidc => String::new(),
        },
        retry: waiting,
        claim: None,
    }
}

pub async fn signin_page(
    State(st): State<AppState>,
    auth: AuthSession<Backend>,
    Query(q): Query<SigninQuery>,
) -> AppResult<Response> {
    let next = safe_next(q.next.as_deref(), &st.ctx.config().public_url);
    let error = error_words(q.error.as_deref().unwrap_or(""), &st.provider_name());
    let mut page = signin_view(&st, &next, &error).await;
    page.claim = claimer(&st, &auth).await?;
    Ok(html(page))
}

/// The signed-in user's name when the server behind an identity provider waits for its first
/// admin's setup code.
async fn claimer(st: &AppState, auth: &AuthSession<Backend>) -> AppResult<Option<String>> {
    let cfg = st.ctx.config();
    let Some(me) = auth.user.as_ref() else {
        return Ok(None);
    };
    let waits = cfg.auth.mode == AuthMode::Oidc
        && !me.admin
        && accounts::wants_setup_code(&st.ctx.store, &cfg).await?;
    Ok(waits.then(|| me.name.clone()))
}

#[derive(Deserialize)]
pub struct SigninForm {
    pub name: String,
    pub password: String,
    #[serde(default)]
    pub remember: Option<String>,
    #[serde(default)]
    pub next: Option<String>,
}

async fn wait_page(st: &AppState, next: &str, secs: u64) -> Response {
    let page = signin_view(
        st,
        next,
        &format!("Too many tries on that name; wait {secs} s."),
    )
    .await;
    let mut r = html_status(StatusCode::TOO_MANY_REQUESTS, page);
    if let Ok(v) = header::HeaderValue::from_str(&secs.to_string()) {
        r.headers_mut().insert(header::RETRY_AFTER, v);
    }
    r
}

/// Signs the session in: the id rotates, the sign-in time is kept for the absolute lifetime,
/// and the expiry is set outright (a pending sign-in had a short one).
async fn finish_login(
    auth: &mut AuthSession<Backend>,
    user: &SessionUser,
    remember: bool,
    session_days: u64,
) -> AppResult<()> {
    let signed_in_before = auth.user.is_some();
    auth.login(user)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("login: {e}")))?;
    // the login rotates the id only for a session nobody was signed in to
    if signed_in_before {
        auth.session
            .cycle_id()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("session: {e}")))?;
    }
    auth.session
        .insert(SIGNED_IN_AT, jiff::Timestamp::now().as_second())
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("session: {e}")))?;
    auth.session.set_expiry(Some(if remember {
        tower_sessions::Expiry::OnInactivity(time::Duration::days(session_days.max(1) as i64))
    } else {
        tower_sessions::Expiry::OnSessionEnd
    }));
    Ok(())
}

pub async fn signin(
    State(st): State<AppState>,
    mut auth: AuthSession<Backend>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(f): Form<SigninForm>,
) -> AppResult<Response> {
    let cfg = st.ctx.config();
    let next = safe_next(f.next.as_deref(), &cfg.public_url);
    if cfg.auth.mode != AuthMode::Local {
        return Ok(Redirect::to(&next).into_response());
    }
    if !same_origin(&Method::POST, &headers) {
        return Err(AppError::Forbidden("that came from another site".into()));
    }
    let name = f.name.trim().to_string();
    if name.chars().count() > accounts::NAME_MOST {
        let page = signin_view(&st, &next, "That name and password do not go together.").await;
        return Ok(html_status(StatusCode::UNAUTHORIZED, page));
    }
    let client = client_ip(&headers, peer.ip(), &cfg);
    if let Err(secs) = st.limiter.check(&name, client) {
        return Ok(wait_page(&st, &next, secs).await);
    }
    let ip = client.to_string();
    let user = auth
        .authenticate(Credentials::Password {
            name: name.clone(),
            password: f.password.clone(),
            ip: ip.clone(),
        })
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("authenticate: {e}")))?;
    let Some(user) = user else {
        let page = signin_view(&st, &next, "That name and password do not go together.").await;
        return Ok(html_status(StatusCode::UNAUTHORIZED, page));
    };
    finish_login(
        &mut auth,
        &user,
        f.remember.is_some(),
        cfg.auth.session_days,
    )
    .await?;
    tracing::info!(user = %user.name, ip, "signed in");
    Ok(Redirect::to(&next).into_response())
}

#[derive(Deserialize)]
pub struct FirstForm {
    pub name: String,
    pub password: String,
    #[serde(default)]
    pub code: String,
}

/// The first admin, when built-in accounts have nobody yet: behind the setup code, made once.
pub async fn first_admin(
    State(st): State<AppState>,
    mut auth: AuthSession<Backend>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(f): Form<FirstForm>,
) -> AppResult<Response> {
    let cfg = st.ctx.config();
    if cfg.auth.mode != AuthMode::Local {
        return Err(AppError::Conflict(
            "built-in accounts are not in use".into(),
        ));
    }
    if !same_origin(&Method::POST, &headers) {
        return Err(AppError::Forbidden("that came from another site".into()));
    }
    if st.ctx.store.user_count().await? > 0 {
        return Err(AppError::Conflict(
            "there is a user already; sign in".into(),
        ));
    }
    let Some(code) = st.ctx.store.setting(SETUP_CODE_KEY).await? else {
        return Err(AppError::Conflict(
            "no setup code is waiting; restart reel to get one".into(),
        ));
    };
    let ip = client_ip(&headers, peer.ip(), &cfg).to_string();
    if !same_code(&f.code, &code) {
        st.ctx
            .store
            .add_signin_attempt(&ip, "first admin", false)
            .await?;
        tracing::warn!(ip, "first admin refused: wrong setup code");
        let page = signin_view(
            &st,
            "/",
            "That is not the setup code; it is in reel's log and in reel doctor.",
        )
        .await;
        return Ok(html_status(StatusCode::FORBIDDEN, page));
    }
    let name = f.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 40 {
        let page = signin_view(&st, "/", "A name has 1 to 40 characters.").await;
        return Ok(html_status(StatusCode::BAD_REQUEST, page));
    }
    if let Err(e) = password_ok(&f.password) {
        let page = signin_view(&st, "/", &e).await;
        return Ok(html_status(StatusCode::BAD_REQUEST, page));
    }
    let password = f.password.clone();
    let hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let user = UserRow::new(&name, "admin", Some(hash));
    if !st
        .ctx
        .store
        .insert_first_admin(&user, SETUP_CODE_KEY)
        .await?
    {
        return Err(AppError::Conflict(
            "there is a user already; sign in".into(),
        ));
    }
    tracing::info!(user = %user.name, ip, "first admin made");
    finish_login(
        &mut auth,
        &SessionUser::from(&user),
        true,
        cfg.auth.session_days,
    )
    .await?;
    Ok(Redirect::to("/").into_response())
}

#[derive(Deserialize)]
pub struct ClaimForm {
    #[serde(default)]
    pub code: String,
}

/// The first admin behind an identity provider: a user signed in through it, with the setup
/// code, once, while there is no admin.
pub async fn claim_admin(
    State(st): State<AppState>,
    auth: AuthSession<Backend>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(f): Form<ClaimForm>,
) -> AppResult<Response> {
    let cfg = st.ctx.config();
    if cfg.auth.mode != AuthMode::Oidc {
        return Err(AppError::Conflict(
            "the setup code here is for a server behind an identity provider".into(),
        ));
    }
    if !same_origin(&Method::POST, &headers) {
        return Err(AppError::Forbidden("that came from another site".into()));
    }
    let Some(me) = claimer(&st, &auth).await? else {
        return if auth.user.is_some() {
            Err(AppError::Conflict("this reel has an admin already".into()))
        } else {
            Ok(Redirect::to("/signin").into_response())
        };
    };
    let Some(code) = st.ctx.store.setting(SETUP_CODE_KEY).await? else {
        return Err(AppError::Conflict(
            "no setup code is waiting; restart reel to get one".into(),
        ));
    };
    let ip = client_ip(&headers, peer.ip(), &cfg).to_string();
    if !same_code(&f.code, &code) {
        st.ctx
            .store
            .add_signin_attempt(&ip, "setup code", false)
            .await?;
        tracing::warn!(ip, user = %me, "first admin refused: wrong setup code");
        let mut page = signin_view(
            &st,
            "/",
            "That is not the setup code; it is in reel's log and in reel doctor.",
        )
        .await;
        page.claim = Some(me);
        return Ok(html_status(StatusCode::FORBIDDEN, page));
    }
    let id = auth.user.as_ref().map(|u| u.id.clone()).unwrap_or_default();
    if !st.ctx.store.claim_first_admin(&id, SETUP_CODE_KEY).await? {
        return Err(AppError::Conflict("this reel has an admin already".into()));
    }
    tracing::info!(user = %me, ip, "first admin made with the setup code");
    Ok(Redirect::to("/").into_response())
}

pub async fn signout(mut auth: AuthSession<Backend>) -> AppResult<Response> {
    auth.logout()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("logout: {e}")))?;
    Ok(Redirect::to("/signin").into_response())
}

/// Every session of the user ends, this one too: the session version is part of what a
/// session has to match.
pub async fn signout_everywhere(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    mut auth: AuthSession<Backend>,
) -> AppResult<Json<Value>> {
    st.ctx.store.bump_session_version(&me.id).await?;
    let _ = auth.logout().await;
    Ok(Json(json!({ "ok": true })))
}

pub async fn oidc_start(
    State(st): State<AppState>,
    auth: AuthSession<Backend>,
) -> AppResult<Response> {
    to_provider(&st, &auth, None).await
}

async fn to_provider(
    st: &AppState,
    auth: &AuthSession<Backend>,
    link: Option<String>,
) -> AppResult<Response> {
    let oidc = match &st.provider {
        Some(p) => p.for_sign_in().await,
        None => None,
    };
    let Some(oidc) = oidc else {
        return Ok(error_page(
            StatusCode::SERVICE_UNAVAILABLE,
            &format!("{} is not reachable yet.", st.provider_name()),
            "reel keeps trying by itself; open this page again in a minute.",
        ));
    };
    let (url, mut pending) = oidc.start();
    pending.link = link;
    auth.session
        .insert(OIDC_PENDING, pending)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("session: {e}")))?;
    auth.session
        .set_expiry(Some(tower_sessions::Expiry::AtDateTime(
            time::OffsetDateTime::now_utc() + time::Duration::minutes(OIDC_PENDING_MINUTES),
        )));
    Ok(Redirect::to(&url).into_response())
}

#[derive(Deserialize)]
pub struct OidcCallback {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

pub async fn oidc_callback(
    State(st): State<AppState>,
    mut auth: AuthSession<Backend>,
    Query(q): Query<OidcCallback>,
) -> AppResult<Response> {
    let Some((provider, oidc)) = st.provider.as_ref().and_then(|p| Some((p, p.current()?))) else {
        return Err(AppError::Busy(
            "the identity provider is not reachable".into(),
        ));
    };
    let pending: Option<OidcPending> = auth
        .session
        .get(OIDC_PENDING)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("session: {e}")))?;
    let Some(pending) = pending else {
        return Err(AppError::BadRequest(
            "no sign-in was started from this browser; try again".into(),
        ));
    };
    if !q
        .state
        .as_deref()
        .is_some_and(|state| same_code(state, &pending.csrf))
    {
        return Err(AppError::Forbidden(
            "the state does not match this browser's sign-in".into(),
        ));
    }
    auth.session
        .remove::<OidcPending>(OIDC_PENDING)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("session: {e}")))?;
    if let Some(e) = q.error {
        tracing::warn!(error = %e, why = q.error_description.as_deref().unwrap_or(""), "the provider refused");
        return Ok(Redirect::to(&format!("/signin?error={REFUSED}")).into_response());
    }
    let Some(code) = q.code else {
        return Err(AppError::BadRequest("the provider sent no code".into()));
    };
    let refused = |e: &dyn std::fmt::Display| {
        tracing::warn!(error = %e, "oidc sign-in failed");
        AppError::Forbidden(format!("the sign-in did not verify: {e}"))
    };
    let tokens = oidc
        .tokens(&code, &pending)
        .await
        .map_err(|e| refused(&e))?;
    let cfg = st.ctx.config();
    let mut verified = oidc.identity(&tokens, &pending, &cfg);
    if verified.as_ref().is_err_and(|e| e.by_its_signature()) {
        tracing::info!("the id token does not verify with the keys reel has; asking the identity provider for its keys again");
        if provider.refresh().await {
            if let Some(fresh) = provider.current() {
                verified = fresh.identity(&tokens, &pending, &cfg);
            }
        }
    }
    let identity = verified.map_err(|e| refused(&e))?;
    if let Some(token) = &pending.link {
        if let Err(why) = accounts::redeem_link(&st.ctx.store, token, &identity).await? {
            tracing::warn!(subject = %identity.subject, why = %why, "link refused");
            return Ok(error_page(
                StatusCode::CONFLICT,
                &why.words(&st.provider_name()),
                why.what_now(),
            ));
        }
    }
    let user = auth
        .authenticate(Credentials::Identity(identity))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("authenticate: {e}")))?;
    let Some(user) = user else {
        return Err(AppError::Forbidden("this account is disabled".into()));
    };
    finish_login(&mut auth, &user, true, st.ctx.config().auth.session_days).await?;
    tracing::info!(user = %user.name, "signed in through the identity provider");
    if !user.admin && accounts::wants_setup_code(&st.ctx.store, &cfg).await? {
        return Ok(Redirect::to("/signin").into_response());
    }
    Ok(Redirect::to("/").into_response())
}

/// A link an admin made: the sign-in through the provider that follows binds its user.
pub async fn oidc_link(
    State(st): State<AppState>,
    auth: AuthSession<Backend>,
    Path(token): Path<String>,
) -> AppResult<Response> {
    if st.ctx.config().auth.mode != AuthMode::Oidc {
        return Ok(error_page(
            StatusCode::GONE,
            "This reel does not sign in through an identity provider.",
            "Sign in on the sign-in page.",
        ));
    }
    let why = match accounts::user_by_link(&st.ctx.store, &token).await? {
        None => LinkRefused::Gone,
        Some(u) if u.linked_to(&st.ctx.config().auth.oidc_issuer) => LinkRefused::Linked(u.name),
        Some(u) if u.disabled_at.is_some() => LinkRefused::Disabled(u.name),
        Some(_) => return to_provider(&st, &auth, Some(token)).await,
    };
    Ok(error_page(
        StatusCode::GONE,
        &why.words(&st.provider_name()),
        why.what_now(),
    ))
}

pub async fn list_users(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let counts = st.ctx.store.owner_counts().await?;
    let users: Vec<Value> = st
        .ctx
        .store
        .users()
        .await?
        .iter()
        .map(|u| {
            json!({
                "id": u.id, "name": u.name, "role": u.role, "disabled": u.disabled_at.is_some(),
                "created": format::date(&u.created_at),
                "last_seen": u.last_seen_at.as_deref().map(format::datetime).unwrap_or_else(|| "never".into()),
                "clips": counts.iter().find(|(o, _)| o.as_deref() == Some(&u.id)).map(|(_, n)| *n).unwrap_or(0),
                "has_password": u.password_hash.is_some(),
            })
        })
        .collect();
    Ok(Json(json!({ "users": users })))
}

#[derive(Deserialize)]
pub struct NewUser {
    pub name: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub role: Option<String>,
}

async fn hashed(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))
}

pub async fn create_user(
    State(st): State<AppState>,
    Json(body): Json<NewUser>,
) -> AppResult<Json<Value>> {
    let cfg = st.ctx.config();
    let name = body.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(AppError::BadRequest("a name has 1 to 40 characters".into()));
    }
    if st.ctx.store.user_by_name(&name).await?.is_some() {
        return Err(AppError::Conflict(format!("{name} exists already")));
    }
    let role = match body.role.as_deref().unwrap_or("member") {
        "admin" => "admin",
        "member" => "member",
        other => return Err(AppError::BadRequest(format!("{other} is not a role"))),
    };
    let hash = if cfg.auth.mode == AuthMode::Local {
        password_ok(&body.password).map_err(AppError::BadRequest)?;
        Some(hashed(body.password.clone()).await?)
    } else {
        None
    };
    let user = UserRow::new(&name, role, hash);
    st.ctx.store.insert_user(&user).await?;
    tracing::info!(user = %user.name, role, "user made");
    Ok(Json(
        json!({ "ok": true, "id": user.id, "name": user.name, "role": user.role }),
    ))
}

#[derive(Deserialize)]
pub struct PatchUser {
    pub name: Option<String>,
    pub role: Option<String>,
    pub disabled: Option<bool>,
    pub password: Option<String>,
}

pub async fn patch_user(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
    Json(body): Json<PatchUser>,
) -> AppResult<Json<Value>> {
    let user = st.ctx.store.user(&id).await?.ok_or(AppError::NotFound)?;
    let admins = st.ctx.store.admin_count().await?;
    let last_admin = user.is_admin() && user.disabled_at.is_none() && admins <= 1;
    if let Some(name) = &body.name {
        // behind a proxy or as the single user the name is who you are: renamed, you would come
        // back as somebody new
        let by_name = matches!(
            st.ctx.config().auth.mode,
            AuthMode::Proxy | AuthMode::Single
        );
        if by_name && user.id == me.id {
            return Err(AppError::Conflict(
                "you come in by this name; another admin, or reel users rename, renames you".into(),
            ));
        }
        accounts::rename(&st.ctx.store, &user, name)
            .await
            .map_err(accounts::Refused::app)?;
    }
    if let Some(role) = &body.role {
        if !["admin", "member"].contains(&role.as_str()) {
            return Err(AppError::BadRequest(format!("{role} is not a role")));
        }
        if role == "member" && last_admin {
            return Err(AppError::Conflict("the last admin stays an admin".into()));
        }
        st.ctx.store.set_role(&id, role).await?;
    }
    if let Some(disabled) = body.disabled {
        if disabled && user.id == me.id {
            return Err(AppError::Conflict("you cannot disable yourself".into()));
        }
        if disabled && last_admin {
            return Err(AppError::Conflict("the last admin stays enabled".into()));
        }
        st.ctx.store.set_disabled(&id, disabled).await?;
    }
    if let Some(pw) = &body.password {
        password_ok(pw).map_err(AppError::BadRequest)?;
        let hash = hashed(pw.clone()).await?;
        st.ctx.store.set_password(&id, &hash).await?;
        st.ctx.store.bump_session_version(&id).await?;
    }
    tracing::info!(user = %user.name, by = %me.name, "user changed");
    Ok(Json(json!({ "ok": true })))
}

/// A one-time link for the user to sign in through the provider by, which binds them.
pub async fn make_link(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let user = st.ctx.store.user(&id).await?.ok_or(AppError::NotFound)?;
    let (url, until) = accounts::make_link(&st.ctx.store, &st.ctx.config(), &user, &me.name)
        .await
        .map_err(accounts::Refused::app)?;
    Ok(Json(json!({
        "url": url, "until": format::datetime(&until), "hours": accounts::LINK_HOURS,
    })))
}

pub async fn unlink(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let user = st.ctx.store.user(&id).await?.ok_or(AppError::NotFound)?;
    if user.id == me.id {
        return Err(AppError::Conflict(
            "you cannot unlink yourself; another admin can".into(),
        ));
    }
    accounts::unlink(&st.ctx.store, &user, &me.name).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn signin_attempts(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let since = jiff::Timestamp::now()
        .checked_sub(jiff::Span::new().hours(24 * 7))
        .map(|t| t.to_string())
        .unwrap_or_default();
    let rows: Vec<Value> = st
        .ctx
        .store
        .failed_attempts_after(&since)
        .await?
        .iter()
        .map(|a| json!({ "ip": a.ip, "name": a.name, "when": format::when(&a.at) }))
        .collect();
    Ok(Json(json!({ "attempts": rows })))
}

#[cfg(test)]
mod tests {
    use super::safe_next;

    #[test]
    fn the_way_back_is_a_path_on_this_site_as_a_browser_reads_it() {
        let site = "https://clips.example.org";
        for (next, to) in [
            ("/clips/01J?from=%2F", "/clips/01J?from=%2F"),
            ("/%5Cevil.example/", "/%5Cevil.example/"),
            ("//evil.example/", "/"),
            ("/\\evil.example/", "/"),
            ("/\t/evil.example/", "/"),
            ("/\n/evil.example/", "/"),
            ("https://evil.example/", "/"),
            ("clips", "/"),
        ] {
            assert_eq!(safe_next(Some(next), site), to, "{next:?}");
        }
        assert_eq!(safe_next(None, site), "/");
        assert_eq!(
            safe_next(Some("/clips/x"), ""),
            "/",
            "no public URL, no way back"
        );
    }
}
