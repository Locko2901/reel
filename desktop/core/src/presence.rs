//! Discord's Rich Presence: while reel clips or records a game, the Discord app on this PC shows
//! it to friends. Told over Discord's local IPC, a named pipe on Windows and a Unix socket
//! elsewhere: no account, no token.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, WriteHalf};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

/// The Discord application "reel". Its ID is public: it goes out with every presence, and every
/// program that shows one carries its own; the application's secret is never needed.
pub const APPLICATION_ID: &str = "1573628367599697921";
/// The application's art, uploaded under this key.
const ART: &str = "reel";
const BUTTON: &str = "reel on GitHub";
const REPOSITORY: &str = "https://github.com/Locko2901/reel";
const ANSWER: Duration = Duration::from_secs(10);
/// No frame of Discord's comes near this; a larger length is a broken stream.
const MAX_FRAME: u32 = 1 << 20;

const HANDSHAKE: u32 = 0;
const FRAME: u32 = 1;
const CLOSE: u32 = 2;
const PING: u32 = 3;
const PONG: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doing {
    Clipping,
    Recording,
}

impl Doing {
    pub fn word(self) -> &'static str {
        match self {
            Doing::Clipping => "Clipping",
            Doing::Recording => "Recording",
        }
    }
}

/// What friends see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub doing: Doing,
    /// None with Name the game off, or for a name Discord would refuse.
    pub game: Option<String>,
    /// Unix seconds the time counts from: the game's start, or the recording's.
    pub since: u64,
    pub button: bool,
}

/// What the settings and the moment ask for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Want {
    /// Show on Discord is off.
    Off,
    /// On, with nothing to show, and why.
    Nothing(String),
    Show(Shown),
}

impl Want {
    /// Whether going from `self` to `next` is a change Discord should hear of.
    pub fn differs(&self, next: &Want) -> bool {
        match (self, next) {
            (Want::Show(a), Want::Show(b)) => a.differs(b),
            _ => self != next,
        }
    }
}

/// The moment as the app sees it.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub show: bool,
    pub name_game: bool,
    pub button: bool,
    /// The Game that runs, as the bar names it, and when it started, in Unix seconds.
    pub game: Option<(String, u64)>,
    pub replay_buffer: bool,
    /// A session recording runs, since then.
    pub recording: Option<u64>,
}

pub fn want(f: &Facts) -> Want {
    if !f.show {
        return Want::Off;
    }
    let Some((name, started)) = &f.game else {
        return Want::Nothing("no game runs".into());
    };
    let (doing, since) = match (f.recording, f.replay_buffer) {
        (Some(at), _) => (Doing::Recording, at),
        (None, true) => (Doing::Clipping, *started),
        (None, false) => return Want::Nothing(format!("{name} runs without the replay buffer")),
    };
    Want::Show(Shown {
        doing,
        game: if f.name_game { field(name) } else { None },
        since,
        button: f.button,
    })
}

/// Discord takes 2 to 128 characters in a line.
fn field(s: &str) -> Option<String> {
    let s = s.trim();
    if s.chars().count() < 2 {
        return None;
    }
    Some(
        s.chars()
            .take(128)
            .collect::<String>()
            .trim_end()
            .to_string(),
    )
}

impl Shown {
    /// "Starfall Tactics, Clipping", or "Clipping" without the game.
    pub fn words(&self) -> String {
        match &self.game {
            Some(g) => format!("{g}, {}", self.doing.word()),
            None => self.doing.word().to_string(),
        }
    }

    /// A start read again a moment later moves by a second; a new run of the game moves more.
    pub fn differs(&self, o: &Shown) -> bool {
        self.doing != o.doing
            || self.game != o.game
            || self.button != o.button
            || self.since.abs_diff(o.since) > 2
    }

