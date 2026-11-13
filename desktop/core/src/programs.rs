//! Settings, Games and apps: the programs reel has seen, each with a name its clips take, a
//! class and its own settings, and what they come to with the settings for every game.
use crate::steam::{SteamApp, SteamKind};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Game,
    NotAGame,
    Ignored,
    /// New in the list, acting as an unknown program did before there was one. A class of a
    /// later version reads as this one.
    #[default]
    #[serde(other)]
    NotSorted,
}

/// One program's own setting for a behaviour: as its class gives it, or on or off for it alone.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Own {
    On,
    Off,
    #[default]
    #[serde(other)]
    AsClass,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Naming {
    /// After its window's title, for an emulator or a launcher that shows the game it runs.
    Window,
    #[default]
    #[serde(other)]
    Name,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NameFrom {
    Steam,
    Hand,
    #[default]
    #[serde(other)]
    Guess,
}

/// How a program came into the list.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Came {
    Recording,
    Fullscreen,
    Hand,
    Steam,
    /// A program added to Steam by hand, outside its store.
    SteamShortcut,
    #[default]
    #[serde(other)]
    Clip,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct OwnSettings {
    pub buffer: Own,
    pub record: Own,
    pub stop: Own,
    pub wait: Own,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Entry {
    pub id: String,
    /// The program, or a Steam app's folder, where every program inside counts as this one.
    pub path: PathBuf,
    pub folder: bool,
    pub steam_app: Option<u32>,
    /// Steam calls it an application, not a game.
    pub application: bool,
    pub name: String,
    pub name_from: NameFrom,
    /// The name its file gives it, for the page's "guessed from its file".
    pub guess: String,
    pub class: Class,
    pub own: OwnSettings,
    pub naming: Naming,
    pub came: Came,
    pub came_at: Timestamp,
    pub seen_at: Option<Timestamp>,
    /// Came from Steam after its first read, and not seen yet.
    pub fresh: bool,
    /// A Steam app no longer installed: it stays, and its clips keep the name.
    pub uninstalled: bool,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            id: String::new(),
            path: PathBuf::new(),
            folder: false,
            steam_app: None,
            application: false,
            name: String::new(),
            name_from: NameFrom::Guess,
            guess: String::new(),
            class: Class::NotSorted,
            own: OwnSettings::default(),
            naming: Naming::Name,
            came: Came::Clip,
            came_at: Timestamp::UNIX_EPOCH,
            seen_at: None,
            fresh: false,
            uninstalled: false,
        }
    }
}

impl Entry {
    /// A program by its file, its name guessed from it.
    pub fn program(path: &Path, guess: &str, class: Class, came: Came, now: Timestamp) -> Entry {
        Entry {
            id: id_of(path),
            path: path.to_path_buf(),
            name: guess.to_string(),
            guess: guess.to_string(),
            class,
            came,
            came_at: now,
            ..Entry::default()
        }
    }

    /// What its class and its own settings give each behaviour, with these settings for every
    /// game.
    pub fn resolve(&self, every: &Every) -> Resolved {
        resolve(self.class, &self.own, every)
    }

    pub fn holds(&self, exe: &Path) -> bool {
        if self.folder {
            within(&self.path, exe)
        } else {
            key(&self.path) == key(exe)
        }
    }
}

/// The settings for every game, each set on the page of its subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Every {
    /// The replay buffer always (true), or only while a game runs.
    pub buffer_always: bool,
    pub record: bool,
    pub stop: bool,
    /// Uploads and the library's heavy work wait while a game has the screen.
    pub wait: bool,
}

impl Default for Every {
    fn default() -> Self {
        Self {
            buffer_always: true,
            record: false,
            stop: true,
            wait: false,
        }
    }
}

/// The replay buffer while a program runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Buffer {
    Runs,
    /// Only while it has the whole screen: a program not sorted yet.
    InFullscreen,
    NotStarted,
    /// Stopped while it runs, whatever the others want: its own Off.
    Stops,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    No,
    /// While its window is in front, a borderless one too.
    InFront,
    /// While it is in front in full screen: a program not sorted yet, as before the list.
    InFullscreen,
}

