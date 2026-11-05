//! Standalone's Library folder: reel on this PC's whole folder (the database, the clips, the
//! kept originals), which the app moves to another drive itself, and a folder holding a library
//! that it takes up as it is. A move copies, checks, switches, then removes; a journal lets the
//! next start finish or roll back one the app or the PC stopped halfway.
use crate::runtime::Runtime;
use reel_desktop_core::settings::Mode;
use reel_pipeline::copier::{self, Entry};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The folder a picked folder with other things in it gets for the library.
pub const SUBFOLDER: &str = "reel library";
/// Room kept free on the target beyond the library's size.
pub const SPARE: u64 = 1_000_000_000;
const MARKER_REL: &str = "library/.reel-library";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Fixed,
    Removable,
    Network,
}

/// What the move asks of the drives, apart from the files.
pub trait Disks {
    /// Free and total bytes of the drive that holds `path`.
    fn space(&self, path: &Path) -> (u64, u64);
    fn same_drive(&self, a: &Path, b: &Path) -> bool;
    fn kind(&self, path: &Path) -> Kind;
}

/// The folders a target is checked against.
#[derive(Debug, Clone)]
pub struct Places {
    /// The library folder in force.
    pub current: PathBuf,
    /// Where the library has always been, `library` in the app's data folder.
    pub origin: PathBuf,
    /// The app's data and settings folders.
    pub app: Vec<PathBuf>,
    /// The folder the clips are watched in.
    pub clips: Option<PathBuf>,
}

/// What a picked folder is for.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    Move {
        to: PathBuf,
        /// The folder is not there yet: the move makes it.
        made: bool,
        same_drive: bool,
        bytes: u64,
        free: u64,
        removable: bool,
    },
    Use {
        dir: PathBuf,
        clips: u64,
    },
}

/// The nearest folder of `p` that is there, with links resolved, and the rest of `p` after it.
fn real(p: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut at = p.to_path_buf();
    loop {
        if let Ok(c) = std::fs::canonicalize(&at) {
            let mut out = plain(&c);
            for r in rest.iter().rev() {
                out.push(r);
            }
            return out;
        }
        match (at.file_name().map(|n| n.to_owned()), at.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                at = parent.to_path_buf();
            }
            _ => return p.to_path_buf(),
        }
    }
}

/// A path as the system shows it: on Windows without the `\\?\` that canonicalize adds.
pub fn plain(p: &Path) -> PathBuf {
    if cfg!(windows) {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    p.to_path_buf()
}

/// A path as compared: Windows ignores the case of names.
fn key(p: &Path) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(reel_desktop_core::install::path_key(&p.to_string_lossy()))
    } else {
        p.to_path_buf()
    }
}

fn same(a: &Path, b: &Path) -> bool {
    key(a) == key(b)
}

/// `p` is `dir` or inside it.
fn inside(p: &Path, dir: &Path) -> bool {
    key(p).starts_with(key(dir))
}

fn empty(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut d| d.next().is_none())
        .unwrap_or(false)
}

/// The id a library's marker names and its clips, when the folder holds one.
pub type Peek = dyn Fn(&Path) -> Option<(String, u64)>;

/// What a picked folder would be: a move of the library into it (or into `reel library` in it),
/// or the use of the library it holds; else why not, in words for the page.
pub fn plan(
    picked: &Path,
    places: &Places,
    bytes: u64,
    disks: &dyn Disks,
    peek: &Peek,
) -> Result<Target, String> {
    if !picked.is_dir() {
        return Err(format!("{} is not a folder", picked.display()));
    }
    let picked = real(picked);
    let current = real(&places.current);
    let origin = real(&places.origin);
    let shown = |p: &Path| p.display().to_string();
    if let Some(lib) = picked
        .ancestors()
        .skip(1)
        .find(|a| peek(a).is_some() && !same(a, &current))
    {
        return Err(format!(
            "{} is inside the reel library in {}: pick {} itself to use that library, or a folder outside it",
            shown(&picked),
            shown(lib),
            shown(lib)
        ));
    }
    let to = if peek(&picked).is_some() || empty(&picked) {
        picked.clone()
    } else {
        let sub = picked.join(SUBFOLDER);
        if sub.exists() && peek(&sub).is_none() && !empty(&sub) {
            return Err(format!(
                "{} has a folder {SUBFOLDER} already, with other things in it: pick that folder itself, or another one",
                shown(&picked)
            ));
        }
        sub
    };
    if same(&to, &current) {
        return Err(format!("the library is in {} now", shown(&to)));
    }
    if inside(&to, &current) {
        return Err(format!(
            "{} is inside the library; pick a folder outside it",
            shown(&to)
        ));
    }
    if let Some(clips) = places.clips.as_deref().map(real) {
        if same(&to, &clips) {
            return Err(format!(
                "{} is the clips folder, where OBS saves: reel would take the library's files for new clips",
                shown(&to)
            ));
        }
        if inside(&to, &clips) {
            return Err(format!(
                "{} is inside the clips folder, where OBS saves: reel would take the library's files for new clips",
                shown(&to)
            ));
        }
    }
    if !same(&to, &origin) && places.app.iter().map(|d| real(d)).any(|d| inside(&to, &d)) {
        return Err(format!(
            "{} is in the app's own folder; pick one of your own",
            shown(&to)
        ));
    }
    if disks.kind(&to) == Kind::Network {
        return Err(format!(
            "{} is on the network: reel's database needs a drive of this PC; for a library on a NAS, run reel as a server",
            shown(&to)
        ));
    }
    if let Some((id, clips)) = peek(&to) {
        if id.is_empty() {
            return Err(format!(
                "{} has a database, but its library has no .reel-library file",
                shown(&to)
            ));
        }
        return Ok(Target::Use { dir: to, clips });
    }
    let same_drive = disks.same_drive(&current, &to);
    let (free, _) = disks.space(&to);
    if !same_drive && free < bytes.saturating_add(SPARE) {
        return Err(format!(
            "{} has {} free, and the library needs {} with 1 GB to spare",
            drive_name(&to),
            reel_core::format::size(free),
            reel_core::format::size(bytes)
        ));
    }
    Ok(Target::Move {
        made: !to.exists(),
        to,
        same_drive,
        bytes,
        free,
        removable: disks.kind(&picked) == Kind::Removable,
    })
}

