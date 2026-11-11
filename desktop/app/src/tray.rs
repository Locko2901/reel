use crate::status::Status;
use reel_desktop_core::settings::TrayIcon;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Wry};

pub const TRAY_ID: &str = "main";

/// One entry of the tray's menu; an `Act` carries its id, label and whether it can be clicked.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Line(String, String),
    Act(String, String, bool),
    Sub(String, String, Vec<Item>),
    Gap,
}

fn act(id: &str, label: impl Into<String>, on: bool) -> Item {
    Item::Act(id.into(), label.into(), on)
}

/// The menu's first line: the app's state and what it is doing.
fn state_line(st: &Status) -> String {
    if st.state_line.is_empty() {
        st.state_word.clone()
    } else {
        format!("{}: {}", st.state_word, st.state_line)
    }
}

pub fn items(st: &Status) -> Vec<Item> {
    let standalone = st.mode == "standalone";
    let mut out = vec![Item::Line("state".into(), state_line(st))];
    if st.obs_box.as_ref().is_some_and(|b| b.state == "gave_up") {
        out.push(act("obs-again", "Start OBS again", true));
    }
    out.push(Item::Gap);
    let recent: Vec<(String, String)> = st
        .recent
        .iter()
        .filter_map(|r| {
            let id = if standalone {
                r.local_clip_id.clone()
            } else {
                r.clip_id.clone()
            }?;
            Some((id, r.title.clone().unwrap_or_else(|| r.name.clone())))
        })
        .take(5)
        .collect();
    if !recent.is_empty() {
        let clips = recent
            .into_iter()
            .map(|(id, label)| {
                let mut acts = vec![act(&format!("open:{id}"), "Open", true)];
                if !standalone {
                    acts.push(act(&format!("copy:{id}"), "Copy link", true));
                }
                Item::Sub(format!("clip:{id}"), label, acts)
            })
            .collect();
        out.push(Item::Sub("recent".into(), "Last clips".into(), clips));
    }
    if let Some(v) = st.update_version.as_ref() {
        if st.update_installing {
            let line = &st.update_line;
            let line = if line.starts_with("reel ") {
                line.clone()
            } else {
                capital(line)
            };
            out.push(Item::Line("update".into(), line));
        } else {
            out.push(act("update", format!("Install reel {v}"), true));
        }
    }
    out.push(act("save", "Save clip", st.save.ok));
    if !standalone {
        out.push(act("save-share", "Save and share", st.save_share.ok));
    }
    let why = if !st.save.ok && !st.save.why.is_empty() {
        Some(capital(&st.save.why))
    } else if !standalone && !st.save_share.ok && !st.save_share.why.is_empty() {
        Some(format!("Save and share: {}", st.save_share.why))
    } else {
        None
    };
    if let Some(w) = why {
        out.push(Item::Line("save-why".into(), w));
    }
    out.push(match &st.session {
        Some(s) if s.starting => {
            Item::Line("session".into(), "Starting the session recording".into())
        }
        Some(s) if s.stopping => {
            Item::Line("session".into(), "Stopping the session recording".into())
        }
        Some(s) => act(
            "session",
            format!("Stop the session recording ({})", s.length),
            true,
        ),
        None => act("session", "Record a session", st.record.ok),
    });
    out.push(act(
        "recording",
        if st.recording_paused {
            "Resume clips"
        } else {
            "Pause clips"
        },
        true,
    ));
    if !standalone {
        out.push(act(
            "share",
            "Share the last clip",
            st.paired && !st.recent.is_empty(),
        ));
    }
    if !standalone || st.paired {
        out.push(act(
            "pause",
            if st.paused_by_user {
                "Resume uploads"
            } else {
                "Pause uploads"
            },
            true,
        ));
    }
    out.push(Item::Gap);
    out.push(act(
        "library",
        "Open library",
        standalone || !st.server_url.is_empty(),
    ));
    out.push(act("settings", "Open settings", true));
    // On Linux OBS's own tray icon opens its window.
    match st.obs_box.as_ref().filter(|b| b.can_show && !st.obs_linux) {
        Some(b) if b.shown => out.push(act("obs-hide", "Hide OBS", true)),
        Some(b) => out.push(act("obs-show", "Open OBS", b.state == "running")),
        None => {}
    }
    out.push(Item::Gap);
    out.push(act("quit", "Quit", true));
    out
}

