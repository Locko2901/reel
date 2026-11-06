use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub site_name: String,
    pub listen: String,
    pub public_url: String,
    pub data_dir: PathBuf,
    /// The clips themselves and the trash; empty is `library` in the data folder.
    pub library_dir: PathBuf,
    pub timezone: String,
    pub auth: AuthConfig,
    pub ingest: IngestConfig,
    pub media: MediaConfig,
    pub pipeline: PipelineConfig,
    pub share: ShareConfig,
    pub storage: StorageConfig,
    pub import: ImportConfig,
    pub notify: NotifyConfig,
    pub appearance: AppearanceConfig,
    pub advanced: AdvancedConfig,
    pub device: DeviceConfig,
    pub edit: EditConfig,
    pub external: ExternalConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EditConfig {
    /// The editor's Keep the last buttons, in seconds.
    pub quick_trims: Vec<u32>,
}

impl Default for EditConfig {
    fn default() -> Self {
        Self {
            quick_trims: vec![15, 30, 60],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub dev_user: String,
    pub trusted_proxies: Vec<String>,
    pub admins: Vec<String>,
    pub session_days: u64,
    pub session_max_days: u64,
    pub oidc_issuer: String,
    pub oidc_name: String,
    pub oidc_client_id: String,
    pub oidc_scopes: String,
    pub oidc_username_claim: String,
    pub oidc_role_claim: String,
    pub oidc_admin_value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    Proxy,
    Local,
    #[serde(alias = "dev")]
    Single,
    Oidc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IngestConfig {
    pub incoming_dir: PathBuf,
    pub originals_dir: PathBuf,
    pub poll_seconds: u64,
    pub stable_polls: u32,
    pub keep_originals: KeepOriginals,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeepOriginals {
    Reencoded,
    All,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaConfig {
    pub ffmpeg: String,
    pub ffprobe: String,
    pub hwaccel: Hwaccel,
    pub render_device: String,
    pub copy_max_height: u32,
    pub copy_max_mbit: u32,
    pub encode_mbit: u32,
    pub normalize_audio: bool,
    pub audio_kbit: u32,
    /// What Normalise loudness levels to, in LUFS: the ingest's and the editor's.
    pub loudness_target: i32,
    /// Every new clip gets a version levelled to the target, made current, the original kept.
    pub normalize_new: bool,
    pub copy_audio: String,
    pub x264_preset: String,
    /// Size-limited copies on the GPU encoder instead of two-pass x264: faster, a little larger.
    pub copies_on_hardware: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Hwaccel {
    Auto,
    Qsv,
    Vaapi,
    Nvenc,
    Amf,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    pub workers: usize,
    pub job_timeout_minutes: u64,
    pub nice: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShareConfig {
    pub discord_limit_mb: u64,
    pub discord_limits_mb: Vec<u64>,
    pub trash_days: u64,
    pub discord_cache_gb: u64,
    pub download_default: bool,
    pub expiry_default: String,
    pub embed_mb: u64,
    /// Preview videos for the links; off in standalone mode, where no link embeds anywhere.
    pub previews: bool,
    /// The bitrate of the web copy a link's page plays, video and audio together.
    pub web_mbit: u64,
    pub web_max_height: u32,
    /// Revoke the links never posted nor opened, `cleanup_days` after they were made.
    pub cleanup: bool,
    pub cleanup_days: u64,
}

/// Link previews from an S3-compatible storage. Its Debug never shows the secret.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExternalConfig {
    pub on: bool,
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub public_url: String,
    /// The preview video a link served from the storage gets, larger than the server's.
    pub embed_mb: u64,
    /// The most reel keeps there, from its own records.
    pub limit_gb: u64,
    /// Previews that go up before a link exists: off, edited, edited_new.
    pub ahead: String,
    pub new_days: u64,
    /// A link younger than this keeps its preview there whatever the order.
    pub protect_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub disk_warn_gb: u64,
    /// The oldest clips go to the trash first to keep the library under this; 0 is no cap.
    pub cap_gb: u64,
    /// The cap never trims a clip that is not on a server too.
    pub cap_keeps_unsent: bool,
    /// The app's standalone library, where a clip can be on no server; never a setting.
    #[serde(skip)]
    pub standalone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportConfig {
    pub upload_max_gb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifyConfig {
    pub kind: String,
    pub ntfy_url: String,
    pub ntfy_topic: String,
    pub ntfy_token: String,
    pub webhook_url: String,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeviceConfig {
    pub chunk_mb: u64,
    pub upload_expiry_hours: u64,
    pub app_updates: bool,
    pub updates_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedConfig {
    pub log_level: String,
    pub update_check: bool,
    pub update_repo: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            site_name: "reel".into(),
            listen: "0.0.0.0:8080".into(),
            public_url: "http://localhost:8080".into(),
            data_dir: PathBuf::from("/data"),
            library_dir: PathBuf::new(),
            timezone: String::new(),
            auth: AuthConfig::default(),
            ingest: IngestConfig::default(),
            media: MediaConfig::default(),
            pipeline: PipelineConfig::default(),
            share: ShareConfig::default(),
            storage: StorageConfig::default(),
            import: ImportConfig::default(),
            notify: NotifyConfig::default(),
            appearance: AppearanceConfig::default(),
            advanced: AdvancedConfig::default(),
            device: DeviceConfig::default(),
            edit: EditConfig::default(),
            external: ExternalConfig::default(),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: AuthMode::Local,
            dev_user: "you".into(),
            trusted_proxies: vec![],
            admins: vec![],
            session_days: 30,
            session_max_days: 90,
            oidc_issuer: String::new(),
            oidc_name: String::new(),
            oidc_client_id: String::new(),
            oidc_scopes: "openid profile email".into(),
            oidc_username_claim: "preferred_username".into(),
            oidc_role_claim: String::new(),
            oidc_admin_value: "admin".into(),
        }
    }
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            incoming_dir: PathBuf::from("/incoming"),
            originals_dir: PathBuf::from("/originals"),
            poll_seconds: 5,
            stable_polls: 2,
            keep_originals: KeepOriginals::Reencoded,
            extensions: ["mkv", "mp4", "mov", "webm", "ts", "avi"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            hwaccel: Hwaccel::Auto,
            render_device: "/dev/dri/renderD128".into(),
            copy_max_height: 1440,
            copy_max_mbit: 50,
            encode_mbit: 10,
            normalize_audio: false,
            audio_kbit: 320,
            loudness_target: -16,
            normalize_new: false,
            copy_audio: "copy".into(),
            x264_preset: "slow".into(),
            copies_on_hardware: false,
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            workers: 1,
            job_timeout_minutes: 60,
            nice: 10,
        }
    }
}

impl Default for ShareConfig {
    fn default() -> Self {
        Self {
            discord_limit_mb: 20,
            discord_limits_mb: vec![10, 20, 50, 100, 500],
            trash_days: 7,
            discord_cache_gb: 5,
            download_default: true,
            expiry_default: "never".into(),
            embed_mb: 20,
            previews: true,
            web_mbit: 10,
            web_max_height: 1440,
            cleanup: false,
            cleanup_days: 30,
        }
    }
}

impl std::fmt::Debug for ExternalConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalConfig")
            .field("on", &self.on)
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("bucket", &self.bucket)
            .field("access_key", &self.access_key)
            .field("secret_key", &redacted(&self.secret_key))
            .field("public_url", &self.public_url)
            .field("embed_mb", &self.embed_mb)
            .field("limit_gb", &self.limit_gb)
            .field("ahead", &self.ahead)
            .field("new_days", &self.new_days)
            .field("protect_days", &self.protect_days)
            .finish()
    }
}

/// A secret as Debug shows it: whether it is set, never its value.
pub fn redacted(secret: &str) -> &'static str {
    if secret.is_empty() {
        "not set"
    } else {
        "set, not shown"
    }
}

impl Default for ExternalConfig {
    fn default() -> Self {
        Self {
            on: false,
            endpoint: String::new(),
            region: "auto".into(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            public_url: String::new(),
            embed_mb: 40,
            limit_gb: 5,
            ahead: "off".into(),
            new_days: 7,
            protect_days: 14,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            disk_warn_gb: 20,
            cap_gb: 0,
            cap_keeps_unsent: true,
            standalone: false,
        }
    }
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self { upload_max_gb: 8 }
    }
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            kind: "off".into(),
            ntfy_url: String::new(),
            ntfy_topic: String::new(),
            ntfy_token: String::new(),
            webhook_url: String::new(),
            events: [
                "job_failed",
                "file_unreadable",
                "disk_low",
                "storage",
                "library_cap",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
        }
    }
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            chunk_mb: 32,
            upload_expiry_hours: 24,
            app_updates: true,
            updates_token: String::new(),
        }
    }
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            log_level: "info".into(),
            update_check: true,
            update_repo: "Locko2901/reel".into(),
        }
    }
}

