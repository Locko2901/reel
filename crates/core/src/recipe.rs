use serde::{Deserialize, Serialize};

/// dB, in steps of 0.5.
pub const GAIN_MIN: f64 = -20.0;
pub const GAIN_MAX: f64 = 20.0;
/// A quick trim starts on the keyframe at or before its time when one is this close, so
/// it is a stream copy; OBS writes one every 2 s.
pub const KEEP_SNAP: f64 = 2.5;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CutRecipe {
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
    #[serde(default)]
    pub mute: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gain_db: f64,
    /// Normalised to this integrated loudness (LUFS), two-pass; the gain is then unused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loudness: Option<f64>,
    /// One entry per audio track of the original, mixed into the version's one track; None
    /// plays the first track alone, as players do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracks: Option<Vec<TrackEdit>>,
    /// The picture reframed to a window of it; none is the whole picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reframe: Option<Reframe>,
    /// Seconds the cut fades in from black and silence at its start.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fade_in: f64,
    /// Seconds it fades out to black and silence at its end.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fade_out: f64,
}

/// A window of the picture, kept at its own pixels: the cut is that window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Reframe {
    pub shape: ReframeShape,
    /// Where the window sits across the picture: 0 at its left edge, 1 at its right (top to
    /// bottom for a picture narrower than the window's shape).
    #[serde(default = "half")]
    pub x: f64,
}

