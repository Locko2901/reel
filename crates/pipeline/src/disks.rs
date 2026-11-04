//! The disks reel's folders are on, each once (by device) and named for what is on it, and
//! whether a folder is on a network share, where SQLite's locking is not to be trusted.
use reel_core::config::KeepOriginals;
use reel_core::format::size;
use reel_core::Config;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub id: u64,
    pub free: u64,
    pub total: u64,
    /// The folders on it: data, library, originals.
    pub holds: Vec<&'static str>,
}

impl Disk {
    /// What is on it, as `data and originals`; the library is left out inside the data folder.
    pub fn name(&self, library_elsewhere: bool) -> String {
        let parts: Vec<&str> = self
            .holds
            .iter()
            .copied()
            .filter(|h| !(*h == "library" && !library_elsewhere && self.holds.contains(&"data")))
            .collect();
        match parts.as_slice() {
            [] => String::new(),
            [one] => one.to_string(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        }
    }

    pub fn line(&self, library_elsewhere: bool) -> String {
        format!(
            "{} disk {} free of {}",
            self.name(library_elsewhere),
            size(self.free),
            size(self.total)
        )
    }

    pub fn low(&self, warn_gb: u64) -> bool {
        warn_gb > 0 && self.total > 0 && self.free < warn_gb * 1_000_000_000
    }
}

/// A folder's disk: its device, free and total bytes.
pub type DiskAt = (u64, u64, u64);

/// None when the folder is not there.
pub type Probe = dyn Fn(&Path) -> Option<DiskAt>;

pub fn probe(path: &Path) -> Option<DiskAt> {
    let meta = std::fs::metadata(path).ok()?;
    let (free, total) = crate::notify::free_and_total(path);
    Some((device(path, &meta), free, total))
}

#[cfg(unix)]
fn device(_path: &Path, meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.dev()
}

#[cfg(not(unix))]
fn device(path: &Path, _meta: &std::fs::Metadata) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let root = path
        .components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase());
    root.hash(&mut h);
    h.finish()
}

/// The folders that hold reel's data: the data folder, the library, and the kept originals
/// once their folder is there. The incoming folder is a drop that reel empties, not its data.
pub fn reel_folders(cfg: &Config) -> Vec<(&'static str, PathBuf)> {
    let mut out = vec![
        ("data", cfg.data_dir.clone()),
        ("library", cfg.library_dir()),
    ];
    if cfg.ingest.keep_originals != KeepOriginals::None {
        out.push(("originals", cfg.ingest.originals_dir.clone()));
    }
    out
}

/// Each distinct disk once, in the folders' order; a folder that is not there has none.
pub fn disks_with(folders: &[(&'static str, PathBuf)], probe: &Probe) -> Vec<Disk> {
    let mut out: Vec<Disk> = Vec::new();
    for (name, dir) in folders {
        let Some((id, free, total)) = probe(dir) else {
            continue;
        };
        match out.iter_mut().find(|d| d.id == id) {
            Some(d) => d.holds.push(name),
            None => out.push(Disk {
                id,
                free,
                total,
                holds: vec![name],
            }),
        }
    }
    out
}

pub fn disks(cfg: &Config) -> Vec<Disk> {
    disks_with(&reel_folders(cfg), &probe)
}

/// The kind of network file system a statfs magic number names, or None for a local one.
pub fn network_kind(magic: u64) -> Option<&'static str> {
    match magic as u32 {
        0x6969 => Some("NFS"),
        0x517B | 0xFE53_4D42 => Some("SMB"),
        0xFF53_4D42 => Some("CIFS"),
        0x0102_1997 => Some("9P"),
        0x5346_414F | 0x6B41_4653 => Some("AFS"),
        0x00C3_6400 => Some("Ceph"),
        0x7375_7245 => Some("Coda"),
        0x564C => Some("NCP"),
        _ => None,
    }
}

/// The statfs magic number of the file system under a path (Linux only).
pub fn fs_magic(path: &Path) -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        nix::sys::statfs::statfs(path)
            .ok()
            .map(|s| s.filesystem_type().0 as u64)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        None
    }
}

/// The network share a folder is on, if it is on one.
pub fn network_share(path: &Path) -> Option<&'static str> {
    fs_magic(path).and_then(network_kind)
}

