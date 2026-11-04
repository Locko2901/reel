//! The WebView2 arguments and data folder for each way the webview may play sound on Windows.
use crate::settings::WebviewAudio;
use std::path::Path;
use std::time::Duration;

/// wry's own arguments (a builder's arguments replace them) with Chromium's audio service
/// moved into the browser process.
const IN_BROWSER: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,AudioServiceOutOfProcess --autoplay-policy=no-user-gesture-required";

/// Beside the usual `EBWebView`: one folder takes one set of arguments at a time, and the
/// default's browser process may still be closing.
pub const FALLBACK_FOLDER: &str = "webview-fallback";

/// A folder a test build left behind, removed when found.
pub const LEFT_OVER: &str = "browser-audio";

/// How long the first window waits for an older WebView2 to let the usual folder go.
pub const WAIT: Duration = Duration::from_secs(8);
pub const EVERY: Duration = Duration::from_millis(500);

/// The arguments the webview gets for a way of playing sound; none is wry's own.
pub fn args(audio: WebviewAudio) -> Option<&'static str> {
    match audio {
        WebviewAudio::Browser => Some(IN_BROWSER),
        WebviewAudio::Service => None,
    }
}

/// None is the usual `EBWebView` folder, where the login is.
pub fn folder(audio: WebviewAudio) -> Option<&'static str> {
    match audio {
        WebviewAudio::Browser => None,
        WebviewAudio::Service => Some(FALLBACK_FOLDER),
    }
}

/// Whether the folder was there; an error while a file in it is still open.
pub fn remove_left_over(local: &Path) -> std::io::Result<bool> {
    let dir = local.join(LEFT_OVER);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

pub fn retry<T, E>(
    tries: u32,
    pause: impl Fn(),
    mut f: impl FnMut(u32) -> Result<T, E>,
) -> Result<T, E> {
    let mut n = 1;
    loop {
        match f(n) {
            Ok(v) => return Ok(v),
            Err(e) if n >= tries => return Err(e),
            Err(_) => {
                pause();
                n += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn the_default_plays_from_the_browser_process_in_the_usual_folder() {
        let browser = args(WebviewAudio::default()).unwrap();
        assert_eq!(WebviewAudio::default(), WebviewAudio::Browser);
        assert_eq!(folder(WebviewAudio::Browser), None, "the login's folder");
        assert!(browser.contains("AudioServiceOutOfProcess"));
        for wry in [
            "msWebOOUI",
            "msPdfOOUI",
            "msSmartScreenProtection",
            "--autoplay-policy=",
        ] {
            assert!(browser.contains(wry), "wry's own {wry} is kept");
        }
        assert_eq!(
            browser.matches("--disable-features").count(),
            1,
            "Chromium keeps only the last of two"
        );
        assert_eq!(args(WebviewAudio::Service), None, "wry's own arguments");
        assert_eq!(folder(WebviewAudio::Service), Some(FALLBACK_FOLDER));
        assert_ne!(FALLBACK_FOLDER, LEFT_OVER);
    }

    #[test]
    fn the_test_builds_folder_goes_and_the_others_stay() {
        let local = tempfile::tempdir().unwrap();
        let usual = local.path().join("EBWebView");
        std::fs::create_dir_all(usual.join("Default")).unwrap();
        std::fs::write(usual.join("Default/Cookies"), b"1.2.0's login").unwrap();
        let left = local.path().join(LEFT_OVER).join("EBWebView/Default");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(left.join("Cookies"), b"the test build's login").unwrap();
        std::fs::create_dir_all(local.path().join(FALLBACK_FOLDER)).unwrap();
        assert!(remove_left_over(local.path()).unwrap());
        assert!(!local.path().join(LEFT_OVER).exists());
        assert!(usual.join("Default/Cookies").exists(), "the login stays");
        assert!(local.path().join(FALLBACK_FOLDER).exists());
        assert!(!remove_left_over(local.path()).unwrap(), "gone already");
    }

    #[test]
    fn the_wait_tries_until_it_works_and_gives_the_last_error_up() {
        let pauses = Cell::new(0);
        let got = retry(
            16,
            || pauses.set(pauses.get() + 1),
            |n| {
                if n < 3 {
                    Err(format!("held, try {n}"))
                } else {
                    Ok(n)
                }
            },
        );
        assert_eq!(got, Ok(3));
        assert_eq!(pauses.get(), 2);
        let pauses = Cell::new(0);
        let got: Result<u32, String> = retry(
            4,
            || pauses.set(pauses.get() + 1),
            |n| Err(format!("held, try {n}")),
        );
        assert_eq!(got, Err("held, try 4".to_string()));
        assert_eq!(pauses.get(), 3, "no pause after the last try");
    }
}
