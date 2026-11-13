//! Settings, Games and apps in the app: the list beside the settings, Steam's games read into
//! it, the name a clip saved now takes, the waits, and what the page asks.
use crate::runtime::{Inner, Runtime};
use jiff::Timestamp;
use reel_desktop_core::gaming::{self, ProcessInfo};
use reel_desktop_core::programs::{
    self, Came, Class, Entry, Every, Front, NameFrom, Naming, Own, Programs,
};
use reel_desktop_core::steam;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

type R<T> = Result<T, String>;

/// Steam on this PC as last read.
#[derive(Debug, Default)]
pub struct SteamSeen {
    pub found: Option<steam::Found>,
    pub error: Option<String>,
    pub reads: u64,
}

/// The list as kept; a file that does not read is put aside and the list starts empty.
pub fn load(file: &Path) -> Programs {
    match Programs::load(file) {
        Ok(p) => p,
        Err(e) => {
            let aside = file.with_extension("json.unread");
            tracing::warn!(error = %e, aside = %aside.display(), "games and apps: the list did not read; it starts empty and the file is kept aside");
            let _ = std::fs::rename(file, &aside);
            Programs::load(file).unwrap_or_default()
        }
    }
}

/// One reel knows is no game: a browser, Explorer, Discord, OBS, reel.
fn no_game(p: &ProcessInfo) -> bool {
    let stem = p
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| {
            p.exe_name
                .to_lowercase()
                .trim_end_matches(".exe")
                .to_string()
        });
    reel_core::games::is_not_a_game(&stem)
}

fn word(c: Came) -> &'static str {
    match c {
        Came::Clip => "a clip was saved in front of it",
        Came::Recording => "a session recording was named after it",
        Came::Fullscreen => "it took the whole screen",
        Came::Hand => "added by hand",
        Came::Steam => "from Steam",
        Came::SteamShortcut => "added to Steam by hand",
    }
}

impl Inner {
    fn keep_programs(&self) {
        if let Err(e) = self.programs.save() {
            tracing::warn!(error = %e, "games and apps: the list was not written");
        }
    }

    /// A program not in the list comes in as Not sorted yet, named from its file.
    fn take_in(&mut self, p: &ProcessInfo, came: Came) -> Option<String> {
        if p.path.as_os_str().is_empty() || no_game(p) {
            return None;
        }
        let guess = gaming::game_name(&p.exe_name, &p.title)?;
        let e = Entry::program(&p.path, &guess, Class::NotSorted, came, Timestamp::now());
        let id = e.id.clone();
        if self.programs.add(e).is_ok() {
            self.programs.seen(&id, Timestamp::now());
            tracing::info!(program = %p.path.display(), name = %guess, "games and apps: it came in as Not sorted yet: {}", word(came));
            self.keep_programs();
        }
        Some(id)
    }

