//! The bundled OBS as a process; on Linux, the installed OBS (maybe Flathub's) run a second time.
use crate::bundled::Bundled;
use std::path::Path;
use std::time::Duration;

pub struct Proc {
    pub pid: u32,
    #[cfg(windows)]
    handle: isize,
    #[cfg(not(windows))]
    child: std::process::Child,
}

/// Starts OBS hidden: Windows gives the start's show state to the first window a program shows.
/// Starts OBS; without `replay` it leaves its replay buffer off, as while recording is paused.
pub fn spawn(b: &Bundled, replay: bool) -> std::io::Result<Proc> {
    sys::spawn(b, replay)
}

/// As `spawn`, held in a job that ends OBS with this process, so ending the task ends OBS;
/// the programs OBS starts leave the job.
#[cfg(windows)]
pub fn spawn_held(b: &Bundled, args: &[String]) -> std::io::Result<Proc> {
    sys::spawn_held(b, args)
}

/// Waits for OBS to exit; its exit code, or -1.
pub fn wait(p: Proc) -> i32 {
    sys::wait(p)
}

/// Asks OBS to quit as closing its window does; it stops the replay buffer and saves.
pub fn close(pid: u32) {
    sys::close(pid)
}

pub fn kill(pid: u32) {
    sys::kill(pid)
}

pub fn processes() -> Vec<crate::install::Process> {
    sys::processes()
}

