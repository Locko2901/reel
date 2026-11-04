//! The one reel program that runs as administrator: the Program Files copy of OBS and its tasks.
//! Usage: reel-elevate <install|write|remove> <install folder> [answer file] [DOMAIN\user]
//!        reel-elevate start | reel-elevate window <show|hide|close> | reel-elevate clear-marks
// Without a console: a console program run by a task shows a console window.
#![cfg_attr(windows, windows_subsystem = "windows")]
use reel_desktop_core::elevated::{asked, Asked};

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (step, install, answer, user) = match asked(&args) {
        Some(Asked::Step(step, install, answer, user)) => (step, install, answer, user),
        Some(Asked::Start) => return sys::start(),
        Some(Asked::Window(w)) => return sys::window(w),
        // still run by the sign-in task of a copy an older version made
        Some(Asked::ClearMarks) => {
            if let Some(root) = std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(|p| p.join("obs").join("config")))
            {
                reel_desktop_core::elevated::clear_marks(&root);
            }
            return 0;
        }
        None => {
            println!("usage: reel-elevate <install|write|remove> <install folder> [answer file] [DOMAIN\\user] | start | window <show|hide|close>");
            return 2;
        }
    };
    let done = sys::run(step, &install, &user);
    let text = match &done {
        Ok(()) => "ok".to_string(),
        Err(e) => format!("error: {e}"),
    };
    println!("{text}");
    if let Some(a) = answer {
        let _ = std::fs::write(a, &text);
    }
    i32::from(done.is_err())
}

#[cfg(windows)]
mod sys {
    use reel_desktop_core::bundled::{Bundled, OBS_EXE, OBS_EXE_BEFORE};
    use reel_desktop_core::elevated::{self, win, Step, Window, HELPER};
    use reel_desktop_core::obsproc;
    use std::path::Path;

    /// Run by the sign-in task: starts the copy's OBS and waits, so ending the task ends OBS.
    pub fn start() -> i32 {
        let Some(pf) = obsproc::program_files() else {
            return 1;
        };
        let copy = elevated::copy(&pf);
        elevated::clear_marks(&copy.config_root);
        if obsproc::window_task_exists() {
            let _ = elevated::tray_off_before_start(&copy);
        }
        match obsproc::spawn_held(&copy, &elevated::copy_args(&copy)) {
            Ok(p) => obsproc::wait(p),
            Err(_) => 1,
        }
    }

    /// Run by the second task: shows, hides or closes only the OBS that runs from the copy.
    pub fn window(w: Window) -> i32 {
        let Some(pf) = obsproc::program_files() else {
            return 1;
        };
        let pids = obsproc::running(&elevated::copy(&pf)).ours;
        match w {
            Window::Show | Window::Hide => {
                for p in &pids {
                    obsproc::set_shown(*p, w == Window::Show);
                }
            }
            Window::Close => {
                for p in &pids {
                    obsproc::close(*p);
                }
                for p in &pids {
                    if !obsproc::gone(*p, elevated::GRACE) {
                        obsproc::kill(*p);
                    }
                }
            }
        }
        0
    }

    pub fn run(step: Step, install: &Path, user: &str) -> Result<(), String> {
        let pf = obsproc::program_files().ok_or("Windows did not say where Program Files is")?;
        let root = pf.join("reel");
        let obs = root.join("obs");
        let source = Bundled::windows(install);
        let _ = win::end_task(false);
        win::end_processes(&obs)?;
        match step {
            Step::Remove => remove(&root, &obs),
            Step::Install => {
                if user.trim().is_empty() {
                    return Err("no user to start OBS for at sign-in".into());
                }
                std::fs::create_dir_all(&obs).map_err(|e| format!("{}: {e}", obs.display()))?;
                elevated::put_obs(&elevated::obs_root(&source), &obs)
                    .map_err(|e| format!("copying OBS into {}: {e}", obs.display()))?;
                let me = std::env::current_exe().map_err(|e| e.to_string())?;
                let helper = root.join(HELPER);
                if me != helper {
                    std::fs::copy(&me, &helper)
                        .map_err(|e| format!("{}: {e}", helper.display()))?;
                }
                settings(&source, &root, &obs, true)?;
                win::register_tasks(user, &helper)
            }
            Step::Write => {
                if ![OBS_EXE, OBS_EXE_BEFORE]
                    .iter()
                    .any(|n| obs.join("bin").join("64bit").join(n).is_file())
                {
                    return Err(format!(
                        "there is no copy of OBS in {}; turn Capture games that run as administrator off and on again",
                        obs.display()
                    ));
                }
                // Task Scheduler can be slow to report the second task: a tray icon off stays off.
                let tray_off = obsproc::window_task_exists()
                    || elevated::tray_on(&elevated::copy(&pf)) == Some(false);
                settings(&source, &root, &obs, tray_off)
            }
        }
    }

    /// Ends the copy's OBS first, as an OBS that exits writes its own settings over these.
    fn settings(source: &Bundled, root: &Path, obs: &Path, tray_off: bool) -> Result<(), String> {
        win::end_processes(obs)?;
        let config = obs.join("config");
        elevated::take_settings(&elevated::staging(source), &config, tray_off)?;
        elevated::clear_marks(&config);
        win::secure(root)?;
        if let Some(problem) = win::check(root) {
            let _ = win::end_task(true);
            let _ = std::fs::remove_dir_all(root);
            return Err(format!(
                "the rights of {} are not Program Files' own, so it was removed: {problem}",
                root.display()
            ));
        }
        Ok(())
    }

    /// A running program cannot remove itself, so run from the copy the helper goes at reboot.
    fn remove(root: &Path, obs: &Path) -> Result<(), String> {
        use windows::core::HSTRING;
        use windows::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};
        let task = win::end_task(true);
        let me = std::env::current_exe().map_err(|e| e.to_string())?;
        if !me.starts_with(root) {
            if root.exists() {
                std::fs::remove_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
            }
        } else {
            if obs.exists() {
                std::fs::remove_dir_all(obs).map_err(|e| format!("{}: {e}", obs.display()))?;
            }
            unsafe {
                let _ = MoveFileExW(
                    &HSTRING::from(me.as_os_str()),
                    None,
                    MOVEFILE_DELAY_UNTIL_REBOOT,
                );
                let _ = MoveFileExW(
                    &HSTRING::from(root.as_os_str()),
                    None,
                    MOVEFILE_DELAY_UNTIL_REBOOT,
                );
            }
        }
        task
    }
}

#[cfg(not(windows))]
mod sys {
    use reel_desktop_core::elevated::{Step, Window};
    use std::path::Path;

    pub fn start() -> i32 {
        1
    }

    pub fn window(_w: Window) -> i32 {
        1
    }

    pub fn run(_step: Step, _install: &Path, _user: &str) -> Result<(), String> {
        Err("the OBS for games that run as administrator is Windows only".into())
    }
}
