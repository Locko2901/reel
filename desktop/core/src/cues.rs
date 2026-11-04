//! The volume is applied by scaling the samples into a cached copy once, since the system
//! players have no volume knob.
use crate::settings::CueSettings;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const SAVE_WAV: &[u8] = include_bytes!("../../assets/sounds/save.wav");
pub const UPLOADED_WAV: &[u8] = include_bytes!("../../assets/sounds/uploaded.wav");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cue {
    Captured,
    Uploaded,
    LinkCopied,
}

impl Cue {
    pub const ALL: [Cue; 3] = [Cue::Captured, Cue::Uploaded, Cue::LinkCopied];

    pub fn name(&self) -> &'static str {
        match self {
            Cue::Captured => "captured",
            Cue::Uploaded => "uploaded",
            Cue::LinkCopied => "link_copied",
        }
    }

    pub fn parse(s: &str) -> Option<Cue> {
        Cue::ALL.iter().copied().find(|c| c.name() == s)
    }

    pub fn bundled(&self) -> &'static [u8] {
        match self {
            Cue::Captured => SAVE_WAV,
            Cue::Uploaded | Cue::LinkCopied => UPLOADED_WAV,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CueError {
    #[error("{0}: only a WAV file can be a cue")]
    NotWav(PathBuf),
    #[error("{0}: cannot read it: {1}")]
    Read(PathBuf, std::io::Error),
    #[error("not a WAV we can read: {0}")]
    Decode(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub fn scaled_wav(bytes: &[u8], volume: f32) -> Result<Vec<u8>, CueError> {
    let volume = volume.clamp(0.0, 1.0);
    let mut reader =
        hound::WavReader::new(Cursor::new(bytes)).map_err(|e| CueError::Decode(e.to_string()))?;
    let spec = reader.spec();
    let mut out = Cursor::new(Vec::new());
    {
        let mut writer =
            hound::WavWriter::new(&mut out, spec).map_err(|e| CueError::Decode(e.to_string()))?;
        match (spec.sample_format, spec.bits_per_sample) {
            (hound::SampleFormat::Float, _) => {
                for s in reader.samples::<f32>() {
                    let s = s.map_err(|e| CueError::Decode(e.to_string()))?;
                    writer
                        .write_sample(s * volume)
                        .map_err(|e| CueError::Decode(e.to_string()))?;
                }
            }
            (hound::SampleFormat::Int, bits) if bits <= 16 => {
                for s in reader.samples::<i16>() {
                    let s = s.map_err(|e| CueError::Decode(e.to_string()))?;
                    writer
                        .write_sample((s as f32 * volume).round() as i16)
                        .map_err(|e| CueError::Decode(e.to_string()))?;
                }
            }
            (hound::SampleFormat::Int, _) => {
                for s in reader.samples::<i32>() {
                    let s = s.map_err(|e| CueError::Decode(e.to_string()))?;
                    writer
                        .write_sample((s as f64 * volume as f64).round() as i32)
                        .map_err(|e| CueError::Decode(e.to_string()))?;
                }
            }
        }
        writer
            .finalize()
            .map_err(|e| CueError::Decode(e.to_string()))?;
    }
    Ok(out.into_inner())
}

pub fn source(cue: Cue, settings: &CueSettings) -> Result<Vec<u8>, CueError> {
    match &settings.file {
        Some(p) => {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !ext.eq_ignore_ascii_case("wav") {
                return Err(CueError::NotWav(p.clone()));
            }
            std::fs::read(p).map_err(|e| CueError::Read(p.clone(), e))
        }
        None => Ok(cue.bundled().to_vec()),
    }
}

fn cache_name(cue: Cue, settings: &CueSettings) -> String {
    let vol = (settings.volume.clamp(0.0, 1.0) * 100.0).round() as u32;
    match &settings.file {
        Some(p) => {
            let meta = std::fs::metadata(p).ok();
            let len = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let id = &crate::hash::sha256_hex(p.to_string_lossy().as_bytes())[..8];
            format!("{}-{vol}-custom-{id}-{len}-{mtime}.wav", cue.name())
        }
        None => format!("{}-{vol}.wav", cue.name()),
    }
}

#[derive(Clone)]
pub enum Backend {
    #[cfg(windows)]
    WinMm,
    #[cfg(not(windows))]
    Command(String),
    /// Records what would have played, when no player exists and in the tests.
    Silent(Arc<Mutex<Vec<String>>>),
}

impl Backend {
    pub fn name(&self) -> String {
        match self {
            #[cfg(windows)]
            Backend::WinMm => "winmm".into(),
            #[cfg(not(windows))]
            Backend::Command(c) => c.clone(),
            Backend::Silent(_) => "silent".into(),
        }
    }
}

pub struct Player {
    cache_dir: PathBuf,
    backend: Backend,
}

impl Player {
    /// winmm on Windows; elsewhere paplay, aplay or ffplay from the PATH, or silence.
    pub fn new(cache_dir: &Path) -> Player {
        Player {
            cache_dir: cache_dir.to_path_buf(),
            backend: detect_backend(),
        }
    }

    pub fn silent(cache_dir: &Path) -> Player {
        Player {
            cache_dir: cache_dir.to_path_buf(),
            backend: Backend::Silent(Arc::new(Mutex::new(Vec::new()))),
        }
    }

    pub fn backend(&self) -> &Backend {
        &self.backend
    }

    /// What the silent backend recorded, by cue name.
    pub fn played(&self) -> Vec<String> {
        match &self.backend {
            Backend::Silent(v) => v.lock().unwrap().clone(),
            #[allow(unreachable_patterns)]
            _ => Vec::new(),
        }
    }

    pub fn prepare(&self, cue: Cue, settings: &CueSettings) -> Result<PathBuf, CueError> {
        std::fs::create_dir_all(&self.cache_dir)?;
        let file = self.cache_dir.join(cache_name(cue, settings));
        if file.exists() {
            return Ok(file);
        }
        let bytes = scaled_wav(&source(cue, settings)?, settings.volume)?;
        let part = crate::settings::part_path(&file);
        std::fs::write(&part, bytes)?;
        std::fs::rename(&part, &file)?;
        Ok(file)
    }

    /// Never fails loudly: one log line says what happened, which the end to end run asserts.
    pub fn play(&self, cue: Cue, settings: &CueSettings) {
        if !settings.enabled {
            tracing::info!("cue {} skipped: off in the settings", cue.name());
            return;
        }
        self.sound(cue, settings);
    }

    /// Plays even when the cue is off.
    pub fn preview(&self, cue: Cue, settings: &CueSettings) -> Result<(), CueError> {
        let file = self.prepare(cue, settings)?;
        self.start(cue, &file);
        Ok(())
    }

    fn sound(&self, cue: Cue, settings: &CueSettings) {
        match self.prepare(cue, settings) {
            Ok(file) => self.start(cue, &file),
            Err(e) => tracing::warn!("cue {} skipped: {e}", cue.name()),
        }
    }

    fn start(&self, cue: Cue, file: &Path) {
        match &self.backend {
            Backend::Silent(v) => {
                v.lock().unwrap().push(cue.name().to_string());
                tracing::info!("cue {} played (silent: no audio output here)", cue.name());
            }
            #[cfg(not(windows))]
            Backend::Command(cmd) => {
                let mut c = reel_core::proc::command(cmd);
                if cmd == "ffplay" {
                    c.args(["-nodisp", "-autoexit", "-loglevel", "quiet"]);
                }
                c.arg(file)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                match c.spawn() {
                    Ok(mut child) => {
                        std::thread::spawn(move || {
                            let _ = child.wait();
                        });
                        tracing::info!("cue {} played ({cmd})", cue.name());
                    }
                    Err(e) => tracing::warn!("cue {} skipped: {cmd}: {e}", cue.name()),
                }
            }
            #[cfg(windows)]
            Backend::WinMm => {
                use windows::core::PCWSTR;
                use windows::Win32::Media::Audio::{
                    PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT,
                };
                let wide: Vec<u16> = file
                    .as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                let ok = unsafe {
                    PlaySoundW(
                        PCWSTR(wide.as_ptr()),
                        None,
                        SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
                    )
                };
                if ok.as_bool() {
                    tracing::info!("cue {} played (winmm)", cue.name());
                } else {
                    tracing::warn!("cue {} skipped: PlaySound refused the file", cue.name());
                }
            }
        }
    }
}

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
fn detect_backend() -> Backend {
    Backend::WinMm
}

#[cfg(not(windows))]
fn detect_backend() -> Backend {
    for cmd in ["paplay", "aplay", "ffplay"] {
        if on_path(cmd) {
            return Backend::Command(cmd.to_string());
        }
    }
    Backend::Silent(Arc::new(Mutex::new(Vec::new())))
}

#[cfg(not(windows))]
fn on_path(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p).any(|d| {
                let f = d.join(cmd);
                f.is_file()
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(bytes: &[u8]) -> Vec<i16> {
        hound::WavReader::new(Cursor::new(bytes))
            .unwrap()
            .samples::<i16>()
            .map(|s| s.unwrap())
            .collect()
    }

    #[test]
    fn scaling_halves_the_samples_and_keeps_the_format() {
        let full = samples(SAVE_WAV);
        let half = samples(&scaled_wav(SAVE_WAV, 0.5).unwrap());
        assert_eq!(full.len(), half.len());
        let loud = full
            .iter()
            .zip(&half)
            .find(|(a, _)| a.abs() > 1000)
            .unwrap();
        assert!((loud.0 / 2 - *loud.1).abs() <= 1, "{loud:?}");
        let spec = hound::WavReader::new(Cursor::new(SAVE_WAV)).unwrap().spec();
        assert_eq!(spec.channels, 2);
        assert_eq!(spec.sample_rate, 44_100);
        let muted = samples(&scaled_wav(UPLOADED_WAV, 0.0).unwrap());
        assert!(muted.iter().all(|s| *s == 0));
        assert!(scaled_wav(b"not a wav", 1.0).is_err());
    }

    #[test]
    fn a_custom_cue_must_be_a_wav_and_the_silent_backend_records() {
        let dir = tempfile::tempdir().unwrap();
        let player = Player::silent(&dir.path().join("cues"));
        let mp3 = dir.path().join("x.mp3");
        std::fs::write(&mp3, b"x").unwrap();
        let bad = CueSettings {
            enabled: true,
            volume: 1.0,
            file: Some(mp3),
        };
        assert!(matches!(
            player.preview(Cue::Captured, &bad),
            Err(CueError::NotWav(_))
        ));
        let custom = dir.path().join("own.wav");
        std::fs::write(&custom, UPLOADED_WAV).unwrap();
        let own = CueSettings {
            enabled: true,
            volume: 0.3,
            file: Some(custom),
        };
        let f = player.prepare(Cue::LinkCopied, &own).unwrap();
        assert!(f
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("link_copied-30-custom"));
        player.play(Cue::LinkCopied, &own);
        let off = CueSettings {
            enabled: false,
            ..CueSettings::default()
        };
        player.play(Cue::Uploaded, &off);
        player.play(Cue::Captured, &CueSettings::default());
        assert_eq!(
            player.played(),
            vec!["link_copied".to_string(), "captured".to_string()]
        );
        assert!(dir.path().join("cues").join("captured-100.wav").exists());
        assert_eq!(Cue::parse("uploaded"), Some(Cue::Uploaded));
        assert_eq!(Cue::parse("nope"), None);
        assert!(Player::new(dir.path()).backend().name().len() > 1);
    }
}
