use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::routes::html;
use crate::views::Who;
use crate::AppState;
use askama::Template;
use axum::extract::{Path, State};
use axum::response::Response;
use axum::{Extension, Json};
use reel_core::settings::{self as catalog, Def, Effective, Kind, Section, Source};
use reel_core::Config;
use serde_json::{json, Value};

pub struct SectionView {
    pub id: &'static str,
    pub label: &'static str,
    pub line: String,
    pub current: bool,
    pub changed: usize,
}

/// A line of the nav: a group's head, a section, or one of the desktop app's own pages (`app`),
/// which show only in the app's window, as does a head over them alone.
pub struct NavEntry {
    pub head: bool,
    pub app: bool,
    pub id: &'static str,
    pub label: &'static str,
    pub line: String,
    pub current: bool,
    pub changed: usize,
}

impl NavEntry {
    fn head(label: &'static str) -> NavEntry {
        NavEntry {
            head: true,
            app: false,
            id: "",
            label,
            line: String::new(),
            current: false,
            changed: 0,
        }
    }
    fn app(id: &'static str, label: &'static str) -> NavEntry {
        NavEntry {
            head: false,
            app: true,
            id,
            label,
            line: String::new(),
            current: false,
            changed: 0,
        }
    }
    fn section(s: &SectionView) -> NavEntry {
        NavEntry {
            head: false,
            app: false,
            id: s.id,
            label: s.label,
            line: s.line.clone(),
            current: s.current,
            changed: s.changed,
        }
    }
}

/// Where a page of the nav shows: always, in the app's standalone library alone, or beside a
/// server alone.
#[derive(Clone, Copy, PartialEq)]
enum Shows {
    Always,
    Standalone,
    WithServer,
}

/// A page of the nav: its id, whether it is one of the desktop app's own pages, where it shows.
type NavPage = (&'static str, bool, Shows);

const fn reel(id: &'static str) -> NavPage {
    (id, false, Shows::Always)
}

const fn app(id: &'static str) -> NavPage {
    (id, true, Shows::Always)
}

/// The nav by what a person does, the sections and the desktop app's pages side by side;
/// desktop/ui/shell.js groups the same way.
const NAV: [(&str, &[NavPage]); 4] = [
    (
        "Recording",
        &[
            app("obs"),
            app("clips"),
            app("upload"),
            app("hotkeys"),
            app("sounds"),
        ],
    ),
    (
        "Library",
        &[
            reel("storage"),
            ("library", true, Shows::Standalone),
            reel("ingest"),
            ("encoder", true, Shows::Standalone),
            ("sharing", false, Shows::Standalone),
            reel("imports"),
        ],
    ),
    (
        "Server",
        &[
            app("server"),
            ("sharing", false, Shows::WithServer),
            reel("devices"),
            reel("access"),
            reel("notifications"),
        ],
    ),
    (
        "System",
        &[
            reel("general"),
            reel("appearance"),
            app("startup"),
            app("status"),
            app("log"),
            reel("advanced"),
        ],
    ),
];

fn app_label(id: &str, standalone: bool) -> &'static str {
    match id {
        "status" => "Status",
        "server" => "Server",
        "obs" => "OBS",
        "clips" if standalone => "Clips folder",
        "clips" => "Clips folder and cache",
        "upload" if standalone => "New clips",
        "upload" => "Upload",
        "hotkeys" => "Hotkeys",
        "sounds" => "Sounds",
        "startup" => "Startup",
        "log" => "Log",
        "library" => "Library folder",
        "encoder" => "Encoder",
        _ => "",
    }
}

