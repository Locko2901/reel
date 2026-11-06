//! Every setting reel has, and the config built from its layers; the environment always wins.
use crate::config::{AuthMode, Config, ConfigError, Hwaccel, KeepOriginals};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    General,
    Storage,
    Ingest,
    Sharing,
    Imports,
    Notifications,
    Access,
    Devices,
    Appearance,
    Advanced,
}

impl Section {
    pub const ALL: [Section; 10] = [
        Section::General,
        Section::Storage,
        Section::Ingest,
        Section::Sharing,
        Section::Imports,
        Section::Notifications,
        Section::Access,
        Section::Devices,
        Section::Appearance,
        Section::Advanced,
    ];

    pub fn id(&self) -> &'static str {
        match self {
            Section::General => "general",
            Section::Storage => "storage",
            Section::Ingest => "ingest",
            Section::Sharing => "sharing",
            Section::Imports => "imports",
            Section::Notifications => "notifications",
            Section::Access => "access",
            Section::Devices => "devices",
            Section::Appearance => "appearance",
            Section::Advanced => "advanced",
        }
    }

    pub fn parse(id: &str) -> Option<Section> {
        Section::ALL.iter().copied().find(|s| s.id() == id)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Storage => "Library and storage",
            Section::Ingest => "Ingest",
            Section::Sharing => "Sharing",
            Section::Imports => "Imports",
            Section::Notifications => "Notifications",
            Section::Access => "Access",
            Section::Devices => "Devices",
            Section::Appearance => "Appearance",
            Section::Advanced => "Advanced",
        }
    }

    /// Whether the app's standalone library has this section: notifications, access and devices
    /// are a server's.
    pub fn in_standalone(&self) -> bool {
        !matches!(
            self,
            Section::Notifications | Section::Access | Section::Devices
        )
    }

    /// The label in the app's standalone library: Sharing keeps the copies under a size alone.
    pub fn label_in(&self, standalone: bool) -> &'static str {
        match self {
            Section::Sharing if standalone => "Copies under a size",
            _ => self.label(),
        }
    }

    /// The line under the section's title; with `library`, the standalone library's own.
    pub fn line_in(&self, library: Option<&std::path::Path>) -> String {
        let Some(library) = library else {
            return self.line().into();
        };
        match self {
            Section::General => "The clock the dates follow, and the editor's quick trims.".into(),
            Section::Storage => format!("What the library keeps, how long the trash and the copies under a size stay, and what the disk is doing. The library is in {} on this PC; its folder and its cap are under Library folder.", library.display()),
            Section::Ingest => "What is copied and what is re-encoded, and the loudness of new clips. The GPU encoder is under Encoder.".into(),
            Section::Sharing => "The sizes a copy under a size offers, for the apps with an upload limit.".into(),
            Section::Imports => "A folder of clips on this PC to take into the library, and how big an upload may be.".into(),
            Section::Appearance => "The look of the pages, and what this window remembers.".into(),
            Section::Advanced => "The check for new releases, the history of every change, and a copy of the database.".into(),
            _ => self.line().into(),
        }
    }

    pub fn line(&self) -> &'static str {
        match self {
            Section::General => "The name on the pages, the address links carry, the clock the dates follow, and the editor's quick trims.",
            Section::Storage => "Where the library, the incoming clips and the kept originals live, how long the trash and the caches keep things, and what the disk is doing.",
            Section::Ingest => "How the incoming folder is read, what is copied and what is re-encoded, and the workers that do it.",
            Section::Sharing => "The defaults a new link gets, the copies under a size, and where link previews load from.",
            Section::Imports => "A folder to scan and how big an upload may be.",
            Section::Notifications => "Where reel tells you about failures, a full disk and new versions: ntfy or a webhook.",
            Section::Access => "Who gets in and how reel knows who they are. Share links stay public either way.",
            Section::Devices => "The paired apps: a code to pair one, their tokens, what they upload, and the chunks they send.",
            Section::Appearance => "The look of the pages, and what this browser remembers.",
            Section::Advanced => "Logs, the update check, the history of every change, exports and the restart.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Int {
        min: i64,
        max: i64,
        unit: &'static str,
    },
    Bool,
    Text,
    Url,
    Path,
    List,
    Secret,
    Select(&'static [&'static str]),
    /// Any number of the options, ticked
    Set(&'static [&'static str]),
}

impl Kind {
    pub fn name(&self) -> &'static str {
        match self {
            Kind::Int { .. } => "int",
            Kind::Bool => "bool",
            Kind::Text => "text",
            Kind::Url => "url",
            Kind::Path => "path",
            Kind::List => "list",
            Kind::Secret => "secret",
            Kind::Select(_) => "select",
            Kind::Set(_) => "set",
        }
    }
}

/// One setting. `fixed` ones live only in config.toml or the environment, needed before the
/// database opens; one that `depends` on a switch shows only while it is on, its value kept.
#[derive(Debug, Clone, Copy)]
pub struct Def {
    pub key: &'static str,
    pub env: &'static str,
    pub section: Section,
    pub panel: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
    pub restart: bool,
    pub fixed: bool,
    pub depends: Option<&'static str>,
}

const fn def(
    key: &'static str,
    env: &'static str,
    section: Section,
    panel: &'static str,
    label: &'static str,
    help: &'static str,
    kind: Kind,
) -> Def {
    Def {
        key,
        env,
        section,
        panel,
        label,
        help,
        kind,
        restart: false,
        fixed: false,
        depends: None,
    }
}

const fn under(parent: &'static str, d: Def) -> Def {
    Def {
        depends: Some(parent),
        ..d
    }
}

const fn restart(d: Def) -> Def {
    Def { restart: true, ..d }
}

const fn fixed(d: Def) -> Def {
    Def {
        fixed: true,
        restart: true,
        ..d
    }
}

const fn int(min: i64, max: i64, unit: &'static str) -> Kind {
    Kind::Int { min, max, unit }
}

pub const EXPIRIES: &[&str] = &["never", "1d", "7d", "30d"];
pub const EVENTS: &[&str] = &[
    "job_failed",
    "file_unreadable",
    "disk_low",
    "storage",
    "library_cap",
    "update_available",
    "link_made",
];

