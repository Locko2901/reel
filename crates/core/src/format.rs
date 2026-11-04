use jiff::{Timestamp, Zoned};

pub fn duration(secs: f64) -> String {
    let total = secs.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn duration_ms(secs: f64) -> String {
    let secs = secs.max(0.0);
    let whole = secs.floor() as u64;
    let frac = ((secs - whole as f64) * 100.0).round() as u64;
    let (m, s) = (whole / 60, whole % 60);
    format!("{m}:{s:02}.{frac:02}")
}

/// Sizes with one decimal above kB, everywhere: `27.1 GB`, `1.1 MB`, `512 kB`, `9 B`.
pub fn size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e12 {
        format!("{:.1} TB", b / 1e12)
    } else if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} kB", b / 1e3)
    } else {
        format!("{bytes} B")
    }
}

/// A resolution with the times sign between width and height.
pub fn resolution(w: u32, h: u32) -> String {
    format!("{w} \u{d7} {h}")
}

pub fn now() -> String {
    Timestamp::now().to_string()
}

pub fn parse(ts: &str) -> Option<Timestamp> {
    ts.parse::<Timestamp>()
        .ok()
        .or_else(|| ts.parse::<Zoned>().ok().map(|z| z.timestamp()))
}

static TZ: std::sync::RwLock<Option<jiff::tz::TimeZone>> = std::sync::RwLock::new(None);

/// The zone the dates on the pages follow; an empty name means the host's clock.
pub fn set_timezone(name: &str) {
    let tz = if name.is_empty() {
        None
    } else {
        jiff::tz::TimeZone::get(name).ok()
    };
    if let Ok(mut slot) = TZ.write() {
        *slot = tz;
    }
}

pub fn timezone() -> jiff::tz::TimeZone {
    TZ.read()
        .ok()
        .and_then(|t| t.clone())
        .unwrap_or_else(jiff::tz::TimeZone::system)
}

/// A stored moment as the wall clock of the pages' zone, the one a title was made from.
pub fn local_datetime(ts: &str) -> Option<jiff::civil::DateTime> {
    parse(ts).map(|t| local(t).datetime())
}

fn local(ts: Timestamp) -> Zoned {
    ts.to_zoned(timezone())
}

fn squeeze(s: String) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn date(ts: &str) -> String {
    match parse(ts) {
        Some(t) => {
            let z = local(t);
            let now = local(Timestamp::now());
            if z.date() == now.date() {
                "today".into()
            } else if z.date() == now.date().yesterday().unwrap_or(now.date()) {
                "yesterday".into()
            } else if z.year() == now.year() {
                squeeze(z.strftime("%b %e").to_string())
            } else {
                squeeze(z.strftime("%b %e, %Y").to_string())
            }
        }
        None => ts.to_string(),
    }
}

pub fn datetime(ts: &str) -> String {
    match parse(ts) {
        Some(t) => {
            let z = local(t);
            let now = local(Timestamp::now());
            if z.date() == now.date() {
                squeeze(z.strftime("today %H:%M").to_string())
            } else if z.year() == now.year() {
                squeeze(z.strftime("%b %e, %H:%M").to_string())
            } else {
                squeeze(z.strftime("%b %e, %Y %H:%M").to_string())
            }
        }
        None => ts.to_string(),
    }
}

pub fn day_key(ts: &str) -> String {
    match parse(ts) {
        Some(t) => local(t).date().to_string(),
        None => ts.to_string(),
    }
}

pub fn day_label(ts: &str) -> String {
    let Some(t) = parse(ts) else {
        return ts.to_string();
    };
    let z = local(t);
    let now = local(Timestamp::now());
    let days = (now.date() - z.date()).get_days() as i64;
    if days == 0 {
        "Today".into()
    } else if days == 1 {
        "Yesterday".into()
    } else if (2..7).contains(&days) {
        squeeze(z.strftime("%A, %b %e").to_string())
    } else if z.year() == now.year() {
        squeeze(z.strftime("%b %e").to_string())
    } else {
        squeeze(z.strftime("%b %e, %Y").to_string())
    }
}

/// The one shape of a moment on the pages: relative under a day (`10 min ago`), then
/// `Oct 7, 21:14`, with the year when it is not this one.
pub fn when(ts: &str) -> String {
    let Some(t) = parse(ts) else {
        return ts.to_string();
    };
    let secs = Timestamp::now().as_second() - t.as_second();
    if (0..86_400).contains(&secs) {
        return relative(ts);
    }
    let z = local(t);
    let now = local(Timestamp::now());
    if z.year() == now.year() {
        squeeze(z.strftime("%b %e, %H:%M").to_string())
    } else {
        squeeze(z.strftime("%b %e, %Y %H:%M").to_string())
    }
}

/// `when` without the time of day past the last day, for a table's narrow cell: "6 h ago",
/// "Oct 13", "Oct 13, 2025".
pub fn day(ts: &str) -> String {
    let Some(t) = parse(ts) else {
        return ts.to_string();
    };
    let secs = Timestamp::now().as_second() - t.as_second();
    if (0..86_400).contains(&secs) {
        return relative(ts);
    }
    let z = local(t);
    if z.year() == local(Timestamp::now()).year() {
        squeeze(z.strftime("%b %e").to_string())
    } else {
        squeeze(z.strftime("%b %e, %Y").to_string())
    }
}