/// What one program comes to: every game, then its class, then its own setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    pub buffer: Buffer,
    pub record: bool,
    pub stop: bool,
    pub wait: Wait,
    /// Clips saved in front of it take its name.
    pub names: bool,
}

pub fn resolve(class: Class, own: &OwnSettings, every: &Every) -> Resolved {
    if class == Class::Ignored {
        return Resolved {
            buffer: if every.buffer_always {
                Buffer::Runs
            } else {
                Buffer::NotStarted
            },
            record: false,
            stop: false,
            wait: Wait::No,
            names: false,
        };
    }
    let buffer = match (own.buffer, every.buffer_always, class) {
        (Own::Off, _, _) => Buffer::Stops,
        (Own::On, _, _) | (_, true, _) | (_, false, Class::Game) => Buffer::Runs,
        (_, false, Class::NotSorted) => Buffer::InFullscreen,
        _ => Buffer::NotStarted,
    };
    let game = class == Class::Game;
    let either = |own: Own, by_class: bool| match own {
        Own::On => true,
        Own::Off => false,
        Own::AsClass => by_class,
    };
    let wait = match (own.wait, class) {
        (Own::On, _) => Wait::InFront,
        (Own::Off, _) => Wait::No,
        (_, Class::Game) if every.wait => Wait::InFront,
        (_, Class::NotSorted) if every.wait => Wait::InFullscreen,
        _ => Wait::No,
    };
    Resolved {
        buffer,
        record: either(own.record, game && every.record),
        stop: either(own.stop, game && every.stop),
        wait,
        names: true,
    }
}

/// A program that runs, as the rules for several programs at once see it.
#[derive(Debug, Clone, Copy)]
pub struct Running<'a> {
    pub entry: &'a Entry,
    /// It is in front, in full screen.
    pub fullscreen: bool,
}

/// Whether the replay buffer runs: always, unless a running program stops it; only while a game
/// runs, when a running program keeps it and none stops it.
pub fn buffer_runs(every: &Every, running: &[Running]) -> bool {
    let mut kept = every.buffer_always;
    for r in running {
        match r.entry.resolve(every).buffer {
            Buffer::Stops => return false,
            Buffer::Runs => kept = true,
            Buffer::InFullscreen if r.fullscreen => kept = true,
            _ => {}
        }
    }
    kept
}

/// The running program a session recording starts with, if one records by itself: one
/// recording, whatever runs.
pub fn records<'a>(every: &Every, running: &[Running<'a>]) -> Option<&'a Entry> {
    running
        .iter()
        .find(|r| r.entry.resolve(every).record)
        .map(|r| r.entry)
}

/// The running programs a session recording stops with: it stops once all of them closed.
pub fn stoppers<'a>(every: &Every, running: &[Running<'a>]) -> Vec<&'a Entry> {
    running
        .iter()
        .filter(|r| r.entry.resolve(every).stop)
        .map(|r| r.entry)
        .collect()
}

/// The program in front, as the waits see it.
#[derive(Debug, Clone, Copy)]
pub enum Front<'a> {
    Nothing,
    /// One reel knows is no game: a browser, Explorer, Discord, OBS, reel.
    NoGame,
    Listed(&'a Entry),
    /// Not in the list, or one that could not be read: as before the list, it counts as a game
    /// while it has the whole screen.
    Unknown,
}

/// Whether uploads and the library's heavy work wait now.
pub fn waits(every: &Every, front: Front, fullscreen: bool) -> bool {
    let wait = match front {
        Front::Nothing | Front::NoGame => return false,
        Front::Unknown => resolve(Class::NotSorted, &OwnSettings::default(), every).wait,
        Front::Listed(e) => e.resolve(every).wait,
    };
    match wait {
        Wait::No => false,
        Wait::InFront => true,
        Wait::InFullscreen => fullscreen,
    }
}