pub static DEFS: &[Def] = &[
    def("site_name", "REEL_SITE_NAME", Section::General, "Site", "Site name", "on the pages, in the share page's footer and in link previews as the site name", Kind::Text),
    def("public_url", "REEL_PUBLIC_URL", Section::General, "Site", "Public URL", "what every share link starts with; must be reachable from the internet for link previews to play", Kind::Url),
    fixed(def("listen", "REEL_LISTEN", Section::General, "Site", "Listen address", "the address and port the server binds; set when the process starts", Kind::Text)),
    def("timezone", "REEL_TIMEZONE", Section::General, "Site", "Time zone", "an IANA name such as Europe/Berlin for the dates on the pages; empty follows the host's clock", Kind::Text),
    def("edit.quick_trims", "REEL_QUICK_TRIMS", Section::General, "Clip editor", "Quick trims", "the editor's Keep the last buttons, in seconds: each keeps the end of the clip as a new version; 1 to 4 values from 5 to 600", Kind::List),
    fixed(def("data_dir", "REEL_DATA_DIR", Section::Storage, "Folders", "Data folder", "the database and the caches live here, and the library unless the Library folder says otherwise; a fast local disk; set when the process starts", Kind::Path)),
    fixed(def("library_dir", "REEL_LIBRARY_DIR", Section::Storage, "Folders", "Library folder", "the clips themselves, every version with its posters, loops and filmstrips, and the trash: the bulk, which may live on a NAS or a big slow drive; a .reel-library file in it tells reel it is the right folder; set when the process starts", Kind::Path)),
    def("ingest.incoming_dir", "REEL_INCOMING_DIR", Section::Storage, "Folders", "Incoming folder", "raw clips dropped here are picked up, published, then removed; a failed subfolder takes what ffmpeg cannot read", Kind::Path),
    def("ingest.originals_dir", "REEL_ORIGINALS_DIR", Section::Storage, "Folders", "Originals folder", "where the raw file of a re-encoded clip is kept", Kind::Path),
    def("ingest.keep_originals", "REEL_KEEP_ORIGINALS", Section::Storage, "Folders", "Keep originals", "the raw file of re-encoded clips only, of every clip, or of none", Kind::Select(&["reencoded", "all", "none"])),
    def("share.trash_days", "REEL_TRASH_DAYS", Section::Storage, "Trash and caches", "Trash", "a deleted clip stays in the trash this long, with Undo, before it is purged; the trash is .trash in the library folder, so a delete is a rename on any disk", int(1, 365, "days")),
    def("share.discord_cache_gb", "REEL_DISCORD_CACHE_GB", Section::Storage, "Trash and caches", "Copies under a size", "the copies made for downloads under a size, and the animated pictures, are kept up to this much; the oldest go first", int(1, 500, "GB")),
    def("storage.disk_warn_gb", "REEL_DISK_WARN_GB", Section::Storage, "Trash and caches", "Disk warning", "each disk that holds reel's data (the data folder's, the library's, the kept originals') turns amber under this much free space, and Notifications may send; 0 turns it off", int(0, 10_000, "GB free")),
    def("storage.cap_gb", "REEL_LIBRARY_CAP_GB", Section::Storage, "Trash and caches", "Library cap", "housekeeping keeps the library (each clip's versions and its preview video) under this size: the oldest clips go to the trash first, the ones without a live link before the others; a starred clip or one in a collection never goes; 0 is no cap", int(0, 100_000, "GB")),
    def("storage.cap_keeps_unsent", "REEL_LIBRARY_CAP_KEEPS_UNSENT", Section::Storage, "Trash and caches", "The cap spares unsent clips", "the app's standalone library: a clip that is not on a server too is never trimmed by the cap; a server leaves this out, every clip being on it", Kind::Bool),
    def("ingest.poll_seconds", "REEL_POLL_SECONDS", Section::Ingest, "Incoming folder", "Poll every", "how often the folder is read; a file is taken once its size holds still for the stable polls", int(1, 3600, "s")),
    def("ingest.stable_polls", "REEL_STABLE_POLLS", Section::Ingest, "Incoming folder", "Stable polls", "polls a file must keep the same size before it is read, for slow copies over the network", int(1, 20, "polls")),
    def("ingest.extensions", "REEL_EXTENSIONS", Section::Ingest, "Incoming folder", "Extensions", "file endings the watcher and the upload take; anything else is ignored", Kind::List),
    def("media.copy_max_height", "REEL_COPY_MAX_HEIGHT", Section::Ingest, "Copy or re-encode", "Copy video up to", "H.264 at most this tall is copied untouched, never re-encoded", int(480, 4320, "p")),
    def("media.copy_max_mbit", "REEL_COPY_MAX_MBIT", Section::Ingest, "Copy or re-encode", "Copy video under", "and at most this fast; anything above is re-encoded", int(1, 200, "Mbit/s")),
    def("media.encode_mbit", "REEL_ENCODE_MBIT", Section::Ingest, "Copy or re-encode", "Re-encode at", "the video bitrate for everything that is not copied", int(1, 100, "Mbit/s")),
    def("media.audio_kbit", "REEL_AUDIO_KBIT", Section::Ingest, "Copy or re-encode", "Audio", "AAC at this rate when the audio is re-encoded, or the source rate when lower; AAC is copied as it is", int(64, 512, "kbit/s")),
    def("media.normalize_audio", "REEL_NORMALIZE_AUDIO", Section::Ingest, "Copy or re-encode", "Normalise loudness", "forces an audio re-encode for every clip", Kind::Bool),
    def("media.normalize_new", "REEL_NORMALIZE_NEW", Section::Ingest, "Copy or re-encode", "Normalise new clips", "every new clip, from the app, the incoming folder or an upload, gets a version levelled to the loudness target right after the ingest, made current, the video copied and only the audio encoded again; the original stays as it is", Kind::Bool),
    def("media.loudness_target", "REEL_LOUDNESS_TARGET", Section::Ingest, "Copy or re-encode", "Loudness target", "the integrated loudness Normalise loudness brings a clip to, here at ingest, in the clip editor and for each clip of a montage; -16 is the usual level for phones and streaming, -23 for broadcast", int(-30, -5, "LUFS")),
    def("media.copies_on_hardware", "REEL_COPIES_ON_HARDWARE", Section::Ingest, "Hardware and workers", "Copies on the GPU", "copies under a size on the hardware encoder in one pass instead of two-pass x264: faster, a little larger", Kind::Bool),
    def("media.copy_audio", "REEL_COPY_AUDIO", Section::Ingest, "Copy or re-encode", "Audio for copies", "the copies under a size and the preview videos keep the source audio untouched when it is AAC (copy), else AAC at the source rate up to 320; a number forces that AAC rate; only when the video would fall below a watchable floor is the audio stepped down, and the copy says so", Kind::Select(&["copy", "320", "256", "192", "160"])),
    def("media.x264_preset", "REEL_X264_PRESET", Section::Ingest, "Copy or re-encode", "x264 preset for copies", "the two-pass libx264 of the copies under a size; slow is about half the speed of medium on an N100 and worth it at these sizes; faster presets trade quality for time", Kind::Select(&["faster", "fast", "medium", "slow", "slower"])),
    restart(def("media.hwaccel", "REEL_HWACCEL", Section::Ingest, "Hardware and workers", "Hardware encoding", "the GPU route for re-encodes, probed at startup: QSV and VAAPI on Linux, NVENC, AMF and QSV on Windows; the copies under a size use the CPU unless Copies on the GPU is on", Kind::Select(&["auto", "qsv", "vaapi", "nvenc", "amf", "none"]))),
    restart(def("media.render_device", "REEL_RENDER_DEVICE", Section::Ingest, "Hardware and workers", "Render device", "the DRI device the container was given", Kind::Path)),
    fixed(def("media.ffmpeg", "REEL_FFMPEG", Section::Ingest, "Hardware and workers", "ffmpeg", "the binary reel runs; the image sets it", Kind::Text)),
    fixed(def("media.ffprobe", "REEL_FFPROBE", Section::Ingest, "Hardware and workers", "ffprobe", "the probe reel runs; the image sets it", Kind::Text)),
    restart(def("pipeline.workers", "REEL_WORKERS", Section::Ingest, "Hardware and workers", "Workers", "ffmpeg runs at a time; more than one competes with a media server for the GPU. A job someone waits for (a link's preview, a cut, a copy under a size, an upload's ingest) does not wait for them: it runs at once and pauses the others meanwhile", int(1, 8, ""))),
    restart(def("pipeline.nice", "REEL_NICE", Section::Ingest, "Hardware and workers", "Niceness", "CPU priority of every ffmpeg, 0 to 19; higher gives way sooner", int(0, 19, ""))),
    restart(def("pipeline.job_timeout_minutes", "REEL_JOB_TIMEOUT_MINUTES", Section::Ingest, "Hardware and workers", "Job timeout", "a job running longer is killed and marked failed", int(5, 1440, "min"))),
    def("share.embed_mb", "REEL_EMBED_MB", Section::Sharing, "Link preview", "Preview video size from the server", "the video file link previews (og:video) play inline, in Discord and other apps, when this server serves it; a version over this size, or one that is not H.264 with AAC, gets a smaller copy for it. Previewers give up fetching after a few seconds, so the file has to arrive within about five seconds of the server's upload: 20 MB suits 30 to 35 Mbit/s", int(5, 95, "MB")),
    def("external.on", "REEL_EXTERNAL", Section::Sharing, "External storage", "Link previews from external storage", "Discord and other apps give up on a link's preview video after about nine seconds, and from a home upload a large one often comes too late. With this on, reel puts each link's preview video in an S3-compatible storage (Cloudflare R2 is recommended; Backblaze B2, Wasabi, MinIO and AWS work) and the previews load from there in a moment, whatever the upload is doing. The preview videos are then stored with that service; the pages, the clips and the web copies stay here. On R2's free tier (10 GB, no fee for downloads) the default limit costs nothing. A new link waits for its preview's upload, about as long as the file takes at the upload's speed, unless it went up ahead. Discord keeps a link's preview for a while, so a message already posted keeps the embed it had", Kind::Bool),
    under("external.on", def("external.endpoint", "REEL_EXTERNAL_ENDPOINT", Section::Sharing, "External storage", "S3 endpoint", "the service's S3 address; for R2 https://ACCOUNT_ID.r2.cloudflarestorage.com, on the bucket's settings page", Kind::Url)),
    under("external.on", def("external.region", "REEL_EXTERNAL_REGION", Section::Sharing, "External storage", "Region", "the bucket's region: auto for R2 and MinIO, the bucket's own on the others", Kind::Text)),
    under("external.on", def("external.bucket", "REEL_EXTERNAL_BUCKET", Section::Sharing, "External storage", "Bucket", "a bucket of reel's own is best; reel writes under reel/ in it and never touches what it did not write", Kind::Text)),
    under("external.on", def("external.access_key", "REEL_EXTERNAL_ACCESS_KEY", Section::Sharing, "External storage", "Access key", "the id of a token that may read and write this bucket only", Kind::Text)),
    under("external.on", def("external.secret_key", "REEL_EXTERNAL_SECRET_KEY", Section::Sharing, "External storage", "Secret key", "the token's secret; written, never shown again", Kind::Secret)),
    under("external.on", def("external.public_url", "REEL_EXTERNAL_PUBLIC_URL", Section::Sharing, "External storage", "Public address", "where the bucket's files are served from, such as https://media.example.com on a custom domain; R2's r2.dev address works for a test, but Cloudflare limits its rate", Kind::Url)),
    under("external.on", def("external.embed_mb", "REEL_EXTERNAL_EMBED_MB", Section::Sharing, "External storage", "Preview video size from storage", "the preview video of a link served from the storage; the storage has no upload limit, so it can be sharper than the server's, and chat apps still fetch it in time", int(5, 200, "MB"))),
    under("external.on", def("external.limit_gb", "REEL_EXTERNAL_LIMIT_GB", Section::Sharing, "External storage", "Limit", "the most reel keeps there, counted from what it wrote. R2's free 10 GB are shared by every bucket of the account, so with others there keep this under what they leave. At the limit the previews made ahead go first, then the links' previews, the least used first; a link inside the protected window keeps its own", int(1, 10_000, "GB"))),
    under("external.on", def("external.ahead", "REEL_EXTERNAL_AHEAD", Section::Sharing, "External storage", "Made ahead", "previews that go up before a link exists, so a later link is instant; they make room first", Kind::Select(&["off", "edited", "edited_new"]))),
    under("external.on", def("external.new_days", "REEL_EXTERNAL_NEW_DAYS", Section::Sharing, "External storage", "New for", "how long a clip counts as new after it came in, for Made ahead", int(1, 365, "days"))),
    under("external.on", def("external.protect_days", "REEL_EXTERNAL_PROTECT_DAYS", Section::Sharing, "External storage", "Protected window", "a link's preview stays in the storage this long after the link is made, whatever the order; when these alone fill the limit, a new link's preview comes from this server and Status says so; 0 protects none", int(0, 365, "days"))),
    under("external.on", def("share.cleanup", "REEL_LINK_CLEANUP", Section::Sharing, "External storage", "Revoke links never shared", "revokes a link no chat app ever fetched and no one ever opened, once it is as old as the days below, and its preview leaves the storage. A link posted or opened even once is never revoked, however quiet it goes, so an old chat never shows a broken link; nor is one with an expiry of its own. A link pasted where no preview is built and never opened looks never shared", Kind::Bool)),
    under("share.cleanup", def("share.cleanup_days", "REEL_LINK_CLEANUP_DAYS", Section::Sharing, "External storage", "Revoke after", "how many days after it was made a link never shared is revoked", int(1, 365, "days"))),
    def("share.web_mbit", "REEL_WEB_MBIT", Section::Sharing, "Link page", "Web copy bitrate", "a link's page plays a copy of its version at this rate, video and audio together, once the copy is made, and the preview video until then. Everyone watching shares the home upload: at about 30 Mbit/s up, 10 Mbit/s carries two or three viewers at once. A version already under it, H.264 with AAC and no taller than the height below, plays as it is", int(2, 50, "Mbit/s")),
    def("share.web_max_height", "REEL_WEB_MAX_HEIGHT", Section::Sharing, "Link page", "Web copy height", "the tallest a web copy is; a shorter version keeps its own height", Kind::Select(&["720", "1080", "1440", "2160"])),
    def("share.discord_limit_mb", "REEL_DISCORD_LIMIT_MB", Section::Sharing, "Copies under a size", "Default limit", "the limit of a copy under a size asked for without naming one, such as a bare discord.mp4 address; one of the list. Unless set, it is 20, Discord's without Nitro, or the largest limit under 20 when the list has no 20", int(1, 500, "MB")),
    def("share.discord_limits_mb", "REEL_DISCORD_LIMITS_MB", Section::Sharing, "Copies under a size", "Limit list", "the limits the picker offers, in MB; chat apps' upload limits are the point of them: Discord takes 20 MB without Nitro, 50 with Nitro Basic and 500 with Nitro, and some apps still stop at 10", Kind::List),
    def("share.download_default", "REEL_SHARE_DOWNLOAD_DEFAULT", Section::Sharing, "New links", "Allow the download", "whether a new link's page offers the full file, until changed in the link dialog", Kind::Bool),
    def("share.expiry_default", "REEL_SHARE_EXPIRY_DEFAULT", Section::Sharing, "New links", "Expire", "what a new link's expiry starts on in the dialog", Kind::Select(EXPIRIES)),
    def("import.upload_max_gb", "REEL_UPLOAD_MAX_GB", Section::Imports, "Upload", "Upload size limit", "an upload bigger than this is refused; the proxy in front has a limit of its own", int(1, 64, "GB")),
    def("notify.kind", "REEL_NOTIFY_KIND", Section::Notifications, "Where", "Send to", "off, an ntfy topic, or a webhook that takes JSON", Kind::Select(&["off", "ntfy", "webhook"])),
    def("notify.ntfy_url", "REEL_NTFY_URL", Section::Notifications, "Where", "ntfy server", "the server's address, such as https://ntfy.example.org", Kind::Url),
    def("notify.ntfy_topic", "REEL_NTFY_TOPIC", Section::Notifications, "Where", "ntfy topic", "the topic reel posts to", Kind::Text),
    def("notify.ntfy_token", "REEL_NTFY_TOKEN", Section::Notifications, "Where", "ntfy token", "an access token when the topic is protected; written, never shown again", Kind::Secret),
    def("notify.webhook_url", "REEL_WEBHOOK_URL", Section::Notifications, "Where", "Webhook URL", "receives a JSON body with the event, the clip, the reason and a link", Kind::Url),
    def("notify.events", "REEL_NOTIFY_EVENTS", Section::Notifications, "Events", "Events", "what reel tells about; the first five are on for a new install", Kind::Set(EVENTS)),
    fixed(def("auth.mode", "REEL_AUTH_MODE", Section::Access, "Identity", "Mode", "local: reel's own accounts; proxy: a proxy in front names the user; oidc: an identity provider signs in; single: no sign-in, only on a loopback address; set when the process starts", Kind::Select(&["local", "proxy", "oidc", "single"]))),
    fixed(def("auth.trusted_proxies", "REEL_TRUSTED_PROXIES", Section::Access, "Identity", "Trusted proxies", "only these addresses may name the user; anything else answers 401", Kind::List)),
    fixed(def("auth.dev_user", "REEL_DEV_USER", Section::Access, "Identity", "Single user", "the name everyone is in single mode", Kind::Text)),
    def("auth.admins", "REEL_ADMINS", Section::Access, "Identity", "Admins", "users with these names are admins; the first user ever seen is one too; with an identity provider the name is the provider's claim, trusted at first sight", Kind::List),
    restart(def("auth.session_days", "REEL_SESSION_DAYS", Section::Access, "Sessions", "Idle timeout", "a session ends this long after its last use", int(1, 365, "days"))),
    def("auth.session_max_days", "REEL_SESSION_MAX_DAYS", Section::Access, "Sessions", "Session lifetime", "a session ends this long after the sign-in, however active; 0 is no limit", int(0, 3650, "days")),
    restart(def("auth.oidc_issuer", "REEL_OIDC_ISSUER", Section::Access, "Identity provider", "Issuer", "the OpenID Connect issuer URL exactly as the provider gives it, its last slash included, such as https://auth.example.org/application/o/reel/; discovery runs at start", Kind::Url)),
    def("auth.oidc_name", "REEL_OIDC_NAME", Section::Access, "Identity provider", "Provider name", "what the sign-in page calls the provider, as in Sign in with Authentik; empty shows the issuer's host; shown as typed, at most 40 characters", Kind::Text),
    restart(def("auth.oidc_client_id", "REEL_OIDC_CLIENT_ID", Section::Access, "Identity provider", "Client id", "the client registered at the issuer; its secret comes from REEL_OIDC_CLIENT_SECRET or the file REEL_OIDC_CLIENT_SECRET_FILE names, never from here", Kind::Text)),
    restart(def("auth.oidc_scopes", "REEL_OIDC_SCOPES", Section::Access, "Identity provider", "Scopes", "what is asked of the issuer, space separated", Kind::Text)),
    def("auth.oidc_username_claim", "REEL_OIDC_USERNAME_CLAIM", Section::Access, "Identity provider", "Name claim", "the claim shown as the user's name (email only when the provider marks it verified); the subject when it is missing; who the user is comes from the issuer and subject, never from the name", Kind::Text),
    def("auth.oidc_role_claim", "REEL_OIDC_ROLE_CLAIM", Section::Access, "Identity provider", "Role claim", "a claim (a string or a list) that makes an admin when it holds the admin value, followed at every sign-in both ways; the Admins list and the last admin stay admins; empty means the Admins list alone, at first sight", Kind::Text),
    def("auth.oidc_admin_value", "REEL_OIDC_ADMIN_VALUE", Section::Access, "Identity provider", "Admin value", "the value in the role claim that means admin", Kind::Text),
    def("device.chunk_mb", "REEL_DEVICE_CHUNK_MB", Section::Devices, "Uploads from devices", "Chunk size", "a paired app sends a clip in chunks of this size and resumes at the last one after a drop; 90 is the most, so a chunk passes Cloudflare's 100 MB request limit; on the LAN larger is faster", int(1, 90, "MB")),
    def("device.upload_expiry_hours", "REEL_DEVICE_UPLOAD_EXPIRY_HOURS", Section::Devices, "Uploads from devices", "Unfinished uploads expire", "an upload in pieces, the app's or the browser's, that nothing has touched for this long is removed with its file; it starts again from the beginning", int(1, 720, "h")),
    def("device.app_updates", "REEL_APP_UPDATES", Section::Devices, "App updates", "Offer app updates", "a paired app older than this server, on Windows or the Linux AppImage, is offered the app of the server's version, signed, from the app-updates folder in the data folder; the .deb does not update itself", Kind::Bool),
    def("device.updates_token", "REEL_UPDATES_TOKEN", Section::Devices, "App updates", "Release token", "for a private fork only: a GitHub token that reads its releases; the server fetches its own version's installers and their signatures from the release at start and once a day while they are missing, and a public repository's releases need none; written, never shown again", Kind::Secret),
    def("appearance.theme", "REEL_THEME", Section::Appearance, "Theme", "Theme", "dark is the only theme so far; the light one is on the roadmap", Kind::Select(&["dark"])),
    def("advanced.log_level", "REEL_LOG_LEVEL", Section::Advanced, "Logs", "Log level", "how much the log says; RUST_LOG wins when it is set", Kind::Select(&["error", "warn", "info", "debug"])),
    def("advanced.update_check", "REEL_UPDATE_CHECK", Section::Advanced, "Updates", "Check for updates", "once a day, against the public releases of the repository, nothing else", Kind::Bool),
    def("advanced.update_repo", "REEL_UPDATE_REPO", Section::Advanced, "Updates", "Repository", "the GitHub repository whose releases are checked, as owner/name", Kind::Text),
];

