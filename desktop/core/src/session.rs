//! A recording of a whole session, started and stopped by hand or by something else later: who
//! asked, the reminders while it runs, and the disk's room for it.
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// What started or stopped a session recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    Window,
    Tray,
    Hotkey,
    /// `--record-session`, `--start-session` or `--stop-session`.
    Flag,
    /// OBS itself: its own Start Recording, a recording running when reel connected, its Stop.
    Obs,
    /// The disk OBS records to, or the library's, ran short of room.
    Disk,
    /// reel quits.
    Quit,
    /// OBS ended or stopped answering mid recording.
    ObsEnded,
}

impl By {
    /// For the lines and the log: "started from the tray", "stopped by the hotkey".
    pub fn words(self) -> &'static str {
        match self {
            By::Window => "from the window",
            By::Tray => "from the tray",
            By::Hotkey => "by the hotkey",
            By::Flag => "from the command line",
            By::Obs => "in OBS",
            By::Disk => "by reel, the disk being almost full",
            By::Quit => "by reel's Quit",
            By::ObsEnded => "by OBS ending",
        }
    }
}

/// A session recording that runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Recording {
    pub started: SystemTime,
    pub by: By,
    /// The file OBS writes, once it said.
    pub file: Option<PathBuf>,
    /// Files OBS finished before this one, when it split the recording.
    pub parts: u32,
    /// How far in the last reminder came; zero before the first.
    pub reminded_at: Duration,
    /// The game seen last while it ran, which names it.
    pub game: Option<String>,
    /// A stop was asked, and by what; OBS has not said that it stopped yet.
    pub stopping: Option<By>,
}

impl Recording {
    pub fn new(started: SystemTime, by: By) -> Recording {
        Recording {
            started,
            by,
            file: None,
            parts: 0,
            reminded_at: Duration::ZERO,
            game: None,
            stopping: None,
        }
    }

    /// How long it runs by the wall clock; zero when the clock went back past the start.
    pub fn length(&self, now: SystemTime) -> Duration {
        now.duration_since(self.started).unwrap_or_default()
    }

    /// The line the tray and the window show: "1 h 12 min, started from the tray".
    pub fn line(&self, now: SystemTime) -> String {
        if let Some(by) = self.stopping {
            return format!("stopping, asked {}", by.words());
        }
        format!(
            "{}, started {}",
            minutes_words(self.length(now)),
            self.by.words()
        )
    }
}

/// Whether a reminder is due: one at each `every_minutes` of the recording on the wall clock,
/// counted from its start. After a sleep only one comes, however many marks it slept through;
/// an interval changed mid recording counts on from the last reminder's mark.
pub fn reminder_due(length: Duration, every_minutes: u32, reminded_at: Duration) -> bool {
    if every_minutes == 0 {
        return false;
    }
    let every = u64::from(every_minutes) * 60;
    length.as_secs() / every > reminded_at.as_secs() / every
}

/// For a line shown by the minute: "under a minute", "12 min", "1 h 12 min".
pub fn minutes_words(d: Duration) -> String {
    if d.as_secs() < 60 {
        "under a minute".into()
    } else {
        length_words(d)
    }
}

