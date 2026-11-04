//! Run by the installers before they touch the install folder: ends the app and its OBS.
//! Usage: reel-stop <install folder> [--app-wait <seconds>] | reel-stop --forget
//!   | reel-stop --clips <answer file> <install folder> [folder of Windows' own ...]
//!   | reel-stop --delete-clips <clips folder> <install folder> [folder of Windows' own ...]
use reel_desktop_core::uninstall::{self, Clips, RealDisk};
use reel_desktop_core::{install, obsproc, settings};
use std::path::Path;
use std::time::{Duration, Instant};

fn main() {
    std::process::exit(run());
}

fn forget() -> i32 {
    match reel_desktop_core::secrets::TokenStore::new(Path::new(".")).clear() {
        Ok(()) => {
            println!("the pairing token is removed");
            0
        }
        Err(e) => {
            println!("the pairing token could not be removed: {e}");
            1
        }
    }
}

fn places(rest: &[String]) -> Vec<uninstall::Place> {
    let install = rest.first().map(String::as_str).unwrap_or("");
    let also = rest.get(1..).unwrap_or(&[]);
    uninstall::places(install, &settings::paths().config_dir, also)
}

fn clips(rest: &[String]) -> i32 {
    let Some(out) = rest.first() else {
        return 2;
    };
    let file = settings::paths().settings_file();
    let answer = match settings::Settings::load(&file) {
        Ok(s) => uninstall::decide(
            &s.clips_dir.to_string_lossy(),
            &places(&rest[1..]),
            &RealDisk,
        ),
        Err(_) => Clips::Missing,
    };
    let line = answer.line(|dir| {
        let (bytes, files) = uninstall::size(std::path::Path::new(dir));
        uninstall::size_words(bytes, files)
    });
    // UTF-16 in a file: a command's output would lose letters outside the code page.
    let mut utf16 = vec![0xFF, 0xFE];
    utf16.extend(line.encode_utf16().flat_map(u16::to_le_bytes));
    match std::fs::write(out, utf16) {
        Ok(()) => {
            println!("{line}");
            0
        }
        Err(e) => {
            println!("{out} could not be written: {e}");
            1
        }
    }
}

fn delete_clips(rest: &[String]) -> i32 {
    let Some(want) = rest.first() else {
        return 2;
    };
    match uninstall::decide(want, &places(&rest[1..]), &RealDisk) {
        Clips::Offer(dir) if install::path_key(&dir) == install::path_key(want) => {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => {
                    println!("the clips folder {dir} is deleted");
                    0
                }
                Err(e) => {
                    println!("the clips folder {dir} is not all deleted: {e}");
                    1
                }
            }
        }
        Clips::Keep { dir, why } => {
            println!("the clips folder {dir} is kept: {why}");
            1
        }
        _ => {
            println!("the clips folder {want} is not the folder offered: it is kept");
            1
        }
    }
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--forget") {
        return forget();
    }
    match args.first().map(String::as_str) {
        Some("--clips") => return clips(&args[1..]),
        Some("--delete-clips") => return delete_clips(&args[1..]),
        _ => {}
    }
    let Some(dir) = args.first().map(|d| d.trim_end_matches('\\').to_string()) else {
        println!("usage: reel-stop <install folder> [--app-wait <seconds>] | reel-stop --forget");
        return 2;
    };
    let app_wait = args
        .iter()
        .position(|a| a == "--app-wait")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let folder = Path::new(&dir);
    if folder.is_dir() {
        let _ = std::fs::write(install::mark(folder), "");
    }
    let (app, _) = install::plan(&obsproc::processes(), &dir);
    if !app.is_empty() {
        let exe = folder.join("reel-desktop.exe");
        println!("reel runs (pid {}): asked to quit", pids(&app));
        if let Ok(mut c) = reel_core::proc::command(&exe)
            .arg("--quit-for-install")
            .spawn()
        {
            let until = Instant::now() + Duration::from_secs(5);
            while Instant::now() < until && matches!(c.try_wait(), Ok(None)) {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let until = Instant::now() + Duration::from_secs(app_wait);
        while Instant::now() < until && !install::plan(&obsproc::processes(), &dir).0.is_empty() {
            std::thread::sleep(Duration::from_millis(250));
        }
        for p in install::plan(&obsproc::processes(), &dir).0 {
            println!("reel did not quit in {app_wait} s: ended (pid {})", p.pid);
            obsproc::kill(p.pid);
            obsproc::gone(p.pid, Duration::from_secs(5));
        }
    }
    let list = obsproc::processes();
    let (_, obs) = install::plan(&list, &dir);
    if install::unreadable_obs(&list) {
        println!("an OBS runs that Windows does not say the folder of: the task for games that run as administrator is ended");
        obsproc::task_end();
    }
    for p in &obs {
        println!("{} ran from reel's OBS (pid {}): ended", p.name, p.pid);
        obsproc::kill(p.pid);
    }
    for p in &obs {
        if !obsproc::gone(p.pid, Duration::from_secs(10)) {
            println!("{} (pid {}) is still running", p.name, p.pid);
        }
    }
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        match locked(&folder.join("obs")) {
            None => break,
            Some(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(500)),
            Some(f) => {
                let left = install::plan(&obsproc::processes(), &dir).1;
                println!(
                    "{} of reel's OBS is still in use{}",
                    f.file_name().unwrap_or_default().to_string_lossy(),
                    if left.is_empty() {
                        String::new()
                    } else {
                        format!(", by {} (pid {})", names(&left), pids(&left))
                    }
                );
                return 1;
            }
        }
    }
    println!("nothing of reel's runs any more");
    0
}

fn pids(list: &[install::Process]) -> String {
    list.iter()
        .map(|p| p.pid.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn names(list: &[install::Process]) -> String {
    list.iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The first file under `obs` (its settings aside) that cannot be opened for writing alone.
fn locked(obs: &Path) -> Option<std::path::PathBuf> {
    let mut stack = vec![obs.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if !(d == obs && e.file_name().eq_ignore_ascii_case("config")) {
                    stack.push(p);
                }
            } else if in_use(&p) {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(windows)]
fn in_use(p: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    match std::fs::OpenOptions::new()
        .write(true)
        .share_mode(0)
        .open(p)
    {
        Ok(_) => false,
        Err(e) => matches!(e.raw_os_error(), Some(32) | Some(33) | Some(1224)),
    }
}

#[cfg(not(windows))]
fn in_use(_: &Path) -> bool {
    false
}