    /// The name a clip or a session recording saved now takes: the program in front as the
    /// list names it, else OBS's game capture window's as the list names it, else as before
    /// (OBS's window, then a name from the program in front), and a program in front that is
    /// not in the list comes in as Not sorted yet.
    pub(crate) fn name_in_front(&mut self, came: Came) -> Option<String> {
        let front = self.foreground.foreground_process();
        let now = Timestamp::now();
        if let Some(p) = &front {
            if let Some(e) = self.programs.find(&p.path) {
                let (id, class) = (e.id.clone(), e.class);
                let named = match e.naming {
                    Naming::Window => {
                        reel_core::games::clean(Some(&p.title)).or_else(|| Some(e.name.clone()))
                    }
                    Naming::Name => Some(e.name.clone()),
                };
                if self.programs.seen(&id, now) {
                    self.keep_programs();
                }
                if class != Class::Ignored {
                    return named;
                }
            }
        }
        let by_window = self.obs_game_exe.as_deref().and_then(|exe| {
            let listed: Vec<&Entry> = self
                .programs
                .entries
                .iter()
                .filter(|e| {
                    !e.folder
                        && e.path
                            .file_name()
                            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(exe))
                })
                .collect();
            match listed[..] {
                [one] if one.class != Class::Ignored => Some(one.name.clone()),
                _ => None,
            }
        });
        if by_window.is_some() {
            return by_window;
        }
        let guess = front
            .as_ref()
            .filter(|p| self.programs.find(&p.path).is_none())
            .and_then(|p| {
                self.take_in(p, came);
                gaming::game_name(&p.exe_name, &p.title)
            });
        self.obs_game.clone().or(guess)
    }

    /// Whether uploads and the library's heavy work wait now; the program in front is seen,
    /// and one not in the list that takes the whole screen comes in as Not sorted yet.
    pub(crate) fn front_waits(&mut self, front: Option<&ProcessInfo>, fullscreen: bool) -> bool {
        let every = Every {
            wait: self.settings.upload.wait_for_games,
            ..Every::default()
        };
        let Some(p) = front else {
            let f = if fullscreen {
                Front::Unknown
            } else {
                Front::Nothing
            };
            return programs::waits(&every, f, fullscreen);
        };
        if self.programs.find(&p.path).is_none() {
            if no_game(p) {
                return false;
            }
            if fullscreen {
                self.take_in(p, Came::Fullscreen);
            }
        }
        match self.programs.find(&p.path).map(|e| e.id.clone()) {
            Some(id) => {
                if self.programs.seen(&id, Timestamp::now()) {
                    self.keep_programs();
                }
                let e = self.programs.get(&id).expect("just found");
                programs::waits(&every, Front::Listed(e), fullscreen)
            }
            None => programs::waits(&every, Front::Unknown, fullscreen),
        }
    }

    fn steam_name(&self, exe: &Path) -> Option<String> {
        let f = self.steam.found.as_ref()?;
        f.apps
            .iter()
            .filter(|a| a.kind != steam::SteamKind::Shortcut)
            .find(|a| {
                let mut e =
                    Entry::program(&a.path, "", Class::Game, Came::Steam, Timestamp::UNIX_EPOCH);
                e.folder = true;
                e.holds(exe)
            })
            .map(|a| a.name.clone())
    }

    /// The clips and session recordings of this PC named after each name.
    fn named(&self) -> HashMap<String, (usize, usize)> {
        let mut n: HashMap<String, (usize, usize)> = HashMap::new();
        for c in &self.ledger.clips {
            if let Some(g) = &c.game {
                let at = n.entry(g.clone()).or_default();
                if c.recording {
                    at.1 += 1;
                } else {
                    at.0 += 1;
                }
            }
        }
        n
    }

    fn view(&self, e: &Entry, named: &HashMap<String, (usize, usize)>) -> Value {
        let (clips, recordings) = named.get(&e.name).copied().unwrap_or_default();
        let mut v = serde_json::to_value(e).unwrap_or(Value::Null);
        v["clips"] = json!(clips);
        v["recordings"] = json!(recordings);
        v["came_words"] = json!(word(e.came));
        v
    }

    fn steam_view(&self) -> Value {
        let s = &self.steam;
        let from: Vec<&Entry> = self
            .programs
            .entries
            .iter()
            .filter(|e| matches!(e.came, Came::Steam | Came::SteamShortcut))
            .collect();
        let last = from
            .iter()
            .filter(|e| e.fresh)
            .max_by_key(|e| e.came_at)
            .map(|e| json!({ "name": e.name, "at": e.came_at }));
        let names = |k: steam::SteamKind| -> Vec<String> {
            s.found
                .as_ref()
                .map(|f| {
                    f.apps
                        .iter()
                        .filter(|a| a.kind == k)
                        .map(|a| a.name.clone())
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut games = names(steam::SteamKind::Game);
        games.sort_by_key(|n| n.to_lowercase());
        json!({
            "on": self.settings.games.steam,
            "read": s.reads > 0,
            "found": s.found.is_some(),
            "error": s.error,
            "libraries": s.found.as_ref().map(|f| f.libraries.len()).unwrap_or(0),
            "games": games,
            "applications": names(steam::SteamKind::Application),
            "shortcuts": names(steam::SteamKind::Shortcut),
            "out": s.found.as_ref().map(|f| f.out.len()).unwrap_or(0),
            "in_list": from.len(),
            "last": last,
        })
    }
}

/// A change on a program's page; each field that is there changes.
#[derive(Debug, Default, Deserialize)]
pub struct Change {
    pub class: Option<Class>,
    pub naming: Option<Naming>,
    pub wait: Option<Own>,
}

fn clean_name(name: &str) -> R<String> {
    let n: String = name.trim().chars().filter(|c| !c.is_control()).collect();
    if n.is_empty() {
        return Err("a name cannot be empty".into());
    }
    if n.chars().count() > 80 {
        return Err("a name has at most 80 characters".into());
    }
    Ok(n)
}

impl Runtime {
    pub(crate) fn spawn_steam(&self) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        self.lock().steam_wake = Some(tx.clone());
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut watching: Option<(Vec<PathBuf>, CancellationToken)> = None;
            loop {
                let dirs = rt.read_steam().await;
                if watching.as_ref().map(|w| &w.0) != Some(&dirs) {
                    if let Some((_, c)) = watching.take() {
                        c.cancel();
                    }
                    if !dirs.is_empty() {
                        let c = CancellationToken::new();
                        let w = steam::watch(
                            dirs.clone(),
                            Duration::from_secs(3),
                            tx.clone(),
                            c.clone(),
                        );
                        tauri::async_runtime::spawn(w);
                        watching = Some((dirs, c));
                    }
                }
                // an hourly look, for Steam installed later or a library on a drive that comes later
                tokio::select! {
                    got = rx.recv() => if got.is_none() { return },
                    _ = tokio::time::sleep(Duration::from_secs(3600)) => {}
                }
                while rx.try_recv().is_ok() {}
            }
        });
    }

    /// Reads Steam again soon.
    pub(crate) fn steam_now(&self) {
        if let Some(tx) = &self.lock().steam_wake {
            let _ = tx.send(());
        }
    }

    async fn read_steam(&self) -> Vec<PathBuf> {
        let read = tauri::async_runtime::spawn_blocking(|| {
            steam::locate().map(|d| steam::read(&d).map_err(|e| format!("{}: {e}", d.display())))
        })
        .await
        .ok()
        .flatten();
        let now = Timestamp::now();
        let mut g = self.lock();
        let dirs = match read {
            None => {
                if g.steam.reads == 0 || g.steam.found.is_some() {
                    tracing::info!(
                        "Steam read: Steam is not installed on this PC, or reel did not find it"
                    );
                }
                g.steam.found = None;
                g.steam.error = None;
                Vec::new()
            }
            Some(Err(e)) => {
                tracing::warn!("Steam read: its files did not read: {e}");
                g.steam.error = Some(e);
                Vec::new()
            }
            Some(Ok(f)) => {
                tracing::info!("{}", f.log_line());
                let pending = g.settings.obs.setup_opens(g.bundled.is_some());
                if !g.settings.games.steam {
                    tracing::info!(
                        "Steam read: nothing added, Add my Steam games to the list is off"
                    );
                } else if pending {
                    tracing::info!("Steam read: nothing added until the setup is finished");
                } else {
                    let took = g.programs.take_steam(&f.apps, now);
                    if !took.added.is_empty() || took.uninstalled > 0 {
                        tracing::info!(
                            "Steam read: {} new in the list ({}), {} no longer installed",
                            took.added.len(),
                            took.added.join(", "),
                            took.uninstalled
                        );
                    }
                    g.keep_programs();
                }
                let dirs = steam::watched(&f);
                g.steam.found = Some(f);
                g.steam.error = None;
                dirs
            }
        };
        g.steam.reads += 1;
        drop(g);
        self.push_status();
        dirs
    }

    /// Settings, Games and apps' page: the list, Steam, and what its words depend on.
    pub fn programs_page(&self) -> Value {
        let g = self.lock();
        let named = g.named();
        let entries: Vec<Value> = g
            .programs
            .entries
            .iter()
            .map(|e| g.view(e, &named))
            .collect();
        json!({
            "entries": entries,
            "not_again": g.programs.not_again,
            "steam": g.steam_view(),
            "standalone": g.settings.mode == reel_desktop_core::settings::Mode::Standalone,
            "paired": g.api.is_some(),
            "linux": cfg!(not(windows)),
            "waits_on": g.settings.upload.wait_for_games,
        })
    }

    pub async fn steam_page(&self) -> Value {
        for _ in 0..100 {
            if self.lock().steam.reads > 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        self.lock().steam_view()
    }

    fn entry_view(&self, id: &str) -> R<Value> {
        let g = self.lock();
        let e = g
            .programs
            .get(id)
            .ok_or("that program is no longer in the list")?;
        Ok(g.view(e, &g.named()))
    }

    pub fn program_set(&self, id: &str, c: Change) -> R<Value> {
        {
            let mut g = self.lock();
            let e = g
                .programs
                .get_mut(id)
                .ok_or("that program is no longer in the list")?;
            if let Some(class) = c.class {
                e.class = class;
            }
            if let Some(n) = c.naming {
                e.naming = n;
            }
            if let Some(w) = c.wait {
                e.own.wait = w;
            }
            tracing::info!(name = %e.name, class = ?e.class, naming = ?e.naming, wait = ?e.own.wait, "games and apps: a program changed");
            g.programs.save().map_err(|e| e.to_string())?;
        }
        self.push_status();
        self.entry_view(id)
    }

    /// A program by hand: one that runs now or a file picked.
    pub fn program_add(&self, path: &str, name: &str, class: Class) -> R<Value> {
        let path = gaming::program_of(Path::new(path.trim()))?;
        if !path.is_absolute() {
            return Err("pick the program's file".into());
        }
        let name = clean_name(name)?;
        let id = {
            let mut g = self.lock();
            let guess = gaming::name_of(&path, "");
            let mut e = Entry::program(&path, &guess, class, Came::Hand, Timestamp::now());
            e.name_from = if g.steam_name(&path).as_deref() == Some(name.as_str()) {
                NameFrom::Steam
            } else if name == guess {
                NameFrom::Guess
            } else {
                NameFrom::Hand
            };
            e.name = name;
            let id = e.id.clone();
            g.programs.add(e)?;
            g.programs.save().map_err(|e| e.to_string())?;
            tracing::info!(program = %path.display(), class = ?class, "games and apps: a program was added by hand");
            id
        };
        self.push_status();
        self.entry_view(&id)
    }

    pub fn program_remove(&self, id: &str) -> R<()> {
        {
            let mut g = self.lock();
            let e = g
                .programs
                .remove(id)
                .ok_or("that program is no longer in the list")?;
            tracing::info!(name = %e.name, steam = ?e.steam_app, "games and apps: removed from the list");
            g.programs.save().map_err(|e| e.to_string())?;
        }
        self.push_status();
        Ok(())
    }

    pub fn program_add_back(&self, app: u32) -> R<()> {
        {
            let mut g = self.lock();
            if !g.programs.add_back(app) {
                return Err("that game is not among those not added again".into());
            }
            g.programs.save().map_err(|e| e.to_string())?;
        }
        tracing::info!(app, "games and apps: Steam may add it again");
        self.steam_now();
        self.push_status();
        Ok(())
    }

    /// The programs that run now, for Add a program: each with the name it would get and,
    /// when it is in the list, its entry.
    pub fn programs_running(&self) -> Vec<Value> {
        let now = gaming::running_now();
        let g = self.lock();
        now.into_iter()
            .map(|p| {
                let steam = g.steam_name(&p.path);
                let listed = g.programs.find(&p.path);
                let name = match (listed, &steam) {
                    (Some(e), _) => e.name.clone(),
                    (None, Some(s)) => s.clone(),
                    (None, None) => gaming::name_of(&p.path, &p.title),
                };
                json!({
                    "path": p.path,
                    "title": p.title,
                    "name": name,
                    "steam": steam.is_some(),
                    "guess": gaming::name_of(&p.path, ""),
                    "listed": listed.map(|e| e.id.clone()),
                })
            })
            .collect()
    }

    /// A file picked for Add a program: the program it is, and the name it would get.
    pub fn program_picked(&self, file: &Path) -> R<Value> {
        let path = gaming::program_of(file)?;
        let g = self.lock();
        let steam = g.steam_name(&path);
        Ok(json!({
            "path": path,
            "name": steam.clone().unwrap_or_else(|| gaming::name_of(&path, "")),
            "steam": steam.is_some(),
            "guess": gaming::name_of(&path, ""),
            "listed": g.programs.find(&path).map(|e| e.id.clone()),
        }))
    }

    /// What a rename would change: the clips and recordings of this PC named after it, two of
    /// the titles before and after, and those a server has.
    pub fn rename_plan(&self, id: &str, name: &str) -> R<Value> {
        let name = clean_name(name)?;
        let g = self.lock();
        let old = g
            .programs
            .get(id)
            .ok_or("that program is no longer in the list")?
            .name
            .clone();
        let older: Vec<_> = g
            .ledger
            .clips
            .iter()
            .filter(|c| c.game.as_deref() == Some(old.as_str()))
            .collect();
        let mut examples = Vec::new();
        for c in older.iter().rev() {
            if let (Some(t), Some(at)) = (&c.title, c.recorded_at) {
                if let Some(now) = programs::retitle(t, &old, &name, at) {
                    examples.push(json!([t, now]));
                }
            }
            if examples.len() == 2 {
                break;
            }
        }
        Ok(json!({
            "old": old,
            "name": name,
            "clips": older.iter().filter(|c| !c.recording).count(),
            "recordings": older.iter().filter(|c| c.recording).count(),
            "examples": examples,
            "on_server": older.iter().filter(|c| c.clip_id.is_some()).count(),
            "standalone": g.settings.mode == reel_desktop_core::settings::Mode::Standalone,
        }))
    }

    /// The program's new name; with `older`, the clips and recordings of this PC named after
    /// the old one take the new one, and the titles reel made them change with it, here and in
    /// the library on this PC.
    pub async fn rename(&self, id: &str, name: &str, older: bool) -> R<Value> {
        let name = clean_name(name)?;
        let (old, moved) = {
            let mut g = self.lock();
            let e = g
                .programs
                .get_mut(id)
                .ok_or("that program is no longer in the list")?;
            let old = std::mem::replace(&mut e.name, name.clone());
            e.name_from = NameFrom::Hand;
            g.programs.save().map_err(|e| e.to_string())?;
            let moved = if older && old != name {
                let m = g.ledger.rename_game(&old, &name);
                if let Err(e) = g.ledger.save() {
                    tracing::warn!(error = %e, "ledger");
                }
                m
            } else {
                Vec::new()
            };
            (old, moved)
        };
        let mut library = 0;
        if let Some(local) = self.local() {
            let store = &local.ctx.store;
            for (clip, recorded) in moved
                .iter()
                .filter_map(|(c, at)| c.clone().map(|c| (c, *at)))
            {
                let Ok(Some(c)) = store.clip(&clip).await else {
                    continue;
                };
                if c.game.as_deref().map(str::trim) == Some(old.as_str()) {
                    let _ = store.set_clip_game(&clip, Some(&name)).await;
                }
                let title = recorded.and_then(|at| programs::retitle(&c.title, &old, &name, at));
                if let Some(t) = &title {
                    let _ = store.update_clip_text(&clip, Some(t), None).await;
                }
                library += 1;
                local.ctx.emit(reel_core::events::Event::Clip {
                    id: clip.clone(),
                    state: "updated".into(),
                    title: title.unwrap_or(c.title),
                });
            }
        }
        tracing::info!(
            from = %old,
            to = %name,
            older = moved.len(),
            library,
            "games and apps: renamed"
        );
        self.push_status();
        self.entry_view(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_trimmed_and_kept_short() {
        assert_eq!(clean_name("  Ember Pass ").unwrap(), "Ember Pass");
        assert!(clean_name("  ").is_err());
        assert!(clean_name(&"x".repeat(81)).is_err());
        assert_eq!(clean_name("a\u{7}b").unwrap(), "ab");
    }

    #[test]
    fn the_programs_reel_knows_are_no_game_never_come_in() {
        let p = |path: &str| ProcessInfo {
            exe_name: Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            title: String::new(),
            path: PathBuf::from(path),
        };
        assert!(no_game(&p("/apps/firefox")));
        assert!(no_game(&p("/apps/Discord.exe")));
        assert!(!no_game(&p("/games/emberpass")));
    }
}
