//! reel-elevate's steps one by one, for its check under Wine, which keeps no Windows file rights.
//! Usage: elevate_parts <install folder> <copy root, ...\reel> <DOMAIN\user> | look | wait
//!        | old <DOMAIN\user> | apartment | hammer <install folder>
fn main() {
    #[cfg(windows)]
    {
        use reel_desktop_core::bundled::Bundled;
        use reel_desktop_core::elevated::{self, win};
        use reel_desktop_core::obsproc;
        use std::path::PathBuf;
        let a: Vec<String> = std::env::args().skip(1).collect();
        let copy = || elevated::copy(&obsproc::program_files().expect("Program Files"));
        if a.first().map(String::as_str) == Some("look") {
            let pids = obsproc::running(&copy()).ours;
            if pids.is_empty() {
                println!("none");
            }
            for p in pids {
                for w in obsproc::windows(p) {
                    if !w.owned && w.caption && obsproc::is_main(&w.title) {
                        println!(
                            "{p} {} {}",
                            if w.visible { "shown" } else { "hidden" },
                            if w.minimized { "minimized" } else { "normal" }
                        );
                    }
                }
            }
            return;
        }
        if a.first().map(String::as_str) == Some("wait") {
            let Some(p) = obsproc::running(&copy()).ours.first().copied() else {
                println!("none");
                return;
            };
            let helper = copy()
                .config_root
                .parent()
                .and_then(|o| o.parent())
                .map(|r| r.join(elevated::HELPER))
                .expect("the copy's helper");
            let hurry = std::sync::atomic::AtomicBool::new(true);
            let t = std::time::Instant::now();
            let _ = reel_core::proc::command(helper)
                .args(["window", "close"])
                .spawn();
            let code = obsproc::wait_pid(p, &hurry);
            println!("exit {code} seen after {:.2} s", t.elapsed().as_secs_f64());
            return;
        }
        if a.first().map(String::as_str) == Some("old") {
            match old::tasks(&a[1], &copy()) {
                Ok(()) => println!("old tasks: registered"),
                Err(e) => println!("old tasks FAILED: {e}"),
            }
            return;
        }
        if a.first().map(String::as_str) == Some("hammer") {
            hammer::run(&PathBuf::from(&a[1]));
            return;
        }
        if a.first().map(String::as_str) == Some("apartment") {
            use reel_desktop_core::elevated::Window;
            println!("second task: {}", obsproc::window_task_exists());
            println!("task: {}", obsproc::task_exists());
            println!("task running: {}", obsproc::task_running());
            println!("hide: {:?}", obsproc::window_run(Window::Hide).is_ok());
            obsproc::task_end();
            let ole = unsafe { windows::Win32::System::Ole::OleInitialize(None) };
            println!(
                "ole: {}",
                match ole {
                    Ok(()) => "S_OK".to_string(),
                    Err(e) => format!("{:#010x}", e.code().0),
                }
            );
            return;
        }
        let (install, root, user) = (PathBuf::from(&a[0]), PathBuf::from(&a[1]), &a[2]);
        let obs = root.join("obs");
        let source = Bundled::windows(&install);
        let say = |what: &str, r: Result<String, String>| match r {
            Ok(v) => println!("{what}: {v}"),
            Err(e) => println!("{what} FAILED: {e}"),
        };
        say(
            "copy",
            elevated::put_obs(&elevated::obs_root(&source), &obs)
                .map(|n| format!("{n} files"))
                .map_err(|e| e.to_string()),
        );
        say(
            "settings",
            elevated::take_settings(&elevated::staging(&source), &obs.join("config"), true)
                .map(|n| format!("{n} files")),
        );
        say("rights set", win::secure(&root).map(|_| "done".into()));
        say("rights read", win::sddl(&root));
        say(
            "task",
            win::register_tasks(user, &root.join(elevated::HELPER)).map(|_| "registered".into()),
        );
        say("task running", Ok(obsproc::task_running().to_string()));
    }
}

