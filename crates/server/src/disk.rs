use reel_core::config::{KeepOriginals, MARKER, TRASH};
use reel_core::format;
use reel_core::settings::{Effective, Source};
use reel_pipeline::disks::Disk;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct DiskInfo {
    pub library: u64,
    pub cache: u64,
    pub web: u64,
    pub trash: u64,
    pub uploads: u64,
    pub originals: Originals,
    /// The data disk's.
    pub free: u64,
    pub total: u64,
    /// Each disk that holds reel's data once, the data disk first.
    pub disks: Vec<Disk>,
    pub library_elsewhere: bool,
    /// The network share each of these folders is on, if one.
    pub data_share: Option<&'static str>,
    pub library_share: Option<&'static str>,
    /// Settings' four folders by key, each with its disk (device, free, total) if it is there.
    pub folders: Vec<(&'static str, Option<reel_pipeline::disks::DiskAt>)>,
    pub computed: Instant,
    pub standalone: bool,
}

/// The kept originals, as Keep originals and the Originals folder have them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Originals {
    /// Keep originals is none.
    Off,
    /// The folder was never set and is not there: nothing kept yet, it is made on first use.
    NoneYet,
    Kept(u64),
    /// The Originals folder was set and is not there.
    Unreachable,
}

impl Originals {
    pub fn bytes(self) -> Option<u64> {
        match self {
            Originals::Kept(b) => Some(b),
            _ => None,
        }
    }
}

impl DiskInfo {
    pub fn used(&self) -> u64 {
        self.library + self.cache + self.web + self.trash + self.uploads
    }

    pub fn line(&self, clips: u64) -> String {
        format!("Clips {}", format::size(clips))
    }

    /// What the library takes on disk, in words; standalone has no preview videos or web copies.
    pub fn detail(&self, clips: u64) -> String {
        let originals = match self.originals {
            Originals::Kept(b) => format!("; kept originals {}", format::size(b)),
            Originals::Unreachable => "; the Originals folder is not reachable".into(),
            Originals::Off | Originals::NoneYet => String::new(),
        };
        if self.standalone {
            return format!(
                "every version of every clip (originals and cuts) {}; the library on disk with posters, loops and filmstrips {}; copies under a size {}, trash {}, uploads {}{}; {}",
                format::size(clips),
                format::size(self.library),
                format::size(self.cache),
                format::size(self.trash),
                format::size(self.uploads),
                originals,
                self.disks_line()
            );
        }
        format!(
            "every version of every clip (originals, cuts, preview videos) {}; the library on disk with posters, loops and filmstrips {}; caches {} (copies under a size {}, web copies for the links' pages {}), trash {}, uploads {}{}; {}",
            format::size(clips),
            format::size(self.library),
            format::size(self.cache + self.web),
            format::size(self.cache),
            format::size(self.web),
            format::size(self.trash),
            format::size(self.uploads),
            originals,
            self.disks_line()
        )
    }

