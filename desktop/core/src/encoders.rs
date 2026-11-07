//! Which encoders the bundled OBS may record with on this PC: the ones OBS lists, and each
//! one's last test.
use serde::{Deserialize, Serialize};
use std::path::Path;

/// x264 on the CPU: OBS always has it.
pub const X264: &str = "obs_x264";

/// The video encoders OBS lists in its log as it starts, under "Available Encoders"; None when
/// the log has no such list (OBS stopped before it, or a dialog holds it).
pub fn listed(log: &str) -> Option<Vec<String>> {
    let lines: Vec<&str> = log.lines().map(untimed).collect();
    let at = lines
        .iter()
        .rposition(|l| l.trim() == "Available Encoders:")?;
    let mut ids = Vec::new();
    let mut video = false;
    for l in &lines[at + 1..] {
        let t = l.trim();
        if t == "Video Encoders:" {
            video = true;
        } else if let Some(entry) = t.strip_prefix("- ") {
            if video {
                ids.push(
                    entry
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                );
            }
        } else {
            break;
        }
    }
    Some(ids)
}

/// A line of OBS's log without the time OBS puts before it ("18:16:28.973: ").
fn untimed(l: &str) -> &str {
    let b = l.as_bytes();
    let timed = b.len() >= 14
        && b[2] == b':'
        && b[5] == b':'
        && b[8] == b'.'
        && b[12] == b':'
        && b[..12]
            .iter()
            .all(|c| c.is_ascii_digit() || matches!(c, b':' | b'.'));
    if timed {
        l[13..].strip_prefix(' ').unwrap_or(&l[13..])
    } else {
        l
    }
}

/// Words OBS's encoders and outputs write when a start fails.
const FAILED: &[&str] = &[
    "fail",
    "error",
    "not supported",
    "unsupported",
    "wrong adapter",
    "non-amd adapter",
    "could not",
    "couldn't",
    "cannot",
    "not found",
    "invalid",
];
/// What a line about the recording's encoder or its output names.
const ABOUT: &[&str] = &[
    "advanced_video_recording",
    "encoder",
    "replay",
    "amf",
    "qsv",
    "nvenc",
    "vaapi",
    "x264",
];

/// OBS's own reason an encoder did not record, from its log of that start: the lines after its
/// list of encoders that say a start of the encoder or the replay buffer failed.
pub fn obs_reason(log: &str) -> Vec<String> {
    let lines: Vec<&str> = log.lines().map(untimed).collect();
    let from = lines
        .iter()
        .rposition(|l| l.trim() == "Available Encoders:")
        .unwrap_or(0);
    let mut why: Vec<String> = lines[from..]
        .iter()
        .map(|l| l.trim())
        .filter(|l| {
            let low = l.to_ascii_lowercase();
            FAILED.iter().any(|w| low.contains(w)) && ABOUT.iter().any(|w| low.contains(w))
        })
        .map(str::to_string)
        .collect();
    why.dedup();
    let keep = why.len().saturating_sub(6);
    why.split_off(keep)
}

/// One encoder's last test on this PC: ffmpeg's test encode, then a start in OBS itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Verdict {
    pub id: String,
    pub ok: bool,
    /// Why not, in words; empty when it records.
    pub why: String,
}

/// What reel knows of the encoders on this PC, kept beside its ledger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Known {
    pub tested: Vec<Verdict>,
}

fn id_of(id: &str) -> &str {
    if id.is_empty() {
        X264
    } else {
        id
    }
}

impl Known {
    pub fn load(path: &Path) -> Known {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let part = path.with_extension("part");
        std::fs::write(&part, serde_json::to_vec_pretty(self).unwrap_or_default())?;
        std::fs::rename(&part, path)
    }

    pub fn verdict(&self, id: &str) -> Option<&Verdict> {
        self.tested.iter().find(|v| v.id == id_of(id))
    }

    pub fn set(&mut self, v: Verdict) {
        match self.tested.iter_mut().find(|t| t.id == v.id) {
            Some(t) => *t = v,
            None => self.tested.push(v),
        }
    }