fn half() -> f64 {
    0.5
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReframeShape {
    /// 9:16, a phone's short.
    Tall,
    Square,
}

impl ReframeShape {
    pub fn aspect(self) -> (u32, u32) {
        match self {
            Self::Tall => (9, 16),
            Self::Square => (1, 1),
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Self::Tall => "tall",
            Self::Square => "square",
        }
    }
}

pub const FADE_MAX: f64 = 5.0;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackEdit {
    #[serde(default)]
    pub on: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gain_db: f64,
}

/// At most this many audio tracks are mixed; OBS records six.
pub const TRACKS_MAX: usize = 16;

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl CutRecipe {
    pub fn trimmed(&self) -> bool {
        self.start.map(|s| s > 0.0).unwrap_or(false) || self.end.is_some()
    }

    /// The tracks the version plays, by index, with their gains.
    pub fn active_tracks(&self) -> Vec<(usize, f64)> {
        if self.mute {
            return Vec::new();
        }
        match &self.tracks {
            None => vec![(0, 0.0)],
            Some(t) => t
                .iter()
                .enumerate()
                .filter(|(_, e)| e.on)
                .map(|(i, e)| (i, e.gain_db))
                .collect(),
        }
    }

    pub fn audio_changed(&self) -> bool {
        !self.mute
            && (self.gain_db != 0.0
                || self.loudness.is_some()
                || self.active_tracks() != [(0, 0.0)])
    }

    pub fn is_noop(&self) -> bool {
        !self.trimmed() && !self.mute && !self.audio_changed() && !self.pictured()
    }

    pub fn fades(&self) -> bool {
        self.fade_in > 0.0 || self.fade_out > 0.0
    }

    /// The picture changes, reframed or faded: it is encoded again, never copied.
    pub fn pictured(&self) -> bool {
        self.reframe.is_some() || self.fades()
    }

    /// The fades and the window as they may be: each fade 0 to `FADE_MAX` seconds, both together
    /// within a cut this long, the window inside the picture.
    pub fn check_picture(&mut self, len: f64) -> Result<(), String> {
        for f in [&mut self.fade_in, &mut self.fade_out] {
            if !f.is_finite() || *f < 0.0 || *f > FADE_MAX {
                return Err(format!("a fade lasts up to {FADE_MAX} s"));
            }
            // ffmpeg reads a zero length as its own default (about a second): a hair is none
            if *f < 0.05 {
                *f = 0.0;
            }
        }
        if self.fade_in + self.fade_out > len {
            return Err("the fades are longer than the cut".into());
        }
        if let Some(r) = self.reframe.as_mut() {
            r.x = if r.x.is_finite() {
                r.x.clamp(0.0, 1.0)
            } else {
                0.5
            };
        }
        Ok(())
    }

    /// Mute wins over the gain, the loudness and the tracks, the loudness over the gain; no
    /// track on is mute, and the first track alone at 0 dB is no track choice at all.
    pub fn settled(mut self) -> Self {
        if !self.mute && self.tracks.is_some() {
            let active = self.active_tracks();
            if active.is_empty() {
                self.mute = true;
            } else if active == [(0, 0.0)] {
                self.tracks = None;
            }
        }
        if self.mute {
            self.gain_db = 0.0;
            self.loudness = None;
            self.tracks = None;
        }
        if self.loudness.is_some() {
            self.gain_db = 0.0;
        }
        self
    }

    pub fn check_gain(db: f64) -> Result<f64, String> {
        if !db.is_finite() || !(GAIN_MIN..=GAIN_MAX).contains(&db) {
            return Err(format!("the volume goes from {GAIN_MIN} to +{GAIN_MAX} dB"));
        }
        let stepped = (db * 2.0).round() / 2.0;
        if (stepped - db).abs() > 1e-6 {
            return Err("the volume moves in steps of 0.5 dB".into());
        }
        Ok(stepped)
    }

    /// The last `seconds` of a clip: None when the clip is not longer than that.
    pub fn keep_last(duration: f64, keyframes: &[f64], seconds: f64) -> Option<CutRecipe> {
        let from = duration - seconds;
        if seconds <= 0.0 || from < 0.5 {
            return None;
        }
        let start = keyframes
            .iter()
            .copied()
            .filter(|k| *k <= from + 0.001 && *k >= from - KEEP_SNAP && *k > 0.0)
            .fold(None, |best: Option<f64>, k| {
                Some(best.map_or(k, |b| b.max(k)))
            })
            .unwrap_or(from);
        Some(CutRecipe {
            start: Some(start),
            ..Default::default()
        })
    }

    /// What a rendered cut is, from its own length: a cut without an end ran to the end of
    /// its source, which is its start plus its length.
    pub fn describe_cut(&self, cut_len: f64) -> String {
        self.describe(self.start.unwrap_or(0.0) + cut_len)
    }

    pub fn describe(&self, duration: f64) -> String {
        let mut parts = Vec::new();
        if self.start.is_some() || self.end.is_some() {
            let s = self.start.unwrap_or(0.0);
            let e = self.end.unwrap_or(duration);
            parts.push(format!(
                "{} to {}",
                crate::format::duration_ms(s),
                crate::format::duration_ms(e)
            ));
        }
        if !self.mute && self.tracks.is_some() {
            let tracks: Vec<String> = self
                .active_tracks()
                .iter()
                .map(|(i, g)| {
                    if *g == 0.0 {
                        format!("track {}", i + 1)
                    } else {
                        format!("track {} {}", i + 1, crate::format::gain(*g))
                    }
                })
                .collect();
            parts.push(tracks.join(" and "));
        }
        if self.mute {
            parts.push("muted".into());
        } else if let Some(t) = self.loudness {
            parts.push(format!("loudness to {t:.0} LUFS"));
        } else if self.gain_db != 0.0 {
            parts.push(crate::format::gain(self.gain_db));
        }
        if parts.is_empty() {
            "unchanged".into()
        } else {
            parts.join(", ")
        }
    }
}

/// A size-limited copy: the limit and the rules it was made under, so a copy made under
/// older rules is not found and gets remade on the next request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscordRecipe {
    pub limit_bytes: u64,
    #[serde(default)]
    pub audio: String,
    #[serde(default)]
    pub preset: String,
    #[serde(default)]
    pub ladder: u32,
}