/// The word a select or a set shows for a stored value: a value is a code, the pages
/// show words. Unknown values come back as they are, with underscores as spaces.
pub fn option_word(key: &str, value: &str) -> String {
    let word = match (key, value) {
        ("ingest.keep_originals", "reencoded") => "re-encoded clips only",
        ("ingest.keep_originals", "all") => "every clip",
        ("ingest.keep_originals", "none") => "none",
        ("media.copy_audio", "copy") => "as it is",
        ("media.copy_audio", rate) => return format!("AAC at {rate} kbit/s"),
        ("media.hwaccel", "auto") => "automatic",
        ("media.hwaccel", "qsv") => "Intel Quick Sync",
        ("media.hwaccel", "vaapi") => "VAAPI",
        ("media.hwaccel", "nvenc") => "NVIDIA NVENC",
        ("media.hwaccel", "amf") => "AMD AMF",
        ("media.hwaccel", "none") => "none, the CPU",
        ("notify.kind", "off") => "off",
        ("notify.kind", "ntfy") => "an ntfy topic",
        ("notify.kind", "webhook") => "a webhook",
        ("notify.events", "job_failed") => "a job failed",
        ("notify.events", "file_unreadable") => "a file could not be read",
        ("notify.events", "disk_low") => "the disk is low",
        ("notify.events", "update_available") => "an update is available",
        ("notify.events", "link_made") => "a link was made",
        ("notify.events", "storage") => "the external storage needs a look",
        ("notify.events", "library_cap") => "the library cap moved clips",
        ("auth.mode", "local") => "local accounts",
        ("auth.mode", "proxy") => "a proxy in front",
        ("auth.mode", "oidc") => "an identity provider",
        ("auth.mode", "single") => "single user",
        ("share.web_max_height", h) => return format!("{h}p"),
        ("external.ahead", "off") => "none",
        ("external.ahead", "edited") => "for edited clips",
        ("external.ahead", "edited_new") => "for edited and new clips",
        ("share.expiry_default", "never") => "never",
        ("share.expiry_default", "1d") => "after 1 day",
        ("share.expiry_default", "7d") => "after 7 days",
        ("share.expiry_default", "30d") => "after 30 days",
        ("advanced.log_level", "error") => "errors only",
        ("advanced.log_level", "warn") => "warnings",
        ("advanced.log_level", "info") => "info",
        ("advanced.log_level", "debug") => "debug, everything",
        (_, v) => return v.replace('_', " "),
    };
    word.to_string()
}

