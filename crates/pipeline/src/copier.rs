//! Copies files to another folder and proves the copy before anything relies on it: each file's
//! SHA-256 is taken as it is read, the copy is flushed to its disk, then read back and compared.
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const BUF: usize = 1 << 20;

/// One thing under a folder, by its path relative to that folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub rel: PathBuf,
    pub size: u64,
    pub dir: bool,
}

/// A file copied, with the hash of the bytes read from where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    pub rel: PathBuf,
    pub size: u64,
    pub hash: [u8; 32],
}

/// The error a stop asked for returns with, so callers can tell it from a failure.
pub fn stopped() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Interrupted, "stopped")
}

pub fn is_stopped(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::Interrupted && e.to_string() == "stopped"
}

/// Every folder and file under `root`, folders before what they hold, in a stable order. A link
/// is refused rather than followed: reel makes none, and a copy must not leave its tree.
pub fn list(root: &Path, skip: &dyn Fn(&Path) -> bool) -> std::io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let mut names: Vec<_> =
            std::fs::read_dir(root.join(&rel))?.collect::<std::io::Result<Vec<_>>>()?;
        names.sort_by_key(|e| e.file_name());
        let mut dirs = Vec::new();
        for e in names {
            let r = rel.join(e.file_name());
            if skip(&r) {
                continue;
            }
            let t = e.file_type()?;
            if t.is_symlink() {
                return Err(std::io::Error::other(format!(
                    "{} is a link; reel's folders hold none",
                    root.join(&r).display()
                )));
            }
            if t.is_dir() {
                out.push(Entry {
                    rel: r.clone(),
                    size: 0,
                    dir: true,
                });
                dirs.push(r);
            } else {
                out.push(Entry {
                    rel: r,
                    size: e.metadata()?.len(),
                    dir: false,
                });
            }
        }
        stack.extend(dirs.into_iter().rev());
    }
    Ok(out)
}

pub fn total(entries: &[Entry]) -> u64 {
    entries.iter().map(|e| e.size).sum()
}

/// Copies one file, hashing what it reads; the copy is on its disk when this returns.
/// `tick` gets the bytes as they go.
pub fn copy_file(
    from: &Path,
    to: &Path,
    stop: &AtomicBool,
    tick: &mut dyn FnMut(u64),
) -> std::io::Result<Copied> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut src = std::fs::File::open(from)?;
    let modified = src.metadata().and_then(|m| m.modified()).ok();
    let mut dst = std::fs::File::create(to)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BUF];
    let mut size = 0u64;
    loop {
        if stop.load(Ordering::SeqCst) {
            return Err(stopped());
        }
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        dst.write_all(&buf[..n])?;
        size += n as u64;
        tick(n as u64);
    }
    if let Some(t) = modified {
        let _ = dst.set_modified(t);
    }
    dst.sync_all()?;
    Ok(Copied {
        rel: PathBuf::new(),
        size,
        hash: hasher.finalize().into(),
    })
}

/// A file's size and SHA-256, read from its disk.
pub fn hash_file(
    path: &Path,
    stop: &AtomicBool,
    tick: &mut dyn FnMut(u64),
) -> std::io::Result<(u64, [u8; 32])> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BUF];
    let mut size = 0u64;
    loop {
        if stop.load(Ordering::SeqCst) {
            return Err(stopped());
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
        tick(n as u64);
    }
    Ok((size, hasher.finalize().into()))
}

/// Copies `entries` from under `from` to under `to`; the marker file, when there is one among
/// them, goes last, so the target is a library only once the rest is there.
pub fn copy_all(
    from: &Path,
    to: &Path,
    entries: &[Entry],
    last: Option<&Path>,
    stop: &AtomicBool,
    tick: &mut dyn FnMut(u64),
) -> std::io::Result<Vec<Copied>> {
    std::fs::create_dir_all(to)?;
    let mut out = Vec::with_capacity(entries.len());
    let (late, early): (Vec<&Entry>, Vec<&Entry>) = entries
        .iter()
        .partition(|e| last.is_some_and(|l| e.rel == l));
    for e in early.into_iter().chain(late) {
        if stop.load(Ordering::SeqCst) {
            return Err(stopped());
        }
        if e.dir {
            std::fs::create_dir_all(to.join(&e.rel))?;
            continue;
        }
        let mut c = copy_file(&from.join(&e.rel), &to.join(&e.rel), stop, tick)
            .map_err(|err| with_path(err, &from.join(&e.rel)))?;
        c.rel = e.rel.clone();
        out.push(c);
    }
    Ok(out)
}

/// Reads every copied file back from under `to`: its size and hash as they were read at the
/// source, or the first that differs as an error.
pub fn check_all(
    to: &Path,
    copied: &[Copied],
    stop: &AtomicBool,
    tick: &mut dyn FnMut(u64),
) -> std::io::Result<()> {
    for c in copied {
        let at = to.join(&c.rel);
        let (size, hash) = hash_file(&at, stop, tick).map_err(|e| with_path(e, &at))?;
        if size != c.size {
            return Err(std::io::Error::other(format!(
                "{} reads back as {size} bytes, not {}",
                at.display(),
                c.size
            )));
        }
        if hash != c.hash {
            return Err(std::io::Error::other(format!(
                "{} does not read back as it was written",
                at.display()
            )));
        }
    }
    Ok(())
}