impl DiscordRecipe {
    pub fn for_limit(limit_bytes: u64, m: &crate::config::MediaConfig) -> Self {
        DiscordRecipe {
            limit_bytes,
            audio: m.copy_audio.clone(),
            preset: m.x264_preset.clone(),
            ladder: crate::plan::LADDER_VERSION,
        }
    }
    /// The text the rendition row keeps, and the key a cached copy is found by.
    pub fn json(&self) -> String {
        serde_json::to_string(self).expect("a recipe serialises")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnimRecipe {
    pub format: AnimFormat,
    pub width: u32,
    pub fps: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnimFormat {
    Gif,
    Webp,
}

impl AnimFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "gif" => Some(AnimFormat::Gif),
            "webp" => Some(AnimFormat::Webp),
            _ => None,
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            AnimFormat::Gif => "gif",
            AnimFormat::Webp => "webp",
        }
    }
    pub fn mime(self) -> &'static str {
        match self {
            AnimFormat::Gif => "image/gif",
            AnimFormat::Webp => "image/webp",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            AnimFormat::Gif => "GIF",
            AnimFormat::Webp => "WebP",
        }
    }
    /// Bytes per pixel of a frame, calm clip to busy, measured with `args::anim`'s settings.
    fn bytes_per_pixel(self) -> (f64, f64) {
        match self {
            AnimFormat::Gif => (0.12, 0.55),
            AnimFormat::Webp => (0.01, 0.02),
        }
    }
}

pub const ANIM_MAX_SECONDS: f64 = 15.0;
pub const ANIM_WIDTHS: [u32; 2] = [480, 720];
pub const ANIM_FPS: [u32; 2] = [12, 24];

impl AnimRecipe {
    pub fn new(format: AnimFormat, width: u32, fps: u32) -> Option<Self> {
        (ANIM_WIDTHS.contains(&width) && ANIM_FPS.contains(&fps)).then_some(AnimRecipe {
            format,
            width,
            fps,
        })
    }
    pub fn json(&self) -> String {
        serde_json::to_string(self).expect("a recipe serialises")
    }
    pub fn size(&self, width: u32, height: u32) -> (u32, u32) {
        let w = self.width.min(width.max(2));
        let h = if width == 0 {
            w * 9 / 16
        } else {
            (f64::from(w) * f64::from(height) / f64::from(width)).round() as u32
        };
        (w, (h.max(2)).div_ceil(2) * 2)
    }
    /// What it will weigh, from a calm picture to a busy one, in bytes.
    pub fn estimate(&self, width: u32, height: u32, seconds: f64) -> (u64, u64) {
        let (w, h) = self.size(width, height);
        let pixels = f64::from(w) * f64::from(h) * f64::from(self.fps) * seconds.max(0.0);
        let (low, high) = self.format.bytes_per_pixel();
        ((pixels * low) as u64, (pixels * high) as u64)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MontageRecipe {
    pub parts: Vec<MontagePart>,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    #[serde(default)]
    pub join: MontageJoin,
    #[serde(default)]
    pub fit: MontageFit,
    /// Every clip's sound brought to one loudness; off, each keeps its own.
    #[serde(default = "yes")]
    pub even_loudness: bool,
}

fn yes() -> bool {
    true
}

fn one() -> f64 {
    1.0
}

fn is_one(v: &f64) -> bool {
    (*v - 1.0).abs() < 1e-9
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MontagePart {
    pub clip: String,
    pub rendition: String,
    /// Where the part starts in the version; none is its start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    /// Where it ends; none is the version's end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    /// Its sound: 1 as it is, 0 silent, up to 2.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub volume: f64,
}

impl MontagePart {
    pub fn whole(clip: String, rendition: String) -> Self {
        Self {
            clip,
            rendition,
            start: None,
            end: None,
            volume: 1.0,
        }
    }

    /// The seconds it runs, of a version this long.
    pub fn length(&self, duration: f64) -> f64 {
        let start = self.start.unwrap_or(0.0).clamp(0.0, duration);
        let end = self.end.unwrap_or(duration).clamp(start, duration);
        end - start
    }
}

/// How one clip of a montage gives way to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JoinKind {
    #[default]
    Cut,
    /// The two pictures and sounds cross for `seconds`.
    Fade,
    /// The picture goes through black, the sound crosses.
    Black,
}

impl JoinKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cut" => Some(Self::Cut),
            "fade" => Some(Self::Fade),
            "black" => Some(Self::Black),
            _ => None,
        }
    }

    /// ffmpeg's xfade transition for it; none for a cut.
    pub fn xfade(self) -> Option<&'static str> {
        match self {
            Self::Cut => None,
            Self::Fade => Some("fade"),
            Self::Black => Some("fadeblack"),
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Self::Cut => "hard cuts",
            Self::Fade => "crossfades",
            Self::Black => "fades through black",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct MontageJoin {
    pub kind: JoinKind,
    /// How long each join takes; 0 for a cut.
    #[serde(default)]
    pub seconds: f64,
}

impl MontageJoin {
    /// The seconds each join overlaps two clips: none for a cut.
    pub fn overlap(&self) -> f64 {
        if self.kind == JoinKind::Cut {
            0.0
        } else {
            self.seconds.max(0.0)
        }
    }
}

/// The shortest and longest join, in seconds.
pub const JOIN_SECONDS: (f64, f64) = (0.2, 2.0);
/// Seconds.
pub const PART_SHORTEST: f64 = 0.5;

/// How a clip of another shape fills the montage's frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MontageFit {
    /// All of it, with bars where its shape differs.
    #[default]
    Bars,
    /// The frame filled, what is over cut off at the sides or top and bottom.
    Fill,
}

