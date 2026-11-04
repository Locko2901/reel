use std::path::Path;

/// A synthetic clip spec for tests and the demo: testsrc2 video with a sine tone.
#[derive(Debug, Clone)]
pub struct Synth {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub seconds: f64,
    pub vcodec: String,
    pub vbit_kbit: u32,
    pub maxrate_kbit: Option<u32>,
    pub bframes: Option<u32>,
    pub audio: bool,
    pub abit_kbit: u32,
    pub label: String,
    /// The tone's level against lavfi's sine (-18 dBFS), in dB.
    pub tone_db: f64,
    pub audio_tracks: u32,
    /// Titles of the audio tracks, kept in an MKV.
    pub track_titles: Vec<String>,
}

impl Synth {
    fn base(
        width: u32,
        height: u32,
        fps: u32,
        seconds: f64,
        vcodec: &str,
        vbit_kbit: u32,
        label: &str,
    ) -> Self {
        Self {
            width,
            height,
            fps,
            seconds,
            vcodec: vcodec.into(),
            vbit_kbit,
            maxrate_kbit: None,
            bframes: None,
            audio: true,
            abit_kbit: 128,
            label: label.into(),
            tone_db: 0.0,
            audio_tracks: 1,
            track_titles: Vec::new(),
        }
    }
    pub fn h264_1080p(seconds: f64, label: &str) -> Self {
        Self::base(1920, 1080, 60, seconds, "libx264", 6000, label)
    }
    pub fn hevc_1440p(seconds: f64, label: &str) -> Self {
        Self::base(2560, 1440, 60, seconds, "libx265", 8000, label)
    }
    pub fn h264_1440p(seconds: f64, label: &str) -> Self {
        Self::base(2560, 1440, 60, seconds, "libx264", 20000, label)
    }
    pub fn h264_high_bitrate(seconds: f64, label: &str) -> Self {
        Self::base(1920, 1080, 60, seconds, "libx264", 20000, label)
    }
    pub fn small(seconds: f64, label: &str) -> Self {
        Self::base(640, 360, 30, seconds, "libx264", 800, label)
    }
    /// A common OBS recording profile: H.264 High 1080p60, VBR 16 Mbit/s up to 25, 2 s keyframes, 2 B-frames, AAC 320 kbit/s.
    pub fn obs(seconds: f64, label: &str) -> Self {
        Self {
            maxrate_kbit: Some(25000),
            bframes: Some(2),
            abit_kbit: 320,
            ..Self::base(1920, 1080, 60, seconds, "libx264", 16000, label)
        }
    }

    pub fn args(&self, output: &Path) -> Vec<String> {
        let s = |x: &str| x.to_string();
        let text = self.label.replace('\'', "").replace(':', "\\:");
        let mut a = vec![
            s("-f"),
            s("lavfi"),
            s("-i"),
            format!(
                "testsrc2=size={}x{}:rate={}",
                self.width, self.height, self.fps
            ),
        ];
        let tracks = if self.audio {
            self.audio_tracks.max(1)
        } else {
            0
        };
        for t in 0..tracks {
            a.extend([
                s("-f"),
                s("lavfi"),
                s("-i"),
                format!("sine=frequency={}:sample_rate=48000", 440 + 220 * t),
            ]);
        }
        if tracks > 1 {
            a.extend([s("-map"), s("0:v")]);
            for t in 1..=tracks {
                a.extend([s("-map"), format!("{t}:a")]);
            }
        }
        a.extend([s("-t"), format!("{:.2}", self.seconds)]);
        for (t, title) in self.track_titles.iter().enumerate() {
            a.extend([format!("-metadata:s:a:{t}"), format!("title={title}")]);
        }
        if self.audio && self.tone_db != 0.0 {
            a.extend([s("-af"), format!("volume={:.1}dB", self.tone_db)]);
        }
        a.extend([s("-vf"), format!("drawtext=text='{text} %{{pts\\:hms}}':fontcolor=white:fontsize=48:x=40:y=40:box=1:boxcolor=black@0.5")]);
        let maxrate = self.maxrate_kbit.unwrap_or(self.vbit_kbit);
        a.extend([
            s("-c:v"),
            self.vcodec.clone(),
            s("-b:v"),
            format!("{}k", self.vbit_kbit),
            s("-maxrate"),
            format!("{maxrate}k"),
            s("-bufsize"),
            format!("{}k", maxrate * 2),
            s("-pix_fmt"),
            s("yuv420p"),
            s("-g"),
            format!("{}", self.fps * 2),
        ]);
        if let Some(b) = self.bframes {
            a.extend([s("-bf"), b.to_string()]);
        }
        if self.vcodec == "libx264" {
            a.extend([s("-preset"), s("veryfast"), s("-profile:v"), s("high")]);
        }
        if self.vcodec == "libx265" {
            a.extend([
                s("-preset"),
                s("ultrafast"),
                s("-x265-params"),
                s("log-level=error"),
            ]);
        }
        if self.audio {
            a.extend([
                s("-c:a"),
                s("aac"),
                s("-b:a"),
                format!("{}k", self.abit_kbit),
            ]);
        }
        let name = output.to_string_lossy();
        if name.ends_with(".mkv") || name.ends_with(".mkv.part") {
            a.extend([s("-f"), s("matroska")]);
        } else {
            a.extend([s("-movflags"), s("+faststart"), s("-f"), s("mp4")]);
        }
        a.push(output.to_string_lossy().into_owned());
        a
    }
}
