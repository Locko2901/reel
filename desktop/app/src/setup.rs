//! Which encoders the bundled OBS can use on this PC, by a short test encode with the bundled
//! ffmpeg. H.264 throughout, so reel keeps the clips untouched.
use reel_desktop_core::adapters;
use reel_media::HwBackend;
use serde::Serialize;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

/// A test encode takes under a second; a driver's first start (a laptop's sleeping graphics
/// card waking, a first CUDA context) can take a few more. A driver that hangs costs this.
pub const LIMIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Waiting,
    Testing,
    Works,
    NotHere,
    NoAnswer,
}

#[derive(Debug, Clone, Serialize)]
pub struct EncoderTest {
    /// OBS's encoder id.
    pub id: String,
    pub word: String,
    pub state: State,
    pub ok: bool,
    /// What the test found, in words.
    pub line: String,
    /// ffmpeg's own lines, behind the row's details.
    pub details: String,
    pub ms: Option<u64>,
}

/// The test so far, sent to the page each time a row changes.
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub run: u64,
    pub rows: Vec<EncoderTest>,
    pub done: usize,
    pub total: usize,
    pub finished: bool,
}

/// An encoder the setup tests: the ffmpeg backend of its test, OBS's id, its name, the test's
/// device, and the PCI vendor of the graphics it needs where one vendor makes them.
#[derive(Debug, Clone, Copy)]
pub struct Gpu {
    pub backend: HwBackend,
    pub id: &'static str,
    pub word: &'static str,
    pub device: &'static str,
    pub vendor: Option<u32>,
}

#[cfg(windows)]
const GPU: &[Gpu] = &[
    Gpu {
        backend: HwBackend::Nvenc,
        id: "obs_nvenc_h264_tex",
        word: "NVIDIA NVENC H.264",
        device: "",
        vendor: Some(adapters::NVIDIA),
    },
    Gpu {
        backend: HwBackend::Amf,
        id: "h264_texture_amf",
        word: "AMD AMF H.264",
        device: "",
        vendor: Some(adapters::AMD),
    },
    Gpu {
        backend: HwBackend::Qsv,
        id: "obs_qsv11_v2",
        word: "Intel Quick Sync H.264",
        device: "",
        vendor: Some(adapters::INTEL),
    },
];
#[cfg(not(windows))]
const GPU: &[Gpu] = &[
    Gpu {
        backend: HwBackend::Nvenc,
        id: "obs_nvenc_h264_tex",
        word: "NVIDIA NVENC H.264",
        device: "",
        vendor: Some(adapters::NVIDIA),
    },
    Gpu {
        backend: HwBackend::Vaapi,
        id: "ffmpeg_vaapi_tex",
        word: "VAAPI H.264 (AMD, Intel)",
        device: "/dev/dri/renderD128",
        vendor: None,
    },
];

/// How one encoder's test ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// This ffmpeg was built without the encoder.
    NotInFfmpeg,
    /// Windows lists no graphics adapter of the encoder's vendor; nothing was run.
    NoAdapter,
    Passed,
    /// ffmpeg's error lines, all of them.
    Failed(String),
    NoAnswer(Duration),
    NotStarted(String),
}

/// What a PC without the encoder's graphics shows, by backend.
pub fn absent(backend: HwBackend) -> &'static str {
    match backend {
        HwBackend::Nvenc => "no NVIDIA graphics card here",
        HwBackend::Amf => "no AMD graphics here",
        HwBackend::Qsv => "Quick Sync is not available on this processor",
        HwBackend::Vaapi => "no graphics with a VAAPI driver here",
        HwBackend::Software => "",
    }
}

/// The card is there but its driver is older than this ffmpeg's NVENC.
const TOO_OLD: &[&str] = &[
    "required nvenc api version",
    "minimum required nvidia driver",
];
/// The card answered and refused something: its own words say more than ours.
const REFUSED: &[&str] = &[
    "invalid param",
    "unsupported param",
    "out of memory",
    "incompatible client key",
    "initializeencoder failed",
];
/// The encoder cannot be opened, there is no device, or the driver is missing.
const NOT_HERE: &[&str] = &[
    "cannot load",
    "amfrt64",
    "no capable devices found",
    "no nvenc capable devices",
    "cuinit",
    "cuda_error_no_device",
    "no device available",
    "mfx session",
    "device creation failed",
    "failed to create direct3d device",
    "vaapi connection",
    "no va display",
    "could not open encoder",
    "error while opening encoder",
];