/// The sections in the nav's order, and the nav: the groups, a head shown in a browser only
/// over a section of reel's, the app's pages only in the app's window.
fn nav_of(mut sections: Vec<SectionView>, standalone: bool) -> (Vec<SectionView>, Vec<NavEntry>) {
    let shows = |w: Shows| match w {
        Shows::Always => true,
        Shows::Standalone => standalone,
        Shows::WithServer => !standalone,
    };
    let at = |id: &str| {
        NAV.iter()
            .flat_map(|(_, pages)| pages.iter())
            .filter(|(_, app, w)| !app && shows(*w))
            .position(|(p, _, _)| *p == id)
    };
    sections.sort_by_key(|s| at(s.id).unwrap_or(usize::MAX));
    let mut nav = Vec::new();
    for (head, pages) in NAV {
        let mut entries = Vec::new();
        for (id, is_app, _) in pages.iter().filter(|(_, _, w)| shows(*w)) {
            if *is_app {
                entries.push(NavEntry::app(id, app_label(id, standalone)));
            } else if let Some(s) = sections.iter().find(|s| s.id == *id) {
                entries.push(NavEntry::section(s));
            }
        }
        if entries.is_empty() {
            continue;
        }
        let mut h = NavEntry::head(head);
        h.app = entries.iter().all(|e| e.app);
        nav.push(h);
        nav.extend(entries);
    }
    nav.extend(
        sections
            .iter()
            .filter(|s| at(s.id).is_none())
            .map(NavEntry::section),
    );
    (sections, nav)
}

/// One choice of a select or a set: the stored value, the word shown, whether it is on; a
/// set's option is a row of its own, its words as the title and a short line under them.
pub struct OptionView {
    pub value: String,
    pub label: String,
    pub title: String,
    pub line: &'static str,
    pub chosen: bool,
}

pub struct RowView {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: &'static str,
    pub value: String,
    pub options: Vec<OptionView>,
    pub unit: String,
    pub min: i64,
    pub max: i64,
    pub locked: Option<String>,
    pub restart: bool,
    pub changed: bool,
    pub default_text: String,
    pub is_on: bool,
    pub secret_set: bool,
    pub chosen: Vec<String>,
    /// The switch this row shows under, and whether that switch (or one above it) is off.
    pub depends: &'static str,
    pub dep_off: bool,
    /// A line under the help about the thing itself (a folder's disk), and its colour.
    pub note: String,
    pub note_class: &'static str,
}

pub struct PanelView {
    pub section: &'static str,
    pub name: &'static str,
    pub rows: Vec<RowView>,
    pub actions: Vec<ActionRow>,
}

/// A button of a Settings row that does something at once, as the rows save at once.
pub struct ActionView {
    pub id: &'static str,
    pub label: &'static str,
    pub path: &'static str,
    pub confirm: &'static str,
    pub link: bool,
    pub extra: &'static str,
}

/// A value the buttons of a row send along.
pub struct FieldView {
    pub name: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
}

/// One row of an action panel: the title and the help on the left, the buttons (and a
/// field) in the control column, like a settings row.
pub struct ActionRow {
    pub title: &'static str,
    pub help: String,
    pub field: Option<FieldView>,
    pub buttons: Vec<ActionView>,
    pub depends: &'static str,
    pub dep_off: bool,
}

pub struct ActionPanel {
    pub section: &'static str,
    pub name: &'static str,
    pub line: String,
    pub rows: Vec<ActionRow>,
}

fn act(id: &'static str, label: &'static str, path: &'static str) -> ActionView {
    ActionView {
        id,
        label,
        path,
        confirm: "",
        link: false,
        extra: "",
    }
}

fn with(mut a: ActionView, extra: &'static str) -> ActionView {
    a.extra = extra;
    a
}

fn sure(mut a: ActionView) -> ActionView {
    a.confirm = "Sure?";
    a
}

fn link(id: &'static str, label: &'static str, path: &'static str) -> ActionView {
    ActionView {
        id,
        label,
        path,
        confirm: "",
        link: true,
        extra: "",
    }
}

fn action_row(title: &'static str, help: &str, buttons: Vec<ActionView>) -> ActionRow {
    ActionRow {
        title,
        help: help.into(),
        field: None,
        buttons,
        depends: "",
        dep_off: false,
    }
}

fn under_switch(parent: &'static str, cfg: &Config, mut r: ActionRow) -> ActionRow {
    r.depends = parent;
    r.dep_off = catalog::get(cfg, parent) != "true" || !catalog::shown(cfg, parent);
    r
}

/// Server actions the app's standalone library leaves out: a restart would stop its reel for good.
const SERVER_ACTIONS: &[&str] = &[
    "external-check",
    "external-test",
    "clear-web",
    "remake-embeds",
    "notify-test",
    "export-config",
    "setup-again",
    "restart",
];