/// The short line under an option of a set, which shows as a row of its own with a switch;
/// empty where the words say it all.
pub fn option_line(key: &str, value: &str) -> &'static str {
    match (key, value) {
        ("notify.events", "job_failed") => "an ingest, a transcode or a cut stopped with an error",
        ("notify.events", "file_unreadable") => {
            "a file in the incoming folder ffmpeg cannot read, moved to its failed folder"
        }
        ("notify.events", "disk_low") => {
            "once when the free space falls under the disk warning (Storage)"
        }
        ("notify.events", "update_available") => "a newer reel is out",
        ("notify.events", "storage") => {
            "the link previews' storage at 80 and 100 % of its limit, or an upload or a delete there failed"
        }
        ("notify.events", "link_made") => "a share link was made, on the pages or from the app",
        ("notify.events", "library_cap") => {
            "once a run, with how many clips went to the trash and their size"
        }
        _ => "",
    }
}

/// What a stored value says in words: a select's or a set's options as the pages name them,
/// on and off for a switch, empty as "empty".
pub fn value_words(d: &Def, value: &str) -> String {
    if value.is_empty() {
        return "empty".into();
    }
    match d.kind {
        Kind::Bool => if value == "true" { "on" } else { "off" }.into(),
        Kind::Select(_) | Kind::Set(_) => value
            .split(',')
            .map(|v| option_word(d.key, v.trim()))
            .collect::<Vec<_>>()
            .join(", "),
        _ => value.to_string(),
    }
}

pub fn find(key: &str) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.key == key)
}

/// The settings the app's standalone library has no use for: a server's, or the app's own set
/// on the app's pages.
const NOT_IN_STANDALONE: &[&str] = &[
    "site_name",
    "public_url",
    "listen",
    "data_dir",
    "library_dir",
    "ingest.incoming_dir",
    "ingest.originals_dir",
    "storage.cap_gb",
    "storage.cap_keeps_unsent",
    "ingest.poll_seconds",
    "ingest.stable_polls",
    "ingest.extensions",
    "media.copies_on_hardware",
    "media.hwaccel",
    "media.render_device",
    "media.ffmpeg",
    "media.ffprobe",
    "pipeline.workers",
    "pipeline.nice",
    "share.embed_mb",
    "share.cleanup",
    "share.cleanup_days",
    "share.web_mbit",
    "share.web_max_height",
    "share.download_default",
    "share.expiry_default",
    "advanced.log_level",
];

/// A setting's help in the app's standalone library, where a server's words would not fit it.
pub fn help_in(d: &Def, standalone: bool) -> &'static str {
    if !standalone {
        return d.help;
    }
    match d.key {
        "storage.disk_warn_gb" => "each disk that holds the library turns amber under this much free space; 0 turns it off",
        "media.normalize_new" => "every new clip, from OBS or an upload, gets a version levelled to the loudness target right after the ingest, made current, the video copied and only the audio encoded again; the original stays as it is",
        "media.copy_audio" => "the copies under a size keep the source audio untouched when it is AAC (copy), else AAC at the source rate up to 320; a number forces that AAC rate; only when the video would fall below a watchable floor is the audio stepped down, and the copy says so",
        "media.x264_preset" => "the two-pass libx264 of the copies under a size, when Copies on the GPU is off; slow is about half the speed of medium and worth it at these sizes; faster presets trade quality for time",
        "import.upload_max_gb" => "an upload bigger than this is refused",
        "advanced.update_check" => "once a day, the newest release of the repository and nothing else; a newer reel for this app is offered under Startup, to install",
        _ => d.help,
    }
}

pub fn in_standalone(key: &str) -> bool {
    find(key).is_some_and(|d| d.section.in_standalone())
        && !key.starts_with("external.")
        && !NOT_IN_STANDALONE.contains(&key)
}

/// Whether a setting shows: every switch it depends on, and theirs, is on.
pub fn shown(cfg: &Config, key: &str) -> bool {
    let mut at = find(key).and_then(|d| d.depends);
    while let Some(parent) = at {
        if get(cfg, parent) != "true" {
            return false;
        }
        at = find(parent).and_then(|d| d.depends);
    }
    true
}

fn list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_int(raw: &str, min: i64, max: i64) -> Result<i64, String> {
    let n: i64 = raw
        .trim()
        .parse()
        .map_err(|_| format!("{} is not a whole number", raw.trim()))?;
    if n < min || n > max {
        return Err(format!("must be between {min} and {max}"));
    }
    Ok(n)
}

fn parse_bool(raw: &str) -> Result<bool, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" | "" => Ok(false),
        other => Err(format!("{other} is not yes or no")),
    }
}

fn parse_select(raw: &str, options: &[&str]) -> Result<String, String> {
    let v = raw.trim().to_ascii_lowercase();
    if options.contains(&v.as_str()) {
        Ok(v)
    } else {
        Err(format!("one of {}", options.join(", ")))
    }
}

