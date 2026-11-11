use crate::level::{parse_ebur128, Level};
use reel_core::ProbeInfo;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// Told of each ffmpeg a task starts and of its end, for a caller that pauses some of them while
/// more pressing work runs; the time paused is not counted towards its timeout.
pub trait Pauser: Send + Sync {
    fn started(&self, pid: u32);
    fn ended(&self, pid: u32);
    fn paused(&self, pid: u32) -> Duration;
}

pub type OnSpawn = std::sync::Arc<dyn Pauser>;

tokio::task_local! {
    pub static ON_SPAWN: OnSpawn;
}

/// Tells the end of an ffmpeg that was told as started, however its run ends.
struct Told(Option<(OnSpawn, u32)>);

impl Told {
    fn paused(&self) -> Duration {
        self.0
            .as_ref()
            .map_or(Duration::ZERO, |(f, pid)| f.paused(*pid))
    }
}

impl Drop for Told {
    fn drop(&mut self) {
        if let Some((f, pid)) = &self.0 {
            f.ended(*pid);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("{tool} could not start: {source}")]
    Spawn {
        tool: String,
        source: std::io::Error,
    },
    #[error("{tool} failed ({code}): {stderr}")]
    Failed {
        tool: String,
        code: i32,
        stderr: String,
    },
    #[error("{tool} timed out after {minutes} minutes")]
    Timeout { tool: String, minutes: u64 },
    #[error("cancelled")]
    Cancelled,
    #[error("{0}")]
    Probe(#[from] reel_core::probe::ProbeError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Progress = Box<dyn FnMut(f64) + Send>;

#[derive(Debug, Clone)]
pub struct Ffmpeg {
    pub ffmpeg: String,
    pub ffprobe: String,
    pub nice: i32,
    pub timeout: Duration,
}

fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join(" | ")
}

impl Ffmpeg {
    /// A tool process, without a console window on Windows (reel_core::proc).
    fn tool(&self, program: &str) -> Command {
        Command::from(reel_core::proc::command(program))
    }

    /// ffmpeg for a job: under `nice` on Unix when the setting asks; on Windows in the idle
    /// priority class, so a game in front keeps the CPU and the GPU.
    fn ffmpeg_command(&self) -> Command {
        #[cfg(unix)]
        {
            if self.nice > 0 {
                let mut c = Command::from(reel_core::proc::command("nice"));
                c.arg("-n").arg(self.nice.to_string()).arg(&self.ffmpeg);
                return c;
            }
            self.tool(&self.ffmpeg)
        }
        #[cfg(windows)]
        {
            let idle = if self.nice > 0 {
                reel_core::proc::IDLE_PRIORITY_CLASS
            } else {
                0
            };
            Command::from(reel_core::proc::command_with(&self.ffmpeg, idle))
        }
    }

    pub fn new(ffmpeg: &str, ffprobe: &str, nice: i32, timeout_minutes: u64) -> Self {
        Self {
            ffmpeg: ffmpeg.into(),
            ffprobe: ffprobe.into(),
            nice,
            timeout: Duration::from_secs(timeout_minutes.max(1) * 60),
        }
    }

    pub async fn version(&self) -> Option<String> {
        let out = self
            .tool(&self.ffmpeg)
            .arg("-version")
            .output()
            .await
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        text.lines().next().map(|l| l.trim().to_string())
    }

    pub async fn encoders(&self) -> Vec<String> {
        let Ok(out) = self
            .tool(&self.ffmpeg)
            .args(["-hide_banner", "-encoders"])
            .output()
            .await
        else {
            return vec![];
        };
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.split_whitespace().nth(1).map(|s| s.to_string()))
            .collect()
    }

    pub async fn probe(&self, path: &Path) -> Result<ProbeInfo, MediaError> {
        let out = self
            .tool(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_streams",
                "-show_format",
            ])
            .arg(path)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| MediaError::Spawn {
                tool: "ffprobe".into(),
                source: e,
            })?;
        if !out.status.success() {
            return Err(MediaError::Failed {
                tool: "ffprobe".into(),
                code: out.status.code().unwrap_or(-1),
                stderr: tail(&String::from_utf8_lossy(&out.stderr), 3),
            });
        }
        Ok(ProbeInfo::from_ffprobe_json(&String::from_utf8_lossy(
            &out.stdout,
        ))?)
    }

    /// The names of every tag a file carries, its container's and its streams'.
    pub async fn tag_names(&self, path: &Path) -> Result<Vec<String>, MediaError> {
        let out = self
            .tool(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format_tags:stream_tags",
                "-of",
                "json",
            ])
            .arg(path)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| MediaError::Spawn {
                tool: "ffprobe".into(),
                source: e,
            })?;
        if !out.status.success() {
            return Err(MediaError::Failed {
                tool: "ffprobe".into(),
                code: out.status.code().unwrap_or(-1),
                stderr: tail(&String::from_utf8_lossy(&out.stderr), 3),
            });
        }
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
        let mut names = Vec::new();
        let tags = std::iter::once(&v["format"]["tags"]).chain(
            v["streams"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|s| &s["tags"]),
        );
        for t in tags {
            if let Some(map) = t.as_object() {
                names.extend(map.keys().cloned());
            }
        }
        Ok(names)
    }

    pub async fn average_color(&self, image: &Path) -> Result<[u8; 3], MediaError> {
        let out = self
            .tool(&self.ffmpeg)
            .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"])
            .arg(image)
            .args([
                "-vf",
                "scale=1:1:flags=area",
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-",
            ])
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| MediaError::Spawn {
                tool: "ffmpeg".into(),
                source: e,
            })?;
        if !out.status.success() || out.stdout.len() < 3 {
            return Err(MediaError::Failed {
                tool: "ffmpeg".into(),
                code: out.status.code().unwrap_or(-1),
                stderr: tail(&String::from_utf8_lossy(&out.stderr), 3),
            });
        }
        Ok([out.stdout[0], out.stdout[1], out.stdout[2]])
    }

    /// The MD5 of one stream's packets, as copied: equal for the source and a remux of it.
    pub async fn stream_md5(&self, path: &Path, stream: &str) -> Result<String, MediaError> {
        let out = self
            .tool(&self.ffmpeg)
            .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i"])
            .arg(path)
            .args(["-map", stream, "-c", "copy", "-f", "md5", "-"])
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| MediaError::Spawn {
                tool: "ffmpeg".into(),
                source: e,
            })?;
        if !out.status.success() {
            return Err(MediaError::Failed {
                tool: "ffmpeg".into(),
                code: out.status.code().unwrap_or(-1),
                stderr: tail(&String::from_utf8_lossy(&out.stderr), 3),
            });
        }
        let text = String::from_utf8_lossy(&out.stdout);
        Ok(text.trim().trim_start_matches("MD5=").to_string())
    }

    pub async fn keyframes(&self, path: &Path) -> Result<Vec<f64>, MediaError> {
        let out = self
            .tool(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "packet=pts_time,dts_time,flags",
                "-of",
                "csv=p=0",
            ])
            .arg(path)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| MediaError::Spawn {
                tool: "ffprobe".into(),
                source: e,
            })?;
        if !out.status.success() {
            return Err(MediaError::Failed {
                tool: "ffprobe".into(),
                code: out.status.code().unwrap_or(-1),
                stderr: tail(&String::from_utf8_lossy(&out.stderr), 3),
            });
        }
        let mut kf: Vec<f64> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let parts: Vec<&str> = l.split(',').collect();
                if parts.len() < 3 || !parts[2].contains('K') {
                    return None;
                }
                parts[0]
                    .parse::<f64>()
                    .ok()
                    .or_else(|| parts[1].parse::<f64>().ok())
            })
            .collect();
        kf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        kf.dedup();
        Ok(kf)
    }

    /// Unlike `probe`, also for a file without video.
    pub async fn audio_streams(&self, path: &Path) -> Result<usize, MediaError> {
        let out = self
            .tool(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "a",
                "-show_entries",
                "stream=index",
                "-of",
                "csv=p=0",
            ])
            .arg(path)
            .stdin(Stdio::null())
            .output()
            .await
            .map_err(|e| MediaError::Spawn {
                tool: "ffprobe".into(),
                source: e,
            })?;
        if !out.status.success() {
            return Err(MediaError::Failed {
                tool: "ffprobe".into(),
                code: out.status.code().unwrap_or(-1),
                stderr: tail(&String::from_utf8_lossy(&out.stderr), 3),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count())
    }

    /// A measurement pass, for its log.
    pub async fn analyse(&self, args: &[String]) -> Result<String, MediaError> {
        let mut cmd = self.ffmpeg_command();
        cmd.args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "info"])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let out = tokio::time::timeout(self.timeout, cmd.output())
            .await
            .map_err(|_| MediaError::Timeout {
                tool: "ffmpeg".into(),
                minutes: self.timeout.as_secs() / 60,
            })?
            .map_err(|e| MediaError::Spawn {
                tool: "ffmpeg".into(),
                source: e,
            })?;
        let log = String::from_utf8_lossy(&out.stderr).into_owned();
        if !out.status.success() {
            return Err(MediaError::Failed {
                tool: "ffmpeg".into(),
                code: out.status.code().unwrap_or(-1),
                stderr: tail(&log, 4),
            });
        }
        Ok(log)
    }

    /// Every track measured, the loudest counted.
    pub async fn audio_level(&self, path: &Path) -> Result<Level, MediaError> {
        let tracks = self.audio_streams(path).await?;
        let mut levels = Vec::with_capacity(tracks);
        for track in 0..tracks {
            let log = self.analyse(&crate::args::level(path, track)).await?;
            levels.push(parse_ebur128(&log).ok_or_else(|| MediaError::Failed {
                tool: "ffmpeg".into(),
                code: 0,
                stderr: format!("no loudness summary for audio track {track}"),
            })?);
        }
        Ok(Level::loudest(&levels))
    }

    pub async fn run(
        &self,
        args: &[String],
        duration: f64,
        progress: Option<Progress>,
    ) -> Result<(), MediaError> {
        self.run_with(args, duration, progress, None).await
    }

    /// Runs ffmpeg; a cancelled token kills it and the result is `Cancelled`.
    pub async fn run_with(
        &self,
        args: &[String],
        duration: f64,
        mut progress: Option<Progress>,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), MediaError> {
        if cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
            return Err(MediaError::Cancelled);
        }
        let mut cmd = self.ffmpeg_command();
        cmd.args([
            "-hide_banner",
            "-nostdin",
            "-nostats",
            "-loglevel",
            "error",
            "-progress",
            "pipe:1",
            "-y",
        ])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
        tracing::debug!(args = ?args, "ffmpeg");
        let mut child = cmd.spawn().map_err(|e| MediaError::Spawn {
            tool: "ffmpeg".into(),
            source: e,
        })?;
        let started = tokio::time::Instant::now();
        let told = Told(
            ON_SPAWN
                .try_with(|f| f.clone())
                .ok()
                .zip(child.id())
                .map(|(f, pid)| {
                    f.started(pid);
                    (f, pid)
                }),
        );
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let reader = async {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(v) = line
                    .strip_prefix("out_time_us=")
                    .or_else(|| line.strip_prefix("out_time_ms="))
                {
                    if let (Ok(us), Some(p)) = (v.trim().parse::<f64>(), progress.as_mut()) {
                        if duration > 0.0 {
                            p((us / 1_000_000.0 / duration).clamp(0.0, 0.999));
                        }
                    }
                }
            }
        };
        let err_reader = async {
            let mut buf = String::new();
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                buf.push_str(&line);
                buf.push('\n');
            }
            buf
        };
        let waited = async {
            let (_, stderr_text) = tokio::join!(reader, err_reader);
            let status = child.wait().await?;
            Ok::<_, std::io::Error>((status, stderr_text))
        };
        let never = CancellationToken::new();
        let cancel = cancel.unwrap_or(&never);
        let limit = limit_for(self.timeout, duration);
        let outcome = {
            let deadline = tokio::time::sleep(limit);
            tokio::pin!(deadline, waited);
            loop {
                tokio::select! {
                    r = &mut waited => break Some(Ok(r)),
                    _ = cancel.cancelled() => break None,
                    _ = &mut deadline => {
                        let at = started + limit + told.paused();
                        if tokio::time::Instant::now() >= at {
                            break Some(Err(()));
                        }
                        deadline.as_mut().reset(at);
                    }
                }
            }
        };
        let Some(outcome) = outcome else {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err(MediaError::Cancelled);
        };
        match outcome {
            Ok(Ok((status, stderr_text))) => {
                if status.success() {
                    Ok(())
                } else {
                    Err(MediaError::Failed {
                        tool: "ffmpeg".into(),
                        code: status.code().unwrap_or(-1),
                        stderr: tail(&stderr_text, 4),
                    })
                }
            }
            Ok(Err(e)) => Err(MediaError::Io(e)),
            Err(_) => Err(MediaError::Timeout {
                tool: "ffmpeg".into(),
                minutes: limit.as_secs() / 60,
            }),
        }
    }

    pub async fn run_to(
        &self,
        args: &[String],
        output: &Path,
        duration: f64,
        progress: Option<Progress>,
    ) -> Result<(), MediaError> {
        self.run_to_with(args, output, duration, progress, None)
            .await
    }

    pub async fn run_to_with(
        &self,
        args: &[String],
        output: &Path,
        duration: f64,
        progress: Option<Progress>,
        cancel: Option<&CancellationToken>,
    ) -> Result<(), MediaError> {
        let part = part_path(output);
        let mut full: Vec<String> = args.to_vec();
        full.push(part.to_string_lossy().into_owned());
        if let Err(e) = self.run_with(&full, duration, progress, cancel).await {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(e);
        }
        tokio::fs::rename(&part, output).await?;
        Ok(())
    }
}

/// A run's time limit: the job timeout, or four times the length of the video it works on, so
/// hours of a session recording are not cut off as a hung ffmpeg would be.
pub fn limit_for(timeout: Duration, duration: f64) -> Duration {
    if duration.is_finite() && duration > 0.0 {
        timeout.max(Duration::from_secs_f64(duration * 4.0))
    } else {
        timeout
    }
}

pub fn part_path(output: &Path) -> PathBuf {
    let mut s = output.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}
