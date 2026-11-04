//! The global shortcuts through the plugin, on Windows and X11; Wayland uses portal.rs.
use tauri::{AppHandle, Manager, State, Wry};
use tauri_plugin_global_shortcut::{Code, GlobalShortcut, Modifiers, Shortcut, ShortcutState};

/// Whether the plugin's key grabs reach the whole desktop; on Wayland they would reach X
/// windows alone.
pub fn grabs() -> bool {
    #[cfg(target_os = "linux")]
    {
        crate::linux::session() == crate::linux::Session::X11
            && std::env::var_os("DISPLAY").is_some()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

fn manager(app: &AppHandle) -> Option<State<'_, GlobalShortcut<Wry>>> {
    app.try_state::<GlobalShortcut<Wry>>()
}

pub use reel_desktop_core::settings::HotkeyAction as Action;

/// Ctrl+, opens the settings while the window has the focus. On Linux it stays registered:
/// the library's unregister waits on the main thread for the next X event and freezes the app.
pub fn settings_shortcut(app: &AppHandle, focused: bool) {
    if (cfg!(target_os = "linux") && !focused) || manager(app).is_none() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(gs) = manager(&app) else {
            return;
        };
        let sc: Shortcut = match "CmdOrCtrl+Comma".parse() {
            Ok(s) => s,
            Err(_) => return,
        };
        if focused {
            if gs.is_registered(sc) {
                return;
            }
            let _ = gs.on_shortcut(sc, |app, _sc, event| {
                if event.state == ShortcutState::Pressed {
                    tracing::info!("settings opened by the shortcut");
                    crate::shell::show(app, crate::shell::View::Settings, None);
                }
            });
        } else if gs.is_registered(sc) {
            let _ = gs.unregister(sc);
        }
    });
}

/// Registers the shortcuts; an empty string means none. Returns one line per problem.
pub fn register(
    app: &AppHandle,
    keys: &[(&str, Action)],
    on: impl Fn(&AppHandle, Action) + Send + Sync + 'static,
) -> Vec<String> {
    let Some(gs) = manager(app) else {
        return vec!["no X display for the hotkeys: bind the command line instead".into()];
    };
    let _ = gs.unregister_all();
    let focused = app
        .get_window(crate::shell::MAIN)
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    settings_shortcut(app, focused);
    let mut problems = Vec::new();
    let on = std::sync::Arc::new(on);
    for &(text, action) in keys {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let sc: Shortcut = match text.parse() {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("{text}: not a shortcut ({e})"));
                continue;
            }
        };
        let cb = on.clone();
        if let Err(e) = gs.on_shortcut(sc, move |app, _sc, event| {
            if event.state == ShortcutState::Pressed {
                cb(app, action);
            }
        }) {
            problems.push(format!("{text}: {e}"));
        }
    }
    problems
}

/// A key that types nothing and moves no caret, so a hotkey on it needs no modifier.
pub fn types_nothing(key: Code) -> bool {
    use Code::*;
    matches!(
        key,
        F1 | F2
            | F3
            | F4
            | F5
            | F6
            | F7
            | F8
            | F9
            | F10
            | F11
            | F12
            | F13
            | F14
            | F15
            | F16
            | F17
            | F18
            | F19
            | F20
            | F21
            | F22
            | F23
            | F24
            | Pause
            | ScrollLock
            | Insert
            | PrintScreen
            | MediaPlay
            | MediaPause
            | MediaPlayPause
            | MediaStop
            | MediaTrackNext
            | MediaTrackPrevious
            | AudioVolumeUp
            | AudioVolumeDown
            | AudioVolumeMute
    )
}

/// Why a key that types or moves the caret needs Ctrl, Alt or the Windows key.
pub fn needs_modifier(name: &str, key: Code) -> String {
    use Code::*;
    let does = if matches!(
        key,
        ArrowUp | ArrowDown | ArrowLeft | ArrowRight | Home | End | PageUp | PageDown
    ) {
        "moves the caret"
    } else {
        "types"
    };
    format!("F8 is fine alone; {name} {does}, so it needs Ctrl, Alt or Win")
}

fn parsed(text: &str) -> Option<Shortcut> {
    text.trim().parse().ok()
}

/// A recorded combination before anything is registered: `None` when cleared or unchanged,
/// a new one to try, or why it is refused.
pub fn judge(text: &str, mine: &str, others: &[(&str, &str)]) -> Result<Option<Shortcut>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let Some(sc) = parsed(text) else {
        return Err(format!("{text} is not a key combination the app can take"));
    };
    if !types_nothing(sc.key)
        && !sc
            .mods
            .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        let name = text.rsplit('+').next().unwrap_or(text).trim();
        return Err(needs_modifier(name, sc.key));
    }
    if parsed("CmdOrCtrl+Comma") == Some(sc) {
        return Err(format!("{text} opens the app's settings"));
    }
    if let Some((label, _)) = others.iter().find(|(_, o)| parsed(o) == Some(sc)) {
        return Err(format!("{text} is already {label}"));
    }
    if parsed(mine) == Some(sc) {
        return Ok(None);
    }
    Ok(Some(sc))
}