/// A name this long still fits a phone's sign-in button in two lines.
const PROVIDER_NAME_MAX: usize = 40;

fn one_line(raw: &str, max: usize) -> Result<String, String> {
    let v = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if v.chars().any(char::is_control) {
        return Err("plain text on one line".into());
    }
    if v.chars().count() > max {
        return Err(format!("at most {max} characters"));
    }
    Ok(v)
}

/// Sets one key from its string form, checking the kind's own limits.
pub fn set(cfg: &mut Config, key: &str, raw: &str) -> Result<(), String> {
    let d = find(key).ok_or_else(|| format!("no setting named {key}"))?;
    let raw = if key == "auth.mode" && raw.trim().eq_ignore_ascii_case("dev") {
        "single"
    } else {
        raw
    };
    let value = match d.kind {
        Kind::Int { min, max, .. } => parse_int(raw, min, max)?.to_string(),
        Kind::Bool => parse_bool(raw)?.to_string(),
        Kind::Select(options) => parse_select(raw, options)?,
        Kind::Url => {
            // discovery compares an issuer character for character, its last slash included
            let v = if key == "auth.oidc_issuer" {
                raw.trim()
            } else {
                raw.trim().trim_end_matches('/')
            }
            .to_string();
            if !(v.is_empty() || v.starts_with("http://") || v.starts_with("https://")) {
                return Err("a URL starts with http:// or https://".into());
            }
            if key.starts_with("notify.") && v.starts_with("http://") && !private_host(&v) {
                return Err("https, unless the host is on the private network".into());
            }
            v
        }
        Kind::List => list(raw).join(", "),
        Kind::Set(options) => {
            let items = list(raw);
            if let Some(bad) = items.iter().find(|i| !options.contains(&i.as_str())) {
                return Err(format!("{bad} is not one of {}", options.join(", ")));
            }
            items.join(", ")
        }
        Kind::Text if key == "auth.oidc_name" => one_line(raw, PROVIDER_NAME_MAX)?,
        Kind::Text | Kind::Path | Kind::Secret => raw.trim().to_string(),
    };
    let n = |v: &str| v.parse::<i64>().unwrap_or(0);
    match key {
        "site_name" => cfg.site_name = value,
        "public_url" => cfg.public_url = value,
        "listen" => cfg.listen = value,
        "timezone" => cfg.timezone = value,
        "data_dir" => cfg.data_dir = PathBuf::from(value),
        "library_dir" => cfg.library_dir = PathBuf::from(value),
        "ingest.incoming_dir" => cfg.ingest.incoming_dir = PathBuf::from(value),
        "ingest.originals_dir" => cfg.ingest.originals_dir = PathBuf::from(value),
        "ingest.keep_originals" => {
            cfg.ingest.keep_originals = match value.as_str() {
                "all" => KeepOriginals::All,
                "none" => KeepOriginals::None,
                _ => KeepOriginals::Reencoded,
            }
        }
        "share.trash_days" => cfg.share.trash_days = n(&value) as u64,
        "share.discord_cache_gb" => cfg.share.discord_cache_gb = n(&value) as u64,
        "storage.disk_warn_gb" => cfg.storage.disk_warn_gb = n(&value) as u64,
        "ingest.poll_seconds" => cfg.ingest.poll_seconds = n(&value) as u64,
        "ingest.stable_polls" => cfg.ingest.stable_polls = n(&value) as u32,
        "ingest.extensions" => {
            let exts = list(&value);
            if exts.iter().any(|e| {
                e.is_empty() || e.len() > 8 || !e.chars().all(|c| c.is_ascii_alphanumeric())
            }) {
                return Err("extensions are letters and digits, 1 to 8 each".into());
            }
            if exts.is_empty() {
                return Err("at least one extension".into());
            }
            cfg.ingest.extensions = exts.into_iter().map(|e| e.to_ascii_lowercase()).collect();
        }
        "media.copy_max_height" => cfg.media.copy_max_height = n(&value) as u32,
        "media.copy_max_mbit" => cfg.media.copy_max_mbit = n(&value) as u32,
        "media.encode_mbit" => cfg.media.encode_mbit = n(&value) as u32,
        "media.audio_kbit" => cfg.media.audio_kbit = n(&value) as u32,
        "media.normalize_audio" => cfg.media.normalize_audio = value == "true",
        "media.loudness_target" => cfg.media.loudness_target = n(&value) as i32,
        "media.normalize_new" => cfg.media.normalize_new = value == "true",
        "media.copies_on_hardware" => cfg.media.copies_on_hardware = value == "true",
        "storage.cap_gb" => cfg.storage.cap_gb = n(&value) as u64,
        "storage.cap_keeps_unsent" => cfg.storage.cap_keeps_unsent = value == "true",
        "share.embed_mb" => cfg.share.embed_mb = n(&value) as u64,
        "share.web_mbit" => cfg.share.web_mbit = n(&value) as u64,
        "share.web_max_height" => cfg.share.web_max_height = n(&value) as u32,
        "share.cleanup" => cfg.share.cleanup = value == "true",
        "share.cleanup_days" => cfg.share.cleanup_days = n(&value) as u64,
        "external.on" => cfg.external.on = value == "true",
        "external.endpoint" => cfg.external.endpoint = value,
        "external.region" => cfg.external.region = value,
        "external.bucket" => cfg.external.bucket = value,
        "external.access_key" => cfg.external.access_key = value,
        "external.secret_key" => cfg.external.secret_key = value,
        "external.public_url" => cfg.external.public_url = value,
        "external.embed_mb" => cfg.external.embed_mb = n(&value) as u64,
        "external.limit_gb" => cfg.external.limit_gb = n(&value) as u64,
        "external.ahead" => cfg.external.ahead = value,
        "external.new_days" => cfg.external.new_days = n(&value) as u64,
        "external.protect_days" => cfg.external.protect_days = n(&value) as u64,
        "media.copy_audio" => cfg.media.copy_audio = value,
        "media.x264_preset" => cfg.media.x264_preset = value,
        "media.hwaccel" => {
            cfg.media.hwaccel = match value.as_str() {
                "qsv" => Hwaccel::Qsv,
                "vaapi" => Hwaccel::Vaapi,
                "nvenc" => Hwaccel::Nvenc,
                "amf" => Hwaccel::Amf,
                "none" => Hwaccel::None,
                _ => Hwaccel::Auto,
            }
        }
        "media.render_device" => cfg.media.render_device = value,
        "media.ffmpeg" => cfg.media.ffmpeg = value,
        "media.ffprobe" => cfg.media.ffprobe = value,
        "pipeline.workers" => cfg.pipeline.workers = n(&value) as usize,
        "pipeline.nice" => cfg.pipeline.nice = n(&value) as i32,
        "pipeline.job_timeout_minutes" => cfg.pipeline.job_timeout_minutes = n(&value) as u64,
        "share.discord_limit_mb" => cfg.share.discord_limit_mb = n(&value) as u64,
        "share.discord_limits_mb" => {
            let mut limits = Vec::new();
            for item in list(&value) {
                limits.push(parse_int(&item, 1, 500)? as u64);
            }
            if limits.is_empty() || limits.len() > 8 {
                return Err("1 to 8 limits".into());
            }
            limits.sort_unstable();
            limits.dedup();
            cfg.share.discord_limits_mb = limits;
        }
        "share.download_default" => cfg.share.download_default = value == "true",
        "share.expiry_default" => cfg.share.expiry_default = value,
        "import.upload_max_gb" => cfg.import.upload_max_gb = n(&value) as u64,
        "notify.kind" => cfg.notify.kind = value,
        "notify.ntfy_url" => cfg.notify.ntfy_url = value,
        "notify.ntfy_topic" => cfg.notify.ntfy_topic = value,
        "notify.ntfy_token" => cfg.notify.ntfy_token = value,
        "notify.webhook_url" => cfg.notify.webhook_url = value,
        "notify.events" => cfg.notify.events = list(&value),
        "auth.mode" => {
            cfg.auth.mode = match value.as_str() {
                "proxy" => AuthMode::Proxy,
                "single" => AuthMode::Single,
                "oidc" => AuthMode::Oidc,
                _ => AuthMode::Local,
            }
        }
        "auth.trusted_proxies" => cfg.auth.trusted_proxies = list(&value),
        "auth.dev_user" => cfg.auth.dev_user = value,
        "auth.admins" => cfg.auth.admins = list(&value),
        "auth.session_days" => cfg.auth.session_days = n(&value) as u64,
        "auth.session_max_days" => cfg.auth.session_max_days = n(&value) as u64,
        "auth.oidc_issuer" => cfg.auth.oidc_issuer = value,
        "auth.oidc_name" => cfg.auth.oidc_name = value,
        "auth.oidc_client_id" => cfg.auth.oidc_client_id = value,
        "auth.oidc_scopes" => cfg.auth.oidc_scopes = value,
        "auth.oidc_username_claim" => cfg.auth.oidc_username_claim = value,
        "auth.oidc_role_claim" => cfg.auth.oidc_role_claim = value,
        "auth.oidc_admin_value" => cfg.auth.oidc_admin_value = value,
        "device.chunk_mb" => cfg.device.chunk_mb = n(&value) as u64,
        "device.upload_expiry_hours" => cfg.device.upload_expiry_hours = n(&value) as u64,
        "device.app_updates" => cfg.device.app_updates = value == "true",
        "device.updates_token" => cfg.device.updates_token = value,
        "appearance.theme" => cfg.appearance.theme = value,
        "advanced.log_level" => cfg.advanced.log_level = value,
        "advanced.update_check" => cfg.advanced.update_check = value == "true",
        "advanced.update_repo" => cfg.advanced.update_repo = value,
        "edit.quick_trims" => {
            let mut secs = Vec::new();
            for item in list(&value) {
                secs.push(parse_int(&item, 5, 600)? as u32);
            }
            secs.sort_unstable();
            secs.dedup();
            if secs.is_empty() || secs.len() > 4 {
                return Err("1 to 4 values".into());
            }
            cfg.edit.quick_trims = secs;
        }
        _ => return Err(format!("no setting named {key}")),
    }
    Ok(())
}

