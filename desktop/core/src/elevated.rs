//! The OBS for games that run as administrator: a copy in Program Files, made by reel-elevate.
use crate::bundled::{Bundled, Platform, OBS_EXE, OBS_EXE_BEFORE};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Makes or remakes the copy, with its settings, rights and tasks.
    Install,
    /// The settings alone, when the setup changes.
    Write,
    /// Removes the tasks and the copy.
    Remove,
}

impl Step {
    pub fn word(self) -> &'static str {
        match self {
            Step::Install => "install",
            Step::Write => "write",
            Step::Remove => "remove",
        }
    }

    pub fn from_word(w: &str) -> Option<Step> {
        [Step::Install, Step::Write, Step::Remove]
            .into_iter()
            .find(|s| s.word() == w)
    }
}

/// The helper's file name, beside the app and in the copy's folder.
pub const HELPER: &str = "reel-elevate.exe";

/// The task's description, as Task Scheduler shows it.
pub const TASK_ABOUT: &str = "Starts the OBS that came with reel, copied into Program Files, with the highest rights, so its game capture reaches games that run as administrator; reel's helper starts it hidden, without a tray icon. reel runs it and ends it; reel itself does not run with these rights.";

/// The second task's description.
pub const WINDOW_ABOUT: &str = "Shows, hides or closes the OBS that came with reel, in Program Files, when reel asks: a program with the highest rights takes no such request from reel itself. reel runs it; reel itself does not run with these rights.";

/// What the second task does to the copy's OBS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Show,
    Hide,
    /// As its window's close button does; ended after the grace.
    Close,
}

impl Window {
    pub fn word(self) -> &'static str {
        match self {
            Window::Show => "show",
            Window::Hide => "hide",
            Window::Close => "close",
        }
    }

    pub fn from_word(w: &str) -> Option<Window> {
        [Window::Show, Window::Hide, Window::Close]
            .into_iter()
            .find(|v| v.word() == w)
    }
}

/// How long the copy's OBS gets to stop its outputs and exit before the helper ends it.
pub const GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// What the helper's command line asks: a step, or one of the tasks' commands.
#[derive(Debug, PartialEq, Eq)]
pub enum Asked {
    Step(Step, PathBuf, Option<PathBuf>, String),
    /// Still the first action of the sign-in task of a copy an older version made.
    ClearMarks,
    Start,
    Window(Window),
}

/// Exactly the words of one of the helper's uses, nothing more.
pub fn asked(args: &[String]) -> Option<Asked> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["clear-marks"] => Some(Asked::ClearMarks),
        ["start"] => Some(Asked::Start),
        ["window", w] => Window::from_word(w).map(Asked::Window),
        [step, install, rest @ ..] if rest.len() <= 2 => Some(Asked::Step(
            Step::from_word(step)?,
            PathBuf::from(install),
            rest.first().filter(|a| !a.is_empty()).map(PathBuf::from),
            rest.get(1).map(|u| u.to_string()).unwrap_or_default(),
        )),
        _ => None,
    }
}

/// A copy an older version made runs OBS as obs64.exe until it is made again.
pub fn copy(program_files: &Path) -> Bundled {
    let root = program_files.join("reel").join("obs");
    let bin = root.join("bin").join("64bit");
    Bundled {
        exe: [OBS_EXE, OBS_EXE_BEFORE]
            .iter()
            .map(|n| bin.join(n))
            .find(|p| p.is_file())
            .unwrap_or_else(|| bin.join(OBS_EXE)),
        config_root: root.join("config"),
        platform: Platform::Windows,
        flatpak: false,
    }
}

/// Whether the copy still runs OBS under its old name and has to be made again.
pub fn copy_named_before(copy: &Bundled) -> bool {
    copy.exe
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case(OBS_EXE_BEFORE))
}

/// The OBS version the copy was made from, as the helper wrote it; None without a copy.
pub fn copy_version(copy: &Bundled) -> Option<String> {
    let obs = copy.config_root.parent()?;
    std::fs::read_to_string(obs.join("reel-version.txt"))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Asked once per app version: after a No or a failure only the next version asks again.
pub fn update_at_start(on: bool, there: bool, stale: bool, asked_at: &str, version: &str) -> bool {
    on && there && stale && asked_at != version
}

/// No plugin but its own: OBS also loads plugins from OBS_PLUGINS_PATH, which any process of
/// the user can set.
pub fn copy_args(copy: &Bundled) -> Vec<String> {
    let mut a = copy.args(false);
    a.push("--only-bundled-plugins".into());
    a
}

/// No tray icon: the helper starts the copy's OBS hidden and the second task shows it.
pub fn without_tray(user_ini: &str) -> String {
    [
        "SysTrayEnabled",
        "SysTrayWhenStarted",
        "SysTrayMinimizeToTray",
    ]
    .iter()
    .fold(user_ini.to_string(), |t, k| {
        crate::bundled::ini_set(&t, "BasicWindow", k, "false")
    })
}

/// Turns the tray icon off again just before OBS starts, as OBS reads user.ini only then;
/// whether it was written.
pub fn tray_off_before_start(copy: &Bundled) -> std::io::Result<bool> {
    if tray_on(copy) != Some(true) {
        return Ok(false);
    }
    let file = copy.config_root.join("obs-studio").join("user.ini");
    let text = std::fs::read_to_string(&file)?;
    write_whole(&file, &without_tray(&text))?;
    Ok(true)
}

/// A file written beside, then put in place: OBS starting meanwhile never reads half of it.
fn write_whole(to: &Path, text: &str) -> std::io::Result<()> {
    let part = to.with_extension("reel-part");
    std::fs::write(&part, text)?;
    std::fs::rename(&part, to)
}

/// On unless user.ini says SysTrayEnabled=false, as OBS reads it; None without a user.ini.
pub fn tray_on(copy: &Bundled) -> Option<bool> {
    let text =
        std::fs::read_to_string(copy.config_root.join("obs-studio").join("user.ini")).ok()?;
    Some(
        crate::bundled::ini_value(&text, "BasicWindow", "SysTrayEnabled").as_deref()
            != Some("false"),
    )
}

/// The bundled OBS's own folder (`obs`), from its program's path.
pub fn obs_root(b: &Bundled) -> PathBuf {
    b.exe
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// Where the app stages the copy's settings for the helper, in the install.
pub fn staging(b: &Bundled) -> PathBuf {
    obs_root(b).join("copy-settings")
}

/// Puts the settings the copy gets where the helper takes them, nothing else left there.
pub fn stage(b: &Bundled, files: &[(String, String)]) -> std::io::Result<()> {
    let dir = staging(b);
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, text) in files {
        let p = dir.join(rel);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, text)?;
    }
    Ok(())
}

