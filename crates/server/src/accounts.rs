//! Sessions, password hashes, sign-in limits and the OIDC client, on axum-login and governor.
use axum::extract::ConnectInfo;
use axum::http::Request;
use axum_login::{AuthUser, AuthnBackend, UserId};
use governor::clock::{Clock, DefaultClock};
use governor::state::keyed::DefaultKeyedStateStore;
use governor::{Quota, RateLimiter};
use openidconnect::core::{
    CoreAuthDisplay, CoreAuthPrompt, CoreAuthenticationFlow, CoreErrorResponseType,
    CoreGenderClaim, CoreJsonWebKey, CoreJweContentEncryptionAlgorithm, CoreJwsSigningAlgorithm,
    CoreProviderMetadata, CoreRevocableToken, CoreRevocationErrorResponse,
    CoreTokenIntrospectionResponse, CoreTokenType,
};
use openidconnect::{
    AdditionalClaims, AuthorizationCode, Client, ClientId, ClientSecret, CsrfToken,
    EmptyExtraTokenFields, EndpointMaybeSet, EndpointNotSet, EndpointSet, IdTokenFields, IssuerUrl,
    Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, StandardErrorResponse,
    StandardTokenResponse, TokenResponse,
};
use reel_core::Config;
use reel_store::models::User as UserRow;
use reel_store::Store;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;
use tower_governor::key_extractor::KeyExtractor;
use tower_governor::GovernorError;
use tower_sessions::session::{Id, Record};
use tower_sessions::{session_store, ExpiredDeletion, SessionStore};

pub const SESSION_COOKIE: &str = "reel_session";
pub const SIGNED_IN_AT: &str = "signed_in_at";
pub const OIDC_PENDING: &str = "oidc_pending";
/// How long a browser has between the redirect to the provider and the callback.
pub const OIDC_PENDING_MINUTES: i64 = 10;
pub const SIGNIN_BURST: u32 = 5;
pub const SIGNIN_PERIOD: Duration = Duration::from_secs(120);

/// The auth hash covers the password and session version, so changing either ends every session.
#[derive(Debug, Clone)]
pub struct SessionUser {
    pub id: String,
    pub name: String,
    pub admin: bool,
    auth_hash: Vec<u8>,
}

impl From<&UserRow> for SessionUser {
    fn from(u: &UserRow) -> Self {
        let digest = Sha256::digest(
            format!(
                "{}:{}",
                u.password_hash.as_deref().unwrap_or(""),
                u.session_version
            )
            .as_bytes(),
        );
        SessionUser {
            id: u.id.clone(),
            name: u.name.clone(),
            admin: u.is_admin(),
            auth_hash: digest.to_vec(),
        }
    }
}

impl AuthUser for SessionUser {
    type Id = String;
    fn id(&self) -> String {
        self.id.clone()
    }
    fn session_auth_hash(&self) -> &[u8] {
        &self.auth_hash
    }
}

pub enum Credentials {
    Password {
        name: String,
        password: String,
        ip: String,
    },
    /// An identity the provider vouched for, found by issuer and subject or made on first sight.
    Identity(OidcIdentity),
}

/// `admin_claim` is None when no role claim is configured.
#[derive(Debug, Clone)]
pub struct OidcIdentity {
    pub issuer: String,
    pub subject: String,
    pub name: String,
    pub admin_claim: Option<bool>,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("hashing: {0}")]
    Hash(String),
}

#[derive(Clone)]
pub struct Backend {
    pub store: Store,
    pub cfg: Arc<Config>,
    dummy: Arc<String>,
}

impl Backend {
    pub fn new(store: Store, cfg: Arc<Config>) -> Self {
        Self {
            store,
            cfg,
            dummy: Arc::new(password_auth::generate_hash("nobody has this password")),
        }
    }
}

/// The row for a name, made on first sight.
pub async fn user_on_sight(
    store: &Store,
    cfg: &Config,
    name: &str,
    admin_claim: Option<bool>,
) -> Result<UserRow, sqlx::Error> {
    let listed = cfg.auth.admins.iter().any(|a| a.eq_ignore_ascii_case(name));
    match store.user_by_name(name).await? {
        Some(u) => {
            let wants_admin = listed || admin_claim == Some(true);
            if wants_admin && !u.is_admin() {
                store.set_role(&u.id, "admin").await?;
                return Ok(UserRow {
                    role: "admin".into(),
                    ..u
                });
            }
            Ok(u)
        }
        None => {
            let first = store.user_count().await? == 0;
            let role = if listed || first || admin_claim == Some(true) {
                "admin"
            } else {
                "member"
            };
            let u = UserRow::new(name, role, None);
            store.insert_user(&u).await?;
            tracing::info!(user = %u.name, role, "user made on first sight");
            Ok(u)
        }
    }
}