pub fn get(cfg: &Config, key: &str) -> String {
    let join = |v: &[String]| v.join(", ");
    match key {
        "site_name" => cfg.site_name.clone(),
        "public_url" => cfg.public_url.clone(),
        "listen" => cfg.listen.clone(),
        "timezone" => cfg.timezone.clone(),
        "data_dir" => cfg.data_dir.display().to_string(),
        "library_dir" => cfg.library_dir().display().to_string(),
        "ingest.incoming_dir" => cfg.ingest.incoming_dir.display().to_string(),
        "ingest.originals_dir" => cfg.ingest.originals_dir.display().to_string(),
        "ingest.keep_originals" => match cfg.ingest.keep_originals {
            KeepOriginals::Reencoded => "reencoded",
            KeepOriginals::All => "all",
            KeepOriginals::None => "none",
        }
        .into(),
        "share.trash_days" => cfg.share.trash_days.to_string(),
        "share.discord_cache_gb" => cfg.share.discord_cache_gb.to_string(),
        "storage.disk_warn_gb" => cfg.storage.disk_warn_gb.to_string(),
        "ingest.poll_seconds" => cfg.ingest.poll_seconds.to_string(),
        "ingest.stable_polls" => cfg.ingest.stable_polls.to_string(),
        "ingest.extensions" => join(&cfg.ingest.extensions),
        "media.copy_max_height" => cfg.media.copy_max_height.to_string(),
        "media.copy_max_mbit" => cfg.media.copy_max_mbit.to_string(),
        "media.encode_mbit" => cfg.media.encode_mbit.to_string(),
        "media.audio_kbit" => cfg.media.audio_kbit.to_string(),
        "media.normalize_audio" => cfg.media.normalize_audio.to_string(),
        "media.loudness_target" => cfg.media.loudness_target.to_string(),
        "media.normalize_new" => cfg.media.normalize_new.to_string(),
        "media.copies_on_hardware" => cfg.media.copies_on_hardware.to_string(),
        "storage.cap_gb" => cfg.storage.cap_gb.to_string(),
        "storage.cap_keeps_unsent" => cfg.storage.cap_keeps_unsent.to_string(),
        "share.embed_mb" => cfg.share.embed_mb.to_string(),
        "share.web_mbit" => cfg.share.web_mbit.to_string(),
        "share.web_max_height" => cfg.share.web_max_height.to_string(),
        "share.cleanup" => cfg.share.cleanup.to_string(),
        "share.cleanup_days" => cfg.share.cleanup_days.to_string(),
        "external.on" => cfg.external.on.to_string(),
        "external.endpoint" => cfg.external.endpoint.clone(),
        "external.region" => cfg.external.region.clone(),
        "external.bucket" => cfg.external.bucket.clone(),
        "external.access_key" => cfg.external.access_key.clone(),
        "external.secret_key" => cfg.external.secret_key.clone(),
        "external.public_url" => cfg.external.public_url.clone(),
        "external.embed_mb" => cfg.external.embed_mb.to_string(),
        "external.limit_gb" => cfg.external.limit_gb.to_string(),
        "external.ahead" => cfg.external.ahead.clone(),
        "external.new_days" => cfg.external.new_days.to_string(),
        "external.protect_days" => cfg.external.protect_days.to_string(),
        "media.copy_audio" => cfg.media.copy_audio.clone(),
        "media.x264_preset" => cfg.media.x264_preset.clone(),
        "media.hwaccel" => match cfg.media.hwaccel {
            Hwaccel::Auto => "auto",
            Hwaccel::Qsv => "qsv",
            Hwaccel::Vaapi => "vaapi",
            Hwaccel::Nvenc => "nvenc",
            Hwaccel::Amf => "amf",
            Hwaccel::None => "none",
        }
        .into(),
        "media.render_device" => cfg.media.render_device.clone(),
        "media.ffmpeg" => cfg.media.ffmpeg.clone(),
        "media.ffprobe" => cfg.media.ffprobe.clone(),
        "pipeline.workers" => cfg.pipeline.workers.to_string(),
        "pipeline.nice" => cfg.pipeline.nice.to_string(),
        "pipeline.job_timeout_minutes" => cfg.pipeline.job_timeout_minutes.to_string(),
        "share.discord_limit_mb" => cfg.share.discord_limit_mb.to_string(),
        "share.discord_limits_mb" => cfg
            .share
            .discord_limits_mb
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join(", "),
        "share.download_default" => cfg.share.download_default.to_string(),
        "share.expiry_default" => cfg.share.expiry_default.clone(),
        "import.upload_max_gb" => cfg.import.upload_max_gb.to_string(),
        "notify.kind" => cfg.notify.kind.clone(),
        "notify.ntfy_url" => cfg.notify.ntfy_url.clone(),
        "notify.ntfy_topic" => cfg.notify.ntfy_topic.clone(),
        "notify.ntfy_token" => cfg.notify.ntfy_token.clone(),
        "notify.webhook_url" => cfg.notify.webhook_url.clone(),
        "notify.events" => join(&cfg.notify.events),
        "auth.mode" => match cfg.auth.mode {
            AuthMode::Proxy => "proxy",
            AuthMode::Local => "local",
            AuthMode::Single => "single",
            AuthMode::Oidc => "oidc",
        }
        .into(),
        "auth.trusted_proxies" => join(&cfg.auth.trusted_proxies),
        "auth.dev_user" => cfg.auth.dev_user.clone(),
        "auth.admins" => join(&cfg.auth.admins),
        "auth.session_days" => cfg.auth.session_days.to_string(),
        "auth.session_max_days" => cfg.auth.session_max_days.to_string(),
        "auth.oidc_issuer" => cfg.auth.oidc_issuer.clone(),
        "auth.oidc_name" => cfg.auth.oidc_name.clone(),
        "auth.oidc_client_id" => cfg.auth.oidc_client_id.clone(),
        "auth.oidc_scopes" => cfg.auth.oidc_scopes.clone(),
        "auth.oidc_username_claim" => cfg.auth.oidc_username_claim.clone(),
        "auth.oidc_role_claim" => cfg.auth.oidc_role_claim.clone(),
        "auth.oidc_admin_value" => cfg.auth.oidc_admin_value.clone(),
        "device.chunk_mb" => cfg.device.chunk_mb.to_string(),
        "device.upload_expiry_hours" => cfg.device.upload_expiry_hours.to_string(),
        "device.app_updates" => cfg.device.app_updates.to_string(),
        "device.updates_token" => cfg.device.updates_token.clone(),
        "appearance.theme" => cfg.appearance.theme.clone(),
        "advanced.log_level" => cfg.advanced.log_level.clone(),
        "advanced.update_check" => cfg.advanced.update_check.to_string(),
        "advanced.update_repo" => cfg.advanced.update_repo.clone(),
        "edit.quick_trims" => cfg
            .edit
            .quick_trims
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Default,
    Table,
    File,
    Env,
}

impl Source {
    pub fn name(&self) -> &'static str {
        match self {
            Source::Default => "default",
            Source::Table => "table",
            Source::File => "file",
            Source::Env => "env",
        }
    }
}

/// The config in force, with where each key came from.
#[derive(Debug, Clone)]
pub struct Effective {
    pub config: Arc<Config>,
    pub sources: HashMap<&'static str, Source>,
    pub table: HashMap<String, String>,
}

impl Effective {
    pub fn source(&self, key: &str) -> Source {
        self.sources.get(key).copied().unwrap_or(Source::Default)
    }

