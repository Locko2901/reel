//! The password rule, the setup and pairing codes, and a constant-time code comparison.
pub const MIN_PASSWORD: usize = 12;
pub const MAX_PASSWORD: usize = 128;

pub fn password_ok(password: &str) -> Result<(), String> {
    let n = password.chars().count();
    if n < MIN_PASSWORD {
        return Err(format!("a password has at least {MIN_PASSWORD} characters"));
    }
    if n > MAX_PASSWORD {
        return Err(format!("a password has at most {MAX_PASSWORD} characters"));
    }
    Ok(())
}

/// The one-time code the first admin needs.
pub fn new_setup_code() -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 10];
    getrandom::fill(&mut bytes).expect("random code");
    let chars: String = bytes
        .iter()
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect();
    format!("{}-{}", &chars[..5], &chars[5..])
}

pub fn new_pair_code() -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("random code");
    let chars: String = bytes
        .iter()
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect();
    format!("{}-{}", &chars[..4], &chars[4..])
}

/// The form a code is compared and hashed in.
pub fn normal_code(code: &str) -> String {
    code.trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

pub fn same_code(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim().to_ascii_lowercase(), b.trim().to_ascii_lowercase());
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_rule_and_the_setup_code() {
        assert!(password_ok("short").is_err());
        assert!(password_ok("twelve chars!").is_ok());
        let c = new_setup_code();
        assert_eq!(c.len(), 11);
        assert_ne!(c, new_setup_code());
        assert!(same_code(&c, &c.to_uppercase()));
        assert!(!same_code(&c, "abcde-fghjk"));
        let p = new_pair_code();
        assert_eq!(p.len(), 9);
        assert_eq!(normal_code(&p).len(), 8);
        assert_eq!(normal_code(" ABCD-2345 "), "abcd2345");
        assert_eq!(normal_code("abcd 2345"), "abcd2345");
    }
}
