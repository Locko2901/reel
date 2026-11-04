use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::Timestamp;
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    pub title: String,
    pub recorded_at: Option<DateTime>,
    /// The game, when the name carries one (Medal's and ShadowPlay's do).
    pub game: Option<String>,
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).unwrap())
}

fn shadowplay() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(
        &RE,
        r"^(?P<game>.+?) (?P<y>\d{4})\.(?P<m>\d{2})\.(?P<d>\d{2}) - (?P<H>\d{2})\.(?P<M>\d{2})\.(?P<S>\d{2})(?:\.\d+)?(?:\.DVR)?$",
    )
}

fn obs() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(
        &RE,
        r"^(?:Replay[ _])?(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})[ _T](?P<H>\d{2})-(?P<M>\d{2})-(?P<S>\d{2})$",
    )
}

/// `MedalTV<Game><YYYYMMDDHHMMSS>[ms]`: the game may end in a digit (Overwatch2), so the
/// trailing digits are split where a plausible date begins, the game taking the rest.
fn medal_parts(stem: &str) -> Option<(String, DateTime)> {
    let rest = stem.strip_prefix("MedalTV")?;
    let letters = rest.trim_end_matches(|c: char| c.is_ascii_digit());
    let digits = &rest[letters.len()..];
    for k in 0..=3.min(digits.len()) {
        let stamp = &digits[k..];
        if stamp.len() < 14 {
            break;
        }
        if let Some(d) = stamp_date(&stamp[..14]) {
            let game = format!("{letters}{}", &digits[..k]);
            let game = game.trim();
            return Some((game.to_string(), d));
        }
    }
    None
}

/// `YYYYMMDDHHMMSS` as a date within this century.
fn stamp_date(s: &str) -> Option<DateTime> {
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i16>().ok();
    let d = DateTime::new(
        n(0, 4)?,
        n(4, 6)? as i8,
        n(6, 8)? as i8,
        n(8, 10)? as i8,
        n(10, 12)? as i8,
        n(12, 14)? as i8,
        0,
    )
    .ok()?;
    (2010..=2100).contains(&d.year()).then_some(d)
}

fn anywhere() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(
        &RE,
        r"(?P<y>\d{4})[-._](?P<m>\d{2})[-._](?P<d>\d{2})[ _T-]+(?P<H>\d{2})[-._:](?P<M>\d{2})[-._:](?P<S>\d{2})",
    )
}

fn compact() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(
        &RE,
        r"(?:^|[^\d])(?P<y>20\d{2})(?P<m>\d{2})(?P<d>\d{2})[-_T]?(?P<H>\d{2})(?P<M>\d{2})(?P<S>\d{2})(?:[^\d]|$)",
    )
}

fn unix_millis() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    re(&RE, r"(?:^|[^\d])(?P<ms>\d{13})(?:[^\d]|$)")
}

fn date_of(c: &regex::Captures) -> Option<DateTime> {
    let n = |k: &str| c.name(k).and_then(|m| m.as_str().parse::<i16>().ok());
    DateTime::new(
        n("y")?,
        n("m")? as i8,
        n("d")? as i8,
        n("H")? as i8,
        n("M")? as i8,
        n("S")? as i8,
        0,
    )
    .ok()
}

/// A 13-digit Unix millisecond stamp, taken as a date only between 2010 and a day from now.
fn millis_date(c: &regex::Captures) -> Option<DateTime> {
    let ms: i64 = c.name("ms")?.as_str().parse().ok()?;
    let ts = Timestamp::from_millisecond(ms).ok()?;
    let low = Timestamp::from_second(1_262_304_000).ok()?;
    let high = Timestamp::now()
        .checked_add(jiff::Span::new().hours(24))
        .ok()?;
    if ts < low || ts > high {
        return None;
    }
    Some(ts.to_zoned(jiff::tz::TimeZone::system()).datetime())
}

