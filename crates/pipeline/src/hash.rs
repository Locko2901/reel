//! The SHA-256 of a file, read in a blocking task so a large clip never stalls the runtime.
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_of_bytes(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub async fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let mut f = std::fs::File::open(&path)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(hex(&h.finalize()))
    })
    .await?
}

pub fn looks_like_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) && s == s.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_file_hash_matches_the_bytes_hash() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.bin");
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&p, &data).unwrap();
        assert_eq!(sha256_file(&p).await.unwrap(), sha256_of_bytes(&data));
        assert_eq!(
            sha256_of_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(looks_like_sha256(&sha256_of_bytes(b"x")));
        assert!(!looks_like_sha256("ABC"));
    }
}