/// A clip's title from its recording time, in the pages' zone (`names::time_title`).
pub fn title_time(ts: &str) -> String {
    match parse(ts) {
        Some(t) => crate::names::time_title(&local(t).datetime()),
        None => ts.to_string(),
    }
}

pub fn relative(ts: &str) -> String {
    let Some(t) = parse(ts) else {
        return ts.to_string();
    };
    let secs = (Timestamp::now().as_second() - t.as_second()).max(0);
    if secs < 60 {
        "just now".into()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86_400 {
        format!("{} h ago", secs / 3600)
    } else {
        format!("{} d ago", secs / 86_400)
    }
}

pub fn until(ts: &str) -> String {
    let Some(t) = parse(ts) else {
        return ts.to_string();
    };
    let secs = t.as_second() - Timestamp::now().as_second();
    let unit = |n: i64, word: &str| format!("in {n} {word}{}", if n == 1 { "" } else { "s" });
    if secs <= 0 {
        "expired".into()
    } else if secs < 3600 {
        unit((secs / 60).max(1), "minute")
    } else if secs < 86_400 {
        unit(secs / 3600, "hour")
    } else {
        unit(secs / 86_400, "day")
    }
}

/// A long value cut for the pages into head and tail, the tail kept whole so the cut never
/// falls inside the last token.
pub fn split_tail(value: &str, keep: usize) -> (String, String) {
    let n = value.chars().count();
    if n <= keep {
        return (value.to_string(), String::new());
    }
    let at = n - keep;
    (
        value.chars().take(at).collect(),
        value.chars().skip(at).collect(),
    )
}

/// A gain in decibels with its sign, the decimal only when there is one: `+6 dB`, `-3.5 dB`.
pub fn gain(db: f64) -> String {
    let n = if db.fract() == 0.0 {
        format!("{}", db.abs() as i64)
    } else {
        format!("{:.1}", db.abs())
    };
    match db {
        d if d > 0.0 => format!("+{n} dB"),
        d if d < 0.0 => format!("-{n} dB"),
        _ => "0 dB".into(),
    }
}

/// A measured level with one decimal: `-16.3 dBTP`, `-21.8 LUFS`.
pub fn level(v: f64, unit: &str) -> String {
    format!("{v:.1} {unit}")
}

pub fn mbit(bits: u64) -> String {
    format!("{:.1} Mbit/s", bits as f64 / 1e6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_leaves_out_the_time_past_the_last_day() {
        let ago = |hours: i64| {
            Timestamp::now()
                .checked_sub(jiff::SignedDuration::from_hours(hours))
                .unwrap()
                .to_string()
        };
        assert!(day(&ago(3)).ends_with("ago"), "{}", day(&ago(3)));
        let older = day(&ago(24 * 3));
        assert!(!older.contains(':') && !older.contains("ago"), "{older}");
    }

    #[test]
    fn gains_carry_their_sign_and_levels_one_decimal() {
        assert_eq!(gain(6.0), "+6 dB");
        assert_eq!(gain(-3.5), "-3.5 dB");
        assert_eq!(gain(0.0), "0 dB");
        assert_eq!(level(-16.0, "LUFS"), "-16.0 LUFS");
        assert_eq!(level(-1.04, "dBTP"), "-1.0 dBTP");
    }

    #[test]
    fn sizes_carry_one_decimal_above_kb() {
        assert_eq!(size(9), "9 B");
        assert_eq!(size(512_000), "512 kB");
        assert_eq!(size(1_140_000), "1.1 MB");
        assert_eq!(size(27_130_000_000), "27.1 GB");
        assert_eq!(size(15_000_000_000_000), "15.0 TB");
    }

    #[test]
    fn a_moment_reads_relative_under_a_day_then_as_a_date() {
        let now = Timestamp::now();
        let ago = (now - jiff::Span::new().minutes(10)).to_string();
        assert_eq!(when(&ago), "10 min ago");
        let old = (now - jiff::Span::new().hours(30)).to_string();
        let w = when(&old);
        assert!(w.contains(':') && !w.contains("ago"), "{w}");
        assert_eq!(resolution(1920, 1080), "1920 \u{d7} 1080");
    }

    #[test]
    fn day_labels() {
        let now = Timestamp::now();
        let at = |days: i64| {
            now.checked_sub(jiff::Span::new().hours(24 * days))
                .unwrap()
                .to_string()
        };
        assert_eq!(day_label(&at(0)), "Today");
        assert_eq!(day_label(&at(1)), "Yesterday");
        let week = day_label(&at(3));
        assert!(week.contains(", "), "weekday and date: {week}");
        let older = day_label(&at(30));
        assert!(!older.contains(", ") || older.len() > 8, "{older}");
        assert_eq!(day_key(&at(0)), day_key(&now.to_string()));
        assert_ne!(day_key(&at(0)), day_key(&at(1)));
    }

    #[test]
    fn durations_and_sizes() {
        assert_eq!(duration(72.0), "1:12");
        assert_eq!(duration(3661.0), "1:01:01");
        assert_eq!(duration_ms(8.5), "0:08.50");
        assert_eq!(size(56_300_000), "56.3 MB");
        assert_eq!(size(1_250_000_000), "1.2 GB");
    }
}
