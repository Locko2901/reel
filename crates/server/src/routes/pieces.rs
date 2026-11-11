//! Resumable uploads in pieces, the app's and the browser's alike.
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::body::Body;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use reel_pipeline::hash::sha256_file;
use serde_json::json;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

/// Whose upload: a device's, or one from the browser.
#[derive(Clone, Copy)]
pub enum Kind {
    Device,
    Web,
}

/// An upload as its pieces see it: where they go, how big it is, how much arrived.
pub struct Pieces {
    pub kind: Kind,
    pub id: String,
    pub part: PathBuf,
    pub size: i64,
    pub received: i64,
}

impl Pieces {
    async fn save(&self, st: &AppState, received: i64) -> AppResult<()> {
        match self.kind {
            Kind::Device => st.ctx.store.upload_received(&self.id, received).await?,
            Kind::Web => st.ctx.store.web_upload_received(&self.id, received).await?,
        }
        Ok(())
    }

    /// What is really there: the part file's length, never past what was acknowledged.
    async fn end(&self, st: &AppState) -> AppResult<i64> {
        let have = tokio::fs::metadata(&self.part)
            .await
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        let end = have.min(self.received);
        if end != self.received {
            self.save(st, end).await?;
        }
        Ok(end)
    }
}

/// One piece at `offset`; a wrong offset, or a piece of the same upload still being written,
/// is answered with where the upload is.
pub async fn put(
    st: &AppState,
    p: &Pieces,
    offset: i64,
    headers: &HeaderMap,
    body: Body,
) -> AppResult<Response> {
    let max = st.ctx.config().device_max_chunk_bytes();
    if let Some(len) = headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
    {
        if len > max {
            return Err(AppError::TooLarge(format!(
                "a chunk is at most {max} bytes"
            )));
        }
    }
    if !st.uploads_busy.lock().await.insert(p.id.clone()) {
        return Ok((
            StatusCode::CONFLICT,
            Json(json!({ "error": "a chunk of this upload is still being written", "offset": p.received })),
        )
            .into_response());
    }
    let result = append(st, p, offset, max, body).await;
    st.uploads_busy.lock().await.remove(&p.id);
    result
}

async fn append(
    st: &AppState,
    p: &Pieces,
    offset: i64,
    max: u64,
    body: Body,
) -> AppResult<Response> {
    let end = p.end(st).await?;
    if offset != end {
        return Ok((
            StatusCode::CONFLICT,
            Json(
                json!({ "error": format!("the upload is at {end}, not {offset}"), "offset": end }),
            ),
        )
            .into_response());
    }
    let mut f = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&p.part)
        .await?;
    f.set_len(end as u64).await?;
    use tokio::io::AsyncSeekExt;
    f.seek(std::io::SeekFrom::Start(end as u64)).await?;
    let mut written: u64 = 0;
    let mut stream = body.into_data_stream();
    let mut failed: Option<AppError> = None;
    while let Some(next) = stream.next().await {
        let chunk = match next {
            Ok(c) => c,
            Err(e) => {
                failed = Some(AppError::BadRequest(format!("the body stopped: {e}")));
                break;
            }
        };
        if written + chunk.len() as u64 > max {
            failed = Some(AppError::TooLarge(format!(
                "a chunk is at most {max} bytes"
            )));
            break;
        }
        if end as u64 + written + chunk.len() as u64 > p.size as u64 {
            failed = Some(AppError::BadRequest(format!(
                "that goes past the announced size of {} bytes",
                p.size
            )));
            break;
        }
        f.write_all(&chunk).await?;
        written += chunk.len() as u64;
    }
    f.flush().await?;
    f.sync_data().await?;
    drop(f);
    let now_at = end + written as i64;
    p.save(st, now_at).await?;
    if let Some(e) = failed {
        return Err(e);
    }
    Ok(Json(json!({ "offset": now_at })).into_response())
}

/// The hashes of whole uploads, each made once in a task of its own: a finish whose request a
/// proxy gave up on, as one may for the minutes ten gigabytes take, leaves it running, and the
/// finish asked again takes its answer.
#[derive(Default)]
pub struct Hashing(tokio::sync::Mutex<std::collections::HashMap<String, Hash>>);

type Hash = tokio::sync::watch::Receiver<Option<Result<String, String>>>;

async fn hash_of(st: &AppState, p: &Pieces) -> AppResult<String> {
    let mut rx = {
        let mut running = st.hashing.0.lock().await;
        running
            .entry(p.id.clone())
            .or_insert_with(|| {
                let (tx, rx) = tokio::sync::watch::channel(None);
                let part = p.part.clone();
                tokio::spawn(async move {
                    let _ = tx.send(Some(sha256_file(&part).await.map_err(|e| e.to_string())));
                });
                rx
            })
            .clone()
    };
    let done = rx
        .wait_for(Option::is_some)
        .await
        .map(|v| v.clone().unwrap_or_else(|| Err("no hash".into())))
        .unwrap_or_else(|_| Err("the hash stopped".into()));
    st.hashing.0.lock().await.remove(&p.id);
    done.map_err(|e| {
        crate::error::AppError::Internal(anyhow::anyhow!(
            "the upload's file could not be read: {e}"
        ))
    })
}

/// Whether the whole file arrived as `sha256` says: Ok, or the answer that says from where to
/// go on (a SHA-256 that does not match starts the upload over).
pub async fn complete(st: &AppState, p: &Pieces, sha256: &str) -> AppResult<Result<(), Response>> {
    let have = tokio::fs::metadata(&p.part)
        .await
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    if have != p.size || p.received != p.size {
        let end = have.min(p.received);
        p.save(st, end).await?;
        return Ok(Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": format!("{end} of {} bytes are here", p.size), "offset": end })),
        )
            .into_response()));
    }
    let sha = hash_of(st, p).await?;
    if sha != sha256 {
        tokio::fs::File::create(&p.part).await?;
        p.save(st, 0).await?;
        tracing::warn!(upload = %p.id, "checksum mismatch; the upload starts over");
        return Ok(Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "the SHA-256 of what arrived is not the one announced; the upload starts over", "offset": 0 })),
        )
            .into_response()));
    }
    Ok(Ok(()))
}