fn action_panels(
    disk_line: String,
    storage_line: String,
    trash_line: String,
    cfg: &Config,
) -> Vec<ActionPanel> {
    vec![
        ActionPanel {
            section: "sharing",
            name: "External storage",
            line: String::new(),
            rows: vec![
                under_switch("external.on", cfg, action_row("In the storage", &storage_line, vec![act("external-check", "Check now", "/api/external/check")])),
                under_switch("external.on", cfg, action_row("Test the storage", "writes a small file with the settings above, reads it back over the public address and deletes it; the answer shows here", vec![act("external-test", "Test", "/api/external/test")])),
            ],
        },
        ActionPanel {
            section: "storage",
            name: "Now",
            line: "the trash is purged on its own after its days and the copies trimmed to their size; these do it at once".into(),
            rows: vec![
                action_row("The trash", &trash_line, vec![link("open-trash", "Open", "/trash"), sure(act("purge-trash", "Purge now", "/api/storage/purge-trash"))]),
                action_row("Clear the copies under a size", "the copies made for downloads go; a copy is made again when it is asked for", vec![act("clear-discord", "Clear", "/api/storage/clear-discord")]),
                action_row("Clear the web copies", "the sharper copies the links' pages play go; each page plays its preview video until its copy is made again, on its next view", vec![act("clear-web", "Clear", "/api/storage/clear-web")]),
                action_row("Rebuild posters, loops and filmstrips", "one job per clip at the lowest priority, at the poster time each clip keeps", vec![act("rebuild-derived", "Rebuild", "/api/storage/rebuild-derived")]),
                action_row("Remake the default posters", "every clip that keeps its default poster gets it again, skipping black frames; a poster you picked stays", vec![act("remake-posters", "Remake", "/api/storage/remake-posters")]),
                action_row("Remake the preview videos", "the ones missing under the current size and the ones made for another size or rule, which keep serving until the new one is ready; nothing is remade on its own", vec![act("remake-embeds", "Remake", "/api/storage/remake-embeds")]),
                action_row("Disk", &disk_line, vec![act("disk-refresh", "Refresh", "/api/storage/disk-refresh")]),
            ],
        },
        ActionPanel {
            section: "imports",
            name: "Scan a folder",
            line: if cfg.standalone() {
                "every clip file in it is copied into the library and published, the folder stays as it was, and a second scan skips what it imported before; subfolders are left alone. Move into reel takes the files themselves instead: they are removed from that folder".into()
            } else {
                "every clip file in it is copied in and published, the folder stays as it was, and a second scan skips what it imported before; subfolders are left alone. Move into reel takes the files themselves instead, as the incoming folder does: they are removed from that folder".into()
            },
            rows: vec![ActionRow {
                title: "Folder",
                help: if cfg.standalone() { "a folder on this PC" } else { "a path reel can read" }.into(),
                field: Some(FieldView {
                    name: "path",
                    label: "Folder",
                    placeholder: match (cfg.standalone(), cfg!(windows)) {
                        (true, true) => r"D:\Videos\old clips",
                        (true, false) => "/home/you/Videos/old clips",
                        _ => "/mnt/clips/old",
                    },
                }),
                depends: "",
                dep_off: false,
                buttons: vec![
                    act("scan-folder", "Scan now", "/api/imports/scan"),
                    sure(with(
                        act("move-folder", "Move into reel", "/api/imports/scan"),
                        r#"{"mode":"move"}"#,
                    )),
                ],
            }],
        },
        ActionPanel {
            section: "notifications",
            name: "Try it",
            line: String::new(),
            rows: vec![action_row("Send a test note", "it goes where the rows above say, whatever the events; the answer shows here", vec![act("notify-test", "Send a test", "/api/notify/test")])],
        },
        ActionPanel {
            section: "advanced",
            name: "Export",
            line: String::new(),
            rows: vec![
                action_row("The database", if cfg.standalone() { "a consistent copy taken while reel runs, the secrets blank; a copy of the clips' files with it is Library folder, Make a copy" } else { "a consistent copy taken while reel runs, the secrets blank" }, vec![link("export-database", "Download", "/api/export/database")]),
                action_row("The settings as config.toml", "the settings in force, the secrets blank", vec![link("export-config", "Download", "/api/export/config")]),
            ],
        },
        ActionPanel {
            section: "advanced",
            name: "The process",
            line: String::new(),
            rows: vec![
                action_row("The first run", "the six steps again, over the current values", vec![link("setup-again", "Run it again", "/setup")]),
                action_row("Doctor", "what doctor sees is on the Status page, with a re-probe", vec![link("doctor", "Open Status", "/status")]),
                action_row("Restart reel", "the process exits cleanly; the container's restart policy, or whatever runs reel, starts it again", vec![sure(act("restart", "Restart", "/api/restart"))]),
            ],
        },
    ]
}