/// The game as a recorder writes it into a name, as words: a few names are mapped, the rest
/// split at a capital after a small letter and between letters and digits (Left4Dead).
pub fn game_word(raw: &str) -> String {
    let raw = raw.trim();
    let mapped = match raw {
        "BeatSaber" => "Beat Saber",
        "Overwatch2" => "Overwatch 2",
        "LordsoftheFallen" => "Lords of the Fallen",
        "VRChat" => "VRChat",
        _ => "",
    };
    if !mapped.is_empty() {
        return mapped.to_string();
    }
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 {
            let p = chars[i - 1];
            let boundary = (c.is_uppercase() && p.is_lowercase())
                || (c.is_ascii_digit() && p.is_alphabetic())
                || (c.is_alphabetic() && p.is_ascii_digit());
            if boundary && p != ' ' {
                out.push(' ');
            }
        }
        out.push(*c);
    }
    out
}

/// The year the pages count as the current one.
pub fn this_year() -> i16 {
    Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::system())
        .year()
}

/// The moment a title carries: `Oct 8, 16:33`, with the year when it is not the current one.
pub fn time_title_in(d: &DateTime, now_year: i16) -> String {
    if d.year() == now_year {
        d.strftime("%b %-d, %H:%M").to_string()
    } else {
        d.strftime("%b %-d, %Y, %H:%M").to_string()
    }
}

pub fn time_title(d: &DateTime) -> String {
    time_title_in(d, this_year())
}

/// A clip's title from its game and moment (`VRChat, Apr 18, 03:35`), or the moment alone.
pub fn title_for(game: Option<&str>, d: &DateTime, now_year: i16) -> String {
    match game.map(str::trim).filter(|g| !g.is_empty()) {
        Some(g) => format!("{g}, {}", time_title_in(d, now_year)),
        None => time_title_in(d, now_year),
    }
}

/// Whether the title the parser gave a name is one it made from the name's moment
/// (`VRChat, Apr 18, 03:35`), rather than words the name carried.
pub fn made_from_moment(p: &Parsed) -> bool {
    p.recorded_at
        .is_some_and(|d| p.title == title_for(p.game.as_deref(), &d, this_year()))
}

/// The title a clip gets from its recording moment as the pages show it, the wall clock
/// of `zone`: the same moment the meta line says.
pub fn title_at_in(game: Option<&str>, at: Timestamp, zone: &TimeZone, now_year: i16) -> String {
    title_for(game, &at.to_zoned(zone.clone()).datetime(), now_year)
}

/// `title_at_in` for a stored moment, in the pages' zone.
pub fn title_at(game: Option<&str>, recorded_at: &str) -> Option<String> {
    let at = crate::format::parse(recorded_at)?;
    Some(title_at_in(
        game,
        at,
        &crate::format::timezone(),
        this_year(),
    ))
}

/// A title made from `at` as UTC's wall clock (the moment alone or after one of `games`), made
/// again in `zone`; None for any other title, and for one that reads the same there.
pub fn utc_title_in(
    title: &str,
    games: &[&str],
    at: Timestamp,
    zone: &TimeZone,
    now_year: i16,
) -> Option<String> {
    let utc = at.to_zoned(TimeZone::UTC).datetime();
    let moments = [
        time_title_in(&utc, utc.year()),
        time_title_in(&utc, utc.year() + 1),
    ];
    let game = moments.iter().find_map(|m| {
        if title == m.as_str() {
            return Some(None);
        }
        let prefix = title.strip_suffix(m.as_str())?.strip_suffix(", ")?;
        games
            .iter()
            .any(|g| !g.trim().is_empty() && g.trim().eq_ignore_ascii_case(prefix.trim()))
            .then_some(Some(prefix))
    })?;
    let remade = title_at_in(game, at, zone, now_year);
    (remade != title).then_some(remade)
}

/// The title of a clip named after an app that is no game, with that name dropped, when the
/// title is exactly the name and the moment the server made of it; any other title stays.
pub fn without_not_a_game(title: &str, d: &DateTime) -> Option<String> {
    let (name, moment) = title.split_once(", ")?;
    if !crate::games::drops_from_title(name) {
        return None;
    }
    [time_title_in(d, d.year()), time_title_in(d, d.year() + 1)]
        .contains(&moment.to_string())
        .then(|| moment.to_string())
}

