//! The little of S3 that link previews need, signed with AWS Signature Version 4. Addresses are
//! path-style (endpoint/bucket/key), but on AWS, whose buckets answer at bucket.endpoint.

use crate::hash::hex;
use anyhow::{anyhow, bail};
use sha2::{Digest, Sha256};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

/// Where the objects go and the key that may write them. Its Debug never shows the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct Bucket {
    pub endpoint: String,
    pub region: String,
    pub name: String,
    pub access_key: String,
    pub secret_key: String,
}

impl std::fmt::Debug for Bucket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bucket")
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("name", &self.name)
            .field("access_key", &self.access_key)
            .field("secret_key", &reel_core::config::redacted(&self.secret_key))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub key: String,
    pub size: u64,
}

pub struct S3 {
    bucket: Bucket,
    client: reqwest::Client,
}

/// How long a CDN in front of the storage may keep an object: a preview deleted with its link
/// stays at most ten minutes in the edges' caches; a chat app's proxy keeps its own.
pub const CACHE_CONTROL: &str = "public, max-age=600";

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

impl S3 {
    pub fn new(bucket: Bucket) -> anyhow::Result<Self> {
        if bucket.endpoint.is_empty() || bucket.name.is_empty() {
            bail!("the storage needs an endpoint and a bucket");
        }
        if bucket.access_key.is_empty() || bucket.secret_key.is_empty() {
            bail!("the storage needs an access key and its secret");
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .user_agent(format!("reel/{}", reel_core::VERSION))
            .build()?;
        Ok(Self { bucket, client })
    }

    /// The object's address, and its path as the signature reads it.
    fn address(&self, key: &str, query: &[(&str, &str)]) -> anyhow::Result<(reqwest::Url, String)> {
        let base = reqwest::Url::parse(self.bucket.endpoint.trim_end_matches('/'))
            .map_err(|e| anyhow!("the endpoint {}: {e}", self.bucket.endpoint))?;
        let host = base
            .host_str()
            .ok_or_else(|| anyhow!("the endpoint {} has no host", self.bucket.endpoint))?;
        let key_path = key.split('/').map(encode).collect::<Vec<_>>().join("/");
        let virtual_host = host.ends_with("amazonaws.com");
        let path = if virtual_host {
            format!("/{key_path}")
        } else {
            format!("/{}/{key_path}", encode(&self.bucket.name))
        };
        let mut url = base.clone();
        if virtual_host {
            url.set_host(Some(&format!("{}.{host}", self.bucket.name)))?;
        }
        url.set_path(&path);
        let q = canonical_query(query);
        url.set_query((!q.is_empty()).then_some(q.as_str()));
        Ok((url, path))
    }

    async fn send(
        &self,
        method: reqwest::Method,
        key: &str,
        query: &[(&str, &str)],
        body: Vec<u8>,
        extra: &[(&str, &str)],
        sent: Option<Arc<AtomicU64>>,
    ) -> anyhow::Result<reqwest::Response> {
        let (url, path) = self.address(key, query)?;
        let host = match url.port() {
            Some(p) => format!("{}:{p}", url.host_str().unwrap_or_default()),
            None => url.host_str().unwrap_or_default().to_string(),
        };
        let payload = if body.is_empty() {
            EMPTY_SHA256.to_string()
        } else {
            hex(&Sha256::digest(&body))
        };
        let amz_date = jiff::Timestamp::now()
            .strftime("%Y%m%dT%H%M%SZ")
            .to_string();
        let signed = [
            ("host", host.as_str()),
            ("x-amz-content-sha256", payload.as_str()),
            ("x-amz-date", amz_date.as_str()),
        ];
        let authorization = authorization(&Request {
            method: method.as_str(),
            path: &path,
            query,
            headers: &signed,
            payload: &payload,
            amz_date: &amz_date,
            region: &self.bucket.region,
            access_key: &self.bucket.access_key,
            secret_key: &self.bucket.secret_key,
        });
        let mut req = self
            .client
            .request(method, url)
            .timeout(Duration::from_secs(30 + body.len() as u64 / 100_000))
            .header("x-amz-content-sha256", &payload)
            .header("x-amz-date", &amz_date)
            .header("authorization", authorization);
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let body = match sent {
            Some(sent) => {
                req = req.header(reqwest::header::CONTENT_LENGTH, body.len());
                let counted = Counted {
                    body: std::io::Cursor::new(body),
                    sent,
                };
                reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::with_capacity(
                    counted,
                    256 * 1024,
                ))
            }
            None => body.into(),
        };
        req.body(body)
            .send()
            .await
            .map_err(|e| anyhow!("the storage at {}: {}", self.bucket.endpoint, plain(&e)))
    }

