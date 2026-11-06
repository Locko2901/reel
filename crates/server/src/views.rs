use reel_core::format;
use reel_store::models::*;

pub struct DayGroup {
    pub key: String,
    pub label: String,
    pub cards: Vec<Card>,
}

pub struct Who {
    pub name: String,
    pub initial: String,
    pub admin: bool,
    pub local: bool,
    /// The app's standalone library: the pages leave out a server's parts.
    pub standalone: bool,
    pub line: String,
    pub health: &'static str,
    pub health_word: &'static str,
    pub health_line: String,
    /// The header menu's Version row in a browser: this server's version, the dot's class and
    /// what its release check found.
    pub version: &'static str,
    pub version_dot: &'static str,
    pub version_line: String,
}

/// The word the status chip's popover shows for a health class.
pub fn health_word(health: &str) -> &'static str {
    match health {
        "warn" => "warning",
        "err" => "error",
        _ => "ok",
    }
}

impl Who {
    pub fn of(me: &crate::auth::User, cfg: &reel_core::Config) -> Who {
        use reel_core::config::AuthMode;
        let line = match cfg.auth.mode {
            _ if cfg.standalone() => "on this PC".to_string(),
            AuthMode::Proxy => "signed in through the proxy".to_string(),
            AuthMode::Single => "single user".to_string(),
            AuthMode::Local | AuthMode::Oidc => {
                if me.admin { "admin" } else { "member" }.to_string()
            }
        };
        Who {
            name: me.name.clone(),
            initial: me
                .name
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".into()),
            admin: me.admin,
            local: matches!(cfg.auth.mode, AuthMode::Local | AuthMode::Oidc),
            standalone: cfg.standalone(),
            line,
            health: "ok",
            health_word: "ok",
            health_line: String::new(),
            version: reel_core::VERSION,
            version_dot: "",
            version_line: String::new(),
        }
    }

    /// The header's user with the health dot filled in from the Status checks.
    pub async fn at(st: &crate::AppState, me: &crate::auth::User) -> Who {
        let mut who = Who::of(me, &st.ctx.config());
        let (health, health_line) = crate::routes::status::level(st).await;
        who.health = health;
        who.health_word = health_word(health);
        who.health_line = health_line;
        let updates = st.ctx.updates.read().await.clone();
        (who.version_dot, who.version_line) =
            version_words(&updates, st.ctx.config().advanced.update_check);
        who
    }
}

/// The server's version against the newest release, in the words and the colour of the app's
/// Versions row: pink while a newer one is out.
pub fn version_words(
    u: &reel_pipeline::updates::UpdateState,
    check_on: bool,
) -> (&'static str, String) {
    match (u.newer(), &u.latest, &u.error) {
        (Some(r), _, _) => ("active", format!("reel {} is out", r.version)),
        (None, Some(r), _) => (
            "",
            format!("up to date: {} is the newest release", r.version),
        ),
        (None, None, Some(e)) => ("", format!("the releases could not be read: {e}")),
        (None, None, None) if check_on => (
            "",
            "the newest release is read a minute after the start, then once a day".into(),
        ),
        _ => (
            "",
            "the check for new releases is off (Settings, Advanced)".into(),
        ),
    }
}

pub struct Card {
    pub id: String,
    pub title: String,
    pub text: String,
    /// What the page sorts by: the moment, the seconds, the title in small letters, the links.
    pub when: String,
    pub duration_s: i64,
    pub title_key: String,
    pub shares: i64,
    pub owner: Option<String>,
    pub poster: Option<String>,
    pub hover: Option<String>,
    pub duration: String,
    pub meta: Vec<Meta>,
    pub shared: String,
    pub state: String,
    pub progress: i64,
    pub error: Option<String>,
    /// The game in small letters, empty for none; what the library's game filter matches.
    pub game_key: String,
    /// The ids of the collections the clip is in, space separated, for the library's filter.
    pub sets: String,
    /// Whether the user may rename, share and delete it: its owner or an admin.
    pub can_edit: bool,
    pub starred: bool,
}

