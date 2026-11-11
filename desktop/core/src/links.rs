//! The `reel-desktop://copy/<clip>` and `open/<clip>` links of a Windows notification's buttons,
//! and the update's `install/<value>` and `later/<value>`. A web page can open them too, so
//! anything but exactly those forms is refused, and the update's carry a value of this run's.

pub const SCHEME: &str = "reel-desktop";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    Copy(String),
    Open(String),
    /// The update's Install now, with the value the run that asked made.
    Install(String),
    Later(String),
}

impl Link {
    pub fn url(&self) -> String {
        match self {
            Link::Copy(id) => format!("{SCHEME}://copy/{id}"),
            Link::Open(id) => format!("{SCHEME}://open/{id}"),
            Link::Install(value) => format!("{SCHEME}://install/{value}"),
            Link::Later(value) => format!("{SCHEME}://later/{value}"),
        }
    }

    pub fn clip(&self) -> Option<&str> {
        match self {
            Link::Copy(id) | Link::Open(id) => Some(id),
            Link::Install(_) | Link::Later(_) => None,
        }
    }
}

/// What a start's arguments ask for; `Refused` is an argument in our scheme that is no link.
#[derive(Debug, PartialEq, Eq)]
pub enum Asked {
    Link(Link),
    Refused(String),
    Nothing,
}

/// A ULID: 26 characters of Crockford's base 32 in capitals.
pub fn is_clip_id(s: &str) -> bool {
    s.len() == 26
        && s.as_bytes()[0] <= b'7'
        && s.bytes().all(|b| {
            matches!(b, b'0'..=b'9' | b'A'..=b'H' | b'J' | b'K' | b'M' | b'N' | b'P'..=b'T' | b'V'..=b'Z')
        })
}

/// A run's value in the update's links: 128 bits as 22 characters of base64url.
pub fn is_run_value(s: &str) -> bool {
    reel_core::ids::looks_like_token(s)
}

/// The scheme in any case (as a browser may write it) and one trailing slash (as Windows may
/// add) are allowed; no query, no other verb, no other id.
pub fn parse(arg: &str) -> Option<Link> {
    let head = format!("{SCHEME}://");
    if !arg
        .get(..head.len())
        .is_some_and(|h| h.eq_ignore_ascii_case(&head))
    {
        return None;
    }
    let rest = &arg[head.len()..];
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let (verb, id) = rest.split_once('/')?;
    match verb {
        "copy" if is_clip_id(id) => Some(Link::Copy(id.to_string())),
        "open" if is_clip_id(id) => Some(Link::Open(id.to_string())),
        "install" if is_run_value(id) => Some(Link::Install(id.to_string())),
        "later" if is_run_value(id) => Some(Link::Later(id.to_string())),
        _ => None,
    }
}

pub fn from_args(args: &[String]) -> Asked {
    let ours = |a: &&String| {
        a.get(..SCHEME.len() + 1)
            .is_some_and(|h| h.eq_ignore_ascii_case(&format!("{SCHEME}:")))
    };
    match args.iter().skip(1).find(ours) {
        Some(a) => match parse(a) {
            Some(link) => Asked::Link(link),
            None => Asked::Refused(a.chars().take(120).collect()),
        },
        None => Asked::Nothing,
    }
}

/// A notification of Windows that shows and makes no sound: reel's own cues are its sounds.
pub fn plain_toast_xml(title: &str, body: &str) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual>{SILENT}</toast>",
        escape(title),
        escape(body)
    )
}

const SILENT: &str = "<audio silent=\"true\"/>";

/// The buttons use protocol activation, so they reach the app from the notification centre
/// after the toast has gone, or when the app is not running.
pub fn toast_xml(title: &str, body: &str, clip: &str, copy: bool) -> String {
    let open = Link::Open(clip.to_string()).url();
    let copy = if copy {
        format!(
            "<action content=\"Copy link\" activationType=\"protocol\" arguments=\"{}\"/>",
            escape(&Link::Copy(clip.to_string()).url())
        )
    } else {
        String::new()
    };
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual>\
         <actions>{copy}<action content=\"Open\" activationType=\"protocol\" arguments=\"{}\"/></actions>{SILENT}</toast>",
        escape(title),
        escape(body),
        escape(&open)
    )
}