/// ffmpeg's lines of a failed test in plain words, where they are one of the common failures;
/// None keeps ffmpeg's own.
pub fn plain(backend: HwBackend, stderr: &str) -> Option<String> {
    let s = stderr.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| s.contains(w));
    if backend == HwBackend::Nvenc && has(TOO_OLD) {
        return Some(
            "the NVIDIA driver is older than this ffmpeg needs: an update of the driver fixes it"
                .into(),
        );
    }
    if has(REFUSED) || !has(NOT_HERE) {
        return None;
    }
    Some(absent(backend).into())
}

/// The state rule: a row's state, its words and its details from how its test ended.
pub fn judged(backend: HwBackend, outcome: &Outcome) -> (State, String, String) {
    match outcome {
        Outcome::Passed => (State::Works, "a test encode worked".into(), String::new()),
        Outcome::NotInFfmpeg => (
            State::NotHere,
            format!("this ffmpeg has no {}", backend.encoder()),
            String::new(),
        ),
        Outcome::NoAdapter => (
            State::NotHere,
            absent(backend).into(),
            "Windows lists no graphics adapter of this maker, so the test encode was not run"
                .into(),
        ),
        Outcome::NoAnswer(limit) => (
            State::NoAnswer,
            format!("did not answer in {} s", limit.as_secs()),
            String::new(),
        ),
        Outcome::NotStarted(e) => (
            State::NotHere,
            format!("ffmpeg did not start: {e}"),
            String::new(),
        ),
        Outcome::Failed(stderr) => {
            let details = stderr.trim().to_string();
            let line = plain(backend, stderr).unwrap_or_else(|| {
                let first = details.lines().find(|l| !l.trim().is_empty());
                format!(
                    "the test encode failed: {}",
                    first.unwrap_or("no reason given")
                )
            });
            (State::NotHere, line, details)
        }
    }
}

/// Whether a vendor's test is skipped: Windows listed hardware adapters, none of this vendor.
/// An empty list (a remote session may show none) skips nothing.
pub fn skipped(vendor: Option<u32>, listed: Option<&[u32]>) -> bool {
    match (vendor, listed) {
        (Some(v), Some(l)) => !l.is_empty() && !l.contains(&v),
        _ => false,
    }
}

/// One test encode within `limit`; ffmpeg is stopped when it runs out.
pub async fn encode(ffmpeg: &str, args: &[String], limit: Duration) -> Outcome {
    let mut cmd = tokio::process::Command::from(reel_core::proc::command(ffmpeg));
    cmd.args([
        "-hide_banner",
        "-nostdin",
        "-nostats",
        "-loglevel",
        "error",
        "-y",
    ])
    .args(args)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Outcome::NotStarted(e.to_string()),
    };
    let mut stderr = child.stderr.take().expect("piped");
    let ended = async {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes).await;
        (child.wait().await, bytes)
    };
    match tokio::time::timeout(limit, ended).await {
        Err(_) => Outcome::NoAnswer(limit),
        Ok((Ok(status), _)) if status.success() => Outcome::Passed,
        Ok((Ok(_), bytes)) => Outcome::Failed(String::from_utf8_lossy(&bytes).into_owned()),
        Ok((Err(e), _)) => Outcome::NotStarted(e.to_string()),
    }
}

fn row(gpu: &Gpu) -> EncoderTest {
    EncoderTest {
        id: gpu.id.into(),
        word: gpu.word.into(),
        state: State::Waiting,
        ok: false,
        line: String::new(),
        details: String::new(),
        ms: None,
    }
}

fn x264() -> EncoderTest {
    EncoderTest {
        id: "obs_x264".into(),
        word: "x264 on the CPU".into(),
        state: State::Works,
        ok: true,
        line: "always there; the GPU's encoders leave the game more room".into(),
        details: String::new(),
        ms: None,
    }
}

fn ms(d: Duration) -> u64 {
    d.as_millis().try_into().unwrap_or(u64::MAX)
}

/// The test's settings: what runs, with which ffmpeg, within what time.
pub struct Plan<'a> {
    pub ffmpeg: &'a str,
    pub ffprobe: &'a str,
    pub gpus: &'a [Gpu],
    pub adapters: Option<&'a [u32]>,
    pub limit: Duration,
    pub run: u64,
}

