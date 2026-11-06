use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProbeInfo {
    pub duration: f64,
    pub size: u64,
    pub format_name: String,
    pub width: u32,
    pub height: u32,
    pub vcodec: Option<String>,
    pub profile: Option<String>,
    pub pix_fmt: Option<String>,
    pub fps: f64,
    pub vbitrate: Option<u64>,
    pub acodec: Option<String>,
    pub abitrate: Option<u64>,
    pub creation_time: Option<String>,
    #[serde(default)]
    pub audio_streams: u32,
    /// Each audio track's title, empty when it has none worth showing.
    #[serde(default)]
    pub audio_titles: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("ffprobe output is not JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no video stream")]
    NoVideo,
}

fn parse_fps(s: &str) -> f64 {
    if let Some((a, b)) = s.split_once('/') {
        let a: f64 = a.parse().unwrap_or(0.0);
        let b: f64 = b.parse().unwrap_or(1.0);
        if b > 0.0 {
            a / b
        } else {
            0.0
        }
    } else {
        s.parse().unwrap_or(0.0)
    }
}

fn num<T: std::str::FromStr>(v: &serde_json::Value) -> Option<T> {
    match v {
        serde_json::Value::String(s) => s.parse().ok(),
        serde_json::Value::Number(n) => n.to_string().parse().ok(),
        _ => None,
    }
}

impl ProbeInfo {
    pub fn from_ffprobe_json(text: &str) -> Result<Self, ProbeError> {
        let v: serde_json::Value = serde_json::from_str(text)?;
        let streams = v
            .get("streams")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();
        let video = streams
            .iter()
            .find(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("video"))
            .ok_or(ProbeError::NoVideo)?;
        let audio = streams
            .iter()
            .find(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("audio"));
        let format = v.get("format").cloned().unwrap_or_default();
        let format_bitrate: Option<u64> = format.get("bit_rate").and_then(num);
        let duration = video
            .get("duration")
            .and_then(num::<f64>)
            .or_else(|| format.get("duration").and_then(num::<f64>))
            .unwrap_or(0.0);
        let fps = video
            .get("avg_frame_rate")
            .and_then(|r| r.as_str())
            .map(parse_fps)
            .filter(|f| *f > 0.0)
            .or_else(|| {
                video
                    .get("r_frame_rate")
                    .and_then(|r| r.as_str())
                    .map(parse_fps)
            })
            .unwrap_or(0.0);
        let vbitrate = video.get("bit_rate").and_then(num::<u64>).or_else(|| {
            let abit: u64 = audio
                .and_then(|a| a.get("bit_rate"))
                .and_then(num)
                .unwrap_or(0);
            format_bitrate.map(|b| b.saturating_sub(abit))
        });
        let tags = format.get("tags").cloned().unwrap_or_default();
        let creation_time = tags
            .get("creation_time")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string());
        Ok(Self {
            duration,
            size: format.get("size").and_then(num).unwrap_or(0),
            format_name: format
                .get("format_name")
                .and_then(|f| f.as_str())
                .unwrap_or("")
                .to_string(),
            width: video.get("width").and_then(num).unwrap_or(0),
            height: video.get("height").and_then(num).unwrap_or(0),
            vcodec: video
                .get("codec_name")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string()),
            profile: video
                .get("profile")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string()),
            pix_fmt: video
                .get("pix_fmt")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string()),
            fps,
            vbitrate,
            acodec: audio
                .and_then(|a| a.get("codec_name"))
                .and_then(|c| c.as_str())
                .map(|s| s.to_string()),
            abitrate: audio.and_then(|a| a.get("bit_rate")).and_then(num),
            creation_time,
            audio_streams: streams
                .iter()
                .filter(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("audio"))
                .count() as u32,
            audio_titles: streams
                .iter()
                .filter(|s| s.get("codec_type").and_then(|t| t.as_str()) == Some("audio"))
                .map(|s| {
                    let tag = |k: &str| {
                        s.get("tags")
                            .and_then(|t| t.get(k))
                            .and_then(|v| v.as_str())
                            .map(|v| v.trim().to_string())
                            .unwrap_or_default()
                    };
                    let handler = tag("handler_name");
                    let generic = handler.is_empty()
                        || handler.to_ascii_lowercase().starts_with("sound")
                        || handler.to_ascii_lowercase().starts_with("core media");
                    track_title(&match tag("title") {
                        t if !t.is_empty() => t,
                        _ if !generic => handler,
                        _ => String::new(),
                    })
                })
                .collect(),
        })
    }

    pub fn has_audio(&self) -> bool {
        self.acodec.is_some()
    }
    pub fn is_h264(&self) -> bool {
        self.vcodec.as_deref() == Some("h264")
    }
    pub fn vbitrate_mbit(&self) -> f64 {
        self.vbitrate.unwrap_or(0) as f64 / 1_000_000.0
    }
}

