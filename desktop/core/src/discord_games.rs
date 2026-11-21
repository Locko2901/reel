//! Discord's list of the games it knows, for the picture Show on Discord sends: a game's icon,
//! found by its Steam app id or, on Windows, by its program's folders and file name.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// The list Discord's own app reads; it needs no sign in.
pub const LIST: &str = "https://discord.com/api/v9/games/detectable";
/// reel asks for the list at most once in this many seconds.
pub const EVERY: u64 = 24 * 60 * 60;
const TIMEOUT: Duration = Duration::from_secs(30);
/// The list is about 2.5 MB packed and 13 MB unpacked: far more is no list of games.
const MAX: usize = 64 << 20;

/// Discord's list: `REEL_DISCORD_GAMES_URL` when set (for tests), else Discord's.
pub fn url() -> String {
    std::env::var("REEL_DISCORD_GAMES_URL")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| LIST.to_string())
}

/// A game of Discord's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Known {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl Known {
    /// Its icon on Discord's CDN: square, at 256 px, the largest Discord serves.
    pub fn picture(&self) -> Option<String> {
        let icon = self.icon.as_deref()?;
        Some(format!(
            "https://cdn.discordapp.com/app-icons/{}/{icon}.png?size=256",
            self.id
        ))
    }
}

/// The list as reel keeps it in its data folder: only what a game is found by.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Games {
    /// When reel last asked Discord, in Unix seconds, whatever came of it.
    pub asked: u64,
    pub etag: Option<String>,
    /// By Steam app id; None where two games have the same.
    pub steam: BTreeMap<u32, Option<Known>>,
    /// By a Windows program's last folders and file name, lowercase with `/`; None where two
    /// games have the same.
    pub paths: BTreeMap<String, Option<Known>>,
}

impl Games {
    /// The list kept in `file`; None when there is none or it cannot be read.
    pub fn load(file: &Path) -> Option<Games> {
        serde_json::from_slice(&std::fs::read(file).ok()?).ok()
    }

    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let part = crate::settings::part_path(file);
        std::fs::write(&part, serde_json::to_vec(self)?)?;
        std::fs::rename(&part, file)
    }

    /// Never asked, a day or more since, or a clock that went back.
    pub fn due(&self, now: u64) -> bool {
        self.asked > now || now - self.asked >= EVERY
    }

    fn empty(&self) -> bool {
        self.steam.is_empty() && self.paths.is_empty()
    }

    /// The game a program is: by its Steam app id first, else, on Windows, by the program's
    /// folders and file name. A name that more than one game has finds none.
    pub fn find(
        &self,
        steam_app: Option<u32>,
        program: Option<&Path>,
        windows: bool,
    ) -> Option<&Known> {
        if let Some(k) = steam_app.and_then(|a| self.steam.get(&a)) {
            return k.as_ref();
        }
        if !windows {
            return None;
        }
        let text = program?.to_string_lossy().replace('\\', "/").to_lowercase();
        let parts: Vec<&str> = text.split('/').filter(|p| !p.is_empty()).collect();
        let mut found: Option<&Known> = None;
        for n in 2..=parts.len() {
            match self.paths.get(&parts[parts.len() - n..].join("/")) {
                None => {}
                Some(None) => return None,
                Some(Some(k)) if found.is_some_and(|f| f.id != k.id) => return None,
                Some(Some(k)) => found = Some(k),
            }
        }
        found
    }
}

/// A game of the list, or anything else there, skipped: one odd game does not lose the list.
#[derive(Deserialize)]
#[serde(untagged)]
enum Item {
    Game(Listed),
    Other(serde::de::IgnoredAny),
}

#[derive(Deserialize)]
struct Listed {
    id: String,
    #[serde(default)]
    icon_hash: Option<String>,
    #[serde(default)]
    executables: Option<Vec<Exe>>,
    #[serde(default)]
    third_party_skus: Option<Vec<Sku>>,
}

#[derive(Deserialize)]
struct Exe {
    name: String,
    #[serde(default)]
    os: Option<String>,
    #[serde(default)]
    is_launcher: Option<bool>,
}

