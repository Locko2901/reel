//! A zip written as it is sent, files stored as they are, its exact length known up front.
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub enum Source {
    Bytes(Vec<u8>),
    File(PathBuf),
}

pub struct Entry {
    /// The path in the archive, with `/` between folders.
    pub name: String,
    pub size: u64,
    pub source: Source,
}

impl Entry {
    pub fn bytes(name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Entry {
            name: name.into(),
            size: bytes.len() as u64,
            source: Source::Bytes(bytes),
        }
    }
}

/// Where ZIP64 takes over; lower in the tests, to write its records with small files.
#[derive(Clone, Copy)]
pub struct Limits {
    pub size: u64,
    pub offset: u64,
    pub count: u64,
}

pub const ZIP32: Limits = Limits {
    size: 0xFFFF_FFFF,
    offset: 0xFFFF_FFFF,
    count: 0xFFFF,
};

const LOCAL: u32 = 0x0403_4b50;
const DESCRIPTOR: u32 = 0x0807_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END64: u32 = 0x0606_4b50;
const LOCATOR64: u32 = 0x0706_4b50;
const END: u32 = 0x0605_4b50;
/// A data descriptor follows the data; the names are UTF-8.
const FLAGS: u16 = 0x0808;
/// Made on Unix (so the files unpack as rw-r--r--), by the version that knows ZIP64.
const MADE_BY: u16 = 0x0300 | 45;
const FILE_MODE: u32 = 0o100644 << 16;

pub struct Zip {
    entries: Vec<Entry>,
    limits: Limits,
    /// The entries' time, in MS-DOS's form (time, date).
    dos: (u16, u16),
}

struct Written {
    crc: u32,
    size: u64,
    offset: u64,
}

impl Zip {
    pub fn new(entries: Vec<Entry>, when: jiff::civil::DateTime) -> Self {
        Self::with_limits(entries, when, ZIP32)
    }

    pub fn with_limits(entries: Vec<Entry>, when: jiff::civil::DateTime, limits: Limits) -> Self {
        let year = (when.year().clamp(1980, 2107) - 1980) as u16;
        let date = (year << 9) | ((when.month() as u16) << 5) | when.day() as u16;
        let time = ((when.hour() as u16) << 11)
            | ((when.minute() as u16) << 5)
            | (when.second() as u16 / 2);
        Zip {
            entries,
            limits,
            dos: (time, date),
        }
    }

    fn big(&self, size: u64) -> bool {
        size >= self.limits.size
    }

    fn central_extra(&self, size: u64, offset: u64) -> usize {
        let n = 2 * usize::from(self.big(size)) + usize::from(offset >= self.limits.offset);
        if n == 0 {
            0
        } else {
            4 + 8 * n
        }
    }

    pub fn length(&self) -> u64 {
        let mut offset = 0u64;
        let mut central = 0u64;
        for e in &self.entries {
            let name = e.name.len() as u64;
            let big = self.big(e.size);
            central += 46 + name + self.central_extra(e.size, offset) as u64;
            offset += 30 + name + if big { 20 } else { 0 } + e.size + if big { 24 } else { 16 };
        }
        offset + central + self.end_length(offset, central)
    }

    fn needs_end64(&self, start: u64, size: u64) -> bool {
        self.entries.len() as u64 >= self.limits.count
            || start >= self.limits.offset
            || size >= self.limits.offset
    }

    fn end_length(&self, start: u64, size: u64) -> u64 {
        22 + if self.needs_end64(start, size) {
            56 + 20
        } else {
            0
        }
    }