/// The update's question: Install now and Later as links with this run's value.
pub fn ask_toast_xml(title: &str, body: &str, value: &str) -> String {
    let button = |label: &str, link: Link| {
        format!(
            "<action content=\"{label}\" activationType=\"protocol\" arguments=\"{}\"/>",
            escape(&link.url())
        )
    };
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual>\
         <actions>{}{}</actions>{SILENT}</toast>",
        escape(title),
        escape(body),
        button("Install now", Link::Install(value.to_string())),
        button("Later", Link::Later(value.to_string())),
    )
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "01M6BCN0TDHAX3FE17KH9KW306";

    #[test]
    fn the_two_links_and_back() {
        let copy = format!("reel-desktop://copy/{ID}");
        let open = format!("reel-desktop://open/{ID}");
        assert_eq!(parse(&copy), Some(Link::Copy(ID.into())));
        assert_eq!(parse(&open), Some(Link::Open(ID.into())));
        assert_eq!(Link::Copy(ID.into()).url(), copy);
        assert_eq!(Link::Open(ID.into()).url(), open);
        assert_eq!(
            parse(&format!("REEL-DESKTOP://open/{ID}/")),
            Some(Link::Open(ID.into())),
            "the scheme in capitals and a slash at the end, as a browser or Windows may write it"
        );
    }

    const RUN: &str = "q0Xw-3_mT9cJb1LkZp4aRg";

    #[test]
    fn the_updates_two_links_and_back() {
        let install = format!("reel-desktop://install/{RUN}");
        let later = format!("reel-desktop://later/{RUN}");
        assert_eq!(parse(&install), Some(Link::Install(RUN.into())));
        assert_eq!(parse(&later), Some(Link::Later(RUN.into())));
        assert_eq!(Link::Install(RUN.into()).url(), install);
        assert_eq!(Link::Later(RUN.into()).url(), later);
        assert_eq!(
            parse(&format!("Reel-Desktop://install/{RUN}/")),
            Some(Link::Install(RUN.into()))
        );
        assert_eq!(Link::Install(RUN.into()).clip(), None);
        assert_eq!(Link::Open(ID.into()).clip(), Some(ID));
        for bad in [
            "reel-desktop://install".to_string(),
            "reel-desktop://install/".into(),
            format!("reel-desktop://install/{ID}"),
            format!("reel-desktop://install/{RUN}x"),
            format!("reel-desktop://install/{}", &RUN[1..]),
            format!("reel-desktop://install/{RUN}?now=1"),
            format!("reel-desktop://install/{RUN}/again"),
            format!("reel-desktop://INSTALL/{RUN}"),
            format!("reel-desktop://update/{RUN}"),
            format!("reel-desktop://install/{}", RUN.replace('-', "+")),
            format!("reel-desktop://later/{}", RUN.replace('_', "/")),
            format!("reel-desktop://open/{RUN}"),
        ] {
            assert_eq!(parse(&bad), None, "{bad}");
        }
    }

    #[test]
    fn the_updates_toast_asks_with_install_now_and_later() {
        let xml = ask_toast_xml(
            "reel 1.14.1 is ready",
            "Install now closes reel & opens it again",
            RUN,
        );
        assert!(xml.contains(&format!(
            r#"<actions><action content="Install now" activationType="protocol" arguments="reel-desktop://install/{RUN}"/><action content="Later" activationType="protocol" arguments="reel-desktop://later/{RUN}"/></actions>"#
        )), "{xml}");
        assert!(xml.contains("<text>reel 1.14.1 is ready</text><text>Install now closes reel &amp; opens it again</text>"));
        assert!(xml.ends_with(r#"<audio silent="true"/></toast>"#), "{xml}");
        assert_eq!(xml.matches('<').count(), xml.matches('>').count());
    }

    #[test]
    fn anything_else_is_no_link() {
        for bad in [
            format!("reel-desktop://delete/{ID}"),
            format!("reel-desktop://upload/{ID}"),
            format!("reel-desktop://COPY/{ID}"),
            format!("reel-desktop://copy/{ID}?x=1"),
            format!("reel-desktop://copy/{ID}#x"),
            format!("reel-desktop://copy/{ID}//"),
            format!("reel-desktop://copy/{ID}/more"),
            format!("reel-desktop://copy/{}", ID.to_lowercase()),
            format!("reel-desktop://copy/../{ID}"),
            format!("reel-desktop:copy/{ID}"),
            format!("reel-desktop://copy//{ID}"),
            format!("reel-desktop://open/{ID}0"),
            "reel-desktop://open/01M6BCN0TDHAX3FE17KH9KW30I".into(),
            "reel-desktop://open/81M6BCN0TDHAX3FE17KH9KW306".into(),
            "reel-desktop://open/".into(),
            "reel-desktop://".into(),
            format!("https://copy/{ID}"),
            format!("reel-desktop://open/{ID} --quit-for-install"),
            format!("reel-desktop://open/{ID}%2F"),
            "réel-desktop://open/x".into(),
        ] {
            assert_eq!(parse(&bad), None, "{bad}");
        }
    }

    #[test]
    fn the_toast_carries_both_links_and_escapes_the_words() {
        let xml = toast_xml(
            "Clip uploaded",
            "Me & <you> \"ace\"\u{7}. Ctrl+Alt+L copies",
            ID,
            true,
        );
        assert!(xml.contains(&format!(
            r#"<action content="Copy link" activationType="protocol" arguments="reel-desktop://copy/{ID}"/>"#
        )));
        assert!(xml.contains(&format!(
            r#"<action content="Open" activationType="protocol" arguments="reel-desktop://open/{ID}"/>"#
        )));
        assert!(
            xml.contains("<text>Me &amp; &lt;you&gt; &quot;ace&quot;. Ctrl+Alt+L copies</text>")
        );
        assert_eq!(xml.matches('<').count(), xml.matches('>').count());
        let here = toast_xml("Clip in the library", "A clip", ID, false);
        assert!(!here.contains("Copy link") && !here.contains("reel-desktop://copy/"));
        assert!(here.contains(&format!(
            r#"<actions><action content="Open" activationType="protocol" arguments="reel-desktop://open/{ID}"/></actions>"#
        )));
    }

    #[test]
    fn every_toast_is_silent_as_reels_cues_are_its_sounds() {
        for xml in [
            toast_xml("Clip uploaded", "A clip", ID, true),
            toast_xml("Clip in the library", "A clip", ID, false),
            plain_toast_xml("Link copied", "Me & <you>"),
        ] {
            assert!(xml.ends_with(r#"<audio silent="true"/></toast>"#), "{xml}");
            assert_eq!(xml.matches("<audio").count(), 1, "{xml}");
            assert!(!xml.contains("src="), "{xml}");
            assert_eq!(xml.matches('<').count(), xml.matches('>').count());
        }
        assert!(plain_toast_xml("Link copied", "Me & <you>")
            .contains("<text>Link copied</text><text>Me &amp; &lt;you&gt;</text>"));
    }

    #[test]
    fn a_starts_arguments() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            from_args(&args(&[
                "reel-desktop.exe",
                &format!("reel-desktop://copy/{ID}")
            ])),
            Asked::Link(Link::Copy(ID.into()))
        );
        assert_eq!(
            from_args(&args(&[
                "reel-desktop.exe",
                "Reel-Desktop://wipe/everything"
            ])),
            Asked::Refused("Reel-Desktop://wipe/everything".into())
        );
        assert_eq!(
            from_args(&args(&["reel-desktop.exe", "--hidden"])),
            Asked::Nothing
        );
        assert_eq!(
            from_args(&args(&[&format!("reel-desktop://copy/{ID}")])),
            Asked::Nothing,
            "the program's own path is not an argument"
        );
    }
}