pub fn menu(app: &AppHandle, st: &Status) -> tauri::Result<Menu<Wry>> {
    menu_of(app, items(st))
}

fn menu_of(app: &AppHandle, items: Vec<Item>) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    for item in items {
        match item {
            Item::Gap => menu.append(&PredefinedMenuItem::separator(app)?)?,
            Item::Sub(id, label, inner) => menu.append(&submenu(app, &id, &label, inner)?)?,
            other => menu.append(&entry(app, other)?)?,
        }
    }
    Ok(menu)
}

fn entry(app: &AppHandle, item: Item) -> tauri::Result<MenuItem<Wry>> {
    match item {
        Item::Line(id, label) => MenuItem::with_id(app, id, label, false, None::<&str>),
        Item::Act(id, label, on) => MenuItem::with_id(app, id, label, on, None::<&str>),
        _ => MenuItem::with_id(app, "gap", "", false, None::<&str>),
    }
}

fn submenu(
    app: &AppHandle,
    id: &str,
    label: &str,
    items: Vec<Item>,
) -> tauri::Result<Submenu<Wry>> {
    let sub = Submenu::with_id(app, id, label, true)?;
    for item in items {
        match item {
            Item::Sub(i, l, inner) => sub.append(&submenu(app, &i, &l, inner)?)?,
            Item::Gap => sub.append(&PredefinedMenuItem::separator(app)?)?,
            other => sub.append(&entry(app, other)?)?,
        }
    }
    Ok(sub)
}

pub(crate) fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

macro_rules! tray_pngs {
    ($kind:literal) => {
        [
            include_bytes!(concat!("../icons/tray/", $kind, "-16.png")).as_slice(),
            include_bytes!(concat!("../icons/tray/", $kind, "-20.png")).as_slice(),
            include_bytes!(concat!("../icons/tray/", $kind, "-24.png")).as_slice(),
            include_bytes!(concat!("../icons/tray/", $kind, "-32.png")).as_slice(),
            include_bytes!(concat!("../icons/tray/", $kind, "-64.png")).as_slice(),
        ]
    };
}

/// The tray's icon for its size: on Windows 16, 20, 24 or 32 px by the display's scale; on
/// Linux 64 px, which the desktop scales to its panel. The kind is kept for the recording's dot.
pub fn icon(app: &AppHandle, kind: TrayIcon) -> tauri::Result<Image<'static>> {
    ICON.lock().unwrap_or_else(|e| e.into_inner()).0 = kind;
    plain(app, kind)
}

fn plain(app: &AppHandle, kind: TrayIcon) -> tauri::Result<Image<'static>> {
    let set = match kind {
        TrayIcon::Colour => tray_pngs!("colour"),
        TrayIcon::White => tray_pngs!("white"),
        TrayIcon::Black => tray_pngs!("black"),
    };
    let at = if cfg!(windows) {
        let scale = app
            .primary_monitor()
            .ok()
            .flatten()
            .map(|m| m.scale_factor())
            .unwrap_or(1.0);
        match scale {
            s if s >= 1.75 => 3,
            s if s >= 1.375 => 2,
            s if s >= 1.125 => 1,
            _ => 0,
        }
    } else {
        4
    };
    Image::from_bytes(set[at])
}

/// The icon's kind and whether it shows the recording's dot, as last set.
static ICON: std::sync::Mutex<(TrayIcon, bool)> = std::sync::Mutex::new((TrayIcon::Colour, false));

pub fn set_icon(app: &AppHandle, kind: TrayIcon) {
    let dot = {
        let mut set = ICON.lock().unwrap_or_else(|e| e.into_inner());
        set.0 = kind;
        set.1
    };
    draw(app, kind, dot);
}

