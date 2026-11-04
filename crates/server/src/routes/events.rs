use crate::AppState;
use axum::extract::State;
use axum::http::header::HeaderName;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use futures::StreamExt;
use reel_core::events::Event as BusEvent;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;

/// Sent in place of the events a slow stream fell behind on; the page then asks again.
const MISSED: &str = r#"{"type":"missed"}"#;

/// The live updates, which nginx would otherwise hold back until its buffer fills; axum's
/// `Sse` sends `Cache-Control: no-cache` itself.
pub async fn events(State(st): State<AppState>) -> impl IntoResponse {
    let stream = BroadcastStream::new(st.ctx.bus.subscribe()).filter_map(|e| async move {
        line(e).map(|data| Ok::<_, Infallible>(Event::default().data(data)))
    });
    (
        [(HeaderName::from_static("x-accel-buffering"), "no")],
        Sse::new(stream).keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("ping"),
        ),
    )
}

fn line(e: Result<BusEvent, BroadcastStreamRecvError>) -> Option<String> {
    match e {
        Ok(ev) => serde_json::to_string(&ev).ok(),
        Err(BroadcastStreamRecvError::Lagged(n)) => {
            tracing::debug!(skipped = n, "an event stream fell behind");
            Some(MISSED.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_stream_that_fell_behind_says_so_and_goes_on() {
        let bus = reel_core::events::new_bus();
        let mut lines =
            Box::pin(BroadcastStream::new(bus.subscribe()).filter_map(|e| async { line(e) }));
        for n in 0..300 {
            let _ = bus.send(BusEvent::Ingest {
                name: format!("clip {n}"),
                state: "queued".into(),
            });
        }
        assert_eq!(lines.next().await.as_deref(), Some(MISSED));
        let next = lines.next().await.unwrap();
        assert!(
            next.contains(r#""type":"ingest""#) && next.contains("clip 44"),
            "the events kept after the gap follow it: {next}"
        );
    }
}