/// Tests the encoders one after another, in the order the setup prefers them; x264 on the CPU
/// always works and comes last. `sent` has the rows each time one changes.
pub async fn run_tests(plan: Plan<'_>, mut sent: impl FnMut(&Progress)) -> Vec<EncoderTest> {
    let started = Instant::now();
    let mut rows: Vec<EncoderTest> = plan.gpus.iter().map(row).chain([x264()]).collect();
    let total = plan.gpus.len();
    let mut tell = |rows: &[EncoderTest], done: usize, finished: bool| {
        sent(&Progress {
            run: plan.run,
            rows: rows.to_vec(),
            done,
            total,
            finished,
        })
    };
    tell(&rows, 0, false);
    let ff = reel_media::Ffmpeg::new(plan.ffmpeg, plan.ffprobe, 0, 1);
    let t = Instant::now();
    let have = tokio::time::timeout(plan.limit, ff.encoders())
        .await
        .unwrap_or_default();
    tracing::info!(
        n = have.len(),
        ms = ms(t.elapsed()),
        "setup: the encoders of this ffmpeg listed"
    );
    for (i, gpu) in plan.gpus.iter().enumerate() {
        let enc = gpu.backend.encoder();
        let t = Instant::now();
        let outcome = if skipped(gpu.vendor, plan.adapters) {
            Outcome::NoAdapter
        } else if !have.iter().any(|e| e == enc) {
            Outcome::NotInFfmpeg
        } else {
            rows[i].state = State::Testing;
            tell(&rows, i, false);
            let args = reel_media::args::hw_self_test(gpu.backend, gpu.device, cfg!(windows));
            encode(plan.ffmpeg, &args, plan.limit).await
        };
        let took = ms(t.elapsed());
        let (state, line, details) = judged(gpu.backend, &outcome);
        tracing::info!(
            encoder = gpu.id,
            state = ?state,
            ms = took,
            line = %line,
            ffmpeg = %details.lines().collect::<Vec<_>>().join(" | "),
            "setup: encoder tested"
        );
        rows[i] = EncoderTest {
            state,
            ok: state == State::Works,
            line,
            details,
            ms: Some(took),
            ..row(gpu)
        };
        tell(&rows, i + 1, false);
    }
    tracing::info!(
        run = plan.run,
        ms = ms(started.elapsed()),
        "setup: encoder test done"
    );
    tell(&rows, total, true);
    rows
}

/// The last run, stopped when another starts (the setup page opened again).
static CURRENT: Mutex<Option<CancellationToken>> = Mutex::new(None);