    /// The selection rule: x264 always; another encoder only when its last test here passed,
    /// in ffmpeg and in OBS. The error is why not.
    pub fn selectable(&self, id: &str) -> Result<(), String> {
        let id = id_of(id);
        if id == X264 {
            return Ok(());
        }
        match self.verdict(id) {
            Some(v) if v.ok => Ok(()),
            Some(v) => Err(v.why.clone()),
            None => Err("not tested on this PC yet: the setup's Encoder step tests it".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "18:16:28.902: FFmpeg VAAPI H264 encoding not supported
18:16:28.920: NVENC not supported
18:16:28.973: ---------------------------------
18:16:28.973: Available Encoders:
18:16:28.973:   Video Encoders:
18:16:28.973: \t- ffmpeg_svt_av1 (SVT-AV1)
18:16:28.973: \t- obs_qsv11_v2 (QuickSync H.264)
18:16:28.973: \t- obs_x264 (x264)
18:16:28.973:   Audio Encoders:
18:16:28.973: \t- ffmpeg_aac (FFmpeg AAC)
18:16:28.973: ==== Startup complete ===============================================
18:16:29.133: [qsv encoder: 'advanced_video_recording'] settings:
18:16:29.140: [qsv encoder: 'advanced_video_recording'] qsv failed to load
18:16:29.141: creating encoder 'advanced_video_recording' (obs_qsv11_v2) failed
18:16:29.150: [rtmp-services plugin] Remote update failed: could not resolve host
";

    #[test]
    fn obs_lists_its_video_encoders_at_each_start() {
        assert_eq!(
            listed(LOG).unwrap(),
            ["ffmpeg_svt_av1", "obs_qsv11_v2", "obs_x264"]
        );
        assert_eq!(
            listed("12:00:00.000: Command Line Arguments: --multi\n"),
            None
        );
        let windows = LOG.replace('\n', "\r\n");
        assert_eq!(listed(&windows).unwrap().len(), 3, "Windows' line ends");
    }

    #[test]
    fn obs_reason_is_its_lines_about_the_failed_start() {
        assert_eq!(
            obs_reason(LOG),
            [
                "[qsv encoder: 'advanced_video_recording'] qsv failed to load",
                "creating encoder 'advanced_video_recording' (obs_qsv11_v2) failed"
            ],
            "the lines before the list of encoders and the unrelated ones are left out"
        );
        assert!(obs_reason("12:00:00.000: ==== Replay Buffer Start ====\n").is_empty());
    }

    fn tested(pairs: &[(&str, bool)]) -> Known {
        Known {
            tested: pairs
                .iter()
                .map(|(id, ok)| Verdict {
                    id: id.to_string(),
                    ok: *ok,
                    why: if *ok {
                        String::new()
                    } else {
                        format!("{id} failed")
                    },
                })
                .collect(),
        }
    }

    #[test]
    fn the_selection_rule() {
        let k = tested(&[("obs_qsv11_v2", true), ("h264_texture_amf", false)]);
        assert_eq!(k.selectable("obs_x264"), Ok(()));
        assert_eq!(k.selectable(""), Ok(()), "empty is x264");
        assert_eq!(k.selectable("obs_qsv11_v2"), Ok(()));
        assert_eq!(
            k.selectable("h264_texture_amf"),
            Err("h264_texture_amf failed".into()),
            "refused with the reason of its test"
        );
        assert!(
            k.selectable("obs_nvenc_h264_tex")
                .unwrap_err()
                .contains("not tested"),
            "never tested here: not offered"
        );
    }

    #[test]
    fn what_is_known_is_kept_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("encoders.json");
        assert_eq!(Known::load(&path), Known::default(), "none yet");
        let k = tested(&[("obs_qsv11_v2", false)]);
        k.save(&path).unwrap();
        assert_eq!(Known::load(&path), k);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(
            Known::load(&path),
            Known::default(),
            "a broken file is none"
        );
    }
}
