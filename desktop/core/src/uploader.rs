//! One file to the server, resumable, paced, and retried until cancelled.
use crate::api::{Api, ApiError, ClipStatus, Created, NewUpload, ShareAnswer};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

pub const MAX_CHUNK_BYTES: u64 = 90 * 1024 * 1024;
pub const PIECE: usize = 256 * 1024;

#[derive(Debug, Clone, Default)]
pub struct UploadMeta {
    pub file_name: Option<String>,
    pub recorded_at: Option<String>,
    pub game: Option<String>,
    pub title: Option<String>,
    pub keep_last: Option<u64>,
    pub audio_names: Option<Vec<String>>,
    pub kind: Option<String>,
}

#[derive(Clone)]
pub struct Control {
    pub cancel: CancellationToken,
    pub paused: watch::Receiver<bool>,
    /// 0 is unlimited.
    pub bandwidth_kbit: u64,
    /// None takes the server's chunk size.
    pub chunk_bytes: Option<u64>,
    pub retry_base: Duration,
    pub retry_max: Duration,
    pub poll_every: Duration,
}

impl Control {
    pub fn new(paused: watch::Receiver<bool>) -> Control {
        Control {
            cancel: CancellationToken::new(),
            paused,
            bandwidth_kbit: 0,
            chunk_bytes: None,
            retry_base: Duration::from_secs(1),
            retry_max: Duration::from_secs(60),
            poll_every: Duration::from_secs(2),
        }
    }

    pub fn never_paused() -> (Control, watch::Sender<bool>) {
        let (tx, rx) = watch::channel(false);
        (Control::new(rx), tx)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    Hashing { done: u64, total: u64 },
    Uploading { sent: u64, total: u64 },
    Paused { sent: u64, total: u64 },
    Retrying { reason: String, in_secs: u64 },
    Done { clip_id: String, deduped: bool },
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UploadOutcome {
    pub clip_id: String,
    pub deduped: bool,
    pub sha256: String,
    pub upload_id: Option<String>,
    pub title: String,
}

#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("cancelled")]
    Cancelled,
    #[error("the server does not know this device; pair it again")]
    Unauthorized,
    #[error("{0}")]
    Failed(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<ApiError> for UploadError {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Unauthorized => UploadError::Unauthorized,
            other => UploadError::Failed(other.to_string()),
        }
    }
}

/// Grows from the base to the max, doubling, with up to half the delay added at random
/// so devices that lost the server together do not come back together.
pub struct Backoff {
    attempt: u32,
    base: Duration,
    max: Duration,
    seed: u64,
}

impl Backoff {
    pub fn new(base: Duration, max: Duration) -> Backoff {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9)
            | 1;
        Backoff {
            attempt: 0,
            base,
            max,
            seed,
        }
    }

    fn random(&mut self) -> u64 {
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;
        x
    }