/// Whether a reel made title of a clip named after `old` is one that a rename gives `new`: the
/// title reel wrote ("Ember Pass, Nov 9, 21:14", with the year in another year), not one
/// changed by hand.
pub fn retitle(title: &str, old: &str, new: &str, recorded: Timestamp) -> Option<String> {
    let rest = title.strip_prefix(old)?.strip_prefix(", ")?;
    let d = recorded.to_zoned(jiff::tz::TimeZone::system()).datetime();
    let without = reel_core::names::time_title_in(&d, d.year());
    let with = reel_core::names::time_title_in(&d, d.year().saturating_add(1));
    (rest == without || rest == with).then(|| format!("{}, {rest}", new.trim()))
}

/// The list as it is kept, in `programs.json` beside the settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct Programs {
    pub entries: Vec<Entry>,
    /// Steam apps removed by hand, so Steam does not add them back.
    pub not_again: Vec<NotAgain>,
    /// Steam was read into the list before: later ones are new.
    pub steam_read: bool,
    #[serde(skip)]
    file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct NotAgain {
    pub app: u32,
    pub name: String,
}

/// What a read of Steam changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Took {
    /// The names that came in.
    pub added: Vec<String>,
    pub uninstalled: usize,
}

impl Programs {
    pub fn load(file: &Path) -> anyhow::Result<Programs> {
        let mut p: Programs = match std::fs::read_to_string(file) {
            Ok(text) => serde_json::from_str(&text)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Programs::default(),
            Err(e) => return Err(e.into()),
        };
        p.file = Some(file.to_path_buf());
        Ok(p)
    }

    /// Written as a part file and renamed into place, so a crash leaves the old file.
    pub fn save(&self) -> anyhow::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let part = crate::settings::part_path(file);
        std::fs::write(&part, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&part, file)?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// The entry a running program is: its own file first, then a Steam folder that holds it.
    pub fn find(&self, exe: &Path) -> Option<&Entry> {
        let i = self.position(exe)?;
        Some(&self.entries[i])
    }

    pub fn find_mut(&mut self, exe: &Path) -> Option<&mut Entry> {
        let i = self.position(exe)?;
        Some(&mut self.entries[i])
    }

    fn position(&self, exe: &Path) -> Option<usize> {
        let k = key(exe);
        self.entries
            .iter()
            .position(|e| !e.folder && key(&e.path) == k)
            .or_else(|| {
                self.entries
                    .iter()
                    .position(|e| e.folder && within(&e.path, exe))
            })
    }