fn listed(cfg: &Config, name: &str) -> bool {
    cfg.auth.admins.iter().any(|a| a.eq_ignore_ascii_case(name))
}

/// The row for an identity, by issuer and subject, never by name. With a role claim the role
/// follows it on every sign-in, both ways, but the last admin stays an admin.
pub async fn user_for_identity(
    store: &Store,
    cfg: &Config,
    id: &OidcIdentity,
) -> Result<UserRow, sqlx::Error> {
    match store.user_by_oidc(&id.issuer, &id.subject).await? {
        Some(u) => {
            let Some(claim) = id.admin_claim else {
                return Ok(u);
            };
            let wants_admin = claim || listed(cfg, &u.name);
            if wants_admin == u.is_admin() {
                return Ok(u);
            }
            if !wants_admin && u.disabled_at.is_none() && store.admin_count().await? <= 1 {
                tracing::warn!(user = %u.name, "the provider no longer calls the last admin an admin; the role stays");
                return Ok(u);
            }
            let role = if wants_admin { "admin" } else { "member" };
            store.set_role(&u.id, role).await?;
            tracing::info!(user = %u.name, role, "role follows the provider's claim");
            Ok(UserRow {
                role: role.into(),
                ..u
            })
        }
        None => {
            let first = store.user_count().await? == 0;
            let mut row = UserRow::new(&id.name, "member", None);
            row.oidc_issuer = Some(id.issuer.clone());
            row.oidc_subject = Some(id.subject.clone());
            let row = store.insert_user_freely(row).await?;
            let admin = first || id.admin_claim == Some(true) || listed(cfg, &row.name);
            if admin {
                store.set_role(&row.id, "admin").await?;
            }
            let role = if admin { "admin" } else { "member" };
            tracing::info!(user = %row.name, subject = %id.subject, role, "identity seen for the first time");
            Ok(UserRow {
                role: role.into(),
                ..row
            })
        }
    }
}

impl AuthnBackend for Backend {
    type User = SessionUser;
    type Credentials = Credentials;
    type Error = BackendError;

    async fn authenticate(&self, creds: Credentials) -> Result<Option<SessionUser>, BackendError> {
        match creds {
            Credentials::Password { name, password, ip } => {
                let row = self.store.user_by_name(&name).await?;
                let hash = row
                    .as_ref()
                    .filter(|u| u.disabled_at.is_none())
                    .and_then(|u| u.password_hash.clone());
                let dummy = self.dummy.clone();
                let ok = tokio::task::spawn_blocking(move || match hash {
                    Some(h) => password_auth::verify_password(&password, &h).is_ok(),
                    None => {
                        let _ = password_auth::verify_password(&password, &dummy);
                        false
                    }
                })
                .await
                .map_err(|e| BackendError::Hash(e.to_string()))?;
                self.store.add_signin_attempt(&ip, &name, ok).await?;
                if !ok {
                    tracing::warn!(ip, name, "sign-in refused");
                }
                Ok(if ok {
                    row.as_ref().map(SessionUser::from)
                } else {
                    None
                })
            }
            Credentials::Identity(identity) => {
                let row = user_for_identity(&self.store, &self.cfg, &identity).await?;
                Ok(if row.disabled_at.is_some() {
                    None
                } else {
                    Some(SessionUser::from(&row))
                })
            }
        }
    }

    async fn get_user(&self, id: &UserId<Self>) -> Result<Option<SessionUser>, BackendError> {
        Ok(self
            .store
            .user(id)
            .await?
            .filter(|u| u.disabled_at.is_none())
            .map(|u| SessionUser::from(&u)))
    }
}

pub fn hash_password(password: &str) -> String {
    password_auth::generate_hash(password)
}

/// Keeps only the SHA-256 of the session id, so a copy of the database signs nobody in.
#[derive(Debug, Clone)]
pub struct HashedIdStore<S: SessionStore>(pub S);

fn hashed(id: &Id) -> Id {
    let digest = Sha256::digest(id.0.to_le_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    Id(i128::from_le_bytes(bytes))
}

#[async_trait::async_trait]
impl<S: SessionStore> SessionStore for HashedIdStore<S> {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        let mut inner = record.clone();
        inner.id = hashed(&record.id);
        self.0.create(&mut inner).await?;
        if inner.id != hashed(&record.id) {
            return Err(session_store::Error::Backend(
                "the store changed a session id it was given hashed".into(),
            ));
        }
        Ok(())
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let mut inner = record.clone();
        inner.id = hashed(&record.id);
        self.0.save(&inner).await
    }

    async fn load(&self, id: &Id) -> session_store::Result<Option<Record>> {
        Ok(self.0.load(&hashed(id)).await?.map(|mut r| {
            r.id = *id;
            r
        }))
    }

    async fn delete(&self, id: &Id) -> session_store::Result<()> {
        self.0.delete(&hashed(id)).await
    }
}