    pub fn next_delay(&mut self) -> Duration {
        let pow = self.base.saturating_mul(1u32 << self.attempt.min(16));
        let flat = pow.min(self.max);
        self.attempt = self.attempt.saturating_add(1);
        let jitter_ns = (flat.as_nanos() as u64 / 2).max(1);
        flat + Duration::from_nanos(self.random() % jitter_ns)
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

/// The ideal moment, after the start, by which `bytes` may have gone at `kbit` per second.
pub fn ideal_offset(bytes: u64, kbit: u64) -> Duration {
    if kbit == 0 {
        return Duration::ZERO;
    }
    Duration::from_secs_f64(bytes as f64 * 8.0 / (kbit as f64 * 1000.0))
}

struct PacerInner {
    started: Instant,
    scheduled: u64,
}

/// A token bucket over the whole upload: each piece waits until its ideal moment; a
/// gap longer than a second re-anchors the clock so a pause is not followed by a burst.
#[derive(Clone)]
pub struct Pacer {
    kbit: u64,
    inner: Arc<Mutex<PacerInner>>,
}

impl Pacer {
    pub fn new(kbit: u64) -> Pacer {
        Pacer {
            kbit,
            inner: Arc::new(Mutex::new(PacerInner {
                started: Instant::now(),
                scheduled: 0,
            })),
        }
    }

    /// How long to wait before sending `bytes`, and books them.
    pub fn reserve(&self, bytes: u64) -> Duration {
        if self.kbit == 0 {
            return Duration::ZERO;
        }
        let mut p = self.inner.lock().unwrap();
        let now = Instant::now();
        let due = p.started + ideal_offset(p.scheduled, self.kbit);
        if now > due + Duration::from_secs(1) {
            p.started = now - ideal_offset(p.scheduled, self.kbit);
        }
        let due = p.started + ideal_offset(p.scheduled, self.kbit);
        p.scheduled += bytes;
        due.saturating_duration_since(now)
    }
}

fn paced_body(data: Vec<u8>, pacer: Pacer) -> reqwest::Body {
    let len = data.len();
    let data = Arc::new(data);
    let stream = futures_util::stream::unfold(0usize, move |pos| {
        let data = data.clone();
        let pacer = pacer.clone();
        async move {
            if pos >= len {
                return None;
            }
            let end = (pos + PIECE).min(len);
            let wait = pacer.reserve((end - pos) as u64);
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            Some((Ok::<_, std::io::Error>(data[pos..end].to_vec()), end))
        }
    });
    reqwest::Body::wrap_stream(stream)
}

async fn wait_while_paused(
    ctl: &Control,
    tx: &mpsc::UnboundedSender<Progress>,
    sent: u64,
    total: u64,
) -> Result<(), UploadError> {
    let mut paused = ctl.paused.clone();
    if !*paused.borrow() {
        return Ok(());
    }
    let _ = tx.send(Progress::Paused { sent, total });
    tracing::info!("upload paused: a game has the screen, the uploads are paused, or one someone waits for goes first");
    loop {
        tokio::select! {
            _ = ctl.cancel.cancelled() => return Err(UploadError::Cancelled),
            r = paused.changed() => {
                if r.is_err() || !*paused.borrow() {
                    tracing::info!("upload goes on");
                    return Ok(());
                }
            }
        }
    }
}

async fn pause_for(
    ctl: &Control,
    tx: &mpsc::UnboundedSender<Progress>,
    backoff: &mut Backoff,
    why: &str,
) -> Result<(), UploadError> {
    let d = backoff.next_delay();
    tracing::warn!(
        reason = why,
        wait_secs = d.as_secs_f64(),
        "upload will retry"
    );
    let _ = tx.send(Progress::Retrying {
        reason: why.to_string(),
        in_secs: d.as_secs(),
    });
    tokio::select! {
        _ = ctl.cancel.cancelled() => Err(UploadError::Cancelled),
        _ = tokio::time::sleep(d) => Ok(()),
    }
}

async fn read_at(path: &Path, offset: u64, len: u64) -> std::io::Result<Vec<u8>> {
    let mut f = tokio::fs::File::open(path).await?;
    f.seek(std::io::SeekFrom::Start(offset)).await?;
    let mut buf = vec![0u8; len as usize];
    let mut got = 0usize;
    while got < buf.len() {
        let n = f.read(&mut buf[got..]).await?;
        if n == 0 {
            break;
        }
        got += n;
    }
    buf.truncate(got);
    Ok(buf)
}

enum Step {
    Again,
    Restart,
}

pub async fn upload_file(
    api: &Api,
    path: &Path,
    meta: UploadMeta,
    ctl: &Control,
    tx: mpsc::UnboundedSender<Progress>,
) -> Result<UploadOutcome, UploadError> {
    let total = tokio::fs::metadata(path).await?.len();
    let name = meta.file_name.clone().unwrap_or_else(|| {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "clip.mkv".into())
    });
    // the hash reads the whole file, hours of a session recording: not while a game has the screen
    wait_while_paused(ctl, &tx, 0, total).await?;
    let ptx = tx.clone();
    let sha = tokio::select! {
        _ = ctl.cancel.cancelled() => return Err(UploadError::Cancelled),
        r = crate::hash::sha256_file(path, move |done| { let _ = ptx.send(Progress::Hashing { done, total }); }) => r.map_err(|e| UploadError::Failed(format!("cannot hash {}: {e}", path.display())))?,
    };
    tracing::info!(file = %name, size = total, sha = %&sha[..12], "upload starts");
    let new = NewUpload {
        file_name: name.clone(),
        size: total,
        sha256: sha.clone(),
        recorded_at: meta.recorded_at.clone(),
        game: meta.game.clone(),
        title: meta.title.clone(),
        keep_last: meta.keep_last,
        audio_names: meta.audio_names.clone(),
        kind: meta.kind.clone(),
    };
    let mut backoff = Backoff::new(ctl.retry_base, ctl.retry_max);
    let pacer = Pacer::new(ctl.bandwidth_kbit);
    let mut restarted = false;
    'create: loop {
        if ctl.cancel.is_cancelled() {
            return Err(UploadError::Cancelled);
        }
        let created = match api.create_upload(&new).await {
            Ok(c) => c,
            Err(e) if e.is_transient() => {
                pause_for(ctl, &tx, &mut backoff, &e.to_string()).await?;
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        backoff.reset();
        let (upload_id, mut offset, chunk) = match created {
            Created::Exists { clip_id, clip } => {
                tracing::info!(clip = %clip_id, "the server has this file already");
                let title = clip.map(|c| c.title).unwrap_or_default();
                let _ = tx.send(Progress::Done {
                    clip_id: clip_id.clone(),
                    deduped: true,
                });
                return Ok(UploadOutcome {
                    clip_id,
                    deduped: true,
                    sha256: sha,
                    upload_id: None,
                    title: if title.is_empty() {
                        meta.title.clone().unwrap_or_else(|| name.clone())
                    } else {
                        title
                    },
                });
            }
            Created::Created {
                upload_id,
                offset,
                chunk_bytes,
            }
            | Created::Resumed {
                upload_id,
                offset,
                chunk_bytes,
            } => {
                let chunk = ctl
                    .chunk_bytes
                    .filter(|c| *c > 0)
                    .unwrap_or(chunk_bytes)
                    .clamp(1, MAX_CHUNK_BYTES);
                (upload_id, offset, chunk)
            }
        };
        tracing::info!(upload = %upload_id, offset, chunk, "upload open");
        loop {
            match send_chunks(
                api,
                path,
                &upload_id,
                &mut offset,
                total,
                chunk,
                &pacer,
                ctl,
                &tx,
                &mut backoff,
            )
            .await?
            {
                Step::Restart => continue 'create,
                Step::Again => {}
            }
            match api.finish_upload(&upload_id).await {
                Ok(fin) => {
                    tracing::info!(upload = %upload_id, clip = %fin.clip_id, state = %fin.state, "upload finished; the server has the whole file");
                    let deduped = fin.state == "ready";
                    let _ = tx.send(Progress::Done {
                        clip_id: fin.clip_id.clone(),
                        deduped,
                    });
                    return Ok(UploadOutcome {
                        clip_id: fin.clip_id,
                        deduped,
                        sha256: sha,
                        upload_id: Some(upload_id),
                        title: meta.title.clone().unwrap_or_else(|| name.clone()),
                    });
                }
                Err(ApiError::Conflict {
                    offset: Some(o), ..
                }) => {
                    offset = o;
                }
                Err(ApiError::Conflict {
                    offset: None,
                    message,
                }) => {
                    offset = api
                        .upload_status(&upload_id)
                        .await
                        .map(|s| s.offset)
                        .unwrap_or(0);
                    tracing::warn!(%message, offset, "finish refused; the upload goes on from the server's offset");
                }
                Err(ApiError::ChecksumMismatch) => {
                    if restarted {
                        let reason =
                            "the file did not arrive whole twice; is it still being written?"
                                .to_string();
                        let _ = tx.send(Progress::Failed {
                            reason: reason.clone(),
                        });
                        return Err(UploadError::Failed(reason));
                    }
                    restarted = true;
                    offset = 0;
                    tracing::warn!("checksum mismatch; the upload starts over once");
                }
                Err(ApiError::NotFound) => {
                    tracing::warn!("the upload is gone on the server; starting again");
                    continue 'create;
                }
                Err(e) if e.is_transient() => {
                    pause_for(ctl, &tx, &mut backoff, &e.to_string()).await?;
                }
                Err(e) => {
                    let _ = tx.send(Progress::Failed {
                        reason: e.to_string(),
                    });
                    return Err(e.into());
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn send_chunks(
    api: &Api,
    path: &Path,
    upload_id: &str,
    offset: &mut u64,
    total: u64,
    chunk: u64,
    pacer: &Pacer,
    ctl: &Control,
    tx: &mpsc::UnboundedSender<Progress>,
    backoff: &mut Backoff,
) -> Result<Step, UploadError> {
    while *offset < total {
        if ctl.cancel.is_cancelled() {
            return Err(UploadError::Cancelled);
        }
        wait_while_paused(ctl, tx, *offset, total).await?;
        let len = chunk.min(total - *offset);
        let data = read_at(path, *offset, len).await?;
        if data.len() as u64 != len {
            return Err(UploadError::Failed(format!(
                "{} shrank while uploading",
                path.display()
            )));
        }
        let body = paced_body(data, pacer.clone());
        match api.put_chunk(upload_id, *offset, body, len).await {
            Ok(end) => {
                *offset = end;
                backoff.reset();
                let _ = tx.send(Progress::Uploading {
                    sent: *offset,
                    total,
                });
            }
            Err(ApiError::Conflict {
                offset: Some(o), ..
            }) => {
                tracing::info!(from = *offset, to = o, "resynced to the server's offset");
                *offset = o;
            }
            Err(ApiError::Conflict { offset: None, .. }) => {
                match api.upload_status(upload_id).await {
                    Ok(s) => *offset = s.offset,
                    Err(e) if e.is_transient() => {
                        pause_for(ctl, tx, backoff, &e.to_string()).await?
                    }
                    Err(ApiError::NotFound) => return Ok(Step::Restart),
                    Err(e) => return Err(e.into()),
                }
            }
            Err(ApiError::NotFound) => return Ok(Step::Restart),
            Err(e) if e.is_transient() => {
                pause_for(ctl, tx, backoff, &e.to_string()).await?;
                match api.upload_status(upload_id).await {
                    Ok(s) => *offset = s.offset,
                    Err(ApiError::NotFound) => return Ok(Step::Restart),
                    Err(_) => {}
                }
            }
            Err(e) => {
                let _ = tx.send(Progress::Failed {
                    reason: e.to_string(),
                });
                return Err(e.into());
            }
        }
    }
    Ok(Step::Again)
}

/// What the server does with a clip once it has the file: publishing, then the preview video.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerPhase {
    Publishing { progress: f64 },
    Preview { progress: Option<f64> },
}

/// Polls until the clip's link would embed: published, with the preview video in place.
pub async fn watch_clip(
    api: &Api,
    clip_id: &str,
    ctl: &Control,
    mut on_phase: impl FnMut(ServerPhase),
) -> Result<ClipStatus, UploadError> {
    let mut backoff = Backoff::new(ctl.retry_base, ctl.retry_max);
    loop {
        if ctl.cancel.is_cancelled() {
            return Err(UploadError::Cancelled);
        }
        match api.clip(clip_id).await {
            Ok(c) if c.ready && c.preview_ready => return Ok(c),
            Ok(c) if c.failed() => {
                return Err(UploadError::Failed(
                    c.error
                        .clone()
                        .unwrap_or_else(|| "the server could not read the file".into()),
                ));
            }
            Ok(c) if c.preview_error.is_some() => {
                return Err(UploadError::Failed(format!(
                    "the preview video could not be made: {}",
                    c.preview_error.clone().unwrap_or_default()
                )));
            }
            Ok(c) => {
                backoff.reset();
                on_phase(if c.ready {
                    ServerPhase::Preview {
                        progress: c.preview_progress,
                    }
                } else {
                    ServerPhase::Publishing {
                        progress: c.progress,
                    }
                });
            }
            Err(e) if e.is_transient() => {
                let d = backoff.next_delay();
                tokio::select! {
                    _ = ctl.cancel.cancelled() => return Err(UploadError::Cancelled),
                    _ = tokio::time::sleep(d) => {}
                }
                continue;
            }
            Err(e) => return Err(e.into()),
        }
        tokio::select! {
            _ = ctl.cancel.cancelled() => return Err(UploadError::Cancelled),
            _ = tokio::time::sleep(ctl.poll_every) => {}
        }
    }
}

/// The link of the device's newest clip, or the one named, once the server has prepared it.
pub async fn share_when_ready(
    api: &Api,
    clip_id: Option<&str>,
    ctl: &Control,
    mut on_step: impl FnMut(&str, f64),
) -> Result<ShareAnswer, UploadError> {
    let mut backoff = Backoff::new(ctl.retry_base, ctl.retry_max);
    loop {
        if ctl.cancel.is_cancelled() {
            return Err(UploadError::Cancelled);
        }
        match api.share(clip_id).await {
            Ok(ShareAnswer::Preparing { step, progress, .. }) => {
                backoff.reset();
                on_step(&step, progress);
            }
            Ok(ready) => return Ok(ready),
            Err(e) if e.is_transient() => {
                let d = backoff.next_delay();
                tokio::select! {
                    _ = ctl.cancel.cancelled() => return Err(UploadError::Cancelled),
                    _ = tokio::time::sleep(d) => {}
                }
                continue;
            }
            Err(e) => return Err(e.into()),
        }
        tokio::select! {
            _ = ctl.cancel.cancelled() => return Err(UploadError::Cancelled),
            _ = tokio::time::sleep(ctl.poll_every) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::Mock;

    fn fast(paused: watch::Receiver<bool>) -> Control {
        let mut c = Control::new(paused);
        c.retry_base = Duration::from_millis(10);
        c.retry_max = Duration::from_millis(40);
        c.poll_every = Duration::from_millis(10);
        c
    }

    fn file(dir: &Path, name: &str, len: usize) -> (std::path::PathBuf, Vec<u8>) {
        let data: Vec<u8> = (0..len).map(|i| (i * 7 % 253) as u8).collect();
        let p = dir.join(name);
        std::fs::write(&p, &data).unwrap();
        (p, data)
    }

    fn drain(rx: &mut mpsc::UnboundedReceiver<Progress>) -> Vec<Progress> {
        let mut v = Vec::new();
        while let Ok(p) = rx.try_recv() {
            v.push(p);
        }
        v
    }

    #[test]
    fn the_pacing_math() {
        assert_eq!(ideal_offset(1_000_000, 8_000), Duration::from_secs(1));
        assert_eq!(ideal_offset(250_000, 2_000), Duration::from_secs(1));
        assert_eq!(ideal_offset(5, 0), Duration::ZERO);
        let p = Pacer::new(0);
        assert_eq!(p.reserve(1 << 20), Duration::ZERO);
        let p = Pacer::new(8_000);
        assert!(
            p.reserve(1_000_000) < Duration::from_millis(5),
            "the first piece goes at once"
        );
        let second = p.reserve(1_000_000);
        assert!(
            second > Duration::from_millis(900) && second <= Duration::from_secs(1),
            "{second:?}"
        );
        let mut b = Backoff::new(Duration::from_millis(100), Duration::from_millis(400));
        let d1 = b.next_delay();
        let d2 = b.next_delay();
        let d3 = b.next_delay();
        let d4 = b.next_delay();
        assert!(
            d1 >= Duration::from_millis(100) && d1 < Duration::from_millis(150),
            "{d1:?}"
        );
        assert!(
            d2 >= Duration::from_millis(200) && d2 < Duration::from_millis(300),
            "{d2:?}"
        );
        assert!(
            d3 >= Duration::from_millis(400) && d3 < Duration::from_millis(600),
            "{d3:?}"
        );
        assert!(
            d4 >= Duration::from_millis(400) && d4 < Duration::from_millis(600),
            "capped {d4:?}"
        );
        b.reset();
        assert!(b.next_delay() < Duration::from_millis(150));
    }

    #[tokio::test]
    async fn a_cut_chunk_resumes_at_the_servers_offset_and_the_clip_is_watched_after() {
        let mock = Mock::start().await;
        let api = mock.paired_api().await;
        let dir = tempfile::tempdir().unwrap();
        let (p, data) = file(dir.path(), "a.mkv", 2500);
        mock.state.lock().unwrap().cut_next_chunk = true;
        mock.state.lock().unwrap().ready_after_polls = 2;
        let (ctl, _pause) = Control::never_paused();
        let ctl = fast(ctl.paused);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let out = upload_file(&api, &p, UploadMeta::default(), &ctl, tx)
            .await
            .unwrap();
        assert!(!out.deduped);
        assert_eq!(out.sha256, crate::hash::sha256_hex(&data));
        {
            let s = mock.state.lock().unwrap();
            let u = s.uploads.get(out.upload_id.as_deref().unwrap()).unwrap();
            assert_eq!(u.data, data, "the file arrived whole after the cut");
            assert!(
                s.chunk_puts >= 3,
                "the cut half, then the rest: {}",
                s.chunk_puts
            );
        }
        let seen = drain(&mut rx);
        assert!(seen.iter().any(|p| matches!(p, Progress::Hashing { .. })));
        assert!(
            seen.iter().any(|p| matches!(p, Progress::Retrying { .. })),
            "{seen:?}"
        );
        assert!(
            matches!(seen.last(), Some(Progress::Done { deduped: false, .. })),
            "{seen:?}"
        );
        let mut phases = Vec::new();
        let status = watch_clip(&api, &out.clip_id, &ctl, |ph| phases.push(ph))
            .await
            .unwrap();
        assert!(status.ready && status.preview_ready);
        assert!(
            phases
                .iter()
                .any(|p| matches!(p, ServerPhase::Publishing { .. })),
            "{phases:?}"
        );
        let again = upload_file(
            &api,
            &p,
            UploadMeta::default(),
            &ctl,
            mpsc::unbounded_channel().0,
        )
        .await
        .unwrap();
        assert!(
            again.deduped,
            "the second send of the same file is the same clip"
        );
        assert_eq!(again.clip_id, out.clip_id);
    }

    #[tokio::test]
    async fn a_checksum_mismatch_restarts_once_and_dropped_requests_retry() {
        let mock = Mock::start().await;
        let api = mock.paired_api().await;
        let dir = tempfile::tempdir().unwrap();
        let (p, data) = file(dir.path(), "b.mkv", 3000);
        {
            let mut s = mock.state.lock().unwrap();
            s.drop_chunks = 2;
            s.corrupt_first_finish = true;
        }
        let (ctl, _pause) = Control::never_paused();
        let ctl = fast(ctl.paused);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let out = upload_file(&api, &p, UploadMeta::default(), &ctl, tx)
            .await
            .unwrap();
        {
            let s = mock.state.lock().unwrap();
            let u = s.uploads.get(out.upload_id.as_deref().unwrap()).unwrap();
            assert_eq!(u.data, data);
            assert_eq!(s.drop_chunks, 0);
            assert!(
                !s.corrupt_first_finish,
                "the first finish saw a corrupted file"
            );
            assert!(
                s.chunk_puts >= 6,
                "three chunks twice, plus the dropped ones: {}",
                s.chunk_puts
            );
        }
        let seen = drain(&mut rx);
        assert!(
            seen.iter()
                .filter(|p| matches!(p, Progress::Retrying { .. }))
                .count()
                >= 2,
            "{seen:?}"
        );
        assert!(
            seen.iter()
                .filter(|p| matches!(p, Progress::Uploading { sent: 1024, .. }))
                .count()
                >= 2,
            "sent from zero again: {seen:?}"
        );
    }

    #[tokio::test]
    async fn paused_waits_and_cancel_stops() {
        let mock = Mock::start().await;
        let api = mock.paired_api().await;
        let dir = tempfile::tempdir().unwrap();
        let (p, _) = file(dir.path(), "c.mkv", 2048);
        let (pause_tx, pause_rx) = watch::channel(true);
        let ctl = fast(pause_rx);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let api2 = api.clone();
        let p2 = p.clone();
        let ctl2 = ctl.clone();
        let task =
            tokio::spawn(
                async move { upload_file(&api2, &p2, UploadMeta::default(), &ctl2, tx).await },
            );
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(
            mock.state.lock().unwrap().chunk_puts,
            0,
            "nothing goes while paused"
        );
        let seen = drain(&mut rx);
        assert!(
            seen.iter().any(|p| matches!(p, Progress::Paused { .. })),
            "{seen:?}"
        );
        pause_tx.send(false).unwrap();
        let out = task.await.unwrap().unwrap();
        assert!(mock.state.lock().unwrap().chunk_puts >= 2);
        assert!(!out.clip_id.is_empty());

        let (_tx2, rx2) = watch::channel(true);
        let ctl = fast(rx2);
        let (p3, _) = file(dir.path(), "d.mkv", 2600);
        let cancel = ctl.cancel.clone();
        let api3 = api.clone();
        let task = tokio::spawn(async move {
            upload_file(
                &api3,
                &p3,
                UploadMeta::default(),
                &ctl,
                mpsc::unbounded_channel().0,
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
        let r = task.await.unwrap();
        assert!(matches!(r, Err(UploadError::Cancelled)), "{r:?}");
    }

    #[tokio::test]
    async fn share_waits_for_the_preview() {
        let mock = Mock::start().await;
        let api = mock.paired_api().await;
        let dir = tempfile::tempdir().unwrap();
        let (p, _) = file(dir.path(), "e.mkv", 1500);
        mock.state.lock().unwrap().preview_after_polls = 2;
        let (ctl, _pause) = Control::never_paused();
        let ctl = fast(ctl.paused);
        assert!(
            matches!(
                share_when_ready(&api, None, &ctl, |_, _| {}).await,
                Err(UploadError::Failed(_))
            ),
            "no clip yet"
        );
        let out = upload_file(
            &api,
            &p,
            UploadMeta::default(),
            &ctl,
            mpsc::unbounded_channel().0,
        )
        .await
        .unwrap();
        let mut steps = Vec::new();
        let answer = share_when_ready(&api, None, &ctl, |s, _| steps.push(s.to_string()))
            .await
            .unwrap();
        assert!(
            matches!(answer, ShareAnswer::Ready { ref clip_id, .. } if *clip_id == out.clip_id)
        );
        assert!(steps.iter().any(|s| s == "preview"), "{steps:?}");
    }
}