/// The drive as people name it: `D:` on Windows, the folder otherwise.
pub fn drive_name(p: &Path) -> String {
    use std::path::Component;
    match p.components().next() {
        Some(Component::Prefix(pre)) if cfg!(windows) => pre
            .as_os_str()
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_string(),
        _ => format!("the drive of {}", p.display()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum How {
    Rename,
    Copy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Renaming,
    Copying,
    Checking,
    Switching,
    Removing,
}

/// A move under way, written before each phase, read at the next start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub from: PathBuf,
    pub to: PathBuf,
    pub how: How,
    pub made: bool,
    pub phase: Phase,
}

impl Journal {
    pub fn load(file: &Path) -> Option<Journal> {
        serde_json::from_slice(&std::fs::read(file).ok()?).ok()
    }

    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        let part = reel_desktop_core::settings::part_path(file);
        std::fs::write(&part, serde_json::to_vec_pretty(self)?)?;
        std::fs::File::open(&part)?.sync_all()?;
        std::fs::rename(&part, file)
    }
}

/// What a move tells as it goes: the phase, the bytes done and the bytes in all.
pub type Tick<'a> = &'a mut dyn FnMut(Phase, u64, u64);

/// The steps of a move that are the app's: write the settings, start reel on this PC.
pub trait Switch {
    fn switch(&mut self, to: &Path) -> Result<(), String>;
}

/// Moves the library `j.from` to `j.to` as `j.how` says; the journal is at `file` throughout.
/// A rename refused is copied instead, `space` checked first. Returns once the library is in
/// force at its new place; the old folder's removal is `remove_old`'s.
pub fn run(
    j: &mut Journal,
    file: &Path,
    stop: &AtomicBool,
    space: &dyn Fn(&Path) -> u64,
    sw: &mut dyn Switch,
    tick: Tick,
) -> Result<(), String> {
    let io = |what: &str, e: std::io::Error| format!("{what}: {e}");
    if j.how == How::Rename {
        j.phase = Phase::Renaming;
        j.save(file).map_err(|e| io("the move's journal", e))?;
        if !j.made {
            let _ = std::fs::remove_dir(&j.to);
        } else if let Some(parent) = j.to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io("the target", e))?;
        }
        match rename(&j.from, &j.to) {
            Ok(()) => {
                tick(Phase::Renaming, 1, 1);
                j.phase = Phase::Switching;
                let switched = j
                    .save(file)
                    .map_err(|e| io("the move's journal", e))
                    .and_then(|_| sw.switch(&j.to));
                if let Err(e) = switched {
                    // the library goes back to the folder the settings still name
                    let back = rename(&j.to, &j.from);
                    if j.made {
                        let _ = std::fs::remove_dir(&j.to);
                    } else {
                        let _ = std::fs::create_dir_all(&j.to);
                    }
                    if back.is_ok() {
                        let _ = std::fs::remove_file(file);
                    }
                    return Err(e);
                }
                j.phase = Phase::Removing;
                let _ = j.save(file);
                return Ok(());
            }
            Err(e) => {
                tracing::info!(error = %e, "library move: the rename was refused, the library is copied instead");
                if !j.made {
                    let _ = std::fs::create_dir_all(&j.to);
                }
                j.how = How::Copy;
            }
        }
    }
    let entries = copier::list(&j.from, &|_| false).map_err(|e| io("the library", e))?;
    let total = copier::total(&entries);
    if space(&j.to) < total.saturating_add(SPARE) {
        return Err(format!(
            "{} has {} free, and the library needs {} with 1 GB to spare",
            drive_name(&j.to),
            reel_core::format::size(space(&j.to)),
            reel_core::format::size(total)
        ));
    }
    j.phase = Phase::Copying;
    j.save(file).map_err(|e| io("the move's journal", e))?;
    if let Err(e) = copy_and_check(j, file, &entries, total, stop, tick) {
        roll_back(j);
        let _ = std::fs::remove_file(file);
        return Err(e);
    }
    j.phase = Phase::Switching;
    let switched = j
        .save(file)
        .map_err(|e| io("the move's journal", e))
        .and_then(|_| sw.switch(&j.to));
    if let Err(e) = switched {
        // the settings still name the old folder, which reel goes on from: the copy goes
        roll_back(j);
        let _ = std::fs::remove_file(file);
        return Err(e);
    }
    j.phase = Phase::Removing;
    let _ = j.save(file);
    Ok(())
}

fn copy_and_check(
    j: &mut Journal,
    file: &Path,
    entries: &[Entry],
    total: u64,
    stop: &AtomicBool,
    tick: Tick,
) -> Result<(), String> {
    let mut done = 0;
    let copied = copier::copy_all(
        &j.from,
        &j.to,
        entries,
        Some(Path::new(MARKER_REL)),
        stop,
        &mut |n| {
            done += n;
            tick(Phase::Copying, done, total);
        },
    )
    .map_err(|e| said(&e, "copying"))?;
    j.phase = Phase::Checking;
    j.save(file)
        .map_err(|e| format!("the move's journal: {e}"))?;
    let mut done = 0;
    copier::check_all(&j.to, &copied, stop, &mut |n| {
        done += n;
        tick(Phase::Checking, done, total);
    })
    .map_err(|e| said(&e, "checking the copy"))
}

fn said(e: &std::io::Error, while_: &str) -> String {
    if copier::is_stopped(e) {
        "stopped".into()
    } else {
        format!("{while_}: {e}")
    }
}

/// On Windows a file can stay held a moment after reel on this PC stopped; a rename is tried
/// again for a few seconds before it counts as refused.
fn rename(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for _ in 0..if cfg!(windows) { 10 } else { 1 } {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("the rename was not tried")))
}

/// Takes away what a move put into its target: the names the library has, files then folders,
/// and the target folder when the move made it.
pub fn roll_back(j: &Journal) {
    if let Ok(entries) = copier::list(&j.from, &|_| false) {
        copier::undo(&j.to, &entries);
    }
    if j.made {
        let _ = std::fs::remove_dir(&j.to);
    }
}

/// Removes the library's old folder once the new one is seen to hold it: what the library
/// had, then the folder when nothing else is in it.
fn remove_old(j: &Journal, tick: Tick) -> Result<(), String> {
    if j.how == How::Rename || !j.from.exists() {
        return Ok(());
    }
    if reel_pipeline::library::holds_library(&j.to).is_none() {
        return Err(format!(
            "{} does not hold the library, so the old folder {} stays",
            j.to.display(),
            j.from.display()
        ));
    }
    let entries = copier::list(&j.from, &|_| false).map_err(|e| e.to_string())?;
    let total = copier::total(&entries);
    let mut done = 0;
    for e in entries.iter().filter(|e| !e.dir) {
        let _ = std::fs::remove_file(j.from.join(&e.rel));
        done += e.size;
        tick(Phase::Removing, done, total);
    }
    for e in entries.iter().rev().filter(|e| e.dir) {
        let _ = std::fs::remove_dir(j.from.join(&e.rel));
    }
    let _ = std::fs::remove_dir(&j.from);
    Ok(())
}

/// What the next start does with a journal it finds, before reel on this PC starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recovery {
    /// The move had not switched: what it copied goes, the library stays where it was.
    RollBack,
    /// The copy was checked, or the rename went through: the settings name the target.
    Forward,
    /// Settled: only the old folder is left to remove.
    Remove,
    /// The rename did not happen.
    Nothing,
}

pub fn recovery(j: &Journal, holds: &dyn Fn(&Path) -> bool) -> Recovery {
    match j.phase {
        Phase::Copying | Phase::Checking => Recovery::RollBack,
        Phase::Switching => Recovery::Forward,
        Phase::Removing => Recovery::Remove,
        Phase::Renaming if !holds(&j.from) && holds(&j.to) => Recovery::Forward,
        Phase::Renaming => Recovery::Nothing,
    }
}

/// How a start left a move it found in the journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settled {
    /// The library stays in the folder the settings name; nothing of the move is left.
    Stayed,
    /// The settings name the target: the old folder is to be removed.
    Moved,
    /// The settings could not be written: what the move put in the target went (or the rename
    /// was undone), and the library stays in its old folder, whole.
    Unwritten(String),
    /// The settings could not be written once the old folder was being removed: reel on this
    /// PC runs from the target for this start, and the journal stays for the next.
    TargetForNow(String),
}