#[async_trait::async_trait]
impl<S: ExpiredDeletion> ExpiredDeletion for HashedIdStore<S> {
    async fn delete_expired(&self) -> session_store::Result<()> {
        self.0.delete_expired().await
    }
}

/// Keys sign-in attempts by the client behind a trusted proxy, by the peer for anyone else.
#[derive(Clone)]
pub struct ClientKey {
    pub cfg: Arc<Config>,
}

impl KeyExtractor for ClientKey {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<IpAddr, GovernorError> {
        let peer = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip())
            .ok_or(GovernorError::UnableToExtractKey)?;
        Ok(crate::auth::client_ip(req.headers(), peer, &self.cfg))
    }
}

/// The per-account limit: `SIGNIN_BURST` tries on one name, then one more every `SIGNIN_PERIOD`.
pub struct NameLimiter {
    limiter: RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>,
    clock: DefaultClock,
}

impl Default for NameLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl NameLimiter {
    pub fn new() -> Self {
        let quota = Quota::with_period(SIGNIN_PERIOD)
            .expect("a period")
            .allow_burst(NonZeroU32::new(SIGNIN_BURST).expect("a burst"));
        Self {
            limiter: RateLimiter::keyed(quota),
            clock: DefaultClock::default(),
        }
    }

    /// Ok to try, or how many seconds until the next try on that name.
    pub fn check(&self, name: &str) -> Result<(), u64> {
        let key = name.trim().to_lowercase();
        self.limiter
            .check_key(&key)
            .map_err(|not_until| not_until.wait_time_from(self.clock.now()).as_secs().max(1))
    }
}

/// Non standard claims by name, so the role claim can be whatever the provider calls it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExtraClaims(pub serde_json::Map<String, serde_json::Value>);

impl AdditionalClaims for ExtraClaims {}

pub type ReelIdTokenFields = IdTokenFields<
    ExtraClaims,
    EmptyExtraTokenFields,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJwsSigningAlgorithm,
>;

pub type ReelTokenResponse = StandardTokenResponse<ReelIdTokenFields, CoreTokenType>;

pub type ReelClient = Client<
    ExtraClaims,
    CoreAuthDisplay,
    CoreGenderClaim,
    CoreJweContentEncryptionAlgorithm,
    CoreJsonWebKey,
    CoreAuthPrompt,
    StandardErrorResponse<CoreErrorResponseType>,
    ReelTokenResponse,
    CoreTokenIntrospectionResponse,
    CoreRevocableToken,
    CoreRevocationErrorResponse,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// What the session holds between the redirect to the provider and its callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OidcPending {
    pub csrf: String,
    pub nonce: String,
    pub pkce: String,
}

pub struct Oidc {
    pub client: ReelClient,
    pub http: openidconnect::reqwest::Client,
    pub issuer: String,
    pub scopes: Vec<String>,
    pub username_claim: String,
    pub role_claim: String,
    pub admin_value: String,
}

pub const OIDC_SECRET_ENV: &str = "REEL_OIDC_CLIENT_SECRET";
pub const OIDC_SECRET_FILE_ENV: &str = "REEL_OIDC_CLIENT_SECRET_FILE";

pub fn oidc_secret() -> anyhow::Result<Option<String>> {
    if let Ok(v) = std::env::var(OIDC_SECRET_ENV) {
        if !v.trim().is_empty() {
            return Ok(Some(v.trim().to_string()));
        }
    }
    if let Ok(path) = std::env::var(OIDC_SECRET_FILE_ENV) {
        if !path.trim().is_empty() {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("{OIDC_SECRET_FILE_ENV} {path}: {e}"))?;
            return Ok(Some(text.trim().to_string()));
        }
    }
    Ok(None)
}

impl Oidc {
    pub async fn from_config(cfg: &Config) -> anyhow::Result<Oidc> {
        let http = openidconnect::reqwest::ClientBuilder::new()
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()?;
        let issuer = IssuerUrl::new(cfg.auth.oidc_issuer.clone())?;
        let metadata = CoreProviderMetadata::discover_async(issuer.clone(), &http)
            .await
            .map_err(|e| anyhow::anyhow!("discovery at {}: {e}", cfg.auth.oidc_issuer))?;
        let secret = oidc_secret()?.map(ClientSecret::new);
        let client = ReelClient::from_provider_metadata(
            metadata,
            ClientId::new(cfg.auth.oidc_client_id.clone()),
            secret,
        )
        .set_redirect_uri(RedirectUrl::new(format!(
            "{}/oidc/callback",
            cfg.public_url.trim_end_matches('/')
        ))?);
        Ok(Oidc {
            client,
            http,
            issuer: cfg.auth.oidc_issuer.clone(),
            scopes: cfg
                .auth
                .oidc_scopes
                .split_whitespace()
                .map(String::from)
                .collect(),
            username_claim: cfg.auth.oidc_username_claim.clone(),
            role_claim: cfg.auth.oidc_role_claim.clone(),
            admin_value: cfg.auth.oidc_admin_value.clone(),
        })
    }