/// "12 s", "12 min", "1 h", "1 h 12 min".
pub fn length_words(d: Duration) -> String {
    let s = d.as_secs();
    match (s / 3600, (s % 3600) / 60) {
        (0, 0) => format!("{s} s"),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// Below this much room on the disk OBS records to, a session recording does not start, and one
/// that runs stops: what is left keeps the PC and its games working.
pub const FLOOR: u64 = 5_000_000_000;

/// Whether a disk is short of room: under the floor once `needs` more bytes are written to it
/// (the library's copy of the recording still to be made there, in standalone).
pub fn short_of_room(free: u64, needs: u64) -> bool {
    free < FLOOR.saturating_add(needs)
}

/// What a start depends on, as the app knows it now.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub leaving: bool,
    pub recording: bool,
    pub starting: bool,
    pub obs_set_up: bool,
    pub connected: bool,
    /// The setup tries an encoder in OBS.
    pub trial: bool,
    /// The free bytes of the disk OBS records to, when known.
    pub free: Option<u64>,
}

/// Why a session recording cannot start now; None when it can.
pub fn start_refused(f: &Facts) -> Option<String> {
    Some(if f.leaving {
        "reel is quitting".into()
    } else if f.recording {
        "a session recording runs already".into()
    } else if f.starting {
        "a session recording is starting".into()
    } else if !f.obs_set_up {
        "no OBS is set up for reel".into()
    } else if !f.connected {
        "OBS is not connected".into()
    } else if f.trial {
        "the setup is trying an encoder in OBS".into()
    } else {
        let free = f.free.filter(|free| short_of_room(*free, 0))?;
        format!(
            "the disk OBS records to has {} left, under the {} reel keeps free",
            size_words(free),
            size_words(FLOOR)
        )
    })
}

/// "4.8 GB", "820 MB".
pub fn size_words(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1e9)
    } else {
        format!("{} MB", bytes / 1_000_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);

    /// Runs a recording minute by minute; the minutes at which a reminder came.
    fn reminders(minutes: u64, every: u32, slept: Option<(u64, u64)>) -> Vec<u64> {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let mut r = Recording::new(start, By::Hotkey);
        let mut came = Vec::new();
        for m in 0..=minutes {
            if slept.is_some_and(|(from, to)| m > from && m < to) {
                continue;
            }
            let length = r.length(start + Duration::from_secs(m * 60 + 3));
            if reminder_due(length, every, r.reminded_at) {
                r.reminded_at = length;
                came.push(m);
            }
        }
        came
    }

    #[test]
    fn a_reminder_comes_every_interval_from_the_start() {
        assert_eq!(reminders(125, 30, None), [30, 60, 90, 120]);
        assert_eq!(reminders(29, 30, None), Vec::<u64>::new());
        assert_eq!(reminders(10, 1, None), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(reminders(300, 0, None), Vec::<u64>::new(), "off");
    }

    #[test]
    fn a_pc_that_slept_through_several_marks_owes_one_reminder() {
        assert_eq!(
            reminders(200, 30, Some((40, 175))),
            [30, 175, 180],
            "asleep from 40 min to 175 min: one at waking, for the 150 min mark, then 180 on time"
        );
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let r = Recording::new(start, By::Tray);
        assert_eq!(
            r.length(start - MIN),
            Duration::ZERO,
            "a clock set back before the start"
        );
        assert!(!reminder_due(r.length(start - MIN), 30, Duration::ZERO));
    }

    #[test]
    fn an_interval_changed_mid_recording_counts_on_from_the_last_reminder() {
        assert!(reminder_due(25 * MIN, 10, Duration::ZERO));
        assert!(
            !reminder_due(26 * MIN, 30, 25 * MIN),
            "from 10 to 30 min after the one at 25 min: none until 30"
        );
        assert!(reminder_due(30 * MIN, 30, 25 * MIN));
        assert!(
            reminder_due(41 * MIN, 20, 30 * MIN),
            "from 30 to 20 min after the one at 30: the 40 min mark"
        );
    }

    #[test]
    fn the_lengths_in_words() {
        assert_eq!(length_words(Duration::from_secs(12)), "12 s");
        assert_eq!(length_words(Duration::from_secs(12 * 60 + 59)), "12 min");
        assert_eq!(length_words(Duration::from_secs(3600)), "1 h");
        assert_eq!(length_words(Duration::from_secs(4320 + 30)), "1 h 12 min");
        assert_eq!(minutes_words(Duration::from_secs(59)), "under a minute");
        assert_eq!(minutes_words(Duration::from_secs(61)), "1 min");
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let mut r = Recording::new(start, By::Tray);
        assert_eq!(
            r.line(start + 72 * MIN),
            "1 h 12 min, started from the tray"
        );
        r.stopping = Some(By::Hotkey);
        assert_eq!(r.line(start + 72 * MIN), "stopping, asked by the hotkey");
        assert_eq!(size_words(4_830_000_000), "4.8 GB");
        assert_eq!(size_words(820_400_000), "820 MB");
    }

    #[test]
    fn a_start_says_why_not_first_what_matters_most() {
        let ready = Facts {
            obs_set_up: true,
            connected: true,
            free: Some(200_000_000_000),
            ..Facts::default()
        };
        assert_eq!(start_refused(&ready), None);
        let not = |change: fn(&mut Facts)| {
            let mut f = ready.clone();
            change(&mut f);
            start_refused(&f).unwrap()
        };
        assert_eq!(not(|f| f.connected = false), "OBS is not connected");
        assert_eq!(not(|f| f.obs_set_up = false), "no OBS is set up for reel");
        assert_eq!(
            not(|f| f.recording = true),
            "a session recording runs already"
        );
        assert_eq!(
            not(|f| f.starting = true),
            "a session recording is starting"
        );
        assert_eq!(
            not(|f| f.trial = true),
            "the setup is trying an encoder in OBS"
        );
        assert_eq!(
            not(|f| f.free = Some(3_100_000_000)),
            "the disk OBS records to has 3.1 GB left, under the 5.0 GB reel keeps free"
        );
        assert_eq!(
            not(|f| {
                f.leaving = true;
                f.connected = false;
            }),
            "reel is quitting"
        );
        let unknown = Facts {
            free: None,
            ..ready.clone()
        };
        assert_eq!(
            start_refused(&unknown),
            None,
            "a disk not read is no reason"
        );
    }

    #[test]
    fn the_room_a_recording_needs_counts_the_librarys_copy_still_to_come() {
        assert!(!short_of_room(40_000_000_000, 0));
        assert!(short_of_room(4_999_999_999, 0));
        assert!(
            short_of_room(22_000_000_000, 17_500_000_000),
            "22 GB free and a 17.5 GB recording the library copies onto the same disk"
        );
        assert!(!short_of_room(23_000_000_000, 17_500_000_000));
        assert!(short_of_room(0, u64::MAX), "no overflow");
    }
}