/// Whether a file's tag says where it was made or what made it: a place, the device's make or
/// model, a person's name. reel's own files carry none of these.
pub fn personal_tag(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("location")
        || k.contains("gps")
        || k.contains("iso6709")
        || k.starts_with("com.android.")
        || matches!(
            k.as_str(),
            "make" | "model" | "artist" | "album_artist" | "author" | "composer"
        )
        || (k.starts_with("com.apple.quicktime.")
            && [
                "make",
                "model",
                "software",
                "author",
                "artist",
                "displayname",
            ]
            .iter()
            .any(|w| k.ends_with(w)))
}

/// The most characters a track's name keeps.
pub const TRACK_TITLE_MAX: usize = 40;

/// A track's name as reel keeps and shows it, from a file or a device: no control characters,
/// trimmed, at most `TRACK_TITLE_MAX` characters.
pub fn track_title(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(TRACK_TITLE_MAX)
        .collect::<String>()
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_typical_ffprobe_document() {
        let json = r#"{"streams":[{"codec_type":"video","codec_name":"h264","profile":"High","width":1920,"height":1080,"pix_fmt":"yuv420p","avg_frame_rate":"60/1","bit_rate":"9600000","duration":"72.0"},{"codec_type":"audio","codec_name":"aac","bit_rate":"192000"}],"format":{"format_name":"matroska,webm","duration":"72.1","size":"86100000","bit_rate":"9800000","tags":{"creation_time":"2026-10-06T19:14:03.000000Z"}}}"#;
        let p = ProbeInfo::from_ffprobe_json(json).unwrap();
        assert_eq!(p.width, 1920);
        assert_eq!(p.fps, 60.0);
        assert_eq!(p.vbitrate, Some(9_600_000));
        assert!(p.is_h264());
        assert!(p.has_audio());
        assert_eq!(p.audio_streams, 1);
        assert_eq!(p.audio_titles, vec![String::new()]);
        let three = r#"{"streams":[{"codec_type":"video","codec_name":"h264"},{"codec_type":"audio","tags":{"title":"Game"}},{"codec_type":"audio","tags":{"handler_name":"SoundHandler"}},{"codec_type":"audio","tags":{"handler_name":"Discord"}}],"format":{}}"#;
        let p3 = ProbeInfo::from_ffprobe_json(three).unwrap();
        assert_eq!(p3.audio_titles, vec!["Game", "", "Discord"]);
        assert_eq!(
            p.creation_time.as_deref(),
            Some("2026-10-06T19:14:03.000000Z")
        );
    }

    #[test]
    fn a_place_or_a_device_is_personal_and_reels_own_tags_are_not() {
        for k in [
            "location",
            "location-eng",
            "com.apple.quicktime.location.ISO6709",
            "com.apple.quicktime.make",
            "com.apple.quicktime.model",
            "com.android.version",
            "make",
            "Model",
            "artist",
        ] {
            assert!(personal_tag(k), "{k}");
        }
        for k in [
            "encoder",
            "major_brand",
            "minor_version",
            "compatible_brands",
            "handler_name",
            "language",
            "creation_time",
            "title",
            "comment",
            "vendor_id",
            "DURATION",
        ] {
            assert!(!personal_tag(k), "{k}");
        }
    }

    #[test]
    fn a_track_title_is_kept_short_and_without_control_characters() {
        assert_eq!(track_title("  Game\u{0}\n "), "Game");
        assert_eq!(
            track_title(&"x".repeat(100)).chars().count(),
            TRACK_TITLE_MAX
        );
        let json = r#"{"streams":[{"codec_type":"video","codec_name":"h264"},{"codec_type":"audio","tags":{"title":"Mic\u0007 of a very long name that goes on and on and on"}}],"format":{}}"#;
        let p = ProbeInfo::from_ffprobe_json(json).unwrap();
        assert_eq!(
            p.audio_titles,
            vec!["Mic of a very long name that goes on and"]
        );
    }

    #[test]
    fn falls_back_to_format_bitrate_minus_audio() {
        let json = r#"{"streams":[{"codec_type":"video","codec_name":"hevc","width":2560,"height":1440,"avg_frame_rate":"0/0","r_frame_rate":"60/1"},{"codec_type":"audio","codec_name":"opus","bit_rate":"128000"}],"format":{"format_name":"matroska","duration":"30","size":"100","bit_rate":"20128000"}}"#;
        let p = ProbeInfo::from_ffprobe_json(json).unwrap();
        assert_eq!(p.vbitrate, Some(20_000_000));
        assert_eq!(p.fps, 60.0);
    }
}
