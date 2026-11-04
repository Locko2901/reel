use crate::config::MediaConfig;
use crate::probe::ProbeInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VideoPlan {
    Copy,
    Encode { max_height: u32, mbit: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AudioPlan {
    None,
    Copy,
    Encode { kbit: u32, normalize: Option<i32> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub video: VideoPlan,
    pub audio: AudioPlan,
    pub reason: String,
}

impl Plan {
    pub fn copies_video(&self) -> bool {
        matches!(self.video, VideoPlan::Copy)
    }
    pub fn copies_audio(&self) -> bool {
        matches!(self.audio, AudioPlan::Copy)
    }
    pub fn has_audio(&self) -> bool {
        !matches!(self.audio, AudioPlan::None)
    }
}

/// The AAC rate for a re-encode: the configured rate, or the source's when that is lower.
pub fn audio_kbit(m: &MediaConfig, source_bits: Option<u64>) -> u32 {
    match source_bits {
        Some(b) if b > 0 && (b / 1000) < m.audio_kbit as u64 => (b / 1000).max(32) as u32,
        _ => m.audio_kbit,
    }
}

pub fn plan_master(p: &ProbeInfo, m: &MediaConfig) -> Plan {
    let pix_ok = matches!(
        p.pix_fmt.as_deref(),
        Some("yuv420p") | Some("yuvj420p") | None
    );
    let mbit = p.vbitrate_mbit();
    let encode = VideoPlan::Encode {
        max_height: m.copy_max_height,
        mbit: m.encode_mbit,
    };
    let (video, video_reason) = if !p.is_h264() {
        (
            encode,
            format!(
                "video re-encoded: {} is not H.264",
                p.vcodec.clone().unwrap_or_else(|| "the codec".into())
            ),
        )
    } else if p.height > m.copy_max_height {
        (
            encode,
            format!(
                "video re-encoded: {}p is over {}p",
                p.height, m.copy_max_height
            ),
        )
    } else if mbit > m.copy_max_mbit as f64 {
        (
            encode,
            format!(
                "video re-encoded: {:.1} Mbit/s is over {} Mbit/s",
                mbit, m.copy_max_mbit
            ),
        )
    } else if !pix_ok {
        (
            encode,
            format!(
                "video re-encoded: {} is not 8-bit 4:2:0",
                p.pix_fmt.clone().unwrap_or_default()
            ),
        )
    } else {
        (
            VideoPlan::Copy,
            format!("video copied: H.264 {}p at {:.1} Mbit/s", p.height, mbit),
        )
    };
    let (audio, audio_reason) = match p.acodec.as_deref() {
        None => (AudioPlan::None, "no audio".to_string()),
        Some("aac") if !m.normalize_audio => (AudioPlan::Copy, "audio copied: AAC".to_string()),
        Some(codec) => {
            let kbit = audio_kbit(m, p.abitrate);
            let why = if codec == "aac" {
                "normalized".to_string()
            } else {
                format!("was {codec}")
            };
            (
                AudioPlan::Encode {
                    kbit,
                    normalize: m.normalize_audio.then_some(m.loudness_target),
                },
                format!("audio to AAC {kbit} kbit/s ({why})"),
            )
        }
    };
    Plan {
        video,
        audio,
        reason: format!("{video_reason}; {audio_reason}"),
    }
}

/// What a copy is made from: the current version's shape and audio.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub acodec: Option<String>,
    pub abitrate: Option<u64>,
    pub vbitrate: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioChoice {
    None,
    Copy { bits: u64 },
    Encode { kbit: u32 },
}

impl AudioChoice {
    pub fn bits(&self) -> u64 {
        match self {
            AudioChoice::None => 0,
            AudioChoice::Copy { bits } => *bits,
            AudioChoice::Encode { kbit } => *kbit as u64 * 1000,
        }
    }
    fn words(&self) -> String {
        match self {
            AudioChoice::None => "no audio".into(),
            AudioChoice::Copy { bits } => format!("audio copied at {} kbit/s", bits / 1000),
            AudioChoice::Encode { kbit } => format!("audio AAC {kbit} kbit/s"),
        }
    }
}

/// A size-limited copy, chosen audio first: the best video the remaining budget allows.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyPlan {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub scales: bool,
    pub drops_fps: bool,
    pub vbit: u64,
    pub audio: AudioChoice,
    pub audio_lowered: bool,
    pub preset: String,
    pub reason: String,
}

/// Bumped when the ladder or the audio rule changes, so cached copies are remade.
pub const LADDER_VERSION: u32 = 2;
/// The bits per pixel per second a rung has to reach to be taken.
pub const BPP_TARGET: f64 = 0.08;
/// The same for the embed copy: 720p60 from 0.06, the rate a 720p60 stream runs at.
pub const BPP_TARGET_EMBED: f64 = 0.06;
/// What the lowest rung may fall to before the audio is lowered, and then before the copy is
/// refused.
pub const BPP_FLOOR: f64 = 0.04;
/// The share of the limit the streams may take; the container has the rest.
pub const SIZE_MARGIN: f64 = 0.93;
const AUDIO_STEPS: [u32; 4] = [256, 192, 160, 128];

fn even(n: f64) -> u32 {
    let w = n.round() as u32;
    w - w % 2
}

/// The source audio as the setting says: copied when it is AAC and the setting allows, else
/// AAC at the setting's rate or the source rate up to the configured one.
pub fn copy_audio(src: &SourceInfo, m: &MediaConfig) -> AudioChoice {
    let Some(codec) = src.acodec.as_deref() else {
        return AudioChoice::None;
    };
    match m.copy_audio.parse::<u32>() {
        Ok(kbit) => AudioChoice::Encode { kbit },
        Err(_) if codec == "aac" => AudioChoice::Copy {
            bits: src.abitrate.filter(|b| *b > 0).unwrap_or(320_000),
        },
        Err(_) => AudioChoice::Encode {
            kbit: audio_kbit(m, src.abitrate),
        },
    }
}

/// The rungs in the order they are tried: the source size first (up to `max_height`), then
/// lower heights at the source frame rate, and only then the same heights at 30 fps.
fn rungs(src: &SourceInfo, max_height: u32) -> Vec<(u32, u32, f64)> {
    let mut heights = vec![src.height.min(max_height)];
    for h in [1080, 720, 480] {
        if h < heights[0] {
            heights.push(h);
        }
    }
    let mut rates = vec![src.fps];
    if src.fps > 30.5 {
        rates.push(30.0);
    }
    let mut out = Vec::new();
    for fps in rates {
        for &h in &heights {
            let w = if h == src.height {
                even(src.width as f64)
            } else {
                even(h as f64 * src.width as f64 / src.height.max(1) as f64)
            };
            out.push((w, h, fps));
        }
    }
    out
}

fn bpp(vbit: u64, w: u32, h: u32, fps: f64) -> f64 {
    vbit as f64 / (w as f64 * h as f64 * fps)
}

/// The web copy of a version for its link's page: its own height up to `max_height` and frame
/// rate, `mbit` for video and audio together less the container's share.
pub fn web_plan(src: &SourceInfo, m: &MediaConfig, mbit: u64, max_height: u32) -> CopyPlan {
    let height = even(src.height.min(max_height) as f64);
    let width = if height == src.height {
        even(src.width as f64)
    } else {
        even(height as f64 * src.width as f64 / src.height.max(1) as f64)
    };
    let audio = copy_audio(src, m);
    let vbit = ((mbit * 1_000_000) as f64 * SIZE_MARGIN) as u64;
    let vbit = vbit.saturating_sub(audio.bits()).max(1_000_000);
    CopyPlan {
        width,
        height,
        fps: src.fps,
        scales: height < src.height,
        drops_fps: false,
        vbit,
        reason: format!(
            "{height}p{} at {:.1} Mbit/s, {}",
            src.fps.round(),
            vbit as f64 / 1e6,
            audio.words()
        ),
        audio,
        audio_lowered: false,
        preset: "medium".into(),
    }
}

/// What a copy is for: a size-limited copy is refused when nothing fits; the embed copy takes a
/// lower target and is made at the lowest rung anyway, since a link needs one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Purpose {
    Limited,
    Embed,
}

impl Purpose {
    fn target(self) -> f64 {
        match self {
            Purpose::Limited => BPP_TARGET,
            Purpose::Embed => BPP_TARGET_EMBED,
        }
    }

    fn must(self) -> bool {
        self == Purpose::Embed
    }
}

/// The copy under `limit_bytes`: the first rung reaching the purpose's target, else the lowest
/// above the floor, then lower audio step by step, then refused or, for an embed, the lowest.
pub fn copy_plan(
    limit_bytes: u64,
    duration: f64,
    src: &SourceInfo,
    m: &MediaConfig,
    max_height: u32,
    purpose: Purpose,
) -> Result<CopyPlan, String> {
    let target = purpose.target();
    let must = purpose.must();
    let duration = duration.max(0.5);
    let budget = limit_bytes as f64 * SIZE_MARGIN * 8.0 / duration;
    let audio = copy_audio(src, m);
    let ladder = rungs(src, max_height);
    let cap = src.vbitrate.filter(|b| *b > 0).map(|b| b.max(500_000));
    let video_bits = |audio_bits: u64| -> u64 {
        let v = (budget - audio_bits as f64).max(0.0) as u64;
        match cap {
            Some(c) => v.min(c),
            None => v,
        }
    };
    let pick = |audio: &AudioChoice, lowered: bool, min_bpp: f64| -> Option<CopyPlan> {
        let vbit = video_bits(audio.bits());
        let last = ladder.len() - 1;
        for (i, &(w, h, fps)) in ladder.iter().enumerate() {
            let density = bpp(vbit, w, h, fps);
            if density >= target || (i == last && density >= min_bpp) {
                let mut reason = format!(
                    "{h}p{} at {:.1} Mbit/s, {}",
                    fps.round(),
                    vbit as f64 / 1e6,
                    audio.words()
                );
                if lowered {
                    reason.push_str(&format!(
                        "; audio lowered to {} kbit/s to fit {} MB",
                        audio.bits() / 1000,
                        limit_bytes / 1_000_000
                    ));
                }
                return Some(CopyPlan {
                    width: w,
                    height: h,
                    fps,
                    scales: h < src.height,
                    drops_fps: fps < src.fps - 0.5,
                    vbit,
                    audio: audio.clone(),
                    audio_lowered: lowered,
                    preset: m.x264_preset.clone(),
                    reason,
                });
            }
        }
        None
    };
    if let Some(plan) = pick(&audio, false, BPP_FLOOR) {
        return Ok(plan);
    }
    if !matches!(audio, AudioChoice::None) {
        for kbit in AUDIO_STEPS {
            if (kbit as u64 * 1000) >= audio.bits() {
                continue;
            }
            if let Some(plan) = pick(&AudioChoice::Encode { kbit }, true, BPP_FLOOR) {
                return Ok(plan);
            }
        }
    }
    if must {
        let lowest = match audio {
            AudioChoice::None => AudioChoice::None,
            _ => AudioChoice::Encode { kbit: 128 },
        };
        return pick(&lowest, true, 0.0).ok_or_else(|| "no rung at all".to_string());
    }
    Err(format!(
        "a {} s clip does not fit under {} MB at a watchable quality, even at 480p30 with the audio at 128 kbit/s",
        duration.round(),
        limit_bytes / 1_000_000
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_web_copy_keeps_the_height_up_to_the_cap_and_the_rate_with_the_audio_inside() {
        let m = MediaConfig::default();
        let src = |w, h| SourceInfo {
            width: w,
            height: h,
            fps: 60.0,
            acodec: Some("aac".into()),
            abitrate: Some(320_000),
            vbitrate: Some(30_000_000),
        };
        let p = web_plan(&src(2560, 1440), &m, 10, 1440);
        assert_eq!(
            (p.width, p.height, p.scales, p.drops_fps),
            (2560, 1440, false, false)
        );
        assert_eq!(p.vbit, 8_980_000);
        assert_eq!(p.audio, AudioChoice::Copy { bits: 320_000 });
        let p = web_plan(&src(3840, 2160), &m, 10, 1440);
        assert_eq!((p.width, p.height, p.scales), (2560, 1440, true));
        let p = web_plan(&src(1920, 1080), &m, 10, 1440);
        assert_eq!((p.width, p.height, p.scales), (1920, 1080, false));
        assert!(
            p.reason.starts_with("1080p60 at 9.0 Mbit/s"),
            "{}",
            p.reason
        );
    }

    fn probe(
        codec: &str,
        h: u32,
        mbit: f64,
        pix: &str,
        acodec: Option<&str>,
        akbit: u64,
    ) -> ProbeInfo {
        ProbeInfo {
            vcodec: Some(codec.into()),
            height: h,
            width: h * 16 / 9,
            vbitrate: Some((mbit * 1e6) as u64),
            pix_fmt: Some(pix.into()),
            acodec: acodec.map(String::from),
            abitrate: if akbit > 0 { Some(akbit * 1000) } else { None },
            duration: 60.0,
            ..Default::default()
        }
    }

    #[test]
    fn the_obs_profile_is_remuxed_untouched() {
        let p = plan_master(
            &probe("h264", 1080, 16.0, "yuv420p", Some("aac"), 320),
            &MediaConfig::default(),
        );
        assert!(p.copies_video() && p.copies_audio(), "{}", p.reason);
        assert_eq!(
            p.reason,
            "video copied: H.264 1080p at 16.0 Mbit/s; audio copied: AAC"
        );
    }

    #[test]
    fn copy_limits_are_1440p_and_50_mbit() {
        let m = MediaConfig::default();
        assert!(
            plan_master(&probe("h264", 1440, 45.0, "yuv420p", Some("aac"), 320), &m).copies_video()
        );
        assert!(
            !plan_master(&probe("h264", 2160, 20.0, "yuv420p", Some("aac"), 320), &m)
                .copies_video()
        );
        assert!(
            !plan_master(&probe("h264", 1080, 60.0, "yuv420p", Some("aac"), 320), &m)
                .copies_video()
        );
        assert!(
            !plan_master(&probe("hevc", 1080, 9.0, "yuv420p", Some("aac"), 320), &m).copies_video()
        );
        assert!(
            !plan_master(&probe("av1", 1080, 5.0, "yuv420p", Some("aac"), 320), &m).copies_video()
        );
        assert!(
            !plan_master(&probe("vp9", 1080, 5.0, "yuv420p", Some("aac"), 320), &m).copies_video()
        );
        assert!(!plan_master(
            &probe("h264", 1080, 9.0, "yuv420p10le", Some("aac"), 320),
            &m
        )
        .copies_video());
        let r = plan_master(&probe("hevc", 1440, 30.0, "yuv420p", Some("aac"), 320), &m).reason;
        assert!(r.contains("hevc is not H.264"), "{r}");
    }

    #[test]
    fn audio_is_copied_when_aac_else_aac_at_320_or_the_source_rate() {
        let m = MediaConfig::default();
        assert!(
            plan_master(&probe("h264", 1080, 16.0, "yuv420p", Some("aac"), 128), &m).copies_audio()
        );
        let opus = plan_master(&probe("h264", 1080, 16.0, "yuv420p", Some("opus"), 128), &m);
        assert_eq!(
            opus.audio,
            AudioPlan::Encode {
                kbit: 128,
                normalize: None
            }
        );
        assert!(
            opus.reason.ends_with("audio to AAC 128 kbit/s (was opus)"),
            "{}",
            opus.reason
        );
        let pcm = plan_master(
            &probe("h264", 1080, 16.0, "yuv420p", Some("pcm_s16le"), 1536),
            &m,
        );
        assert_eq!(
            pcm.audio,
            AudioPlan::Encode {
                kbit: 320,
                normalize: None
            }
        );
        let flac = plan_master(&probe("h264", 1080, 16.0, "yuv420p", Some("flac"), 0), &m);
        assert_eq!(
            flac.audio,
            AudioPlan::Encode {
                kbit: 320,
                normalize: None
            }
        );
        let silent = plan_master(&probe("h264", 1080, 16.0, "yuv420p", None, 0), &m);
        assert_eq!(silent.audio, AudioPlan::None);
    }

    #[test]
    fn normalization_is_a_setting_that_forces_a_reencode() {
        let m = MediaConfig {
            normalize_audio: true,
            ..MediaConfig::default()
        };
        let p = plan_master(&probe("h264", 1080, 16.0, "yuv420p", Some("aac"), 320), &m);
        assert!(p.copies_video());
        assert_eq!(
            p.audio,
            AudioPlan::Encode {
                kbit: 320,
                normalize: Some(-16)
            }
        );
        assert!(p.reason.contains("normalized"), "{}", p.reason);
    }

    fn source(w: u32, h: u32, fps: f64, vbit_mbit: f64) -> SourceInfo {
        SourceInfo {
            width: w,
            height: h,
            fps,
            acodec: Some("aac".into()),
            abitrate: Some(320_000),
            vbitrate: Some((vbit_mbit * 1e6) as u64),
        }
    }

    fn rung(limit_mb: u64, seconds: f64, src: &SourceInfo) -> (u32, u32, bool, u32) {
        let p = copy_plan(
            limit_mb * 1_000_000,
            seconds,
            src,
            &MediaConfig::default(),
            1440,
            Purpose::Limited,
        )
        .unwrap_or_else(|e| panic!("{limit_mb} MB, {seconds} s: {e}"));
        let total = (p.vbit + p.audio.bits()) as f64 * seconds / 8.0;
        assert!(
            total <= limit_mb as f64 * 1e6,
            "{limit_mb} MB, {seconds} s: {} over the limit",
            p.reason
        );
        (
            p.height,
            p.fps.round() as u32,
            p.audio_lowered,
            (p.audio.bits() / 1000) as u32,
        )
    }

    #[test]
    fn the_ladder_keeps_the_audio_and_takes_the_first_rung_dense_enough() {
        let hd = source(1920, 1080, 60.0, 16.0);
        let qhd = source(2560, 1440, 60.0, 30.0);
        assert_eq!(rung(10, 30.0, &hd), (480, 60, false, 320));
        assert_eq!(rung(50, 30.0, &hd), (1080, 60, false, 320));
        assert_eq!(rung(500, 30.0, &hd), (1080, 60, false, 320));
        assert_eq!(rung(10, 120.0, &hd), (480, 30, true, 128));
        assert_eq!(rung(50, 120.0, &hd), (480, 60, false, 320));
        assert_eq!(rung(500, 120.0, &hd), (1080, 60, false, 320));
        assert_eq!(rung(10, 30.0, &qhd), (480, 60, false, 320));
        assert_eq!(rung(50, 30.0, &qhd), (1080, 60, false, 320));
        assert_eq!(rung(500, 30.0, &qhd), (1440, 60, false, 320));
        assert_eq!(rung(10, 120.0, &qhd), (480, 30, true, 128));
        assert_eq!(rung(50, 120.0, &qhd), (480, 60, false, 320));
        assert_eq!(rung(500, 120.0, &qhd), (1440, 60, false, 320));
    }

    #[test]
    fn the_video_never_takes_more_bits_than_the_source_has() {
        let hd = source(1920, 1080, 60.0, 16.0);
        let p = copy_plan(
            500_000_000,
            30.0,
            &hd,
            &MediaConfig::default(),
            1440,
            Purpose::Limited,
        )
        .unwrap();
        assert_eq!(p.vbit, 16_000_000);
        assert!(!p.scales && !p.drops_fps);
        assert_eq!(
            p.reason,
            "1080p60 at 16.0 Mbit/s, audio copied at 320 kbit/s"
        );
    }

    #[test]
    fn a_lowered_audio_says_so_and_a_hopeless_copy_is_refused() {
        let hd = source(1920, 1080, 60.0, 16.0);
        let p = copy_plan(
            10_000_000,
            120.0,
            &hd,
            &MediaConfig::default(),
            1440,
            Purpose::Limited,
        )
        .unwrap();
        assert_eq!(p.audio, AudioChoice::Encode { kbit: 128 });
        assert!(
            p.reason
                .ends_with("; audio lowered to 128 kbit/s to fit 10 MB"),
            "{}",
            p.reason
        );
        let refused = copy_plan(
            1_000_000,
            30.0,
            &hd,
            &MediaConfig::default(),
            1440,
            Purpose::Limited,
        );
        assert!(
            refused
                .as_ref()
                .unwrap_err()
                .contains("does not fit under 1 MB"),
            "{refused:?}"
        );
        let embed = copy_plan(
            1_000_000,
            30.0,
            &hd,
            &MediaConfig::default(),
            1080,
            Purpose::Embed,
        )
        .unwrap();
        assert_eq!((embed.height, embed.fps.round() as u32), (480, 30));
        assert!(embed.audio_lowered);
    }

    #[test]
    fn the_audio_setting_copies_or_fixes_the_rate() {
        let mut src = source(1920, 1080, 60.0, 16.0);
        let m = MediaConfig::default();
        assert_eq!(copy_audio(&src, &m), AudioChoice::Copy { bits: 320_000 });
        src.acodec = Some("opus".into());
        src.abitrate = Some(128_000);
        assert_eq!(copy_audio(&src, &m), AudioChoice::Encode { kbit: 128 });
        let fixed = MediaConfig {
            copy_audio: "192".into(),
            ..MediaConfig::default()
        };
        src.acodec = Some("aac".into());
        assert_eq!(copy_audio(&src, &fixed), AudioChoice::Encode { kbit: 192 });
        src.acodec = None;
        assert_eq!(copy_audio(&src, &m), AudioChoice::None);
        let embed = copy_plan(
            85_000_000,
            120.0,
            &source(2560, 1440, 60.0, 30.0),
            &m,
            1080,
            Purpose::Embed,
        )
        .unwrap();
        assert!(embed.height <= 1080);
    }

    #[test]
    fn the_embed_copy_takes_720p60_where_a_limited_copy_takes_480p60() {
        let hd = source(1920, 1080, 60.0, 16.0);
        let m = MediaConfig::default();
        let embed = copy_plan(20_000_000, 35.0, &hd, &m, 1080, Purpose::Embed).unwrap();
        assert_eq!((embed.height, embed.fps.round() as u32), (720, 60));
        assert!(
            embed
                .reason
                .starts_with("720p60 at 3.9 Mbit/s, audio copied"),
            "{}",
            embed.reason
        );
        let limited = copy_plan(20_000_000, 35.0, &hd, &m, 1440, Purpose::Limited).unwrap();
        assert_eq!((limited.height, limited.fps.round() as u32), (480, 60));
        let long = copy_plan(20_000_000, 60.0, &hd, &m, 1080, Purpose::Embed).unwrap();
        assert_eq!((long.height, long.fps.round() as u32), (480, 60));
        let short = copy_plan(20_000_000, 15.0, &hd, &m, 1080, Purpose::Embed).unwrap();
        assert_eq!((short.height, short.fps.round() as u32), (1080, 60));
    }
}