/// Settles a move found in the journal, before reel on this PC starts. `names` says whether the
/// settings in force name the target already; `write` makes them do so, and can fail (the drive
/// with the settings full). Nothing here removes the old folder: `remove_moved` does, after.
pub fn settle(
    j: &mut Journal,
    file: &Path,
    names: bool,
    write: &mut dyn FnMut(&Path) -> Result<(), String>,
    holds: &dyn Fn(&Path) -> bool,
) -> Settled {
    let r = recovery(j, holds);
    match r {
        Recovery::RollBack => {
            roll_back(j);
            let _ = std::fs::remove_file(file);
            Settled::Stayed
        }
        Recovery::Nothing => {
            if !j.made && !j.to.exists() {
                let _ = std::fs::create_dir_all(&j.to);
            }
            let _ = std::fs::remove_file(file);
            Settled::Stayed
        }
        Recovery::Forward | Recovery::Remove => {
            let written = if names { Ok(()) } else { write(&j.to) };
            match written {
                Ok(()) => {
                    j.phase = Phase::Removing;
                    let _ = j.save(file);
                    Settled::Moved
                }
                Err(e) if r == Recovery::Forward && j.how == How::Copy => {
                    roll_back(j);
                    let _ = std::fs::remove_file(file);
                    Settled::Unwritten(e)
                }
                Err(e) if r == Recovery::Forward => match rename(&j.to, &j.from) {
                    Ok(()) => {
                        if !j.made {
                            let _ = std::fs::create_dir_all(&j.to);
                        }
                        let _ = std::fs::remove_file(file);
                        Settled::Unwritten(e)
                    }
                    Err(_) => Settled::TargetForNow(e),
                },
                Err(e) => Settled::TargetForNow(e),
            }
        }
    }
}

/// What the page says after a start settled a move the app did not finish.
pub fn settled_line(j: &Journal, s: &Settled) -> String {
    let (to, from) = (j.to.display(), j.from.display());
    match s {
        Settled::Stayed => format!(
            "The move to {to} was cut short when the app stopped; the library stayed in {from}, nothing was lost."
        ),
        Settled::Moved => format!("The move to {to} was finished at this start; the library is there."),
        Settled::Unwritten(e) => format!(
            "The move to {to} could not be finished: this start could not write the settings ({e}). The library stays in {from}, whole; make room on the drive with the app's settings and move it again."
        ),
        Settled::TargetForNow(e) => format!(
            "This start could not write the settings ({e}): reel on this PC runs from {to}, where the library is whole, and {from} is left as it is until they can be written."
        ),
    }
}

/// Removes the old folder of a move, and only while the settings in force name its target.
pub fn remove_moved(j: &Journal, in_force: &Path, tick: Tick) -> Result<(), String> {
    if !same(&real(in_force), &real(&j.to)) {
        return Err(format!(
            "the settings name {}, not {}, so the old folder {} stays",
            in_force.display(),
            j.to.display(),
            j.from.display()
        ));
    }
    remove_old(j, tick)
}

/// The real drives.
pub struct RealDisks;

impl Disks for RealDisks {
    fn space(&self, path: &Path) -> (u64, u64) {
        reel_pipeline::notify::free_and_total(&nearest(path))
    }

    fn same_drive(&self, a: &Path, b: &Path) -> bool {
        match (
            reel_pipeline::disks::probe(&nearest(a)),
            reel_pipeline::disks::probe(&nearest(b)),
        ) {
            (Some(x), Some(y)) => x.0 == y.0,
            _ => false,
        }
    }

    fn kind(&self, path: &Path) -> Kind {
        kind_of(&nearest(path))
    }
}

/// The nearest folder of `p` that is there.
pub fn nearest(p: &Path) -> PathBuf {
    let mut at = p.to_path_buf();
    while !at.exists() {
        match at.parent() {
            Some(parent) => at = parent.to_path_buf(),
            None => break,
        }
    }
    at
}

#[cfg(windows)]
fn kind_of(p: &Path) -> Kind {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    let s = plain(p).to_string_lossy().into_owned();
    if s.starts_with(r"\\") {
        return Kind::Network;
    }
    let root: Vec<u16> = std::ffi::OsStr::new(&format!("{}\\", s.get(..2).unwrap_or("")))
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // DRIVE_REMOVABLE is 2, DRIVE_REMOTE is 4
    match unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) } {
        2 => Kind::Removable,
        4 => Kind::Network,
        _ => Kind::Fixed,
    }
}

#[cfg(not(windows))]
fn kind_of(p: &Path) -> Kind {
    if reel_pipeline::disks::network_share(p).is_some() {
        return Kind::Network;
    }
    if p.starts_with("/media") || p.starts_with("/run/media") {
        return Kind::Removable;
    }
    Kind::Fixed
}

/// A move, a use or a copy of the library, under way or just ended, for the page.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Job {
    /// move, use or copy.
    pub kind: String,
    pub phase: String,
    pub from: String,
    pub to: String,
    pub done: u64,
    pub total: u64,
    /// When it started, in milliseconds since 1970, for the time left.
    pub started_ms: u64,
    pub running: bool,
    pub ok: bool,
    /// What the page says once it ended.
    pub line: String,
}

/// The library folder in the status: where it is, and whether reel on this PC waits for it.
#[derive(Debug, Clone, Serialize, Default)]
pub struct LibraryLine {
    pub dir: String,
    /// Not in its place of origin.
    pub moved: bool,
    pub away: Option<String>,
    pub job: Option<Job>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// What a copy of the library would be: its folder, its size, the drive's room.
#[derive(Debug, Clone, Serialize)]
pub struct CopyPlan {
    pub dir: PathBuf,
    pub bytes: u64,
    pub free: u64,
    pub removable: bool,
}

/// A copy being made: the folder to remove when the app stopped halfway.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CopyJournal {
    part: PathBuf,
}

pub fn copy_plan(
    into: &Path,
    places: &Places,
    bytes: u64,
    at: &jiff::Zoned,
    disks: &dyn Disks,
) -> Result<CopyPlan, String> {
    if !into.is_dir() {
        return Err(format!("{} is not a folder", into.display()));
    }
    let into = real(into);
    let current = real(&places.current);
    if inside(&into, &current) {
        return Err(format!(
            "{} is inside the library; pick a folder outside it",
            into.display()
        ));
    }
    if let Some(clips) = places.clips.as_deref().map(real) {
        if inside(&into, &clips) {
            return Err(format!(
                "{} is in the clips folder, where OBS saves: reel would take the copy's files for new clips",
                into.display()
            ));
        }
    }
    let (dir, part) = reel_pipeline::library_copy::names(&into, at);
    if dir.exists() || part.exists() {
        return Err(format!("{} is there already", dir.display()));
    }
    let (free, _) = disks.space(&into);
    if free < bytes.saturating_add(SPARE) {
        return Err(format!(
            "{} has {} free, and the copy needs {} with 1 GB to spare",
            drive_name(&into),
            reel_core::format::size(free),
            reel_core::format::size(bytes)
        ));
    }
    Ok(CopyPlan {
        dir,
        bytes,
        free,
        removable: disks.kind(&into) == Kind::Removable,
    })
}

fn peek_real(p: &Path) -> Option<(String, u64)> {
    let id = reel_pipeline::library::holds_library(p)?;
    let clips = std::fs::read_dir(p.join("library"))
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .count() as u64
        })
        .unwrap_or(0);
    Some((id, clips))
}

fn size_of(dir: &Path) -> u64 {
    copier::list(dir, &|_| false)
        .map(|e| copier::total(&e))
        .unwrap_or(0)
}

/// The settings lane's answer to the switch: the library folder written, or why not.
struct AppSwitch(Runtime);