/// The trash's size on disk is the same figure as the Disk row's.
fn trash_line(clips: usize, bytes: u64) -> String {
    let held = match clips {
        0 => "empty".to_string(),
        1 => format!("1 clip, {} on disk", reel_core::format::size(bytes)),
        n => format!("{n} clips, {} on disk", reel_core::format::size(bytes)),
    };
    format!("{held}; Open lists them, to restore or delete one now; Purge now deletes them all for good, nothing comes back")
}

pub struct UserView {
    pub id: String,
    pub name: String,
    pub role: String,
    pub disabled: bool,
    pub clips: i64,
    pub last_seen: String,
    pub me: bool,
    /// Bound to an identity at the configured provider.
    pub linked: bool,
}

pub struct AttemptView {
    pub ip: String,
    pub name: String,
    pub when: String,
}

pub struct AccessView {
    pub local: bool,
    pub oidc: bool,
    /// The proxy or the single user's setting names who you are, so you are not renamed here.
    pub by_name: bool,
    pub users: Vec<UserView>,
    pub attempts: Vec<AttemptView>,
}

pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub created: String,
    pub last_seen: String,
    pub uploads: i64,
    pub revoked: bool,
}

pub struct DevicesView {
    pub devices: Vec<DeviceView>,
    pub apps: Vec<crate::routes::device::AppDownload>,
    pub server_url: String,
    pub pair_minutes: i64,
}

async fn devices_view(st: &AppState) -> AppResult<DevicesView> {
    let devices = st
        .ctx
        .store
        .devices()
        .await?
        .iter()
        .map(|d| DeviceView {
            id: d.device.id.clone(),
            name: d.device.name.clone(),
            owner: d.owner_name.clone().unwrap_or_else(|| "gone".into()),
            created: reel_core::format::when(&d.device.created_at),
            last_seen: d
                .device
                .last_seen_at
                .as_deref()
                .map(reel_core::format::when)
                .unwrap_or_else(|| "never".into()),
            uploads: d.uploads,
            revoked: d.device.revoked_at.is_some(),
        })
        .collect();
    Ok(DevicesView {
        devices,
        apps: crate::routes::device::downloads(&st.ctx.config()).await,
        server_url: st.ctx.config().public_url.clone(),
        pair_minutes: crate::routes::device::PAIR_MINUTES,
    })
}

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub section: String,
    pub index: bool,
    pub sections: Vec<SectionView>,
    pub nav_entries: Vec<NavEntry>,
    pub panels: Vec<PanelView>,
    pub actions: Vec<ActionPanel>,
    pub access: AccessView,
    pub devices: DevicesView,
}

async fn access_view(st: &AppState, me: &User) -> AppResult<AccessView> {
    use reel_core::config::AuthMode;
    let cfg = st.ctx.config();
    let counts = st.ctx.store.owner_counts().await?;
    let users = st
        .ctx
        .store
        .users()
        .await?
        .iter()
        .map(|u| UserView {
            id: u.id.clone(),
            name: u.name.clone(),
            role: u.role.clone(),
            disabled: u.disabled_at.is_some(),
            clips: counts
                .iter()
                .find(|(o, _)| o.as_deref() == Some(&u.id))
                .map(|(_, n)| *n)
                .unwrap_or(0),
            last_seen: u
                .last_seen_at
                .as_deref()
                .map(reel_core::format::when)
                .unwrap_or_else(|| "never".into()),
            me: u.id == me.id,
            linked: u.linked_to(&cfg.auth.oidc_issuer),
        })
        .collect();
    let since = jiff::Timestamp::now()
        .checked_sub(jiff::Span::new().hours(24 * 7))
        .map(|t| t.to_string())
        .unwrap_or_default();
    let attempts = st
        .ctx
        .store
        .failed_attempts_after(&since)
        .await?
        .iter()
        .take(50)
        .map(|a| AttemptView {
            ip: a.ip.clone(),
            name: a.name.clone(),
            when: reel_core::format::when(&a.at),
        })
        .collect();
    Ok(AccessView {
        local: matches!(cfg.auth.mode, AuthMode::Local | AuthMode::Oidc),
        oidc: cfg.auth.mode == AuthMode::Oidc,
        by_name: matches!(cfg.auth.mode, AuthMode::Proxy | AuthMode::Single),
        users,
        attempts,
    })
}

