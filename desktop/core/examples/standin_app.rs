//! A stand-in for reel-desktop.exe in the installers' check under Wine: it keeps restarting its
//! OBS by an uppercase verbatim path and ignores `--quit-for-install`, so the stop helper ends it.
fn main() {
    if std::env::args().any(|a| a == "--quit-for-install") {
        return;
    }
    let dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_default();
    let obs = format!(
        r"\\?\{}",
        dir.join("obs")
            .join("bin")
            .join("64bit")
            .join(reel_desktop_core::bundled::OBS_EXE)
            .display()
    )
    .to_uppercase();
    loop {
        let _ = reel_core::proc::command(&obs)
            .current_dir(dir.join("obs").join("bin").join("64bit"))
            .status();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