/// The montage's frame: wide 16:9, tall 9:16 for a phone's short, or square.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MontageShape {
    #[default]
    Wide,
    Tall,
    Square,
}

impl MontageShape {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "wide" => Some(Self::Wide),
            "tall" => Some(Self::Tall),
            "square" => Some(Self::Square),
            _ => None,
        }
    }

    pub fn of(width: u32, height: u32) -> Self {
        match width.cmp(&height) {
            std::cmp::Ordering::Greater => Self::Wide,
            std::cmp::Ordering::Less => Self::Tall,
            std::cmp::Ordering::Equal => Self::Square,
        }
    }
}

pub const MONTAGE_MOST: usize = 100;

impl MontageRecipe {
    pub fn json(&self) -> String {
        serde_json::to_string(self).expect("a recipe serialises")
    }
}

/// A montage's length from its parts' lengths: each join's overlap taken off.
pub fn montage_length(lengths: &[f64], join: &MontageJoin) -> f64 {
    let joins = lengths.len().saturating_sub(1) as f64;
    (lengths.iter().sum::<f64>() - joins * join.overlap()).max(0.0)
}

/// A montage's frame for these versions (width, height, rate) in this shape: the short side the
/// tallest version's, 360 to 1080, the long side at 16:9 to it; the highest rate, 24 to 60.
pub fn montage_frame(parts: &[(u32, u32, f64)], shape: MontageShape) -> (u32, u32, u32) {
    let tallest = parts
        .iter()
        .map(|&(w, h, _)| if w >= h { h } else { w })
        .max()
        .unwrap_or(1080);
    let short = tallest.clamp(360, 1080) / 2 * 2;
    let long = (short * 16 / 9).div_ceil(2) * 2;
    let (width, height) = match shape {
        MontageShape::Wide => (long, short),
        MontageShape::Tall => (short, long),
        MontageShape::Square => (short, short),
    };
    let fps = parts
        .iter()
        .map(|p| p.2.round() as u32)
        .max()
        .unwrap_or(60)
        .clamp(24, 60);
    (width, height, fps)
}

/// The embed copy's rules; an embed copy with another recipe is kept and served until the
/// Storage action remakes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbedRecipe {
    pub audio: String,
    pub ladder: u32,
    #[serde(default)]
    pub size_mb: u64,
    #[serde(default)]
    pub target: u32,
}