/// Waits for a process to exit; whether it did within `within`.
pub fn gone(pid: u32, within: Duration) -> bool {
    let until = std::time::Instant::now() + within;
    loop {
        if !processes().iter().any(|p| p.pid == pid) {
            return true;
        }
        if std::time::Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn set_shown(pid: u32, shown: bool) {
    sys::set_shown(pid, shown)
}

/// A top-level window of OBS's process, as the app sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Win {
    pub id: isize,
    pub title: String,
    pub visible: bool,
    pub owned: bool,
    /// A title bar: OBS's main window has one, its helper and pop-up windows do not.
    pub caption: bool,
    /// Minimized: on the taskbar, or hidden after OBS minimized it.
    pub minimized: bool,
}

/// OBS's main window title: "OBS 32.2.2 - Portable Mode - Profile: ..." or "OBS Studio 32...".
pub fn is_main(title: &str) -> bool {
    let rest = title.strip_prefix("OBS ").unwrap_or("");
    let rest = rest.strip_prefix("Studio ").unwrap_or(rest);
    rest.chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
}

fn main_window(w: &Win) -> bool {
    !w.owned && w.caption && is_main(&w.title)
}

/// Only the main window: OBS's dialogs, error boxes included, stay on the screen.
pub fn to_hide(ws: &[Win]) -> Vec<isize> {
    ws.iter()
        .filter(|w| w.visible && main_window(w))
        .map(|w| w.id)
        .collect()
}

/// Only the main window, never one of the windows OBS keeps hidden for good.
pub fn to_show(ws: &[Win], hid: &[isize]) -> Vec<isize> {
    ws.iter()
        .filter(|w| {
            (!w.visible || w.minimized) && main_window(w) && (hid.is_empty() || hid.contains(&w.id))
        })
        .map(|w| w.id)
        .collect()
}

pub fn windows(pid: u32) -> Vec<Win> {
    sys::listed(pid)
}

/// The titles of OBS's windows, dialogs included: what OBS shows when it stops on an error.
pub fn window_titles(pid: u32) -> Vec<String> {
    sys::window_titles(pid)
}

/// Running OBS processes: a user's own (another program file) and the bundled one's.
#[derive(Debug, Default)]
pub struct Running {
    pub others: Vec<String>,
    pub ours: Vec<u32>,
    /// OBS processes with the highest rights, whose path Windows hides from this process.
    pub unreadable: Vec<u32>,
}

pub fn running(b: &Bundled) -> Running {
    sys::running(b)
}

/// Waits for a process the app did not start, its exit code or -1. Without a handle to wait
/// on, it polls the process list, faster once `hurry` is set.
pub fn wait_pid(pid: u32, hurry: &std::sync::atomic::AtomicBool) -> i32 {
    sys::wait_pid(pid, hurry)
}

/// The task that starts OBS with the highest rights, at sign-in and when the app runs it.
/// Task Scheduler keeps a task as a file of its name, so it has no colon.
pub const TASK: &str = "reel OBS (games that run as administrator)";

/// The task that runs reel-elevate to show, hide or close the copy's OBS.
pub const TASK_WINDOW: &str = "reel OBS window (games that run as administrator)";

/// A copy an older version made has none, and its OBS keeps its tray icon until made again.
pub fn window_task_exists() -> bool {
    sys::window_task_exists()
}

/// Runs the second task without a prompt. For Show, the helper may take the foreground,
/// which Windows gives only to the program the user is in.
pub fn window_run(w: crate::elevated::Window) -> Result<(), String> {
    sys::window_run(w)
}

/// Where the elevated helper writes its answer: a name no one can guess ahead of time.
pub fn answer_path(dir: &Path) -> std::path::PathBuf {
    dir.join(format!(
        "reel-admin-{}.txt",
        reel_core::ids::new_device_token()
    ))
}

/// Ok for "ok", else the error's first line; the rest goes to the log.
pub fn script_answer(text: Option<&str>, code: u32) -> Result<(), String> {
    let Some(text) = text else {
        return Err(format!(
            "reel-elevate.exe, run as administrator, left no answer (exit code {code})"
        ));
    };
    let text = text.trim_start_matches('\u{feff}').trim();
    if text == "ok" {
        return Ok(());
    }
    tracing::warn!(answer = %text, code, "the administrator step failed");
    let first = text.lines().next().unwrap_or("").trim();
    Err(first.strip_prefix("error: ").unwrap_or(first).to_string())
}

/// Stages the copy's settings (`files`) in the install, then runs reel-elevate.exe as
/// administrator. `answered` is called once Windows' prompt was answered Yes.
pub fn elevated_step(
    step: crate::elevated::Step,
    source: &Bundled,
    files: &[(String, String)],
    answered: &dyn Fn(),
) -> Result<(), String> {
    sys::elevated_step(step, source, files, answered)
}

/// The system's Program Files folder, as Windows keeps it (not as the environment says).
pub fn program_files() -> Option<std::path::PathBuf> {
    sys::program_files()
}

pub fn task_exists() -> bool {
    sys::task_exists()
}

/// Starts OBS through the task; a task one made oneself runs without asking.
pub fn task_run() -> Result<(), String> {
    sys::task_run()
}

/// Ends the OBS the task started, as Task Scheduler ends a task.
pub fn task_end() {
    sys::task_end()
}

/// Asks Task Scheduler, as this process may not see the path of the OBS the task started.
pub fn task_running() -> bool {
    sys::task_running()
}

#[cfg(windows)]
mod sys {
    use super::{Proc, Running};
    use crate::bundled::Bundled;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, BOOL, HANDLE, HWND, LPARAM, WPARAM};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        CreateProcessW, GetExitCodeProcess, OpenProcess, QueryFullProcessImageNameW,
        TerminateProcess, WaitForSingleObject, INFINITE, PROCESS_CREATION_FLAGS,
        PROCESS_INFORMATION, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_TERMINATE, STARTF_USESHOWWINDOW, STARTUPINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible, PostMessageW,
        SendMessageTimeoutW, SetForegroundWindow, ShowWindow, GW_OWNER, SC_MINIMIZE, SC_RESTORE,
        SMTO_ABORTIFHUNG, SW_HIDE, SW_SHOWMINNOACTIVE, WM_CLOSE, WM_SYSCOMMAND,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// CreateProcessW, as std's Command cannot set the start's show state that keeps OBS hidden.
    pub fn spawn(b: &Bundled, replay: bool) -> std::io::Result<Proc> {
        let mut args = b.args(false);
        if !replay {
            args.retain(|a| a != crate::bundled::START_REPLAY);
        }
        spawn_with(b, &args)
    }

    pub fn spawn_held(b: &Bundled, args: &[String]) -> std::io::Result<Proc> {
        use windows::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
        };
        let io = |e: windows::core::Error| std::io::Error::other(e.to_string());
        unsafe {
            // never closed: the job ends OBS when this process ends
            let job = CreateJobObjectW(None, PCWSTR::null()).map_err(io)?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags =
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .map_err(io)?;
            let p = spawn_with(b, args)?;
            AssignProcessToJobObject(job, HANDLE(p.handle as *mut _)).map_err(io)?;
            Ok(p)
        }
    }

    fn spawn_with(b: &Bundled, args: &[String]) -> std::io::Result<Proc> {
        let exe = b.exe.to_string_lossy().to_string();
        let mut line = format!("\"{exe}\"");
        for a in args {
            line.push(' ');
            line.push_str(a);
        }
        let exe_w = wide(&exe);
        let dir_w = wide(&b.workdir().to_string_lossy());
        let mut line_w = wide(&line);
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            dwFlags: STARTF_USESHOWWINDOW,
            wShowWindow: SW_HIDE.0 as u16,
            ..Default::default()
        };
        let mut pi = PROCESS_INFORMATION::default();
        unsafe {
            CreateProcessW(
                PCWSTR(exe_w.as_ptr()),
                PWSTR(line_w.as_mut_ptr()),
                None,
                None,
                false,
                PROCESS_CREATION_FLAGS(0),
                None,
                PCWSTR(dir_w.as_ptr()),
                &si,
                &mut pi,
            )
            .map_err(|e| std::io::Error::other(e.to_string()))?;
            let _ = CloseHandle(pi.hThread);
        }
        Ok(Proc {
            pid: pi.dwProcessId,
            handle: pi.hProcess.0 as isize,
        })
    }

    pub fn wait(p: Proc) -> i32 {
        let h = HANDLE(p.handle as *mut _);
        unsafe {
            WaitForSingleObject(h, INFINITE);
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code).is_ok();
            let _ = CloseHandle(h);
            if ok {
                code as i32
            } else {
                -1
            }
        }
    }

    fn windows_of(pid: u32) -> Vec<HWND> {
        windows_of_pid(pid, false)
    }

    fn windows_of_pid(pid: u32, owned_too: bool) -> Vec<HWND> {
        struct Found {
            pid: u32,
            owned_too: bool,
            out: Vec<HWND>,
        }
        unsafe extern "system" fn each(hwnd: HWND, l: LPARAM) -> BOOL {
            let f = &mut *(l.0 as *mut Found);
            let mut p = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut p));
            if p == f.pid
                && (f.owned_too
                    || GetWindow(hwnd, GW_OWNER)
                        .map(|o| o.0.is_null())
                        .unwrap_or(true))
            {
                f.out.push(hwnd);
            }
            BOOL(1)
        }
        let mut f = Found {
            pid,
            owned_too,
            out: Vec::new(),
        };
        unsafe {
            let _ = EnumWindows(Some(each), LPARAM(&mut f as *mut Found as isize));
        }
        f.out
    }

    /// Without a main window (OBS still starting, or stopped on an error box) every window of
    /// its own gets the WM_CLOSE.
    pub fn close(pid: u32) {
        let main: Vec<HWND> = listed(pid)
            .into_iter()
            .filter(super::main_window)
            .map(|w| HWND(w.id as *mut _))
            .collect();
        let targets = if main.is_empty() {
            windows_of(pid)
        } else {
            main
        };
        for w in targets {
            unsafe {
                let _ = PostMessageW(w, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }

    pub fn kill(pid: u32) {
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                let _ = TerminateProcess(h, 1);
                let _ = CloseHandle(h);
            }
        }
    }

    /// The windows the app hid, by process, to give back exactly those.
    static HID: std::sync::Mutex<Vec<(u32, isize)>> = std::sync::Mutex::new(Vec::new());

    pub fn listed(pid: u32) -> Vec<super::Win> {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
        windows_of_pid(pid, true)
            .into_iter()
            .map(|w| unsafe {
                let mut buf = [0u16; 256];
                let n = GetWindowTextW(w, &mut buf).max(0) as usize;
                use windows::Win32::UI::WindowsAndMessaging::{
                    GetWindowLongPtrW, GWL_STYLE, WS_CAPTION,
                };
                let style = GetWindowLongPtrW(w, GWL_STYLE) as u32;
                super::Win {
                    id: w.0 as isize,
                    title: String::from_utf16_lossy(&buf[..n]),
                    visible: IsWindowVisible(w).as_bool(),
                    owned: GetWindow(w, GW_OWNER)
                        .map(|o| !o.0.is_null())
                        .unwrap_or(false),
                    caption: style & WS_CAPTION.0 == WS_CAPTION.0,
                    minimized: IsIconic(w).as_bool(),
                }
            })
            .collect()
    }

    pub fn set_shown(pid: u32, shown: bool) {
        let ws = listed(pid);
        let mut hid = HID.lock().unwrap_or_else(|e| e.into_inner());
        if shown {
            let mine: Vec<isize> = hid
                .iter()
                .filter(|(p, _)| *p == pid)
                .map(|(_, w)| *w)
                .collect();
            for id in super::to_show(&ws, &mine) {
                unsafe { show_as_obs(HWND(id as *mut _)) }
            }
            hid.retain(|(p, _)| *p != pid);
        } else {
            for id in super::to_hide(&ws) {
                unsafe { hide_as_obs(HWND(id as *mut _)) }
                if !hid.contains(&(pid, id)) {
                    hid.push((pid, id));
                }
            }
        }
    }

    /// As the title bar's buttons do: Qt misses a state change made from outside the window.
    unsafe fn syscommand(w: HWND, what: u32) {
        let _ = SendMessageTimeoutW(
            w,
            WM_SYSCOMMAND,
            WPARAM(what as usize),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            2000,
            None,
        );
    }

    /// Minimized by OBS first, as OBS turns its preview off only while minimized.
    unsafe fn hide_as_obs(w: HWND) {
        syscommand(w, SC_MINIMIZE);
        let _ = ShowWindow(w, SW_HIDE);
    }

    /// Shown minimized, then restored by OBS itself, so Qt lays out and paints the window and
    /// OBS turns its preview back on.
    unsafe fn show_as_obs(w: HWND) {
        if !IsWindowVisible(w).as_bool() {
            let _ = ShowWindow(w, SW_SHOWMINNOACTIVE);
        }
        syscommand(w, SC_RESTORE);
        let _ = SetForegroundWindow(w);
    }

    /// The shown windows and the main window, not the helper windows OBS keeps hidden.
    pub fn window_titles(pid: u32) -> Vec<String> {
        listed(pid)
            .into_iter()
            .filter(|w| !w.title.is_empty() && (w.visible || super::is_main(&w.title)))
            .map(|w| w.title)
            .collect()
    }

    fn image(pid: u32) -> Option<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let got = QueryFullProcessImageNameW(
                h,
                PROCESS_NAME_WIN32,
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(h);
            got.ok()?;
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    pub fn wait_pid(pid: u32, hurry: &std::sync::atomic::AtomicBool) -> i32 {
        use windows::Win32::System::Threading::PROCESS_SYNCHRONIZE;
        unsafe {
            let h = OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            )
            .or_else(|_| OpenProcess(PROCESS_SYNCHRONIZE, false, pid));
            let h = match h {
                Ok(h) => h,
                Err(e) => {
                    tracing::info!(pid, error = %e, "no handle to wait on for OBS: its exit is watched in the process list");
                    let name = names().into_iter().find(|(p, _)| *p == pid).map(|(_, n)| n);
                    while name.is_some()
                        && names()
                            .into_iter()
                            .any(|(p, n)| p == pid && Some(&n) == name.as_ref())
                    {
                        let every = if hurry.load(std::sync::atomic::Ordering::Relaxed) {
                            100
                        } else {
                            1000
                        };
                        std::thread::sleep(std::time::Duration::from_millis(every));
                    }
                    return -1;
                }
            };
            tracing::debug!(pid, "waiting on OBS's process");
            WaitForSingleObject(h, INFINITE);
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code).is_ok();
            let _ = CloseHandle(h);
            if ok {
                code as i32
            } else {
                -1
            }
        }
    }

    pub fn elevated_step(
        step: crate::elevated::Step,
        source: &Bundled,
        files: &[(String, String)],
        answered: &dyn Fn(),
    ) -> Result<(), String> {
        use crate::elevated::{helper_args, obs_root, stage, Step, HELPER};
        let user = format!(
            "{}\\{}",
            std::env::var("USERDOMAIN").unwrap_or_default(),
            std::env::var("USERNAME").unwrap_or_default()
        );
        let install = obs_root(source)
            .parent()
            .map(std::path::Path::to_path_buf)
            .ok_or("no install folder")?;
        if step != Step::Remove {
            stage(source, files)
                .map_err(|e| format!("the copy's settings were not put in the install: {e}"))?;
        }
        // Writing and removing use the copy's own helper, which only an administrator can change.
        let in_copy = program_files().map(|pf| pf.join("reel").join(HELPER));
        let helper = match in_copy {
            Some(h) if step != Step::Install && h.is_file() => h,
            _ => install.join(HELPER),
        };
        if !helper.is_file() {
            return Err(format!(
                "{} is missing; install reel again",
                helper.display()
            ));
        }
        elevated(
            &helper,
            |answer| helper_args(step, &install, answer, &user),
            answered,
        )
    }

    pub fn program_files() -> Option<std::path::PathBuf> {
        use windows::Win32::System::Com::CoTaskMemFree;
        use windows::Win32::UI::Shell::{
            FOLDERID_ProgramFiles, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
        };
        unsafe {
            let p = SHGetKnownFolderPath(&FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, None).ok()?;
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            s.map(std::path::PathBuf::from)
        }
    }

    /// How long the app waits on Task Scheduler's interface for one call.
    const COM_BOUND: std::time::Duration = std::time::Duration::from_secs(5);

    /// On a thread of its own: COM set up on the main thread would make it multithreaded, and
    /// the window's OleInitialize would then fail with RPC_E_CHANGED_MODE.
    fn tasks<T: Send + 'static>(
        what: &'static str,
        or: T,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("reel-tasks".into())
            .spawn(move || {
                let inited = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                let done = f();
                if inited {
                    unsafe { CoUninitialize() };
                }
                let _ = tx.send(done);
            });
        if spawned.is_err() {
            return or;
        }
        rx.recv_timeout(COM_BOUND).unwrap_or_else(|_| {
            tracing::warn!(what, bound = ?COM_BOUND, "Task Scheduler did not answer in time");
            or
        })
    }

    /// Through the interface rather than schtasks, whose output depends on the language of
    /// Windows. Only inside `tasks`.
    fn registered(name: &str) -> Option<windows::Win32::System::TaskScheduler::IRegisteredTask> {
        use windows::core::{BSTR, VARIANT};
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
        use windows::Win32::System::TaskScheduler::{ITaskService, TaskScheduler};
        unsafe {
            let svc: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER).ok()?;
            let none = VARIANT::default();
            svc.Connect(&none, &none, &none, &none).ok()?;
            let folder = svc.GetFolder(&BSTR::from("\\")).ok()?;
            folder.GetTask(&BSTR::from(name)).ok()
        }
    }

    pub fn task_exists() -> bool {
        tasks("is the task there", false, || {
            registered(super::TASK).is_some()
        })
    }

    pub fn task_run() -> Result<(), String> {
        tasks(
            "run the task",
            Err("Task Scheduler did not answer".into()),
            || {
                let t = registered(super::TASK).ok_or("the task is missing")?;
                unsafe { t.Run(&windows::core::VARIANT::default()) }
                    .map(|_| ())
                    .map_err(|e| format!("the task did not start: {e}"))
            },
        )
    }

    pub fn task_end() {
        tasks("end the task", (), || {
            if let Some(t) = registered(super::TASK) {
                let _ = unsafe { t.Stop(0) };
            }
        })
    }

    pub fn window_task_exists() -> bool {
        tasks("is the second task there", false, || {
            registered(super::TASK_WINDOW).is_some()
        })
    }

    pub fn window_run(w: crate::elevated::Window) -> Result<(), String> {
        use windows::core::{BSTR, VARIANT};
        use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
        if w == crate::elevated::Window::Show {
            unsafe {
                let _ = AllowSetForegroundWindow(ASFW_ANY);
            }
        }
        tasks(
            "run the second task",
            Err("Task Scheduler did not answer".into()),
            move || {
                let t = registered(super::TASK_WINDOW)
                    .ok_or("the task that shows and hides OBS is missing")?;
                unsafe { t.Run(&VARIANT::from(BSTR::from(w.word()))) }
                    .map(|_| ())
                    .map_err(|e| format!("the task that shows and hides OBS did not start: {e}"))
            },
        )
    }

    pub fn task_running() -> bool {
        use windows::Win32::System::TaskScheduler::TASK_STATE_RUNNING;
        tasks("is the task running", false, || {
            registered(super::TASK)
                .and_then(|t| unsafe { t.State() }.ok())
                .is_some_and(|s| s == TASK_STATE_RUNNING)
        })
    }

    /// Runs the helper as administrator; its answer file says what happened.
    fn elevated(
        helper: &std::path::Path,
        args: impl FnOnce(&std::path::Path) -> String,
        answered: &dyn Fn(),
    ) -> Result<(), String> {
        use windows::Win32::Foundation::ERROR_CANCELLED;
        use windows::Win32::UI::Shell::{
            ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
        };
        use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
        let answer = super::answer_path(&std::env::temp_dir());
        let verb = wide("runas");
        let exe = wide(&helper.to_string_lossy());
        let line = args(&answer);
        tracing::info!(helper = %helper.display(), args = %line, "run as administrator");
        let params = wide(&line);
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(exe.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            nShow: SW_HIDE.0,
            ..Default::default()
        };
        let mut code = 1u32;
        let started = unsafe { ShellExecuteExW(&mut info) };
        if let Err(e) = started {
            return Err(if e.code() == ERROR_CANCELLED.to_hresult() {
                "the administrator prompt was refused".to_string()
            } else {
                format!("Windows did not start reel-elevate.exe as administrator: {e}")
            });
        }
        answered();
        unsafe {
            if !info.hProcess.is_invalid() {
                WaitForSingleObject(info.hProcess, INFINITE);
                let _ = GetExitCodeProcess(info.hProcess, &mut code);
                let _ = CloseHandle(info.hProcess);
            }
        }
        let text = std::fs::read(&answer)
            .ok()
            .map(|b| String::from_utf8_lossy(&b).to_string());
        let _ = std::fs::remove_file(&answer);
        super::script_answer(text.as_deref(), code)
    }

    fn names() -> Vec<(u32, String)> {
        let mut out = Vec::new();
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return out;
            };
            let mut e = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snap, &mut e).is_ok();
            while more {
                let n = e
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(e.szExeFile.len());
                out.push((e.th32ProcessID, String::from_utf16_lossy(&e.szExeFile[..n])));
                more = Process32NextW(snap, &mut e).is_ok();
            }
            let _ = CloseHandle(snap);
        }
        out
    }

    pub fn processes() -> Vec<crate::install::Process> {
        let mut out = Vec::new();
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return out;
            };
            let mut e = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snap, &mut e).is_ok();
            while more {
                let n = e
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(e.szExeFile.len());
                out.push(crate::install::Process {
                    pid: e.th32ProcessID,
                    name: String::from_utf16_lossy(&e.szExeFile[..n]),
                    path: image(e.th32ProcessID),
                });
                more = Process32NextW(snap, &mut e).is_ok();
            }
            let _ = CloseHandle(snap);
        }
        out
    }

    pub fn running(b: &Bundled) -> Running {
        let ours = crate::install::path_key(&b.exe.to_string_lossy());
        let mut r = Running::default();
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return r;
            };
            let mut e = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snap, &mut e).is_ok();
            while more {
                let n = e
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(e.szExeFile.len());
                let name = String::from_utf16_lossy(&e.szExeFile[..n]).to_lowercase();
                if crate::bundled::is_obs_exe(&name) || name == "obs32.exe" || name == "obs.exe" {
                    match image(e.th32ProcessID) {
                        Some(path) if crate::install::path_key(&path) == ours => {
                            r.ours.push(e.th32ProcessID)
                        }
                        Some(path) => r.others.push(path),
                        None => {
                            r.unreadable.push(e.th32ProcessID);
                            r.others.push(name)
                        }
                    }
                }
                more = Process32NextW(snap, &mut e).is_ok();
            }
            let _ = CloseHandle(snap);
        }
        r
    }
}

