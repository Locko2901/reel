//! The Stats page's figures, folded from the library's rows by game and month.
use reel_core::format;
use reel_store::stats::StatRow;

#[derive(Debug, PartialEq)]
pub struct GameLine {
    /// The library's filter key: the game in small letters, empty for none.
    pub key: String,
    pub name: String,
    pub clips: i64,
    pub length: String,
    pub size: String,
    /// Its clips against the game with the most, for the row's bar.
    pub bar: u32,
}

impl GameLine {
    /// The library filtered to the game.
    pub fn href(&self) -> String {
        crate::routes::stats::game_href(&self.key)
    }
}

#[derive(Debug, PartialEq)]
pub struct MonthLine {
    pub label: String,
    pub clips: i64,
    pub bar: u32,
}

#[derive(Debug, PartialEq)]
pub struct Figures {
    pub clips: i64,
    pub length: String,
    pub size: String,
    pub games: Vec<GameLine>,
    pub months: Vec<MonthLine>,
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The most months with no clips listed between two that have some: a longer gap (a clip whose
/// date says 1980 after an unzip) is left out, not listed month by month.
const GAP: i64 = 24;

/// "2026-11" as months since year 0, for a year of four digits and a month from 1 to 12.
fn month_index(ym: &str) -> Option<i64> {
    let (y, m) = ym.split_once('-')?;
    if y.len() != 4 || m.len() != 2 {
        return None;
    }
    let (y, m): (i64, i64) = (y.parse().ok()?, m.parse().ok()?);
    (y >= 1 && (1..=12).contains(&m)).then_some(y * 12 + m - 1)
}

fn month_label(i: i64) -> String {
    format!("{} {}", MONTHS[i.rem_euclid(12) as usize], i.div_euclid(12))
}

fn bar(n: i64, most: i64) -> u32 {
    if most <= 0 {
        0
    } else {
        ((n as f64 / most as f64) * 100.0).round() as u32
    }
}

/// The figures: the totals, each game with the most clips first, and every month from the
/// first to the last, the newest first, a month with none included.
pub fn fold(rows: &[StatRow]) -> Figures {
    use std::collections::BTreeMap;
    let mut games: BTreeMap<String, (String, i64, f64, i64)> = BTreeMap::new();
    let mut months: BTreeMap<i64, i64> = BTreeMap::new();
    let (mut clips, mut seconds, mut bytes) = (0i64, 0f64, 0i64);
    for r in rows {
        let g = games
            .entry(r.game.to_lowercase())
            .or_insert_with(|| (r.game.clone(), 0, 0.0, 0));
        g.1 += r.clips;
        g.2 += r.seconds;
        g.3 += r.bytes;
        if let Some(m) = month_index(&r.month) {
            *months.entry(m).or_default() += r.clips;
        }
        clips += r.clips;
        seconds += r.seconds;
        bytes += r.bytes;
    }
    let most = games.values().map(|g| g.1).max().unwrap_or(0);
    let mut games: Vec<GameLine> = games
        .into_iter()
        .map(|(key, (name, n, s, b))| GameLine {
            name: if key.is_empty() {
                "No game".into()
            } else {
                name
            },
            key,
            clips: n,
            length: format::duration(s),
            size: format::size(b.max(0) as u64),
            bar: bar(n, most),
        })
        .collect();
    games.sort_by(|a, b| {
        b.clips
            .cmp(&a.clips)
            .then_with(|| a.key.is_empty().cmp(&b.key.is_empty()))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let mut all = Vec::new();
    let mut before: Option<i64> = None;
    for (&m, &n) in &months {
        if let Some(b) = before.filter(|b| m - b - 1 <= GAP) {
            all.extend((b + 1..m).map(|e| (e, 0)));
        }
        all.push((m, n));
        before = Some(m);
    }
    let busiest = all.iter().map(|(_, n)| *n).max().unwrap_or(0);
    let months = all
        .into_iter()
        .rev()
        .map(|(m, n)| MonthLine {
            label: month_label(m),
            clips: n,
            bar: bar(n, busiest),
        })
        .collect();
    Figures {
        clips,
        length: format::duration(seconds),
        size: format::size(bytes.max(0) as u64),
        games,
        months,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(game: &str, month: &str, clips: i64) -> StatRow {
        StatRow {
            game: game.into(),
            month: month.into(),
            clips,
            seconds: clips as f64 * 30.0,
            bytes: clips * 10_000_000,
        }
    }

    #[test]
    fn games_and_months_from_the_rows() {
        let f = fold(&[
            row("Valorant", "2026-09", 3),
            row("valorant", "2026-11", 1),
            row("", "2026-09", 2),
            row("Overwatch 2", "2026-11", 6),
        ]);
        assert_eq!(f.clips, 12);
        assert_eq!(f.size, format::size(120_000_000));
        let names: Vec<(&str, i64, u32)> = f
            .games
            .iter()
            .map(|g| (g.name.as_str(), g.clips, g.bar))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Overwatch 2", 6, 100),
                ("Valorant", 4, 67),
                ("No game", 2, 33)
            ],
            "the most first, a game in two spellings once, no game last"
        );
        assert_eq!(f.games[1].key, "valorant", "the library's filter key");
        let months: Vec<(&str, i64)> = f
            .months
            .iter()
            .map(|m| (m.label.as_str(), m.clips))
            .collect();
        assert_eq!(
            months,
            vec![("Nov 2026", 7), ("Oct 2026", 0), ("Sep 2026", 5)],
            "newest first, a month with none kept"
        );
    }

    #[test]
    fn a_far_date_lists_no_empty_years() {
        let f = fold(&[
            row("A", "1980-01", 1),
            row("A", "2026-10", 2),
            row("A", "2026-11", 1),
            row("A", "9999-12", 1),
            row("A", "bogus", 4),
        ]);
        let labels: Vec<&str> = f.months.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, vec!["Dec 9999", "Nov 2026", "Oct 2026", "Jan 1980"]);
        assert_eq!(f.clips, 9, "a clip with no month is still counted");
        let gap = fold(&[row("A", "2024-01", 1), row("A", "2026-01", 1)]);
        assert_eq!(
            gap.months.len(),
            25,
            "two years apart: the 23 months between are listed"
        );
        assert_eq!(month_index("10000-01"), None);
        assert_eq!(month_index("2026-13"), None);
    }

    #[test]
    fn a_year_turns_and_nothing_is_nothing() {
        let f = fold(&[row("A", "2025-12", 1), row("A", "2026-01", 1)]);
        assert_eq!(f.months.len(), 2);
        assert_eq!(f.months[0].label, "Jan 2026");
        let none = fold(&[]);
        assert_eq!((none.clips, none.games.len(), none.months.len()), (0, 0, 0));
    }
}
