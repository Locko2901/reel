//! An in-process device API for the tests, with knobs to cut a chunk or drop a request.
use crate::api::Api;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const CHUNK: u64 = 1024;
pub const MAX_CHUNK: u64 = 4096;

#[derive(Default)]
pub struct MockUpload {
    pub device: String,
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    pub data: Vec<u8>,
    pub done: bool,
}

#[derive(Clone)]
pub struct MockClip {
    pub id: String,
    pub device: String,
    pub title: String,
    pub sha256: String,
    pub state: String,
    pub polls_until_ready: u32,
    pub preview_polls: u32,
}

pub struct MockState {
    pub code: String,
    pub token: String,
    pub rate_limit: bool,
    pub cut_next_chunk: bool,
    /// The first finish sees a flipped byte, answers 422 and resets the upload.
    pub corrupt_first_finish: bool,
    pub drop_chunks: u32,
    pub ready_after_polls: u32,
    pub preview_after_polls: u32,
    pub uploads: HashMap<String, MockUpload>,
    pub clips: Vec<MockClip>,
    pub share_calls: u32,
    pub chunk_puts: u32,
    next: u32,
}

impl MockState {
    pub fn ready_all(&mut self) {
        for c in &mut self.clips {
            c.state = "ready".into();
            c.polls_until_ready = 0;
            c.preview_polls = 0;
        }
    }
    fn next_id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }
    fn clip_mut(&mut self, device: &str, id: &str) -> Option<&mut MockClip> {
        self.clips
            .iter_mut()
            .find(|c| c.id == id && c.device == device)
    }
}

pub struct Mock {
    pub url: String,
    pub code: String,
    pub state: Arc<Mutex<MockState>>,
}

type St = Arc<Mutex<MockState>>;

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

#[allow(clippy::result_large_err)]
fn device_of(st: &St, headers: &HeaderMap) -> Result<String, Response> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());
    let s = st.lock().unwrap();
    match token.as_deref() {
        Some(t) if t == s.token => Ok("d1".into()),
        Some("other-device") => Ok("d2".into()),
        _ => Err(err(StatusCode::UNAUTHORIZED, "no such device")),
    }
}

fn clip_json(c: &MockClip) -> Value {
    json!({
        "id": c.id, "title": c.title, "state": c.state,
        "progress": if c.state == "ready" { 1.0 } else { 0.5 },
        "ready": c.state == "ready",
        "preview_ready": c.state == "ready" && c.preview_polls == 0,
        "error": Value::Null, "share_url": Value::Null,
    })
}

async fn pair(State(st): State<St>, Json(body): Json<Value>) -> Response {
    let s = st.lock().unwrap();
    if s.rate_limit {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "7")],
            Json(json!({ "error": "too many tries" })),
        )
            .into_response();
    }
    if body["code"].as_str() != Some(s.code.as_str()) {
        return err(
            StatusCode::FORBIDDEN,
            "that code is not right, or it expired",
        );
    }
    let token = s.token.clone();
    (
        StatusCode::CREATED,
        Json(json!({ "token": token, "device_id": "d1", "name": body["name"], "user": "robin", "server_name": "reel" })),
    )
        .into_response()
}