    pub fn unsorted(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.class == Class::NotSorted)
            .count()
    }

    /// Adds an entry, refused when its program or folder is in the list already.
    pub fn add(&mut self, entry: Entry) -> Result<&Entry, String> {
        let k = key(&entry.path);
        if let Some(e) = self.entries.iter().find(|e| key(&e.path) == k) {
            return Err(format!("{} is in the list already", e.name));
        }
        if let Some(app) = entry.steam_app {
            self.not_again.retain(|n| n.app != app);
        }
        self.entries.push(entry);
        Ok(self.entries.last().expect("just added"))
    }

    /// Takes it out; a Steam app is remembered, so Steam does not add it again.
    pub fn remove(&mut self, id: &str) -> Option<Entry> {
        let i = self.entries.iter().position(|e| e.id == id)?;
        let e = self.entries.remove(i);
        if let Some(app) = e.steam_app {
            if !self.not_again.iter().any(|n| n.app == app) {
                self.not_again.push(NotAgain {
                    app,
                    name: e.name.clone(),
                });
            }
        }
        Some(e)
    }

    /// Steam may add it again; it comes in at the next read.
    pub fn add_back(&mut self, app: u32) -> bool {
        let before = self.not_again.len();
        self.not_again.retain(|n| n.app != app);
        self.not_again.len() != before
    }

    /// Steam's installed apps: new ones come in, one taken out by hand stays out, one no longer
    /// installed stays with its name, and one installed again is the same entry.
    pub fn take_steam(&mut self, apps: &[SteamApp], now: Timestamp) -> Took {
        let mut took = Took::default();
        let fresh = self.steam_read;
        for app in apps {
            if self.not_again.iter().any(|n| n.app == app.app_id) {
                continue;
            }
            let shortcut = app.kind == SteamKind::Shortcut;
            if let Some(e) = self
                .entries
                .iter_mut()
                .find(|e| e.steam_app == Some(app.app_id))
            {
                e.uninstalled = false;
                e.path = app.path.clone();
                if e.name_from == NameFrom::Steam {
                    e.name = app.name.clone();
                }
                continue;
            }
            let k = key(&app.path);
            if self.entries.iter().any(|e| key(&e.path) == k) {
                continue;
            }
            let class = match app.kind {
                SteamKind::Game => Class::Game,
                SteamKind::Application | SteamKind::Shortcut => Class::NotSorted,
            };
            self.entries.push(Entry {
                id: if shortcut {
                    id_of(&app.path)
                } else {
                    format!("steam-{}", app.app_id)
                },
                path: app.path.clone(),
                folder: !shortcut,
                steam_app: Some(app.app_id),
                application: app.kind == SteamKind::Application,
                name: app.name.clone(),
                name_from: NameFrom::Steam,
                guess: app.name.clone(),
                class,
                came: if shortcut {
                    Came::SteamShortcut
                } else {
                    Came::Steam
                },
                came_at: now,
                fresh,
                ..Entry::default()
            });
            took.added.push(app.name.clone());
        }
        for e in self.entries.iter_mut() {
            if e.folder
                && !e.uninstalled
                && e.steam_app
                    .is_some_and(|id| !apps.iter().any(|a| a.app_id == id))
            {
                e.uninstalled = true;
                took.uninstalled += 1;
            }
        }
        self.steam_read = true;
        took
    }

    /// Seen in front now; true when that is news worth saving: the first time, or a new day.
    pub fn seen(&mut self, id: &str, now: Timestamp) -> bool {
        let Some(e) = self.get_mut(id) else {
            return false;
        };
        let day = |t: Timestamp| t.as_second() / 86_400;
        let news = e.seen_at.is_none_or(|t| day(t) != day(now) || e.fresh);
        e.seen_at = Some(now);
        e.fresh = false;
        news
    }
}

/// A short id from the path, the same for the same program.
pub fn id_of(path: &Path) -> String {
    let h = crate::hash::sha256_hex(key(path).as_bytes());
    format!("p-{}", &h[..12])
}

/// A path as compared: forward slashes, no trailing one, and on Windows in small letters.
fn key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.to_string()
    }
}