#[derive(Deserialize)]
struct Sku {
    #[serde(default)]
    distributor: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

/// Discord's list as it comes, down to the Steam app ids and the Windows programs that name a
/// folder: a file name alone, such as `game.exe`, is too common to say which game it is.
pub fn trim(list: &[u8]) -> Result<Games, String> {
    let listed: Vec<Item> =
        serde_json::from_slice(list).map_err(|e| format!("not a list of games: {e}"))?;
    let mut games = Games::default();
    for g in listed {
        let Item::Game(g) = g else { continue };
        if g.id.is_empty() || !g.id.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let icon = g.icon_hash.filter(|h| {
            !h.is_empty()
                && h.len() <= 64
                && h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        });
        let known = Known { id: g.id, icon };
        for s in g.third_party_skus.into_iter().flatten() {
            if s.distributor.as_deref() != Some("steam") {
                continue;
            }
            if let Some(app) = s.id.and_then(|i| i.parse::<u32>().ok()) {
                put(&mut games.steam, app, &known);
            }
        }
        for e in g.executables.into_iter().flatten() {
            if e.os.as_deref() != Some("win32") || e.is_launcher == Some(true) {
                continue;
            }
            let name = e
                .name
                .trim_start_matches('>')
                .replace('\\', "/")
                .to_lowercase();
            let name = name.trim_matches('/');
            if name.contains('/') {
                put(&mut games.paths, name.to_string(), &known);
            }
        }
    }
    Ok(games)
}

fn put<K: Ord>(map: &mut BTreeMap<K, Option<Known>>, key: K, k: &Known) {
    map.entry(key)
        .and_modify(|was| {
            if was.as_ref().is_some_and(|w| w.id != k.id) {
                *was = None;
            }
        })
        .or_insert_with(|| Some(k.clone()));
}

/// What came of asking Discord for its list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// A new list: how many games by Steam app id, and how many program names.
    New { steam: usize, paths: usize },
    /// The list has not changed since the one kept.
    Same,
    /// The list kept stays as it was.
    Failed(String),
}

/// Asks Discord for its list, with the ETag of the one kept, and gives back the list to keep
/// and what came of it. Whatever came of it, the next ask is a day later.
pub async fn refresh(mut kept: Games, url: &str, now: u64) -> (Games, Asked) {
    let etag = kept.etag.clone().filter(|_| !kept.empty());
    let said = match ask(url, etag.as_deref()).await {
        Ok(None) => Asked::Same,
        Ok(Some((body, etag))) => match tokio::task::spawn_blocking(move || trim(&body)).await {
            Ok(Ok(games)) => {
                let said = Asked::New {
                    steam: games.steam.len(),
                    paths: games.paths.len(),
                };
                kept = Games { etag, ..games };
                said
            }
            Ok(Err(e)) => Asked::Failed(e),
            Err(e) => Asked::Failed(e.to_string()),
        },
        Err(e) => Asked::Failed(e),
    };
    kept.asked = now;
    (kept, said)
}

fn chain(e: &dyn std::error::Error) -> String {
    let mut words = e.to_string();
    let mut cur = e.source();
    while let Some(s) = cur {
        words = format!("{words}: {s}");
        cur = s.source();
    }
    words
}