    /// Why a key cannot be changed from the UI, or None when it can.
    pub fn locked(&self, key: &str) -> Option<String> {
        let d = find(key)?;
        match self.source(key) {
            Source::Env => Some(format!("set by the environment ({})", d.env)),
            Source::File => Some("set in config.toml".into()),
            _ if d.fixed => {
                Some("set when the process starts, in config.toml or the environment".into())
            }
            _ => None,
        }
    }
}

/// The two layers that exist before the database opens.
pub struct Layers {
    pub base: Config,
    pub file: Option<toml::Value>,
}

pub fn load_layers() -> Result<Layers, ConfigError> {
    let path = std::env::var("REEL_CONFIG").map(PathBuf::from).ok();
    let file = match path {
        Some(p) if p.exists() => {
            let text = std::fs::read_to_string(&p).map_err(|e| ConfigError::Read(p.clone(), e))?;
            Some(
                toml::from_str::<toml::Value>(&text)
                    .map_err(|e| ConfigError::Parse(p.clone(), e))?,
            )
        }
        _ => None,
    };
    Ok(Layers {
        base: Config::default(),
        file,
    })
}

pub fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

pub fn no_env(_name: &str) -> Option<String> {
    None
}

fn file_value(file: &toml::Value, key: &str) -> Option<String> {
    let mut cur = file;
    for part in key.split('.') {
        cur = cur.get(part)?;
    }
    Some(match cur {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(n) => n.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Array(items) => items
            .iter()
            .map(|i| match i {
                toml::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    })
}

/// A host that is only reachable from inside: loopback, a private or link-local range, a
/// bare name, or one of the local suffixes. Plain http is fine there and nowhere else.
pub fn private_host(url: &str) -> bool {
    let rest = url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(v6) = host.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    let host = host.to_ascii_lowercase();
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        };
    }
    host == "localhost"
        || !host.contains('.')
        || [".local", ".lan", ".internal", ".home.arpa"]
            .iter()
            .any(|s| host.ends_with(s))
}

