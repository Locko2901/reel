//! A stand-in for the bundled OBS in the installers' check under Wine: holds Qt6Core.dll open
//! until ended, or with OBS's command line writes args.txt and shows a window a WM_CLOSE ends.
fn main() {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_default();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let windowed = args.iter().any(|a| a == "--profile");
    #[cfg(windows)]
    let _held = {
        use std::os::windows::fs::OpenOptionsExt;
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(dir.join("Qt6Core.dll"));
        if !windowed {
            held.as_ref().expect("Qt6Core.dll beside the stand-in");
        }
        held
    };
    if windowed {
        let _ = std::fs::write(dir.join("args.txt"), args.join(" "));
        #[cfg(windows)]
        window::run(&dir);
        return;
    }
    println!("holding {}", dir.join("Qt6Core.dll").display());
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

#[cfg(windows)]
mod window {
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        PostQuitMessage, RegisterClassW, ShowWindow, TranslateMessage, CW_USEDEFAULT, MSG,
        SW_SHOWNORMAL, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };

    static DIR: OnceLock<PathBuf> = OnceLock::new();

    extern "system" fn on(h: HWND, m: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        unsafe {
            match m {
                WM_CLOSE => {
                    if let Some(d) = DIR.get() {
                        let _ = std::fs::write(d.join("closed.txt"), "closed");
                    }
                    let _ = DestroyWindow(h);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                _ => DefWindowProcW(h, m, wp, lp),
            }
        }
    }

    pub fn run(dir: &Path) {
        let _ = DIR.set(dir.to_path_buf());
        unsafe {
            let module = GetModuleHandleW(None).unwrap_or_default();
            let class = w!("ReelObsStandin");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(on),
                hInstance: module.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            if let Ok(h) = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                w!("OBS 32.1.0 - Portable Mode - Profile: reel - Scenes: reel"),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                640,
                400,
                None,
                None,
                module,
                None,
            ) {
                let _ = ShowWindow(h, SW_SHOWNORMAL);
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