fn draw(app: &AppHandle, kind: TrayIcon, dot: bool) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match plain(app, kind) {
        Ok(i) => {
            let i = if dot { with_dot(&i) } else { i };
            let _ = tray.set_icon(Some(i));
        }
        Err(e) => tracing::warn!(error = %e, "tray icon"),
    }
}

/// The recording's red, as the window's.
const RED: [u8; 3] = [0xe5, 0x48, 0x4d];
/// reel's surface: a ring around the dot, so it reads on a light tray and on the colour icon.
const RING: [u8; 3] = [0x0d, 0x0b, 0x10];

/// The icon with a red dot in its lower right corner: a session recording runs.
pub fn with_dot(icon: &Image<'_>) -> Image<'static> {
    let (w, h) = (icon.width(), icon.height());
    let mut px = icon.rgba().to_vec();
    dot(&mut px, w, h);
    Image::new_owned(px, w, h)
}

fn dot(px: &mut [u8], w: u32, h: u32) {
    let size = w.min(h) as f32;
    let r = size * 0.22;
    let ring = (size / 16.0).max(1.0);
    let (cx, cy) = (w as f32 - r - ring, h as f32 - r - ring);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let outer = (r + ring + 0.5 - d).clamp(0.0, 1.0);
            if outer <= 0.0 {
                continue;
            }
            let at = ((y * w + x) * 4) as usize;
            over(&mut px[at..at + 4], RING, outer);
            over(&mut px[at..at + 4], RED, (r + 0.5 - d).clamp(0.0, 1.0));
        }
    }
}

/// `rgb` at coverage `a` over a straight alpha RGBA pixel.
fn over(p: &mut [u8], rgb: [u8; 3], a: f32) {
    if a <= 0.0 {
        return;
    }
    let below = p[3] as f32 / 255.0;
    let out = a + below * (1.0 - a);
    for c in 0..3 {
        let v = (rgb[c] as f32 * a + p[c] as f32 * below * (1.0 - a)) / out;
        p[c] = v.round().clamp(0.0, 255.0) as u8;
    }
    p[3] = (out * 255.0).round() as u8;
}

pub fn refresh(app: &AppHandle, st: &Status) {
    static SHOWN: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    let line = state_line(st);
    let mut shown = SHOWN.lock().unwrap_or_else(|e| e.into_inner());
    if *shown != line {
        tracing::debug!(line = %line, "the tray's state line");
        *shown = line;
    }
    drop(shown);
    let recording = st.session.as_ref().is_some_and(|s| !s.starting);
    let redraw = {
        let mut set = ICON.lock().unwrap_or_else(|e| e.into_inner());
        let changed = set.1 != recording;
        set.1 = recording;
        changed.then_some(set.0)
    };
    if let Some(kind) = redraw {
        draw(app, kind, recording);
    }
    let _ = writer(app).send((items(st), st.tooltip()));
}

/// How long the tray's writer waits for the rest of a burst of changes.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(100);