impl Switch for AppSwitch {
    fn switch(&mut self, to: &Path) -> Result<(), String> {
        self.0.switch_to(to)
    }
}

impl Runtime {
    /// The folder reel on this PC runs from.
    pub fn library_dir(&self) -> PathBuf {
        let g = self.lock();
        g.settings.library_dir(&g.paths)
    }

    fn places(&self) -> Places {
        let g = self.lock();
        let clips = g
            .obs_dir
            .clone()
            .filter(|_| g.settings.clips_follow_obs)
            .unwrap_or_else(|| g.settings.clips_dir.clone());
        Places {
            current: g.settings.library_dir(&g.paths),
            origin: g.paths.library_dir(),
            app: vec![g.paths.data_dir.clone(), g.paths.config_dir.clone()],
            clips: Some(clips),
        }
    }

    fn move_journal(&self) -> PathBuf {
        self.lock().paths.data_dir.join("library-move.json")
    }

    fn copy_journal(&self) -> PathBuf {
        self.lock().paths.data_dir.join("library-copy.json")
    }

    pub fn library_line(&self) -> LibraryLine {
        let g = self.lock();
        let dir = g.settings.library_dir(&g.paths);
        LibraryLine {
            moved: dir != g.paths.library_dir(),
            dir: dir.display().to_string(),
            away: g.library_away.clone(),
            job: g.library_job.clone(),
        }
    }

    /// The library folder with its size and its drive's room, for the Library panel.
    pub async fn library_info(&self) -> serde_json::Value {
        let dir = self.library_dir();
        let origin = self.lock().paths.library_dir();
        tauri::async_runtime::spawn_blocking(move || {
            let (free, total) = RealDisks.space(&dir);
            serde_json::json!({
                "dir": dir.display().to_string(),
                "moved": dir != origin,
                "bytes": size_of(&dir),
                "free": free,
                "total": total,
                "drive": if cfg!(windows) { drive_name(&real(&dir)) } else { String::new() },
                "removable": RealDisks.kind(&dir) == Kind::Removable,
            })
        })
        .await
        .unwrap_or_default()
    }

