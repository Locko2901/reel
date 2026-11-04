use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

/// A notification, never on a thread of the async runtime: on Linux zbus blocks on a runtime of
/// its own, which panics on a runtime's thread. It makes no sound: reel's cues are its sounds.
pub fn show(app: &AppHandle, title: &str, body: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        let _ = deliver(title, body);
    }
    #[cfg(windows)]
    {
        let (app, title, body) = (app.clone(), title.to_string(), body.to_string());
        std::thread::spawn(move || {
            let xml = reel_desktop_core::links::plain_toast_xml(&title, &body);
            if let Err(e) = toast(&xml, &app.config().identifier) {
                tracing::warn!(error = %e, "the notification as a toast; through the plugin");
                by_plugin(&app, &title, &body);
            }
        });
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let (app, title, body) = (app.clone(), title.to_string(), body.to_string());
        std::thread::spawn(move || by_plugin(&app, &title, &body));
    }
}

#[cfg(not(target_os = "linux"))]
fn by_plugin(app: &AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!(error = %e, "notification");
    }
}

/// A clip's notification with Copy link and Open buttons (Open alone without `copy`), where the
/// desktop's notifications have them.
pub fn show_clip(app: &AppHandle, title: &str, body: &str, clip: &str, copy: bool) {
    #[cfg(windows)]
    {
        let (app, title, body, clip) = (
            app.clone(),
            title.to_string(),
            body.to_string(),
            clip.to_string(),
        );
        std::thread::spawn(move || {
            let xml = reel_desktop_core::links::toast_xml(&title, &body, &clip, copy);
            if let Err(e) = toast(&xml, &app.config().identifier) {
                tracing::warn!(error = %e, "the notification with its buttons; without them");
                by_plugin(&app, &title, &body);
            }
        });
    }
    #[cfg(target_os = "linux")]
    {
        let (app, title, body, clip) = (
            app.clone(),
            title.to_string(),
            body.to_string(),
            clip.to_string(),
        );
        std::thread::spawn(move || deliver_with_actions(app, &title, &body, &clip, copy));
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (clip, copy);
        show(app, title, body);
    }
}

/// A toast as the installed app (its Start menu shortcut carries the identifier), or as
/// PowerShell from a build folder, as the notification plugin does.
#[cfg(windows)]
fn toast(xml: &str, identifier: &str) -> windows::core::Result<()> {
    use windows::core::HSTRING;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    const POWERSHELL: &str =
        "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";
    let built = std::env::current_exe()
        .ok()
        .and_then(|e| {
            e.parent()
                .map(|d| d.ends_with("target/debug") || d.ends_with("target/release"))
        })
        .unwrap_or(false);
    let id = if built { POWERSHELL } else { identifier };
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(id))?.Show(&toast)
}

#[cfg(target_os = "linux")]
fn deliver_with_actions(app: AppHandle, title: &str, body: &str, clip: &str, copy: bool) {
    use reel_desktop_core::links::Link;
    use tauri::Manager;
    let actions = notify_rust::get_capabilities()
        .map(|c| c.iter().any(|x| x == "actions"))
        .unwrap_or(false);
    let mut n = notify_rust::Notification::new();
    n.appname("reel")
        .summary(title)
        .body(body)
        .auto_icon()
        .hint(notify_rust::Hint::SuppressSound(true));
    if actions && copy {
        n.action("copy", "Copy link");
    }
    if actions {
        n.action("open", "Open");
    }
    let handle = match n.show() {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(error = %e, "notification");
            return;
        }
    };
    if !actions {
        return;
    }
    let clip = clip.to_string();
    handle.wait_for_action(move |action| {
        let link = match action {
            "copy" => Link::Copy(clip),
            "open" => Link::Open(clip),
            _ => return,
        };
        let on_main = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Some(rt) = on_main.try_state::<crate::runtime::Runtime>() {
                rt.run_link(link);
            }
        });
    });
}

#[cfg(target_os = "linux")]
pub fn deliver(title: &str, body: &str) -> std::thread::JoinHandle<Result<(), String>> {
    let (title, body) = (title.to_string(), body.to_string());
    std::thread::spawn(move || {
        let sent = notify_rust::Notification::new()
            .appname("reel")
            .summary(&title)
            .body(&body)
            .auto_icon()
            .hint(notify_rust::Hint::SuppressSound(true))
            .show()
            .map(|_| ())
            .map_err(|e| e.to_string());
        if let Err(e) = &sent {
            tracing::warn!(error = %e, "notification");
        }
        sent
    })
}

pub fn copy(app: &AppHandle, text: &str) -> Result<(), String> {
    app.clipboard()
        .write_text(text.to_string())
        .map_err(|e| e.to_string())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn a_notification_from_inside_the_runtime_goes_from_a_thread_of_its_own() {
        let on_the_runtime = tokio::spawn(async {
            notify_rust::Notification::new()
                .summary("reel")
                .show()
                .map(|_| ())
        })
        .await;
        assert!(
            on_the_runtime.is_err_and(|e| e.is_panic()),
            "the blocking D-Bus call on a runtime's thread panics, as the plugin's did"
        );
        let ours = tokio::spawn(async { deliver("reel", "a clip") })
            .await
            .expect("no panic in the runtime");
        assert!(
            ours.join().is_ok(),
            "sent from a thread of its own: it went, or the bus said why, without a panic"
        );
    }
}