/// One item of a card's line, with the class a phone orders it by (none for the time).
pub struct Meta {
    pub text: String,
    pub class: &'static str,
}

impl Meta {
    fn new(class: &'static str, text: impl Into<String>) -> Meta {
        Meta {
            text: text.into(),
            class,
        }
    }
}

/// A card as a user sees it: the owner named only when it is someone else, so a server
/// with one uploader never says "by" on every card.
pub fn card_for(row: &ClipRow, me: &crate::auth::User) -> Card {
    let mut c = card(row);
    if c.owner.as_deref() == Some(me.name.as_str()) {
        c.owner = None;
    }
    c.can_edit = me.may_edit(row.clip.owner_id.as_deref());
    c
}

pub fn card(row: &ClipRow) -> Card {
    let c = &row.clip;
    let when = c
        .recorded_at
        .clone()
        .unwrap_or_else(|| c.created_at.clone());
    let moment = format::when(&when);
    let titled = moment != format::relative(&when) && c.title.contains(&format::title_time(&when));
    let file_time = c.recorded_source.as_deref() == Some("file time");
    let mut meta = Vec::new();
    match (titled, file_time) {
        (false, false) => meta.push(Meta::new("", moment)),
        (false, true) => meta.push(Meta::new("", format!("{moment}, file time"))),
        (true, true) => meta.push(Meta::new("", "file time")),
        (true, false) => {}
    }
    if let Some(h) = row.height {
        let copied = row.video_copied.unwrap_or(1) == 1;
        let orig_h = c
            .original_probe
            .as_deref()
            .and_then(|p| serde_json::from_str::<reel_core::ProbeInfo>(p).ok())
            .map(|p| p.height)
            .unwrap_or(0);
        if !copied && orig_h > h as u32 {
            meta.push(Meta::new("res", format!("{h}p, from {orig_h}p")));
        } else {
            meta.push(Meta::new("res", format!("{h}p")));
        }
    }
    match c.source.as_str() {
        "imported" => meta.push(Meta::new("from", "imported")),
        "cut" => meta.push(Meta::new("from", "cut from a clip")),
        "montage" => meta.push(Meta::new("from", "a montage")),
        _ => {}
    }
    let shared = match row.shares {
        0 => String::new(),
        1 => "shared".into(),
        n => format!("shared, {n} links"),
    };
    Card {
        game_key: c
            .game
            .as_deref()
            .map(|g| g.trim().to_lowercase())
            .unwrap_or_default(),
        sets: String::new(),
        can_edit: false,
        starred: false,
        id: c.id.clone(),
        when: when.clone(),
        duration_s: row.duration.unwrap_or(0.0).round() as i64,
        title_key: c.title.to_ascii_lowercase(),
        shares: row.shares,
        text: format!(
            "{}\n{}",
            c.title.to_ascii_lowercase(),
            c.description.to_ascii_lowercase()
        ),
        title: c.title.clone(),
        owner: row.owner_name.clone(),
        poster: row
            .rendition_id
            .as_ref()
            .map(|r| format!("/media/renditions/{r}.jpg?t={}", c.poster_key())),
        hover: row
            .rendition_id
            .as_ref()
            .map(|r| format!("/media/renditions/{r}.loop.mp4")),
        duration: row.duration.map(format::duration).unwrap_or_default(),
        meta,
        shared,
        state: c.state.clone(),
        progress: ((row.progress.unwrap_or(0.0)) * 100.0).round() as i64,
        error: c.error.as_deref().map(short_error),
    }
}

pub fn short_error(e: &str) -> String {
    let head = e.split(" | ").next().unwrap_or(e);
    let head = head.split(": ffprobe failed").next().unwrap_or(head);
    if head.chars().count() > 90 {
        let cut: String = head.chars().take(88).collect();
        format!("{cut}...")
    } else {
        head.to_string()
    }
}