    /// `data disk 371.5 GB free of 485.2 GB; library disk 14.0 TB free of 15.0 TB`.
    pub fn disks_line(&self) -> String {
        if self.disks.is_empty() {
            return format!(
                "data disk {} free of {}",
                format::size(self.free),
                format::size(self.total)
            );
        }
        self.disks
            .iter()
            .map(|d| d.line(self.library_elsewhere))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// The line under a folder in Settings: its disk, and whether that is the data folder's.
    pub fn folder_line(&self, key: &str) -> String {
        let disk = |k: &str| self.folders.iter().find(|(f, _)| *f == k).map(|(_, d)| *d);
        let Some(of) = disk(key) else {
            return String::new();
        };
        let Some((id, free, total)) = of else {
            return "not there".into();
        };
        let room = format!("{} free of {}", format::size(free), format::size(total));
        let data = disk("data_dir").flatten().map(|d| d.0);
        let share = match key {
            "data_dir" => self.data_share,
            "library_dir" => self.library_share,
            _ => None,
        }
        .map(|k| format!(", a network share ({k})"))
        .unwrap_or_default();
        if key == "data_dir" {
            format!("on a disk with {room}{share}")
        } else if data == Some(id) {
            format!("on the data disk, {room}{share}")
        } else {
            format!("on another disk than the data folder, {room}{share}")
        }
    }

    /// The disks under the Disk warning, by their names.
    pub fn low(&self, warn_gb: u64) -> Vec<String> {
        self.disks
            .iter()
            .filter(|d| d.low(warn_gb))
            .map(|d| d.name(self.library_elsewhere))
            .collect()
    }

    pub fn to_json(&self, clips: u64) -> serde_json::Value {
        serde_json::json!({
            "clips": clips, "used": self.used(), "library": self.library, "cache": self.cache, "web": self.web, "trash": self.trash, "uploads": self.uploads,
            "originals": self.originals.bytes(), "free": self.free, "total": self.total,
            "disks": self.disks.iter().map(|d| serde_json::json!({ "name": d.name(self.library_elsewhere), "free": d.free, "total": d.total })).collect::<Vec<_>>(),
            "line": self.line(clips), "detail": self.detail(clips), "age_s": self.computed.elapsed().as_secs(),
        })
    }
}

pub fn dir_size(path: &Path) -> u64 {
    dir_size_but(path, &[])
}

/// The size of a folder without some of its own entries (the library without its trash).
fn dir_size_but(path: &Path, skip: &[&str]) -> u64 {
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            if dir == path && skip.iter().any(|s| entry.file_name() == *s) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

pub fn free_and_total(path: &Path) -> (u64, u64) {
    reel_pipeline::notify::free_and_total(path)
}

pub fn compute(eff: &Effective) -> DiskInfo {
    let config = &eff.config;
    let disks = reel_pipeline::disks::disks(config);
    let (free, total) = match disks.iter().find(|d| d.holds.contains(&"data")) {
        Some(d) => (d.free, d.total),
        None => free_and_total(&config.data_dir),
    };
    let dir = &config.ingest.originals_dir;
    let originals = if config.ingest.keep_originals == KeepOriginals::None {
        Originals::Off
    } else if dir.is_dir() {
        Originals::Kept(dir_size(dir))
    } else if eff.source("ingest.originals_dir") == Source::Default {
        Originals::NoneYet
    } else {
        Originals::Unreachable
    };
    DiskInfo {
        library: dir_size_but(&config.library_dir(), &[TRASH, MARKER]),
        cache: dir_size(&config.cache_dir()),
        web: dir_size(&config.web_cache_dir()),
        trash: dir_size(&config.trash_dir()) + dir_size(&config.old_trash_dir()),
        uploads: dir_size(&config.uploads_dir()),
        originals,
        free,
        total,
        disks,
        library_elsewhere: config.library_elsewhere(),
        data_share: reel_pipeline::disks::network_share(&config.data_dir),
        library_share: reel_pipeline::disks::network_share(&config.library_dir()),
        folders: [
            ("data_dir", config.data_dir.clone()),
            ("library_dir", config.library_dir()),
            ("ingest.incoming_dir", config.ingest.incoming_dir.clone()),
            ("ingest.originals_dir", config.ingest.originals_dir.clone()),
        ]
        .into_iter()
        .map(|(k, p)| (k, reel_pipeline::disks::probe(&p)))
        .collect(),
        computed: Instant::now(),
        standalone: config.standalone(),
    }
}

#[derive(Clone, Default)]
pub struct DiskCache {
    inner: Arc<Mutex<Option<DiskInfo>>>,
}

impl DiskCache {
    /// Counts the trash afresh each time, so every page has the same figure right after a delete.
    pub async fn get(&self, eff: &Arc<Effective>, refresh: bool) -> DiskInfo {
        let mut slot = self.inner.lock().await;
        if let Some(info) = slot.as_mut() {
            if !refresh && info.computed.elapsed() < Duration::from_secs(60) {
                let (new, old) = (eff.config.trash_dir(), eff.config.old_trash_dir());
                if let Ok(t) =
                    tokio::task::spawn_blocking(move || dir_size(&new) + dir_size(&old)).await
                {
                    info.trash = t;
                }
                return info.clone();
            }
        }
        let e = eff.clone();
        let info = tokio::task::spawn_blocking(move || compute(&e))
            .await
            .unwrap_or_else(|_| compute(eff));
        *slot = Some(info.clone());
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reel_core::Config;
    use std::collections::HashMap;

    fn info(originals: Originals) -> String {
        DiskInfo {
            library: 0,
            cache: 0,
            web: 0,
            trash: 0,
            uploads: 0,
            originals,
            free: 0,
            total: 0,
            disks: vec![],
            library_elsewhere: false,
            data_share: None,
            library_share: None,
            folders: vec![],
            computed: Instant::now(),
            standalone: false,
        }
        .detail(0)
    }

    #[test]
    fn the_storage_line_names_kept_originals_as_the_setting_does() {
        assert!(info(Originals::Kept(67_900_000_000))
            .contains("uploads 0 B; kept originals 67.9 GB; data disk"));
        assert!(info(Originals::Unreachable)
            .contains("; the Originals folder is not reachable; data disk"));
        for quiet in [Originals::Off, Originals::NoneYet] {
            let line = info(quiet);
            assert!(
                !line.contains("kept originals") && !line.contains("not reachable"),
                "{quiet:?}: {line}"
            );
        }
    }

    #[test]
    fn the_library_figure_leaves_out_its_trash_and_the_trash_counts_both_places() {
        let root = std::env::temp_dir().join(format!("reel-disk-lib-{}", std::process::id()));
        let lib = root.join("nas");
        std::fs::create_dir_all(lib.join("C1")).unwrap();
        std::fs::create_dir_all(lib.join(TRASH).join("C2")).unwrap();
        std::fs::create_dir_all(root.join("data/trash/C3")).unwrap();
        std::fs::write(lib.join("C1/master.mp4"), [0u8; 100]).unwrap();
        std::fs::write(lib.join(TRASH).join("C2/master.mp4"), [0u8; 30]).unwrap();
        std::fs::write(root.join("data/trash/C3/master.mp4"), [0u8; 7]).unwrap();
        std::fs::write(lib.join(MARKER), "instance X\n").unwrap();
        let mut cfg = Config {
            data_dir: root.join("data"),
            library_dir: lib.clone(),
            ..Config::default()
        };
        cfg.ingest.keep_originals = KeepOriginals::None;
        let info = compute(&Effective {
            config: Arc::new(cfg),
            sources: HashMap::new(),
            table: HashMap::new(),
        });
        assert_eq!(info.library, 100);
        assert_eq!(info.trash, 37);
        assert!(info.library_elsewhere);
        assert_eq!(info.disks.len(), 1, "one disk under the test's two folders");
        assert!(info.disks_line().starts_with("data and library disk "));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_originals_folder_is_not_reachable_only_when_it_was_set() {
        let root = std::env::temp_dir().join(format!("reel-disk-{}", std::process::id()));
        std::fs::create_dir_all(root.join("kept")).unwrap();
        std::fs::write(root.join("kept/a.mkv"), [0u8; 10]).unwrap();
        let state = |dir: &str, keep: KeepOriginals, source: Source| {
            let mut cfg = Config {
                data_dir: root.clone(),
                ..Config::default()
            };
            cfg.ingest.originals_dir = root.join(dir);
            cfg.ingest.keep_originals = keep;
            let sources = HashMap::from([("ingest.originals_dir", source)]);
            compute(&Effective {
                config: Arc::new(cfg),
                sources,
                table: HashMap::new(),
            })
            .originals
        };
        assert_eq!(
            state("kept", KeepOriginals::Reencoded, Source::Env),
            Originals::Kept(10)
        );
        assert_eq!(
            state("gone", KeepOriginals::All, Source::Table),
            Originals::Unreachable
        );
        assert_eq!(
            state("gone", KeepOriginals::Reencoded, Source::Default),
            Originals::NoneYet
        );
        assert_eq!(
            state("gone", KeepOriginals::None, Source::Env),
            Originals::Off
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