/// The setup's encoder test with the bundled ffmpeg. `asked` is when the page asked, in ms
/// since 1970, so the log shows how long the request took to reach the app.
pub async fn test_encoders(
    run: u64,
    asked: Option<u64>,
    sent: impl FnMut(&Progress),
) -> Vec<EncoderTest> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(ms)
        .unwrap_or_default();
    tracing::info!(
        run,
        reached_app_ms = asked.map(|a| now.saturating_sub(a)),
        "setup: encoder test asked"
    );
    let cancel = CancellationToken::new();
    if let Some(before) = CURRENT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(cancel.clone())
    {
        before.cancel();
    }
    let t = Instant::now();
    let vendors = adapters::vendors();
    if let Some(a) = &vendors {
        let listed: Vec<String> = a.iter().map(|v| format!("{v:#06x}")).collect();
        tracing::info!(vendors = ?listed, ms = ms(t.elapsed()), "setup: graphics adapters listed");
    }
    let ffmpeg = crate::standalone::sidecar("ffmpeg").display().to_string();
    let ffprobe = crate::standalone::sidecar("ffprobe").display().to_string();
    let plan = Plan {
        ffmpeg: &ffmpeg,
        ffprobe: &ffprobe,
        gpus: GPU,
        adapters: vendors.as_deref(),
        limit: LIMIT,
        run,
    };
    tokio::select! {
        rows = run_tests(plan, sent) => rows,
        _ = cancel.cancelled() => {
            tracing::info!(run, "setup: encoder test stopped, another started");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reel_desktop_core::adapters::{AMD, INTEL, NVIDIA};

    const NVENC_WINDOWS: &str = "[h264_nvenc @ 0000021632c52d80] Cannot load nvcuda.dll
[vost#0:0/h264_nvenc @ 0000021632c52d80] Error while opening encoder - maybe incorrect parameters such as bit_rate, rate, width or height.
[vf#0:0 @ 0000021632c4e8c0] Error sending frames to consumers: Invalid argument
[vf#0:0 @ 0000021632c4e8c0] Task finished with error code: -22 (Invalid argument)
[vost#0:0/h264_nvenc @ 0000021632c52d80] [enc:h264_nvenc @ 0000021632c6e140] Could not open encoder before EOF
[vost#0:0/h264_nvenc @ 0000021632c52d80] Task finished with error code: -22 (Invalid argument)
";

    #[test]
    fn the_common_failures_in_plain_words() {
        let cases: &[(HwBackend, &str, &str)] = &[
            (HwBackend::Nvenc, NVENC_WINDOWS, "no NVIDIA graphics card here"),
            (
                HwBackend::Nvenc,
                "[h264_nvenc @ 0x55] Cannot load libcuda.so.1\n",
                "no NVIDIA graphics card here",
            ),
            (
                HwBackend::Nvenc,
                "[h264_nvenc @ 0x55] dl_fn->cuda_dl->cuInit(0) failed -> CUDA_ERROR_NO_DEVICE: no CUDA-capable device is detected\n",
                "no NVIDIA graphics card here",
            ),
            (
                HwBackend::Nvenc,
                "[vost#0:0/h264_nvenc @ 0000021632c52d80] [enc:h264_nvenc @ 0000021632c6e140] Could not open encoder before EOF | [vost#0:0/h264_nvenc @ 0000021632c52d80] Task finished with error code: -22 (Invalid argument)",
                "no NVIDIA graphics card here",
            ),
            (
                HwBackend::Amf,
                "[h264_amf @ 000001] DLL amfrt64.dll failed to open\n[vost#0:0/h264_amf @ 000001] Error while opening encoder\n",
                "no AMD graphics here",
            ),
            (
                HwBackend::Qsv,
                "[h264_qsv @ 000001] Error initializing an internal MFX session: unsupported (-3)\n",
                "Quick Sync is not available on this processor",
            ),
            (
                HwBackend::Qsv,
                "[AVHWDeviceContext @ 000001] Failed to create Direct3D device\nDevice creation failed: -1313558101.\n",
                "Quick Sync is not available on this processor",
            ),
            (
                HwBackend::Vaapi,
                "[AVHWDeviceContext @ 0x55] Failed to initialise VAAPI connection: -1 (unknown libva error).\nDevice creation failed: -5.\n",
                "no graphics with a VAAPI driver here",
            ),
            (
                HwBackend::Nvenc,
                "[h264_nvenc @ 0x55] Driver does not support the required nvenc API version. Required: 13.0 Found: 12.1\n[h264_nvenc @ 0x55] The minimum required Nvidia driver for nvenc is 570.0 or newer\n",
                "the NVIDIA driver is older than this ffmpeg needs: an update of the driver fixes it",
            ),
        ];
        for (backend, stderr, want) in cases {
            assert_eq!(plain(*backend, stderr).as_deref(), Some(*want), "{stderr}");
        }
    }

    #[test]
    fn a_failure_it_cannot_place_keeps_ffmpegs_words() {
        let refused = "[h264_nvenc @ 0x55] InitializeEncoder failed: invalid param (8): Invalid Level.\n[vost#0:0/h264_nvenc @ 0x55] Could not open encoder before EOF\n";
        assert_eq!(plain(HwBackend::Nvenc, refused), None);
        let (state, line, details) = judged(HwBackend::Nvenc, &Outcome::Failed(refused.into()));
        assert_eq!(state, State::NotHere);
        assert_eq!(
            line,
            "the test encode failed: [h264_nvenc @ 0x55] InitializeEncoder failed: invalid param (8): Invalid Level."
        );
        assert_eq!(details, refused.trim());
        let odd = "[out#0/null @ 0x55] something new\n";
        assert_eq!(plain(HwBackend::Qsv, odd), None);
        assert_eq!(
            judged(HwBackend::Qsv, &Outcome::Failed(String::new())).1,
            "the test encode failed: no reason given"
        );
    }

    #[test]
    fn the_state_rule() {
        let b = HwBackend::Nvenc;
        assert_eq!(judged(b, &Outcome::Passed).0, State::Works);
        let (state, line, details) = judged(b, &Outcome::Failed(NVENC_WINDOWS.into()));
        assert_eq!(
            (state, line.as_str()),
            (State::NotHere, "no NVIDIA graphics card here")
        );
        assert!(details.starts_with("[h264_nvenc @ 0000021632c52d80] Cannot load nvcuda.dll"));
        assert_eq!(
            judged(b, &Outcome::NoAnswer(Duration::from_secs(10))),
            (
                State::NoAnswer,
                "did not answer in 10 s".to_string(),
                String::new()
            )
        );
        let (state, line, _) = judged(HwBackend::Amf, &Outcome::NoAdapter);
        assert_eq!(
            (state, line.as_str()),
            (State::NotHere, "no AMD graphics here")
        );
        assert_eq!(
            judged(b, &Outcome::NotInFfmpeg).1,
            "this ffmpeg has no h264_nvenc"
        );
        assert_eq!(
            judged(b, &Outcome::NotStarted("gone".into())).0,
            State::NotHere
        );
    }

    #[test]
    fn a_vendor_windows_does_not_list_is_skipped() {
        let intel_only: &[u32] = &[INTEL];
        assert!(skipped(Some(NVIDIA), Some(intel_only)));
        assert!(skipped(Some(AMD), Some(intel_only)));
        assert!(!skipped(Some(INTEL), Some(intel_only)));
        assert!(
            !skipped(Some(NVIDIA), Some(&[])),
            "an empty list skips nothing"
        );
        assert!(!skipped(Some(NVIDIA), None), "no list skips nothing");
        assert!(!skipped(None, Some(intel_only)), "VAAPI is any vendor's");
    }

    #[cfg(unix)]
    fn standin(dir: &std::path::Path, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let exe = dir.join("ffmpeg");
        std::fs::write(&exe, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        exe.display().to_string()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_test_that_hangs_ends_at_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        // the listing has both encoders; NVENC's test fails at once, VAAPI's hangs
        let ffmpeg = standin(
            dir.path(),
            r#"case "$*" in
  *-encoders*) printf ' V....D h264_nvenc NVIDIA\n V....D h264_vaapi VAAPI\n'; exit 0 ;;
  *h264_nvenc*) echo '[h264_nvenc @ 0x1] Cannot load libcuda.so.1' >&2; exit 1 ;;
  *) exec sleep 30 ;;
esac"#,
        );
        let gpus = [
            Gpu {
                backend: HwBackend::Nvenc,
                id: "obs_nvenc_h264_tex",
                word: "NVIDIA NVENC H.264",
                device: "",
                vendor: Some(adapters::NVIDIA),
            },
            Gpu {
                backend: HwBackend::Vaapi,
                id: "ffmpeg_vaapi_tex",
                word: "VAAPI H.264 (AMD, Intel)",
                device: "/dev/dri/renderD128",
                vendor: None,
            },
        ];
        let mut seen: Vec<Progress> = Vec::new();
        let t = Instant::now();
        let rows = run_tests(
            Plan {
                ffmpeg: &ffmpeg,
                ffprobe: "ffprobe",
                gpus: &gpus,
                adapters: None,
                limit: Duration::from_secs(2),
                run: 7,
            },
            |p| seen.push(p.clone()),
        )
        .await;
        let took = t.elapsed();
        assert!(took < Duration::from_secs(6), "took {took:?}");
        let states: Vec<State> = rows.iter().map(|r| r.state).collect();
        assert_eq!(states, [State::NotHere, State::NoAnswer, State::Works]);
        assert_eq!(rows[0].line, "no NVIDIA graphics card here");
        assert_eq!(rows[1].line, "did not answer in 2 s");
        assert!(rows[1].ms.unwrap() >= 2000);
        // the page saw every encoder from the start, each test begin, and each end
        let by_step: Vec<(Vec<State>, usize, bool)> = seen
            .iter()
            .map(|p| (p.rows.iter().map(|r| r.state).collect(), p.done, p.finished))
            .collect();
        use State::*;
        assert_eq!(
            by_step,
            [
                (vec![Waiting, Waiting, Works], 0, false),
                (vec![Testing, Waiting, Works], 0, false),
                (vec![NotHere, Waiting, Works], 1, false),
                (vec![NotHere, Testing, Works], 1, false),
                (vec![NotHere, NoAnswer, Works], 2, false),
                (vec![NotHere, NoAnswer, Works], 2, true),
            ]
        );
        assert!(seen.iter().all(|p| p.run == 7 && p.total == 2));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_vendor_not_listed_is_not_run() {
        let dir = tempfile::tempdir().unwrap();
        let ran = dir.path().join("ran");
        let ffmpeg = standin(
            dir.path(),
            &format!(
                r#"case "$*" in
  *-encoders*) printf ' V....D h264_nvenc NVIDIA\n'; exit 0 ;;
  *) touch '{}'; exit 0 ;;
esac"#,
                ran.display()
            ),
        );
        let gpus = [Gpu {
            backend: HwBackend::Nvenc,
            id: "obs_nvenc_h264_tex",
            word: "NVIDIA NVENC H.264",
            device: "",
            vendor: Some(adapters::NVIDIA),
        }];
        let rows = run_tests(
            Plan {
                ffmpeg: &ffmpeg,
                ffprobe: "ffprobe",
                gpus: &gpus,
                adapters: Some(&[INTEL]),
                limit: Duration::from_secs(2),
                run: 1,
            },
            |_| {},
        )
        .await;
        assert_eq!(rows[0].state, State::NotHere);
        assert_eq!(rows[0].line, "no NVIDIA graphics card here");
        assert!(!ran.exists(), "ffmpeg was asked to encode");
    }
}
