//! The last lines of the log for the Advanced page, and the live log level.
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

const KEEP: usize = 2000;
static RING: Mutex<VecDeque<(i64, String)>> = Mutex::new(VecDeque::new());
type Setter = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;
static SETTER: OnceLock<Setter> = OnceLock::new();

/// The binary installs how the level is changed at run time; without it the level is fixed.
pub fn install_level_setter(f: impl Fn(&str) -> Result<(), String> + Send + Sync + 'static) {
    let _ = SETTER.set(Box::new(f));
}

pub fn set_level(level: &str) -> Result<(), String> {
    match SETTER.get() {
        Some(f) => f(level),
        None => Err("the log level is fixed by RUST_LOG for this process".into()),
    }
}

/// The newest `n` lines, oldest first.
pub fn last(n: usize) -> Vec<String> {
    let ring = RING.lock().unwrap_or_else(|e| e.into_inner());
    ring.iter()
        .rev()
        .take(n)
        .map(|(_, l)| l.clone())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Every kept line from the last `secs` seconds, oldest first.
pub fn since(secs: i64) -> Vec<String> {
    let cutoff = jiff::Timestamp::now().as_second() - secs;
    let ring = RING.lock().unwrap_or_else(|e| e.into_inner());
    ring.iter()
        .filter(|(at, _)| *at >= cutoff)
        .map(|(_, l)| l.clone())
        .collect()
}

pub struct RingLayer;

#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.rest
                .push_str(&format!(" {}={:?}", field.name(), value));
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.rest.push_str(&format!(" {}={value:?}", field.name()));
        }
    }
}

impl<S: tracing::Subscriber> Layer<S> for RingLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut f = Fields::default();
        event.record(&mut f);
        let now = jiff::Timestamp::now();
        let line = format!(
            "{} {:>5} {}{}",
            now.strftime("%Y-%m-%dT%H:%M:%SZ"),
            event.metadata().level(),
            f.message,
            f.rest
        );
        let mut ring = RING.lock().unwrap_or_else(|e| e.into_inner());
        if ring.len() >= KEEP {
            ring.pop_front();
        }
        ring.push_back((now.as_second(), line));
    }
}