/// The config under the table: what Reset goes back to.
pub fn under(ctx: &reel_pipeline::Ctx) -> Config {
    let l = ctx.layers();
    let env: &dyn Fn(&str) -> Option<String> = if l.env {
        &catalog::process_env
    } else {
        &catalog::no_env
    };
    catalog::build(&l.base, l.file.as_ref(), &[], env)
        .map(|e| (*e.config).clone())
        .unwrap_or_else(|_| l.base.clone())
}

pub fn row(d: &Def, eff: &Effective, under: &Config) -> RowView {
    let value = catalog::get(&eff.config, d.key);
    let default_value = catalog::get(under, d.key);
    let chosen: Vec<String> = value
        .split(',')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    let (kind, options, unit, min, max) = match d.kind {
        Kind::Int { min, max, unit } => ("int", vec![], unit.to_string(), min, max),
        Kind::Select(opts) | Kind::Set(opts) => (
            d.kind.name(),
            opts.iter()
                .map(|o| {
                    let label = catalog::option_word(d.key, o);
                    let mut title = label.clone();
                    if let Some(first) = title.get(..1) {
                        title.replace_range(..1, &first.to_uppercase());
                    }
                    OptionView {
                        value: o.to_string(),
                        title,
                        label,
                        line: catalog::option_line(d.key, o),
                        chosen: chosen.iter().any(|c| c == o),
                    }
                })
                .collect(),
            String::new(),
            0,
            0,
        ),
        other => (other.name(), vec![], String::new(), 0, 0),
    };
    let changed = eff.source(d.key) == Source::Table;
    let secret = matches!(d.kind, Kind::Secret);
    let secret_set = secret && !value.is_empty();
    let value = if secret { String::new() } else { value };
    RowView {
        key: d.key,
        label: d.label,
        help: catalog::help_in(d, eff.config.standalone()),
        kind,
        is_on: value == "true",
        secret_set,
        chosen,
        default_text: if matches!(d.kind, Kind::Secret) {
            "the empty value".into()
        } else {
            catalog::value_words(d, &default_value)
        },
        value,
        options,
        unit,
        min,
        max,
        locked: eff.locked(d.key),
        restart: d.restart,
        changed,
        depends: d.depends.unwrap_or(""),
        dep_off: !catalog::shown(&eff.config, d.key),
        note: String::new(),
        note_class: "",
    }
}

/// The folders' lines: the disk each is on, the library away, the database on a share.
pub fn folder_notes(rows: &mut [RowView], disk: &crate::disk::DiskInfo, ctx: &reel_pipeline::Ctx) {
    for r in rows.iter_mut() {
        r.note = disk.folder_line(r.key);
        match r.key {
            "library_dir" => {
                if let Some(why) = ctx.library.away() {
                    r.note =
                        format!("not usable: {why}; nothing is written there until it is back");
                    r.note_class = "st-err";
                }
            }
            "data_dir" => {
                if let Some(kind) = disk.data_share {
                    r.note = reel_pipeline::disks::share_warning(kind);
                    r.note_class = "st-warn";
                }
            }
            _ => {}
        }
    }
}

fn panels(eff: &Effective, under: &Config) -> Vec<PanelView> {
    let mut out: Vec<PanelView> = Vec::new();
    // the unsent rule is the standalone library's alone, set under Library folder there
    let here = |d: &&catalog::Def| {
        if eff.config.standalone() {
            catalog::in_standalone(d.key)
        } else {
            d.key != "storage.cap_keeps_unsent"
        }
    };
    for d in catalog::DEFS.iter().filter(here) {
        let r = row(d, eff, under);
        match out
            .last_mut()
            .filter(|p| p.section == d.section.id() && p.name == d.panel)
        {
            Some(p) => p.rows.push(r),
            None => out.push(PanelView {
                section: d.section.id(),
                name: d.panel,
                rows: vec![r],
                actions: Vec::new(),
            }),
        }
    }
    out
}

