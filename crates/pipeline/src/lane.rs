//! The jobs someone waits for run in a lane of their own; while one runs, the background
//! jobs' ffmpeg is paused, so reel never runs two encodes at once on a shared GPU.
use reel_store::models::{Job, PRIORITY_LANE};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub fn in_lane(job: &Job) -> bool {
    job.priority <= PRIORITY_LANE
}

#[derive(Default)]
pub struct Lane {
    inner: Mutex<Inner>,
    pauses: AtomicU64,
}

#[derive(Default)]
struct Inner {
    running: usize,
    background: HashMap<u32, Paused>,
}

/// How long a background ffmpeg has been paused: the pauses that ended and the one under way.
#[derive(Default)]
struct Paused {
    before: Duration,
    since: Option<Instant>,
}

impl Paused {
    fn so_far(&self) -> Duration {
        self.before + self.since.map_or(Duration::ZERO, |s| s.elapsed())
    }
}

impl reel_media::ffmpeg::Pauser for Lane {
    fn started(&self, pid: u32) {
        let mut g = self.inner.lock().unwrap();
        let mut p = Paused::default();
        if g.running > 0 {
            self.pause(pid, &mut p);
        }
        g.background.insert(pid, p);
    }

    fn ended(&self, pid: u32) {
        self.inner.lock().unwrap().background.remove(&pid);
    }

    fn paused(&self, pid: u32) -> Duration {
        let g = self.inner.lock().unwrap();
        g.background
            .get(&pid)
            .map_or(Duration::ZERO, Paused::so_far)
    }
}

impl Lane {
    /// A lane job runs until the guard drops.
    pub fn enter(self: &Arc<Self>) -> Running {
        let mut g = self.inner.lock().unwrap();
        g.running += 1;
        if g.running == 1 && !g.background.is_empty() {
            tracing::info!(
                processes = g.background.len(),
                "background work paused while a job someone waits for runs"
            );
            for (pid, p) in g.background.iter_mut() {
                self.pause(*pid, p);
            }
        }
        Running(self.clone())
    }

    fn leave(&self) {
        let mut g = self.inner.lock().unwrap();
        g.running -= 1;
        if g.running == 0 && !g.background.is_empty() {
            tracing::info!("background work goes on");
            for (pid, p) in g.background.iter_mut() {
                self.signal(*pid, false);
                if let Some(s) = p.since.take() {
                    p.before += s.elapsed();
                }
            }
        }
    }

    fn pause(&self, pid: u32, p: &mut Paused) {
        if self.signal(pid, true) {
            p.since.get_or_insert_with(Instant::now);
        }
    }

    /// How many times a background ffmpeg was paused.
    pub fn pauses(&self) -> u64 {
        self.pauses.load(Ordering::Relaxed)
    }

    #[cfg(unix)]
    fn signal(&self, pid: u32, stop: bool) -> bool {
        use nix::sys::signal::{kill, Signal};
        let sig = if stop {
            Signal::SIGSTOP
        } else {
            Signal::SIGCONT
        };
        match kill(nix::unistd::Pid::from_raw(pid as i32), sig) {
            Ok(()) => {
                if stop {
                    self.pauses.fetch_add(1, Ordering::Relaxed);
                }
                true
            }
            Err(e) => {
                tracing::debug!(pid, error = %e, "a background ffmpeg was not signalled");
                false
            }
        }
    }

    /// Windows has no stop signal: there the background job runs on beside the lane's.
    #[cfg(not(unix))]
    fn signal(&self, _pid: u32, _stop: bool) -> bool {
        false
    }
}

pub struct Running(Arc<Lane>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.leave();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use reel_media::ffmpeg::{Ffmpeg, MediaError, OnSpawn, ON_SPAWN};
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    /// A stand-in for ffmpeg that works for `tenths` tenths of a second, and only while it
    /// is not stopped.
    fn worker(dir: &Path, tenths: u32) -> Ffmpeg {
        let path = dir.join(format!("ffmpeg-{tenths}"));
        std::fs::write(
            &path,
            format!("#!/bin/sh\ni=0\nwhile [ $i -lt {tenths} ]; do sleep 0.1; i=$((i+1)); done\n"),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Ffmpeg {
            ffmpeg: path.to_string_lossy().into_owned(),
            ffprobe: "ffprobe".into(),
            nice: 0,
            timeout: Duration::from_secs(2),
        }
    }

    /// Runs `ff` as a background job's ffmpeg while the lane runs a job for `held`.
    async fn paused_for(ff: Ffmpeg, held: Duration) -> (Result<(), MediaError>, Arc<Lane>) {
        let lane = Arc::new(Lane::default());
        let told: OnSpawn = lane.clone();
        let run = tokio::spawn(ON_SPAWN.scope(told, async move { ff.run(&[], 0.0, None).await }));
        for _ in 0..500 {
            if !lane.inner.lock().unwrap().background.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            !lane.inner.lock().unwrap().background.is_empty(),
            "no ffmpeg started"
        );
        {
            let _running = lane.enter();
            tokio::time::sleep(held).await;
        }
        (run.await.unwrap(), lane)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_time_a_background_ffmpeg_spends_paused_is_not_counted_towards_its_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let (short, long) = (worker(dir.path(), 10), worker(dir.path(), 40));
        let ((within, lane), (past, _)) = tokio::join!(
            paused_for(short, Duration::from_secs(3)),
            paused_for(long, Duration::from_secs(1)),
        );
        assert!(lane.pauses() >= 1);
        assert!(
            within.is_ok(),
            "1 s of work paused for 3 s, against a timeout of 2 s: {within:?}"
        );
        assert!(
            matches!(past, Err(MediaError::Timeout { .. })),
            "4 s of work, against a timeout of 2 s: {past:?}"
        );
    }
}