async fn whoami(State(st): State<St>, headers: HeaderMap) -> Response {
    let device = match device_of(&st, &headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    Json(json!({ "device_id": device, "name": "pc", "user": "robin", "server_name": "reel", "public_url": "https://clips.example.org", "chunk_bytes": CHUNK, "max_chunk_bytes": MAX_CHUNK, "version": "0.1.0" })).into_response()
}

async fn create(State(st): State<St>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let device = match device_of(&st, &headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut s = st.lock().unwrap();
    let sha = body["sha256"].as_str().unwrap_or("").to_string();
    if let Some(c) = s.clips.iter().find(|c| c.sha256 == sha) {
        let c = c.clone();
        return Json(json!({ "status": "exists", "clip_id": c.id, "clip": clip_json(&c) }))
            .into_response();
    }
    if let Some((id, u)) = s
        .uploads
        .iter()
        .find(|(_, u)| u.device == device && u.sha256 == sha && !u.done)
    {
        return Json(json!({ "status": "resumed", "upload_id": id, "offset": u.data.len(), "chunk_bytes": CHUNK })).into_response();
    }
    let id = s.next_id("u");
    s.uploads.insert(
        id.clone(),
        MockUpload {
            device,
            file_name: body["file_name"].as_str().unwrap_or("clip").into(),
            size: body["size"].as_u64().unwrap_or(0),
            sha256: sha,
            data: Vec::new(),
            done: false,
        },
    );
    (
        StatusCode::CREATED,
        Json(json!({ "status": "created", "upload_id": id, "offset": 0, "chunk_bytes": CHUNK })),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
struct OffsetQ {
    offset: u64,
}

async fn chunk(
    State(st): State<St>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<OffsetQ>,
    body: Bytes,
) -> Response {
    let device = match device_of(&st, &headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut s = st.lock().unwrap();
    s.chunk_puts += 1;
    if body.len() as u64 > MAX_CHUNK {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "chunk too big");
    }
    if s.drop_chunks > 0 {
        s.drop_chunks -= 1;
        return err(StatusCode::SERVICE_UNAVAILABLE, "busy");
    }
    let cut = s.cut_next_chunk;
    s.cut_next_chunk = false;
    let Some(u) = s.uploads.get_mut(&id).filter(|u| u.device == device) else {
        return err(StatusCode::NOT_FOUND, "no such upload");
    };
    if u.done {
        return err(StatusCode::CONFLICT, "finished already");
    }
    let have = u.data.len() as u64;
    if q.offset != have {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": format!("the upload is at {have}"), "offset": have })),
        )
            .into_response();
    }
    if cut {
        u.data.extend_from_slice(&body[..body.len() / 2]);
        return err(StatusCode::INTERNAL_SERVER_ERROR, "connection cut");
    }
    u.data.extend_from_slice(&body);
    Json(json!({ "offset": u.data.len() })).into_response()
}

async fn status(State(st): State<St>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let device = match device_of(&st, &headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let s = st.lock().unwrap();
    let Some(u) = s.uploads.get(&id).filter(|u| u.device == device) else {
        return err(StatusCode::NOT_FOUND, "no such upload");
    };
    Json(json!({ "upload_id": id, "offset": u.data.len(), "size": u.size, "state": if u.done { "done" } else { "open" } })).into_response()
}

async fn finish(State(st): State<St>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let device = match device_of(&st, &headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut s = st.lock().unwrap();
    let ready_after = s.ready_after_polls;
    let preview_after = s.preview_after_polls;
    let corrupt = s.corrupt_first_finish;
    s.corrupt_first_finish = false;
    let Some(u) = s.uploads.get_mut(&id).filter(|u| u.device == device) else {
        return err(StatusCode::NOT_FOUND, "no such upload");
    };
    let have = u.data.len() as u64;
    if have != u.size {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": format!("{have} of {} bytes arrived", u.size), "offset": have })),
        )
            .into_response();
    }
    if corrupt {
        u.data[0] ^= 0xff;
    }
    if crate::hash::sha256_hex(&u.data) != u.sha256 {
        u.data.clear();
        return err(StatusCode::UNPROCESSABLE_ENTITY, "checksum mismatch");
    }
    u.done = true;
    let (sha, title) = (u.sha256.clone(), u.file_name.clone());
    let cid = s.next_id("c");
    s.clips.push(MockClip {
        id: cid.clone(),
        device,
        title,
        sha256: sha,
        state: "processing".into(),
        polls_until_ready: ready_after,
        preview_polls: preview_after,
    });
    Json(json!({ "clip_id": cid, "state": "processing" })).into_response()
}

async fn abandon(State(st): State<St>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let device = match device_of(&st, &headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut s = st.lock().unwrap();
    match s.uploads.get(&id).filter(|u| u.device == device) {
        Some(_) => {
            s.uploads.remove(&id);
            Json(json!({ "ok": true })).into_response()
        }
        None => err(StatusCode::NOT_FOUND, "no such upload"),
    }
}

fn tick(c: &mut MockClip) {
    if c.state == "processing" {
        if c.polls_until_ready == 0 {
            c.state = "ready".into();
        } else {
            c.polls_until_ready -= 1;
        }
    } else if c.preview_polls > 0 {
        c.preview_polls -= 1;
    }
}

async fn clips(State(st): State<St>, headers: HeaderMap) -> Response {
    let device = match device_of(&st, &headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let s = st.lock().unwrap();
    let list: Vec<Value> = s
        .clips
        .iter()
        .rev()
        .filter(|c| c.device == device)
        .map(clip_json)
        .collect();
    Json(json!({ "clips": list })).into_response()
}

async fn clip(State(st): State<St>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let device = match device_of(&st, &headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut s = st.lock().unwrap();
    let Some(c) = s.clip_mut(&device, &id) else {
        return err(StatusCode::NOT_FOUND, "not your clip");
    };
    tick(c);
    Json(clip_json(c)).into_response()
}

async fn share(State(st): State<St>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    let device = match device_of(&st, &headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut s = st.lock().unwrap();
    s.share_calls += 1;
    let found = match body["clip_id"].as_str() {
        Some(id) => s.clip_mut(&device, id),
        None => s.clips.iter_mut().rev().find(|c| c.device == device),
    };
    let Some(c) = found else {
        return err(StatusCode::NOT_FOUND, "no clip from this device yet");
    };
    tick(c);
    if c.state == "failed" {
        return err(StatusCode::CONFLICT, "that clip failed");
    }
    if c.state != "ready" {
        return (
            StatusCode::ACCEPTED,
            Json(json!({ "status": "preparing", "clip_id": c.id, "title": c.title, "step": "ingest", "progress": 0.5 })),
        )
            .into_response();
    }
    if c.preview_polls > 0 {
        return (
            StatusCode::ACCEPTED,
            Json(json!({ "status": "preparing", "clip_id": c.id, "title": c.title, "step": "preview", "progress": 0.7 })),
        )
            .into_response();
    }
    Json(json!({ "status": "ready", "url": format!("https://clips.example.org/s/tok{}", c.id), "clip_id": c.id, "title": c.title })).into_response()
}

impl Mock {
    pub async fn start() -> Mock {
        let state = Arc::new(Mutex::new(MockState {
            code: "abcd-2345".into(),
            token: "tok_9f8e7d6c5b4a39281706f5e4d3c2b1a0ffeeddccbbaa99887766554433".into(),
            rate_limit: false,
            cut_next_chunk: false,
            corrupt_first_finish: false,
            drop_chunks: 0,
            ready_after_polls: 1,
            preview_after_polls: 0,
            uploads: HashMap::new(),
            clips: Vec::new(),
            share_calls: 0,
            chunk_puts: 0,
            next: 0,
        }));
        let app = Router::new()
            .route("/api/device/pair", post(pair))
            .route("/api/device/whoami", get(whoami))
            .route("/api/device/uploads", post(create))
            .route(
                "/api/device/uploads/{id}",
                axum::routing::put(chunk).get(status).delete(abandon),
            )
            .route("/api/device/uploads/{id}/finish", post(finish))
            .route("/api/device/clips", get(clips))
            .route("/api/device/clips/{id}", get(clip))
            .route("/api/device/share", post(share))
            .layer(axum::extract::DefaultBodyLimit::max(
                (MAX_CHUNK * 2) as usize,
            ))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Mock {
            url: format!("http://127.0.0.1:{}", addr.port()),
            code: "abcd-2345".into(),
            state,
        }
    }

    pub async fn paired_api(&self) -> Api {
        let p = Api::pair(&self.url, &self.code, "test pc").await.unwrap();
        Api::new(&self.url, Some(p.token)).unwrap()
    }
}
