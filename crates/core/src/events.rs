use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Job {
        id: String,
        kind: String,
        clip_id: Option<String>,
        rendition_id: Option<String>,
        state: String,
        progress: f64,
        error: Option<String>,
    },
    Clip {
        id: String,
        state: String,
        title: String,
    },
    Ingest {
        name: String,
        state: String,
    },
    Update {
        version: String,
        url: String,
    },
}

pub type Bus = tokio::sync::broadcast::Sender<Event>;

pub fn new_bus() -> Bus {
    tokio::sync::broadcast::channel(256).0
}