/// Settings without a section: on a wide screen General beside the list of sections, as
/// /settings/general; on a phone the list itself, a section a tap away.
pub async fn index(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Response> {
    render(st, me, Section::General, true).await
}

pub async fn page(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(section): Path<String>,
) -> AppResult<Response> {
    let current = Section::parse(&section).ok_or(AppError::NotFound)?;
    render(st, me, current, false).await
}

async fn render(st: AppState, me: User, current: Section, index: bool) -> AppResult<Response> {
    let standalone = st.ctx.config().standalone();
    if standalone && !current.in_standalone() {
        let part = match current {
            Section::Access => crate::standalone::Part::Accounts,
            Section::Devices => crate::standalone::Part::Devices,
            _ => crate::standalone::Part::Server,
        };
        return Ok(crate::standalone::refusal(
            part,
            &format!("/settings/{}", current.id()),
        ));
    }
    let info = st.disk.get(&st.ctx.effective(), false).await;
    let disk_line = info.detail(st.ctx.store.library_bytes().await? as u64);
    let trash_line = trash_line(st.ctx.store.trashed().await?.len(), info.trash);
    let eff = st.ctx.effective();
    let under = under(&st.ctx);
    let mut panels = panels(&eff, &under);
    for p in panels.iter_mut().filter(|p| p.name == "Folders") {
        folder_notes(&mut p.rows, &info, &st.ctx);
    }
    if let Some(r) = panels
        .iter_mut()
        .flat_map(|p| p.rows.iter_mut())
        .find(|r| r.key == "storage.cap_gb")
    {
        let cap = eff.config.storage.cap_gb * 1_000_000_000;
        let plan = reel_pipeline::cap::preview(&st.ctx, cap).await?;
        r.note = reel_pipeline::cap::line(&plan, eff.config.share.trash_days);
        r.note_class = if plan.moved.clips > 0 { "st-warn" } else { "" };
        if plan.moved.clips > 0 && st.ctx.store.cap_hold().await?.is_some() {
            r.note = format!("{} {}", r.note, reel_pipeline::cap::WAITS);
        }
    }
    let mut actions = Vec::new();
    let storage_line = reel_pipeline::external::usage_line(&st.ctx);
    for a in action_panels(disk_line, storage_line, trash_line, &eff.config)
        .into_iter()
        .filter(|a| !standalone || Section::parse(a.section).is_some_and(|s| s.in_standalone()))
        .map(|mut a| {
            if standalone {
                a.rows
                    .retain(|r| r.buttons.iter().all(|b| !SERVER_ACTIONS.contains(&b.id)));
            }
            a
        })
        .filter(|a| !a.rows.is_empty())
    {
        match panels
            .iter_mut()
            .find(|p| p.section == a.section && p.name == a.name)
        {
            Some(p) => p.actions.extend(a.rows),
            None => actions.push(a),
        }
    }
    let (sections, nav_entries) = nav_of(
        Section::ALL
            .iter()
            .filter(|s| !standalone || s.in_standalone())
            .map(|s| SectionView {
                id: s.id(),
                label: s.label_in(standalone),
                line: s.line_in(standalone.then(|| eff.config.library_dir()).as_deref()),
                current: *s == current,
                changed: catalog::DEFS
                    .iter()
                    .filter(|d| d.section == *s && eff.source(d.key) == Source::Table)
                    .count(),
            })
            .collect(),
        standalone,
    );
    Ok(html(SettingsPage {
        who: Who::at(&st, &me).await,
        nav: "settings",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        section: current.id().into(),
        index,
        sections,
        nav_entries,
        panels,
        actions,
        access: access_view(&st, &me).await?,
        devices: devices_view(&st).await?,
    }))
}

pub async fn get_api(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let eff = st.ctx.effective();
    let under = under(&st.ctx);
    let standalone = eff.config.standalone();
    let rows: Vec<Value> = catalog::DEFS
        .iter()
        .filter(|d| !standalone || catalog::in_standalone(d.key))
        .map(|d| {
            let r = row(d, &eff, &under);
            json!({
                "key": r.key, "section": d.section.id(), "panel": d.panel, "label": r.label, "kind": r.kind,
                "value": if r.kind == "secret" { Value::Null } else { Value::String(r.value.clone()) },
                "secret_set": r.secret_set, "source": eff.source(d.key).name(), "locked": r.locked,
                "restart": r.restart, "changed": r.changed, "env": d.env,
            })
        })
        .collect();
    Ok(Json(json!({
        "settings": crate::settings::client(&eff.config),
        "limits": eff.config.share.discord_limits_mb,
        "hwaccel": st.ctx.hw.read().await.backend.name(),
        "rows": rows,
    })))
}

fn as_text(v: &Value) -> Result<Option<String>, AppError> {
    Ok(match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Array(items) => Some(
            items
                .iter()
                .map(|i| match i {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join(", "),
        ),
        _ => {
            return Err(AppError::BadRequest(
                "a value is text, a number, yes or no, or null".into(),
            ))
        }
    })
}

fn check_path(d: &Def, value: &str) -> Result<(), AppError> {
    if !matches!(d.kind, Kind::Path) || value.is_empty() || d.restart {
        return Ok(());
    }
    if !std::path::Path::new(value).is_dir() {
        return Err(AppError::BadRequest(format!("{value}: no such folder")));
    }
    Ok(())
}

const NTFY_URL: &str = "notify.ntfy_url";
const NTFY_TOKEN: &str = "notify.ntfy_token";

/// One key at a time: `{"ingest.poll_seconds": 5}`; `null` goes back to the value under the table.
pub async fn patch_api(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<serde_json::Map<String, Value>>,
) -> AppResult<Json<Value>> {
    let mut last = json!({ "ok": true });
    let token_too = body.contains_key(NTFY_TOKEN);
    for (key, raw) in body {
        let d = catalog::find(&key)
            .ok_or_else(|| AppError::BadRequest(format!("no setting named {key}")))?;
        if st.ctx.config().standalone() && !catalog::in_standalone(&key) {
            return Err(AppError::Standalone(format!(
                "{} is not a setting of reel on this PC: it is a server's, or the app's own",
                d.label
            )));
        }
        let eff = st.ctx.effective();
        if let Some(why) = eff.locked(&key) {
            return Err(AppError::Conflict(format!("{}: {why}", d.label)));
        }
        let old_value = catalog::get(&eff.config, &key);
        let old_table = eff.table.get(&key).cloned();
        let text = as_text(&raw)?;
        // the token was written for one server: it never goes to the next one by itself
        let token_goes =
            key == NTFY_URL && !token_too && !eff.config.notify.ntfy_token.is_empty() && {
                let would_be = match &text {
                    Some(t) => {
                        let mut trial = (*eff.config).clone();
                        catalog::set(&mut trial, &key, t)
                            .ok()
                            .map(|_| catalog::get(&trial, &key))
                    }
                    None => Some(catalog::get(&under(&st.ctx), &key)),
                };
                would_be.is_some_and(|v| v != old_value)
            };
        if token_goes && eff.source(NTFY_TOKEN) != Source::Table {
            return Err(AppError::Conflict(format!(
                "{}: the ntfy token is set {}; change the server there too, so the token goes to no other one",
                d.label,
                if eff.source(NTFY_TOKEN) == Source::Env { "in the environment" } else { "in config.toml" }
            )));
        }
        let stored = match &text {
            Some(t) => {
                let mut trial = (*eff.config).clone();
                catalog::set(&mut trial, &key, t)
                    .map_err(|e| AppError::BadRequest(format!("{}: {e}", d.label)))?;
                trial
                    .validate()
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                let normalised = catalog::get(&trial, &key);
                check_path(d, &normalised)?;
                if normalised == catalog::get(&under(&st.ctx), &key) {
                    st.ctx.store.delete_setting(&key).await?;
                    None
                } else {
                    st.ctx.store.set_setting(&key, &normalised).await?;
                    Some(normalised)
                }
            }
            None => {
                st.ctx.store.delete_setting(&key).await?;
                None
            }
        };
        if token_goes {
            st.ctx.store.delete_setting(NTFY_TOKEN).await?;
            st.ctx
                .store
                .add_setting_history(NTFY_TOKEN, None, None, &me.name)
                .await?;
            tracing::info!(user = %me.name, "the ntfy token cleared with its server's address");
        }
        let new_eff = st.ctx.reload().await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!("the settings could not be reloaded: {e}"))
        })?;
        let new_value = catalog::get(&new_eff.config, &key);
        let secret = matches!(d.kind, Kind::Secret);
        st.ctx
            .store
            .add_setting_history(
                &key,
                if secret {
                    None
                } else {
                    old_table.as_deref().or(Some(old_value.as_str()))
                },
                if secret { None } else { stored.as_deref() },
                &me.name,
            )
            .await?;
        tracing::info!(key, user = %me.name, "setting changed");
        if key == "storage.cap_gb" {
            // saved by an admin: the cap acts from now on
            st.ctx.store.end_cap_hold().await?;
        }
        if key.starts_with("external.") {
            st.ctx.external.kick();
        }
        if key == "advanced.log_level" {
            if let Err(e) = reel_pipeline::logging::set_level(&new_value) {
                tracing::warn!(error = %e, "the log level stays");
            }
        }
        last = json!({
            "ok": true, "key": key, "label": d.label,
            "value": if secret { if new_value.is_empty() { "not set".to_string() } else { "set".to_string() } } else { new_value },
            "old": if secret { Value::Null } else { Value::String(old_value) },
            "old_table": if secret { Value::Null } else { old_table.map(Value::String).unwrap_or(Value::Null) },
            "changed": new_eff.source(&key) == Source::Table,
            "restart": d.restart,
            "default": if secret { Value::Null } else { Value::String(catalog::get(&under(&st.ctx), &key)) },
            "default_text": if secret { "the empty value".to_string() } else { catalog::value_words(d, &catalog::get(&under(&st.ctx), &key)) },
            "words": if secret { Value::Null } else { Value::String(catalog::value_words(d, &catalog::get(&new_eff.config, &key))) },
        });
        if token_goes {
            last["cleared"] = json!([NTFY_TOKEN]);
            last["note"] = json!("The ntfy token was cleared, so it goes to no other server: enter it again for this one.");
        }
    }
    Ok(Json(last))
}

