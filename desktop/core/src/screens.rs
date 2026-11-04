//! The screens for the setup's capture step, the main one first. On Windows the id is the
//! monitor's device interface name, which OBS's monitor capture keys on.
use crate::bundled::Screen;

pub fn screens() -> Vec<Screen> {
    let mut s = sys::screens();
    s.sort_by_key(|x| !x.main);
    s
}

#[cfg(windows)]
mod sys {
    use crate::bundled::Screen;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, DEVMODEW,
        DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS, HDC, HMONITOR, MONITORINFOEXW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EDD_GET_DEVICE_INTERFACE_NAME, MONITORINFOF_PRIMARY,
    };

    fn text(buf: &[u16]) -> String {
        let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..n])
    }

    pub fn screens() -> Vec<Screen> {
        unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, l: LPARAM) -> BOOL {
            let out = &mut *(l.0 as *mut Vec<HMONITOR>);
            out.push(m);
            BOOL(1)
        }
        let mut monitors: Vec<HMONITOR> = Vec::new();
        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(each),
                LPARAM(&mut monitors as *mut Vec<HMONITOR> as isize),
            );
        }
        let mut out = Vec::new();
        for (i, m) in monitors.into_iter().enumerate() {
            unsafe {
                let mut info = MONITORINFOEXW::default();
                info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
                if !GetMonitorInfoW(m, &mut info.monitorInfo as *mut _ as *mut _).as_bool() {
                    continue;
                }
                let device = PCWSTR(info.szDevice.as_ptr());
                let mut mode = DEVMODEW {
                    dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                    ..Default::default()
                };
                let _ = EnumDisplaySettingsW(device, ENUM_CURRENT_SETTINGS, &mut mode);
                let mut dd = DISPLAY_DEVICEW {
                    cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                    ..Default::default()
                };
                let id = if EnumDisplayDevicesW(device, 0, &mut dd, EDD_GET_DEVICE_INTERFACE_NAME)
                    .as_bool()
                {
                    text(&dd.DeviceID)
                } else {
                    text(&info.szDevice)
                };
                let named = text(&dd.DeviceString);
                let r = info.monitorInfo.rcMonitor;
                let (w, h) = if mode.dmPelsWidth > 0 {
                    (mode.dmPelsWidth, mode.dmPelsHeight)
                } else {
                    ((r.right - r.left) as u32, (r.bottom - r.top) as u32)
                };
                out.push(Screen {
                    id,
                    name: if named.is_empty() || named.starts_with("Generic") {
                        format!("Screen {}", i + 1)
                    } else {
                        named
                    },
                    width: w,
                    height: h,
                    hz: mode.dmDisplayFrequency,
                    main: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                });
            }
        }
        out
    }
}

#[cfg(not(windows))]
mod sys {
    use crate::bundled::Screen;

    /// The X screen; on Wayland OBS records the screen picked through PipeWire instead.
    pub fn screens() -> Vec<Screen> {
        let out = reel_core::proc::command("xdpyinfo")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let (w, h) = out
            .lines()
            .find_map(|l| {
                let d = l.trim().strip_prefix("dimensions:")?.trim();
                let (wh, _) = d.split_once(' ')?;
                let (w, h) = wh.split_once('x')?;
                Some((w.parse().ok()?, h.parse().ok()?))
            })
            .unwrap_or((1920, 1080));
        let wayland = std::env::var("XDG_SESSION_TYPE").as_deref() == Ok("wayland")
            || std::env::var_os("WAYLAND_DISPLAY").is_some();
        let (id, name) = if wayland {
            (
                crate::bundled::PIPEWIRE,
                "The screen you pick when the desktop asks, once",
            )
        } else {
            ("0", "Screen 1")
        };
        vec![Screen {
            id: id.into(),
            name: name.into(),
            width: w,
            height: h,
            hz: 60,
            main: true,
        }]
    }
}