pub struct ShareView {
    pub id: String,
    pub token: String,
    pub url: String,
    pub url_head: String,
    pub url_tail: String,
    pub label: String,
    pub created: String,
    pub views: i64,
    pub download: bool,
    pub expiry: String,
    pub expiry_class: String,
    pub state: String,
    pub serves: String,
    pub pinned: bool,
    /// When an app last built the link's preview and how often people opened it.
    pub activity: String,
    pub posted: Option<String>,
    /// Where the link's preview comes from while the external storage is in use.
    pub preview: String,
    /// The link for the current version, when the link follows the clip and was posted
    /// before that version came.
    pub version_url: Option<String>,
}

/// A link the cleanup will revoke says so where its expiry would be: "never shared, revoked in
/// 12 days", amber within a week.
pub fn cleanup_note(v: &mut ShareView, s: &Share, cfg: &reel_core::Config) {
    if !(cfg.external_on() && cfg.share.cleanup) {
        return;
    }
    let Some(at) = s.cleanup_at(cfg.share.cleanup_days) else {
        return;
    };
    let left = at.as_second() - jiff::Timestamp::now().as_second();
    v.expiry = if left <= 0 {
        "never shared, revoked at the next cleanup".into()
    } else {
        format!("never shared, revoked {}", format::until(&at.to_string()))
    };
    v.expiry_class = if left < 7 * 86_400 {
        "st-warn".into()
    } else {
        String::new()
    };
}

/// "posted 3 h ago, opened 5 times": when an app last fetched a link's page for its preview,
/// and how often a person opened it.
pub fn activity(last_posted_at: Option<&str>, opened: i64) -> String {
    let posted = match last_posted_at {
        Some(t) => format!("posted {}", format::when(t)),
        None => "not posted".into(),
    };
    let opened = match opened {
        0 => "not opened".to_string(),
        1 => "opened once".to_string(),
        n => format!("opened {n} times"),
    };
    format!("{posted}, {opened}")
}

/// The Expires cell of the Shares table in a few characters: "never", "6 d", "expired", and
/// the cleanup's revoke as "13 d, unshared" in amber; the sentence is its title.
pub fn expiry_short(s: &Share, cfg: &reel_core::Config) -> (String, String) {
    let now = jiff::Timestamp::now().as_second();
    let left = |at: i64| {
        let secs = at - now;
        if secs < 3600 {
            format!("{} min", (secs / 60).max(1))
        } else if secs < 86_400 {
            format!("{} h", secs / 3600)
        } else {
            format!("{} d", secs / 86_400)
        }
    };
    if s.revoked_at.is_some() {
        return ("revoked".into(), "st-neutral".into());
    }
    if cfg.external_on() && cfg.share.cleanup {
        if let Some(at) = s.cleanup_at(cfg.share.cleanup_days) {
            let at = at.as_second();
            let when = if at <= now { "due".into() } else { left(at) };
            return (format!("{when}, unshared"), "st-warn".into());
        }
    }
    match s.expires_at.as_deref().and_then(format::parse) {
        None => ("never".into(), String::new()),
        Some(_) if s.is_expired() => ("expired".into(), "st-neutral".into()),
        Some(t) => {
            let at = t.as_second();
            let soon = at - now < 7 * 86_400;
            (left(at), if soon { "st-warn" } else { "" }.into())
        }
    }
}