/// One thread sets the tray's menu and tooltip, in turn, the newest of a burst and only when
/// it changed: Linux's appindicator can crash on a menu replaced in a burst, as a crashing
/// OBS's line changes several times a second. The main thread never waits on it.
fn writer(app: &AppHandle) -> &'static std::sync::mpsc::Sender<(Vec<Item>, String)> {
    static TX: std::sync::OnceLock<std::sync::mpsc::Sender<(Vec<Item>, String)>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(Vec<Item>, String)>();
        let app = app.clone();
        std::thread::spawn(move || {
            let mut set: Option<(Vec<Item>, String)> = None;
            while let Ok(mut next) = rx.recv() {
                std::thread::sleep(SETTLE);
                while let Ok(newer) = rx.try_recv() {
                    next = newer;
                }
                let Some(tray) = app.tray_by_id(TRAY_ID) else {
                    continue;
                };
                if set.as_ref() == Some(&next) {
                    continue;
                }
                match menu_of(&app, next.0.clone()) {
                    Ok(m) => {
                        let _ = tray.set_menu(Some(m));
                    }
                    Err(e) => tracing::warn!(error = %e, "tray menu"),
                }
                let _ = tray.set_tooltip(Some(next.1.clone()));
                set = Some(next);
            }
        });
        tx
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::{Action, RecentItem};

    fn flat(items: &[Item]) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        for i in items {
            match i {
                Item::Act(id, _, on) => out.push((id.clone(), *on)),
                Item::Line(id, _) => out.push((id.clone(), false)),
                Item::Sub(id, _, inner) => {
                    out.push((id.clone(), true));
                    out.extend(flat(inner));
                }
                Item::Gap => {}
            }
        }
        out
    }

    fn ids(st: &Status) -> Vec<String> {
        flat(&items(st)).into_iter().map(|(id, _)| id).collect()
    }

    fn on(st: &Status, id: &str) -> bool {
        flat(&items(st))
            .into_iter()
            .find(|(i, _)| i == id)
            .unwrap_or_else(|| panic!("no {id}"))
            .1
    }

    fn clip(server: Option<&str>, local: Option<&str>, title: &str) -> RecentItem {
        RecentItem {
            clip_id: server.map(String::from),
            local_clip_id: local.map(String::from),
            name: format!("{title}.mp4"),
            title: Some(title.into()),
            when: "just now".into(),
            state: "confirmed".into(),
            phase: None,
            share_url: None,
            path: None,
        }
    }

    fn status(mode: &str, paired: bool, url: &str) -> Status {
        let ok = Action {
            ok: true,
            why: String::new(),
        };
        Status {
            mode: mode.into(),
            paired,
            server_url: url.into(),
            state_word: "idle".into(),
            save: ok.clone(),
            save_share: ok,
            update_version: Some("9.9.9".into()),
            recent: vec![
                clip(None, Some("LOCAL1"), "In the library"),
                clip(Some("SERVER1"), Some("LOCAL2"), "Sent up"),
                clip(Some("SERVER2"), None, "From connected days"),
            ],
            ..Status::default()
        }
    }

    fn entries(st: &Status) -> Vec<(String, String, bool)> {
        items(st)
            .into_iter()
            .filter_map(|i| match i {
                Item::Act(id, label, on) => Some((id, label, on)),
                Item::Line(id, label) => Some((id, label, false)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn an_obs_the_app_stopped_starting_is_started_again_from_under_the_state_line() {
        let mut st = status("connected", true, "https://clips.example.org");
        let line = |state: &str| crate::obsbox::BoxLine {
            state: state.into(),
            line: "OBS stopped (exit code 3); it stopped 5 times in a row".into(),
            shown: false,
            version: "33.0.1".into(),
            elevated: false,
            without_rights: false,
            can_show: true,
        };
        st.obs_box = Some(line("running"));
        assert!(!ids(&st).contains(&"obs-again".to_string()));
        st.obs_box = Some(line("gave_up"));
        assert_eq!(&ids(&st)[..2], ["state", "obs-again"]);
        assert!(on(&st, "obs-again"));
        assert!(!on(&st, "obs-show"), "no window to open");
    }

    #[test]
    fn pause_clips_sits_with_the_saves_and_paused_greys_them_with_its_reason() {
        for mode in ["connected", "standalone"] {
            let mut st = status(mode, true, "https://clips.example.org");
            let got = entries(&st);
            let at = |got: &[(String, String, bool)], id: &str| {
                got.iter().position(|(i, _, _)| i == id).unwrap()
            };
            let pause = &got[at(&got, "recording")];
            assert_eq!((pause.1.as_str(), pause.2), ("Pause clips", true));
            assert!(at(&got, "recording") > at(&got, "save"), "{got:?}");
            assert!(on(&st, "save"));

            st.recording_paused = true;
            st.state_word = "clips paused".into();
            st.state_line = "the replay buffer is stopped".into();
            let (save, share) =
                crate::status::save_actions(true, true, true, None, true, mode == "standalone");
            st.save = save;
            st.save_share = share;
            let got = entries(&st);
            let resume = &got[at(&got, "recording")];
            assert_eq!((resume.1.as_str(), resume.2), ("Resume clips", true));
            assert!(!on(&st, "save"), "Save clip is greyed");
            if mode == "connected" {
                assert!(!on(&st, "save-share"), "and Save and share");
            }
            assert_eq!(got[at(&got, "save-why")].1, "Clips are paused");
            assert_eq!(got[0].1, "clips paused: the replay buffer is stopped");
            assert_eq!(
                st.tooltip(),
                "reel: clips paused, the replay buffer is stopped"
            );
        }
    }

    #[test]
    fn a_session_recording_starts_and_stops_from_the_tray_which_says_for_how_long() {
        for mode in ["connected", "standalone"] {
            let mut st = status(mode, true, "https://clips.example.org");
            st.record = Action {
                ok: true,
                why: String::new(),
            };
            let got = entries(&st);
            let at = |got: &[(String, String, bool)], id: &str| {
                got.iter().position(|(i, _, _)| i == id).unwrap()
            };
            let record = &got[at(&got, "session")];
            assert_eq!((record.1.as_str(), record.2), ("Record a session", true));
            assert!(
                at(&got, "session") > at(&got, "save")
                    && at(&got, "session") < at(&got, "recording"),
                "with the saves, before Pause clips: {got:?}"
            );
            st.record = Action {
                ok: false,
                why: "OBS is not connected".into(),
            };
            assert!(!on(&st, "session"), "greyed while it cannot start");

            st.session = Some(crate::session::Line {
                since_ms: 1,
                line: "1 h 12 min, started by the hotkey".into(),
                length: "1 h 12 min".into(),
                ..Default::default()
            });
            st.state_word = "recording a session".into();
            st.state_line = "1 h 12 min, started by the hotkey".into();
            let got = entries(&st);
            let stop = &got[at(&got, "session")];
            assert_eq!(
                (stop.1.as_str(), stop.2),
                ("Stop the session recording (1 h 12 min)", true)
            );
            assert_eq!(
                got[0].1,
                "recording a session: 1 h 12 min, started by the hotkey"
            );
            assert_eq!(
                st.tooltip(),
                "reel: recording a session, 1 h 12 min, started by the hotkey"
            );
            assert!(on(&st, "save"), "a replay still saves while it records");

            st.session.as_mut().unwrap().stopping = true;
            assert!(items(&st).iter().any(
                |i| matches!(i, Item::Line(id, l) if id == "session" && l == "Stopping the session recording")
            ));
            st.session = Some(crate::session::Line {
                starting: true,
                ..Default::default()
            });
            assert!(items(&st).iter().any(
                |i| matches!(i, Item::Line(id, l) if id == "session" && l == "Starting the session recording")
            ));
        }
    }

    #[test]
    fn the_recording_dot_is_red_in_the_corner_ringed_and_leaves_the_rest() {
        for size in [16u32, 20, 24, 32, 64] {
            let mut px = vec![0u8; (size * size * 4) as usize];
            for p in px.chunks_mut(4) {
                p.copy_from_slice(&[242, 130, 216, 255]);
            }
            dot(&mut px, size, size);
            let at = |x: u32, y: u32| {
                let i = ((y * size + x) * 4) as usize;
                [px[i], px[i + 1], px[i + 2], px[i + 3]]
            };
            let r = size as f32 * 0.22;
            let ring = (size as f32 / 16.0).max(1.0);
            let c = (size as f32 - r - ring) as u32;
            assert_eq!(
                at(c, c),
                [RED[0], RED[1], RED[2], 255],
                "{size} px: the dot's middle"
            );
            assert_eq!(
                at(1, 1),
                [242, 130, 216, 255],
                "{size} px: the mark is left as it was"
            );
            let edge = (size as f32 - ring / 2.0 - 0.5).round() as u32;
            let e = at(edge, c);
            assert!(
                e[0] < 120 && e[1] < 120,
                "{size} px: a dark ring outside the red: {e:?}"
            );
        }
        let mut clear = vec![0u8; 16 * 16 * 4];
        dot(&mut clear, 16, 16);
        assert_eq!(
            clear[3], 0,
            "a transparent corner of the icon stays transparent"
        );
        let c = ((12 * 16 + 12) * 4) as usize;
        assert_eq!(clear[c + 3], 255, "the dot is opaque on a transparent icon");
    }

    #[test]
    fn connected_the_tray_has_every_link_action() {
        let st = status("connected", true, "https://clips.example.org");
        let got = ids(&st);
        for id in [
            "save",
            "save-share",
            "share",
            "pause",
            "library",
            "update",
            "open:SERVER1",
            "copy:SERVER1",
            "open:SERVER2",
            "copy:SERVER2",
        ] {
            assert!(got.contains(&id.to_string()), "{id} in {got:?}");
        }
        assert!(!got.iter().any(|i| i.contains("LOCAL")), "{got:?}");
        assert!(on(&st, "library") && on(&st, "share"));
        let unset = status("connected", false, "");
        assert!(!on(&unset, "library"), "no server to open yet");
        assert!(!on(&unset, "share"), "not paired");
    }

    #[test]
    fn standalone_the_tray_makes_and_copies_no_link() {
        let st = status("standalone", false, "");
        let got = ids(&st);
        for gone in ["save-share", "share", "pause", "copy:"] {
            assert!(
                !got.iter().any(|i| i.starts_with(gone)),
                "{gone} in {got:?}"
            );
        }
        assert!(
            on(&st, "update"),
            "the release's update installs from the tray as a server's does"
        );
        assert_eq!(
            got.iter()
                .filter(|i| i.starts_with("open:"))
                .collect::<Vec<_>>(),
            ["open:LOCAL1", "open:LOCAL2"],
            "the library's clips on this PC, by their id there"
        );
        assert!(on(&st, "save") && on(&st, "library") && on(&st, "settings"));
        let paired = status("standalone", true, "https://clips.example.org");
        let got = ids(&paired);
        assert!(
            got.contains(&"pause".to_string()),
            "Upload to the server pauses"
        );
        assert!(!got.iter().any(|i| i.starts_with("copy:") || i == "share"));
        let mut cannot = status("standalone", false, "");
        cannot.save = Action {
            ok: false,
            why: "OBS is not connected".into(),
        };
        cannot.save_share = Action {
            ok: false,
            why: "the app is not paired with a server".into(),
        };
        let lines: Vec<String> = items(&cannot)
            .into_iter()
            .filter_map(|i| match i {
                Item::Line(id, l) if id == "save-why" => Some(l),
                _ => None,
            })
            .collect();
        assert_eq!(lines, ["OBS is not connected"]);
        cannot.save.ok = true;
        assert!(
            !items(&cannot)
                .iter()
                .any(|i| matches!(i, Item::Line(id, _) if id == "save-why")),
            "no word about Save and share"
        );
    }

    #[test]
    fn an_install_that_waits_says_so_in_the_tray_and_cannot_be_asked_twice() {
        for mode in ["connected", "standalone"] {
            let mut st = status(mode, true, "https://clips.example.org");
            assert!(on(&st, "update"));
            st.update_installing = true;
            st.update_line = "reel 9.9.9 installs once the library has moved".into();
            let line = items(&st).into_iter().find_map(|i| match i {
                Item::Line(id, l) if id == "update" => Some(l),
                _ => None,
            });
            assert_eq!(
                line.as_deref(),
                Some("reel 9.9.9 installs once the library has moved"),
                "{mode}: the update's line, greyed, its reel in lower case"
            );
            assert!(!on(&st, "update"), "{mode}");
            st.update_line = "installing reel 9.9.9".into();
            assert!(items(&st).iter().any(
                |i| matches!(i, Item::Line(id, l) if id == "update" && l == "Installing reel 9.9.9")
            ));
        }
    }
}
