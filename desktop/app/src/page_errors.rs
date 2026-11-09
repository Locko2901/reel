//! A server page's errors as the app's Log takes them: short, without queries, a few a minute.

use std::time::{Duration, Instant};

const PER_MINUTE: u32 = 10;
const MESSAGE_CHARS: usize = 300;
const STACK_LINES: usize = 5;
const LINE_CHARS: usize = 200;
const PATH_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub struct PageError {
    /// One line.
    pub message: String,
    /// The first lines of the stack, joined by " | ".
    pub stack: String,
    pub path: String,
}

/// `origin` (the server's) is cut out of every URL: the stack names the files by their paths.
pub fn shape(message: &str, stack: &str, path: &str, origin: Option<&str>) -> PageError {
    let clean = |s: &str, max: usize| {
        let s = without_queries(s);
        let s = match origin {
            Some(o) if !o.is_empty() => s.replace(o, ""),
            _ => s,
        };
        cut(&s, max)
    };
    let first = message
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("an error without words");
    let message = clean(first, MESSAGE_CHARS);
    let stack = stack
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        // Chromium's stack starts with the message again
        .skip_while(|l| clean(l, MESSAGE_CHARS) == message)
        .take(STACK_LINES)
        .map(|l| clean(l, LINE_CHARS))
        .collect::<Vec<_>>()
        .join(" | ");
    let path = path.split(['?', '#']).next().unwrap_or("");
    let path = if path.starts_with('/') {
        cut(path, PATH_CHARS)
    } else {
        "a page".to_string()
    };
    PageError {
        message,
        stack,
        path,
    }
}

/// A URL's query and fragment can carry a token: each word holding "://" loses them, all but
/// a stack frame's line and column at the end.
fn without_queries(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if word.contains("://") {
            if let Some(at) = word.find(['?', '#']) {
                let kept = line_and_column(&word[at..]).to_string();
                word.truncate(at);
                word.push_str(&kept);
            }
        }
        out.push_str(word);
        word.clear();
    };
    for c in s.chars() {
        if c.is_whitespace() || matches!(c, '(' | ')' | '"' | '\'') {
            flush(&mut word, &mut out);
            out.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// ":13:44" or ":13" at the end of `s`.
fn line_and_column(s: &str) -> &str {
    let mut start = s.len();
    for _ in 0..2 {
        let head = &s[..start];
        let digits = head.len() - head.trim_end_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 || !head[..head.len() - digits].ends_with(':') {
            break;
        }
        start = head.len() - digits - 1;
    }
    &s[start..]
}

/// At most `max` characters, control characters made spaces, so a line stays one line.
fn cut(s: &str, max: usize) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect();
    if s.chars().count() > max {
        out.push_str("...");
    }
    out
}

/// Ten errors a minute go into the Log; the rest are counted and said once the minute is over.
#[derive(Debug, Default)]
pub struct Limit {
    since: Option<Instant>,
    taken: u32,
    left_out: u32,
}

impl Limit {
    /// Whether this error goes into the Log, and how many the minute before left out.
    pub fn take(&mut self, now: Instant) -> (bool, u32) {
        let mut left_out = 0;
        match self.since {
            Some(t) if now.duration_since(t) < Duration::from_secs(60) => {}
            _ => {
                left_out = std::mem::take(&mut self.left_out);
                self.since = Some(now);
                self.taken = 0;
            }
        }
        if self.taken < PER_MINUTE {
            self.taken += 1;
            (true, left_out)
        } else {
            self.left_out += 1;
            (false, left_out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_error_is_one_line_with_a_short_stack_and_no_query() {
        let o = "http://127.0.0.1:4100";
        let e = shape(
            "TypeError: Cannot read properties of null (reading 'toFixed')",
            "TypeError: Cannot read properties of null (reading 'toFixed')\n    at level (http://127.0.0.1:4100/assets/m/js/format.js:13:44)\n    at describe (http://127.0.0.1:4100/assets/m/js/clip.js:285:9)\n    at update (http://127.0.0.1:4100/assets/m/js/clip.js?v=1:306:5)\n    at audioEditor (http://127.0.0.1:4100/assets/m/js/clip.js:334:3)\n    at trimEditor (http://127.0.0.1:4100/assets/m/js/clip.js:507:3)\n    at clip (http://127.0.0.1:4100/assets/m/js/clip.js:804:3)",
            "/clips/01ABC?from=%2F&token=secret#t=3",
            Some(o),
        );
        assert_eq!(
            e.message,
            "TypeError: Cannot read properties of null (reading 'toFixed')"
        );
        assert_eq!(
            e.stack,
            "at level (/assets/m/js/format.js:13:44) | at describe (/assets/m/js/clip.js:285:9) | at update (/assets/m/js/clip.js:306:5) | at audioEditor (/assets/m/js/clip.js:334:3) | at trimEditor (/assets/m/js/clip.js:507:3)",
            "five lines, the message not again, the origin cut"
        );
        assert_eq!(e.path, "/clips/01ABC");
    }

    #[test]
    fn a_url_in_the_words_loses_its_query_and_a_long_message_is_cut() {
        let e = shape(
            "could not load https://elsewhere.example/s/abc?key=secret#x\nsecond line",
            "level@https://elsewhere.example/a.js?token=t:1:2",
            "not a path",
            None,
        );
        assert_eq!(e.message, "could not load https://elsewhere.example/s/abc");
        assert_eq!(e.stack, "level@https://elsewhere.example/a.js:1:2");
        assert_eq!(e.path, "a page");
        let long = shape(&"x".repeat(1000), "", "/", None);
        assert_eq!(long.message.len(), MESSAGE_CHARS + 3);
        assert!(long.message.ends_with("..."));
        assert_eq!(shape("a\u{7}b", "", "/", None).message, "a b");
        assert_eq!(shape("", "", "/", None).message, "an error without words");
    }

    #[test]
    fn ten_a_minute_and_the_rest_counted() {
        let mut l = Limit::default();
        let t = Instant::now();
        for _ in 0..PER_MINUTE {
            assert_eq!(l.take(t), (true, 0));
        }
        assert_eq!(l.take(t + Duration::from_secs(10)), (false, 0));
        assert_eq!(l.take(t + Duration::from_secs(59)), (false, 0));
        assert_eq!(
            l.take(t + Duration::from_secs(61)),
            (true, 2),
            "a new minute says what the last left out"
        );
        assert_eq!(l.take(t + Duration::from_secs(200)), (true, 0));
    }
}
