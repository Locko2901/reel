//! Downloads in the window, shown by the page's toast: the webview itself shows nothing.
use crate::shell;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// How often the page hears of a download's bytes.
const EVERY: Duration = Duration::from_millis(250);

/// The downloads of this run, by the number the page knows each by.
#[derive(Default)]
pub struct Downloads(Mutex<Book>);

#[derive(Default)]
struct Book {
    next: u64,
    list: HashMap<u64, Entry>,
}

struct Entry {
    path: PathBuf,
    total: Option<u64>,
    told: Instant,
    state: State,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    Running,
    Done,
    Failed,
}

/// What the page is told of a download.
#[derive(Serialize, Debug, PartialEq)]
pub struct Change {
    id: u64,
    state: &'static str,
    name: String,
    folder: String,
    total: Option<u64>,
    got: u64,
    reason: Option<String>,
    openable: bool,
}

/// The files Open may hand to the system: a clip or a picture, never a program.
pub fn openable(path: &Path) -> bool {
    const MEDIA: [&str; 9] = [
        "mp4", "mkv", "mov", "webm", "m4v", "png", "jpg", "jpeg", "webp",
    ];
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA.contains(&e.to_ascii_lowercase().as_str()))
}

impl Book {
    fn change(&self, id: u64, got: u64, reason: Option<&str>) -> Option<Change> {
        let e = self.list.get(&id)?;
        let name = |p: &Path| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        Some(Change {
            id,
            state: match e.state {
                State::Running => "running",
                State::Done => "done",
                State::Failed => "failed",
            },
            name: name(&e.path),
            folder: e.path.parent().map(name).unwrap_or_default(),
            total: e.total,
            got,
            reason: reason.map(String::from),
            openable: openable(&e.path),
        })
    }

    fn started(&mut self, path: PathBuf, total: Option<u64>) -> (u64, Change) {
        self.next += 1;
        let id = self.next;
        self.list.insert(
            id,
            Entry {
                path,
                total,
                told: Instant::now(),
                state: State::Running,
            },
        );
        (id, self.change(id, 0, None).expect("just put in"))
    }

    fn progress(&mut self, id: u64, got: u64) -> Option<Change> {
        let e = self.list.get_mut(&id)?;
        if e.state != State::Running || e.told.elapsed() < EVERY {
            return None;
        }
        e.told = Instant::now();
        self.change(id, got, None)
    }

    /// The end, once: WebKitGTK says a failed download finished too.
    fn ended(&mut self, id: u64, path: Option<PathBuf>, reason: Option<&str>) -> Option<Change> {
        let e = self.list.get_mut(&id)?;
        if e.state != State::Running {
            return None;
        }
        e.state = if reason.is_some() {
            State::Failed
        } else {
            State::Done
        };
        if let Some(p) = path.filter(|p| !p.as_os_str().is_empty()) {
            e.path = p;
        }
        let got = e.total.unwrap_or(0);
        self.change(id, got, reason)
    }

    fn saved(&self, id: u64) -> Option<PathBuf> {
        self.list
            .get(&id)
            .filter(|e| e.state == State::Done)
            .map(|e| e.path.clone())
    }
}

