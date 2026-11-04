//! A clip is deleted locally only after the server confirmed it, and only beyond the limits.
use jiff::Timestamp;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct CacheEntry {
    pub path: PathBuf,
    pub bytes: u64,
    pub confirmed_at: Option<Timestamp>,
    pub uploaded: bool,
}

/// Which local files go, in the order they go: the confirmed ones older than `limit_days`,
/// then the oldest confirmed ones while the confirmed total is over `limit_gb`.
pub fn to_delete(
    entries: &[CacheEntry],
    limit_gb: u64,
    limit_days: u64,
    now: Timestamp,
) -> Vec<PathBuf> {
    let mut confirmed: Vec<&CacheEntry> = entries
        .iter()
        .filter(|e| e.uploaded && e.confirmed_at.is_some())
        .collect();
    confirmed.sort_by_key(|e| e.confirmed_at);
    let mut out = Vec::new();
    if limit_gb == 0 {
        return confirmed.into_iter().map(|e| e.path.clone()).collect();
    }
    let max_age = limit_days.saturating_mul(86_400) as i64;
    let mut total: u64 = confirmed.iter().map(|e| e.bytes).sum();
    let mut kept = Vec::new();
    for e in confirmed {
        let at = e.confirmed_at.expect("filtered");
        let age = now.as_second() - at.as_second();
        if limit_days > 0 && age >= max_age {
            out.push(e.path.clone());
            total = total.saturating_sub(e.bytes);
        } else {
            kept.push(e);
        }
    }
    let limit = limit_gb.saturating_mul(1_000_000_000);
    for e in kept {
        if total <= limit {
            break;
        }
        out.push(e.path.clone());
        total = total.saturating_sub(e.bytes);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs_ago: i64, now: Timestamp) -> Timestamp {
        Timestamp::from_second(now.as_second() - secs_ago).unwrap()
    }

    fn entry(name: &str, gb: f64, confirmed_secs_ago: Option<i64>, now: Timestamp) -> CacheEntry {
        CacheEntry {
            path: PathBuf::from(name),
            bytes: (gb * 1e9) as u64,
            confirmed_at: confirmed_secs_ago.map(|s| at(s, now)),
            uploaded: confirmed_secs_ago.is_some(),
        }
    }

    #[test]
    fn only_confirmed_clips_are_candidates() {
        let now = Timestamp::now();
        let entries = vec![
            entry("fresh", 3.0, None, now),
            entry("old-unconfirmed", 3.0, None, now),
        ];
        assert!(to_delete(&entries, 1, 1, now).is_empty());
        assert!(to_delete(&entries, 0, 0, now).is_empty());
    }

    #[test]
    fn zero_gb_deletes_every_confirmed_clip_at_once() {
        let now = Timestamp::now();
        let entries = vec![
            entry("a", 0.1, Some(10), now),
            entry("b", 0.1, None, now),
            entry("c", 0.1, Some(5), now),
        ];
        assert_eq!(
            to_delete(&entries, 0, 14, now),
            vec![PathBuf::from("a"), PathBuf::from("c")]
        );
    }

    #[test]
    fn the_days_rule_goes_first() {
        let now = Timestamp::now();
        let entries = vec![
            entry("young", 1.0, Some(3600), now),
            entry("old", 1.0, Some(15 * 86_400), now),
        ];
        assert_eq!(to_delete(&entries, 50, 14, now), vec![PathBuf::from("old")]);
        assert!(
            to_delete(&entries, 50, 0, now).is_empty(),
            "0 days is no age limit"
        );
    }

    #[test]
    fn the_size_rule_takes_the_oldest_first_until_under_the_limit() {
        let now = Timestamp::now();
        let entries = vec![
            entry("newest", 2.0, Some(10), now),
            entry("middle", 2.0, Some(100), now),
            entry("oldest", 2.0, Some(1000), now),
            entry("pending", 9.0, None, now),
        ];
        assert_eq!(
            to_delete(&entries, 3, 14, now),
            vec![PathBuf::from("oldest"), PathBuf::from("middle")]
        );
        assert_eq!(
            to_delete(&entries, 5, 14, now),
            vec![PathBuf::from("oldest")]
        );
        assert!(to_delete(&entries, 6, 14, now).is_empty());
    }
}