fn within(dir: &Path, file: &Path) -> bool {
    let d = key(dir);
    !d.is_empty()
        && key(file)
            .strip_prefix(&d)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Class; 4] = [
        Class::Game,
        Class::NotAGame,
        Class::NotSorted,
        Class::Ignored,
    ];

    fn every(buffer_always: bool, record: bool, stop: bool, wait: bool) -> Every {
        Every {
            buffer_always,
            record,
            stop,
            wait,
        }
    }

    fn entry(path: &str, class: Class) -> Entry {
        Entry::program(
            Path::new(path),
            "A Program",
            class,
            Came::Clip,
            Timestamp::UNIX_EPOCH,
        )
    }

    #[test]
    fn the_replay_buffer_by_class_with_only_while_a_game_runs_and_with_always() {
        let none = OwnSettings::default();
        let games = every(false, false, true, false);
        let by = |c| resolve(c, &none, &games).buffer;
        assert_eq!(by(Class::Game), Buffer::Runs, "runs while it runs");
        assert_eq!(by(Class::NotAGame), Buffer::NotStarted);
        assert_eq!(
            by(Class::NotSorted),
            Buffer::InFullscreen,
            "while it has the whole screen"
        );
        assert_eq!(by(Class::Ignored), Buffer::NotStarted);
        let always = every(true, false, true, false);
        for c in ALL {
            assert_eq!(resolve(c, &none, &always).buffer, Buffer::Runs, "{c:?}");
        }
    }

    #[test]
    fn record_by_itself_and_stop_when_it_closes_by_class() {
        let none = OwnSettings::default();
        for (record, stop) in [(false, true), (true, false), (true, true), (false, false)] {
            let e = every(true, record, stop, false);
            let game = resolve(Class::Game, &none, &e);
            assert_eq!(
                (game.record, game.stop),
                (record, stop),
                "a Game as every game"
            );
            for c in [Class::NotAGame, Class::NotSorted, Class::Ignored] {
                let r = resolve(c, &none, &e);
                assert_eq!((r.record, r.stop), (false, false), "{c:?} is off");
            }
        }
    }

    #[test]
    fn the_waits_by_class_off_by_default_and_on() {
        let none = OwnSettings::default();
        let off = Every::default();
        for c in ALL {
            assert_eq!(
                resolve(c, &none, &off).wait,
                Wait::No,
                "{c:?}: off by default"
            );
        }
        let on = every(true, false, true, true);
        assert_eq!(resolve(Class::Game, &none, &on).wait, Wait::InFront);
        assert_eq!(
            resolve(Class::NotSorted, &none, &on).wait,
            Wait::InFullscreen
        );
        assert_eq!(resolve(Class::NotAGame, &none, &on).wait, Wait::No);
        assert_eq!(resolve(Class::Ignored, &none, &on).wait, Wait::No);
    }

    #[test]
    fn clips_take_the_name_of_every_class_but_ignored() {
        let none = OwnSettings::default();
        for c in ALL {
            assert_eq!(
                resolve(c, &none, &Every::default()).names,
                c != Class::Ignored,
                "{c:?}"
            );
        }
    }

    #[test]
    fn each_own_setting_replaces_every_game_and_the_class() {
        for c in [Class::Game, Class::NotAGame, Class::NotSorted] {
            for e in [
                every(true, false, true, false),
                every(false, true, false, true),
            ] {
                let on = |f: fn(&mut OwnSettings)| {
                    let mut own = OwnSettings::default();
                    f(&mut own);
                    resolve(c, &own, &e)
                };
                assert_eq!(on(|o| o.buffer = Own::On).buffer, Buffer::Runs, "{c:?}");
                assert_eq!(on(|o| o.buffer = Own::Off).buffer, Buffer::Stops, "{c:?}");
                assert!(on(|o| o.record = Own::On).record, "{c:?}");
                assert!(!on(|o| o.record = Own::Off).record, "{c:?}");
                assert!(on(|o| o.stop = Own::On).stop, "{c:?}");
                assert!(!on(|o| o.stop = Own::Off).stop, "{c:?}");
                assert_eq!(on(|o| o.wait = Own::On).wait, Wait::InFront, "{c:?}");
                assert_eq!(on(|o| o.wait = Own::Off).wait, Wait::No, "{c:?}");
                let own = OwnSettings::default();
                assert_eq!(on(|_| {}), resolve(c, &own, &e), "as its class");
            }
        }
        let mut own = OwnSettings {
            buffer: Own::Off,
            record: Own::On,
            stop: Own::On,
            wait: Own::On,
        };
        let ignored = resolve(Class::Ignored, &own, &every(true, true, true, true));
        assert_eq!(
            (ignored.record, ignored.stop, ignored.wait, ignored.buffer),
            (false, false, Wait::No, Buffer::Runs),
            "Ignored: as if it were not there, its own settings kept for when it is sorted again"
        );
        own.buffer = Own::On;
        assert_eq!(
            resolve(Class::Ignored, &own, &every(false, false, false, false)).buffer,
            Buffer::NotStarted
        );
    }

    fn run(entry: &Entry, fullscreen: bool) -> Running<'_> {
        Running { entry, fullscreen }
    }

    #[test]
    fn several_programs_the_buffer_and_an_off_that_wins() {
        let games = every(false, false, true, false);
        let always = Every::default();
        let game = entry("/g/game", Class::Game);
        let tool = entry("/g/tool", Class::NotAGame);
        let new = entry("/g/new", Class::NotSorted);
        let mut heavy = entry("/g/heavy", Class::Game);
        heavy.own.buffer = Own::Off;
        assert!(!buffer_runs(&games, &[]), "no game runs");
        assert!(buffer_runs(&always, &[]), "always");
        assert!(buffer_runs(&games, &[run(&game, false)]));
        assert!(!buffer_runs(&games, &[run(&tool, true)]));
        assert!(!buffer_runs(&games, &[run(&new, false)]), "behind");
        assert!(buffer_runs(&games, &[run(&new, true)]), "in full screen");
        assert!(buffer_runs(&games, &[run(&tool, false), run(&game, false)]));
        for mode in [games, always] {
            assert!(
                !buffer_runs(&mode, &[run(&game, false), run(&heavy, false)]),
                "the heavy game's Off wins"
            );
            assert!(!buffer_runs(&mode, &[run(&heavy, false), run(&new, true)]));
        }
    }

    #[test]
    fn several_programs_one_recording_and_its_stop() {
        let e = Every::default();
        let mut a = entry("/g/a", Class::Game);
        a.own.record = Own::On;
        a.name = "Ember Pass".into();
        let mut b = entry("/g/b", Class::Game);
        b.own.record = Own::On;
        let c = entry("/g/c", Class::Game);
        let tool = entry("/g/tool", Class::NotAGame);
        let behind = |e| run(e, false);
        assert!(
            records(&e, &[behind(&c), behind(&tool)]).is_none(),
            "off for every game"
        );
        assert_eq!(
            records(&e, &[behind(&c), behind(&a), behind(&b)]).map(|x| x.name.as_str()),
            Some("Ember Pass"),
            "one recording, the first that records by itself"
        );
        let all = every(true, true, true, false);
        assert!(
            records(&all, &[behind(&c)]).is_some(),
            "every game records by itself"
        );
        assert!(records(&all, &[behind(&tool)]).is_none());
        let stops = stoppers(&e, &[behind(&a), behind(&c), behind(&tool)]);
        assert_eq!(stops.len(), 2, "the games, not the tool");
        let mut keeps = c.clone();
        keeps.own.stop = Own::Off;
        assert!(stoppers(&e, &[behind(&keeps), behind(&tool)]).is_empty());
    }

    #[test]
    fn the_waits_follow_the_program_in_front() {
        let off = Every::default();
        let on = every(true, false, true, true);
        let game = entry("/g/game", Class::Game);
        let tool = entry("/g/tool", Class::NotAGame);
        let new = entry("/g/new", Class::NotSorted);
        let mut heavy = entry("/g/heavy", Class::Game);
        heavy.own.wait = Own::On;
        let mut light = entry("/g/light", Class::Game);
        light.own.wait = Own::Off;
        for fs in [false, true] {
            assert!(!waits(&off, Front::Listed(&game), fs), "off by default");
            assert!(!waits(&off, Front::Unknown, fs));
            assert!(
                waits(&off, Front::Listed(&heavy), fs),
                "on for itself alone"
            );
            assert!(waits(&on, Front::Listed(&game), fs), "a game in front");
            assert!(
                !waits(&on, Front::Listed(&light), fs),
                "off for itself alone"
            );
            assert!(!waits(&on, Front::Listed(&tool), fs));
            assert!(!waits(&on, Front::NoGame, fs), "a browser");
            assert!(!waits(&on, Front::Nothing, fs));
            assert_eq!(waits(&on, Front::Listed(&new), fs), fs, "not sorted yet");
            assert_eq!(waits(&on, Front::Unknown, fs), fs, "unknown, as before");
        }
    }

    #[test]
    fn a_program_is_found_by_its_file_then_by_a_steam_folder() {
        let mut p = Programs::default();
        let mut steam = entry("/lib/steamapps/common/Ember Pass", Class::Game);
        steam.folder = true;
        steam.id = "steam-1".into();
        steam.steam_app = Some(1);
        p.add(steam).unwrap();
        let mut own = entry(
            "/lib/steamapps/common/Ember Pass/tools/editor",
            Class::NotAGame,
        );
        own.id = "editor".into();
        p.add(own).unwrap();
        let at = |s: &str| p.find(Path::new(s)).map(|e| e.id.clone());
        assert_eq!(
            at("/lib/steamapps/common/Ember Pass/bin/ember.exe").as_deref(),
            Some("steam-1")
        );
        assert_eq!(
            at("/lib/steamapps/common/Ember Pass/tools/editor").as_deref(),
            Some("editor"),
            "one listed on its own"
        );
        assert_eq!(at("/lib/steamapps/common/Ember Passage/x"), None);
        assert_eq!(
            at("/lib/steamapps/common/Ember Pass"),
            None,
            "the folder is no program"
        );
        assert!(p
            .add(entry("/lib/steamapps/common/Ember Pass/", Class::Game))
            .is_err());
    }

    #[test]
    fn saved_and_read_again_and_a_later_class_reads_as_not_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("programs.json");
        let mut p = Programs::load(&file).unwrap();
        assert!(p.entries.is_empty());
        let mut e = entry("/g/a", Class::Game);
        e.own.wait = Own::On;
        e.naming = Naming::Window;
        p.add(e).unwrap();
        p.save().unwrap();
        let back = Programs::load(&file).unwrap();
        assert_eq!(back.entries, p.entries);
        let text = std::fs::read_to_string(&file)
            .unwrap()
            .replace("\"game\"", "\"favourite\"")
            .replace("\"window\"", "\"its icon\"");
        std::fs::write(&file, text).unwrap();
        let later = Programs::load(&file).unwrap();
        assert_eq!(later.entries[0].class, Class::NotSorted);
        assert_eq!(later.entries[0].naming, Naming::Name);
        assert_eq!(later.entries[0].own.wait, Own::On);
    }

    fn app(id: u32, name: &str, kind: SteamKind) -> SteamApp {
        SteamApp {
            app_id: id,
            name: name.into(),
            path: if kind == SteamKind::Shortcut {
                PathBuf::from(format!("/games/{name}/run"))
            } else {
                PathBuf::from(format!("/lib/steamapps/common/{name}"))
            },
            kind,
        }
    }

    #[test]
    fn steam_comes_in_stays_out_once_removed_and_keeps_an_uninstalled_one() {
        let mut p = Programs::default();
        let t0 = Timestamp::UNIX_EPOCH;
        let apps = vec![
            app(10, "Cinder Road", SteamKind::Game),
            app(20, "Desk Painter", SteamKind::Application),
            app(30, "Moonlit Racer", SteamKind::Shortcut),
        ];
        let took = p.take_steam(&apps, t0);
        assert_eq!(took.added.len(), 3);
        let cinder = p.get("steam-10").unwrap();
        assert_eq!(
            (cinder.class, cinder.folder, cinder.fresh),
            (Class::Game, true, false)
        );
        assert_eq!(cinder.came, Came::Steam);
        let desk = p.get("steam-20").unwrap();
        assert_eq!((desk.class, desk.application), (Class::NotSorted, true));
        let racer = p.find(Path::new("/games/Moonlit Racer/run")).unwrap();
        assert_eq!((racer.class, racer.folder), (Class::NotSorted, false));
        assert_eq!(racer.came, Came::SteamShortcut);

        let mut more = apps.clone();
        more.push(app(40, "Hollow Pines", SteamKind::Game));
        let took = p.take_steam(&more, t0);
        assert_eq!(took.added, vec!["Hollow Pines".to_string()]);
        assert!(p.get("steam-40").unwrap().fresh, "new from Steam");
        assert!(p.seen("steam-40", t0));
        assert!(!p.get("steam-40").unwrap().fresh, "seen: no longer new");

        p.get_mut("steam-10").unwrap().name = "Cinder".into();
        p.get_mut("steam-10").unwrap().name_from = NameFrom::Hand;
        p.remove("steam-40").unwrap();
        assert_eq!(p.not_again[0].name, "Hollow Pines");
        assert!(p.take_steam(&more, t0).added.is_empty(), "not added again");
        assert_eq!(
            p.get("steam-10").unwrap().name,
            "Cinder",
            "a name given by hand stays"
        );

        let without: Vec<_> = more.iter().filter(|a| a.app_id != 10).cloned().collect();
        let took = p.take_steam(&without, t0);
        assert_eq!(took.uninstalled, 1);
        assert!(
            p.get("steam-10").unwrap().uninstalled,
            "it stays, uninstalled"
        );
        let mut moved = more.clone();
        moved[0].path = PathBuf::from("/other/steamapps/common/Cinder Road");
        p.take_steam(&moved, t0);
        let back = p.get("steam-10").unwrap();
        assert!(!back.uninstalled, "installed again: the same entry");
        assert_eq!(
            back.path,
            PathBuf::from("/other/steamapps/common/Cinder Road")
        );

        assert!(p.add_back(40));
        assert_eq!(
            p.take_steam(&more, t0).added,
            vec!["Hollow Pines".to_string()]
        );
        assert_eq!(p.entries.len(), 4);
    }

    #[test]
    fn a_program_listed_by_hand_is_not_listed_twice_by_steam() {
        let mut p = Programs::default();
        let mut mine = entry("/games/Moonlit Racer/run", Class::Game);
        mine.name = "Moonlit".into();
        p.add(mine).unwrap();
        let took = p.take_steam(
            &[app(30, "Moonlit Racer", SteamKind::Shortcut)],
            Timestamp::UNIX_EPOCH,
        );
        assert!(took.added.is_empty());
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.entries[0].class, Class::Game);
    }

    #[test]
    fn the_unsorted_count_and_seen_once_a_day() {
        let mut p = Programs::default();
        p.add(entry("/a", Class::NotSorted)).unwrap();
        p.add(entry("/b", Class::Game)).unwrap();
        assert_eq!(p.unsorted(), 1);
        let id = p.entries[1].id.clone();
        let t: Timestamp = "2026-11-13T10:00:00Z".parse().unwrap();
        assert!(p.seen(&id, t), "the first time");
        assert!(!p.seen(&id, t + jiff::SignedDuration::from_mins(30)));
        assert!(
            p.seen(&id, t + jiff::SignedDuration::from_hours(24)),
            "another day"
        );
        assert!(!p.seen("none", t));
    }

    #[test]
    fn a_rename_changes_the_titles_reel_made_and_not_one_changed_by_hand() {
        let at: Timestamp = "2026-11-09T21:14:00Z".parse().unwrap();
        let made = crate::gaming::clip_title(Some("De Libvm C Str Onkl Sto"), at);
        let new = retitle(&made, "De Libvm C Str Onkl Sto", "libvmcst", at).unwrap();
        assert_eq!(new, crate::gaming::clip_title(Some("libvmcst"), at));
        assert_eq!(
            retitle("My best round", "De Libvm C Str Onkl Sto", "libvmcst", at),
            None
        );
        assert_eq!(
            retitle(&format!("{made} again"), "De Libvm C Str Onkl Sto", "x", at),
            None,
            "words added by hand"
        );
        let other_moment: Timestamp = "2026-11-09T22:30:00Z".parse().unwrap();
        assert_eq!(
            retitle(&made, "De Libvm C Str Onkl Sto", "x", other_moment),
            None,
            "a title of another moment is not reel's for this clip"
        );
        let last_year: Timestamp = "2025-03-02T08:05:00Z".parse().unwrap();
        let d = last_year.to_zoned(jiff::tz::TimeZone::system()).datetime();
        let with_year = format!(
            "Old Game, {}",
            reel_core::names::time_title_in(&d, d.year() + 1)
        );
        assert_eq!(
            retitle(&with_year, "Old Game", "New Game", last_year),
            Some(with_year.replacen("Old Game", "New Game", 1)),
            "a title with its year"
        );
    }

    #[test]
    fn the_id_is_the_same_for_the_same_program() {
        assert_eq!(id_of(Path::new("/g/a")), id_of(Path::new("/g/a/")));
        assert_ne!(id_of(Path::new("/g/a")), id_of(Path::new("/g/b")));
        assert!(id_of(Path::new("/g/a")).starts_with("p-"));
    }
}