fn book(app: &AppHandle) -> Option<std::sync::MutexGuard<'_, Book>> {
    let d = app.try_state::<Downloads>()?.inner();
    Some(d.0.lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn started(app: &AppHandle, path: PathBuf, total: Option<u64>) -> u64 {
    tracing::info!(path = %path.display(), total = ?total, "download started");
    let Some((id, change)) = book(app).map(|mut b| b.started(path, total)) else {
        return 0;
    };
    tell(app, &change, true);
    id
}

pub fn progress(app: &AppHandle, id: u64, got: u64) {
    if let Some(change) = book(app).and_then(|mut b| b.progress(id, got)) {
        tell(app, &change, false);
    }
}

pub fn finished(app: &AppHandle, id: u64, path: Option<PathBuf>) {
    if let Some(change) = book(app).and_then(|mut b| b.ended(id, path, None)) {
        tracing::info!(id, name = %change.name, folder = %change.folder, "download saved");
        tell(app, &change, true);
    }
}

pub fn failed(app: &AppHandle, id: u64, reason: &str) {
    if let Some(change) = book(app).and_then(|mut b| b.ended(id, None, Some(reason))) {
        tracing::warn!(id, name = %change.name, reason, "download failed");
        tell(app, &change, true);
    }
}

/// A file this run downloaded: the only files a reel page may show or open.
pub fn saved(app: &AppHandle, id: u64) -> Option<PathBuf> {
    book(app).and_then(|b| b.saved(id))
}

/// Tells the page on screen; what its toast shows comes back for the log.
fn tell(app: &AppHandle, change: &Change, log: bool) {
    let Some(wv) = app.get_webview(shell::LIBRARY) else {
        return;
    };
    let Ok(json) = serde_json::to_string(change) else {
        return;
    };
    let js = format!("window.__reelDownload ? window.__reelDownload({json}) : ''");
    if log {
        let id = change.id;
        let _ = wv.eval_with_callback(js, move |shown| {
            tracing::info!(id, shown = %shown.trim_matches('"'), "download shown in the page");
        });
    } else {
        let _ = wv.eval(js);
    }
}

#[cfg(target_os = "linux")]
pub fn attach(webview: &tauri::Webview) {
    use std::cell::Cell;
    use std::rc::Rc;
    use webkit2gtk::{DownloadExt, URIResponseExt, WebContextExt, WebViewExt};
    let app = webview.app_handle().clone();
    let _ = webview.with_webview(move |pv| {
        let Some(context) = pv.inner().context() else {
            return;
        };
        context.connect_download_started(move |_, download| {
            let id = Rc::new(Cell::new(None::<u64>));
            let begin = {
                let (app, id) = (app.clone(), id.clone());
                move |d: &webkit2gtk::Download| match id.get() {
                    Some(i) => i,
                    None => {
                        let path = d.destination().map(|s| local(&s)).unwrap_or_default();
                        let total = d.response().map(|r| r.content_length()).filter(|n| *n > 0);
                        let i = started(&app, path, total);
                        id.set(Some(i));
                        i
                    }
                }
            };
            let begin = Rc::new(begin);
            download.connect_created_destination({
                let begin = begin.clone();
                move |d, _| {
                    begin(d);
                }
            });
            download.connect_received_data({
                let (app, begin) = (app.clone(), begin.clone());
                move |d, _| progress(&app, begin(d), d.received_data_length())
            });
            download.connect_failed({
                let (app, begin) = (app.clone(), begin.clone());
                move |d, e| failed(&app, begin(d), e.message())
            });
            download.connect_finished({
                let (app, begin) = (app.clone(), begin.clone());
                move |d| {
                    let path = d.destination().map(|s| local(&s));
                    finished(&app, begin(d), path)
                }
            });
        });
    });
}

/// A destination as WebKitGTK gives it, a path or a file URI.
#[cfg(target_os = "linux")]
fn local(s: &str) -> PathBuf {
    url::Url::parse(s)
        .ok()
        .filter(|u| u.scheme() == "file")
        .and_then(|u| u.to_file_path().ok())
        .unwrap_or_else(|| PathBuf::from(s))
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn attach(_webview: &tauri::Webview) {}

/// WebView2's download events, beside wry's own handler.
#[cfg(windows)]
pub fn attach(webview: &tauri::Webview) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_4, COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON, COREWEBVIEW2_DOWNLOAD_STATE,
        COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED, COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED,
    };
    use webview2_com::{
        take_pwstr, BytesReceivedChangedEventHandler, DownloadStartingEventHandler,
        StateChangedEventHandler,
    };
    use windows::core::{Interface, PWSTR};
    let app = webview.app_handle().clone();
    let _ = webview.with_webview(move |pv| {
        let heard = unsafe {
            pv.controller()
                .CoreWebView2()
                .and_then(|core| core.cast::<ICoreWebView2_4>())
                .and_then(|core| {
                    let handler = DownloadStartingEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        let op = args.DownloadOperation()?;
                        let mut p = PWSTR::null();
                        args.ResultFilePath(&mut p)?;
                        let path = PathBuf::from(take_pwstr(p));
                        let mut total = 0i64;
                        op.TotalBytesToReceive(&mut total)?;
                        let id = started(&app, path, u64::try_from(total).ok().filter(|n| *n > 0));
                        let a = app.clone();
                        op.add_BytesReceivedChanged(
                            &BytesReceivedChangedEventHandler::create(Box::new(move |op, _| {
                                if let Some(op) = op {
                                    let mut got = 0i64;
                                    op.BytesReceived(&mut got)?;
                                    progress(&a, id, u64::try_from(got).unwrap_or(0));
                                }
                                Ok(())
                            })),
                            &mut 0,
                        )?;
                        let a = app.clone();
                        op.add_StateChanged(
                            &StateChangedEventHandler::create(Box::new(move |op, _| {
                                let Some(op) = op else {
                                    return Ok(());
                                };
                                let mut state = COREWEBVIEW2_DOWNLOAD_STATE::default();
                                op.State(&mut state)?;
                                if state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED {
                                    let mut p = PWSTR::null();
                                    op.ResultFilePath(&mut p)?;
                                    finished(&a, id, Some(PathBuf::from(take_pwstr(p))));
                                } else if state == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
                                    let mut why = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
                                    op.InterruptReason(&mut why)?;
                                    failed(&a, id, interrupted(why.0));
                                }
                                Ok(())
                            })),
                            &mut 0,
                        )?;
                        Ok(())
                    }));
                    core.add_DownloadStarting(&handler, &mut 0)
                })
        };
        if let Err(e) = heard {
            tracing::warn!(error = %e, "the window's downloads will not show in the page");
        }
    });
}

