use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// `progress` gets the bytes hashed so far.
pub async fn sha256_file(
    path: &Path,
    progress: impl Fn(u64) + Send + 'static,
) -> anyhow::Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut f = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        let mut done: u64 = 0;
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            done += n as u64;
            progress(done);
        }
        Ok(hex::encode(hasher.finalize()))
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn hashes_a_file_in_hex_and_reports_progress() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.bin");
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&p, &data).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        let got = sha256_file(&p, move |n| s.lock().unwrap().push(n))
            .await
            .unwrap();
        assert_eq!(got, sha256_hex(&data));
        assert_eq!(got.len(), 64);
        assert!(got
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        let seen = seen.lock().unwrap();
        assert_eq!(*seen.last().unwrap(), 3_000_000);
        assert!(seen.len() >= 3);
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