/// Builds the config in force: `base`, then the table, then the file, then the environment.
pub fn build(
    base: &Config,
    file: Option<&toml::Value>,
    table: &[(String, String)],
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Effective, ConfigError> {
    let mut cfg = base.clone();
    let mut sources = HashMap::new();
    let mut kept = HashMap::new();
    for (k, v) in table {
        let Some(d) = find(k) else { continue };
        if d.fixed || (base.standalone() && !in_standalone(d.key)) {
            continue;
        }
        match set(&mut cfg, d.key, v) {
            Ok(()) => {
                sources.insert(d.key, Source::Table);
                kept.insert(d.key.to_string(), v.clone());
            }
            Err(e) => tracing_skip(d.key, v, &e),
        }
    }
    for d in DEFS {
        if let Some(v) = file.and_then(|f| file_value(f, d.key)) {
            set(&mut cfg, d.key, &v)
                .map_err(|e| ConfigError::Invalid(format!("{}: {e}", d.key)))?;
            sources.insert(d.key, Source::File);
        }
    }
    for d in DEFS {
        if let Some(v) = env(d.env) {
            set(&mut cfg, d.key, &v)
                .map_err(|e| ConfigError::Invalid(format!("{}: {e}", d.env)))?;
            sources.insert(d.key, Source::Env);
        }
    }
    // The built-in default limit yields to a configured list that does not hold it.
    let share = &mut cfg.share;
    if !sources.contains_key("share.discord_limit_mb")
        && !share.discord_limits_mb.contains(&share.discord_limit_mb)
    {
        let limits = &share.discord_limits_mb;
        let under = limits.iter().rev().find(|&&l| l <= share.discord_limit_mb);
        if let Some(&l) = under.or(limits.first()) {
            share.discord_limit_mb = l;
        }
    }
    cfg.validate()?;
    Ok(Effective {
        config: Arc::new(cfg),
        sources,
        table: kept,
    })
}

fn tracing_skip(key: &str, value: &str, why: &str) {
    let value = match find(key).map(|d| d.kind) {
        Some(Kind::Secret) => crate::config::redacted(value).to_string(),
        _ => format!("{value:?}"),
    };
    eprintln!(
        "setting {key} = {value} in the database is not usable ({why}); using the layer under it"
    );
}

pub fn without_secrets(cfg: &Config) -> Config {
    let mut out = cfg.clone();
    for d in DEFS.iter().filter(|d| matches!(d.kind, Kind::Secret)) {
        let _ = set(&mut out, d.key, "");
    }
    out
}

pub fn secret_keys() -> impl Iterator<Item = &'static str> {
    DEFS.iter()
        .filter(|d| matches!(d.kind, Kind::Secret))
        .map(|d| d.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_select_value_has_a_word_that_is_not_the_value() {
        for d in DEFS {
            if let Kind::Select(opts) | Kind::Set(opts) = d.kind {
                for o in opts {
                    let w = option_word(d.key, o);
                    assert!(!w.is_empty() && !w.contains('_'), "{}={o}: {w}", d.key);
                }
            }
        }
        assert_eq!(option_word("media.hwaccel", "nvenc"), "NVIDIA NVENC");
        assert_eq!(option_word("media.copy_audio", "192"), "AAC at 192 kbit/s");
        assert_eq!(option_word("other", "some_value"), "some value");
    }

    #[test]
    fn every_key_round_trips_through_its_string_form() {
        let cfg = Config::default();
        for d in DEFS {
            let mut c = cfg.clone();
            let v = get(&cfg, d.key);
            set(&mut c, d.key, &v).unwrap_or_else(|e| panic!("{}: {e}", d.key));
            assert_eq!(get(&c, d.key), v, "{}", d.key);
        }
    }

    #[test]
    fn the_layers_stack_and_the_environment_wins() {
        let mut base = Config::default();
        base.auth.mode = AuthMode::Single;
        base.listen = "127.0.0.1:8080".into();
        let file: toml::Value =
            toml::from_str("[ingest]\npoll_seconds = 7\n[share]\ntrash_days = 3\n").unwrap();
        let table = vec![
            ("import.fireshare_db".to_string(), "/gone".to_string()),
            ("ingest.poll_seconds".to_string(), "9".to_string()),
            ("share.discord_cache_gb".to_string(), "12".to_string()),
            ("listen".to_string(), "1.2.3.4:1".to_string()),
            ("media.audio_kbit".to_string(), "not a number".to_string()),
        ];
        let env = |name: &str| (name == "REEL_TRASH_DAYS").then(|| "30".to_string());
        let eff = build(&base, Some(&file), &table, &env).unwrap();
        assert_eq!(
            eff.config.ingest.poll_seconds, 7,
            "the file beats the table"
        );
        assert_eq!(
            eff.config.share.trash_days, 30,
            "the environment beats the file"
        );
        assert_eq!(
            eff.config.share.discord_cache_gb, 12,
            "the table beats the default"
        );
        assert_eq!(
            eff.config.listen, "127.0.0.1:8080",
            "a fixed key ignores the table"
        );
        assert_eq!(
            eff.config.media.audio_kbit, 320,
            "an unusable table value is skipped"
        );
        assert!(
            !eff.table.contains_key("import.fireshare_db"),
            "a row for a key that no longer exists is ignored, not an error"
        );
        assert_eq!(eff.source("ingest.poll_seconds"), Source::File);
        assert_eq!(eff.source("share.trash_days"), Source::Env);
        assert_eq!(eff.source("share.discord_cache_gb"), Source::Table);
        assert_eq!(eff.source("site_name"), Source::Default);
        assert!(eff
            .locked("share.trash_days")
            .unwrap()
            .contains("REEL_TRASH_DAYS"));
        assert!(eff.locked("listen").is_some());
        assert!(eff.locked("share.discord_cache_gb").is_none());
    }

    #[test]
    fn a_saved_limit_list_without_the_default_limit_keeps_working() {
        let base = Config::default();
        assert_eq!(base.share.discord_limits_mb, vec![10, 20, 50, 100, 500]);
        assert_eq!(base.share.discord_limit_mb, 20);
        let built = |table: &[(&str, &str)]| {
            let table: Vec<(String, String)> = table
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            build(&base, None, &table, &no_env)
        };
        let old = built(&[("share.discord_limits_mb", "10, 50, 100, 500")]).unwrap();
        assert_eq!(old.config.share.discord_limits_mb, vec![10, 50, 100, 500]);
        assert_eq!(
            old.config.share.discord_limit_mb, 10,
            "the largest under 20"
        );
        let big = built(&[("share.discord_limits_mb", "50, 100")]).unwrap();
        assert_eq!(big.config.share.discord_limit_mb, 50, "else the smallest");
        let chosen = built(&[
            ("share.discord_limits_mb", "10, 50"),
            ("share.discord_limit_mb", "50"),
        ])
        .unwrap();
        assert_eq!(
            chosen.config.share.discord_limit_mb, 50,
            "a limit chosen from the list stays"
        );
        assert!(
            built(&[
                ("share.discord_limits_mb", "10, 50"),
                ("share.discord_limit_mb", "20"),
            ])
            .is_err(),
            "a limit set by hand must still be in the list"
        );
    }

    #[test]
    fn the_standalone_library_reads_no_value_of_a_server_or_of_the_app() {
        let mut base = Config {
            listen: "127.0.0.1:8746".into(),
            ..Config::default()
        };
        base.make_standalone();
        base.storage.cap_gb = 100;
        assert!(base.standalone() && base.auth.mode == crate::config::AuthMode::Single);
        assert!(!base.device.app_updates && !base.share.previews && !base.advanced.update_check);
        let table: Vec<(String, String)> = [
            ("storage.cap_gb", "5"),
            ("public_url", "https://clips.example.org"),
            ("media.hwaccel", "none"),
            ("pipeline.workers", "4"),
            ("notify.kind", "ntfy"),
            ("notify.ntfy_url", "https://ntfy.example.org"),
            ("notify.ntfy_topic", "reel"),
            ("external.on", "false"),
            ("share.expiry_default", "7d"),
            ("ingest.incoming_dir", "/srv/incoming"),
            ("share.trash_days", "9"),
            ("media.loudness_target", "-18"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let eff = build(&base, None, &table, &no_env).unwrap();
        let c = &eff.config;
        assert_eq!(c.storage.cap_gb, 100, "the app's cap, not one saved here");
        assert_eq!(c.public_url, base.public_url);
        assert_eq!(c.media.hwaccel, base.media.hwaccel);
        assert_eq!(c.pipeline.workers, 1);
        assert_eq!(c.notify.kind, "off");
        assert!(!c.external.on);
        assert_eq!(c.ingest.incoming_dir, base.ingest.incoming_dir);
        assert_eq!(c.share.trash_days, 9, "a library's own setting is read");
        assert_eq!(c.media.loudness_target, -18);
        assert!(c.standalone(), "never a setting");
        let server = build(&Config::default(), None, &table, &no_env).unwrap();
        assert_eq!(server.config.storage.cap_gb, 5, "a server reads them all");
        assert_eq!(server.config.notify.kind, "ntfy");
        for key in [
            "share.discord_limit_mb",
            "media.copy_max_height",
            "timezone",
        ] {
            assert!(in_standalone(key), "{key}");
        }
        for key in [
            "auth.session_days",
            "device.chunk_mb",
            "notify.events",
            "share.web_mbit",
        ] {
            assert!(!in_standalone(key), "{key}");
        }
        assert!(Section::ALL
            .iter()
            .filter(|s| s.in_standalone())
            .all(|s| DEFS.iter().any(|d| d.section == *s && in_standalone(d.key))));
    }

    #[test]
    fn limits_and_cross_field_rules_refuse() {
        let mut c = Config::default();
        assert!(set(&mut c, "ingest.poll_seconds", "0").is_err());
        assert!(set(&mut c, "ingest.poll_seconds", "5000").is_err());
        assert!(set(&mut c, "media.hwaccel", "cuda").is_err());
        assert!(set(&mut c, "ingest.extensions", "mkv, not ok!").is_err());
        assert!(set(&mut c, "public_url", "clips.example.org").is_err());
        assert!(set(&mut c, "notify.events", "job_failed, nope").is_err());
        assert!(set(&mut c, "share.web_mbit", "1").is_err());
        set(&mut c, "share.web_mbit", "12").unwrap();
        assert_eq!(c.share.web_mbit, 12);
        assert!(set(&mut c, "share.web_max_height", "900").is_err());
        set(&mut c, "share.web_max_height", "1080").unwrap();
        assert_eq!(c.share.web_max_height, 1080);
        assert_eq!(option_word("share.web_max_height", "1440"), "1440p");
        assert!(set(&mut c, "share.embed_mb", "99").is_err());
        assert!(set(&mut c, "share.embed_mb", "4").is_err());
        set(&mut c, "share.embed_mb", "20").unwrap();
        let mut c = Config::default();
        set(&mut c, "share.discord_limit_mb", "7").unwrap();
        assert!(
            c.validate().is_err(),
            "the default limit must be in the list"
        );
        let mut c = Config::default();
        set(&mut c, "share.discord_limits_mb", "50, 10, 10").unwrap();
        assert_eq!(c.share.discord_limits_mb, vec![10, 50]);
        set(&mut c, "timezone", "Mars/Olympus").unwrap();
        assert!(c.validate().is_err());
    }

    #[test]
    fn every_section_has_rows_and_every_key_is_unique() {
        let mut seen = std::collections::HashSet::new();
        for d in DEFS {
            assert!(seen.insert(d.key), "{} twice", d.key);
            assert!(d.env.starts_with("REEL_"), "{}", d.env);
        }
        for s in Section::ALL {
            assert!(
                DEFS.iter().any(|d| d.section == s),
                "{} is empty",
                s.label()
            );
        }
    }

    #[test]
    fn a_setting_depends_on_a_switch_before_it_in_its_own_section() {
        for (i, d) in DEFS.iter().enumerate() {
            let Some(parent) = d.depends else { continue };
            let at = DEFS.iter().position(|p| p.key == parent);
            let p = at
                .map(|j| &DEFS[j])
                .unwrap_or_else(|| panic!("{} depends on {parent}, which is no setting", d.key));
            assert!(
                matches!(p.kind, Kind::Bool),
                "{} depends on {parent}, which is no switch",
                d.key
            );
            assert!(
                at.unwrap() < i,
                "{} comes before {parent}, the switch it depends on",
                d.key
            );
            assert_eq!(
                p.section, d.section,
                "{} and {parent} are in one section",
                d.key
            );
        }
    }

    #[test]
    fn a_setting_shows_while_every_switch_above_it_is_on() {
        let mut c = Config::default();
        assert!(shown(&c, "external.on"));
        assert!(!shown(&c, "external.bucket"));
        assert!(!shown(&c, "share.cleanup_days"));
        set(&mut c, "external.on", "true").unwrap();
        assert!(shown(&c, "external.bucket"));
        assert!(shown(&c, "share.cleanup"));
        assert!(
            !shown(&c, "share.cleanup_days"),
            "under a switch that is off"
        );
        set(&mut c, "share.cleanup", "true").unwrap();
        assert!(shown(&c, "share.cleanup_days"));
        set(&mut c, "external.on", "false").unwrap();
        assert!(
            !shown(&c, "share.cleanup_days"),
            "under a switch under one that is off"
        );
        assert!(c.share.cleanup, "and the values are kept");
    }

    #[test]
    fn plain_http_only_for_a_private_host() {
        for ok in [
            "http://localhost:8080",
            "http://127.0.0.1/hook",
            "http://10.0.0.10:8081",
            "http://192.168.1.5",
            "http://172.20.0.3",
            "http://ntfy.lan",
            "http://mediaserver",
            "http://[::1]:9",
            "http://[fd00::5]",
        ] {
            assert!(private_host(ok), "{ok}");
        }
        for not in [
            "http://example.org",
            "http://8.8.8.8",
            "http://ntfy.sh/topic",
        ] {
            assert!(!private_host(not), "{not}");
        }
        let mut c = Config::default();
        assert!(set(&mut c, "notify.webhook_url", "http://example.org/hook").is_err());
        assert!(set(&mut c, "notify.webhook_url", "https://example.org/hook").is_ok());
        assert!(set(&mut c, "notify.ntfy_url", "http://ntfy.lan").is_ok());
        assert!(set(&mut c, "public_url", "http://example.org").is_ok());
    }

    #[test]
    fn an_issuer_keeps_its_last_slash_and_other_urls_lose_it() {
        let mut c = Config::default();
        set(
            &mut c,
            "auth.oidc_issuer",
            " https://auth.example.org/application/o/reel/ ",
        )
        .unwrap();
        assert_eq!(
            c.auth.oidc_issuer,
            "https://auth.example.org/application/o/reel/"
        );
        set(&mut c, "public_url", "https://clips.example.org/").unwrap();
        assert_eq!(c.public_url, "https://clips.example.org");
    }

    #[test]
    fn a_provider_name_is_one_line_of_plain_text_and_short() {
        let mut c = Config::default();
        set(&mut c, "auth.oidc_name", "  Home\n  login\t ").unwrap();
        assert_eq!(c.auth.oidc_name, "Home login");
        let most = "é".repeat(PROVIDER_NAME_MAX);
        set(&mut c, "auth.oidc_name", &most).unwrap();
        assert_eq!(c.auth.oidc_name, most);
        let err = set(&mut c, "auth.oidc_name", &format!("{most}x")).unwrap_err();
        assert_eq!(err, format!("at most {PROVIDER_NAME_MAX} characters"));
        assert!(find("auth.oidc_name").unwrap().help.contains(&err));
        assert!(set(&mut c, "auth.oidc_name", "Auth\u{7}entik").is_err());
        assert_eq!(c.auth.oidc_name, most, "a refused value changes nothing");
        set(&mut c, "auth.oidc_name", "").unwrap();
        assert_eq!(c.auth.oidc_name, "");
    }
}