/// Takes away under `to` what a copy of `entries` put there: only those names, files first, then
/// the folders that are empty after, so nothing else in `to` is touched.
pub fn undo(to: &Path, entries: &[Entry]) {
    for e in entries.iter().filter(|e| !e.dir) {
        let _ = std::fs::remove_file(to.join(&e.rel));
    }
    for e in entries.iter().rev().filter(|e| e.dir) {
        let _ = std::fs::remove_dir(to.join(&e.rel));
    }
}

fn with_path(e: std::io::Error, p: &Path) -> std::io::Error {
    if is_stopped(&e) {
        return e;
    }
    std::io::Error::new(e.kind(), format!("{}: {e}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(root: &Path) {
        std::fs::create_dir_all(root.join("C1/v2")).unwrap();
        std::fs::create_dir_all(root.join(".trash/C0")).unwrap();
        std::fs::create_dir_all(root.join("empty")).unwrap();
        std::fs::write(root.join("C1/master.mp4"), vec![7u8; 3 * BUF + 5]).unwrap();
        std::fs::write(root.join("C1/v2/cut.mp4"), b"cut").unwrap();
        std::fs::write(root.join(".trash/C0/master.mp4"), b"gone").unwrap();
        std::fs::write(root.join(".reel-library"), b"instance X\n").unwrap();
    }

    #[test]
    fn a_tree_is_copied_checked_and_undone_by_its_own_names() {
        let tmp = tempfile::tempdir().unwrap();
        let (from, to) = (tmp.path().join("a"), tmp.path().join("b"));
        tree(&from);
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(to.join("theirs.txt"), b"not reel's").unwrap();
        let entries = list(&from, &|_| false).unwrap();
        let mut names: Vec<String> = entries
            .iter()
            .map(|e| e.rel.to_string_lossy().replace('\\', "/"))
            .collect();
        let at = |n: &str| names.iter().position(|x| x == n).unwrap();
        assert!(at("C1") < at("C1/v2") && at("C1/v2") < at("C1/v2/cut.mp4"));
        names.sort();
        assert_eq!(
            names,
            [
                ".reel-library",
                ".trash",
                ".trash/C0",
                ".trash/C0/master.mp4",
                "C1",
                "C1/master.mp4",
                "C1/v2",
                "C1/v2/cut.mp4",
                "empty"
            ]
        );
        assert_eq!(total(&entries), (3 * BUF + 5 + 3 + 4 + 11) as u64);
        let stop = AtomicBool::new(false);
        let mut seen = 0;
        let copied = copy_all(
            &from,
            &to,
            &entries,
            Some(Path::new(".reel-library")),
            &stop,
            &mut |n| seen += n,
        )
        .unwrap();
        assert_eq!(seen, total(&entries));
        assert_eq!(
            copied.last().unwrap().rel,
            PathBuf::from(".reel-library"),
            "the marker goes last"
        );
        assert!(to.join("empty").is_dir());
        check_all(&to, &copied, &stop, &mut |_| {}).unwrap();
        std::fs::write(to.join("C1/v2/cut.mp4"), b"cux").unwrap();
        let e = check_all(&to, &copied, &stop, &mut |_| {}).unwrap_err();
        assert!(e.to_string().contains("does not read back"), "{e}");
        std::fs::write(to.join("C1/v2/cut.mp4"), b"cu").unwrap();
        let e = check_all(&to, &copied, &stop, &mut |_| {}).unwrap_err();
        assert!(e.to_string().contains("2 bytes, not 3"), "{e}");
        undo(&to, &entries);
        let left: Vec<_> = std::fs::read_dir(&to)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["theirs.txt"], "only what the copy put there goes");
        assert!(
            from.join("C1/master.mp4").is_file(),
            "the source is untouched"
        );
    }

    #[test]
    fn a_stop_ends_the_copy_and_says_it_was_a_stop() {
        let tmp = tempfile::tempdir().unwrap();
        let (from, to) = (tmp.path().join("a"), tmp.path().join("b"));
        tree(&from);
        let entries = list(&from, &|_| false).unwrap();
        let stop = AtomicBool::new(false);
        let mut n = 0;
        let e = copy_all(&from, &to, &entries, None, &stop, &mut |b| {
            n += b;
            if n > BUF as u64 {
                stop.store(true, Ordering::SeqCst);
            }
        })
        .unwrap_err();
        assert!(is_stopped(&e), "{e}");
        undo(&to, &entries);
        assert_eq!(std::fs::read_dir(&to).unwrap().count(), 0);
    }

    #[test]
    fn what_is_skipped_is_not_listed() {
        let tmp = tempfile::tempdir().unwrap();
        tree(tmp.path());
        let entries = list(tmp.path(), &|r| r.starts_with(".trash")).unwrap();
        assert!(entries.iter().all(|e| !e.rel.starts_with(".trash")));
        assert!(entries.iter().any(|e| e.rel == Path::new("C1/v2/cut.mp4")));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        tree(tmp.path());
        std::os::unix::fs::symlink("/etc", tmp.path().join("C1/out")).unwrap();
        let e = list(tmp.path(), &|_| false).unwrap_err();
        assert!(e.to_string().contains("is a link"), "{e}");
    }
}
