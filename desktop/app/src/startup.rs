//! Start at logon: a Run entry, or a scheduled task with the highest privileges so the
//! hotkeys still work while a game running as administrator has the focus.
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

#[cfg(windows)]
pub const TASK_NAME: &str = "reel desktop (elevated)";

/// Applies the startup settings; the elevated task replaces the Run entry.
pub fn apply(app: &AppHandle, autostart: bool, elevated: bool) -> Result<(), String> {
    elevated_task(elevated)?;
    run_entry(app, autostart && !elevated)
}

/// Runs at every start, so it only reads the registry and starts no process.
pub fn run_entry(app: &AppHandle, want: bool) -> Result<(), String> {
    let launch = app.autolaunch();
    let have = launch.is_enabled().unwrap_or(false);
    if want && !have {
        launch.enable().map_err(|e| e.to_string())?;
    } else if !want && have {
        launch.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(windows)]
fn elevated_task(on: bool) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exists = reel_core::proc::command("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if on == exists {
        return Ok(());
    }
    let args = if on {
        format!(
            "/Create /F /TN \"{TASK_NAME}\" /SC ONLOGON /RL HIGHEST /IT /TR \"\\\"{}\\\" --hidden\"",
            exe.display()
        )
    } else {
        format!("/Delete /F /TN \"{TASK_NAME}\"")
    };
    let wide = |s: &str| -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    };
    let verb = wide("runas");
    let file = wide("schtasks.exe");
    let params = wide(&args);
    let r = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR::null(),
            SW_HIDE,
        )
    };
    if (r.0 as usize) <= 32 {
        return Err(if on {
            "the scheduled task was not made; the UAC prompt was refused or schtasks failed".into()
        } else {
            "the scheduled task was not removed; the UAC prompt was refused or schtasks failed"
                .into()
        });
    }
    Ok(())
}

#[cfg(not(windows))]
fn elevated_task(on: bool) -> Result<(), String> {
    if on {
        tracing::info!("start elevated at logon: only on Windows; nothing done here");
    }
    Ok(())
}