pub const DEVICE_MAX_CHUNK_MB: u64 = 90;

/// The trash's folder in the library and the file that tells the library is the right folder;
/// a clip's folder is a ULID, never one of these.
pub const TRASH: &str = ".trash";
pub const MARKER: &str = ".reel-library";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {0}: {1}")]
    Read(PathBuf, std::io::Error),
    #[error("{0}: {1}")]
    Parse(PathBuf, toml::de::Error),
    #[error("{0}")]
    Invalid(String),
}

impl Config {
    /// The config without the database layer: the defaults, then config.toml, then the environment.
    pub fn load() -> Result<Self, ConfigError> {
        let layers = crate::settings::load_layers()?;
        let eff = crate::settings::build(
            &layers.base,
            layers.file.as_ref(),
            &[],
            &crate::settings::process_env,
        )?;
        Ok((*eff.config).clone())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let invalid = |m: String| Err(ConfigError::Invalid(m));
        if self.pipeline.workers == 0 {
            return invalid("pipeline.workers must be at least 1".into());
        }
        if self.site_name.trim().is_empty() || self.site_name.chars().count() > 40 {
            return invalid("site_name: 1 to 40 characters".into());
        }
        if !(self.public_url.starts_with("http://") || self.public_url.starts_with("https://")) {
            return invalid("public_url must start with http:// or https://".into());
        }
        if !self.timezone.is_empty() && jiff::tz::TimeZone::get(&self.timezone).is_err() {
            return invalid(format!(
                "timezone: {} is not a time zone name",
                self.timezone
            ));
        }
        if !self
            .share
            .discord_limits_mb
            .contains(&self.share.discord_limit_mb)
        {
            return invalid(format!(
                "share.discord_limit_mb must be one of {:?}",
                self.share.discord_limits_mb
            ));
        }
        if !["never", "1d", "7d", "30d"].contains(&self.share.expiry_default.as_str()) {
            return invalid("share.expiry_default: never, 1d, 7d or 30d".into());
        }
        if !["off", "ntfy", "webhook"].contains(&self.notify.kind.as_str()) {
            return invalid("notify.kind: off, ntfy or webhook".into());
        }
        if self.notify.kind == "ntfy"
            && (self.notify.ntfy_url.is_empty() || self.notify.ntfy_topic.is_empty())
        {
            return invalid("notify: ntfy needs a server URL and a topic".into());
        }
        if self.notify.kind == "webhook" && self.notify.webhook_url.is_empty() {
            return invalid("notify: a webhook needs a URL".into());
        }
        for u in [&self.notify.ntfy_url, &self.notify.webhook_url] {
            if !(u.is_empty() || u.starts_with("http://") || u.starts_with("https://")) {
                return invalid(format!("{u}: a URL starts with http:// or https://"));
            }
        }
        if self.appearance.theme != "dark" {
            return invalid("appearance.theme: dark is the only theme so far".into());
        }
        if !["error", "warn", "info", "debug"].contains(&self.advanced.log_level.as_str()) {
            return invalid("advanced.log_level: error, warn, info or debug".into());
        }
        if self.auth.mode == AuthMode::Proxy && self.auth.trusted_proxies.is_empty() {
            return Err(ConfigError::Invalid("auth.mode is proxy but auth.trusted_proxies is empty: name the proxy's address (REEL_TRUSTED_PROXIES), nothing is trusted by default".into()));
        }
        if self.auth.mode == AuthMode::Single && !self.listens_on_loopback() {
            return Err(ConfigError::Invalid(format!(
                "auth.mode is single (no sign-in) but reel listens on {}: single mode is only for a loopback address such as 127.0.0.1; use local (built-in accounts) or proxy for anything reachable from the network",
                self.listen
            )));
        }
        if self.auth.mode == AuthMode::Oidc
            && (self.auth.oidc_issuer.is_empty() || self.auth.oidc_client_id.is_empty())
        {
            return Err(ConfigError::Invalid(
                "auth.mode is oidc: set auth.oidc_issuer and auth.oidc_client_id (and the secret in REEL_OIDC_CLIENT_SECRET or REEL_OIDC_CLIENT_SECRET_FILE)".into(),
            ));
        }
        if !self.auth.oidc_role_claim.trim().is_empty()
            && self.auth.oidc_admin_value.trim().is_empty()
        {
            return Err(ConfigError::Invalid(
                "auth.oidc_admin_value: a role claim needs the value in it that makes an admin, or every empty claim would make one; empty the role claim to go without".into(),
            ));
        }
        if self.auth.session_max_days > 0 && self.auth.session_max_days < self.auth.session_days {
            return Err(ConfigError::Invalid(
                "auth.session_max_days must not be shorter than auth.session_days".into(),
            ));
        }
        if self.library_elsewhere() {
            let library = self.library_dir();
            if library == self.data_dir || self.data_dir.starts_with(&library) {
                return invalid(format!(
                    "library_dir: {} holds the data folder; the library is a folder of its own",
                    library.display()
                ));
            }
            if library.starts_with(&self.ingest.incoming_dir) {
                return invalid(format!(
                    "library_dir: {} is in the incoming folder",
                    library.display()
                ));
            }
        }
        for cidr in &self.auth.trusted_proxies {
            cidr.parse::<ipnet::IpNet>().map_err(|_| {
                ConfigError::Invalid(format!("auth.trusted_proxies: {cidr} is not a CIDR"))
            })?;
        }
        Ok(())
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("reel.db")
    }
    pub fn library_dir(&self) -> PathBuf {
        if self.library_dir.as_os_str().is_empty() {
            self.data_dir.join("library")
        } else {
            self.library_dir.clone()
        }
    }
    pub fn library_elsewhere(&self) -> bool {
        self.library_dir() != self.data_dir.join("library")
    }
    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache").join("discord")
    }
    pub fn web_cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache").join("web")
    }
    /// In the library, so a clip goes to the trash and back by a rename on every setup.
    pub fn trash_dir(&self) -> PathBuf {
        self.library_dir().join(TRASH)
    }
    /// A trash outside the library, emptied into the library's at start.
    pub fn old_trash_dir(&self) -> PathBuf {
        self.data_dir.join("trash")
    }
    pub fn library_marker(&self) -> PathBuf {
        self.library_dir().join(MARKER)
    }
    pub fn uploads_dir(&self) -> PathBuf {
        self.data_dir.join("uploads")
    }
    pub fn failed_dir(&self) -> PathBuf {
        self.ingest.incoming_dir.join("failed")
    }
    pub fn clip_dir(&self, clip_id: &str) -> PathBuf {
        self.library_dir().join(clip_id)
    }
    /// A path the database holds: relative to the data folder, `library/...` in the library
    /// wherever it is.
    pub fn abs(&self, rel: &str) -> PathBuf {
        match rel.strip_prefix("library/") {
            Some(rest) => self.library_dir().join(rest),
            None => self.data_dir.join(rel),
        }
    }

    /// How the database keeps a file's path: relative to the data folder when it is in it, so the
    /// folder can move, else as it is.
    pub fn stored(&self, path: &Path) -> String {
        match path.strip_prefix(&self.data_dir) {
            Ok(rel) if !self.data_dir.as_os_str().is_empty() => rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
            _ => path.to_string_lossy().into_owned(),
        }
    }

    /// The data folder's own directories; the library is made by its check, never here, so a
    /// share that is not mounted does not get one on its bare mount point.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for d in [self.cache_dir(), self.uploads_dir()] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }

    /// True when the listen address is a loopback one, where no sign-in is defensible.
    pub fn listens_on_loopback(&self) -> bool {
        let host = self
            .listen
            .rsplit_once(':')
            .map(|(h, _)| h)
            .unwrap_or(&self.listen)
            .trim_matches(|c| c == '[' || c == ']');
        host == "localhost"
            || host
                .parse::<IpAddr>()
                .map(|ip| ip.is_loopback())
                .unwrap_or(false)
    }

    pub fn is_trusted_proxy(&self, ip: IpAddr) -> bool {
        self.auth.trusted_proxies.iter().any(|c| {
            c.parse::<ipnet::IpNet>()
                .map(|n| n.contains(&ip))
                .unwrap_or(false)
        })
    }

    pub fn accepts_extension(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| {
                self.ingest
                    .extensions
                    .iter()
                    .any(|x| x.eq_ignore_ascii_case(e))
            })
            .unwrap_or(false)
    }

    /// The size a version has to be under to be sent to Discord as it is; the preview copy
    /// aims under it too.
    pub fn embed_bytes(&self) -> u64 {
        self.share.embed_mb * 1_000_000
    }
    /// The desktop app's standalone library: on the loopback, for the one person at the PC.
    pub fn standalone(&self) -> bool {
        self.storage.standalone
    }

    /// The rules the desktop app's standalone mode runs reel by: one user, the gaming PC's
    /// priorities, nothing for a server.
    pub fn make_standalone(&mut self) {
        self.auth.mode = AuthMode::Single;
        self.auth.admins = vec![self.auth.dev_user.clone()];
        self.pipeline.workers = 1;
        self.pipeline.nice = 10;
        self.share.previews = false;
        self.device.app_updates = false;
        // nothing leaves the PC by itself (docs/privacy.md): updates are checked only when asked
        self.advanced.update_check = false;
        self.notify.kind = "off".into();
        self.external.on = false;
        self.storage.standalone = true;
    }

    /// Whether link previews go to the external storage: on, and previews made at all.
    pub fn external_on(&self) -> bool {
        self.external.on && self.share.previews
    }
    pub fn external_embed_bytes(&self) -> u64 {
        self.external.embed_mb * 1_000_000
    }
    pub fn external_limit_bytes(&self) -> u64 {
        self.external.limit_gb * 1_000_000_000
    }
    /// The chunk a paired app sends at a time, and the most the server takes in one request:
    /// 90 MiB passes Cloudflare's 100 MB request limit on the WAN path.
    pub fn device_chunk_bytes(&self) -> u64 {
        self.device.chunk_mb.clamp(1, DEVICE_MAX_CHUNK_MB) * 1024 * 1024
    }
    pub fn device_max_chunk_bytes(&self) -> u64 {
        DEVICE_MAX_CHUNK_MB * 1024 * 1024
    }
    pub fn device_uploads_dir(&self) -> PathBuf {
        self.uploads_dir().join("device")
    }
    /// Uploads from the browser in pieces, one folder each.
    pub fn web_uploads_dir(&self) -> PathBuf {
        self.uploads_dir().join("web")
    }
    pub fn upload_max_bytes(&self) -> usize {
        (self.import.upload_max_gb.max(1) as usize) * 1024 * 1024 * 1024
    }
    pub fn notifies(&self, event: &str) -> bool {
        self.notify.kind != "off" && self.notify.events.iter().any(|e| e == event)
    }

    /// The server's folders replaced by their names in a text shown or sent outside.
    pub fn hide_paths(&self, text: &str) -> String {
        let mut out = text.to_string();
        let library = if self.library_elsewhere() {
            self.library_dir()
        } else {
            PathBuf::new()
        };
        for (dir, name) in [
            (self.uploads_dir(), "uploads"),
            (self.ingest.incoming_dir.clone(), "incoming"),
            (self.ingest.originals_dir.clone(), "originals"),
            (library, "library"),
            (self.data_dir.clone(), "data"),
        ] {
            let shown = dir.display().to_string();
            if !shown.is_empty() && shown != "/" {
                out = out.replace(&shown, name);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nothing_set_the_library_and_its_paths_are_where_they_were() {
        let cfg = Config::default();
        assert!(!cfg.library_elsewhere());
        assert_eq!(cfg.library_dir(), PathBuf::from("/data/library"));
        assert_eq!(cfg.clip_dir("C1"), PathBuf::from("/data/library/C1"));
        assert_eq!(
            cfg.abs("library/C1/master.mp4"),
            PathBuf::from("/data/library/C1/master.mp4")
        );
        assert_eq!(
            cfg.abs("cache/discord/x.mp4"),
            PathBuf::from("/data/cache/discord/x.mp4")
        );
        assert_eq!(cfg.trash_dir(), PathBuf::from("/data/library/.trash"));
        assert_eq!(
            cfg.hide_paths("/data/library/C1/master.mp4: gone"),
            "data/library/C1/master.mp4: gone"
        );
    }

    #[test]
    fn a_library_elsewhere_takes_the_paths_the_database_holds() {
        let cfg = Config {
            library_dir: PathBuf::from("/mnt/nas/reel"),
            ..Config::default()
        };
        assert!(cfg.library_elsewhere());
        assert_eq!(
            cfg.abs("library/C1/v2/cut.mp4"),
            PathBuf::from("/mnt/nas/reel/C1/v2/cut.mp4")
        );
        assert_eq!(
            cfg.abs("cache/web/C1.mp4"),
            PathBuf::from("/data/cache/web/C1.mp4"),
            "the caches stay in the data folder"
        );
        assert_eq!(cfg.db_path(), PathBuf::from("/data/reel.db"));
        assert_eq!(cfg.trash_dir(), PathBuf::from("/mnt/nas/reel/.trash"));
        assert_eq!(
            cfg.library_marker(),
            PathBuf::from("/mnt/nas/reel/.reel-library")
        );
        assert_eq!(
            cfg.hide_paths("/mnt/nas/reel/C1/master.mp4 and /data/reel.db"),
            "library/C1/master.mp4 and data/reel.db"
        );
        assert!(cfg.validate().is_ok());
        let same = Config {
            library_dir: PathBuf::from("/data/library"),
            ..Config::default()
        };
        assert!(
            !same.library_elsewhere(),
            "its own default is not elsewhere"
        );
        for bad in ["/data", "/", "/incoming/lib"] {
            let cfg = Config {
                library_dir: PathBuf::from(bad),
                ..Config::default()
            };
            assert!(cfg.validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn a_role_claim_needs_an_admin_value() {
        let mut cfg = Config::default();
        cfg.auth.oidc_role_claim = "groups".into();
        assert!(cfg.validate().is_ok(), "the default value is admin");
        cfg.auth.oidc_admin_value = " ".into();
        let e = cfg.validate().unwrap_err().to_string();
        assert!(e.contains("auth.oidc_admin_value"), "{e}");
        cfg.auth.oidc_role_claim = String::new();
        assert!(
            cfg.validate().is_ok(),
            "without a role claim the value is not read"
        );
    }

    #[test]
    fn proxy_mode_needs_a_trusted_proxy() {
        let mut cfg = Config::default();
        cfg.auth.mode = AuthMode::Proxy;
        assert!(
            cfg.validate().is_err(),
            "proxy mode with nothing trusted must not start"
        );
        cfg.auth.trusted_proxies = vec!["192.0.2.10/32".into()];
        assert!(cfg.validate().is_ok());
        assert!(cfg.is_trusted_proxy("192.0.2.10".parse().unwrap()));
        assert!(!cfg.is_trusted_proxy("198.51.100.55".parse().unwrap()));
        let mut single = Config::default();
        single.auth.mode = AuthMode::Single;
        assert!(
            single.validate().is_err(),
            "single mode on 0.0.0.0 is refused: no sign-in on the network"
        );
        single.listen = "127.0.0.1:8080".into();
        assert!(
            single.validate().is_ok(),
            "single mode on the loopback is fine"
        );
        single.listen = "localhost:8080".into();
        assert!(single.validate().is_ok());
        single.listen = "[::1]:8080".into();
        assert!(single.validate().is_ok());
        assert_eq!(
            Config::default().auth.mode,
            AuthMode::Local,
            "a new install gets accounts"
        );
        let mut oidc = Config::default();
        oidc.auth.mode = AuthMode::Oidc;
        assert!(
            oidc.validate().is_err(),
            "oidc needs an issuer and a client id"
        );
    }
}