#[cfg(not(windows))]
mod sys {
    use super::{Proc, Running};
    use crate::bundled::Bundled;
    use reel_core::proc::command;
    use std::process::Stdio;

    pub fn spawn(b: &Bundled, replay: bool) -> std::io::Result<Proc> {
        let (program, mut args) = b.command();
        if !replay {
            args.retain(|a| a != crate::bundled::START_REPLAY);
        }
        let program = if b.flatpak {
            program
        } else {
            crate::bundled::own_name(&program, &b.config_root)
        };
        tracing::info!(program = %program.display(), settings = %b.config_root.display(), "reel's OBS starts");
        let mut c = command(program);
        c.args(args).current_dir(b.workdir());
        if !b.flatpak {
            c.env("XDG_CONFIG_HOME", &b.config_root);
        }
        let child = c
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Proc {
            pid: child.id(),
            child,
        })
    }

    pub fn wait(mut p: Proc) -> i32 {
        p.child.wait().ok().and_then(|s| s.code()).unwrap_or(-1)
    }

    fn signal(pid: u32, sig: &str) {
        let _ = command("kill")
            .args([sig, &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    /// The process and all below it, deepest last: Flathub's OBS runs a few levels down.
    fn tree(pid: u32) -> Vec<u32> {
        let parents: Vec<(u32, u32)> = std::fs::read_dir("/proc")
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.file_name().to_string_lossy().parse::<u32>().ok()?;
                let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
                let after = stat.rsplit_once(')')?.1;
                let ppid = after.split_whitespace().nth(1)?.parse::<u32>().ok()?;
                Some((p, ppid))
            })
            .collect();
        let mut out = vec![pid];
        let mut i = 0;
        while i < out.len() {
            let parent = out[i];
            out.extend(
                parents
                    .iter()
                    .filter(|(_, pp)| *pp == parent)
                    .map(|(p, _)| *p),
            );
            i += 1;
        }
        out
    }

    fn is_obs(pid: u32) -> bool {
        std::fs::read_link(format!("/proc/{pid}/exe"))
            .map(|e| {
                e.file_name()
                    .is_some_and(|n| n == "obs" || n == crate::bundled::LINUX_NAME)
            })
            .unwrap_or(false)
    }

    /// SIGTERM, which OBS takes as a close, to OBS itself: one to its sandbox ends it at once.
    pub fn close(pid: u32) {
        let t = tree(pid);
        let target = t.iter().copied().find(|p| is_obs(*p)).unwrap_or(pid);
        signal(target, "-TERM")
    }

    pub fn kill(pid: u32) {
        for p in tree(pid).into_iter().rev() {
            signal(p, "-KILL");
        }
    }

    /// The windows a hiding pass unmapped, by process, to map exactly those again.
    static HID: std::sync::Mutex<Vec<(u32, String)>> = std::sync::Mutex::new(Vec::new());

    /// Only the main window, never the menus, tooltips and helper windows Qt keeps unmapped.
    pub fn set_shown(pid: u32, shown: bool) {
        let mut search = vec!["search", "--all"];
        if !shown {
            search.push("--onlyvisible");
        }
        let pid_s = pid.to_string();
        search.extend(["--pid", pid_s.as_str(), "--name", "^OBS (Studio )?[0-9]"]);
        let Ok(out) = command("xdotool").args(&search).output() else {
            return;
        };
        let mut ids: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let mut hid = HID.lock().unwrap_or_else(|e| e.into_inner());
        if shown {
            for (p, id) in hid.iter() {
                if *p == pid && !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
            hid.retain(|(p, _)| *p != pid);
        } else {
            for id in &ids {
                if !hid.iter().any(|(p, x)| *p == pid && x == id) {
                    hid.push((pid, id.clone()));
                }
            }
        }
        drop(hid);
        let verb = if shown { "windowmap" } else { "windowunmap" };
        if ids.is_empty() {
            tracing::debug!(verb, pid, stderr = %String::from_utf8_lossy(&out.stderr).trim(), "no main window of OBS's found");
            return;
        }
        tracing::debug!(?ids, verb, "OBS's main window");
        let args: Vec<&str> = ids.iter().flat_map(|id| [verb, id.as_str()]).collect();
        let _ = command("xdotool")
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if shown {
            if let Some(id) = ids.first().and_then(|i| i.parse::<isize>().ok()) {
                to_front(id);
            }
        }
    }

    fn named_windows(pid: u32) -> Vec<String> {
        let pid = pid.to_string();
        command("xdotool")
            .args([
                "search",
                "--all",
                "--onlyvisible",
                "--pid",
                pid.as_str(),
                "--name",
                ".",
            ])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn window_titles(pid: u32) -> Vec<String> {
        named_windows(pid)
            .iter()
            .filter_map(|id| {
                let o = command("xdotool")
                    .args(["getwindowname", id.as_str()])
                    .output()
                    .ok()?;
                let t = String::from_utf8_lossy(&o.stdout).trim().to_string();
                (!t.is_empty()).then_some(t)
            })
            .collect()
    }

    fn to_front(id: isize) {
        let id = id.to_string();
        let _ = command("xdotool")
            .args(["windowraise", id.as_str(), "windowfocus", id.as_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    pub fn wait_pid(pid: u32, _hurry: &std::sync::atomic::AtomicBool) -> i32 {
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        -1
    }

    pub fn elevated_step(
        _step: crate::elevated::Step,
        _source: &Bundled,
        _files: &[(String, String)],
        _answered: &dyn Fn(),
    ) -> Result<(), String> {
        Err("the OBS for games run as administrator is Windows only".into())
    }

    pub fn program_files() -> Option<std::path::PathBuf> {
        None
    }

    pub fn task_exists() -> bool {
        false
    }

    pub fn window_task_exists() -> bool {
        false
    }

    pub fn window_run(_w: crate::elevated::Window) -> Result<(), String> {
        Err("the task that shows and hides OBS is Windows only".into())
    }

    pub fn listed(_pid: u32) -> Vec<super::Win> {
        Vec::new()
    }

    pub fn task_run() -> Result<(), String> {
        Err("the task for games that run as administrator is Windows only".into())
    }

    pub fn task_end() {}

    pub fn task_running() -> bool {
        false
    }

    pub fn processes() -> Vec<crate::install::Process> {
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        dir.flatten()
            .filter_map(|e| {
                let pid = e.file_name().to_string_lossy().parse::<u32>().ok()?;
                let name = std::fs::read_to_string(e.path().join("comm"))
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                let path = std::fs::read_link(e.path().join("exe"))
                    .ok()
                    .map(|p| p.display().to_string());
                Some(crate::install::Process { pid, name, path })
            })
            .collect()
    }

    pub fn running(b: &Bundled) -> Running {
        let mut r = Running::default();
        let mark = format!("XDG_CONFIG_HOME={}", b.config_root.display());
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return r;
        };
        for e in dir.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(exe) = std::fs::read_link(e.path().join("exe")) else {
                continue;
            };
            if exe
                .file_name()
                .is_some_and(|n| n == "obs" || n == crate::bundled::LINUX_NAME)
            {
                let env = std::fs::read(e.path().join("environ")).unwrap_or_default();
                if env.split(|c| *c == 0).any(|v| v == mark.as_bytes()) {
                    r.ours.push(pid);
                } else {
                    r.others.push(exe.display().to_string());
                }
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_file_is_unguessable() {
        let (a, b) = (answer_path(Path::new("t")), answer_path(Path::new("t")));
        assert_ne!(a, b);
        assert!(
            a.to_string_lossy().len() > "t/reel-admin-.txt".len() + 40,
            "{}",
            a.display()
        );
    }

    #[test]
    fn only_the_main_window_is_hidden_and_only_the_main_window_comes_back() {
        let w = |id, title: &str, visible, owned, caption| Win {
            id,
            title: title.into(),
            visible,
            owned,
            caption,
            minimized: false,
        };
        let main = "OBS 32.2.2 - Portable Mode - Profile: reel - Scenes: reel";
        let mut ws = vec![
            w(1, main, true, false, true),
            w(2, "GDI+ Window (obs64.exe)", false, false, false),
            w(3, "", false, false, false),
            w(4, "", false, false, true),
            w(5, main, false, false, false),
            w(6, "QTrayIconMessageWindow", false, false, false),
            w(7, "OBSWindowClass", false, false, true),
            w(8, "Properties for 'Game'", true, true, true),
            w(9, "OBS Studio Crash Detected", true, false, true),
        ];
        assert_eq!(
            to_hide(&ws),
            [1],
            "the main window, not a dialog, not an error box"
        );
        ws[0].visible = false;
        assert_eq!(
            to_show(&ws, &[1]),
            [1],
            "Show OBS: the main window the hiding pass hid, nothing else"
        );
        assert_eq!(
            to_show(&ws, &[]),
            [1],
            "Show OBS after a start that kept the main window hidden: the main window alone"
        );
        assert!(
            to_show(&ws, &[1])
                .iter()
                .all(|id| *id != 3 && *id != 4 && *id != 5),
            "never a window OBS keeps hidden"
        );
        ws[0].visible = true;
        assert!(
            to_show(&ws, &[]).is_empty(),
            "a main window on the screen stays"
        );
        ws[0].minimized = true;
        assert_eq!(
            to_show(&ws, &[]),
            [1],
            "a main window minimized on the taskbar comes back"
        );
        assert!(
            is_main("OBS Studio 30.2.3 - Profile: x - Scenes: y")
                && !is_main("OBS")
                && !is_main("OBSWindowClass")
        );
    }

    #[test]
    fn the_task_is_named_as_a_file_may_be() {
        assert!(
            !TASK.contains(['<', '>', ':', '"', '/', '\\', '|', '?', '*']),
            "{TASK}"
        );
        assert!(TASK.len() <= 238);
    }

    #[test]
    fn the_answer_of_the_administrator_step() {
        assert_eq!(script_answer(Some("\u{feff}ok\r\n"), 0), Ok(()));
        assert_eq!(
            script_answer(Some("error: Access is denied.\r\nat line 7"), 1),
            Err("Access is denied.".into())
        );
        assert!(script_answer(None, 1)
            .unwrap_err()
            .contains("left no answer (exit code 1)"));
    }
}
