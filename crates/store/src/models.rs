use reel_core::format::now;
use serde::Serialize;
use sqlx::FromRow;

pub fn version_label(kind: &str, seq: i64) -> String {
    match kind {
        "master" => "original".into(),
        "cut" => format!("cut {seq}"),
        k => k.to_string(),
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Clip {
    pub id: String,
    pub title: String,
    pub description: String,
    pub state: String,
    pub source: String,
    pub source_name: String,
    pub recorded_at: Option<String>,
    pub created_at: String,
    pub published_at: Option<String>,
    pub master_id: Option<String>,
    pub current_id: Option<String>,
    pub poster_time: Option<f64>,
    /// When an uploaded image became the poster; none while the poster is a frame.
    #[sqlx(default)]
    pub poster_image: Option<String>,
    pub original_path: Option<String>,
    pub original_probe: Option<String>,
    pub plan_reason: Option<String>,
    pub error: Option<String>,
    pub deleted_at: Option<String>,
    pub source_clip_id: Option<String>,
    pub recorded_source: Option<String>,
    pub owner_id: Option<String>,
    pub source_hash: Option<String>,
    pub device_id: Option<String>,
    pub game: Option<String>,
    pub uploaded_at: Option<String>,
    pub uploaded_to: Option<String>,
    pub deleted_by: Option<String>,
}

impl Clip {
    /// Whether the clip is the user's own: theirs, or one of the admins' when they are one.
    pub fn is_owned_by(&self, user: &User) -> bool {
        match &self.owner_id {
            Some(owner) => *owner == user.id,
            None => user.is_admin(),
        }
    }

    /// What keys the posters' addresses, so a new one is fetched: the uploaded image's moment,
    /// else the frame's time.
    pub fn poster_key(&self) -> String {
        match &self.poster_image {
            Some(at) => at.clone(),
            None => format!("{}", self.poster_time.unwrap_or(0.0)),
        }
    }

    pub fn new(
        id: String,
        title: String,
        source: &str,
        source_name: String,
        recorded_at: Option<String>,
    ) -> Self {
        Self {
            id,
            title,
            description: String::new(),
            state: "processing".into(),
            source: source.into(),
            source_name,
            recorded_at,
            created_at: now(),
            published_at: None,
            master_id: None,
            current_id: None,
            poster_time: None,
            poster_image: None,
            original_path: None,
            original_probe: None,
            plan_reason: None,
            error: None,
            deleted_at: None,
            source_clip_id: None,
            recorded_source: None,
            owner_id: None,
            source_hash: None,
            device_id: None,
            game: None,
            uploaded_at: None,
            uploaded_to: None,
            deleted_by: None,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ClipRow {
    #[sqlx(flatten)]
    pub clip: Clip,
    pub duration: Option<f64>,
    pub height: Option<i64>,
    pub width: Option<i64>,
    pub size: Option<i64>,
    pub video_copied: Option<i64>,
    pub rendition_id: Option<String>,
    pub shares: i64,
    pub progress: Option<f64>,
    pub owner_name: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Rendition {
    pub id: String,
    pub clip_id: String,
    pub kind: String,
    pub derived_from: Option<String>,
    pub recipe: Option<String>,
    pub state: String,
    pub path: Option<String>,
    pub size: Option<i64>,
    pub duration: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub fps: Option<f64>,
    pub vbitrate: Option<i64>,
    pub video_copied: i64,
    pub seq: i64,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub error: Option<String>,
    pub reason: Option<String>,
    pub audio_loudness: Option<f64>,
    pub audio_peak: Option<f64>,
    pub audio_measured_at: Option<String>,
}

impl Rendition {
    pub fn queued(
        id: String,
        clip_id: String,
        kind: &str,
        derived_from: Option<String>,
        recipe: Option<String>,
        seq: i64,
    ) -> Self {
        Self {
            id,
            clip_id,
            kind: kind.into(),
            derived_from,
            recipe,
            state: "queued".into(),
            path: None,
            size: None,
            duration: None,
            width: None,
            height: None,
            vcodec: None,
            acodec: None,
            fps: None,
            vbitrate: None,
            video_copied: 0,
            seq,
            created_at: now(),
            finished_at: None,
            error: None,
            reason: None,
            audio_loudness: None,
            audio_peak: None,
            audio_measured_at: None,
        }
    }
    pub fn is_ready(&self) -> bool {
        self.state == "ready" && self.path.is_some()
    }
    pub fn is_version(&self) -> bool {
        self.kind == "master" || self.kind == "cut"
    }
    pub fn label(&self) -> String {
        version_label(&self.kind, self.seq)
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Share {
    pub id: String,
    pub clip_id: String,
    pub token: String,
    pub label: String,
    pub allow_download: i64,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub views: i64,
    pub last_viewed_at: Option<String>,
    pub rendition_id: Option<String>,
    pub last_posted_at: Option<String>,
}

impl Share {
    pub fn new(
        clip_id: String,
        label: String,
        allow_download: bool,
        expires_at: Option<String>,
        rendition_id: Option<String>,
    ) -> Self {
        Self {
            id: reel_core::ids::new_id(),
            clip_id,
            token: reel_core::ids::new_token(),
            label,
            allow_download: allow_download as i64,
            created_at: now(),
            expires_at,
            revoked_at: None,
            views: 0,
            last_viewed_at: None,
            rendition_id,
            last_posted_at: None,
        }
    }
    pub fn is_expired(&self) -> bool {
        match &self.expires_at {
            Some(e) => reel_core::format::parse(e)
                .map(|t| t <= jiff::Timestamp::now())
                .unwrap_or(false),
            None => false,
        }
    }
    pub fn is_active(&self) -> bool {
        self.revoked_at.is_none() && !self.is_expired()
    }
    /// No chat app ever fetched it and no person ever opened it.
    pub fn never_shared(&self) -> bool {
        self.last_posted_at.is_none() && self.views == 0
    }
    /// When the cleanup revokes this link: `days` after it was made, for a live link never
    /// shared that has no expiry of its own; never for any other.
    pub fn cleanup_at(&self, days: u64) -> Option<jiff::Timestamp> {
        if !self.is_active() || self.expires_at.is_some() || !self.never_shared() {
            return None;
        }
        reel_core::format::parse(&self.created_at)?
            .checked_add(jiff::SignedDuration::from_hours(24 * days as i64))
            .ok()
    }
    pub fn state(&self) -> &'static str {
        if self.revoked_at.is_some() {
            "revoked"
        } else if self.is_expired() {
            "expired"
        } else {
            "active"
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ShareRow {
    #[sqlx(flatten)]
    pub share: Share,
    pub title: String,
    pub current_id: Option<String>,
    pub clip_deleted_at: Option<String>,
    pub pinned_kind: Option<String>,
    pub pinned_seq: Option<i64>,
    pub cur_kind: Option<String>,
    pub cur_seq: Option<i64>,
}

impl ShareRow {
    /// The version a link plays, for a table's cell: "cut 1", "original".
    pub fn version(&self) -> String {
        match (&self.pinned_kind, &self.cur_kind) {
            (Some(k), _) => version_label(k, self.pinned_seq.unwrap_or(0)),
            (None, Some(k)) => version_label(k, self.cur_seq.unwrap_or(0)),
            (None, None) => "current".into(),
        }
    }

    pub fn serves(&self) -> String {
        match (&self.pinned_kind, &self.cur_kind) {
            (Some(k), _) => format!(
                "pinned to {}",
                version_label(k, self.pinned_seq.unwrap_or(0))
            ),
            (None, Some(k)) => format!(
                "current, now {}",
                version_label(k, self.cur_seq.unwrap_or(0))
            ),
            (None, None) => "current".into(),
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub clip_id: Option<String>,
    pub rendition_id: Option<String>,
    pub payload: String,
    pub priority: i64,
    pub state: String,
    pub attempts: i64,
    pub progress: f64,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub error: Option<String>,
    pub route: Option<String>,
    pub cancelled_at: Option<String>,
    pub dismissed_at: Option<String>,
}

pub const PRIORITY_CUT: i64 = 10;
pub const PRIORITY_DISCORD: i64 = 20;
pub const PRIORITY_EMBED: i64 = 40;
pub const PRIORITY_WEB: i64 = 45;
pub const PRIORITY_INGEST: i64 = 50;
pub const PRIORITY_DERIVE: i64 = 90;
/// An animated picture someone asked for, after everything else.
pub const PRIORITY_ANIM: i64 = 95;
/// A job at this priority or under is one someone waits for (a link, the editor, a download):
/// it runs in the lane, never behind a background job.
pub const PRIORITY_LANE: i64 = PRIORITY_EMBED;

impl Job {
    pub fn new(
        kind: &str,
        clip_id: Option<String>,
        rendition_id: Option<String>,
        payload: serde_json::Value,
        priority: i64,
    ) -> Self {
        Self {
            id: reel_core::ids::new_id(),
            kind: kind.into(),
            clip_id,
            rendition_id,
            payload: payload.to_string(),
            priority,
            state: "queued".into(),
            attempts: 0,
            progress: 0.0,
            created_at: now(),
            started_at: None,
            finished_at: None,
            error: None,
            route: None,
            cancelled_at: None,
            dismissed_at: None,
        }
    }

    /// A fresh attempt of a failed job: the same work, the attempt count carried over.
    pub fn retry_of(old: &Job) -> Self {
        let mut j = Job::new(
            &old.kind,
            old.clip_id.clone(),
            old.rendition_id.clone(),
            serde_json::from_str(&old.payload).unwrap_or(serde_json::Value::Null),
            old.priority,
        );
        j.attempts = old.attempts;
        j
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct IngestFile {
    pub path: String,
    pub size: i64,
    pub first_seen_at: String,
    pub stable_polls: i64,
    pub state: String,
    pub clip_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SettingChange {
    pub id: i64,
    pub key: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub user: String,
    pub at: String,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct User {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub password_hash: Option<String>,
    pub role: String,
    pub created_at: String,
    pub disabled_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub session_version: i64,
    pub oidc_issuer: Option<String>,
    pub oidc_subject: Option<String>,
}

impl User {
    pub fn new(name: &str, role: &str, password_hash: Option<String>) -> Self {
        Self {
            id: reel_core::ids::new_id(),
            name: name.trim().to_string(),
            password_hash,
            role: role.into(),
            created_at: now(),
            disabled_at: None,
            last_seen_at: None,
            session_version: 1,
            oidc_issuer: None,
            oidc_subject: None,
        }
    }
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SigninAttempt {
    pub id: i64,
    pub ip: String,
    pub name: String,
    pub at: String,
    pub ok: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Device {
    pub id: String,
    pub user_id: String,
    pub name: String,
    #[serde(skip)]
    pub token_hash: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
}

impl Device {
    pub fn new(user_id: &str, name: &str, token_hash: &str) -> Self {
        Self {
            id: reel_core::ids::new_id(),
            user_id: user_id.into(),
            name: name.trim().to_string(),
            token_hash: token_hash.into(),
            created_at: now(),
            last_seen_at: None,
            revoked_at: None,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DeviceRow {
    #[sqlx(flatten)]
    pub device: Device,
    pub owner_name: Option<String>,
    pub uploads: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PairCode {
    pub code_hash: String,
    pub user_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub used_at: Option<String>,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DeviceUpload {
    pub id: String,
    pub device_id: String,
    pub user_id: String,
    pub file_name: String,
    pub size: i64,
    pub sha256: String,
    pub received: i64,
    pub state: String,
    pub recorded_at: Option<String>,
    pub game: Option<String>,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub clip_id: Option<String>,
    pub error: Option<String>,
    pub keep_last: Option<f64>,
    /// The audio tracks' names as the device gave them, a JSON list.
    pub audio_names: Option<String>,
}

/// An upload from the browser in pieces; `file_key` is the browser's word for the file (its size
/// and time), so the same file picked again continues where it stopped.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WebUpload {
    pub id: String,
    pub user_id: String,
    pub file_name: String,
    pub size: i64,
    pub file_key: String,
    pub received: i64,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub clip_id: Option<String>,
}

impl WebUpload {
    pub fn new(user_id: &str, file_name: &str, size: i64, file_key: &str) -> Self {
        let at = now();
        Self {
            id: reel_core::ids::new_id(),
            user_id: user_id.into(),
            file_name: file_name.into(),
            size,
            file_key: file_key.into(),
            received: 0,
            state: "open".into(),
            created_at: at.clone(),
            updated_at: at,
            finished_at: None,
            clip_id: None,
        }
    }
}

impl DeviceUpload {
    pub fn new(device: &Device, file_name: &str, size: i64, sha256: &str) -> Self {
        let at = now();
        Self {
            id: reel_core::ids::new_id(),
            device_id: device.id.clone(),
            user_id: device.user_id.clone(),
            file_name: file_name.into(),
            size,
            sha256: sha256.into(),
            received: 0,
            state: "open".into(),
            recorded_at: None,
            game: None,
            title: None,
            created_at: at.clone(),
            updated_at: at,
            finished_at: None,
            clip_id: None,
            error: None,
            keep_last: None,
            audio_names: None,
        }
    }
}

/// One row of the library cap's walk.
#[derive(Debug, Clone, FromRow)]
pub struct ClipSize {
    pub id: String,
    pub created_at: String,
    pub uploaded_at: Option<String>,
    pub bytes: i64,
}

/// A clip as the library cap weighs it: when it was recorded, the room its
/// versions and its preview video take in the library, and what may spare it.
#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct CapRow {
    pub id: String,
    pub title: String,
    pub at: String,
    pub bytes: i64,
    pub uploaded_at: Option<String>,
    /// A share link that plays now.
    pub linked: bool,
    /// In a collection someone put it in (a game's does not count).
    pub in_set: bool,
    pub starred: bool,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub owner_id: Option<String>,
    pub created_at: String,
    /// A game's own collection: the game in small letters; its clips are the clips with it.
    pub game: Option<String>,
}

impl Collection {
    pub fn new(name: &str, owner_id: Option<String>) -> Self {
        Self {
            id: reel_core::ids::new_id(),
            name: name.into(),
            owner_id,
            created_at: now(),
            game: None,
        }
    }
    pub fn is_game(&self) -> bool {
        self.game.is_some()
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct CollectionRow {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub collection: Collection,
    pub clips: i64,
    pub token: Option<String>,
    /// When the newest clip in it was recorded, or came in.
    pub newest: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct CollectionLink {
    pub id: String,
    pub collection_id: String,
    pub token: String,
    pub allow_download: i64,
    pub created_at: String,
    pub revoked_at: Option<String>,
    pub views: i64,
    pub last_posted_at: Option<String>,
    pub last_viewed_at: Option<String>,
}

impl CollectionLink {
    pub fn new(collection_id: &str, allow_download: bool) -> Self {
        Self {
            id: reel_core::ids::new_id(),
            collection_id: collection_id.into(),
            token: reel_core::ids::new_token(),
            allow_download: allow_download as i64,
            created_at: now(),
            revoked_at: None,
            views: 0,
            last_posted_at: None,
            last_viewed_at: None,
        }
    }
}

/// An object reel put in the external storage: uploading, stored or deleting.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ExternalObject {
    pub id: String,
    pub version_id: String,
    pub clip_id: String,
    /// The rendition uploaded: the version itself or its preview copy at the storage's size.
    pub file_id: String,
    pub key: String,
    pub size: i64,
    pub class: String,
    pub state: String,
    pub created_at: String,
    pub stored_at: Option<String>,
    pub error: Option<String>,
    /// A link handed out its address: it goes once no live link serves its version.
    pub exposed: i64,
}

/// Why a version's links are served from this server while the storage is on.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ExternalFallback {
    pub version_id: String,
    pub why: String,
    pub detail: String,
    pub at: String,
}

/// A live link (a share or a collection's) and the version it serves.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct LiveLink {
    pub version_id: Option<String>,
    pub clip_id: String,
    pub created_at: String,
    pub last_posted_at: Option<String>,
    pub last_viewed_at: Option<String>,
}

/// A ready clip's current version.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct CurrentVersion {
    pub clip_id: String,
    pub version_id: String,
    pub kind: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clip_is_its_owners_and_one_without_an_owner_is_the_admins() {
        let robin = User::new("robin", "admin", None);
        let mira = User::new("mira", "member", None);
        let mut clip = Clip::new("c".into(), "t".into(), "device", "a.mkv".into(), None);
        clip.owner_id = Some(mira.id.clone());
        assert!(clip.is_owned_by(&mira));
        assert!(
            !clip.is_owned_by(&robin),
            "an admin's device gets no one else's clip"
        );
        clip.owner_id = Some(robin.id.clone());
        assert!(clip.is_owned_by(&robin) && !clip.is_owned_by(&mira));
        clip.owner_id = None;
        assert!(
            clip.is_owned_by(&robin),
            "the incoming folder's clips are the admins'"
        );
        assert!(!clip.is_owned_by(&mira));
    }
}