    /// Sends the archive, entry by entry, its files read as they are sent. The count of bytes
    /// written; a file whose size changed since the plan stops it.
    pub async fn write<W: AsyncWrite + Unpin>(self, out: &mut W) -> std::io::Result<u64> {
        let mut at = 0u64;
        let mut done = Vec::with_capacity(self.entries.len());
        let mut buf = vec![0u8; 256 * 1024];
        for e in &self.entries {
            let big = self.big(e.size);
            let mut h = Vec::with_capacity(64 + e.name.len());
            h.extend(LOCAL.to_le_bytes());
            h.extend((if big { 45u16 } else { 20u16 }).to_le_bytes());
            h.extend(FLAGS.to_le_bytes());
            h.extend(0u16.to_le_bytes());
            h.extend(self.dos.0.to_le_bytes());
            h.extend(self.dos.1.to_le_bytes());
            h.extend(0u32.to_le_bytes());
            let sizes = if big { u32::MAX } else { 0 };
            h.extend(sizes.to_le_bytes());
            h.extend(sizes.to_le_bytes());
            h.extend((e.name.len() as u16).to_le_bytes());
            h.extend((if big { 20u16 } else { 0 }).to_le_bytes());
            h.extend(e.name.as_bytes());
            if big {
                h.extend(1u16.to_le_bytes());
                h.extend(16u16.to_le_bytes());
                h.extend(0u64.to_le_bytes());
                h.extend(0u64.to_le_bytes());
            }
            out.write_all(&h).await?;
            let offset = at;
            at += h.len() as u64;
            let mut crc = crc32fast::Hasher::new();
            let mut sent = 0u64;
            match &e.source {
                Source::Bytes(b) => {
                    crc.update(b);
                    out.write_all(b).await?;
                    sent = b.len() as u64;
                }
                Source::File(path) => {
                    // a changed size would make a "finished" download with no central directory,
                    // so it fails; one that grows while it is sent is read to its planned size
                    let f = tokio::fs::File::open(path).await?;
                    if f.metadata().await?.len() != e.size {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("{} changed while it was sent", e.name),
                        ));
                    }
                    let mut f = f.take(e.size);
                    loop {
                        let n = f.read(&mut buf).await?;
                        if n == 0 {
                            break;
                        }
                        crc.update(&buf[..n]);
                        out.write_all(&buf[..n]).await?;
                        sent += n as u64;
                    }
                }
            }
            if sent != e.size {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{} changed while it was sent", e.name),
                ));
            }
            at += sent;
            let crc = crc.finalize();
            let mut d = Vec::with_capacity(24);
            d.extend(DESCRIPTOR.to_le_bytes());
            d.extend(crc.to_le_bytes());
            if big {
                d.extend(sent.to_le_bytes());
                d.extend(sent.to_le_bytes());
            } else {
                d.extend((sent as u32).to_le_bytes());
                d.extend((sent as u32).to_le_bytes());
            }
            out.write_all(&d).await?;
            at += d.len() as u64;
            done.push(Written {
                crc,
                size: sent,
                offset,
            });
        }
        let start = at;
        for (e, w) in self.entries.iter().zip(&done) {
            let big = self.big(w.size);
            let far = w.offset >= self.limits.offset;
            let extra = self.central_extra(w.size, w.offset);
            let mut c = Vec::with_capacity(46 + e.name.len() + extra);
            c.extend(CENTRAL.to_le_bytes());
            c.extend(MADE_BY.to_le_bytes());
            c.extend((if extra > 0 { 45u16 } else { 20u16 }).to_le_bytes());
            c.extend(FLAGS.to_le_bytes());
            c.extend(0u16.to_le_bytes());
            c.extend(self.dos.0.to_le_bytes());
            c.extend(self.dos.1.to_le_bytes());
            c.extend(w.crc.to_le_bytes());
            let size32 = if big { u32::MAX } else { w.size as u32 };
            c.extend(size32.to_le_bytes());
            c.extend(size32.to_le_bytes());
            c.extend((e.name.len() as u16).to_le_bytes());
            c.extend((extra as u16).to_le_bytes());
            c.extend(0u16.to_le_bytes());
            c.extend(0u16.to_le_bytes());
            c.extend(0u16.to_le_bytes());
            c.extend(FILE_MODE.to_le_bytes());
            c.extend((if far { u32::MAX } else { w.offset as u32 }).to_le_bytes());
            c.extend(e.name.as_bytes());
            if extra > 0 {
                c.extend(1u16.to_le_bytes());
                c.extend(((extra - 4) as u16).to_le_bytes());
                if big {
                    c.extend(w.size.to_le_bytes());
                    c.extend(w.size.to_le_bytes());
                }
                if far {
                    c.extend(w.offset.to_le_bytes());
                }
            }
            out.write_all(&c).await?;
            at += c.len() as u64;
        }
        let size = at - start;
        let count = self.entries.len() as u64;
        let mut t = Vec::with_capacity(98);
        if self.needs_end64(start, size) {
            let end64 = at;
            t.extend(END64.to_le_bytes());
            t.extend(44u64.to_le_bytes());
            t.extend(MADE_BY.to_le_bytes());
            t.extend(45u16.to_le_bytes());
            t.extend(0u32.to_le_bytes());
            t.extend(0u32.to_le_bytes());
            t.extend(count.to_le_bytes());
            t.extend(count.to_le_bytes());
            t.extend(size.to_le_bytes());
            t.extend(start.to_le_bytes());
            t.extend(LOCATOR64.to_le_bytes());
            t.extend(0u32.to_le_bytes());
            t.extend(end64.to_le_bytes());
            t.extend(1u32.to_le_bytes());
        }
        let count16 = if count >= self.limits.count {
            u16::MAX
        } else {
            count as u16
        };
        let clamp = |v: u64| {
            if v >= self.limits.offset {
                u32::MAX
            } else {
                v as u32
            }
        };
        t.extend(END.to_le_bytes());
        t.extend(0u16.to_le_bytes());
        t.extend(0u16.to_le_bytes());
        t.extend(count16.to_le_bytes());
        t.extend(count16.to_le_bytes());
        t.extend(clamp(size).to_le_bytes());
        t.extend(clamp(start).to_le_bytes());
        t.extend(0u16.to_le_bytes());
        out.write_all(&t).await?;
        at += t.len() as u64;
        out.flush().await?;
        Ok(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn when() -> jiff::civil::DateTime {
        jiff::civil::date(2026, 11, 2).at(10, 41, 30, 0)
    }

    async fn made(entries: Vec<Entry>, limits: Limits) -> (Vec<u8>, u64) {
        let zip = Zip::with_limits(entries, when(), limits);
        let length = zip.length();
        let mut out = Vec::new();
        let sent = zip.write(&mut out).await.unwrap();
        assert_eq!(sent, out.len() as u64);
        (out, length)
    }

    type Files = Vec<(String, Vec<u8>)>;

    fn read_back(bytes: &[u8]) -> Files {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        (0..z.len())
            .map(|i| {
                let mut f = z.by_index(i).unwrap();
                let mut b = Vec::new();
                f.read_to_end(&mut b).unwrap();
                (f.name().unwrap().to_string(), b)
            })
            .collect()
    }

    fn some() -> (Vec<Entry>, Files, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let video: Vec<u8> = (0..300_000u32).map(|n| (n % 251) as u8).collect();
        let path = dir.path().join("master.mp4");
        std::fs::write(&path, &video).unwrap();
        let entries = vec![
            Entry::bytes("README.txt", b"what is in it".to_vec()),
            Entry {
                name: "Überhang (2)/original.mp4".into(),
                size: video.len() as u64,
                source: Source::File(path),
            },
            Entry::bytes("Überhang (2)/clip.json", b"{}".to_vec()),
        ];
        let want = vec![
            ("README.txt".into(), b"what is in it".to_vec()),
            ("Überhang (2)/original.mp4".into(), video),
            ("Überhang (2)/clip.json".into(), b"{}".to_vec()),
        ];
        (entries, want, dir)
    }

    #[tokio::test]
    async fn a_file_that_changed_since_the_plan_ends_the_zip() {
        let (entries, _, dir) = some();
        std::fs::write(dir.path().join("master.mp4"), vec![7u8; 300_001]).unwrap();
        let zip = Zip::with_limits(entries, when(), ZIP32);
        let mut out = Vec::new();
        let err = zip.write(&mut out).await.unwrap_err();
        assert!(
            err.to_string().contains("changed while it was sent"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn a_zip_reads_back_as_it_was_planned() {
        let (entries, want, _dir) = some();
        let (bytes, length) = made(entries, ZIP32).await;
        assert_eq!(bytes.len() as u64, length, "the length said before");
        assert_eq!(read_back(&bytes), want);
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        let f = z.by_index(1).unwrap();
        assert_eq!(f.compression(), zip::CompressionMethod::Stored);
        assert_eq!(f.unix_mode(), Some(0o100644));
        let t = f.last_modified().unwrap();
        assert_eq!(
            (t.year(), t.month(), t.day(), t.hour(), t.minute()),
            (2026, 11, 2, 10, 41)
        );
    }

    #[tokio::test]
    async fn sizes_offsets_and_a_count_past_their_limits_are_written_as_zip64() {
        let (entries, want, _dir) = some();
        let low = Limits {
            size: 100_000,
            offset: 1000,
            count: 2,
        };
        let (bytes, length) = made(entries, low).await;
        assert_eq!(bytes.len() as u64, length);
        assert!(
            bytes.windows(4).any(|w| w == END64.to_le_bytes()),
            "a ZIP64 end"
        );
        assert_eq!(read_back(&bytes), want);
        let mut many = Vec::new();
        for n in 0..70_000 {
            many.push(Entry::bytes(format!("f{n}"), vec![n as u8]));
        }
        let (bytes, length) = made(many, ZIP32).await;
        assert_eq!(bytes.len() as u64, length);
        let z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(z.len(), 70_000, "more entries than a 16-bit count");
    }

    #[tokio::test]
    async fn a_file_that_changed_since_the_plan_stops_the_zip() {
        let (mut entries, _, _dir) = some();
        entries[1].size += 1;
        let zip = Zip::new(entries, when());
        let mut out = Vec::new();
        let err = zip.write(&mut out).await.unwrap_err();
        assert!(err.to_string().contains("changed while it was sent"));
    }
}