    pub fn host(&self) -> String {
        url::Url::parse(&self.issuer)
            .ok()
            .and_then(|u| u.host_str().map(String::from))
            .unwrap_or_else(|| self.issuer.clone())
    }

    /// Where to send the browser, and what to remember until it comes back.
    pub fn start(&self) -> (String, OidcPending) {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut req = self.client.authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        );
        for s in &self.scopes {
            if s != "openid" {
                req = req.add_scope(Scope::new(s.clone()));
            }
        }
        let (url, csrf, nonce) = req.set_pkce_challenge(challenge).url();
        (
            url.to_string(),
            OidcPending {
                csrf: csrf.secret().clone(),
                nonce: nonce.secret().clone(),
                pkce: verifier.secret().clone(),
            },
        )
    }

    /// The callback's code becomes a verified identity; an email names it only when verified.
    pub async fn finish(&self, code: &str, pending: &OidcPending) -> anyhow::Result<OidcIdentity> {
        let tokens = self
            .client
            .exchange_code(AuthorizationCode::new(code.to_string()))?
            .set_pkce_verifier(PkceCodeVerifier::new(pending.pkce.clone()))
            .request_async(&self.http)
            .await
            .map_err(|e| anyhow::anyhow!("the token exchange failed: {e}"))?;
        let id_token = tokens
            .id_token()
            .ok_or_else(|| anyhow::anyhow!("the provider returned no id token"))?;
        let verifier = self.client.id_token_verifier();
        let claims = id_token
            .claims(&verifier, &Nonce::new(pending.nonce.clone()))
            .map_err(|e| anyhow::anyhow!("the id token did not verify: {e}"))?;
        let extra = &claims.additional_claims().0;
        let name = match self.username_claim.as_str() {
            "preferred_username" | "" => claims.preferred_username().map(|u| u.to_string()),
            "email" => claims
                .email()
                .filter(|_| claims.email_verified() == Some(true))
                .map(|e| e.to_string()),
            "name" => claims
                .name()
                .and_then(|n| n.get(None))
                .map(|n| n.to_string()),
            other => extra.get(other).and_then(|v| v.as_str().map(String::from)),
        }
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| claims.subject().to_string());
        let admin_claim = if self.role_claim.is_empty() {
            None
        } else {
            let value = extra.get(&self.role_claim);
            Some(match value {
                Some(serde_json::Value::String(s)) => s == &self.admin_value,
                Some(serde_json::Value::Array(items)) => {
                    items.iter().any(|i| i.as_str() == Some(&self.admin_value))
                }
                _ => false,
            })
        };
        Ok(OidcIdentity {
            issuer: claims.issuer().to_string(),
            subject: claims.subject().to_string(),
            name: name.chars().take(40).collect::<String>().trim().to_string(),
            admin_claim,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_sessions::session::Record;
    use tower_sessions::MemoryStore;

    #[tokio::test]
    async fn the_hashed_store_keeps_nothing_but_the_hash_and_round_trips() {
        let inner = MemoryStore::default();
        let store = HashedIdStore(inner.clone());
        let mut record = Record {
            id: Id::default(),
            data: Default::default(),
            expiry_date: time::OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        let clear = record.id;
        store.create(&mut record).await.unwrap();
        assert_eq!(record.id, clear, "the cookie id stays what the layer made");
        assert!(
            inner.load(&clear).await.unwrap().is_none(),
            "the inner store has no row under the clear id"
        );
        assert!(inner.load(&hashed(&clear)).await.unwrap().is_some());
        let loaded = store.load(&clear).await.unwrap().unwrap();
        assert_eq!(loaded.id, clear);
        store.delete(&clear).await.unwrap();
        assert!(store.load(&clear).await.unwrap().is_none());
    }

    #[test]
    fn the_name_limiter_allows_the_burst_then_waits() {
        let l = NameLimiter::new();
        for _ in 0..SIGNIN_BURST {
            assert!(l.check("Mira").is_ok());
        }
        let wait = l.check("mira").unwrap_err();
        assert!(wait >= 1, "{wait}");
        assert!(
            l.check("someone else").is_ok(),
            "another name has its own count"
        );
    }
}