/// A COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON in a few words.
#[cfg(windows)]
fn interrupted(reason: i32) -> &'static str {
    match reason {
        2 => "the folder refused the file",
        3 => "the disk is full",
        1 | 4 | 5 | 7 | 10 => "the file could not be written",
        6 | 8 | 9 | 11 => "Windows' security check blocked the file",
        12..=16 => "the connection to the server broke",
        20 | 22 => "the server refused it; sign in again",
        17..=25 => "the server did not hand the file over",
        26 => "it was cancelled",
        27 => "the app closed",
        _ => "the webview stopped it",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_is_told_as_it_starts_goes_and_ends_once() {
        let mut b = Book::default();
        let (id, start) = b.started(
            PathBuf::from("/home/u/Downloads/Clip.mp4"),
            Some(120_000_000),
        );
        assert_eq!(
            (
                start.state,
                start.name.as_str(),
                start.folder.as_str(),
                start.total
            ),
            ("running", "Clip.mp4", "Downloads", Some(120_000_000))
        );
        assert_eq!(b.progress(id, 1), None, "not again within EVERY");
        b.list.get_mut(&id).unwrap().told -= EVERY;
        assert_eq!(b.progress(id, 60_000_000).map(|c| c.got), Some(60_000_000));
        assert_eq!(b.saved(id), None, "nothing to show while it runs");
        let end = b
            .ended(
                id,
                Some(PathBuf::from("/home/u/Downloads/Clip (1).mp4")),
                None,
            )
            .unwrap();
        assert_eq!((end.state, end.name.as_str()), ("done", "Clip (1).mp4"));
        assert_eq!(
            b.ended(id, None, Some("late")),
            None,
            "the end is told once"
        );
        assert_eq!(
            b.saved(id),
            Some(PathBuf::from("/home/u/Downloads/Clip (1).mp4"))
        );
        assert_eq!(b.saved(id + 1), None, "only a file this run downloaded");
        assert!(end.openable);
        assert!(!openable(Path::new("/home/u/Downloads/reel_setup.exe")));
        assert!(!openable(Path::new("/home/u/Downloads/run.MP4.sh")));
        assert!(openable(Path::new("/home/u/Downloads/Clip.MKV")));
    }

    #[test]
    fn a_failed_download_says_why_and_shows_no_file() {
        let mut b = Book::default();
        let (id, _) = b.started(PathBuf::from("/tmp/d/a.mp4"), None);
        let end = b.ended(id, None, Some("Permission denied")).unwrap();
        assert_eq!(
            (end.state, end.reason.as_deref()),
            ("failed", Some("Permission denied"))
        );
        assert_eq!(b.ended(id, Some(PathBuf::from("/tmp/d/a.mp4")), None), None);
        assert_eq!(b.saved(id), None);
    }
}