pub fn share_view(s: &Share, public_url: &str, serves: String) -> ShareView {
    let (expiry, expiry_class) = match (&s.revoked_at, &s.expires_at) {
        (Some(_), _) => ("revoked".to_string(), "st-neutral".to_string()),
        (None, None) => ("never expires".to_string(), String::new()),
        (None, Some(e)) => {
            if s.is_expired() {
                ("expired".to_string(), "st-neutral".to_string())
            } else {
                let u = format::until(e);
                let soon = reel_core::format::parse(e)
                    .map(|t| t.as_second() - jiff::Timestamp::now().as_second() < 7 * 86_400)
                    .unwrap_or(false);
                (
                    format!("expires {u}"),
                    if soon {
                        "st-warn".to_string()
                    } else {
                        String::new()
                    },
                )
            }
        }
    };
    let url = format!("{}/s/{}", public_url.trim_end_matches('/'), s.token);
    let (url_head, url_tail) = format::split_tail(&url, 6);
    ShareView {
        id: s.id.clone(),
        token: s.token.clone(),
        url,
        url_head,
        url_tail,
        label: s.label.clone(),
        created: format::when(&s.created_at),
        views: s.views,
        download: s.allow_download == 1,
        expiry,
        expiry_class,
        state: s.state().into(),
        serves,
        pinned: s.rendition_id.is_some(),
        activity: activity(s.last_posted_at.as_deref(), s.views),
        posted: s.last_posted_at.as_deref().map(format::when),
        preview: String::new(),
        version_url: None,
    }
}

pub struct VersionView {
    pub id: String,
    pub label: String,
    pub meta: String,
    pub current: bool,
    pub state: String,
    pub error: Option<String>,
    pub ready: bool,
    pub url: String,
    pub poster: String,
    pub download_url: String,
    pub deletable: bool,
}

pub fn version_view(r: &Rendition, current_id: Option<&str>) -> VersionView {
    let mut parts = Vec::new();
    if let Some(d) = r.duration {
        parts.push(format::duration(d));
    }
    if let Some(s) = r.size {
        parts.push(format::size(s as u64));
    }
    if r.kind == "cut" {
        let recipe = r
            .recipe
            .as_deref()
            .and_then(|x| serde_json::from_str::<reel_core::CutRecipe>(x).ok());
        if let Some(recipe) = &recipe {
            parts.push(recipe.describe_cut(r.duration.unwrap_or(0.0)));
        }
        let audio = recipe.map(|x| x.audio_changed()).unwrap_or(false);
        parts.push(match (r.video_copied == 1, audio) {
            (true, false) => "stream copy".into(),
            (true, true) => "video copied".into(),
            (false, _) => "re-encoded".into(),
        });
    }
    parts.push(format!(
        "{} {}",
        if r.kind == "master" {
            "published"
        } else {
            "rendered"
        },
        format::date(r.finished_at.as_deref().unwrap_or(&r.created_at))
    ));
    let current = current_id == Some(r.id.as_str());
    VersionView {
        id: r.id.clone(),
        label: r.label(),
        meta: parts.join(", "),
        current,
        state: r.state.clone(),
        error: r.error.clone(),
        ready: r.is_ready(),
        url: format!("/media/renditions/{}.mp4", r.id),
        poster: format!("/media/renditions/{}.jpg", r.id),
        download_url: format!("/media/renditions/{}/download", r.id),
        deletable: r.kind == "cut" && r.is_ready() && !current,
    }
}

pub fn serves_text(share: &Share, versions: &[Rendition], current_id: Option<&str>) -> String {
    let label = |id: &str| versions.iter().find(|r| r.id == id).map(|r| r.label());
    match &share.rendition_id {
        Some(p) => format!(
            "pinned to {}",
            label(p).unwrap_or_else(|| "a removed version".into())
        ),
        None => match current_id.and_then(label) {
            Some(l) => format!("current, now {l}"),
            None => "current".into(),
        },
    }
}

/// JSON for a `<script type="application/json">` block: no character in it can end the block or
/// start markup, and JSON.parse reads the same value.
pub fn script_json(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out
}

/// Server paths never reach a page: the data, incoming, originals and upload folders become their names.
pub fn hide_paths(text: &str, cfg: &reel_core::Config) -> String {
    cfg.hide_paths(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_in_a_script_block_cannot_end_it() {
        let value = serde_json::json!({ "tracks": ["</script><script>x()</script>", "a & b", "\u{2028}\u{2029}"] });
        let text = script_json(&value.to_string());
        assert!(
            !text.contains(['<', '>', '&', '\u{2028}', '\u{2029}']),
            "{text}"
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap(),
            value,
            "the page reads the same value"
        );
    }
}
