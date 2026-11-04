//! The client for reel's device API. Headers are never logged.
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("the server does not know this device; pair it again")]
    Unauthorized,
    #[error("too many tries; wait {0} s")]
    RateLimited(u64),
    #[error("{message}")]
    Conflict {
        message: String,
        offset: Option<u64>,
    },
    #[error("the file did not arrive whole (checksum mismatch); it goes again")]
    ChecksumMismatch,
    #[error("not found on the server")]
    NotFound,
    #[error("{1}")]
    Server(u16, String),
    #[error("cannot reach the server: {0}")]
    Network(String),
    #[error("{0}")]
    Invalid(String),
}

impl ApiError {
    /// Worth another try later: the network, a 5xx, a rate limit.
    pub fn is_transient(&self) -> bool {
        match self {
            ApiError::Network(_) | ApiError::RateLimited(_) => true,
            ApiError::Server(code, _) => *code >= 500,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Paired {
    pub token: String,
    pub device_id: String,
    pub name: String,
    pub user: String,
    #[serde(default)]
    pub server_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WhoAmI {
    pub device_id: String,
    pub name: String,
    pub user: String,
    #[serde(default)]
    pub server_name: String,
    #[serde(default)]
    pub public_url: String,
    pub chunk_bytes: u64,
    pub max_chunk_bytes: u64,
    #[serde(default)]
    pub version: String,
    /// The newest release the server last read from the repository.
    #[serde(default)]
    pub newest_release: Option<String>,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub normalize_new: Option<bool>,
    #[serde(default)]
    pub loudness_target: Option<i32>,
    /// The largest upload the server takes, for the setup to warn about a clip too long.
    #[serde(default)]
    pub upload_max_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct NewUpload {
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_last: Option<u64>,
    /// The audio tracks' names in the file's order, where the file may not carry them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Created {
    Created {
        upload_id: String,
        offset: u64,
        chunk_bytes: u64,
    },
    Resumed {
        upload_id: String,
        offset: u64,
        chunk_bytes: u64,
    },
    Exists {
        clip_id: String,
        #[serde(default)]
        clip: Option<Box<ClipStatus>>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct UploadStatus {
    pub upload_id: String,
    pub offset: u64,
    pub size: u64,
    pub state: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Finished {
    pub clip_id: String,
    pub state: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(default)]
pub struct ClipStatus {
    pub id: String,
    pub title: String,
    pub state: String,
    pub progress: f64,
    pub ready: bool,
    pub preview_ready: bool,
    pub preview_progress: Option<f64>,
    pub preview_error: Option<String>,
    pub error: Option<String>,
    pub share_url: Option<String>,
    pub created_at: Option<String>,
    pub recorded_at: Option<String>,
}

impl ClipStatus {
    pub fn failed(&self) -> bool {
        self.state == "failed"
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ShareAnswer {
    Ready {
        url: String,
        clip_id: String,
        #[serde(default)]
        title: String,
    },
    Preparing {
        clip_id: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        step: String,
        #[serde(default)]
        progress: f64,
    },
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error: String,
    #[serde(default)]
    offset: Option<u64>,
}

#[derive(Clone)]
pub struct Api {
    client: reqwest::Client,
    base: String,
    token: Option<String>,
}

impl Api {
    /// `server` is the server URL as the settings keep it; it is checked by the settings rule.
    pub fn new(server: &str, token: Option<String>) -> Result<Api, ApiError> {
        let base = crate::settings::server_base(server).map_err(ApiError::Invalid)?;
        let client = reqwest::Client::builder()
            .user_agent(crate::user_agent())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        Ok(Api {
            client,
            base,
            token,
        })
    }

    pub fn with_token(&self, token: Option<String>) -> Api {
        Api {
            client: self.client.clone(),
            base: self.base.clone(),
            token,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/device/{}", self.base, path.trim_start_matches('/'))
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(t) => req.header(AUTHORIZATION, format!("Bearer {t}")),
            None => req,
        }
    }

    async fn send<T: for<'de> Deserialize<'de>>(
        &self,
        what: &str,
        req: reqwest::RequestBuilder,
    ) -> Result<(StatusCode, T), ApiError> {
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(describe(&e)))?;
        let status = resp.status();
        tracing::debug!(what, status = status.as_u16(), "device api");
        if status.is_success() {
            let body = resp.json::<T>().await.map_err(|e| {
                ApiError::Server(status.as_u16(), format!("unexpected answer: {e}"))
            })?;
            return Ok((status, body));
        }
        let text = resp.text().await.unwrap_or_default();
        let parsed: ErrorBody = serde_json::from_str(&text).unwrap_or(ErrorBody {
            error: String::new(),
            offset: None,
        });
        let message = if parsed.error.is_empty() {
            if text.trim().is_empty() {
                format!("the server answered {status}")
            } else {
                text.chars().take(200).collect()
            }
        } else {
            parsed.error
        };
        Err(match status {
            StatusCode::UNAUTHORIZED => ApiError::Unauthorized,
            StatusCode::TOO_MANY_REQUESTS => ApiError::RateLimited(retry_after(&message)),
            StatusCode::CONFLICT => ApiError::Conflict {
                message,
                offset: parsed.offset,
            },
            StatusCode::UNPROCESSABLE_ENTITY => ApiError::ChecksumMismatch,
            StatusCode::NOT_FOUND => ApiError::NotFound,
            other => ApiError::Server(other.as_u16(), message),
        })
    }

    pub async fn pair(server: &str, code: &str, name: &str) -> Result<Paired, ApiError> {
        let api = Api::new(server, None)?;
        let req = api
            .client
            .post(api.url("pair"))
            .json(&serde_json::json!({ "code": code.trim(), "name": name.trim() }));
        let (_, body) = api.send_pair(req).await?;
        Ok(body)
    }

    async fn send_pair(
        &self,
        req: reqwest::RequestBuilder,
    ) -> Result<(StatusCode, Paired), ApiError> {
        let resp = req
            .send()
            .await
            .map_err(|e| ApiError::Network(describe(&e)))?;
        let status = resp.status();
        tracing::debug!(what = "pair", status = status.as_u16(), "device api");
        if status.is_success() {
            let body = resp.json::<Paired>().await.map_err(|e| {
                ApiError::Server(status.as_u16(), format!("unexpected answer: {e}"))
            })?;
            return Ok((status, body));
        }
        let retry = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok());
        let text = resp.text().await.unwrap_or_default();
        let parsed: ErrorBody = serde_json::from_str(&text).unwrap_or(ErrorBody {
            error: String::new(),
            offset: None,
        });
        let message = if parsed.error.is_empty() {
            format!("the server answered {status}")
        } else {
            parsed.error
        };
        Err(match status {
            StatusCode::TOO_MANY_REQUESTS => ApiError::RateLimited(retry.unwrap_or(120)),
            StatusCode::FORBIDDEN | StatusCode::UNAUTHORIZED => {
                ApiError::Server(status.as_u16(), message)
            }
            StatusCode::NOT_FOUND => ApiError::Server(
                404,
                "that server has no device API; is the URL right and reel up to date?".into(),
            ),
            other => ApiError::Server(other.as_u16(), message),
        })
    }

    pub async fn whoami(&self) -> Result<WhoAmI, ApiError> {
        let req = self.auth(self.client.get(self.url("whoami")));
        Ok(self.send("whoami", req).await?.1)
    }

    pub async fn sections(&self) -> Result<crate::shape::Shape, ApiError> {
        let req = self.auth(self.client.get(self.url("sections")));
        let (_, v): (StatusCode, serde_json::Value) = self.send("sections", req).await?;
        crate::shape::Shape::checked(v)
            .ok_or_else(|| ApiError::Invalid("the sections answer is out of bounds".into()))
    }

    pub async fn create_upload(&self, new: &NewUpload) -> Result<Created, ApiError> {
        let req = self.auth(self.client.post(self.url("uploads")).json(new));
        Ok(self.send("create upload", req).await?.1)
    }

    /// Returns the new end; `len` is the body's length, as the caller may pace it as a stream.
    pub async fn put_chunk(
        &self,
        upload_id: &str,
        offset: u64,
        body: reqwest::Body,
        len: u64,
    ) -> Result<u64, ApiError> {
        let req = self
            .auth(self.client.put(format!(
                "{}?offset={offset}",
                self.url(&format!("uploads/{upload_id}"))
            )))
            .header(CONTENT_TYPE, "application/octet-stream")
            .header(CONTENT_LENGTH, len)
            .body(body);
        #[derive(Deserialize)]
        struct End {
            offset: u64,
        }
        let (_, end): (_, End) = self.send("chunk", req).await?;
        Ok(end.offset)
    }

    pub async fn upload_status(&self, upload_id: &str) -> Result<UploadStatus, ApiError> {
        let req = self.auth(self.client.get(self.url(&format!("uploads/{upload_id}"))));
        Ok(self.send("upload status", req).await?.1)
    }

    pub async fn finish_upload(&self, upload_id: &str) -> Result<Finished, ApiError> {
        let req = self.auth(
            self.client
                .post(self.url(&format!("uploads/{upload_id}/finish"))),
        );
        Ok(self.send("finish", req).await?.1)
    }

    pub async fn abandon_upload(&self, upload_id: &str) -> Result<(), ApiError> {
        let req = self.auth(
            self.client
                .delete(self.url(&format!("uploads/{upload_id}"))),
        );
        let _: (_, serde_json::Value) = self.send("abandon", req).await?;
        Ok(())
    }

    pub async fn clips(&self) -> Result<Vec<ClipStatus>, ApiError> {
        #[derive(Deserialize)]
        struct List {
            clips: Vec<ClipStatus>,
        }
        let req = self.auth(self.client.get(self.url("clips")));
        let (_, l): (_, List) = self.send("clips", req).await?;
        Ok(l.clips)
    }

    pub async fn clip(&self, clip_id: &str) -> Result<ClipStatus, ApiError> {
        let req = self.auth(self.client.get(self.url(&format!("clips/{clip_id}"))));
        Ok(self.send("clip", req).await?.1)
    }

    pub async fn share(&self, clip_id: Option<&str>) -> Result<ShareAnswer, ApiError> {
        let body = match clip_id {
            Some(id) => serde_json::json!({ "clip_id": id }),
            None => serde_json::json!({}),
        };
        let req = self.auth(self.client.post(self.url("share")).json(&body));
        Ok(self.send("share", req).await?.1)
    }
}

fn describe(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "timed out".into()
    } else if e.is_connect() {
        "no connection".into()
    } else {
        let mut s = e.to_string();
        if let Some(i) = s.find(" for url") {
            s.truncate(i);
        }
        s
    }
}

fn retry_after(message: &str) -> u64 {
    message
        .split_whitespace()
        .filter_map(|w| w.parse::<u64>().ok())
        .next()
        .unwrap_or(120)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::Mock;

    #[tokio::test]
    async fn pairing_and_whoami_against_the_mock() {
        let mock = Mock::start().await;
        let bad = Api::pair(&mock.url, "wrong-code", "pc").await;
        assert!(matches!(bad, Err(ApiError::Server(403, _))), "{bad:?}");
        let paired = Api::pair(&mock.url, &mock.code, "gaming pc").await.unwrap();
        assert_eq!(paired.name, "gaming pc");
        assert_eq!(paired.user, "robin");
        let api = Api::new(&mock.url, Some(paired.token.clone())).unwrap();
        let me = api.whoami().await.unwrap();
        assert_eq!(me.device_id, paired.device_id);
        assert_eq!(me.chunk_bytes, 1024);
        let stranger = Api::new(&mock.url, Some("nope".into())).unwrap();
        assert!(matches!(
            stranger.whoami().await,
            Err(ApiError::Unauthorized)
        ));
        let unpaired = Api::new(&mock.url, None).unwrap();
        assert!(matches!(
            unpaired.whoami().await,
            Err(ApiError::Unauthorized)
        ));
        mock.state.lock().unwrap().rate_limit = true;
        assert!(matches!(
            Api::pair(&mock.url, &mock.code, "x").await,
            Err(ApiError::RateLimited(7))
        ));
        assert!(Api::new("http://10.0.0.1:1", None).is_err());
        let down = Api::new("http://127.0.0.1:1", Some("t".into())).unwrap();
        let e = down.whoami().await.unwrap_err();
        assert!(e.is_transient(), "{e}");
    }

    #[test]
    fn whoami_reads_the_servers_new_clip_rules_and_an_older_server() {
        let w: WhoAmI = serde_json::from_str(r#"{"device_id":"d","name":"pc","user":"u","chunk_bytes":1,"max_chunk_bytes":2,"admin":true,"normalize_new":true,"loudness_target":-16}"#).unwrap();
        assert!(w.admin);
        assert_eq!(
            (w.normalize_new, w.loudness_target),
            (Some(true), Some(-16))
        );
        let old: WhoAmI = serde_json::from_str(
            r#"{"device_id":"d","name":"pc","user":"u","chunk_bytes":1,"max_chunk_bytes":2}"#,
        )
        .unwrap();
        assert_eq!((old.admin, old.normalize_new), (false, None));
    }

    #[test]
    fn an_upload_asks_for_its_last_seconds_only_when_set() {
        let whole = NewUpload {
            file_name: "a.mkv".into(),
            ..Default::default()
        };
        assert!(serde_json::to_value(&whole)
            .unwrap()
            .get("keep_last")
            .is_none());
        let short = NewUpload {
            keep_last: Some(30),
            ..whole
        };
        assert_eq!(serde_json::to_value(&short).unwrap()["keep_last"], 30);
    }

    #[tokio::test]
    async fn the_upload_calls_map_their_answers() {
        let mock = Mock::start().await;
        mock.state.lock().unwrap().ready_after_polls = 3;
        let api = mock.paired_api().await;
        let data = vec![7u8; 2500];
        let sha = crate::hash::sha256_hex(&data);
        let new = NewUpload {
            file_name: "a.mkv".into(),
            size: data.len() as u64,
            sha256: sha.clone(),
            ..Default::default()
        };
        let Created::Created {
            upload_id,
            offset,
            chunk_bytes,
        } = api.create_upload(&new).await.unwrap()
        else {
            panic!("created")
        };
        assert_eq!((offset, chunk_bytes), (0, 1024));
        let again = api.create_upload(&new).await.unwrap();
        assert!(
            matches!(again, Created::Resumed { offset: 0, .. }),
            "{again:?}"
        );
        let end = api
            .put_chunk(&upload_id, 0, data[..1024].to_vec().into(), 1024)
            .await
            .unwrap();
        assert_eq!(end, 1024);
        let wrong = api
            .put_chunk(&upload_id, 0, data[..1024].to_vec().into(), 1024)
            .await
            .unwrap_err();
        assert!(
            matches!(
                wrong,
                ApiError::Conflict {
                    offset: Some(1024),
                    ..
                }
            ),
            "{wrong:?}"
        );
        let st = api.upload_status(&upload_id).await.unwrap();
        assert_eq!(st.offset, 1024);
        let early = api.finish_upload(&upload_id).await.unwrap_err();
        assert!(matches!(
            early,
            ApiError::Conflict {
                offset: Some(1024),
                ..
            }
        ));
        api.put_chunk(&upload_id, 1024, data[1024..2048].to_vec().into(), 1024)
            .await
            .unwrap();
        api.put_chunk(&upload_id, 2048, data[2048..].to_vec().into(), 452)
            .await
            .unwrap();
        let fin = api.finish_upload(&upload_id).await.unwrap();
        assert_eq!(fin.state, "processing");
        let c = api.clip(&fin.clip_id).await.unwrap();
        assert_eq!(c.state, "processing");
        let exists = api.create_upload(&new).await.unwrap();
        assert!(matches!(exists, Created::Exists { ref clip_id, .. } if *clip_id == fin.clip_id));
        let list = api.clips().await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(matches!(api.clip("nope").await, Err(ApiError::NotFound)));
        let share = api.share(None).await.unwrap();
        assert!(matches!(share, ShareAnswer::Preparing { ref step, .. } if step == "ingest"));
        mock.state.lock().unwrap().ready_all();
        let share = api.share(Some(&fin.clip_id)).await.unwrap();
        assert!(matches!(share, ShareAnswer::Ready { ref url, .. } if url.starts_with("https://")));
        let other = Api::new(&mock.url, Some("other-device".into())).unwrap();
        assert!(
            matches!(other.clip(&fin.clip_id).await, Err(ApiError::NotFound)),
            "scoped"
        );
        api.abandon_upload(&upload_id).await.unwrap();
    }

    #[tokio::test]
    async fn no_token_reaches_the_log() {
        let (ring, sub) = crate::log::ring_subscriber("trace");
        let mock = Mock::start().await;
        let api = mock.paired_api().await;
        let token = mock.state.lock().unwrap().token.clone();
        use tracing::instrument::WithSubscriber;
        async {
            api.whoami().await.unwrap();
            let _ = api.clip("nope").await;
            tracing::info!("the calls are done");
        }
        .with_subscriber(sub)
        .await;
        let lines = ring.all();
        assert!(
            lines.iter().any(|l| l.contains("the calls are done")),
            "{lines:?}"
        );
        assert!(lines.iter().all(|l| !l.contains(&token)), "{lines:?}");
    }
}