/// The copy's tasks as an older reel-elevate made them: one at sign-in that clears OBS's crash
/// marks, then starts the copy's OBS.
#[cfg(windows)]
mod old {
    use reel_desktop_core::bundled::Bundled;
    use reel_desktop_core::obsproc::{TASK, TASK_WINDOW};
    use windows::core::{Interface, BSTR, VARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::TaskScheduler::{
        IExecAction, ILogonTrigger, ITaskService, TaskScheduler, TASK_ACTION_EXEC,
        TASK_CREATE_OR_UPDATE, TASK_INSTANCES_IGNORE_NEW, TASK_LOGON_INTERACTIVE_TOKEN,
        TASK_RUNLEVEL_HIGHEST, TASK_TRIGGER_LOGON,
    };

    pub fn tasks(user: &str, copy: &Bundled) -> Result<(), String> {
        let e = |err: windows::core::Error| err.to_string();
        let root = copy.exe.ancestors().nth(4).ok_or("the copy's folder")?;
        let helper = root.join(reel_desktop_core::elevated::HELPER);
        let mut args = copy.args(true);
        args.push("--only-bundled-plugins".into());
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let svc: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER).map_err(e)?;
            let none = VARIANT::default();
            svc.Connect(&none, &none, &none, &none).map_err(e)?;
            let folder = svc.GetFolder(&BSTR::from("\\")).map_err(e)?;
            let _ = folder.DeleteTask(&BSTR::from(TASK_WINDOW), 0);
            let def = svc.NewTask(0).map_err(e)?;
            let p = def.Principal().map_err(e)?;
            p.SetUserId(&BSTR::from(user)).map_err(e)?;
            p.SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN).map_err(e)?;
            p.SetRunLevel(TASK_RUNLEVEL_HIGHEST).map_err(e)?;
            let s = def.Settings().map_err(e)?;
            s.SetExecutionTimeLimit(&BSTR::from("PT0S")).map_err(e)?;
            s.SetMultipleInstances(TASK_INSTANCES_IGNORE_NEW)
                .map_err(e)?;
            let t: ILogonTrigger = def
                .Triggers()
                .and_then(|t| t.Create(TASK_TRIGGER_LOGON))
                .and_then(|t| t.cast())
                .map_err(e)?;
            t.SetUserId(&BSTR::from(user)).map_err(e)?;
            for (exe, words) in [
                (helper.as_path(), "clear-marks".to_string()),
                (copy.exe.as_path(), args.join(" ")),
            ] {
                let a: IExecAction = def
                    .Actions()
                    .and_then(|a| a.Create(TASK_ACTION_EXEC))
                    .and_then(|a| a.cast())
                    .map_err(e)?;
                a.SetPath(&BSTR::from(exe.display().to_string()))
                    .map_err(e)?;
                a.SetArguments(&BSTR::from(words)).map_err(e)?;
            }
            folder
                .RegisterTaskDefinition(
                    &BSTR::from(TASK),
                    &def,
                    TASK_CREATE_OR_UPDATE.0,
                    &VARIANT::from(BSTR::from(user)),
                    &VARIANT::default(),
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &VARIANT::default(),
                )
                .map_err(e)?;
        }
        Ok(())
    }
}