/// Plain quoted words, nothing encoded.
pub fn helper_args(step: Step, install: &Path, answer: &Path, user: &str) -> String {
    format!(
        "{} \"{}\" \"{}\" \"{}\"",
        step.word(),
        install.display(),
        answer.display(),
        user
    )
}

/// The settings files the helper writes into the copy, and no other.
pub const SETTINGS: [&str; 6] = [
    "obs-studio/global.ini",
    "obs-studio/user.ini",
    "obs-studio/basic/profiles/reel/basic.ini",
    "obs-studio/basic/profiles/reel/recordEncoder.json",
    "obs-studio/basic/scenes/reel.json",
    "obs-studio/plugin_config/obs-websocket/config.json",
];

/// At most a megabyte, JSON that parses, and no scripts in the scene collection, as the copy
/// runs with the highest rights.
pub fn clean_setting(rel: &str, text: &str) -> Result<String, String> {
    if text.len() > 1 << 20 {
        return Err(format!("{rel} is larger than a settings file of reel's"));
    }
    if !rel.ends_with(".json") {
        return Ok(text.to_string());
    }
    let mut v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("{rel} is no JSON: {e}"))?;
    if rel.ends_with("scenes/reel.json") {
        if let Some(m) = v.get_mut("modules").and_then(|m| m.as_object_mut()) {
            m.remove("scripts-tool");
        }
    }
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

/// Takes the staged settings into the copy's config folder, the known files only.
pub fn take_settings(staging: &Path, config: &Path, tray_off: bool) -> Result<usize, String> {
    let mut n = 0;
    for rel in SETTINGS {
        let from = staging.join(rel);
        let Ok(text) = std::fs::read_to_string(&from) else {
            continue;
        };
        let mut text = clean_setting(rel, &text)?;
        if tray_off && rel == "obs-studio/user.ini" {
            text = without_tray(&text);
        }
        let to = config.join(rel);
        if let Some(d) = to.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        write_whole(&to, &text).map_err(|e| format!("{}: {e}", to.display()))?;
        n += 1;
    }
    if n == 0 {
        return Err(format!("no settings for the copy in {}", staging.display()));
    }
    Ok(n)
}

/// Makes `dst` hold what `src` holds: new or changed files copied, the rest removed.
pub fn mirror(src: &Path, dst: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(dst)?;
    let mut n = 0;
    let mut keep = std::collections::HashSet::new();
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let (from, to) = (e.path(), dst.join(e.file_name()));
        keep.insert(e.file_name());
        if e.file_type()?.is_dir() {
            if to.is_file() {
                std::fs::remove_file(&to)?;
            }
            n += mirror(&from, &to)?;
        } else {
            if to.is_dir() {
                std::fs::remove_dir_all(&to)?;
            }
            let same = match (std::fs::metadata(&from), std::fs::metadata(&to)) {
                (Ok(a), Ok(b)) => {
                    a.len() == b.len() && std::fs::read(&from)? == std::fs::read(&to)?
                }
                _ => false,
            };
            if !same {
                std::fs::copy(&from, &to)?;
                n += 1;
            }
        }
    }
    for e in std::fs::read_dir(dst)? {
        let e = e?;
        if !keep.contains(&e.file_name()) {
            if e.file_type()?.is_dir() {
                std::fs::remove_dir_all(e.path())?;
            } else {
                std::fs::remove_file(e.path())?;
            }
        }
    }
    Ok(n)
}

/// Where OBS's Windows zip keeps its own plugins: `obs-plugins` up to 32, `core` from 33.
const PLUGINS: [&str; 2] = ["core", "obs-plugins"];