/// Whether what a name holds besides its date is nothing or a short tag (-clp, -ses), so
/// the name says only when; words beside the date are a title and stay.
fn only_a_tag(stem: &str, m: &regex::Match) -> bool {
    let rest = format!("{}{}", &stem[..m.start()], &stem[m.end()..]);
    let letters: String = rest.chars().filter(|c| c.is_alphanumeric()).collect();
    letters.is_empty() || (letters.len() <= 4 && letters.chars().all(|c| c.is_ascii_alphabetic()))
}

pub fn parse_name(file_name: &str) -> Parsed {
    parse_name_at(file_name, this_year())
}

/// `parse_name` with the current year given (the tests fix it).
pub fn parse_name_at(file_name: &str, now_year: i16) -> Parsed {
    let stem = file_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(file_name);
    let stem = stem.trim();
    let dated = |game: Option<String>, d: DateTime| {
        let named = game.filter(|g| !crate::games::drops_from_title(g));
        Parsed {
            title: title_for(named.as_deref(), &d, now_year),
            recorded_at: Some(d),
            game: crate::games::clean(named.as_deref()),
        }
    };
    if let Some(c) = shadowplay().captures(stem) {
        if let Some(d) = date_of(&c) {
            return dated(Some(game_word(&c["game"])), d);
        }
    }
    if let Some(c) = obs().captures(stem) {
        if let Some(d) = date_of(&c) {
            return dated(None, d);
        }
    }
    if let Some((game, d)) = medal_parts(stem) {
        let game = if game.is_empty() {
            None
        } else {
            Some(game_word(&game))
        };
        return dated(game, d);
    }
    for re in [anywhere(), compact()] {
        if let Some(c) = re.captures(stem) {
            if let Some(d) = date_of(&c) {
                if only_a_tag(stem, &c.get(0).expect("the whole match")) {
                    return dated(None, d);
                }
                return Parsed {
                    title: stem.to_string(),
                    recorded_at: Some(d),
                    game: None,
                };
            }
        }
    }
    // A Unix millisecond stamp names the moment the file was made, which for a trimmed
    // download is the recording as near as the name says; the words around it say nothing.
    if let Some(d) = unix_millis().captures(stem).and_then(|c| millis_date(&c)) {
        return dated(None, d);
    }
    Parsed {
        title: if stem.is_empty() {
            "untitled".into()
        } else {
            stem.to_string()
        },
        recorded_at: None,
        game: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadowplay_names() {
        let p = parse_name_at("Overwatch 2 2026.10.07 - 21.14.03.02.DVR.mp4", 2026);
        assert_eq!(p.title, "Overwatch 2, Oct 7, 21:14");
        assert_eq!(p.game.as_deref(), Some("Overwatch 2"));
        assert_eq!(p.recorded_at.unwrap().to_string(), "2026-10-07T21:14:03");
        let p = parse_name_at("Desktop 2026.10.05 - 23.41.09.mkv", 2026);
        assert_eq!(p.title, "Desktop, Oct 5, 23:41");
        assert!(p.game.is_none(), "a desktop recording is no game");
        assert!(p.recorded_at.is_some());
    }

    #[test]
    fn an_app_that_is_no_game_leaves_the_moment_alone() {
        let p = parse_name_at("Zen 2026.10.10 - 21.51.00.DVR.mp4", 2026);
        assert_eq!(p.title, "Oct 10, 21:51");
        assert!(p.game.is_none());
        let d: DateTime = "2026-10-10T21:51:00".parse().unwrap();
        assert_eq!(
            without_not_a_game("Zen, Oct 10, 21:51", &d).as_deref(),
            Some("Oct 10, 21:51")
        );
        let last_year: DateTime = "2025-10-10T21:51:00".parse().unwrap();
        assert_eq!(
            without_not_a_game("Zen, Oct 10, 2025, 21:51", &last_year).as_deref(),
            Some("Oct 10, 2025, 21:51"),
            "made in another year, with the year"
        );
        assert_eq!(
            without_not_a_game("Zen, Oct 10, 21:51", &last_year).as_deref(),
            Some("Oct 10, 21:51"),
            "made in its own year, without"
        );
        for kept in [
            "Zen, Oct 10, 21:52",
            "Zen, Oct 10, 21:51, the clutch",
            "Valorant, Oct 10, 21:51",
            "Desktop, Oct 10, 21:51",
            "Oct 10, 21:51",
            "zen and me, Oct 10, 21:51",
        ] {
            assert_eq!(without_not_a_game(kept, &d), None, "{kept}");
        }
    }

    #[test]
    fn obs_names_plain_and_replay() {
        let p = parse_name_at("2026-10-06 21-14-03.mkv", 2026);
        assert_eq!(p.title, "Oct 6, 21:14");
        assert_eq!(p.recorded_at.unwrap().to_string(), "2026-10-06T21:14:03");
        let p = parse_name_at("Replay_2026-10-06_21-14-03.mkv", 2026);
        assert_eq!(p.title, "Oct 6, 21:14");
        assert_eq!(p.recorded_at.unwrap().to_string(), "2026-10-06T21:14:03");
        let p = parse_name_at("Replay 2026-10-06 21-14-03.mkv", 2026);
        assert_eq!(p.recorded_at.unwrap().to_string(), "2026-10-06T21:14:03");
    }

    #[test]
    fn medal_names() {
        let p = parse_name_at("MedalTVValorant20261006211403.mp4", 2026);
        assert_eq!(p.title, "Valorant, Oct 6, 21:14");
        assert_eq!(p.game.as_deref(), Some("Valorant"));
        assert_eq!(p.recorded_at.unwrap().to_string(), "2026-10-06T21:14:03");
        let p = parse_name_at("MedalTVRocket League2026100621140312345.mp4", 2026);
        assert_eq!(p.title, "Rocket League, Oct 6, 21:14");
        assert_eq!(p.recorded_at.unwrap().to_string(), "2026-10-06T21:14:03");
        let p = parse_name_at("MedalTV20261006211403.mp4", 2026);
        assert_eq!(p.title, "Oct 6, 21:14");
        assert!(p.game.is_none());
    }

    #[test]
    fn stamps_of_several_recorders() {
        let p = parse_name_at("2025_03_14_20_15_30-clp.mp4", 2026);
        assert_eq!(
            p.recorded_at.unwrap().to_string(),
            "2025-03-14T20:15:30",
            "the -clp form with underscores"
        );
        assert_eq!(p.title, "Mar 14, 2025, 20:15");
        let p = parse_name_at("2025-03-14 20-16-05-ses.mkv", 2026);
        assert_eq!(
            p.recorded_at.unwrap().to_string(),
            "2025-03-14T20:16:05",
            "the -ses form with dashes and a space"
        );
        let p = parse_name_at("MedalTVValorant20260314201530123.mp4", 2026);
        assert_eq!(
            p.recorded_at.unwrap().to_string(),
            "2026-03-14T20:15:30",
            "Medal with milliseconds"
        );
        assert_eq!(p.title, "Valorant, Mar 14, 20:15");
        let p = parse_name_at("MedalTVVRChat20240314201530.mkv", 2026);
        assert_eq!(
            p.recorded_at.unwrap().to_string(),
            "2024-03-14T20:15:30",
            "Medal without milliseconds"
        );
        assert_eq!(p.title, "VRChat, Mar 14, 2024, 20:15");
        let p = parse_name_at("Replay_2026-03-14_20-15-30.mkv", 2026);
        assert_eq!(
            p.recorded_at.unwrap().to_string(),
            "2026-03-14T20:15:30",
            "OBS replay"
        );
        assert_eq!(p.title, "Mar 14, 20:15");
        let p = parse_name_at("downloaded-clip-trim-1773519330141.mp4", 2026);
        let expected = Timestamp::from_millisecond(1_773_519_330_141)
            .unwrap()
            .to_zoned(jiff::tz::TimeZone::system())
            .datetime();
        assert_eq!(
            p.recorded_at,
            Some(expected),
            "a 13-digit Unix millisecond stamp"
        );
        assert_eq!(p.title, time_title_in(&expected, 2026));
    }

    #[test]
    fn seven_names_get_their_titles() {
        let want = [
            ("2025_05_02_19_30_00-clp.mp4", "May 2, 2025, 19:30", None),
            (
                "MedalTVVRChat20260502193000123.mp4",
                "VRChat, May 2, 19:30",
                Some("VRChat"),
            ),
            ("2025-05-02 19-31-00-ses.mp4", "May 2, 2025, 19:31", None),
            (
                "MedalTVBeatSaber20240502193200.mp4",
                "Beat Saber, May 2, 2024, 19:32",
                Some("Beat Saber"),
            ),
            ("Replay_2026-05-02_19-33-00.mp4", "May 2, 19:33", None),
            (
                "MedalTVOverwatch220240502193400.mp4",
                "Overwatch 2, May 2, 2024, 19:34",
                Some("Overwatch 2"),
            ),
        ];
        for (name, title, game) in want {
            let p = parse_name_at(name, 2026);
            assert_eq!(p.title, title, "{name}");
            assert_eq!(p.game.as_deref(), game, "{name}");
            assert!(p.recorded_at.is_some(), "{name}");
        }
        let p = parse_name_at("downloaded-clip-trim-1777750500141.mp4", 2026);
        let at = Timestamp::from_millisecond(1_777_750_500_141)
            .unwrap()
            .to_zoned(jiff::tz::TimeZone::system())
            .datetime();
        assert_eq!(
            p.recorded_at,
            Some(at),
            "the stamp is the moment the file was made"
        );
        assert_eq!(p.title, time_title_in(&at, 2026));
        assert!(p.game.is_none());
        assert_eq!(game_word("RocketLeague"), "Rocket League");
        assert_eq!(game_word("Left4Dead2"), "Left 4 Dead 2");
        assert_eq!(game_word("LordsoftheFallen"), "Lords of the Fallen");
        assert_eq!(time_title_in(&at, 2025).matches(", 2026, ").count(), 1);
    }

    #[test]
    fn unix_stamps_outside_a_plausible_range_are_not_dates() {
        assert!(
            parse_name_at("clip-9999999999999.mp4", 2026)
                .recorded_at
                .is_none(),
            "year 2286"
        );
        assert!(
            parse_name_at("clip-1000000000000.mp4", 2026)
                .recorded_at
                .is_none(),
            "2001, before any of these recorders"
        );
        assert!(
            parse_name_at("clip-177600224414.mp4", 2026)
                .recorded_at
                .is_none(),
            "12 digits is not the stamp"
        );
        assert!(
            parse_name_at("clip-17760022441410.mp4", 2026)
                .recorded_at
                .is_none(),
            "14 digits is not the stamp"
        );
    }

    #[test]
    fn compact_stamps_and_impossible_dates() {
        let p = parse_name_at("overwatch-clp-20261006-211403.mp4", 2026);
        assert_eq!(p.recorded_at.unwrap().to_string(), "2026-10-06T21:14:03");
        let p = parse_name_at("valorant-ses-20261006211403.mkv", 2026);
        assert_eq!(p.recorded_at.unwrap().to_string(), "2026-10-06T21:14:03");
        let p = parse_name_at("clip-99991399-999999.mp4", 2026);
        assert!(p.recorded_at.is_none(), "an impossible date is not a date");
    }

    #[test]
    fn other_names_keep_the_stem() {
        let p = parse_name_at("the bridge jump.mp4", 2026);
        assert_eq!(p.title, "the bridge jump");
        assert!(p.recorded_at.is_none());
        let p = parse_name_at("Medal_2026-10-01_18-22-10_clip.mp4", 2026);
        assert_eq!(p.title, "Medal_2026-10-01_18-22-10_clip");
        assert!(p.recorded_at.is_some());
    }

    fn berlin() -> TimeZone {
        TimeZone::get("Europe/Berlin").unwrap()
    }

    fn at(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn a_title_from_a_moment_says_the_pages_wall_clock() {
        let t = at("2026-07-14T19:59:00Z");
        assert_eq!(
            title_at_in(Some("Ember Pass"), t, &berlin(), 2026),
            "Ember Pass, Jul 14, 21:59",
            "the meta line's 21:59 in Berlin, not UTC's 19:59"
        );
        assert_eq!(title_at_in(None, t, &TimeZone::UTC, 2026), "Jul 14, 19:59");
        assert_eq!(
            title_at_in(None, at("2026-07-14T23:30:00Z"), &berlin(), 2026),
            "Jul 15, 01:30",
            "past midnight there, the next day"
        );
        assert!(made_from_moment(&parse_name(
            "Ember Pass 2026.07.14 - 19.59.00.DVR.mp4"
        )));
        assert!(made_from_moment(&parse_name(
            "Replay 2026-07-14 19-59-00.mkv"
        )));
        assert!(!made_from_moment(&parse_name(
            "the bridge jump 2026-07-14 19-59-00.mp4"
        )));
        assert!(!made_from_moment(&parse_name("the bridge jump.mp4")));
    }

    #[test]
    fn only_a_title_made_in_utc_is_made_again() {
        let t = at("2026-07-14T19:59:00Z");
        let z = berlin();
        let again = |title: &str, games: &[&str]| utc_title_in(title, games, t, &z, 2026);
        assert_eq!(
            again("Ember Pass, Jul 14, 19:59", &["Ember Pass"]).as_deref(),
            Some("Ember Pass, Jul 14, 21:59")
        );
        assert_eq!(
            again("Jul 14, 19:59", &[]).as_deref(),
            Some("Jul 14, 21:59"),
            "the moment alone"
        );
        assert_eq!(
            again("Ember Pass, Jul 14, 19:59", &["ember pass"]).as_deref(),
            Some("Ember Pass, Jul 14, 21:59"),
            "the game as the title spells it"
        );
        assert_eq!(
            again("Ember Pass, Jul 14, 2026, 19:59", &["Ember Pass"]).as_deref(),
            Some("Ember Pass, Jul 14, 21:59"),
            "made in another year, with the year"
        );
        assert_eq!(
            again("Ember Pass, Jul 14, 21:59", &["Ember Pass"]),
            None,
            "made in the pages' zone already"
        );
        assert_eq!(
            again("Ember Pass, Jul 14, 19:59, the clutch", &["Ember Pass"]),
            None,
            "words someone added to the title"
        );
        assert_eq!(
            again("The clutch, Jul 14, 19:59", &["Ember Pass"]),
            None,
            "not the clip's game before the moment"
        );
        assert_eq!(
            again("Ember Pass, Jul 14, 19:58", &["Ember Pass"]),
            None,
            "another moment"
        );
        assert_eq!(
            utc_title_in("Jul 14, 19:59", &[], t, &TimeZone::UTC, 2026),
            None,
            "the pages in UTC: nothing to do"
        );
        let remade = again("Ember Pass, Jul 14, 19:59", &["Ember Pass"]).unwrap();
        let shown = time_title_in(&t.to_zoned(z.clone()).datetime(), 2026);
        assert!(
            remade.contains(&shown),
            "the card's meta finds its moment in the title and says it once"
        );
        let old = at("2025-07-14T19:59:00Z");
        assert_eq!(
            utc_title_in(
                "Ember Pass, Jul 14, 2025, 19:59",
                &["Ember Pass"],
                old,
                &z,
                2026
            )
            .as_deref(),
            Some("Ember Pass, Jul 14, 2025, 21:59")
        );
    }
}