/// The switch hammered from four threads through the lane and the real step.
#[cfg(windows)]
mod hammer {
    use reel_desktop_core::bundled::Bundled;
    use reel_desktop_core::elevated::{self, Step};
    use reel_desktop_core::lane::{left, on, Answer, Ask, Changes, Job, Lane, Program};
    use reel_desktop_core::obsproc;
    use reel_desktop_core::settings::Settings;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn rng(seed: &mut u64) -> u64 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    }

    fn staged(dir: &Path, rel: &str, out: &mut Vec<(String, String)>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = format!("{rel}{}", e.file_name().to_string_lossy());
            if e.path().is_dir() {
                staged(&e.path(), &format!("{name}/"), out);
            } else if let Ok(text) = std::fs::read_to_string(e.path()) {
                out.push((name, text));
            }
        }
    }

    fn with(elevated: bool) -> Settings {
        let mut s = Settings::default();
        s.obs.bundled = true;
        s.obs.setup.elevated = elevated;
        s
    }

    struct Here {
        source: Bundled,
        files: Vec<(String, String)>,
        pf: PathBuf,
        saved: Mutex<Settings>,
        /// Steps, turns, failures, helpers at once.
        counts: [AtomicU32; 4],
        left: AtomicBool,
    }

    impl Here {
        fn there(&self) -> bool {
            elevated::copy(&self.pf).exe.is_file()
        }
    }

    struct Ours(Arc<Here>);

    impl Program for Ours {
        type Hold = ();

        fn saved(&self) -> Settings {
            self.0.saved.lock().unwrap().clone()
        }

        fn with_obs(&self) -> bool {
            true
        }

        fn copy_there(&self) -> bool {
            self.0.there()
        }

        fn hold(&self) -> Option<()> {
            Some(())
        }

        fn step(
            &self,
            _job: Job,
            step: Step,
            _old: &Settings,
            _new: &Settings,
            answered: &dyn Fn(),
        ) -> Result<(), String> {
            self.0.counts[0].fetch_add(1, Ordering::SeqCst);
            let done = obsproc::elevated_step(step, &self.0.source, &self.0.files, answered);
            if let Err(e) = &done {
                self.0.counts[2].fetch_add(1, Ordering::SeqCst);
                println!("{} failed: {e}", step.word());
            }
            done
        }

        fn save(&self, _old: &Settings, new: Settings) -> Answer {
            self.0.counts[1].fetch_add(1, Ordering::SeqCst);
            *self.0.saved.lock().unwrap() = new.clone();
            Ok(new)
        }

        fn said(&self, _problem: Option<String>) {}

        fn settle(&self) {
            let on = on(&self.0.saved.lock().unwrap());
            let task = obsproc::task_exists();
            self.0
                .left
                .store(left(on, self.0.there(), task), Ordering::SeqCst);
        }

        fn changed(&self) {}
    }

    pub fn run(install: &Path) {
        let source = Bundled::windows(install);
        let mut files = Vec::new();
        staged(&elevated::staging(&source), "", &mut files);
        let pf = obsproc::program_files().expect("Program Files");
        let start = elevated::copy(&pf).exe.is_file();
        let here = Arc::new(Here {
            source,
            files,
            pf,
            saved: Mutex::new(with(start)),
            counts: Default::default(),
            left: AtomicBool::new(false),
        });
        let lane = Arc::new(Lane::new(Duration::from_millis(200)));
        lane.start(Ours(here.clone()));
        let stop = Arc::new(AtomicBool::new(false));
        let watch = {
            let (stop, here) = (stop.clone(), here.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    let n = obsproc::processes()
                        .iter()
                        .filter(|p| p.name.eq_ignore_ascii_case("reel-elevate.exe"))
                        .count() as u32;
                    here.counts[3].fetch_max(n, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(10));
                }
            })
        };
        std::thread::scope(|s| {
            for t in 0..4u64 {
                let lane = &lane;
                s.spawn(move || {
                    let mut seed = t * 104_729 + 7;
                    for _ in 0..5 {
                        std::thread::sleep(Duration::from_millis(rng(&mut seed) % 400));
                        let wanted = rng(&mut seed) & 1 == 0;
                        let _ =
                            lane.ask(Ask::Save(Changes::between(&with(!wanted), &with(wanted))));
                    }
                });
            }
        });
        while !lane.idle() {
            std::thread::sleep(Duration::from_millis(50));
        }
        stop.store(true, Ordering::SeqCst);
        watch.join().unwrap();
        let saved = on(&here.saved.lock().unwrap());
        let c = |i: usize| here.counts[i].load(Ordering::SeqCst);
        println!(
            "requests 20 turns {} steps {} failed {} helpers-at-once {} saved {} copy {} left {}",
            c(1),
            c(0),
            c(2),
            c(3),
            if saved { "on" } else { "off" },
            if here.there() { "there" } else { "none" },
            if here.left.load(Ordering::SeqCst) {
                "said"
            } else {
                "no"
            },
        );
    }
}