/// The list and its ETag, or None when it has not changed since `etag`.
async fn ask(url: &str, etag: Option<&str>) -> Result<Option<(Vec<u8>, Option<String>)>, String> {
    use reqwest::header::{ACCEPT_ENCODING, CONTENT_ENCODING, ETAG, IF_NONE_MATCH};
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(crate::user_agent())
        .build()
        .map_err(|e| chain(&e))?;
    let mut req = client.get(url).header(ACCEPT_ENCODING, "gzip");
    if let Some(t) = etag {
        req = req.header(IF_NONE_MATCH, t);
    }
    let mut resp = req.send().await.map_err(|e| chain(&e))?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(format!("Discord answered {status}"));
    }
    let header = |h| {
        resp.headers()
            .get(h)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };
    let etag = header(ETAG);
    let gzip = match header(CONTENT_ENCODING) {
        None => false,
        Some(e) if e.eq_ignore_ascii_case("gzip") => true,
        Some(e) if e.eq_ignore_ascii_case("identity") => false,
        Some(e) => return Err(format!("the list came packed as {e}")),
    };
    let mut body = Vec::new();
    while let Some(c) = resp.chunk().await.map_err(|e| chain(&e))? {
        body.extend_from_slice(&c);
        if body.len() > MAX {
            return Err(format!("the list is over {} MB", MAX >> 20));
        }
    }
    if gzip {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&body[..])
            .take(MAX as u64 + 1)
            .read_to_end(&mut out)
            .map_err(|e| format!("the list did not unpack: {e}"))?;
        if out.len() > MAX {
            return Err(format!("the list is over {} MB", MAX >> 20));
        }
        body = out;
    }
    Ok(Some((body, etag)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Made up games in the shape of Discord's list.
    fn list() -> serde_json::Value {
        serde_json::json!([
            { "id": "1100000000000000001", "name": "Starfall Tactics", "icon_hash": "a1b2c3d4e5f60718293a4b5c6d7e8f90",
              "executables": [{ "name": "starfall tactics/starfall.exe", "os": "win32", "is_launcher": false }],
              "third_party_skus": [{ "distributor": "steam", "id": "2400100" }, { "distributor": "xbox", "id": "9NXQ" }] },
            { "id": "1100000000000000002", "name": "Ember Pass", "icon_hash": "0f1e2d3c4b5a69788796a5b4c3d2e1f0",
              "executables": [
                  { "name": "ember pass/bin/win64/emberpass.exe", "os": "win32", "is_launcher": false },
                  { "name": "emberpass.exe", "os": "win32", "is_launcher": false },
                  { "name": "ember pass/launcher.exe", "os": "win32", "is_launcher": true },
                  { "name": "ember pass/emberpass", "os": "linux", "is_launcher": false }
              ],
              "third_party_skus": null },
            { "id": "1100000000000000003", "name": "Lowtide", "icon_hash": null,
              "executables": [{ "name": "lowtide/lowtide.exe", "os": "win32" }],
              "third_party_skus": [{ "distributor": "steam", "id": "2400300" }] },
            { "id": "1100000000000000004", "name": "Hollow Pines", "icon_hash": "1234abcd1234abcd1234abcd1234abcd",
              "executables": [{ "name": "binaries/game.exe", "os": "win32", "is_launcher": false }] },
            { "id": "1100000000000000005", "name": "Cinder Road", "icon_hash": "abcd1234abcd1234abcd1234abcd1234",
              "executables": [{ "name": "binaries/game.exe", "os": "win32", "is_launcher": false },
                              { "name": "cinder road/bin/win64/emberpass.exe", "os": "win32", "is_launcher": false }],
              "third_party_skus": [{ "distributor": "steam" }] },
            { "id": "not a number", "name": "Broken", "executables": [] },
            { "id": 1100000000000000006u64, "name": "An id as a number" },
            { "id": "1100000000000000007", "name": "Odd", "executables": [{ "os": "win32" }],
              "third_party_skus": [{ "distributor": "steam", "id": "2400700" }] },
            null
        ])
    }

    fn games() -> Games {
        trim(list().to_string().as_bytes()).unwrap()
    }

    fn found(g: &Games, app: Option<u32>, path: &str, windows: bool) -> Option<String> {
        g.find(app, Some(&PathBuf::from(path)), windows)
            .map(|k| k.id.clone())
    }

    #[test]
    fn the_list_comes_down_to_steam_ids_and_programs_with_a_folder() {
        let g = games();
        assert_eq!(
            g.steam.keys().copied().collect::<Vec<_>>(),
            [2400100, 2400300],
            "an odd game, an id as a number and a null skipped, the rest kept"
        );
        assert_eq!(
            g.paths.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "binaries/game.exe",
                "cinder road/bin/win64/emberpass.exe",
                "ember pass/bin/win64/emberpass.exe",
                "lowtide/lowtide.exe",
                "starfall tactics/starfall.exe",
            ],
            "no file name alone, no launcher, no Linux program"
        );
        assert_eq!(g.paths["binaries/game.exe"], None, "two games have it");
        let starfall = g.steam[&2400100].clone().unwrap();
        assert_eq!(
            starfall.picture().unwrap(),
            "https://cdn.discordapp.com/app-icons/1100000000000000001/a1b2c3d4e5f60718293a4b5c6d7e8f90.png?size=256"
        );
        assert!(starfall.picture().unwrap().len() <= 256);
        assert_eq!(
            g.steam[&2400300].as_ref().unwrap().picture(),
            None,
            "no icon"
        );
        assert!(trim(b"{\"message\": \"401: Unauthorized\"}").is_err());
    }

    #[test]
    fn a_game_is_found_by_its_steam_id_then_by_its_program_on_windows() {
        let g = games();
        let starfall = Some("1100000000000000001".to_string());
        assert_eq!(
            found(&g, Some(2400100), "", false),
            starfall,
            "by the app id"
        );
        assert_eq!(
            found(
                &g,
                Some(2400100),
                r"D:\Games\Ember Pass\bin\win64\EmberPass.exe",
                true
            ),
            starfall,
            "the app id first"
        );
        assert_eq!(
            found(&g, None, r"C:\Games\Starfall Tactics\Starfall.exe", true),
            starfall,
            "by the folder and file name, in any case"
        );
        assert_eq!(
            found(
                &g,
                Some(999),
                r"D:\Games\Ember Pass\bin\win64\EmberPass.exe",
                true
            ),
            Some("1100000000000000002".into()),
            "an app id Discord does not know, then the program"
        );
        assert_eq!(found(&g, None, r"C:\Starfall.exe", true), None);
        assert_eq!(
            found(&g, None, r"C:\Other\EmberPass.exe", true),
            None,
            "a file name alone"
        );
    }

    #[test]
    fn a_program_more_than_one_game_has_finds_none() {
        let g = games();
        assert_eq!(
            found(&g, None, r"C:\Hollow Pines\Binaries\Game.exe", true),
            None
        );
        let mut g = games();
        g.paths.insert(
            "win64/emberpass.exe".into(),
            Some(Known {
                id: "1100000000000000009".into(),
                icon: None,
            }),
        );
        assert_eq!(
            found(
                &g,
                None,
                r"D:\Games\Ember Pass\bin\win64\EmberPass.exe",
                true
            ),
            None,
            "a shorter name of another game matches the same path"
        );
    }

    #[test]
    fn on_linux_only_the_steam_id_finds_a_game() {
        let g = games();
        let path = "/home/mira/.steam/steam/steamapps/common/Starfall Tactics/Starfall.exe";
        assert_eq!(found(&g, None, path, false), None);
        assert_eq!(
            found(&g, Some(2400100), path, false),
            Some("1100000000000000001".into())
        );
    }

    #[test]
    fn the_list_kept_is_read_again_and_asked_for_once_a_day() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data").join("discord-games.json");
        assert_eq!(Games::load(&file), None);
        let mut g = games();
        g.asked = 1_800_000_000;
        g.etag = Some("W/\"1\"".into());
        g.save(&file).unwrap();
        assert_eq!(Games::load(&file), Some(g.clone()));
        assert!(!g.due(1_800_000_000 + EVERY - 1));
        assert!(g.due(1_800_000_000 + EVERY));
        assert!(g.due(1_700_000_000), "a clock that went back");
        assert!(Games::default().due(1_800_000_000), "never asked");
        std::fs::write(&file, "{not json").unwrap();
        assert_eq!(Games::load(&file), None);
    }

    mod standin {
        use super::*;
        use axum::http::{header, HeaderMap, StatusCode};
        use axum::response::IntoResponse;
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct Seen {
            /// Each request's If-None-Match and User-Agent.
            asks: Vec<(Option<String>, String)>,
            fail: bool,
        }

        type Shared = Arc<Mutex<Seen>>;

        /// A stand-in for Discord's list: packed with gzip as Discord sends it, its ETag, a 304
        /// for that ETag, and a 503 while told to fail.
        async fn start() -> (String, Shared) {
            let seen: Shared = Arc::default();
            let kept = seen.clone();
            let app = axum::Router::new().route(
                "/api/v9/games/detectable",
                axum::routing::get(move |h: HeaderMap| {
                    let kept = kept.clone();
                    async move {
                        let get = |n| {
                            h.get(n)
                                .and_then(|v: &axum::http::HeaderValue| v.to_str().ok())
                                .map(str::to_string)
                        };
                        let mut s = kept.lock().unwrap();
                        s.asks.push((
                            get(header::IF_NONE_MATCH),
                            get(header::USER_AGENT).unwrap_or_default(),
                        ));
                        if s.fail {
                            return StatusCode::SERVICE_UNAVAILABLE.into_response();
                        }
                        if get(header::IF_NONE_MATCH).as_deref() == Some("W/\"list-1\"") {
                            return StatusCode::NOT_MODIFIED.into_response();
                        }
                        let mut gz =
                            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                        gz.write_all(list().to_string().as_bytes()).unwrap();
                        (
                            [
                                (header::ETAG, "W/\"list-1\""),
                                (header::CONTENT_ENCODING, "gzip"),
                            ],
                            gz.finish().unwrap(),
                        )
                            .into_response()
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let at = listener.local_addr().unwrap();
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            });
            (format!("http://{at}/api/v9/games/detectable"), seen)
        }

        #[tokio::test]
        async fn the_list_then_a_304_then_a_failure_that_keeps_it() {
            let (url, seen) = start().await;
            let (first, said) = refresh(Games::default(), &url, 100).await;
            assert_eq!(said, Asked::New { steam: 2, paths: 5 });
            assert_eq!(first.etag.as_deref(), Some("W/\"list-1\""));
            assert_eq!(first.asked, 100);
            assert!(first.find(Some(2400100), None, false).is_some());
            let (second, said) = refresh(first.clone(), &url, 100 + EVERY).await;
            assert_eq!(said, Asked::Same);
            assert_eq!(
                second,
                Games {
                    asked: 100 + EVERY,
                    ..first.clone()
                }
            );
            seen.lock().unwrap().fail = true;
            let (third, said) = refresh(second.clone(), &url, 100 + 2 * EVERY).await;
            assert_eq!(
                said,
                Asked::Failed("Discord answered 503 Service Unavailable".into())
            );
            assert_eq!(
                third,
                Games {
                    asked: 100 + 2 * EVERY,
                    ..second
                },
                "kept, asked a day later"
            );
            let asks = seen.lock().unwrap().asks.clone();
            assert_eq!(asks[0].0, None, "nothing kept: no ETag");
            assert_eq!(asks[1].0.as_deref(), Some("W/\"list-1\""));
            assert!(
                asks.iter().all(|(_, ua)| *ua == crate::user_agent()),
                "{asks:?}"
            );
        }

        #[tokio::test]
        async fn a_failure_with_nothing_kept_finds_no_game_and_an_empty_list_sends_no_etag() {
            let (url, seen) = start().await;
            seen.lock().unwrap().fail = true;
            let (g, said) = refresh(Games::default(), &url, 5).await;
            assert!(matches!(said, Asked::Failed(_)));
            assert_eq!(g.find(Some(2400100), None, false), None);
            assert_eq!(g.asked, 5);
            let lost = Games {
                etag: Some("W/\"list-1\"".into()),
                ..Games::default()
            };
            seen.lock().unwrap().fail = false;
            let (g, said) = refresh(lost, &url, 6).await;
            assert_eq!(
                said,
                Asked::New { steam: 2, paths: 5 },
                "an ETag without a list asks for it all"
            );
            assert!(g.find(Some(2400100), None, false).is_some());
            let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let gone = format!("http://{}/", closed.local_addr().unwrap());
            drop(closed);
            let (_, said) = refresh(Games::default(), &gone, 7).await;
            let Asked::Failed(why) = said else { panic!() };
            assert!(why.contains("error sending request"), "{why}");
        }
    }
}