    pub async fn put(&self, key: &str, body: Vec<u8>, content_type: &str) -> anyhow::Result<()> {
        self.put_counted(key, body, content_type, None).await
    }

    /// An upload that counts into `sent` the bytes it hands to the connection.
    pub async fn put_counted(
        &self,
        key: &str,
        body: Vec<u8>,
        content_type: &str,
        sent: Option<Arc<AtomicU64>>,
    ) -> anyhow::Result<()> {
        let r = self
            .send(
                reqwest::Method::PUT,
                key,
                &[],
                body,
                &[
                    ("content-type", content_type),
                    ("cache-control", CACHE_CONTROL),
                ],
                sent,
            )
            .await?;
        refused(r, "the upload").await.map(|_| ())
    }

    /// Deletes an object; one that is not there counts as deleted.
    pub async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let r = self
            .send(reqwest::Method::DELETE, key, &[], Vec::new(), &[], None)
            .await?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(());
        }
        refused(r, "the delete").await.map(|_| ())
    }

    /// Every object under a prefix, page after page.
    pub async fn list(&self, prefix: &str) -> anyhow::Result<Vec<Listed>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![("list-type", "2"), ("prefix", prefix)];
            if let Some(t) = &token {
                query.push(("continuation-token", t));
            }
            let r = self
                .send(reqwest::Method::GET, "", &query, Vec::new(), &[], None)
                .await?;
            let xml = refused(r, "the listing").await?;
            for item in elements(&xml, "Contents") {
                let key = elements(item, "Key").first().map(|k| unescape(k));
                let size = elements(item, "Size")
                    .first()
                    .and_then(|s| s.trim().parse::<u64>().ok());
                if let (Some(key), Some(size)) = (key, size) {
                    out.push(Listed { key, size });
                }
            }
            let more = elements(&xml, "IsTruncated").first().map(|v| v.trim()) == Some("true");
            token = elements(&xml, "NextContinuationToken")
                .first()
                .map(|t| unescape(t));
            if !more || token.is_none() {
                return Ok(out);
            }
        }
    }
}

/// The cause of a transport error in words, the deepest one, without the URL.
fn plain(e: &reqwest::Error) -> String {
    let mut cause = None;
    let mut src = std::error::Error::source(e);
    while let Some(s) = src {
        cause = Some(s.to_string());
        src = s.source();
    }
    if e.is_timeout() {
        "no answer in time".into()
    } else if e.is_connect() {
        match cause {
            Some(c) => format!("could not connect: {c}"),
            None => "could not connect".into(),
        }
    } else {
        cause.unwrap_or_else(|| "the request failed".into())
    }
}

/// The body of a success, or the storage's own code and message for a refusal.
async fn refused(r: reqwest::Response, what: &str) -> anyhow::Result<String> {
    let status = r.status();
    let body = r.text().await.unwrap_or_default();
    if status.is_success() {
        return Ok(body);
    }
    let code = elements(&body, "Code").first().map(|c| unescape(c));
    let message = elements(&body, "Message").first().map(|m| unescape(m));
    match (code, message) {
        (Some(c), Some(m)) => bail!("{what}: the storage answered {} {c}: {m}", status.as_u16()),
        (Some(c), None) => bail!("{what}: the storage answered {} {c}", status.as_u16()),
        _ => bail!("{what}: the storage answered {}", status.as_u16()),
    }
}