    pub fn activity(&self) -> Value {
        let mut a = json!({
            "type": 0,
            "state": self.doing.word(),
            "timestamps": { "start": self.since },
            "assets": { "large_image": ART, "large_text": "reel" },
        });
        if let Some(g) = &self.game {
            a["details"] = json!(g);
            // the member list then says "Playing <game>", as for a game Discord finds itself
            a["status_display_type"] = json!(2);
        }
        if self.button {
            a["buttons"] = json!([{ "label": BUTTON, "url": REPOSITORY }]);
        }
        a
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Discord is not running on this PC")]
    NotRunning,
    #[error("Discord refused it: {0}")]
    Refused(String),
    #[error("Discord closed the connection{}", why(.0))]
    Gone(String),
    #[error("Discord did not answer within {} s", ANSWER.as_secs())]
    Silent,
    #[error("Discord sent what reel cannot read: {0}")]
    Garbled(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn why(s: &str) -> String {
    if s.is_empty() {
        String::new()
    } else {
        format!(": {s}")
    }
}

/// Discord's reason in a CLOSE or an ERROR.
fn said(body: &Value) -> String {
    let message = body["message"].as_str().unwrap_or("no reason given");
    match body["code"].as_i64() {
        Some(code) => format!("{message} ({code})"),
        None => message.to_string(),
    }
}

pub fn encode(op: u32, body: &Value) -> Vec<u8> {
    let body = body.to_string().into_bytes();
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(&op.to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

async fn read_frame<R: AsyncRead + Unpin>(r: &mut R) -> Result<(u32, Value), Error> {
    let mut head = [0u8; 8];
    r.read_exact(&mut head).await?;
    let op = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
    let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
    if len > MAX_FRAME {
        return Err(Error::Garbled(format!("a frame of {len} bytes")));
    }
    let mut body = vec![0; len as usize];
    r.read_exact(&mut body).await?;
    let v = serde_json::from_slice(&body).map_err(|e| Error::Garbled(e.to_string()))?;
    Ok((op, v))
}

/// Where a Discord app may listen, in the order tried: on Unix its runtime folder and the
/// temporary ones, each also where the Flatpak and the Snap of Discord put the socket.
pub fn endpoints(env: impl Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    if cfg!(windows) {
        return (0..10)
            .map(|n| PathBuf::from(format!(r"\\?\pipe\discord-ipc-{n}")))
            .collect();
    }
    let mut bases: Vec<PathBuf> = Vec::new();
    for key in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Some(v) = env(key).filter(|v| !v.is_empty()) {
            bases.push(PathBuf::from(v));
        }
    }
    bases.push(PathBuf::from("/tmp"));
    let mut seen = std::collections::HashSet::new();
    bases.retain(|b| seen.insert(b.clone()));
    let mut out = Vec::new();
    for n in 0..10 {
        for base in &bases {
            for sub in ["", "app/com.discordapp.Discord", "snap.discord"] {
                out.push(base.join(sub).join(format!("discord-ipc-{n}")));
            }
        }
    }
    out
}

/// This PC's places, from its environment.
pub fn here() -> Vec<PathBuf> {
    endpoints(|k| std::env::var(k).ok())
}

trait Io: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Io for T {}

async fn open(p: &Path) -> std::io::Result<Box<dyn Io>> {
    #[cfg(windows)]
    {
        let pipe = tokio::net::windows::named_pipe::ClientOptions::new().open(p)?;
        Ok(Box::new(pipe))
    }
    #[cfg(not(windows))]
    {
        Ok(Box::new(tokio::net::UnixStream::connect(p).await?))
    }
}

/// What came from Discord between two commands.
#[derive(Debug)]
pub enum Incoming {
    Frame(u32, Value),
    /// Discord closed the connection, or it broke, and why.
    Closed(String),
}

/// A connection to a Discord app that said READY.
pub struct Conn {
    write: WriteHalf<Box<dyn Io>>,
    frames: mpsc::Receiver<Incoming>,
    reader: tokio::task::JoinHandle<()>,
    pid: u32,
    nonce: u64,
}

impl Drop for Conn {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

/// The first of `at` that answers, with the handshake done, and where it was.
pub async fn connect(at: &[PathBuf], client_id: &str) -> Result<(Conn, PathBuf), Error> {
    for p in at {
        // a Windows pipe is no file to look for
        if cfg!(not(windows)) && !p.exists() {
            continue;
        }
        let Ok(io) = open(p).await else { continue };
        return Conn::start(io, client_id).await.map(|c| (c, p.clone()));
    }
    Err(Error::NotRunning)
}

impl Conn {
    async fn start(io: Box<dyn Io>, client_id: &str) -> Result<Conn, Error> {
        let (mut read, mut write) = tokio::io::split(io);
        let hello = json!({ "v": 1, "client_id": client_id });
        write.write_all(&encode(HANDSHAKE, &hello)).await?;
        let (op, body) = tokio::time::timeout(ANSWER, read_frame(&mut read))
            .await
            .map_err(|_| Error::Silent)??;
        match op {
            FRAME if body["evt"] == "READY" => {}
            CLOSE => return Err(Error::Refused(said(&body))),
            _ => return Err(Error::Garbled(format!("opcode {op} before READY"))),
        }
        let (tx, frames) = mpsc::channel(16);
        let reader = tokio::spawn(async move {
            loop {
                let next = match read_frame(&mut read).await {
                    Ok((CLOSE, body)) => Incoming::Closed(said(&body)),
                    Ok((op, body)) => Incoming::Frame(op, body),
                    Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                        Incoming::Closed(String::new())
                    }
                    Err(e) => Incoming::Closed(e.to_string()),
                };
                let last = matches!(next, Incoming::Closed(_));
                if tx.send(next).await.is_err() || last {
                    return;
                }
            }
        });
        Ok(Conn {
            write,
            frames,
            reader,
            pid: std::process::id(),
            nonce: 0,
        })
    }

    /// Sets the activity, or clears it with None, and waits for Discord's answer.
    pub async fn set_activity(&mut self, activity: Option<&Value>) -> Result<(), Error> {
        self.nonce += 1;
        let nonce = self.nonce.to_string();
        let cmd = json!({
            "cmd": "SET_ACTIVITY",
            "args": { "pid": self.pid, "activity": activity },
            "nonce": nonce,
        });
        self.write.write_all(&encode(FRAME, &cmd)).await?;
        let by = tokio::time::Instant::now() + ANSWER;
        loop {
            let got = tokio::time::timeout_at(by, self.frames.recv())
                .await
                .map_err(|_| Error::Silent)?;
            match got {
                None => return Err(Error::Gone(String::new())),
                Some(Incoming::Closed(why)) => return Err(Error::Gone(why)),
                Some(Incoming::Frame(PING, body)) => self.pong(&body).await?,
                Some(Incoming::Frame(_, body)) if body["nonce"] == nonce => {
                    return if body["evt"] == "ERROR" {
                        Err(Error::Refused(said(&body["data"])))
                    } else {
                        Ok(())
                    };
                }
                Some(Incoming::Frame(..)) => {}
            }
        }
    }

    /// The next thing Discord sends while no command waits; safe to drop while it waits.
    pub async fn next(&mut self) -> Incoming {
        self.frames
            .recv()
            .await
            .unwrap_or(Incoming::Closed(String::new()))
    }

    pub async fn pong(&mut self, body: &Value) -> Result<(), Error> {
        self.write.write_all(&encode(PONG, body)).await?;
        Ok(())
    }
}

pub struct Timing {
    /// The least time between two activities sent.
    pub every: Duration,
    /// How long to wait for a Discord that is not there.
    pub retry: Duration,
}

/// Discord takes five updates in 20 s.
pub const TIMING: Timing = Timing {
    every: Duration::from_secs(5),
    retry: Duration::from_secs(30),
};

/// What the task tells the app after each step, for the setting's line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// Discord has it.
    Sent(Shown),
    Refused(String),
    NotRunning,
    /// Anything else that went wrong; tried again later.
    Trouble(String),
    /// Nothing shown: off, or nothing to show and why.
    Quiet(Want),
}

async fn incoming(conn: &mut Option<Conn>) -> Incoming {
    match conn {
        Some(c) => c.next().await,
        None => std::future::pending().await,
    }
}

async fn until(at: Option<Instant>) {
    match at {
        Some(t) => tokio::time::sleep_until(t).await,
        None => std::future::pending().await,
    }
}

/// The one task that talks to Discord. It connects only while there is something to show, sends
/// at most once per `every` with the last state, clears and closes when nothing is to show, and
/// tries again after `retry` while Discord is not there. `places` says where to look, each time.
pub async fn run(
    mut want: watch::Receiver<Want>,
    places: impl Fn() -> Vec<PathBuf>,
    timing: Timing,
    report: impl Fn(Report),
) {
    let mut conn: Option<Conn> = None;
    // what Discord has, or refused: not sent again until it changes
    let mut shown: Option<Shown> = None;
    let mut last: Option<Instant> = None;
    let mut retry: Option<Instant> = None;
    // "not running" goes into the Log once each time Discord is missed
    let mut told = false;
    loop {
        let now = Instant::now();
        let w = want.borrow_and_update().clone();
        let mut wake = None;
        match &w {
            Want::Show(s) if shown.as_ref().is_none_or(|o| o.differs(s)) => {
                let due = [last.map(|t| t + timing.every), retry]
                    .into_iter()
                    .flatten()
                    .max();
                if let Some(at) = due.filter(|at| *at > now) {
                    wake = Some(at);
                } else {
                    if conn.is_none() {
                        match connect(&places(), APPLICATION_ID).await {
                            Ok((c, place)) => {
                                tracing::info!("discord: connected at {}", place.display());
                                conn = Some(c);
                                told = false;
                            }
                            Err(Error::NotRunning) => {
                                if !told {
                                    tracing::info!(
                                        "discord: not running; reel tries again every {} s",
                                        timing.retry.as_secs()
                                    );
                                    told = true;
                                }
                                retry = Some(now + timing.retry);
                                report(Report::NotRunning);
                                continue;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "discord: {e}; reel tries again in {} s",
                                    timing.retry.as_secs()
                                );
                                retry = Some(now + timing.retry);
                                report(Report::Trouble(e.to_string()));
                                continue;
                            }
                        }
                    }
                    if let Some(c) = conn.as_mut() {
                        last = Some(Instant::now());
                        match c.set_activity(Some(&s.activity())).await {
                            Ok(()) => {
                                tracing::info!("discord: sent {}", s.words());
                                shown = Some(s.clone());
                                retry = None;
                                report(Report::Sent(s.clone()));
                            }
                            Err(Error::Refused(why)) => {
                                tracing::warn!("discord: refused {}: {why}", s.words());
                                shown = Some(s.clone());
                                report(Report::Refused(why));
                            }
                            Err(e) => {
                                tracing::info!("discord: {e}; reel tries again");
                                conn = None;
                                shown = None;
                                retry = Some(Instant::now() + timing.every);
                                report(Report::NotRunning);
                            }
                        }
                    }
                    continue;
                }
            }
            Want::Show(_) => {}
            Want::Off | Want::Nothing(_) => {
                if let (Some(c), Some(_)) = (conn.as_mut(), shown.as_ref()) {
                    match c.set_activity(None).await {
                        Ok(()) => tracing::info!("discord: cleared"),
                        Err(e) => {
                            tracing::info!("discord: not cleared, the connection closes: {e}")
                        }
                    }
                }
                conn = None;
                shown = None;
                retry = None;
                told = false;
                report(Report::Quiet(w.clone()));
            }
        }
        tokio::select! {
            changed = want.changed() => {
                if changed.is_err() {
                    return;
                }
            }
            got = incoming(&mut conn) => match got {
                Incoming::Frame(PING, body) => {
                    if let Some(c) = conn.as_mut() {
                        let _ = c.pong(&body).await;
                    }
                }
                Incoming::Frame(..) => {}
                Incoming::Closed(why) => {
                    tracing::info!("discord: Discord closed the connection{}", self::why(&why));
                    conn = None;
                    shown = None;
                    retry = Some(Instant::now() + timing.retry);
                    report(Report::NotRunning);
                }
            },
            _ = until(wake) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            show: true,
            name_game: true,
            button: true,
            game: Some(("Starfall Tactics".into(), 1_700_000_000)),
            replay_buffer: true,
            recording: None,
        }
    }

    fn shown(w: Want) -> Shown {
        match w {
            Want::Show(s) => s,
            other => panic!("nothing shown: {other:?}"),
        }
    }

    #[test]
    fn a_frame_is_its_opcode_and_length_little_endian_then_the_json() {
        let hello = json!({ "v": 1, "client_id": "123456789012345678" });
        let bytes = encode(HANDSHAKE, &hello);
        let body = br#"{"client_id":"123456789012345678","v":1}"#;
        assert_eq!(
            body.len(),
            40,
            "Discord's own example says 45: counted here"
        );
        assert_eq!(&bytes[..4], &[0, 0, 0, 0]);
        assert_eq!(&bytes[4..8], &[40, 0, 0, 0]);
        assert_eq!(&bytes[8..], body);
        assert_eq!(&encode(PONG, &json!({}))[..8], &[4, 0, 0, 0, 2, 0, 0, 0]);
    }

    #[test]
    fn what_is_shown_follows_the_game_the_buffer_and_the_recording() {
        let s = shown(want(&facts()));
        assert_eq!(s.doing, Doing::Clipping);
        assert_eq!(
            s.since, 1_700_000_000,
            "the time counts from the game's start"
        );
        assert_eq!(s.words(), "Starfall Tactics, Clipping");
        let rec = shown(want(&Facts {
            recording: Some(1_700_000_600),
            ..facts()
        }));
        assert_eq!(rec.words(), "Starfall Tactics, Recording");
        assert_eq!(
            rec.since, 1_700_000_600,
            "and from the recording's while it runs"
        );
        let held = shown(want(&Facts {
            replay_buffer: false,
            recording: Some(5),
            ..facts()
        }));
        assert_eq!(
            held.doing,
            Doing::Recording,
            "a recording without the buffer"
        );
        assert_eq!(
            want(&Facts {
                replay_buffer: false,
                ..facts()
            }),
            Want::Nothing("Starfall Tactics runs without the replay buffer".into())
        );
        assert_eq!(
            want(&Facts {
                game: None,
                recording: Some(5),
                ..facts()
            }),
            Want::Nothing("no game runs".into()),
            "a recording without a game shows nothing"
        );
        assert_eq!(
            want(&Facts {
                show: false,
                ..facts()
            }),
            Want::Off
        );
    }

    #[test]
    fn the_activity_names_the_game_and_keeps_it_in_the_member_list() {
        let a = shown(want(&facts())).activity();
        assert_eq!(
            a,
            json!({
                "type": 0,
                "details": "Starfall Tactics",
                "state": "Clipping",
                "status_display_type": 2,
                "timestamps": { "start": 1_700_000_000u64 },
                "assets": { "large_image": "reel", "large_text": "reel" },
                "buttons": [{ "label": "reel on GitHub", "url": "https://github.com/Locko2901/reel" }],
            })
        );
        let bare = shown(want(&Facts {
            name_game: false,
            button: false,
            ..facts()
        }));
        assert_eq!(bare.words(), "Clipping");
        let a = bare.activity();
        assert!(a.get("details").is_none() && a.get("status_display_type").is_none());
        assert!(a.get("buttons").is_none());
        assert_eq!(a["state"], "Clipping");
    }

    #[test]
    fn a_name_discord_would_refuse_is_left_out_or_cut() {
        let one = shown(want(&Facts {
            game: Some(("X".into(), 1)),
            ..facts()
        }));
        assert_eq!(one.game, None, "Discord wants two characters at least");
        let long = "Starfall ".repeat(20);
        let cut = shown(want(&Facts {
            game: Some((long, 1)),
            ..facts()
        }));
        let g = cut.game.unwrap();
        assert!(g.chars().count() <= 128 && !g.ends_with(' '), "{g}");
    }

    #[test]
    fn a_change_is_one_discord_hears_of_and_a_second_of_jitter_is_not() {
        let a = shown(want(&facts()));
        let mut b = a.clone();
        b.since += 1;
        assert!(!a.differs(&b));
        b.since += 60;
        assert!(a.differs(&b), "the game ran again");
        let mut c = a.clone();
        c.doing = Doing::Recording;
        assert!(a.differs(&c));
        assert!(Want::Off.differs(&Want::Nothing("no game runs".into())));
        assert!(!Want::Off.differs(&Want::Off));
    }

    #[cfg(not(windows))]
    #[test]
    fn the_socket_is_looked_for_where_discord_its_flatpak_and_its_snap_put_it() {
        let env = |k: &str| match k {
            "XDG_RUNTIME_DIR" => Some("/run/user/1000".to_string()),
            "TMPDIR" => Some("/tmp".to_string()),
            _ => None,
        };
        let at = endpoints(env);
        let first: Vec<String> = at[..6].iter().map(|p| p.display().to_string()).collect();
        assert_eq!(
            first,
            [
                "/run/user/1000/discord-ipc-0",
                "/run/user/1000/app/com.discordapp.Discord/discord-ipc-0",
                "/run/user/1000/snap.discord/discord-ipc-0",
                "/tmp/discord-ipc-0",
                "/tmp/app/com.discordapp.Discord/discord-ipc-0",
                "/tmp/snap.discord/discord-ipc-0",
            ]
        );
        assert_eq!(at.len(), 60, "0 to 9 in each of the six places");
        assert!(at.last().unwrap().ends_with("snap.discord/discord-ipc-9"));
    }

    #[cfg(not(windows))]
    mod standin {
        use super::super::*;
        use std::sync::{Arc, Mutex};
        use tokio::net::UnixListener;

        /// A stand-in Discord: READY to the handshake, each command kept and answered with its
        /// nonce, an ERROR for an activity whose state is "Refuse", and a close when told.
        struct Standin {
            got: Arc<Mutex<Vec<Value>>>,
            close: Option<tokio::sync::oneshot::Sender<()>>,
        }

        fn start(dir: &Path, refuse_handshake: bool) -> (PathBuf, Standin) {
            let path = dir.join("discord-ipc-0");
            let listener = UnixListener::bind(&path).unwrap();
            let got = Arc::new(Mutex::new(Vec::new()));
            let (close, closed) = tokio::sync::oneshot::channel::<()>();
            let kept = got.clone();
            tokio::spawn(async move {
                let (mut s, _) = listener.accept().await.unwrap();
                let (op, hello) = read_frame(&mut s).await.unwrap();
                assert_eq!(op, HANDSHAKE);
                kept.lock().unwrap().push(hello);
                if refuse_handshake {
                    let bye = json!({ "code": 4000, "message": "Invalid Client ID" });
                    s.write_all(&encode(CLOSE, &bye)).await.unwrap();
                    return;
                }
                let ready = json!({ "cmd": "DISPATCH", "evt": "READY", "data": { "v": 1 } });
                s.write_all(&encode(FRAME, &ready)).await.unwrap();
                let mut closed = closed;
                loop {
                    let frame = tokio::select! {
                        f = read_frame(&mut s) => f,
                        _ = &mut closed => return,
                    };
                    let Ok((op, cmd)) = frame else { return };
                    if op == PONG {
                        continue;
                    }
                    kept.lock().unwrap().push(cmd.clone());
                    // a PING first, as Discord may send one at any time
                    s.write_all(&encode(PING, &json!({ "n": 1 })))
                        .await
                        .unwrap();
                    let refuse = cmd["args"]["activity"]["state"] == "Refuse";
                    let answer = if refuse {
                        json!({ "cmd": "SET_ACTIVITY", "evt": "ERROR", "nonce": cmd["nonce"],
                                "data": { "code": 4000, "message": "child \"state\" fails" } })
                    } else {
                        json!({ "cmd": "SET_ACTIVITY", "nonce": cmd["nonce"], "data": cmd["args"]["activity"] })
                    };
                    s.write_all(&encode(FRAME, &answer)).await.unwrap();
                }
            });
            (
                path,
                Standin {
                    got,
                    close: Some(close),
                },
            )
        }

        #[tokio::test]
        async fn reel_says_hello_sets_the_activity_clears_it_and_hears_a_refusal() {
            let dir = tempfile::tempdir().unwrap();
            let (path, standin) = start(dir.path(), false);
            let missing = dir.path().join("discord-ipc-1");
            let (mut conn, at) = connect(&[missing, path.clone()], APPLICATION_ID)
                .await
                .unwrap();
            assert_eq!(at, path);
            let shown = want(&Facts {
                show: true,
                name_game: true,
                button: true,
                game: Some(("Starfall Tactics".into(), 1_700_000_000)),
                replay_buffer: true,
                recording: None,
            });
            let Want::Show(s) = shown else { panic!() };
            conn.set_activity(Some(&s.activity())).await.unwrap();
            conn.set_activity(None).await.unwrap();
            let mut bad = s.activity();
            bad["state"] = json!("Refuse");
            let refused = conn.set_activity(Some(&bad)).await.unwrap_err();
            assert_eq!(
                refused.to_string(),
                "Discord refused it: child \"state\" fails (4000)"
            );
            let got = standin.got.lock().unwrap().clone();
            assert_eq!(got[0], json!({ "v": 1, "client_id": APPLICATION_ID }));
            assert_eq!(got[1]["cmd"], "SET_ACTIVITY");
            assert_eq!(got[1]["args"]["pid"], std::process::id());
            assert_eq!(got[1]["args"]["activity"]["details"], "Starfall Tactics");
            assert_eq!(got[2]["args"]["activity"], Value::Null, "None clears it");
            assert_ne!(got[1]["nonce"], got[2]["nonce"]);
        }

        #[tokio::test]
        async fn a_discord_that_goes_away_is_heard_of() {
            let dir = tempfile::tempdir().unwrap();
            let (path, mut standin) = start(dir.path(), false);
            let (mut conn, _) = connect(&[path], APPLICATION_ID).await.unwrap();
            standin.close.take().unwrap().send(()).unwrap();
            let gone = tokio::time::timeout(Duration::from_secs(5), conn.next())
                .await
                .unwrap();
            assert!(matches!(gone, Incoming::Closed(_)), "{gone:?}");
            assert!(conn.set_activity(None).await.is_err());
        }

        #[tokio::test]
        async fn no_discord_a_stale_socket_and_a_refused_hello() {
            let dir = tempfile::tempdir().unwrap();
            let nothing = connect(&[dir.path().join("discord-ipc-0")], APPLICATION_ID).await;
            assert!(matches!(nothing, Err(Error::NotRunning)));
            let stale = dir.path().join("stale");
            std::fs::create_dir(&stale).unwrap();
            drop(UnixListener::bind(stale.join("discord-ipc-0")).unwrap());
            let left = connect(&[stale.join("discord-ipc-0")], APPLICATION_ID).await;
            assert!(
                matches!(left, Err(Error::NotRunning)),
                "a socket a crashed Discord left behind"
            );
            let (path, _standin) = start(dir.path(), true);
            let refused = connect(&[path], "1").await.err().unwrap();
            assert_eq!(
                refused.to_string(),
                "Discord refused it: Invalid Client ID (4000)"
            );
        }
    }

    #[cfg(not(windows))]
    mod task {
        use super::super::*;
        use std::sync::{Arc, Mutex};
        use tokio::io::AsyncWriteExt;
        use tokio::net::UnixListener;

        #[derive(Debug, Clone, PartialEq)]
        enum Seen {
            Hello,
            Set(Value),
            Closed,
        }

        type Log = Arc<Mutex<Vec<(Instant, Seen)>>>;

        /// A stand-in Discord that takes any number of connections and keeps what each sent;
        /// it refuses an activity for the game "Refused Game".
        fn discord(path: &Path) -> Log {
            let listener = UnixListener::bind(path).unwrap();
            let log: Log = Arc::default();
            let kept = log.clone();
            tokio::spawn(async move {
                loop {
                    let (mut s, _) = listener.accept().await.unwrap();
                    let kept = kept.clone();
                    tokio::spawn(async move {
                        let note = |seen| kept.lock().unwrap().push((Instant::now(), seen));
                        if read_frame(&mut s).await.is_err() {
                            return;
                        }
                        note(Seen::Hello);
                        let ready = json!({ "cmd": "DISPATCH", "evt": "READY" });
                        s.write_all(&encode(FRAME, &ready)).await.unwrap();
                        while let Ok((op, cmd)) = read_frame(&mut s).await {
                            if op != FRAME {
                                continue;
                            }
                            let activity = cmd["args"]["activity"].clone();
                            note(Seen::Set(activity.clone()));
                            let answer = if activity["details"] == "Refused Game" {
                                json!({ "evt": "ERROR", "nonce": cmd["nonce"], "data": { "message": "no" } })
                            } else {
                                json!({ "cmd": "SET_ACTIVITY", "nonce": cmd["nonce"], "data": activity })
                            };
                            if s.write_all(&encode(FRAME, &answer)).await.is_err() {
                                break;
                            }
                        }
                        note(Seen::Closed);
                    });
                }
            });
            log
        }

        fn show(game: &str) -> Want {
            Want::Show(Shown {
                doing: Doing::Clipping,
                game: Some(game.into()),
                since: 1_700_000_000,
                button: true,
            })
        }

        async fn wait_for(what: &str, ok: impl Fn() -> bool) {
            for _ in 0..300 {
                if ok() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("no {what} within 3 s");
        }

        fn sets(log: &Log) -> Vec<(Instant, Value)> {
            log.lock()
                .unwrap()
                .iter()
                .filter_map(|(t, s)| match s {
                    Seen::Set(v) => Some((*t, v.clone())),
                    _ => None,
                })
                .collect()
        }

        fn start(path: PathBuf) -> (watch::Sender<Want>, Arc<Mutex<Vec<Report>>>) {
            let (tx, rx) = watch::channel(Want::Off);
            let reports: Arc<Mutex<Vec<Report>>> = Arc::default();
            let kept = reports.clone();
            let timing = Timing {
                every: Duration::from_millis(400),
                retry: Duration::from_millis(300),
            };
            tokio::spawn(run(
                rx,
                move || vec![path.clone()],
                timing,
                move |r| kept.lock().unwrap().push(r),
            ));
            (tx, reports)
        }

        #[tokio::test]
        async fn one_send_per_interval_the_last_state_wins_and_nothing_clears_and_closes() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("discord-ipc-0");
            let log = discord(&path);
            let (want, reports) = start(path);
            want.send_replace(show("Ember Pass"));
            wait_for("first set", || sets(&log).len() == 1).await;
            want.send_replace(show("Cinder Road"));
            want.send_replace(show("Starfall Tactics"));
            wait_for("second set", || sets(&log).len() == 2).await;
            let got = sets(&log);
            assert_eq!(got[0].1["details"], "Ember Pass");
            assert_eq!(
                got[1].1["details"], "Starfall Tactics",
                "the last state wins"
            );
            assert!(
                got[1].0 - got[0].0 >= Duration::from_millis(350),
                "{:?} apart",
                got[1].0 - got[0].0
            );
            let mut same = show("Starfall Tactics");
            if let Want::Show(s) = &mut same {
                s.since += 1;
            }
            want.send_replace(same);
            want.send_replace(Want::Nothing("no game runs".into()));
            wait_for("the clear", || sets(&log).len() == 3).await;
            assert_eq!(
                sets(&log)[2].1,
                Value::Null,
                "a second of jitter sent nothing"
            );
            wait_for("the close", || {
                log.lock().unwrap().last().map(|(_, s)| s.clone()) == Some(Seen::Closed)
            })
            .await;
            assert_eq!(
                reports.lock().unwrap().last(),
                Some(&Report::Quiet(Want::Nothing("no game runs".into())))
            );
            let hellos = log
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, s)| *s == Seen::Hello)
                .count();
            assert_eq!(hellos, 1, "one connection for all of it");
        }

        #[tokio::test]
        async fn a_discord_that_starts_later_gets_it_and_a_refusal_is_reported() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("discord-ipc-0");
            let (want, reports) = start(path.clone());
            want.send_replace(show("Ember Pass"));
            wait_for("not running", || {
                reports.lock().unwrap().contains(&Report::NotRunning)
            })
            .await;
            let log = discord(&path);
            wait_for("the set once Discord is there", || sets(&log).len() == 1).await;
            wait_for("sent", || {
                matches!(reports.lock().unwrap().last(), Some(Report::Sent(_)))
            })
            .await;
            want.send_replace(show("Refused Game"));
            wait_for("refused", || {
                reports.lock().unwrap().last() == Some(&Report::Refused("no".into()))
            })
            .await;
        }
    }
}
