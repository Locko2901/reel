//! The hotkeys on Wayland, through the desktop's GlobalShortcuts portal. The portal binds once
//! per session, so a change of the keys closes the session and binds in a new one.
use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use ashpd::desktop::CreateSessionOptions;
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

pub const APP_ID: &str = "dev.lockoo.reel";

/// What a shortcut does, by its id, in words.
pub fn what(id: &str) -> &'static str {
    match id {
        "share-last" => "shares the last clip",
        "save-and-share" => "saves the replay and shares it",
        "save" => "saves a clip",
        "upload-last" => "uploads the last clip",
        _ => "does nothing known",
    }
}

/// A combination from the settings in the XDG shortcuts spec's form: Ctrl+Alt+S is CTRL+ALT+s.
pub fn trigger(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for part in text.split('+').map(str::trim) {
        let word = match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmdorctrl" | "commandorcontrol" => "CTRL".to_string(),
            "alt" | "option" => "ALT".to_string(),
            "shift" => "SHIFT".to_string(),
            "super" | "win" | "meta" | "cmd" | "command" => "LOGO".to_string(),
            _ => key(part),
        };
        parts.push(word);
    }
    Some(parts.join("+"))
}

/// A key by its X keysym name, as the spec names keys.
fn key(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let named = match lower.as_str() {
        "space" => "space",
        "enter" | "return" => "Return",
        "tab" => "Tab",
        "escape" | "esc" => "Escape",
        "backspace" => "BackSpace",
        "delete" => "Delete",
        "insert" => "Insert",
        "home" => "Home",
        "end" => "End",
        "pageup" => "Page_Up",
        "pagedown" => "Page_Down",
        "up" | "arrowup" => "Up",
        "down" | "arrowdown" => "Down",
        "left" | "arrowleft" => "Left",
        "right" | "arrowright" => "Right",
        "printscreen" | "print" => "Print",
        "pause" => "Pause",
        "scrolllock" => "Scroll_Lock",
        "comma" => "comma",
        "period" => "period",
        "minus" => "minus",
        "equal" => "equal",
        "mediaplaypause" | "mediaplay" => "XF86AudioPlay",
        "mediapause" => "XF86AudioPause",
        "mediastop" => "XF86AudioStop",
        "mediatracknext" => "XF86AudioNext",
        "mediatrackprevious" => "XF86AudioPrev",
        "audiovolumeup" => "XF86AudioRaiseVolume",
        "audiovolumedown" => "XF86AudioLowerVolume",
        "audiovolumemute" => "XF86AudioMute",
        _ => "",
    };
    if !named.is_empty() {
        return named.to_string();
    }
    let bare = lower
        .strip_prefix("key")
        .or_else(|| lower.strip_prefix("digit"))
        .unwrap_or(&lower);
    if bare.len() == 1 {
        return bare.to_string();
    }
    if let Some(n) = lower.strip_prefix("numpad") {
        return format!("KP_{n}");
    }
    if lower.starts_with('f') && lower[1..].parse::<u8>().is_ok() {
        return name.to_ascii_uppercase();
    }
    name.to_string()
}

/// Binds the shortcuts (id, what it does, keys) and runs `on` with each pressed id until
/// `cancel`; returns each id with the keys the desktop shows for it.
pub async fn bind(
    shortcuts: Vec<(&'static str, &'static str, String)>,
    cancel: CancellationToken,
    on: impl Fn(&str) + Send + 'static,
) -> Result<Vec<(String, String)>, String> {
    if let Ok(id) = ashpd::AppID::try_from(APP_ID) {
        if let Err(e) = ashpd::register_host_app(id).await {
            tracing::info!(error = %e, "the portal has no registry for apps outside a sandbox");
        }
    }
    let portal = GlobalShortcuts::new().await.map_err(|e| e.to_string())?;
    let session = portal
        .create_session(CreateSessionOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    let new: Vec<NewShortcut> = shortcuts
        .iter()
        .map(|(id, what, keys)| {
            NewShortcut::new(*id, *what).preferred_trigger(trigger(keys).as_deref())
        })
        .collect();
    let mut pressed = portal
        .receive_activated()
        .await
        .map_err(|e| e.to_string())?;
    let bound = portal
        .bind_shortcuts(&session, &new, None, BindShortcutsOptions::default())
        .await
        .and_then(|r| r.response())
        .map_err(|e| e.to_string())?;
    let got = bound
        .shortcuts()
        .iter()
        .map(|s| (s.id().to_string(), s.trigger_description().to_string()))
        .collect();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = session.close().await;
                    break;
                }
                ev = pressed.next() => match ev {
                    Some(a) => on(a.shortcut_id()),
                    None => break,
                },
            }
        }
    });
    Ok(got)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_keys_become_the_specs_triggers() {
        assert_eq!(trigger("Ctrl+Alt+S").as_deref(), Some("CTRL+ALT+s"));
        assert_eq!(
            trigger("Ctrl+Alt+Shift+S").as_deref(),
            Some("CTRL+ALT+SHIFT+s")
        );
        assert_eq!(trigger("Super+F9").as_deref(), Some("LOGO+F9"));
        assert_eq!(trigger("F8").as_deref(), Some("F8"));
        assert_eq!(trigger("Shift+PrintScreen").as_deref(), Some("SHIFT+Print"));
        assert_eq!(trigger("Alt+Digit1").as_deref(), Some("ALT+1"));
        assert_eq!(trigger("CmdOrCtrl+KeyU").as_deref(), Some("CTRL+u"));
        assert_eq!(trigger("MediaPlayPause").as_deref(), Some("XF86AudioPlay"));
        assert_eq!(trigger(""), None);
    }
}