/// The text inside each `<name>` element, in order; enough for S3's flat answers.
fn elements<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{name}>"), format!("</{name}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        let Some(j) = after.find(&close) else { break };
        out.push(&after[..j]);
        rest = &after[j + close.len()..];
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// S3's URI encoding: every byte but the unreserved characters as %XX.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn canonical_query(query: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> =
        query.iter().map(|(k, v)| (encode(k), encode(v))).collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(k.map(|b| b ^ 0x36));
    inner.update(data);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(k.map(|b| b ^ 0x5c));
    outer.update(inner);
    let mut out = [0u8; 32];
    out.copy_from_slice(&outer.finalize());
    out
}

/// What a signature covers.
struct Request<'a> {
    method: &'a str,
    path: &'a str,
    query: &'a [(&'a str, &'a str)],
    headers: &'a [(&'a str, &'a str)],
    payload: &'a str,
    amz_date: &'a str,
    region: &'a str,
    access_key: &'a str,
    secret_key: &'a str,
}

/// The Authorization header of AWS Signature Version 4 for S3.
fn authorization(r: &Request) -> String {
    let mut headers: Vec<(String, String)> = r
        .headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    headers.sort();
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed_headers = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{}",
        r.method,
        r.path,
        canonical_query(r.query),
        r.payload
    );
    let date = &r.amz_date[..8];
    let scope = format!("{date}/{}/s3/aws4_request", r.region);
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{scope}\n{}",
        r.amz_date,
        hex(&Sha256::digest(canonical.as_bytes()))
    );
    let key = [r.region.as_bytes(), b"s3", b"aws4_request"].iter().fold(
        hmac(format!("AWS4{}", r.secret_key).as_bytes(), date.as_bytes()),
        |k, part| hmac(&k, part),
    );
    format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={}",
        r.access_key,
        hex(&hmac(&key, to_sign.as_bytes()))
    )
}

/// An upload's body, counting what the connection took of it.
struct Counted {
    body: std::io::Cursor<Vec<u8>>,
    sent: Arc<AtomicU64>,
}

impl tokio::io::AsyncRead for Counted {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let r = Pin::new(&mut self.body).poll_read(cx, buf);
        let read = (buf.filled().len() - before) as u64;
        self.sent.fetch_add(read, Ordering::Relaxed);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCESS: &str = "AKIAIOSFODNN7EXAMPLE";
    const SECRET: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";

    fn signature(header: &str) -> &str {
        header.rsplit("Signature=").next().unwrap()
    }