/// The copy loses the plugins folder the bundled OBS lacks, so nothing of an older OBS stays
/// where 33 still looks for plugins.
pub fn put_obs(source_obs: &Path, copy_obs: &Path) -> std::io::Result<usize> {
    if !PLUGINS.iter().any(|d| source_obs.join(d).is_dir()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "no {} folder in {}",
                PLUGINS.join(" or "),
                source_obs.display()
            ),
        ));
    }
    let mut n = 0;
    for d in ["bin", "data"] {
        n += mirror(&source_obs.join(d), &copy_obs.join(d))?;
    }
    for d in PLUGINS {
        let (from, to) = (source_obs.join(d), copy_obs.join(d));
        if from.is_dir() {
            n += mirror(&from, &to)?;
        } else if to.is_dir() {
            std::fs::remove_dir_all(&to)?;
        }
    }
    std::fs::copy(
        source_obs.join("portable_mode.txt"),
        copy_obs.join("portable_mode.txt"),
    )?;
    std::fs::write(
        copy_obs.join("reel-version.txt"),
        crate::bundled::pinned().version,
    )?;
    Ok(n)
}

/// Clears OBS's crash marks, as Task Scheduler ends OBS without letting it close.
pub fn clear_marks(config: &Path) {
    if let Ok(entries) = std::fs::read_dir(config.join("obs-studio").join(".sentinel")) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().starts_with("run_") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

const ADMINS: [&str; 5] = [
    "BA",
    "SY",
    "S-1-5-32-544",
    "S-1-5-18",
    "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464",
];

/// What a right in an SDDL entry allows, as a mask; a name not known counts as everything.
fn mask(rights: &str) -> u64 {
    if let Some(hex) = rights
        .strip_prefix("0x")
        .or_else(|| rights.strip_prefix("0X"))
    {
        return u64::from_str_radix(hex, 16).unwrap_or(u64::MAX);
    }
    let named = |k: &str| -> Option<u64> {
        Some(match k {
            "FA" => 0x1F01FF,
            "FR" => 0x120089,
            "FW" => 0x120116,
            "FX" => 0x1200A0,
            "GA" => 0x10000000,
            "GR" => 0x80000000,
            "GW" => 0x40000000,
            "GX" => 0x20000000,
            "SD" => 0x10000,
            "RC" => 0x20000,
            "WD" => 0x40000,
            "WO" => 0x80000,
            "CC" => 0x1,
            "DC" => 0x2,
            "LC" => 0x4,
            "SW" => 0x8,
            "RP" => 0x10,
            "WP" => 0x20,
            "DT" => 0x40,
            "LO" => 0x80,
            "CR" => 0x100,
            _ => return None,
        })
    };
    let chars: Vec<char> = rights.chars().collect();
    let mut m = 0;
    for pair in chars.chunks(2) {
        let k: String = pair.iter().collect();
        match named(&k) {
            Some(v) => m |= v,
            None => return u64::MAX,
        }
    }
    m
}

/// What keeps an item's SDDL from being Program Files' own: an owner but the administrators,
/// an entry not inherited, or one letting anyone else change it. None when nothing does.
pub fn rights_problem(path: &str, sddl: &str) -> Option<String> {
    const WRITE: u64 =
        0x2 | 0x4 | 0x10 | 0x40 | 0x100 | 0x10000 | 0x40000 | 0x80000 | 0x10000000 | 0x40000000;
    let Some(rest) = sddl.strip_prefix("O:") else {
        return Some(format!("{path}: its rights name no owner"));
    };
    let owner_end = ["G:", "D:"]
        .iter()
        .filter_map(|m| rest.find(m))
        .min()
        .unwrap_or(rest.len());
    let owner = &rest[..owner_end];
    if !ADMINS.contains(&owner) {
        return Some(format!("{path}: owned by {owner}, not the administrators"));
    }
    let Some(at) = sddl.find("D:") else {
        return Some(format!("{path}: no access list"));
    };
    let mut dacl = &sddl[at + 2..];
    if let Some(s) = dacl.find("S:") {
        dacl = &dacl[..s];
    }
    let flags = &dacl[..dacl.find('(').unwrap_or(dacl.len())];
    if flags.contains('P') {
        return Some(format!(
            "{path}: its rights do not follow Program Files' (inheritance is off)"
        ));
    }
    for ace in dacl.split('(').skip(1) {
        let ace = ace.trim_end_matches(')');
        let f: Vec<&str> = ace.split(';').collect();
        if f.len() < 6 {
            return Some(format!(
                "{path}: an entry of its rights that cannot be read, ({ace})"
            ));
        }
        if f[1].contains("IO") {
            continue;
        }
        if !f[1].contains("ID") {
            return Some(format!(
                "{path}: a right of its own for {}, not one from Program Files",
                f[5]
            ));
        }
        if f[0] == "A" && mask(f[2]) & WRITE != 0 && !ADMINS.contains(&f[5]) {
            return Some(format!("{path}: {} may change it", f[5]));
        }
    }
    None
}

/// Every item under `root`, `root` first, each folder before what is in it.
pub fn items(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    let mut i = 0;
    while i < out.len() {
        if out[i].is_dir() {
            if let Ok(entries) = std::fs::read_dir(&out[i]) {
                let mut more: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
                more.sort();
                out.extend(more);
            }
        }
        i += 1;
    }
    out
}

#[cfg(windows)]
pub mod win {
    use super::items;
    use std::path::Path;
    use windows::core::{Interface, BSTR, HSTRING, PWSTR, VARIANT};
    use windows::Win32::Foundation::{LocalFree, HLOCAL, VARIANT_FALSE, VARIANT_TRUE};
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
        SetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows::Win32::Security::{
        CreateWellKnownSid, InitializeAcl, WinBuiltinAdministratorsSid, ACL, ACL_REVISION,
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        UNPROTECTED_DACL_SECURITY_INFORMATION,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::TaskScheduler::{
        IExecAction, ILogonTrigger, ITaskDefinition, ITaskFolder, ITaskService, TaskScheduler,
        TASK_ACTION_EXEC, TASK_CREATE_OR_UPDATE, TASK_INSTANCES_IGNORE_NEW,
        TASK_INSTANCES_PARALLEL, TASK_INSTANCES_POLICY, TASK_LOGON_INTERACTIVE_TOKEN,
        TASK_RUNLEVEL_HIGHEST, TASK_TRIGGER_LOGON,
    };

    /// Each item under `root` owned by the Administrators, with only what it inherits.
    pub fn secure(root: &Path) -> Result<(), String> {
        let mut sid = [0u8; 68];
        let mut size = sid.len() as u32;
        let mut acl = [0u8; 64];
        unsafe {
            CreateWellKnownSid(
                WinBuiltinAdministratorsSid,
                None,
                PSID(sid.as_mut_ptr() as *mut _),
                &mut size,
            )
            .map_err(|e| format!("the Administrators' SID: {e}"))?;
            InitializeAcl(acl.as_mut_ptr() as *mut ACL, acl.len() as u32, ACL_REVISION)
                .map_err(|e| format!("an empty access list: {e}"))?;
        }
        for p in items(root) {
            let name = HSTRING::from(p.as_os_str());
            let r = unsafe {
                SetNamedSecurityInfoW(
                    &name,
                    SE_FILE_OBJECT,
                    OWNER_SECURITY_INFORMATION
                        | DACL_SECURITY_INFORMATION
                        | UNPROTECTED_DACL_SECURITY_INFORMATION,
                    PSID(sid.as_mut_ptr() as *mut _),
                    PSID::default(),
                    Some(acl.as_ptr() as *const ACL),
                    None,
                )
            };
            if r.is_err() {
                return Err(format!(
                    "the rights of {} could not be set ({})",
                    p.display(),
                    r.0
                ));
            }
        }
        Ok(())
    }

    /// An item's owner and access list as SDDL.
    pub fn sddl(p: &Path) -> Result<String, String> {
        let name = HSTRING::from(p.as_os_str());
        let mut sd = PSECURITY_DESCRIPTOR::default();
        unsafe {
            let r = GetNamedSecurityInfoW(
                &name,
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                None,
                None,
                None,
                None,
                &mut sd,
            );
            if r.is_err() {
                return Err(format!(
                    "the rights of {} could not be read ({})",
                    p.display(),
                    r.0
                ));
            }
            let mut s = PWSTR::null();
            let got = ConvertSecurityDescriptorToStringSecurityDescriptorW(
                sd,
                SDDL_REVISION_1,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut s,
                None,
            );
            let _ = LocalFree(HLOCAL(sd.0));
            got.map_err(|e| format!("the rights of {}: {e}", p.display()))?;
            let text = s.to_string().unwrap_or_default();
            let _ = LocalFree(HLOCAL(s.0 as *mut _));
            Ok(text)
        }
    }

    /// What is wrong with the rights of anything under `root`; None when nothing is.
    pub fn check(root: &Path) -> Option<String> {
        for p in items(root) {
            let shown = p.display().to_string();
            match sddl(&p) {
                Ok(s) => {
                    if let Some(problem) = super::rights_problem(&shown, &s) {
                        return Some(problem);
                    }
                }
                Err(e) => return Some(e),
            }
        }
        None
    }

    fn service() -> Result<ITaskService, String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let svc: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("Task Scheduler: {e}"))?;
            let none = VARIANT::default();
            svc.Connect(&none, &none, &none, &none)
                .map_err(|e| format!("Task Scheduler: {e}"))?;
            Ok(svc)
        }
    }

    fn folder(svc: &ITaskService) -> Result<ITaskFolder, String> {
        unsafe {
            svc.GetFolder(&BSTR::from("\\"))
                .map_err(|e| format!("Task Scheduler's folder: {e}"))
        }
    }

    fn definition(
        svc: &ITaskService,
        user: &str,
        about: &str,
        instances: TASK_INSTANCES_POLICY,
        limit: &str,
    ) -> Result<ITaskDefinition, String> {
        let e = |what: &'static str| {
            move |err: windows::core::Error| format!("the task, {what}: {err}")
        };
        unsafe {
            let def = svc.NewTask(0).map_err(e("a new one"))?;
            def.RegistrationInfo()
                .and_then(|r| r.SetDescription(&BSTR::from(about)))
                .map_err(e("its description"))?;
            let p = def.Principal().map_err(e("its user"))?;
            p.SetUserId(&BSTR::from(user)).map_err(e("its user"))?;
            p.SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)
                .map_err(e("its user"))?;
            p.SetRunLevel(TASK_RUNLEVEL_HIGHEST)
                .map_err(e("its rights"))?;
            let s = def.Settings().map_err(e("its settings"))?;
            s.SetDisallowStartIfOnBatteries(VARIANT_FALSE)
                .map_err(e("its settings"))?;
            s.SetStopIfGoingOnBatteries(VARIANT_FALSE)
                .map_err(e("its settings"))?;
            s.SetExecutionTimeLimit(&BSTR::from(limit))
                .map_err(e("its settings"))?;
            s.SetStartWhenAvailable(VARIANT_TRUE)
                .map_err(e("its settings"))?;
            s.SetMultipleInstances(instances)
                .map_err(e("its settings"))?;
            s.SetPriority(5).map_err(e("its settings"))?;
            if let Ok(idle) = s.IdleSettings() {
                let _ = idle.SetStopOnIdleEnd(VARIANT_FALSE);
            }
            Ok(def)
        }
    }

    /// One action: the copy's helper with `args`, in its own folder.
    fn action(def: &ITaskDefinition, helper: &Path, args: &str) -> Result<(), String> {
        let e = |err: windows::core::Error| format!("the task, its action: {err}");
        unsafe {
            let a: IExecAction = def
                .Actions()
                .and_then(|a| a.Create(TASK_ACTION_EXEC))
                .and_then(|a| a.cast())
                .map_err(e)?;
            a.SetPath(&BSTR::from(helper.display().to_string()))
                .map_err(e)?;
            a.SetArguments(&BSTR::from(args)).map_err(e)?;
            if let Some(dir) = helper.parent() {
                a.SetWorkingDirectory(&BSTR::from(dir.display().to_string()))
                    .map_err(e)?;
            }
        }
        Ok(())
    }

    fn register(
        svc: &ITaskService,
        name: &str,
        def: &ITaskDefinition,
        user: &str,
    ) -> Result<(), String> {
        unsafe {
            folder(svc)?
                .RegisterTaskDefinition(
                    &BSTR::from(name),
                    def,
                    TASK_CREATE_OR_UPDATE.0,
                    &VARIANT::from(BSTR::from(user)),
                    &VARIANT::default(),
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &VARIANT::default(),
                )
                .map_err(|e| format!("the task {name}, registering it: {e}"))?;
        }
        Ok(())
    }

    /// At sign-in, `start`, which stays until OBS exits; on demand, `window` with the app's
    /// word in `$(Arg0)`.
    pub fn register_tasks(user: &str, helper: &Path) -> Result<(), String> {
        let svc = service()?;
        let start = definition(
            &svc,
            user,
            super::TASK_ABOUT,
            TASK_INSTANCES_IGNORE_NEW,
            "PT0S",
        )?;
        unsafe {
            let t: ILogonTrigger = start
                .Triggers()
                .and_then(|t| t.Create(TASK_TRIGGER_LOGON))
                .and_then(|t| t.cast())
                .map_err(|e| format!("the task, its trigger: {e}"))?;
            t.SetUserId(&BSTR::from(user))
                .map_err(|e| format!("the task, its trigger: {e}"))?;
        }
        action(&start, helper, "start")?;
        register(&svc, crate::obsproc::TASK, &start, user)?;
        let window = definition(
            &svc,
            user,
            super::WINDOW_ABOUT,
            TASK_INSTANCES_PARALLEL,
            "PT1M",
        )?;
        action(&window, helper, "window $(Arg0)")?;
        register(&svc, crate::obsproc::TASK_WINDOW, &window, user)
    }

    /// Ends the task, and removes it with the second one when `remove`.
    pub fn end_task(remove: bool) -> Result<(), String> {
        let svc = service()?;
        let folder = folder(&svc)?;
        unsafe {
            if let Ok(t) = folder.GetTask(&BSTR::from(crate::obsproc::TASK)) {
                let _ = t.Stop(0);
                if remove {
                    folder
                        .DeleteTask(&BSTR::from(crate::obsproc::TASK), 0)
                        .map_err(|e| format!("the task was not removed: {e}"))?;
                }
            }
            if remove
                && folder
                    .GetTask(&BSTR::from(crate::obsproc::TASK_WINDOW))
                    .is_ok()
            {
                folder
                    .DeleteTask(&BSTR::from(crate::obsproc::TASK_WINDOW), 0)
                    .map_err(|e| format!("the second task was not removed: {e}"))?;
            }
        }
        Ok(())
    }

    /// Ends every process that runs from `dir`, and waits for them.
    pub fn end_processes(dir: &Path) -> Result<(), String> {
        let d = dir.display().to_string();
        for _ in 0..40 {
            let left: Vec<u32> = crate::obsproc::processes()
                .into_iter()
                .filter(|p| {
                    p.path
                        .as_deref()
                        .is_some_and(|x| crate::install::under(x, &d))
                })
                .map(|p| p.pid)
                .collect();
            if left.is_empty() {
                return Ok(());
            }
            for p in left {
                crate::obsproc::kill(p);
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        Err(format!("OBS in {d} did not stop"))
    }
}

#[cfg(test)]
mod tests {
    use super::update_at_start;

    #[test]
    fn the_copy_is_updated_at_a_start_once_a_version_and_only_when_it_is_stale() {
        assert!(
            update_at_start(true, true, true, "", "1.4.2"),
            "the first start with a stale copy"
        );
        assert!(
            update_at_start(true, true, true, "1.4.1", "1.4.2"),
            "the first start after an update"
        );
        assert!(
            !update_at_start(true, true, true, "1.4.2", "1.4.2"),
            "asked at this version: a No or a failure waits for the next"
        );
        assert!(
            !update_at_start(true, true, false, "", "1.4.2"),
            "the copy is this reel's OBS"
        );
        assert!(
            !update_at_start(true, false, true, "", "1.4.2"),
            "no copy: Put it in place is the user's"
        );
        assert!(
            !update_at_start(false, true, true, "", "1.4.2"),
            "the switch off"
        );
    }

    use super::*;
    use crate::bundled::ObsSetup;

    const TI: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";
    const ME: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";

    #[test]
    fn the_rights_check_tells_program_files_own_from_a_folder_the_user_could_change() {
        let pf = format!("O:BAG:SYD:AI(A;OICIIOID;GA;;;CO)(A;OICIID;FA;;;SY)(A;OICIID;FA;;;BA)(A;CIID;FA;;;{TI})(A;OICIID;0x1200a9;;;BU)(A;OICIID;0x1200a9;;;AC)(A;OICIID;0x1200a9;;;S-1-15-2-2)");
        let cases: Vec<(&str, String, Option<&str>)> = vec![
            (
                "Program Files' own, owned by the administrators",
                pf.clone(),
                None,
            ),
            (
                "owned by SYSTEM, a SACL after the list",
                "O:SYG:SYD:AI(A;OICIID;FA;;;SY)(A;OICIID;FA;;;BA)S:AI(AU;SAFA;FA;;;WD)".into(),
                None,
            ),
            ("no group in it", "O:BAD:AI(A;OICIID;FA;;;SY)".into(), None),
            (
                "an inherited refusal for everyone",
                format!("{pf}(D;OICIID;FA;;;WD)"),
                None,
            ),
            (
                "owned by the user",
                format!("O:{ME}G:SYD:AI(A;OICIID;FA;;;SY)"),
                Some("owned by"),
            ),
            (
                "the user may change it",
                format!("O:BAG:SYD:AI(A;OICIID;FA;;;SY)(A;OICIID;FA;;;{ME})"),
                Some("may change it"),
            ),
            (
                "Users may modify it",
                "O:BAG:SYD:AI(A;OICIID;0x1301bf;;;BU)".into(),
                Some("BU may change it"),
            ),
            (
                "Authenticated Users may write",
                "O:BAG:SYD:AI(A;OICIID;FW;;;AU)".into(),
                Some("AU may change it"),
            ),
            (
                "everyone, in letters",
                "O:BAG:SYD:AI(A;OICIID;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;WD)".into(),
                Some("WD may change it"),
            ),
            (
                "a right of an unknown name",
                "O:BAG:SYD:AI(A;OICIID;ZZ;;;IU)".into(),
                Some("may change it"),
            ),
            (
                "a right of its own",
                "O:BAG:SYD:AI(A;OICI;FA;;;BA)".into(),
                Some("of its own"),
            ),
            (
                "inheritance off",
                "O:BAG:SYD:PAI(A;OICI;FA;;;SY)".into(),
                Some("inheritance is off"),
            ),
            (
                "a folder like %LOCALAPPDATA%",
                format!("O:{ME}G:{ME}D:(A;OICIID;FA;;;SY)(A;OICIID;FA;;;BA)(A;OICIID;FA;;;{ME})"),
                Some("owned by"),
            ),
            (
                "no owner",
                "D:AI(A;OICIID;FA;;;SY)".into(),
                Some("no owner"),
            ),
        ];
        for (what, sddl, want) in cases {
            let got = rights_problem(r"C:\Program Files\reel\obs\x.dll", &sddl);
            match want {
                None => assert!(got.is_none(), "{what}: {got:?}"),
                Some(w) => assert!(
                    got.as_deref().is_some_and(|g| g.contains(w)),
                    "{what}: {got:?}"
                ),
            }
        }
    }

    #[test]
    fn the_helper_mirrors_the_bundled_obs_and_takes_only_known_settings_without_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("install/obs");
        let dst = dir.path().join("pf/reel/obs");
        for (p, t) in [
            ("bin/64bit/reel-capture.exe", "obs 2"),
            ("data/a.txt", "a"),
            ("obs-plugins/64bit/p.dll", "p"),
            ("portable_mode.txt", ""),
        ] {
            std::fs::create_dir_all(src.join(p).parent().unwrap()).unwrap();
            std::fs::write(src.join(p), t).unwrap();
        }
        std::fs::create_dir_all(dst.join("obs-plugins/64bit")).unwrap();
        std::fs::write(dst.join("obs-plugins/64bit/old.dll"), "an older OBS's").unwrap();
        std::fs::create_dir_all(dst.join("config/obs-studio/logs")).unwrap();
        std::fs::write(dst.join("config/obs-studio/logs/a.txt"), "a log").unwrap();
        assert_eq!(put_obs(&src, &dst).unwrap(), 3);
        assert_eq!(
            std::fs::read_to_string(dst.join("bin/64bit/reel-capture.exe")).unwrap(),
            "obs 2"
        );
        assert!(
            !dst.join("obs-plugins/64bit/old.dll").exists(),
            "nothing of an older OBS left"
        );
        assert!(
            dst.join("config/obs-studio/logs/a.txt").exists(),
            "the copy's own settings kept"
        );
        assert!(dst.join("portable_mode.txt").exists());
        let c = Bundled {
            exe: dst.join("bin/64bit/reel-capture.exe"),
            config_root: dst.join("config"),
            platform: Platform::Windows,
            flatpak: false,
        };
        assert_eq!(
            copy_version(&c).as_deref(),
            Some(crate::bundled::pinned().version.as_str())
        );
        assert_eq!(put_obs(&src, &dst).unwrap(), 0, "nothing copied twice");

        let b = Bundled::windows(&dir.path().join("install"));
        let mut files = Bundled {
            config_root: dir.path().join("none"),
            ..b.clone()
        }
        .whole(
            &ObsSetup {
                elevated: true,
                ..ObsSetup::default()
            },
            Path::new("C:/clips"),
            "Alt+F10",
        );
        for (rel, text) in files.iter_mut() {
            if rel.ends_with("scenes/reel.json") {
                let mut v: serde_json::Value = serde_json::from_str(text).unwrap();
                v["modules"]["scripts-tool"] =
                    serde_json::json!([{ "path": "C:/Users/x/evil.lua", "settings": {} }]);
                v["modules"]["auto-scene-switcher"] = serde_json::json!({ "active": false });
                *text = serde_json::to_string(&v).unwrap();
            }
        }
        files.push(("obs-studio/basic/scenes/other.json".into(), "{}".into()));
        stage(&b, &files).unwrap();
        assert!(staging(&b).starts_with(dir.path().join("install/obs")));
        let n = take_settings(&staging(&b), &dst.join("config"), true).unwrap();
        assert_eq!(n, 6, "the six files of reel's, nothing else");
        let user = std::fs::read_to_string(dst.join("config/obs-studio/user.ini")).unwrap();
        for k in [
            "SysTrayEnabled",
            "SysTrayWhenStarted",
            "SysTrayMinimizeToTray",
        ] {
            assert!(
                user.contains(&format!("{k}=false")) && !user.contains(&format!("{k}=true")),
                "no tray icon of OBS's for the task that starts it hidden: {k}"
            );
        }
        assert!(user.contains("ConfirmOnExit=false"), "the rest as staged");
        let the_copy = Bundled {
            config_root: dst.join("config"),
            ..b.clone()
        };
        assert_eq!(tray_on(&the_copy), Some(false), "read back as written");
        take_settings(&staging(&b), &dst.join("config"), false).unwrap();
        let user = std::fs::read_to_string(dst.join("config/obs-studio/user.ini")).unwrap();
        assert!(
            user.contains("SysTrayEnabled=true"),
            "a copy whose task starts OBS in its tray keeps it"
        );
        assert_eq!(
            tray_on(&the_copy),
            Some(true),
            "an OBS that wrote its tray back"
        );
        assert!(
            tray_off_before_start(&the_copy).unwrap(),
            "set right as the task starts it hidden"
        );
        assert_eq!(tray_on(&the_copy), Some(false));
        assert!(!tray_off_before_start(&the_copy).unwrap(), "written once");
        let user = std::fs::read_to_string(dst.join("config/obs-studio/user.ini")).unwrap();
        assert!(user.contains("ConfirmOnExit=false"), "the rest kept");
        assert_eq!(
            tray_on(&Bundled {
                config_root: dst.join("none"),
                ..b.clone()
            }),
            None
        );
        assert!(!dst
            .join("config/obs-studio/basic/scenes/other.json")
            .exists());
        let c: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dst.join("config/obs-studio/basic/scenes/reel.json")).unwrap(),
        )
        .unwrap();
        assert!(
            c["modules"].get("scripts-tool").is_none(),
            "no script for an OBS with the highest rights"
        );
        assert_eq!(
            c["modules"]["auto-scene-switcher"]["active"], false,
            "the rest as it was"
        );
        assert!(clean_setting("obs-studio/basic/scenes/reel.json", "not json").is_err());
        assert!(clean_setting("obs-studio/user.ini", &"x".repeat(2 << 20)).is_err());
        std::fs::create_dir_all(dst.join("config/obs-studio/.sentinel")).unwrap();
        std::fs::write(dst.join("config/obs-studio/.sentinel/run_1"), "").unwrap();
        clear_marks(&dst.join("config"));
        assert!(!dst.join("config/obs-studio/.sentinel/run_1").exists());
        let all = items(&dir.path().join("pf/reel"));
        assert_eq!(all[0], dir.path().join("pf/reel"));
        let at = |end: &str| all.iter().position(|p| p.ends_with(end)).unwrap();
        assert!(
            at("obs") < at("reel-capture.exe"),
            "a folder before what is in it"
        );
    }

    #[test]
    fn the_copy_of_obs_32_made_again_from_33_takes_core_and_loses_obs_plugins() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("install/obs");
        let dst = dir.path().join("pf/reel/obs");
        let put = |root: &Path, files: &[(&str, &str)]| {
            for (p, t) in files {
                std::fs::create_dir_all(root.join(p).parent().unwrap()).unwrap();
                std::fs::write(root.join(p), t).unwrap();
            }
        };
        put(
            &src,
            &[
                ("bin/64bit/reel-capture.exe", "obs 33"),
                ("data/obs-studio/license/gplv2.txt", "gpl"),
                ("core/obs-websocket/obs-websocket.dll", "websocket 33"),
                ("core/win-capture/data/graphics-hook64.dll", "hook 33"),
                ("portable_mode.txt", ""),
                ("config/obs-studio/global.ini", "the bundled OBS's"),
                ("copy-settings/obs-studio/user.ini", "staged"),
            ],
        );
        put(
            &dst,
            &[
                ("bin/64bit/reel-capture.exe", "obs 32"),
                ("data/obs-plugins/obs-websocket/locale/en-US.ini", "32"),
                ("obs-plugins/64bit/obs-websocket.dll", "websocket 32"),
                ("config/obs-studio/user.ini", "the copy's"),
            ],
        );
        assert_eq!(put_obs(&src, &dst).unwrap(), 4);
        assert_eq!(
            std::fs::read_to_string(dst.join("core/obs-websocket/obs-websocket.dll")).unwrap(),
            "websocket 33"
        );
        assert!(dst
            .join("core/win-capture/data/graphics-hook64.dll")
            .exists());
        assert!(
            !dst.join("obs-plugins").exists() && !dst.join("data/obs-plugins").exists(),
            "nothing of OBS 32's plugins left"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("config/obs-studio/user.ini")).unwrap(),
            "the copy's"
        );
        assert!(
            !dst.join("config/obs-studio/global.ini").exists()
                && !dst.join("copy-settings").exists(),
            "no settings but the ones the helper takes"
        );
        assert_eq!(put_obs(&src, &dst).unwrap(), 0, "nothing copied twice");

        std::fs::remove_dir_all(src.join("core")).unwrap();
        put(
            &src,
            &[("obs-plugins/64bit/obs-websocket.dll", "websocket 32")],
        );
        assert_eq!(put_obs(&src, &dst).unwrap(), 1);
        assert!(
            dst.join("obs-plugins/64bit/obs-websocket.dll").exists() && !dst.join("core").exists()
        );

        std::fs::remove_dir_all(src.join("obs-plugins")).unwrap();
        let e = put_obs(&src, &dst).unwrap_err();
        assert!(
            e.to_string().contains("no core or obs-plugins folder"),
            "{e}"
        );
        assert!(dst.join("obs-plugins/64bit/obs-websocket.dll").exists());
    }

    #[test]
    fn the_helper_takes_exactly_the_words_of_one_of_its_uses() {
        let a = |w: &[&str]| asked(&w.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(a(&["start"]), Some(Asked::Start));
        assert_eq!(a(&["clear-marks"]), Some(Asked::ClearMarks));
        for w in [Window::Show, Window::Hide, Window::Close] {
            assert_eq!(a(&["window", w.word()]), Some(Asked::Window(w)));
        }
        assert_eq!(
            a(&["install", r"C:\reel", r"C:\t\a.txt", r"PC\me"]),
            Some(Asked::Step(
                Step::Install,
                PathBuf::from(r"C:\reel"),
                Some(PathBuf::from(r"C:\t\a.txt")),
                r"PC\me".into()
            ))
        );
        assert_eq!(
            a(&["remove", r"C:\reel", ""]),
            Some(Asked::Step(
                Step::Remove,
                PathBuf::from(r"C:\reel"),
                None,
                String::new()
            ))
        );
        // The second task puts the app's word in $(Arg0): anything else does nothing.
        for bad in [
            &[][..],
            &["window"],
            &["window", "show", "now"],
            &["window", "Show"],
            &["window", "\"show\""],
            &["window", r"C:\x.exe"],
            &["start", "x"],
            &["clear-marks", "x"],
            &["install"],
            &["install", r"C:\reel", "a", "b", "c"],
            &["run", r"C:\reel"],
        ] {
            assert_eq!(a(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_helper_is_called_with_plain_words() {
        let a = helper_args(
            Step::Install,
            Path::new(r"C:\Users\O'Neil\AppData\Local\reel"),
            Path::new(r"C:\Users\O'Neil\AppData\Local\Temp\reel-admin-x.txt"),
            r"PC\O'Neil",
        );
        assert_eq!(
            a,
            r#"install "C:\Users\O'Neil\AppData\Local\reel" "C:\Users\O'Neil\AppData\Local\Temp\reel-admin-x.txt" "PC\O'Neil""#
        );
        for s in [Step::Install, Step::Write, Step::Remove] {
            assert_eq!(Step::from_word(s.word()), Some(s));
        }
        let args = copy_args(&copy(Path::new(r"C:\Program Files")));
        assert_eq!(
            args.last().map(String::as_str),
            Some("--only-bundled-plugins")
        );
        assert!(args.iter().any(|a| a == "--portable"));
        assert!(
            !args.iter().any(|a| a == "--minimize-to-tray"),
            "the helper starts OBS hidden, not in a tray of its own"
        );
        let c = copy(Path::new(r"C:\Program Files"));
        assert!(c.exe.starts_with(r"C:\Program Files") && c.exe.ends_with("reel-capture.exe"));
        assert!(!copy_named_before(&c));
        let pf = tempfile::tempdir().unwrap();
        let bin = pf.path().join("reel/obs/bin/64bit");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("obs64.exe"), "OBS of a copy made before").unwrap();
        let before = copy(pf.path());
        assert!(
            before.exe.ends_with("obs64.exe") && copy_named_before(&before),
            "a copy from before runs on as it is, to be made again"
        );
        std::fs::write(bin.join("reel-capture.exe"), "OBS").unwrap();
        assert!(!copy_named_before(&copy(pf.path())));
    }
}