    /// What a picked folder would be for the library, before anything is done.
    pub async fn library_plan(&self, picked: PathBuf) -> Result<Target, String> {
        let places = self.places();
        tauri::async_runtime::spawn_blocking(move || {
            let bytes = size_of(&places.current);
            plan(&picked, &places, bytes, &RealDisks, &peek_real)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    pub async fn library_copy_plan(&self, into: PathBuf) -> Result<CopyPlan, String> {
        let places = self.places();
        let cfg = self
            .lock()
            .local
            .as_ref()
            .map(|l| l.ctx.config())
            .ok_or("reel on this PC is not running; the copy is made through it")?;
        tauri::async_runtime::spawn_blocking(move || {
            let bytes = reel_pipeline::library_copy::measure(&cfg);
            copy_plan(&into, &places, bytes, &jiff::Zoned::now(), &RealDisks)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    fn busy(&self) -> Result<(), String> {
        if !self.is_standalone() {
            return Err("the library folder is standalone's".into());
        }
        if self.lock().library_job.as_ref().is_some_and(|j| j.running) {
            return Err("a move or a copy of the library runs already".into());
        }
        Ok(())
    }

    fn begin(&self, kind: &str, from: &Path, to: &Path, total: u64) {
        {
            let mut g = self.lock();
            g.library_stop.store(false, Ordering::SeqCst);
            g.library_job = Some(Job {
                kind: kind.into(),
                phase: "starting".into(),
                from: from.display().to_string(),
                to: to.display().to_string(),
                total,
                started_ms: now_ms(),
                running: true,
                ..Default::default()
            });
        }
        self.push_status();
    }

    fn progress(&self, phase: &str, done: u64, total: u64) {
        let mut g = self.lock();
        if let Some(j) = g.library_job.as_mut() {
            if j.phase != phase {
                j.started_ms = now_ms();
            }
            j.phase = phase.into();
            j.done = done;
            j.total = total;
        }
    }

    fn end(&self, ok: bool, line: String) {
        {
            let mut g = self.lock();
            if let Some(j) = g.library_job.as_mut() {
                j.running = false;
                j.ok = ok;
                j.phase = if ok { "done" } else { "ended" }.into();
                j.line = line.clone();
            }
        }
        if ok {
            tracing::info!(line, "library");
        } else {
            tracing::warn!(line, "library");
        }
        self.push_status();
    }

    /// A tick that pushes the status at most four times a second.
    fn ticker(&self) -> impl FnMut(Phase, u64, u64) + Send + 'static {
        let rt = self.clone();
        let mut last = std::time::Instant::now();
        move |phase, done, total| {
            let word = match phase {
                Phase::Renaming => "renaming",
                Phase::Copying => "copying",
                Phase::Checking => "checking",
                Phase::Switching => "switching",
                Phase::Removing => "removing",
            };
            rt.progress(word, done, total);
            if last.elapsed() >= std::time::Duration::from_millis(250) || done == total {
                last = std::time::Instant::now();
                rt.push_status();
            }
        }
    }

    pub fn library_stop(&self) {
        self.lock().library_stop.store(true, Ordering::SeqCst);
        tracing::info!("library: stop asked");
    }

    /// Stops reel on this PC for good of this run, its database closed, so the folder can go.
    async fn hold_local(&self) {
        {
            let mut g = self.lock();
            g.moving = true;
            g.local_line = "the library is moving; reel on this PC waits".into();
        }
        crate::shell::go(&self.app, crate::shell::View::Settings, Some("library"));
        let local = self.lock().local.take();
        if let Some(l) = local {
            l.close().await;
        }
        self.push_status();
    }

    fn release_local(&self) {
        self.lock().moving = false;
        self.start_local();
    }

    /// Writes the settings naming `to` as the library folder; the settings in force change only
    /// once the file has them.
    fn write_library_dir(&self, to: &Path) -> Result<(), String> {
        let mut g = self.lock();
        let mut s = g.settings.clone();
        s.standalone.library_dir = if to == g.paths.library_dir() {
            PathBuf::new()
        } else {
            to.to_path_buf()
        };
        s.save(&g.paths.settings_file())
            .map_err(|e| format!("the settings: {e}"))?;
        g.settings = s;
        Ok(())
    }

    /// Writes the library folder in the settings' lane, then starts reel on this PC from it.
    fn switch_to(&self, to: &Path) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let rt = self.clone();
        let to = to.to_path_buf();
        let lane = self.lane();
        let _ = lane.ask(reel_desktop_core::lane::Ask::Run(Box::new(move || {
            let _ = tx.send(rt.write_library_dir(&to));
        })));
        rx.recv()
            .map_err(|_| "the settings were not written".to_string())??;
        tracing::info!(to = %self.library_dir().display(), "library: the settings name the new folder");
        {
            use tauri::Emitter;
            let _ = self.app.emit("settings", &self.settings());
        }
        self.release_local();
        Ok(())
    }

    pub fn library_move(&self, picked: PathBuf) -> Result<(), String> {
        self.busy()?;
        let rt = self.clone();
        tauri::async_runtime::spawn(async move { rt.do_move(picked).await });
        Ok(())
    }

    async fn do_move(&self, picked: PathBuf) {
        let from = self.library_dir();
        let target = match self.library_plan(picked.clone()).await {
            Ok(t) => t,
            Err(e) => {
                self.begin("move", &from, &picked, 0);
                return self.end(
                    false,
                    format!("The library stays in {}: {e}.", from.display()),
                );
            }
        };
        let (to, made, same_drive, bytes) = match target {
            Target::Use { dir, .. } => return self.do_use(dir).await,
            Target::Move {
                to,
                made,
                same_drive,
                bytes,
                ..
            } => (to, made, same_drive, bytes),
        };
        self.begin("move", &from, &to, bytes);
        self.hold_local().await;
        let mut j = Journal {
            from: from.clone(),
            to: to.clone(),
            how: if same_drive { How::Rename } else { How::Copy },
            made,
            phase: Phase::Copying,
        };
        let file = self.move_journal();
        let stop = self.lock().library_stop.clone();
        let rt = self.clone();
        let mut tick = self.ticker();
        let moved = tauri::async_runtime::spawn_blocking(move || {
            let mut sw = AppSwitch(rt);
            run(
                &mut j,
                &file,
                &stop,
                &|p| RealDisks.space(p).0,
                &mut sw,
                &mut tick,
            )
            .map(|_| j)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        let j = match moved {
            Ok(j) => j,
            Err(e) => {
                if self.lock().moving {
                    self.release_local();
                }
                let line = if e == "stopped" {
                    format!(
                        "The move was stopped; the library stays in {}.",
                        from.display()
                    )
                } else {
                    format!("The library stays in {}: {e}.", from.display())
                };
                return self.end(false, line);
            }
        };
        let file = self.move_journal();
        let mut tick = self.ticker();
        let in_force = self.library_dir();
        let removed = tauri::async_runtime::spawn_blocking(move || {
            let r = remove_moved(&j, &in_force, &mut tick);
            if r.is_ok() {
                let _ = std::fs::remove_file(&file);
            }
            r
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        match removed {
            Ok(()) => self.end(true, format!("The library is in {} now.", to.display())),
            Err(e) => self.end(
                true,
                format!(
                    "The library is in {} now; the old folder is not all removed: {e}",
                    to.display()
                ),
            ),
        }
    }

    async fn do_use(&self, dir: PathBuf) {
        let from = self.library_dir();
        self.begin("use", &from, &dir, 0);
        self.hold_local().await;
        let rt = self.clone();
        let to = dir.clone();
        let done = tauri::async_runtime::spawn_blocking(move || rt.switch_to(&to))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        match done {
            Ok(()) => self.end(
                true,
                format!(
                    "reel uses the library in {} now; the one in {} stays where it is.",
                    dir.display(),
                    from.display()
                ),
            ),
            Err(e) => {
                self.release_local();
                self.end(
                    false,
                    format!("The library stays in {}: {e}.", from.display()),
                )
            }
        }
    }

    pub fn library_copy(&self, into: PathBuf) -> Result<(), String> {
        self.busy()?;
        let rt = self.clone();
        tauri::async_runtime::spawn(async move { rt.do_copy(into).await });
        Ok(())
    }

    async fn do_copy(&self, into: PathBuf) {
        let from = self.library_dir();
        let plan = match self.library_copy_plan(into.clone()).await {
            Ok(p) => p,
            Err(e) => {
                self.begin("copy", &from, &into, 0);
                return self.end(false, format!("No copy was made: {e}."));
            }
        };
        let Some(local) = self.lock().local.clone() else {
            self.begin("copy", &from, &plan.dir, 0);
            return self.end(
                false,
                "No copy was made: reel on this PC is not running.".into(),
            );
        };
        self.begin("copy", &from, &plan.dir, plan.bytes);
        let at = jiff::Zoned::now();
        let (_, part) = reel_pipeline::library_copy::names(&real(&into), &at);
        let journal = self.copy_journal();
        let _ = std::fs::write(
            &journal,
            serde_json::to_vec(&CopyJournal { part }).unwrap_or_default(),
        );
        let stop = self.lock().library_stop.clone();
        let rt = self.clone();
        let last = std::sync::Mutex::new(std::time::Instant::now());
        let progress = Arc::new(
            move |phase: reel_pipeline::library_copy::Phase, done: u64, total: u64| {
                rt.progress(phase.word(), done, total);
                let mut l = last.lock().unwrap_or_else(|e| e.into_inner());
                if l.elapsed() >= std::time::Duration::from_millis(250) {
                    *l = std::time::Instant::now();
                    rt.push_status();
                }
            },
        );
        let made =
            reel_pipeline::library_copy::make(&local.ctx, &real(&into), &at, stop, progress).await;
        let _ = std::fs::remove_file(&journal);
        match made {
            Ok(done) => {
                let mut line = format!(
                    "The copy is in {}: {} {}, {}.",
                    done.dir.display(),
                    done.clips,
                    if done.clips == 1 { "clip" } else { "clips" },
                    reel_core::format::size(done.bytes)
                );
                if done.gone > 0 {
                    line.push_str(&format!(
                        " {} deleted for good while it ran {} not in it.",
                        done.gone,
                        if done.gone == 1 { "is" } else { "are" }
                    ));
                }
                if let Some(j) = self.lock().library_job.as_mut() {
                    j.to = done.dir.display().to_string();
                }
                self.end(true, line)
            }
            Err(e) => {
                let stopped = e
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(copier::is_stopped);
                self.end(
                    false,
                    if stopped {
                        "The copy was stopped, and its unfinished folder removed.".into()
                    } else {
                        format!("No copy was made: {e:#}.")
                    },
                )
            }
        }
    }

    /// Settles a move or a copy the app did not see to its end, before reel on this PC starts.
    pub fn recover_library(&self) {
        let file = self.copy_journal();
        if let Some(j) = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<CopyJournal>(&b).ok())
        {
            if j.part.to_string_lossy().ends_with("(unfinished)") {
                let _ = std::fs::remove_dir_all(&j.part);
            }
            let _ = std::fs::remove_file(&file);
            tracing::info!(part = %j.part.display(), "library: an unfinished copy was removed");
            self.lock().library_job = Some(Job {
                kind: "copy".into(),
                phase: "ended".into(),
                to: j.part.display().to_string(),
                line: format!(
                    "A copy cut short when the app stopped was removed: {}.",
                    j.part.display()
                ),
                ..Default::default()
            });
        }
        let file = self.move_journal();
        let Some(mut j) = Journal::load(&file) else {
            return;
        };
        let holds = |p: &Path| reel_pipeline::library::holds_library(p).is_some();
        let names = same(&real(&self.library_dir()), &real(&j.to));
        tracing::warn!(from = %j.from.display(), to = %j.to.display(), phase = ?j.phase, names, "library: a move the app did not finish");
        let settled = settle(
            &mut j,
            &file,
            names,
            &mut |to| self.write_library_dir(to),
            &holds,
        );
        tracing::info!(settled = ?settled, "library: the move is settled at this start");
        match &settled {
            Settled::Moved => {
                let (j2, in_force) = (j.clone(), self.library_dir());
                std::thread::spawn(
                    move || match remove_moved(&j2, &in_force, &mut |_, _, _| {}) {
                        Ok(()) => {
                            let _ = std::fs::remove_file(&file);
                        }
                        Err(e) => tracing::warn!(error = %e, "library: the old folder stays"),
                    },
                );
            }
            Settled::TargetForNow(_) => {
                // for this start only; the file keeps naming the old folder until it can be written
                self.lock().settings.standalone.library_dir = j.to.clone();
            }
            Settled::Stayed | Settled::Unwritten(_) => {}
        }
        self.lock().library_job = Some(Job {
            kind: "move".into(),
            phase: "ended".into(),
            from: j.from.display().to_string(),
            to: j.to.display().to_string(),
            ok: settled == Settled::Moved,
            line: settled_line(&j, &settled),
            ..Default::default()
        });
    }

    /// Standalone with the library elsewhere: whether its folder is there to start from.
    pub fn library_away_now(&self) -> Option<String> {
        let (dir, origin) = {
            let g = self.lock();
            (g.settings.library_dir(&g.paths), g.paths.library_dir())
        };
        if dir == origin || reel_pipeline::library::holds_library(&dir).is_some() {
            return None;
        }
        Some(format!(
            "{} is not there: is its drive plugged in?",
            dir.display()
        ))
    }

    /// Looks for the library's folder every 10 s and starts reel on this PC once it is back.
    pub fn wait_for_library(&self) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                let gone = {
                    let g = rt.lock();
                    g.settings.mode != Mode::Standalone || g.local.is_some() || g.moving
                };
                if gone {
                    rt.lock().library_away = None;
                    return;
                }
                if rt.library_away_now().is_none() {
                    tracing::info!("library: its folder is back; reel on this PC starts");
                    rt.lock().library_away = None;
                    rt.start_local();
                    return;
                }
            }
        });
    }

    /// While reel on this PC runs from a folder elsewhere: its library check finding the folder
    /// gone (a drive unplugged) stops it, and the wait for the folder begins.
    pub fn watch_library(&self, local: Arc<crate::standalone::Local>) {
        let rt = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                let current = rt
                    .lock()
                    .local
                    .as_ref()
                    .is_some_and(|l| Arc::ptr_eq(l, &local));
                if !current {
                    return;
                }
                if local.ctx.library.away().is_none() {
                    continue;
                }
                let Some(why) = rt.library_away_now() else {
                    continue;
                };
                tracing::warn!(
                    why,
                    "library: its folder went away; reel on this PC stops until it is back"
                );
                rt.stop_local();
                {
                    let mut g = rt.lock();
                    g.local_line = why.clone();
                    g.library_away = Some(why);
                }
                rt.push_status();
                rt.wait_for_library();
                return;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        free: u64,
        same: bool,
        kind: Kind,
    }

    impl Disks for Fake {
        fn space(&self, _: &Path) -> (u64, u64) {
            (self.free, self.free * 2)
        }
        fn same_drive(&self, _: &Path, _: &Path) -> bool {
            self.same
        }
        fn kind(&self, _: &Path) -> Kind {
            self.kind
        }
    }

    const ROOMY: Fake = Fake {
        free: 500_000_000_000,
        same: false,
        kind: Kind::Fixed,
    };

    fn peek(p: &Path) -> Option<(String, u64)> {
        reel_pipeline::library::holds_library(p).map(|id| (id, 3))
    }

    /// A library as the app keeps one: the database, the clips with the trash and marker, the
    /// originals and the copies.
    fn library(dir: &Path) {
        std::fs::create_dir_all(dir.join("library/C1/v2")).unwrap();
        std::fs::create_dir_all(dir.join("library/.trash/C0")).unwrap();
        std::fs::create_dir_all(dir.join("originals")).unwrap();
        std::fs::create_dir_all(dir.join("cache/discord")).unwrap();
        std::fs::write(dir.join("reel.db"), vec![1u8; 4096]).unwrap();
        std::fs::write(dir.join("library/C1/master.mp4"), vec![2u8; 3 << 20]).unwrap();
        std::fs::write(dir.join("library/C1/v2/cut.mp4"), b"cut").unwrap();
        std::fs::write(dir.join("library/.trash/C0/master.mp4"), b"bin").unwrap();
        std::fs::write(dir.join("originals/C1.mkv"), b"raw").unwrap();
        std::fs::write(
            dir.join(MARKER_REL),
            reel_pipeline::library::marker_text("01ME"),
        )
        .unwrap();
    }

    struct Fixture {
        tmp: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Fixture {
            let tmp = tempfile::tempdir().unwrap();
            for d in ["app/data/library", "app/config", "clips", "d", "e"] {
                std::fs::create_dir_all(tmp.path().join(d)).unwrap();
            }
            library(&tmp.path().join("app/data/library"));
            Fixture { tmp }
        }
        fn p(&self, rel: &str) -> PathBuf {
            real(&self.tmp.path().join(rel))
        }
        fn places(&self) -> Places {
            Places {
                current: self.p("app/data/library"),
                origin: self.p("app/data/library"),
                app: vec![self.p("app/data"), self.p("app/config")],
                clips: Some(self.p("clips")),
            }
        }
    }

    #[test]
    fn a_folder_takes_the_library_or_a_reel_library_folder_in_it() {
        let t = Fixture::new();
        let pl = t.places();
        match plan(&t.p("d"), &pl, 10, &ROOMY, &peek).unwrap() {
            Target::Move { to, made, .. } => {
                assert_eq!(to, t.p("d"), "an empty folder takes the library itself");
                assert!(!made);
            }
            other => panic!("{other:?}"),
        }
        std::fs::write(t.p("e/notes.txt"), b"mine").unwrap();
        match plan(&t.p("e"), &pl, 10, &ROOMY, &peek).unwrap() {
            Target::Move { to, made, .. } => {
                assert_eq!(to, t.p("e").join(SUBFOLDER));
                assert!(made, "the move makes it");
            }
            other => panic!("{other:?}"),
        }
        std::fs::create_dir_all(t.p("e").join(SUBFOLDER)).unwrap();
        std::fs::write(t.p("e").join(SUBFOLDER).join("x"), b"x").unwrap();
        let e = plan(&t.p("e"), &pl, 10, &ROOMY, &peek).unwrap_err();
        assert!(e.contains("has a folder reel library already"), "{e}");
    }

    #[test]
    fn the_folders_a_library_cannot_go_into_are_refused_with_words() {
        let t = Fixture::new();
        let pl = t.places();
        let refused = |p: PathBuf| plan(&p, &pl, 10, &ROOMY, &peek).unwrap_err();
        assert!(refused(t.p("app/data/library")).contains("is in"));
        std::fs::create_dir_all(t.p("app/data/library/inner")).unwrap();
        assert!(refused(t.p("app/data/library/inner")).contains("inside the library"));
        assert!(refused(t.p("clips")).contains("is the clips folder"));
        std::fs::create_dir_all(t.p("clips/sub")).unwrap();
        assert!(refused(t.p("clips/sub")).contains("inside the clips folder"));
        std::fs::create_dir_all(t.p("app/config/x")).unwrap();
        assert!(refused(t.p("app/config/x")).contains("the app's own folder"));
        let net = Fake {
            kind: Kind::Network,
            ..ROOMY
        };
        assert!(plan(&t.p("d"), &pl, 10, &net, &peek)
            .unwrap_err()
            .contains("on the network"));
        assert!(refused(t.p("nowhere")).contains("is not a folder"));
        library(&t.p("d/copy"));
        let e = refused(t.p("d/copy/library"));
        assert!(
            e.contains("is inside the reel library in") && e.ends_with("or a folder outside it"),
            "{e}"
        );
    }

    #[test]
    fn a_full_target_is_refused_before_anything_moves_unless_it_is_the_same_drive() {
        let t = Fixture::new();
        let pl = t.places();
        let full = Fake {
            free: 2_000_000_000,
            same: false,
            kind: Kind::Fixed,
        };
        let e = plan(&t.p("d"), &pl, 5_000_000_000, &full, &peek).unwrap_err();
        assert!(
            e.contains("2.0 GB free") && e.contains("needs 5.0 GB"),
            "{e}"
        );
        let same = Fake { same: true, ..full };
        assert!(matches!(
            plan(&t.p("d"), &pl, 5_000_000_000, &same, &peek).unwrap(),
            Target::Move {
                same_drive: true,
                ..
            }
        ));
        assert!(
            t.p("app/data/library/reel.db").is_file(),
            "nothing was touched"
        );
    }

    #[test]
    fn a_folder_with_a_library_in_it_is_used_and_the_way_back_is_open() {
        let t = Fixture::new();
        library(&t.p("d/copy"));
        let pl = t.places();
        assert_eq!(
            plan(&t.p("d/copy"), &pl, 10, &ROOMY, &peek).unwrap(),
            Target::Use {
                dir: t.p("d/copy"),
                clips: 3
            }
        );
        let moved = Places {
            current: t.p("d/copy"),
            ..t.places()
        };
        match plan(&t.p("app/data/library"), &moved, 10, &ROOMY, &peek).unwrap() {
            Target::Use { dir, .. } => assert_eq!(dir, t.p("app/data/library")),
            other => panic!("{other:?}"),
        }
        std::fs::remove_dir_all(t.p("app/data/library")).unwrap();
        std::fs::create_dir_all(t.p("app/data/library")).unwrap();
        assert!(
            matches!(
                plan(&t.p("app/data/library"), &moved, 10, &ROOMY, &peek).unwrap(),
                Target::Move { .. }
            ),
            "the library's place of origin takes it back"
        );
    }

    struct Switched(Vec<PathBuf>);

    impl Switch for Switched {
        fn switch(&mut self, to: &Path) -> Result<(), String> {
            self.0.push(to.to_path_buf());
            Ok(())
        }
    }

    fn journal(t: &Fixture, to: &str, how: How) -> Journal {
        Journal {
            from: t.p("app/data/library"),
            to: t.p(to),
            how,
            made: !t.p(to).exists(),
            phase: Phase::Copying,
        }
    }

    fn files(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = copier::list(dir, &|_| false)
            .unwrap()
            .into_iter()
            .filter(|e| !e.dir)
            .map(|e| e.rel.to_string_lossy().replace('\\', "/"))
            .collect();
        v.sort();
        v
    }

    struct Refused;

    impl Switch for Refused {
        fn switch(&mut self, _: &Path) -> Result<(), String> {
            Err("the settings: no room".into())
        }
    }

    #[test]
    fn a_switch_that_fails_leaves_the_library_where_the_settings_name_it() {
        for how in [How::Copy, How::Rename] {
            let t = Fixture::new();
            let before = files(&t.p("app/data/library"));
            let file = t.p("app/data/library-move.json");
            let mut j = journal(&t, "d", how);
            let e = run(
                &mut j,
                &file,
                &AtomicBool::new(false),
                &|_| u64::MAX,
                &mut Refused,
                &mut |_, _, _| {},
            )
            .unwrap_err();
            assert!(e.contains("no room"), "{e}");
            assert_eq!(files(&t.p("app/data/library")), before, "{how:?}");
            assert!(files(&t.p("d")).is_empty(), "{how:?}");
            assert!(t.p("d").is_dir(), "the folder picked stays");
            assert!(
                !file.exists(),
                "no journal to roll forward at the next start"
            );
        }
    }

    #[test]
    fn a_move_on_the_same_drive_is_one_rename() {
        let t = Fixture::new();
        let before = files(&t.p("app/data/library"));
        let file = t.p("app/data/library-move.json");
        let mut j = journal(&t, "d/lib", How::Rename);
        let mut sw = Switched(vec![]);
        let mut phases = vec![];
        run(
            &mut j,
            &file,
            &AtomicBool::new(false),
            &|_| u64::MAX,
            &mut sw,
            &mut |p, _, _| phases.push(p),
        )
        .unwrap();
        assert_eq!(files(&t.p("d/lib")), before);
        assert!(!t.p("app/data/library").exists());
        assert_eq!(sw.0, [t.p("d/lib")]);
        assert_eq!(phases, [Phase::Renaming]);
        assert_eq!(j.how, How::Rename);
        remove_old(&j, &mut |_, _, _| {}).unwrap();
        assert!(t.p("d/lib/reel.db").is_file());
    }

    #[test]
    fn a_move_across_drives_copies_checks_switches_and_then_removes() {
        let t = Fixture::new();
        let before = files(&t.p("app/data/library"));
        let file = t.p("app/data/library-move.json");
        let mut j = journal(&t, "d", How::Copy);
        let mut sw = Switched(vec![]);
        let mut seen = vec![];
        run(
            &mut j,
            &file,
            &AtomicBool::new(false),
            &|_| u64::MAX,
            &mut sw,
            &mut |p, d, n| seen.push((p, d, n)),
        )
        .unwrap();
        assert_eq!(files(&t.p("d")), before, "everything is in the target");
        assert_eq!(
            files(&t.p("app/data/library")),
            before,
            "the old copy stays until the switch is done"
        );
        assert_eq!(sw.0, [t.p("d")]);
        assert_eq!(Journal::load(&file).unwrap().phase, Phase::Removing);
        assert!(seen.iter().any(|s| s.0 == Phase::Copying));
        let last = seen.last().unwrap();
        assert_eq!((last.0, last.1), (Phase::Checking, last.2));
        remove_old(&j, &mut |_, _, _| {}).unwrap();
        assert!(!t.p("app/data/library").exists(), "the old folder is gone");
        assert_eq!(files(&t.p("d")), before);
    }

    #[test]
    fn a_full_target_found_at_the_copy_leaves_everything_as_it_was() {
        let t = Fixture::new();
        let before = files(&t.p("app/data/library"));
        let file = t.p("app/data/library-move.json");
        let mut j = journal(&t, "d", How::Copy);
        let mut sw = Switched(vec![]);
        let e = run(
            &mut j,
            &file,
            &AtomicBool::new(false),
            &|_| 1_000,
            &mut sw,
            &mut |_, _, _| {},
        )
        .unwrap_err();
        assert!(e.contains("1 kB free"), "{e}");
        assert!(sw.0.is_empty());
        assert_eq!(files(&t.p("app/data/library")), before);
        assert!(files(&t.p("d")).is_empty());
    }

    #[test]
    fn a_stop_halfway_rolls_back_and_so_does_the_next_start_after_a_crash() {
        let t = Fixture::new();
        let before = files(&t.p("app/data/library"));
        let file = t.p("app/data/library-move.json");
        let mut j = journal(&t, "e/new", How::Copy);
        let stop = AtomicBool::new(false);
        let mut sw = Switched(vec![]);
        let e = run(
            &mut j,
            &file,
            &stop,
            &|_| u64::MAX,
            &mut sw,
            &mut |_, d, _| {
                if d > 1 << 20 {
                    stop.store(true, Ordering::SeqCst);
                }
            },
        )
        .unwrap_err();
        assert_eq!(e, "stopped");
        assert!(sw.0.is_empty());
        assert!(!t.p("e/new").exists(), "the folder the move made is gone");
        assert!(!file.exists());
        assert_eq!(files(&t.p("app/data/library")), before);

        // the app died while copying: the journal says so, half the files are there
        let j = journal(&t, "e/new", How::Copy);
        j.save(&file).unwrap();
        std::fs::create_dir_all(t.p("e/new/library/C1")).unwrap();
        std::fs::write(t.p("e/new/library/C1/master.mp4"), b"half").unwrap();
        std::fs::write(t.p("e/new/reel.db"), b"half").unwrap();
        let mut found = Journal::load(&file).unwrap();
        let holds = |p: &Path| reel_pipeline::library::holds_library(p).is_some();
        assert_eq!(recovery(&found, &holds), Recovery::RollBack);
        let settled = settle(&mut found, &file, false, &mut |_| unreachable!(), &holds);
        assert_eq!(settled, Settled::Stayed);
        assert!(!t.p("e/new").exists());
        assert!(!file.exists());
        assert_eq!(files(&t.p("app/data/library")), before);
        assert!(settled_line(&found, &settled).contains("nothing was lost"));
    }

    #[test]
    fn a_start_after_the_check_finishes_the_move() {
        let t = Fixture::new();
        let before = files(&t.p("app/data/library"));
        let holds = |p: &Path| reel_pipeline::library::holds_library(p).is_some();
        library(&t.p("d"));
        let mut j = journal(&t, "d", How::Copy);
        j.phase = Phase::Switching;
        assert_eq!(recovery(&j, &holds), Recovery::Forward);
        j.phase = Phase::Removing;
        assert_eq!(recovery(&j, &holds), Recovery::Remove);
        remove_old(&j, &mut |_, _, _| {}).unwrap();
        assert!(!t.p("app/data/library").exists());
        assert_eq!(files(&t.p("d")), before);

        // a rename the app did not see through: the folder that holds the library wins
        let mut r = Journal {
            from: t.p("d"),
            to: t.p("e/lib"),
            how: How::Rename,
            made: true,
            phase: Phase::Renaming,
        };
        assert_eq!(recovery(&r, &holds), Recovery::Nothing);
        std::fs::rename(t.p("d"), t.p("e/lib")).unwrap();
        assert_eq!(recovery(&r, &holds), Recovery::Forward);
        r.phase = Phase::Removing;
        remove_old(&r, &mut |_, _, _| {}).unwrap();
        assert!(
            t.p("e/lib/reel.db").is_file(),
            "a rename leaves nothing to remove"
        );
    }

    /// A move whose copy was checked, its journal at Switching: the app stopped before the
    /// settings named the target.
    fn checked(t: &Fixture) -> (Journal, PathBuf) {
        let from = t.p("app/data/library");
        let entries = copier::list(&from, &|_| false).unwrap();
        copier::copy_all(
            &from,
            &t.p("d"),
            &entries,
            None,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap();
        let mut j = journal(t, "d", How::Copy);
        j.made = false;
        j.phase = Phase::Switching;
        let file = t.p("app/data/library-move.json");
        j.save(&file).unwrap();
        (j, file)
    }

    fn holds(p: &Path) -> bool {
        reel_pipeline::library::holds_library(p).is_some()
    }

    #[test]
    fn a_start_that_cannot_write_the_settings_keeps_the_library_where_they_name_it() {
        let t = Fixture::new();
        let before = files(&t.p("app/data/library"));
        let (mut j, file) = checked(&t);
        let mut tried = vec![];
        let settled = settle(
            &mut j,
            &file,
            false,
            &mut |to| {
                tried.push(to.to_path_buf());
                Err("the settings: No space left on device".into())
            },
            &holds,
        );
        assert_eq!(
            tried,
            [t.p("d")],
            "the settings were asked to name the target"
        );
        assert!(matches!(&settled, Settled::Unwritten(e) if e.contains("No space left")));
        assert_eq!(
            files(&t.p("app/data/library")),
            before,
            "the old folder is whole"
        );
        assert!(files(&t.p("d")).is_empty(), "the copy went");
        assert!(!file.exists(), "nothing is left for the next start");
        let line = settled_line(&j, &settled);
        assert!(
            line.contains("could not write the settings (the settings: No space left on device)")
                && line.contains(&format!(
                    "The library stays in {}, whole",
                    t.p("app/data/library").display()
                )),
            "{line}"
        );
        let e = remove_moved(&j, &t.p("app/data/library"), &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("so the old folder"), "{e}");
        assert_eq!(files(&t.p("app/data/library")), before);
    }

    #[test]
    fn a_rename_the_settings_could_not_follow_is_undone() {
        let t = Fixture::new();
        let before = files(&t.p("app/data/library"));
        let file = t.p("app/data/library-move.json");
        let mut j = journal(&t, "e/lib", How::Rename);
        j.phase = Phase::Renaming;
        j.save(&file).unwrap();
        std::fs::rename(t.p("app/data/library"), t.p("e/lib")).unwrap();
        let settled = settle(&mut j, &file, false, &mut |_| Err("full".into()), &holds);
        assert_eq!(settled, Settled::Unwritten("full".into()));
        assert_eq!(files(&t.p("app/data/library")), before, "renamed back");
        assert!(!t.p("e/lib").exists());
        assert!(!file.exists());
    }

    #[test]
    fn a_removal_begun_is_not_undone_and_waits_for_the_settings() {
        let t = Fixture::new();
        let (mut j, file) = checked(&t);
        j.phase = Phase::Removing;
        j.save(&file).unwrap();
        // the removal had begun: the old folder is not whole any more
        std::fs::remove_file(t.p("app/data/library/reel.db")).unwrap();
        let left = files(&t.p("app/data/library"));
        let settled = settle(&mut j, &file, false, &mut |_| Err("full".into()), &holds);
        assert_eq!(settled, Settled::TargetForNow("full".into()));
        assert_eq!(
            Journal::load(&file).unwrap().phase,
            Phase::Removing,
            "kept for the next start"
        );
        assert_eq!(
            files(&t.p("app/data/library")),
            left,
            "nothing more removed"
        );
        assert!(holds(&t.p("d")), "the target is whole");
        assert!(settled_line(&j, &settled).contains("reel on this PC runs from"));
    }

    #[test]
    fn a_start_that_writes_the_settings_finishes_the_move_and_only_then_removes() {
        let t = Fixture::new();
        let before = files(&t.p("app/data/library"));
        let (mut j, file) = checked(&t);
        let settled = settle(&mut j, &file, false, &mut |_| Ok(()), &holds);
        assert_eq!(settled, Settled::Moved);
        assert_eq!(Journal::load(&file).unwrap().phase, Phase::Removing);
        assert_eq!(
            files(&t.p("app/data/library")),
            before,
            "settle removes nothing"
        );
        remove_moved(&j, &t.p("d"), &mut |_, _, _| {}).unwrap();
        assert!(!t.p("app/data/library").exists());
        assert_eq!(files(&t.p("d")), before);

        // the settings named the target already: nothing is written
        let t = Fixture::new();
        let (mut j, file) = checked(&t);
        let settled = settle(&mut j, &file, true, &mut |_| unreachable!(), &holds);
        assert_eq!(settled, Settled::Moved);
    }

    #[test]
    fn the_old_folder_stays_when_the_new_one_does_not_hold_the_library() {
        let t = Fixture::new();
        let j = Journal {
            from: t.p("app/data/library"),
            to: t.p("d"),
            how: How::Copy,
            made: false,
            phase: Phase::Removing,
        };
        let e = remove_old(&j, &mut |_, _, _| {}).unwrap_err();
        assert!(e.contains("does not hold the library"), "{e}");
        assert!(t.p("app/data/library/reel.db").is_file());
    }
}