/// The warning for the database on a network share, from a probe of the data folder's
/// file system; nothing is refused.
pub fn database_warning(data_dir: &Path, magic: &dyn Fn(&Path) -> Option<u64>) -> Option<String> {
    magic(data_dir).and_then(network_kind).map(share_warning)
}

pub fn share_warning(kind: &str) -> String {
    format!(
        "the data folder is on a network share ({kind}): SQLite's locking is not reliable over the network, so the database can be damaged; keep the data folder on a local disk and the library on the share"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(path: &Path) -> Option<(u64, u64, u64)> {
        let p = path.to_string_lossy();
        if p.starts_with("/mnt/nas") {
            Some((2, 14_000_000_000_000, 15_000_000_000_000))
        } else if p.starts_with("/gone") {
            None
        } else {
            Some((1, 371_500_000_000, 485_200_000_000))
        }
    }

    fn folders(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, PathBuf)> {
        pairs.iter().map(|(n, p)| (*n, PathBuf::from(p))).collect()
    }

    #[test]
    fn each_disk_once_named_for_what_is_on_it() {
        let same = disks_with(
            &folders(&[("data", "/data"), ("library", "/data/library")]),
            &fake,
        );
        assert_eq!(same.len(), 1);
        assert_eq!(
            same[0].line(false),
            "data disk 371.5 GB free of 485.2 GB",
            "nothing set: the line of before"
        );
        assert_eq!(same[0].name(true), "data and library");
        let nas = disks_with(
            &folders(&[
                ("data", "/data"),
                ("library", "/mnt/nas/reel"),
                ("originals", "/mnt/nas/originals"),
            ]),
            &fake,
        );
        assert_eq!(nas.len(), 2);
        assert_eq!(nas[0].line(true), "data disk 371.5 GB free of 485.2 GB");
        assert_eq!(
            nas[1].line(true),
            "library and originals disk 14.0 TB free of 15.0 TB"
        );
        assert_eq!(
            nas[1].name(false),
            "library and originals",
            "a library on a disk of its own is named whatever the setting"
        );
        let gone = disks_with(
            &folders(&[("data", "/data"), ("originals", "/gone")]),
            &fake,
        );
        assert_eq!(gone.len(), 1, "a folder that is not there has no disk");
    }

    #[test]
    fn the_warning_applies_to_each_disk() {
        let d = disks_with(
            &folders(&[("data", "/data"), ("library", "/mnt/nas/reel")]),
            &fake,
        );
        assert!(!d[0].low(20) && !d[1].low(20));
        assert!(d[0].low(400), "371.5 GB free is under 400 GB");
        assert!(!d[1].low(400));
        assert!(!d[0].low(0), "0 turns it off");
    }

    #[test]
    fn a_network_share_is_told_apart_by_its_magic_number() {
        assert_eq!(network_kind(0x6969), Some("NFS"));
        assert_eq!(network_kind(0xFF53_4D42), Some("CIFS"));
        assert_eq!(network_kind(0xFE53_4D42), Some("SMB"));
        assert_eq!(network_kind(0xEF53), None, "ext4");
        assert_eq!(network_kind(0x5846_5342), None, "xfs");
        assert_eq!(network_kind(0x9123_683E), None, "btrfs");
        assert_eq!(network_kind(0x0102_1994), None, "tmpfs");
        let nfs = |_: &Path| Some(0x6969);
        let ext4 = |_: &Path| Some(0xEF53);
        let w = database_warning(Path::new("/data"), &nfs).unwrap();
        assert!(
            w.contains("network share (NFS)") && w.contains("SQLite"),
            "{w}"
        );
        assert_eq!(database_warning(Path::new("/data"), &ext4), None);
        assert_eq!(database_warning(Path::new("/data"), &|_| None), None);
    }

    #[test]
    fn this_machine_has_a_disk_and_a_local_file_system() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, free, total) = probe(tmp.path()).unwrap();
        assert!(total > 0 && free <= total);
        assert!(probe(&tmp.path().join("missing")).is_none());
        if cfg!(target_os = "linux") {
            assert!(fs_magic(tmp.path()).is_some());
        }
    }
}
