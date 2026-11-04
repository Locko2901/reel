//! Versions as the updater compares them: a pre-release before the release of its number.

/// Whether version `a` comes before `b`.
pub fn older(a: &str, b: &str) -> bool {
    let split = |v: &str| {
        let v = v.trim().trim_start_matches('v');
        let (main, pre) = match v.split_once(['-', '+']) {
            Some((m, _)) => (m, true),
            None => (v, false),
        };
        let nums: Vec<u64> = main.split('.').map(|p| p.parse().unwrap_or(0)).collect();
        (nums, pre)
    };
    let ((x, x_pre), (y, y_pre)) = (split(a), split(b));
    for i in 0..x.len().max(y.len()) {
        let (p, q) = (
            x.get(i).copied().unwrap_or(0),
            y.get(i).copied().unwrap_or(0),
        );
        if p != q {
            return p < q;
        }
    }
    x_pre && !y_pre
}

#[cfg(test)]
mod tests {
    use super::older;

    #[test]
    fn by_number_and_a_test_build_before_its_release() {
        assert!(older("1.3.1", "1.4.0"));
        assert!(older("1.9.0", "1.10.0"));
        assert!(older("1.4.1-dev.abc", "1.4.1"));
        assert!(older("v1.4.0", "1.4.1"));
        assert!(!older("1.4.1", "1.4.1"));
        assert!(!older("2.0.0", "1.10.0"));
        assert!(!older("1.4.1", "1.4.1-dev.abc"));
    }
}
