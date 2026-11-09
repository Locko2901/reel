//! The PC's memory, for the setup to warn when the replay buffer would take a quarter of it,
//! and its processors, for the quality it suggests.

pub fn memory_mb() -> Option<u64> {
    sys::memory_mb()
}

/// The PC's logical processors on Windows; on Linux those this process may run on.
pub fn logical_processors() -> Option<usize> {
    std::thread::available_parallelism().ok().map(|n| n.get())
}

#[cfg(windows)]
mod sys {
    pub fn memory_mb() -> Option<u64> {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut m = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        unsafe { GlobalMemoryStatusEx(&mut m) }.ok()?;
        Some(m.ullTotalPhys / (1024 * 1024))
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn memory_mb() -> Option<u64> {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        super::meminfo_mb(&text)
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn meminfo_mb(text: &str) -> Option<u64> {
    let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_memory_is_read() {
        assert_eq!(
            super::meminfo_mb("MemTotal:       32768000 kB\nMemFree: 1 kB\n"),
            Some(32000)
        );
        assert!(super::memory_mb().unwrap_or(1) > 0);
    }

    #[test]
    fn the_processors_are_counted() {
        assert!(super::logical_processors().is_some_and(|n| n >= 1));
    }
}
