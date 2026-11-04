//! A daily file and a ring of the last lines. Nothing in the app logs a token or a header.
use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, reload, EnvFilter, Registry};

pub const RING_LINES: usize = 2000;

#[derive(Clone, Default)]
pub struct Ring {
    lines: Arc<Mutex<VecDeque<String>>>,
    partial: Arc<Mutex<String>>,
}

impl Ring {
    pub fn lines(&self, n: usize) -> Vec<String> {
        let l = self.lines.lock().unwrap();
        l.iter().rev().take(n).rev().cloned().collect()
    }

    pub fn all(&self) -> Vec<String> {
        self.lines.lock().unwrap().iter().cloned().collect()
    }

    fn push(&self, line: &str) {
        let mut l = self.lines.lock().unwrap();
        if l.len() >= RING_LINES {
            l.pop_front();
        }
        l.push_back(line.to_string());
    }
}

pub struct RingWriter(Ring);

impl Write for RingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let text = String::from_utf8_lossy(buf);
        let mut partial = self.0.partial.lock().unwrap();
        partial.push_str(&text);
        while let Some(i) = partial.find('\n') {
            let line = partial[..i].trim_end().to_string();
            if !line.is_empty() {
                self.0.push(&line);
            }
            partial.drain(..=i);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> fmt::MakeWriter<'a> for Ring {
    type Writer = RingWriter;
    fn make_writer(&'a self) -> RingWriter {
        RingWriter(self.clone())
    }
}

type Filter = reload::Handle<EnvFilter, Registry>;

/// The ring of the log that is up, for a panic's line.
static RING: std::sync::OnceLock<Ring> = std::sync::OnceLock::new();

/// Writes a panic straight into the day's file: the file's own writer runs on a thread that
/// the end of the process does not wait for.
pub fn panicked(logs_dir: &Path, info: &std::panic::PanicHookInfo<'_>) {
    let what = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_default();
    let thread = std::thread::current();
    let now = jiff::Timestamp::now();
    let line = format!(
        "{} ERROR panic thread={} at={} what={what:?}",
        now.strftime("%Y-%m-%dT%H:%M:%S%.6fZ"),
        thread.name().unwrap_or("unnamed"),
        info.location().map(|l| l.to_string()).unwrap_or_default(),
    );
    let trace = std::backtrace::Backtrace::force_capture().to_string();
    let file = logs_dir.join(format!("reel-desktop.log.{}", now.strftime("%Y-%m-%d")));
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
    {
        let _ = f.write_all(format!("{line}\n{trace}\n").as_bytes());
        let _ = f.flush();
    }
    if let Some(r) = RING.get() {
        r.push(&line);
    }
}

pub struct LogHandle {
    pub ring: Ring,
    filter: Filter,
    _guard: tracing_appender::non_blocking::WorkerGuard,
}

impl LogHandle {
    pub fn set_level(&self, level: &str) -> Result<(), String> {
        let f = filter_for(level)?;
        self.filter.reload(f).map_err(|e| e.to_string())
    }
}

const QUIET: &str =
    "hyper=warn,reqwest=warn,tungstenite=warn,tokio_tungstenite=warn,notify=warn,tao=warn,wry=warn";

fn filter_for(level: &str) -> Result<EnvFilter, String> {
    let level = match level.trim().to_ascii_lowercase().as_str() {
        "error" | "warn" | "info" | "debug" | "trace" => level.trim().to_ascii_lowercase(),
        other => return Err(format!("{other} is not a log level")),
    };
    EnvFilter::try_new(format!("{level},{QUIET}")).map_err(|e| e.to_string())
}

/// Installs the global subscriber, once per process.
pub fn init(logs_dir: &Path, level: &str) -> Result<LogHandle, String> {
    std::fs::create_dir_all(logs_dir).map_err(|e| e.to_string())?;
    let file = tracing_appender::rolling::daily(logs_dir, "reel-desktop.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file);
    let ring = Ring::default();
    let (filter, handle) = reload::Layer::new(filter_for(level)?);
    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_writer(file_writer),
        )
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_writer(ring.clone()),
        )
        .try_init()
        .map_err(|e| e.to_string())?;
    let _ = RING.set(ring.clone());
    Ok(LogHandle {
        ring,
        filter: handle,
        _guard: guard,
    })
}

pub fn ring_subscriber(level: &str) -> (Ring, impl tracing::Subscriber + Send + Sync) {
    let ring = Ring::default();
    let sub = tracing_subscriber::registry()
        .with(filter_for(level).expect("a level"))
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_writer(ring.clone()),
        );
    (ring, sub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_is_in_the_days_file_with_where_and_what() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().to_path_buf();
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| panicked(&logs, info)));
        let caught = std::thread::Builder::new()
            .name("the-panicking-one".into())
            .spawn(|| panic!("the window could not be made"))
            .unwrap()
            .join();
        std::panic::set_hook(before);
        assert!(caught.is_err());
        let day = jiff::Timestamp::now().strftime("%Y-%m-%d").to_string();
        let text = std::fs::read_to_string(dir.path().join(format!("reel-desktop.log.{day}")))
            .unwrap_or_default();
        let line = text
            .lines()
            .find(|l| l.contains("the-panicking-one"))
            .unwrap_or_default();
        assert!(
            line.contains(" ERROR panic thread=the-panicking-one at="),
            "{text}"
        );
        assert!(line.contains("log.rs:"), "{line}");
        assert!(
            line.ends_with(r#"what="the window could not be made""#),
            "{line}"
        );
    }

    #[test]
    fn the_ring_keeps_whole_lines_and_the_last_ones() {
        let (ring, sub) = ring_subscriber("info");
        tracing::subscriber::with_default(sub, || {
            for i in 0..(RING_LINES + 5) {
                tracing::info!("line {i}");
            }
            tracing::debug!("not at info");
        });
        let all = ring.all();
        assert_eq!(all.len(), RING_LINES);
        assert!(all[0].contains("line 5"));
        assert!(all
            .last()
            .unwrap()
            .contains(&format!("line {}", RING_LINES + 4)));
        assert!(!all.iter().any(|l| l.contains("not at info")));
        assert_eq!(ring.lines(2).len(), 2);
        assert!(filter_for("loud").is_err());
    }
}