    #[test]
    fn hmac_matches_rfc_4231() {
        assert_eq!(
            hex(&hmac(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(&hmac(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn signs_as_the_s3_documentation_does() {
        let get = authorization(&Request {
            method: "GET",
            path: "/test.txt",
            query: &[],
            headers: &[
                ("Host", "examplebucket.s3.amazonaws.com"),
                ("Range", "bytes=0-9"),
                ("x-amz-content-sha256", EMPTY_SHA256),
                ("x-amz-date", "20130524T000000Z"),
            ],
            payload: EMPTY_SHA256,
            amz_date: "20130524T000000Z",
            region: "us-east-1",
            access_key: ACCESS,
            secret_key: SECRET,
        });
        assert!(get.starts_with("AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, "));
        assert_eq!(
            signature(&get),
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
        let list = authorization(&Request {
            method: "GET",
            path: "/",
            query: &[("max-keys", "2"), ("prefix", "J")],
            headers: &[
                ("Host", "examplebucket.s3.amazonaws.com"),
                ("x-amz-content-sha256", EMPTY_SHA256),
                ("x-amz-date", "20130524T000000Z"),
            ],
            payload: EMPTY_SHA256,
            amz_date: "20130524T000000Z",
            region: "us-east-1",
            access_key: ACCESS,
            secret_key: SECRET,
        });
        assert_eq!(
            signature(&list),
            "34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"
        );
    }

    #[test]
    fn addresses_are_path_style_but_on_aws() {
        let s3 = |endpoint: &str| {
            S3::new(Bucket {
                endpoint: endpoint.into(),
                region: "auto".into(),
                name: "previews".into(),
                access_key: "a".into(),
                secret_key: "b".into(),
            })
            .unwrap()
        };
        let (url, path) = s3("https://acct.r2.cloudflarestorage.com/")
            .address("reel/a b.mp4", &[])
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://acct.r2.cloudflarestorage.com/previews/reel/a%20b.mp4"
        );
        assert_eq!(path, "/previews/reel/a%20b.mp4");
        let (url, _) = s3("http://127.0.0.1:9000")
            .address("", &[("prefix", "reel/"), ("list-type", "2")])
            .unwrap();
        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:9000/previews/?list-type=2&prefix=reel%2F"
        );
        let (url, path) = s3("https://s3.eu-central-1.amazonaws.com")
            .address("reel/x.mp4", &[])
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://previews.s3.eu-central-1.amazonaws.com/reel/x.mp4"
        );
        assert_eq!(path, "/reel/x.mp4");
    }

    #[test]
    fn a_listing_and_a_refusal_read_as_they_should() {
        let xml = "<ListBucketResult><IsTruncated>true</IsTruncated><Contents><Key>reel/a&amp;b.mp4</Key><Size>12</Size></Contents><Contents><Key>reel/c.mp4</Key><Size>3</Size></Contents><NextContinuationToken>t1</NextContinuationToken></ListBucketResult>";
        let keys: Vec<String> = elements(xml, "Contents")
            .iter()
            .map(|c| unescape(elements(c, "Key")[0]))
            .collect();
        assert_eq!(keys, vec!["reel/a&b.mp4", "reel/c.mp4"]);
        assert_eq!(elements(xml, "NextContinuationToken"), vec!["t1"]);
        assert!(elements("<a>", "a").is_empty());
    }

    /// Against a real storage when the environment names one (REEL_TEST_S3_ENDPOINT, _REGION,
    /// _BUCKET, _KEY, _SECRET and _PUBLIC, the bucket's public address); skipped otherwise.
    #[tokio::test]
    async fn a_real_storage_takes_lists_serves_and_deletes() {
        let var = |n: &str| std::env::var(format!("REEL_TEST_S3_{n}")).unwrap_or_default();
        if var("ENDPOINT").is_empty() {
            return;
        }
        let bucket = Bucket {
            endpoint: var("ENDPOINT"),
            region: var("REGION"),
            name: var("BUCKET"),
            access_key: var("KEY"),
            secret_key: var("SECRET"),
        };
        let s3 = S3::new(bucket.clone()).unwrap();
        let key = format!("reel-test/{}.txt", reel_core::ids::new_token());
        s3.put(&key, b"twelve bytes".to_vec(), "text/plain")
            .await
            .unwrap();
        let public = format!("{}/{key}", var("PUBLIC").trim_end_matches('/'));
        let got = reqwest::get(&public).await.unwrap();
        assert_eq!(got.status(), 200);
        assert_eq!(got.bytes().await.unwrap().as_ref(), b"twelve bytes");
        let listed = s3.list("reel-test/").await.unwrap();
        assert!(
            listed.contains(&Listed {
                key: key.clone(),
                size: 12
            }),
            "{listed:?}"
        );
        s3.delete(&key).await.unwrap();
        s3.delete(&key).await.unwrap();
        assert!(!s3
            .list("reel-test/")
            .await
            .unwrap()
            .iter()
            .any(|l| l.key == key));
        assert_eq!(reqwest::get(&public).await.unwrap().status(), 404);
        let wrong = S3::new(Bucket {
            secret_key: "not the secret".into(),
            ..bucket
        })
        .unwrap();
        let e = wrong
            .put(&key, b"x".to_vec(), "text/plain")
            .await
            .unwrap_err();
        assert!(e.to_string().contains("SignatureDoesNotMatch"), "{e}");
    }
}
