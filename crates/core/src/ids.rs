use base64::Engine;

pub fn new_id() -> String {
    ulid::Ulid::new().to_string()
}

pub fn new_token() -> String {
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("os randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

/// A device token: 256 bits from the OS as 43 characters of base64url.
pub fn new_device_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("os randomness");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

pub fn looks_like_device_token(s: &str) -> bool {
    s.len() == 43
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn looks_like_token(s: &str) -> bool {
    s.len() == 22
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn looks_like_id(s: &str) -> bool {
    s.len() == 26 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_22_url_safe_chars_and_unique() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
        assert!(looks_like_token(&a), "{a}");
        assert!(looks_like_token(&b), "{b}");
    }

    #[test]
    fn device_tokens_are_43_url_safe_chars() {
        let t = new_device_token();
        assert!(looks_like_device_token(&t), "{t}");
        assert_ne!(t, new_device_token());
        assert!(!looks_like_device_token(&new_token()));
    }

    #[test]
    fn ids_are_ulids() {
        assert!(looks_like_id(&new_id()));
    }
}