impl EmbedRecipe {
    pub fn current(cfg: &crate::Config) -> Self {
        Self::for_size(cfg, cfg.share.embed_mb)
    }
    /// The rules of a preview at another size: the external storage's.
    pub fn for_size(cfg: &crate::Config, size_mb: u64) -> Self {
        EmbedRecipe {
            audio: cfg.media.copy_audio.clone(),
            ladder: crate::plan::LADDER_VERSION,
            size_mb,
            target: (crate::plan::BPP_TARGET_EMBED * 100.0).round() as u32,
        }
    }
    pub fn json(&self) -> String {
        serde_json::to_string(self).expect("a recipe serialises")
    }
}

/// The web copy a link's page plays: remade when the rate, the height or the audio rule changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebRecipe {
    pub mbit: u64,
    pub max_height: u32,
    pub audio: String,
}

impl WebRecipe {
    pub fn current(cfg: &crate::Config) -> Self {
        WebRecipe {
            mbit: cfg.share.web_mbit,
            max_height: cfg.share.web_max_height,
            audio: cfg.media.copy_audio.clone(),
        }
    }
    pub fn json(&self) -> String {
        serde_json::to_string(self).expect("a recipe serialises")
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_reframed_or_faded_cut_changes_its_picture() {
        use super::*;
        let mut r: CutRecipe = serde_json::from_str(
            r#"{"reframe":{"shape":"tall","x":1.4},"fade_in":0.5,"fade_out":1}"#,
        )
        .unwrap();
        assert!(r.pictured() && r.fades() && !r.is_noop());
        assert!(r.check_picture(10.0).is_ok());
        assert_eq!(
            r.reframe.unwrap().x,
            1.0,
            "the window stays inside the picture"
        );
        assert_eq!(r.reframe.unwrap().shape.aspect(), (9, 16));
        assert!(r.check_picture(1.2).is_err(), "fades longer than the cut");
        r.fade_in = 9.0;
        assert!(r.check_picture(60.0).is_err());
        r.fade_in = 0.004;
        assert!(r.check_picture(60.0).is_ok());
        assert_eq!(r.fade_in, 0.0, "a hair of a fade is none");
        let plain = CutRecipe::default();
        assert!(!plain.pictured());
        assert_eq!(
            serde_json::to_string(&plain).unwrap(),
            r#"{"start":null,"end":null,"mute":false}"#
        );
    }

    #[test]
    fn an_animated_picture_takes_two_widths_two_rates_and_says_its_weight() {
        assert!(AnimRecipe::new(AnimFormat::Gif, 480, 12).is_some());
        assert!(AnimRecipe::new(AnimFormat::Webp, 720, 24).is_some());
        assert!(AnimRecipe::new(AnimFormat::Gif, 1080, 12).is_none());
        assert!(AnimRecipe::new(AnimFormat::Gif, 480, 30).is_none());
        let gif = AnimRecipe::new(AnimFormat::Gif, 480, 12).unwrap();
        assert_eq!(gif.size(1920, 1080), (480, 270));
        assert_eq!(gif.size(1080, 1920), (480, 854));
        assert_eq!(
            gif.size(320, 180),
            (320, 180),
            "never wider than the version"
        );
        let (low, high) = gif.estimate(1920, 1080, 10.0);
        assert_eq!(
            (low / 100_000, high / 100_000),
            (18, 85),
            "about 1.8 to 8.5 MB"
        );
        let webp = AnimRecipe::new(AnimFormat::Webp, 480, 12).unwrap();
        assert!(webp.estimate(1920, 1080, 10.0).1 < gif.estimate(1920, 1080, 10.0).0);
        assert_eq!(gif.json(), r#"{"format":"gif","width":480,"fps":12}"#);
        assert_eq!(AnimFormat::parse("webp"), Some(AnimFormat::Webp));
        assert_eq!(AnimFormat::parse("apng"), None);
    }

    fn montage_frame_wide(parts: &[(u32, u32, f64)]) -> (u32, u32, u32) {
        montage_frame(parts, MontageShape::Wide)
    }

    #[test]
    fn a_montage_comes_tall_or_square_too() {
        let parts = [(2560, 1440, 60.0), (1920, 1080, 30.0)];
        assert_eq!(montage_frame(&parts, MontageShape::Tall), (1080, 1920, 60));
        assert_eq!(
            montage_frame(&parts, MontageShape::Square),
            (1080, 1080, 60)
        );
        assert_eq!(MontageShape::of(1080, 1920), MontageShape::Tall);
    }

    #[test]
    fn a_recipe_from_before_the_editor_reads_as_whole_clips_and_cuts() {
        let r: MontageRecipe = serde_json::from_str(
            r#"{"parts":[{"clip":"a","rendition":"ra"},{"clip":"b","rendition":"rb"}],"width":1920,"height":1080,"fps":60}"#,
        )
        .unwrap();
        assert_eq!(r.parts[0], MontagePart::whole("a".into(), "ra".into()));
        assert_eq!(r.join, MontageJoin::default());
        assert_eq!((r.fit, r.even_loudness), (MontageFit::Bars, true));
        let mut p = MontagePart::whole("a".into(), "ra".into());
        assert_eq!(
            serde_json::to_string(&p).unwrap(),
            r#"{"clip":"a","rendition":"ra"}"#
        );
        p.start = Some(2.0);
        p.end = Some(5.5);
        p.volume = 0.5;
        assert_eq!(p.length(10.0), 3.5);
        assert_eq!(p.length(4.0), 2.0, "an end past the version's is its end");
    }

    #[test]
    fn fades_take_their_overlap_off_the_length() {
        let cut = MontageJoin::default();
        let fade = MontageJoin {
            kind: JoinKind::Fade,
            seconds: 0.5,
        };
        assert_eq!(montage_length(&[10.0, 20.0, 5.0], &cut), 35.0);
        assert_eq!(montage_length(&[10.0, 20.0, 5.0], &fade), 34.0);
        assert_eq!(JoinKind::Black.xfade(), Some("fadeblack"));
    }

    #[test]
    fn a_montage_takes_the_tallest_frame_and_the_highest_rate_within_1080p60() {
        assert_eq!(
            montage_frame_wide(&[(1280, 720, 30.0), (1920, 1080, 60.0)]),
            (1920, 1080, 60)
        );
        assert_eq!(
            montage_frame_wide(&[(1280, 720, 30.0), (640, 360, 30.0)]),
            (1280, 720, 30)
        );
        assert_eq!(
            montage_frame_wide(&[(2560, 1440, 144.0)]),
            (1920, 1080, 60),
            "no more than 1080p60"
        );
        assert_eq!(
            montage_frame_wide(&[(1080, 1920, 30.0), (1280, 720, 30.0)]),
            (1920, 1080, 30),
            "a tall video counts by its width: it is pillarboxed in a 16:9 frame"
        );
        assert_eq!(
            montage_frame_wide(&[(320, 180, 15.0)]),
            (640, 360, 24),
            "at least 360p24"
        );
    }

    use super::*;

    #[test]
    fn a_cut_of_the_last_seconds_runs_to_its_source_end() {
        let last = CutRecipe {
            start: Some(44.0),
            ..Default::default()
        };
        assert_eq!(last.describe_cut(31.0), "0:44.00 to 1:15.00");
    }

    #[test]
    fn a_gain_or_a_loudness_is_a_change_and_mute_wins() {
        let gain = CutRecipe {
            gain_db: 6.0,
            ..Default::default()
        };
        assert!(gain.audio_changed() && !gain.is_noop() && !gain.trimmed());
        assert_eq!(gain.describe(10.0), "+6 dB");
        let both = CutRecipe {
            mute: true,
            gain_db: 6.0,
            loudness: Some(-16.0),
            ..Default::default()
        }
        .settled();
        assert_eq!((both.gain_db, both.loudness), (0.0, None));
        assert!(!both.audio_changed() && !both.is_noop());
        assert_eq!(both.describe(10.0), "muted");
        let loud = CutRecipe {
            gain_db: 3.0,
            loudness: Some(-16.0),
            ..Default::default()
        }
        .settled();
        assert_eq!(loud.gain_db, 0.0);
        assert_eq!(loud.describe(10.0), "loudness to -16 LUFS");
        assert!(CutRecipe::default().is_noop());
    }

    #[test]
    fn old_recipes_read_as_before_and_a_plain_cut_writes_as_before() {
        let old: CutRecipe =
            serde_json::from_str(r#"{"start":1.5,"end":null,"mute":false}"#).unwrap();
        assert_eq!((old.gain_db, old.loudness), (0.0, None));
        let plain = CutRecipe {
            start: Some(1.5),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&plain).unwrap(),
            r#"{"start":1.5,"end":null,"mute":false}"#
        );
    }

    #[test]
    fn keep_last_starts_on_the_keyframe_before_when_it_is_close() {
        let kf: Vec<f64> = (0..90).map(|i| i as f64 * 2.0).collect();
        let r = CutRecipe::keep_last(180.0, &kf, 30.0).unwrap();
        assert_eq!((r.start, r.end), (Some(150.0), None));
        let r = CutRecipe::keep_last(179.3, &kf, 15.0).unwrap();
        assert_eq!(r.start, Some(164.0), "the keyframe before 164.3");
        let sparse = [0.0, 10.0, 20.0];
        assert_eq!(
            CutRecipe::keep_last(25.0, &sparse, 9.0).unwrap().start,
            Some(16.0),
            "no keyframe close enough: the exact time, re-encoded"
        );
        assert_eq!(
            CutRecipe::keep_last(25.0, &[], 9.0).unwrap().start,
            Some(16.0)
        );
        assert!(
            CutRecipe::keep_last(20.0, &kf, 30.0).is_none(),
            "shorter than asked"
        );
        assert!(
            CutRecipe::keep_last(30.2, &kf, 30.0).is_none(),
            "nothing worth cutting"
        );
        assert!(CutRecipe::keep_last(60.0, &kf, 0.0).is_none());
    }

    #[test]
    fn tracks_mix_as_set_and_the_first_alone_is_the_default() {
        let t = |on: bool, g: f64| TrackEdit { on, gain_db: g };
        let mix = CutRecipe {
            tracks: Some(vec![t(true, 0.0), t(false, 0.0), t(true, -6.0)]),
            ..Default::default()
        }
        .settled();
        assert_eq!(mix.active_tracks(), vec![(0, 0.0), (2, -6.0)]);
        assert!(mix.audio_changed() && !mix.is_noop());
        assert_eq!(mix.describe(10.0), "track 1 and track 3 -6 dB");
        let first = CutRecipe {
            tracks: Some(vec![t(true, 0.0), t(false, 3.0)]),
            ..Default::default()
        }
        .settled();
        assert_eq!(first.tracks, None, "the first track alone is the default");
        assert!(first.is_noop());
        let none = CutRecipe {
            tracks: Some(vec![t(false, 0.0), t(false, 0.0)]),
            ..Default::default()
        }
        .settled();
        assert!(none.mute && none.tracks.is_none(), "no track on is mute");
        let second = CutRecipe {
            tracks: Some(vec![t(false, 0.0), t(true, 0.0)]),
            ..Default::default()
        }
        .settled();
        assert_eq!(second.active_tracks(), vec![(1, 0.0)]);
        assert!(second.audio_changed());
        let json = serde_json::to_string(&mix).unwrap();
        assert!(
            json.contains(r#""tracks":[{"on":true},{"on":false},{"on":true,"gain_db":-6.0}]"#),
            "{json}"
        );
    }

    #[test]
    fn the_gain_is_checked_to_its_range_and_steps() {
        assert_eq!(CutRecipe::check_gain(-20.0), Ok(-20.0));
        assert_eq!(CutRecipe::check_gain(6.5), Ok(6.5));
        assert!(CutRecipe::check_gain(20.5).is_err());
        assert!(CutRecipe::check_gain(0.3).is_err());
        assert!(CutRecipe::check_gain(f64::NAN).is_err());
    }
}