/// The external storage checked now: a pass, and what it holds.
pub async fn external_check(State(st): State<AppState>) -> AppResult<Json<Value>> {
    Ok(Json(
        json!({ "line": reel_pipeline::external::check_now(&st.ctx).await }),
    ))
}

/// The external storage's Test, with the settings in force.
pub async fn external_test(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let line = reel_pipeline::external::test(&st.ctx.config())
        .await
        .map_err(AppError::BadRequest)?;
    Ok(Json(json!({ "line": line })))
}

/// A test note to wherever the Notifications rows point, whatever the events say.
pub async fn notify_test(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Json<Value>> {
    let cfg = st.ctx.config();
    let note = reel_pipeline::notify::Note::new(
        "test",
        format!("{}: a test", cfg.site_name),
        format!(
            "reel can reach this; the events switched on under Notifications will come here. Sent by {}.",
            me.name
        ),
        cfg.public_url.clone(),
    );
    let went = reel_pipeline::notify::deliver(&cfg, &note)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(json!({ "ok": true, "line": went })))
}

fn value_words(d: &Def, v: &str) -> String {
    match d.kind {
        Kind::Bool => if v == "true" { "on" } else { "off" }.into(),
        Kind::Select(_) | Kind::Set(_) => v
            .split(',')
            .map(|x| catalog::option_word(d.key, x.trim()))
            .collect::<Vec<_>>()
            .join(", "),
        _ => v.into(),
    }
}

pub async fn history(State(st): State<AppState>) -> AppResult<Json<Value>> {
    let rows = st.ctx.store.setting_history(500).await?;
    let out: Vec<Value> = rows
        .iter()
        .map(|c| {
            let secret = catalog::find(&c.key)
                .map(|d| matches!(d.kind, Kind::Secret))
                .unwrap_or(false);
            let words = |v: &Option<String>| match (catalog::find(&c.key), v) {
                (Some(d), Some(v)) if !secret => Some(value_words(d, v)),
                _ => None,
            };
            json!({
                "id": c.id, "key": c.key, "label": catalog::find(&c.key).map(|d| d.label).unwrap_or(""),
                "old": if secret { None } else { c.old_value.clone() },
                "new": if secret { None } else { c.new_value.clone() },
                "old_words": words(&c.old_value), "new_words": words(&c.new_value),
                "user": c.user, "at": c.at, "when": reel_core::format::when(&c.at),
            })
        })
        .collect();
    Ok(Json(json!({ "history": out })))
}