/// Registers a recorded combination once and lets it go, so one another app holds is refused;
/// one the app will not register (`trial` false) is judged alone.
pub fn try_combo(
    app: &AppHandle,
    text: &str,
    mine: &str,
    others: &[(&str, &str)],
    trial: bool,
) -> Result<String, String> {
    let text = text.trim();
    let Some(sc) = judge(text, mine, others)? else {
        return Ok(text.to_string());
    };
    if !trial {
        return Ok(text.to_string());
    }
    let Some(gs) = manager(app) else {
        return Ok(text.to_string());
    };
    if gs.is_registered(sc) {
        return Ok(text.to_string());
    }
    let system = if cfg!(windows) {
        "Windows"
    } else {
        "the desktop"
    };
    match gs.register(sc) {
        Ok(()) => {
            let _ = gs.unregister(sc);
            Ok(text.to_string())
        }
        Err(_) if mine.trim().is_empty() => Err(format!(
            "{text} is held by {system} or another app; this stays without a hotkey"
        )),
        Err(_) => Err(format!(
            "{text} is held by {system} or another app; {} stays",
            mine.trim()
        )),
    }
}

/// Turns the hotkeys off while a field listens, so the keys pressed reach the field.
pub fn pause(app: &AppHandle) {
    if let Some(gs) = manager(app) {
        let _ = gs.unregister_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OTHERS: &[(&str, &str)] = &[
        ("Share the last clip", "Ctrl+Alt+S"),
        ("Upload the last clip", "Ctrl+Alt+U"),
    ];

    #[test]
    fn keys_that_type_nothing_need_no_modifier_and_the_others_say_why() {
        for alone in [
            "F1",
            "F8",
            "Shift+F8",
            "F24",
            "Pause",
            "Shift+ScrollLock",
            "Insert",
            "PrintScreen",
            "MediaPlayPause",
            "MediaTrackNext",
            "AudioVolumeMute",
        ] {
            assert!(judge(alone, "", OTHERS).unwrap().is_some(), "{alone}");
        }
        let why = |t: &str| judge(t, "", OTHERS).unwrap_err();
        assert_eq!(
            why("S"),
            "F8 is fine alone; S types, so it needs Ctrl, Alt or Win"
        );
        assert_eq!(
            why("Shift+7"),
            "F8 is fine alone; 7 types, so it needs Ctrl, Alt or Win"
        );
        for typing in [
            "Space",
            "Enter",
            "Tab",
            "Backspace",
            "Escape",
            "Delete",
            "Comma",
            "Numpad5",
        ] {
            assert!(why(typing).contains(&format!("{typing} types")), "{typing}");
        }
        for moving in ["Up", "Shift+Left", "Home", "End", "PageUp", "PageDown"] {
            assert!(why(moving).contains("moves the caret"), "{moving}");
        }
        assert!(judge("Ctrl+S", "", OTHERS).unwrap().is_some());
        assert!(judge("Alt+Up", "", OTHERS).unwrap().is_some());
        assert!(judge("Super+Space", "", OTHERS).unwrap().is_some());
    }

    #[test]
    fn a_recorded_combination_is_judged_before_it_is_tried() {
        assert_eq!(judge("", "Ctrl+Alt+Shift+S", OTHERS), Ok(None));
        assert_eq!(
            judge("Ctrl+Alt+Shift+S", "Ctrl+Alt+Shift+S", OTHERS),
            Ok(None)
        );
        assert!(judge("Ctrl+Alt+Y", "Ctrl+Alt+Shift+S", OTHERS)
            .unwrap()
            .is_some());
        assert!(judge("Super+F9", "", OTHERS).unwrap().is_some());
        assert_eq!(
            judge("Shift+K", "Ctrl+Alt+Shift+S", OTHERS),
            Err("F8 is fine alone; K types, so it needs Ctrl, Alt or Win".into())
        );
        assert_eq!(
            judge("Ctrl+Alt+S", "Ctrl+Alt+Shift+S", OTHERS),
            Err("Ctrl+Alt+S is already Share the last clip".into())
        );
        assert_eq!(
            judge("Ctrl+Comma", "", OTHERS),
            Err("Ctrl+Comma opens the app's settings".into())
        );
        assert_eq!(
            judge("Ctrl+Alt+NoSuchKey", "", OTHERS),
            Err("Ctrl+Alt+NoSuchKey is not a key combination the app can take".into())
        );
    }
}
