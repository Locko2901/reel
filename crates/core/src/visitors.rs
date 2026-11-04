//! Who asked for a link's page: an app building a preview, another robot, or a person.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visitor {
    Previewer,
    Robot,
    Person,
}

/// Apps that fetch a page for a link's preview, by a lowercase word of their User-Agent.
/// Signal asks as WhatsApp, iMessage as facebookexternalhit; Viber's fetcher looks like its browser.
pub const PREVIEWERS: &[&str] = &[
    "discordbot",
    "telegrambot",
    "whatsapp",
    "slackbot",
    "slack-imgproxy",
    "twitterbot",
    "facebookexternalhit",
    "facebot",
    "linkedinbot",
    "skypeuripreview",
    "microsoftpreview",
    "redditbot",
    "mastodon",
    "synapse",
    "zulipurlpreview",
    "embedly",
    "iframely",
    "vkshare",
    "pinterestbot",
    "cardyb",
    "snap url preview",
    "kakaotalk-scrap",
];

/// Other robots: they neither post a link nor open it.
const ROBOTS: &[&str] = &["crawler", "spider", "slurp"];

/// "bot" ending a name, as in Googlebot/2.1 or PetalBot; but not inside a phone's name such as
/// CUBOT X30.
fn names_a_bot(ua: &str) -> bool {
    ua.match_indices("bot").any(|(i, _)| {
        matches!(
            ua.as_bytes().get(i + 3),
            None | Some(b'/' | b';' | b')' | b'-')
        )
    })
}

pub fn visitor(user_agent: &str) -> Visitor {
    let ua = user_agent.to_ascii_lowercase();
    if PREVIEWERS.iter().any(|w| ua.contains(w)) {
        Visitor::Previewer
    } else if ROBOTS.iter().any(|w| ua.contains(w)) || names_a_bot(&ua) {
        Visitor::Robot
    } else {
        Visitor::Person
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_previewers_robots_and_people_are_told_apart() {
        let previewers = [
            "Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)",
            "TelegramBot (like TwitterBot)",
            "WhatsApp/2.23.20.0 A",
            "Slackbot-LinkExpanding 1.0 (+https://api.slack.com/robots)",
            "Slack-ImgProxy (+https://api.slack.com/robots)",
            "Twitterbot/1.0",
            "facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_11_1) AppleWebKit/601.2.4 (KHTML, like Gecko) Version/9.0.1 Safari/601.2.4 facebookexternalhit/1.1 Facebot Twitterbot/1.0",
            "LinkedInBot/1.0 (compatible; Mozilla/5.0; Apache-HttpClient +http://www.linkedin.com)",
            "Mozilla/5.0 (Windows NT 6.1; WOW64) SkypeUriPreview Preview/0.5",
            "http.rb/5.1.1 (Mastodon/4.2.1; +https://mastodon.social/) Bot",
            "Synapse (bot; +https://github.com/matrix-org/synapse)",
            "Mozilla/5.0 (compatible; redditbot/1.0; +http://www.reddit.com/feedback)",
            "Mozilla/5.0 (compatible; Bluesky Cardyb/1.1; +mailto:support@bsky.app)",
            "Mozilla/5.0 (compatible; Snap URL Preview Service; bot; snapchat; https://developers.snap.com/robots)",
            "Mozilla/5.0 (compatible; ZulipURLPreview/9.0; +https://chat.example.org)",
        ];
        for ua in previewers {
            assert_eq!(visitor(ua), Visitor::Previewer, "{ua}");
        }
        let robots = [
            "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
            "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
            "Mozilla/5.0 (compatible; AhrefsBot/7.0; +http://ahrefs.com/robot/)",
            "Mozilla/5.0 (compatible; Yahoo! Slurp; http://help.yahoo.com/help/us/ysearch/slurp)",
            "DuckDuckBot-Https/1.1; (+https://duckduckgo.com/duckduckbot)",
            "Mozilla/5.0 (compatible; PetalBot;+https://webmaster.petalsearch.com/site/petalbot)",
        ];
        for ua in robots {
            assert_eq!(visitor(ua), Visitor::Robot, "{ua}");
        }
        let people = [
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36",
            "Mozilla/5.0 (X11; Linux x86_64; rv:143.0) Gecko/20100101 Firefox/143.0",
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1",
            "Mozilla/5.0 (Linux; Android 11; CUBOT X30) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
            "Mozilla/5.0 (Linux; Android 13; CUBOT KINGKONG 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
            "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148 Snapchat/13.58.0.42 (like Safari/8621.1.15.10.7, panda)",
            "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36 Viber/25.6.0.0",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) ZulipElectron/5.12.0 Chrome/134.0.0.0 Electron/35.0.0 Safari/537.36",
            "",
        ];
        for ua in people {
            assert_eq!(visitor(ua), Visitor::Person, "{ua}");
        }
    }

    #[test]
    fn every_previewer_word_is_in_small_letters() {
        for w in PREVIEWERS {
            assert_eq!(*w, w.to_ascii_lowercase());
        }
    }
}
