//! The PC's audio devices as Windows lists them. A value is what OBS's WASAPI sources take as
//! `device_id`: an endpoint's id (`IMMDevice::GetId`), or "default".
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Device {
    pub value: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Devices {
    pub outputs: Vec<Device>,
    pub inputs: Vec<Device>,
    /// Discord is on this PC (its folder under %LOCALAPPDATA%).
    pub discord: bool,
}

/// None where the system gives no such list (Linux, where OBS's own list stands in).
pub fn devices() -> Option<Devices> {
    sys::devices()
}

/// `value` is the window as OBS's application audio capture names it.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Program {
    pub value: String,
    pub name: String,
}

/// A window as OBS names it: title, class and program, each with `#` and `:` written as
/// OBS writes them (`#22`, `#3A`, libobs/util/windows/window-helpers.c).
pub fn window_spec(title: &str, class: &str, exe: &str) -> String {
    let e = |s: &str| s.replace('#', "#22").replace(':', "#3A");
    format!("{}:{}:{}", e(title), e(class), e(exe))
}

pub fn spec_program(spec: &str) -> String {
    spec.rsplit(':')
        .next()
        .unwrap_or_default()
        .replace("#3A", ":")
        .replace("#22", "#")
}

/// The programs with a window, one each, by title; on Linux the programs playing sound.
pub fn programs() -> Vec<Program> {
    sys_programs()
}

#[cfg(windows)]
mod sys {
    use super::{Device, Devices};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::{
        eCapture, eRender, EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED, STGM_READ,
    };

    /// On a thread of its own, so COM is set up there and the caller's thread stays as it was.
    pub fn devices() -> Option<Devices> {
        std::thread::spawn(|| unsafe {
            let inited = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let found =
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .ok()
                    .map(|e| Devices {
                        outputs: list(&e, eRender),
                        inputs: list(&e, eCapture),
                        discord: std::env::var_os("LOCALAPPDATA")
                            .map(|d| std::path::Path::new(&d).join("Discord").is_dir())
                            .unwrap_or(false),
                    });
            if inited {
                CoUninitialize();
            }
            found
        })
        .join()
        .ok()
        .flatten()
    }

    unsafe fn list(e: &IMMDeviceEnumerator, flow: EDataFlow) -> Vec<Device> {
        let mut out = Vec::new();
        let Ok(all) = e.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) else {
            return out;
        };
        for i in 0..all.GetCount().unwrap_or(0) {
            let Ok(d) = all.Item(i) else { continue };
            let Ok(id) = d.GetId() else { continue };
            let value = id.to_string().unwrap_or_default();
            CoTaskMemFree(Some(id.0 as *const core::ffi::c_void));
            let name = d
                .OpenPropertyStore(STGM_READ)
                .and_then(|s| s.GetValue(&PKEY_Device_FriendlyName))
                .map(|v| v.to_string())
                .unwrap_or_default();
            if !value.is_empty() {
                out.push(Device {
                    name: if name.is_empty() { value.clone() } else { name },
                    value,
                });
            }
        }
        out
    }
}

#[cfg(windows)]
mod windows_list {
    use super::{window_spec, Program};
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindow, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, GW_OWNER,
    };

    const SKIP: &[&str] = &[
        "reel-desktop.exe",
        "obs64.exe",
        "reel-capture.exe",
        "explorer.exe",
        "textinputhost.exe",
        "applicationframehost.exe",
        "systemsettings.exe",
        "shellexperiencehost.exe",
        "searchhost.exe",
        "startmenuexperiencehost.exe",
    ];

    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = &mut *(lparam.0 as *mut Vec<Program>);
        if !IsWindowVisible(hwnd).as_bool()
            || GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid())
        {
            return BOOL(1);
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut buf) as usize;
        let title = String::from_utf16_lossy(&buf[..n]);
        if title.trim().is_empty() {
            return BOOL(1);
        }
        let n = GetClassNameW(hwnd, &mut buf) as usize;
        let class = String::from_utf16_lossy(&buf[..n]);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return BOOL(1);
        };
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let got =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        if got.is_err() {
            return BOOL(1);
        }
        let full = String::from_utf16_lossy(&path[..len as usize]);
        let exe = full.rsplit('\\').next().unwrap_or_default().to_string();
        if SKIP.contains(&exe.to_lowercase().as_str())
            || out
                .iter()
                .any(|p| super::spec_program(&p.value).eq_ignore_ascii_case(&exe))
        {
            return BOOL(1);
        }
        out.push(Program {
            value: window_spec(&title, &class, &exe),
            name: format!("{title} ({exe})"),
        });
        BOOL(1)
    }

    pub fn programs() -> Vec<Program> {
        let mut out: Vec<Program> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(each), LPARAM(&mut out as *mut Vec<Program> as isize));
        }
        out.sort_by_key(|p| p.name.to_lowercase());
        out
    }
}

#[cfg(windows)]
use windows_list::programs as sys_programs;

/// By the program's name, which OBS's PipeWire audio capture follows.
#[cfg(not(windows))]
fn sys_programs() -> Vec<Program> {
    reel_core::proc::command("pw-dump")
        .output()
        .ok()
        .map(|o| pipewire_programs(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

pub fn pipewire_programs(dump: &str) -> Vec<Program> {
    let Ok(serde_json::Value::Array(objects)) = serde_json::from_str::<serde_json::Value>(dump)
    else {
        return Vec::new();
    };
    let mut out: Vec<Program> = Vec::new();
    for o in &objects {
        let props = &o["info"]["props"];
        if props["media.class"] != "Stream/Output/Audio" {
            continue;
        }
        let Some(value) = props["application.process.binary"]
            .as_str()
            .or(props["application.name"].as_str())
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        if out.iter().any(|p| p.value == value) {
            continue;
        }
        let name = props["application.name"].as_str().unwrap_or(value);
        out.push(Program {
            value: value.to_string(),
            name: name.to_string(),
        });
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

#[cfg(not(windows))]
mod sys {
    pub fn devices() -> Option<super::Devices> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipewire_lists_the_programs_playing_sound_once_each() {
        let dump = r#"[
            {"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","node.name":"speakers"}}},
            {"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio","application.name":"Discord","application.process.binary":"Discord"}}},
            {"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio","application.name":"Discord","application.process.binary":"Discord"}}},
            {"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio","application.name":"Deep Rock Galactic","application.process.binary":"FSD-Win64-Shipping.exe"}}},
            {"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Input/Audio","application.name":"OBS"}}}
        ]"#;
        let p = pipewire_programs(dump);
        assert_eq!(
            p.iter()
                .map(|p| (p.value.as_str(), p.name.as_str()))
                .collect::<Vec<_>>(),
            [
                ("FSD-Win64-Shipping.exe", "Deep Rock Galactic"),
                ("Discord", "Discord")
            ]
        );
        assert!(pipewire_programs("not json").is_empty());
    }

    #[test]
    fn a_window_is_named_as_obs_names_it() {
        let spec = window_spec("Game: Episode #2", "UnrealWindow", "Game.exe");
        assert_eq!(spec, "Game#3A Episode #222:UnrealWindow:Game.exe");
        assert_eq!(spec_program(&spec), "Game.exe");
        assert_eq!(
            spec_program("Discord:Chrome_WidgetWin_1:Discord.exe"),
            "Discord.exe"
        );
    }
}
