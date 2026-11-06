use crate::hw::HwBackend;
use crate::level::Loudnorm;
use reel_core::plan::{AudioChoice, AudioPlan, CopyPlan, Plan, VideoPlan};
use reel_core::CutRecipe;
use std::path::Path;

fn s(x: impl AsRef<str>) -> String {
    x.as_ref().to_string()
}
fn p(x: &Path) -> String {
    x.to_string_lossy().into_owned()
}

/// The ingest's one-pass normalisation to the loudness target.
pub fn loudnorm_once(target: i32) -> String {
    format!("loudnorm=I={target}:TP=-1.5:LRA=11")
}

/// No boosted or normalised cut has a true peak above this (dBTP).
pub const PEAK_CEILING: f64 = -1.0;
/// What the limiter and loudnorm aim at first: half a dB under the ceiling, room for the AAC
/// encoder's own overshoot.
pub const FIRST_AIM: f64 = -1.5;
/// Wide, so loudnorm takes its linear mode (one gain) whenever the peaks allow it.
const LOUDNORM_LRA: f64 = 20.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Encoder {
    pub hw: HwBackend,
    pub device: String,
    /// ffmpeg runs on Windows, where every hardware encoder takes its frames from memory.
    pub windows: bool,
    pub mbit: u32,
    pub fps: f64,
    pub hw_decoder: Option<String>,
    pub source: (u32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Full,
    EncodeOnly,
    Software,
}

impl Encoder {
    pub fn route(&self) -> Route {
        match (self.hw, &self.hw_decoder) {
            (HwBackend::Software, _) => Route::Software,
            (_, Some(_)) if self.uploads() => Route::Full,
            _ => Route::EncodeOnly,
        }
    }

    pub fn software_decode(&self) -> Encoder {
        Encoder {
            hw_decoder: None,
            ..self.clone()
        }
    }

    pub fn software(&self) -> Encoder {
        Encoder {
            hw: HwBackend::Software,
            hw_decoder: None,
            ..self.clone()
        }
    }

    pub fn label(&self) -> String {
        match self.route() {
            Route::Software => "libx264".into(),
            Route::EncodeOnly => format!("{}, software decode", self.hw.name()),
            Route::Full => format!("{}, hardware decode", self.hw.name()),
        }
    }

    fn uploads(&self) -> bool {
        !self.hw.takes_system_frames(self.windows)
    }
}

/// The decoder of the full hardware route, for a backend that keeps its frames on its device.
pub fn hw_decoder(backend: HwBackend, vcodec: Option<&str>, windows: bool) -> Option<String> {
    if backend.takes_system_frames(windows) {
        return None;
    }
    let codec = vcodec?;
    match backend {
        HwBackend::Qsv => match codec {
            "h264" | "hevc" | "av1" | "vp9" => Some(format!("{codec}_qsv")),
            _ => None,
        },
        HwBackend::Vaapi => match codec {
            "h264" | "hevc" | "av1" | "vp9" => Some("vaapi".into()),
            _ => None,
        },
        HwBackend::Nvenc | HwBackend::Amf | HwBackend::Software => None,
    }
}

pub fn target_size(source: (u32, u32), max_height: Option<u32>) -> Option<(u32, u32)> {
    let (w, h) = source;
    let max = max_height?;
    if h == 0 || h <= max {
        return None;
    }
    let scaled = (w as f64 * max as f64 / h as f64).round() as u32;
    Some((scaled - scaled % 2, max))
}

fn input_args(enc: &Encoder, before_input: &[String], input: &Path) -> Vec<String> {
    let mut a = vec![];
    match enc.route() {
        Route::Full => match enc.hw {
            HwBackend::Qsv => {
                a.extend([
                    s("-init_hw_device"),
                    format!("qsv=hw:{}", enc.device),
                    s("-filter_hw_device"),
                    s("hw"),
                    s("-hwaccel"),
                    s("qsv"),
                    s("-hwaccel_output_format"),
                    s("qsv"),
                ]);
                if let Some(d) = &enc.hw_decoder {
                    a.extend([s("-c:v"), s(d)]);
                }
            }
            HwBackend::Vaapi => {
                a.extend([
                    s("-vaapi_device"),
                    s(&enc.device),
                    s("-hwaccel"),
                    s("vaapi"),
                    s("-hwaccel_output_format"),
                    s("vaapi"),
                ]);
            }
            HwBackend::Nvenc | HwBackend::Amf | HwBackend::Software => {}
        },
        Route::EncodeOnly if !enc.uploads() => {}
        Route::EncodeOnly => match enc.hw {
            HwBackend::Qsv => a.extend([
                s("-init_hw_device"),
                format!("qsv=hw:{}", enc.device),
                s("-filter_hw_device"),
                s("hw"),
            ]),
            HwBackend::Vaapi => a.extend([s("-vaapi_device"), s(&enc.device)]),
            HwBackend::Nvenc | HwBackend::Amf | HwBackend::Software => {}
        },
        Route::Software => {}
    }
    a.extend(before_input.iter().cloned());
    a.extend([s("-i"), p(input)]);
    a
}

/// The end of a filter chain on the CPU: the encoder's pixel format (Quick Sync takes NV12 only),
/// uploaded when the encoder works on its device's frames.
fn to_encoder(enc: &Encoder) -> Vec<String> {
    match (enc.uploads(), enc.hw) {
        (false, HwBackend::Qsv) => vec![s("format=nv12")],
        (false, _) => vec![s("format=yuv420p")],
        (true, HwBackend::Qsv) => vec![s("format=nv12"), s("hwupload=extra_hw_frames=64")],
        (true, _) => vec![s("format=nv12"), s("hwupload")],
    }
}

fn video_filter(enc: &Encoder, max_height: Option<u32>) -> String {
    match enc.route() {
        Route::Full => {
            let name = match enc.hw {
                HwBackend::Qsv => "vpp_qsv",
                _ => "scale_vaapi",
            };
            match target_size(enc.source, max_height) {
                Some((w, h)) => format!("{name}=w={w}:h={h}:format=nv12"),
                None => format!("{name}=format=nv12"),
            }
        }
        Route::EncodeOnly => {
            let mut f = vec![];
            if let Some(h) = max_height {
                f.push(format!(
                    "scale='if(gt(ih,{h}),-2,iw)':'if(gt(ih,{h}),{h},ih)':flags=lanczos"
                ));
            }
            f.extend(to_encoder(enc));
            f.join(",")
        }
        Route::Software => {
            let mut f = vec![];
            if let Some(h) = max_height {
                f.push(format!(
                    "scale='if(gt(ih,{h}),-2,iw)':'if(gt(ih,{h}),{h},ih)':flags=lanczos"
                ));
            }
            f.push(s("format=yuv420p"));
            f.join(",")
        }
    }
}

fn video_encoder(hw: HwBackend, vbit: u64, gop: u32, preset: &str) -> Vec<String> {
    let b = format!("{vbit}");
    let buf = format!("{}", vbit * 2);
    match hw {
        HwBackend::Qsv => vec![
            s("-c:v"),
            s("h264_qsv"),
            s("-preset"),
            s("medium"),
            s("-b:v"),
            b.clone(),
            s("-maxrate"),
            b,
            s("-bufsize"),
            buf,
            s("-g"),
            gop.to_string(),
        ],
        HwBackend::Vaapi => vec![
            s("-c:v"),
            s("h264_vaapi"),
            s("-b:v"),
            b.clone(),
            s("-maxrate"),
            b,
            s("-bufsize"),
            buf,
            s("-g"),
            gop.to_string(),
        ],
        HwBackend::Nvenc => vec![
            s("-c:v"),
            s("h264_nvenc"),
            s("-preset"),
            s("p4"),
            s("-tune"),
            s("hq"),
            s("-rc"),
            s("vbr"),
            s("-profile:v"),
            s("high"),
            s("-b:v"),
            b.clone(),
            s("-maxrate"),
            b,
            s("-bufsize"),
            buf,
            s("-g"),
            gop.to_string(),
            s("-bf"),
            s("2"),
        ],
        HwBackend::Amf => vec![
            s("-c:v"),
            s("h264_amf"),
            s("-quality"),
            s("balanced"),
            s("-rc"),
            s("vbr_peak"),
            s("-profile:v"),
            s("high"),
            s("-b:v"),
            b.clone(),
            s("-maxrate"),
            b,
            s("-bufsize"),
            buf,
            s("-g"),
            gop.to_string(),
        ],
        HwBackend::Software => vec![
            s("-c:v"),
            s("libx264"),
            s("-preset"),
            s(preset),
            s("-profile:v"),
            s("high"),
            s("-b:v"),
            b.clone(),
            s("-maxrate"),
            b,
            s("-bufsize"),
            buf,
            s("-g"),
            gop.to_string(),
        ],
    }
}

fn audio_args(has_audio: bool, normalize: Option<i32>, abit: u64) -> Vec<String> {
    if !has_audio {
        return vec![s("-an")];
    }
    let mut a = vec![
        s("-c:a"),
        s("aac"),
        s("-b:a"),
        format!("{abit}"),
        s("-ar"),
        s("48000"),
        s("-ac"),
        s("2"),
    ];
    if let Some(target) = normalize {
        a.extend([s("-af"), loudnorm_once(target)]);
    }
    a
}

/// The end of every MP4 reel writes: none of the source's tags (a phone's place, its make and
/// model; the rotation is side data and stays), the index first so a link plays at once.
fn mp4_out() -> [String; 6] {
    [
        s("-map_metadata"),
        s("-1"),
        s("-movflags"),
        s("+faststart"),
        s("-f"),
        s("mp4"),
    ]
}

pub fn gop_for(fps: f64) -> u32 {
    let f = if fps > 1.0 { fps } else { 60.0 };
    (f * 2.0).round() as u32
}

/// The published master. The output path is appended by `run_to`.
pub fn master(plan: &Plan, enc: &Encoder, input: &Path) -> Vec<String> {
    let mut a = vec![];
    match &plan.video {
        VideoPlan::Copy => {
            a.extend([
                s("-i"),
                p(input),
                s("-map"),
                s("0:v:0"),
                s("-map"),
                s("0:a?"),
                s("-c:v"),
                s("copy"),
            ]);
        }
        VideoPlan::Encode { max_height, mbit } => {
            a.extend(input_args(enc, &[], input));
            a.extend([
                s("-map"),
                s("0:v:0"),
                s("-map"),
                s("0:a?"),
                s("-vf"),
                video_filter(enc, Some(*max_height)),
            ]);
            a.extend(video_encoder(
                enc.hw,
                *mbit as u64 * 1_000_000,
                gop_for(enc.fps),
                "medium",
            ));
        }
    }
    match &plan.audio {
        AudioPlan::None => a.push(s("-an")),
        AudioPlan::Copy => a.extend([s("-c:a"), s("copy")]),
        AudioPlan::Encode { kbit, normalize } => {
            a.extend(audio_args(true, *normalize, *kbit as u64 * 1000))
        }
    }
    a.extend(mp4_out());
    a
}

/// The audio of a cut: none, one track of the master copied, or the file `cut_audio` made.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CutAudio<'a> {
    None,
    Copy(usize),
    File(&'a Path),
}

fn trim_args(recipe: &CutRecipe) -> (Vec<String>, Vec<String>) {
    let mut before = vec![];
    if let Some(start) = recipe.start {
        if start > 0.0 {
            before.extend([s("-ss"), format!("{start:.3}")]);
        }
    }
    let mut after = vec![];
    if let Some(end) = recipe.end {
        let start = recipe.start.unwrap_or(0.0).max(0.0);
        let len = (end - start).max(0.1);
        after.extend([s("-t"), format!("{len:.3}")]);
    }
    (before, after)
}

/// A cut from the master. `copy` is decided by the caller from the keyframe list.
pub fn cut(
    recipe: &CutRecipe,
    copy: bool,
    enc: &Encoder,
    input: &Path,
    audio: CutAudio,
) -> Vec<String> {
    cut_with(recipe, copy, enc, input, audio, None)
}

/// The picture's own filters, before the encoder's: the window cropped out, even sized, and the
/// fades; a reframed or faded cut is decoded in software, so these see its frames.
pub fn picture_filter(recipe: &CutRecipe, len: Option<f64>) -> Option<String> {
    let mut f = Vec::new();
    if let Some(r) = recipe.reframe {
        let (aw, ah) = r.shape.aspect();
        let x = r.x.clamp(0.0, 1.0);
        f.push(format!(
            "crop=w='trunc(min(iw,ih*{aw}/{ah})/2)*2':h='trunc(min(ih,iw*{ah}/{aw})/2)*2':x='(iw-ow)*{x:.3}':y='(ih-oh)*{x:.3}'"
        ));
    }
    if recipe.fade_in > 0.0 {
        f.push(format!("fade=t=in:st=0:d={:.3}", recipe.fade_in));
    }
    if let (true, Some(len)) = (recipe.fade_out > 0.0, len) {
        f.push(format!(
            "fade=t=out:st={:.3}:d={:.3}",
            (len - recipe.fade_out).max(0.0),
            recipe.fade_out
        ));
    }
    (!f.is_empty()).then(|| f.join(","))
}

/// The sound's fades, after its own filters.
pub fn audio_fades(recipe: &CutRecipe, len: f64) -> Option<String> {
    let mut f = Vec::new();
    if recipe.fade_in > 0.0 {
        f.push(format!("afade=t=in:st=0:d={:.3}", recipe.fade_in));
    }
    if recipe.fade_out > 0.0 {
        f.push(format!(
            "afade=t=out:st={:.3}:d={:.3}",
            (len - recipe.fade_out).max(0.0),
            recipe.fade_out
        ));
    }
    (!f.is_empty()).then(|| f.join(","))
}

/// `cut`, with the cut's length for where its fade out starts.
pub fn cut_with(
    recipe: &CutRecipe,
    copy: bool,
    enc: &Encoder,
    input: &Path,
    audio: CutAudio,
    len: Option<f64>,
) -> Vec<String> {
    let (before, after) = trim_args(recipe);
    let mut a = if copy {
        let mut v = before;
        v.extend([s("-i"), p(input)]);
        v
    } else {
        input_args(enc, &before, input)
    };
    if let CutAudio::File(file) = audio {
        a.extend([s("-i"), p(file)]);
    }
    a.extend(after);
    a.extend([s("-map"), s("0:v:0")]);
    match audio {
        CutAudio::None => {}
        CutAudio::Copy(t) => a.extend([s("-map"), format!("0:a:{t}?")]),
        CutAudio::File(_) => a.extend([s("-map"), s("1:a")]),
    }
    if copy {
        a.extend([s("-c:v"), s("copy")]);
    } else {
        let vf = match picture_filter(recipe, len) {
            Some(picture) => format!("{picture},{}", video_filter(enc, None)),
            None => video_filter(enc, None),
        };
        a.extend([s("-vf"), vf]);
        a.extend(video_encoder(
            enc.hw,
            enc.mbit as u64 * 1_000_000,
            gop_for(enc.fps),
            "medium",
        ));
    }
    match audio {
        CutAudio::None => a.push(s("-an")),
        CutAudio::Copy(_) | CutAudio::File(_) => a.extend([s("-c:a"), s("copy")]),
    }
    a.extend(mp4_out());
    a
}

/// What follows the mix of the tracks; None when the audio is copied. A boost, a raised track
/// or summed tracks are limited at `aim`, four times oversampled for the true peak.
pub fn audio_filter(recipe: &CutRecipe, measured: Option<&Loudnorm>, aim: f64) -> Option<String> {
    if !recipe.audio_changed() {
        return None;
    }
    let active = recipe.active_tracks();
    let summed = active.len() > 1 || active.iter().any(|(_, g)| *g > 0.0);
    if let Some(target) = recipe.loudness {
        return Some(match measured {
            Some(m) => format!(
                "loudnorm=I={target:.1}:TP={aim:.2}:LRA={LOUDNORM_LRA:.1}:measured_I={:.2}:measured_TP={:.2}:measured_LRA={:.2}:measured_thresh={:.2}:offset={:.2}:linear=true",
                m.input_i, m.input_tp, m.input_lra, m.input_thresh, m.target_offset
            ),
            None => s("anull"),
        });
    }
    let g = recipe.gain_db;
    let limit = 10f64.powf(aim / 20.0);
    let limiter = format!(
        "aresample=192000,alimiter=limit={limit:.5}:attack=5:release=50:level=0:latency=1,aresample=48000"
    );
    Some(match (g, summed) {
        (g, _) if g > 0.0 => format!("volume={g:.1}dB,{limiter}"),
        (g, true) if g < 0.0 => format!("volume={g:.1}dB,{limiter}"),
        (_, true) => limiter,
        (g, false) if g < 0.0 => format!("volume={g:.1}dB"),
        _ => s("anull"),
    })
}

/// The active tracks, each at its gain, summed as they are into one stream, then `post`; the
/// stream is `[out]`.
pub fn mix_graph(active: &[(usize, f64)], post: &str) -> String {
    let gain = |g: f64| format!("volume={g:.1}dB");
    if let [(i, g)] = active {
        let mut chain = Vec::new();
        if *g != 0.0 {
            chain.push(gain(*g));
        }
        chain.push(post.to_string());
        return format!("[0:a:{i}]{}[out]", chain.join(","));
    }
    let mut parts: Vec<String> = active
        .iter()
        .map(|(i, g)| format!("[0:a:{i}]{}[t{i}]", gain(*g)))
        .collect();
    let labels: String = active.iter().map(|(i, _)| format!("[t{i}]")).collect();
    parts.push(format!(
        "{labels}amix=inputs={}:normalize=0:duration=longest,{post}[out]",
        active.len()
    ));
    parts.join(";")
}

pub fn loudnorm_pass(
    recipe: &CutRecipe,
    input: &Path,
    active: &[(usize, f64)],
    aim: f64,
) -> Vec<String> {
    let target = recipe.loudness.unwrap_or(-16.0);
    let (before, after) = trim_args(recipe);
    let mut a = before;
    a.extend([s("-i"), p(input)]);
    a.extend(after);
    a.extend([
        s("-filter_complex"),
        mix_graph(
            active,
            &format!("loudnorm=I={target:.1}:TP={aim:.2}:LRA={LOUDNORM_LRA:.1}:print_format=json"),
        ),
        s("-map"),
        s("[out]"),
        s("-f"),
        s("null"),
        s("-"),
    ]);
    a
}

/// The channels stay as they were: `-ac 2` would cost a mono track 3 dB.
pub fn cut_audio(recipe: &CutRecipe, graph: &str, kbit: u32, input: &Path) -> Vec<String> {
    let (before, after) = trim_args(recipe);
    let mut a = before;
    a.extend([s("-i"), p(input)]);
    a.extend(after);
    a.extend([
        s("-filter_complex"),
        graph.to_string(),
        s("-map"),
        s("[out]"),
    ]);
    a.extend([
        s("-c:a"),
        s("aac"),
        s("-b:a"),
        format!("{}", kbit as u64 * 1000),
        s("-ar"),
        s("48000"),
        s("-f"),
        s("mp4"),
    ]);
    a
}

pub fn level(input: &Path, track: usize) -> Vec<String> {
    vec![
        s("-i"),
        p(input),
        s("-map"),
        format!("0:a:{track}"),
        s("-af"),
        s("ebur128=peak=true:framelog=verbose"),
        s("-f"),
        s("null"),
        s("-"),
    ]
}

fn copy_audio_args(audio: &AudioChoice) -> Vec<String> {
    match audio {
        AudioChoice::None => vec![s("-an")],
        AudioChoice::Copy { .. } => vec![s("-c:a"), s("copy")],
        AudioChoice::Encode { kbit } => vec![
            s("-c:a"),
            s("aac"),
            s("-b:a"),
            format!("{}", *kbit as u64 * 1000),
            s("-ar"),
            s("48000"),
            s("-ac"),
            s("2"),
        ],
    }
}

/// The filter chain of a planned copy on the route the encoder takes: the GPU scales on the
/// full hardware route, lanczos everywhere else; the frame rate is halved with `-r` outside.
fn copy_filter(enc: &Encoder, plan: &CopyPlan) -> String {
    match enc.route() {
        Route::Full => {
            let name = match enc.hw {
                HwBackend::Qsv => "vpp_qsv",
                _ => "scale_vaapi",
            };
            if plan.scales {
                format!("{name}=w={}:h={}:format=nv12", plan.width, plan.height)
            } else {
                format!("{name}=format=nv12")
            }
        }
        Route::EncodeOnly => {
            let mut f = vec![];
            if plan.scales {
                f.push(format!("scale=-2:{}:flags=lanczos", plan.height));
            }
            f.extend(to_encoder(enc));
            f.join(",")
        }
        Route::Software => {
            let mut f = vec![];
            if plan.scales {
                f.push(format!("scale=-2:{}:flags=lanczos", plan.height));
            }
            f.push(s("format=yuv420p"));
            f.join(",")
        }
    }
}

/// The embed copy: one pass on the route the hardware allows, the plan's size and rate.
pub fn embed(plan: &CopyPlan, enc: &Encoder, input: &Path) -> Vec<String> {
    let mut a = input_args(enc, &[], input);
    a.extend([
        s("-map"),
        s("0:v:0"),
        s("-map"),
        s("0:a:0?"),
        s("-vf"),
        copy_filter(enc, plan),
    ]);
    if plan.drops_fps {
        a.extend([s("-r"), format!("{}", plan.fps)]);
    }
    a.extend(video_encoder(
        enc.hw,
        plan.vbit,
        gop_for(plan.fps),
        "medium",
    ));
    a.extend(copy_audio_args(&plan.audio));
    a.extend(mp4_out());
    a
}

/// Two-pass libx264 for a copy under an upload limit, with the plan's preset. Pass 1 writes
/// nothing.
pub fn copy_pass(pass: u8, plan: &CopyPlan, input: &Path, passlog: &Path) -> Vec<String> {
    let sw = Encoder {
        hw: HwBackend::Software,
        device: String::new(),
        windows: cfg!(windows),
        mbit: 0,
        fps: plan.fps,
        hw_decoder: None,
        source: (0, 0),
    };
    let mut a = vec![s("-i"), p(input), s("-map"), s("0:v:0")];
    if pass == 2 {
        a.extend([s("-map"), s("0:a:0?")]);
    }
    let filter = if plan.drops_fps {
        format!("fps={},{}", plan.fps, copy_filter(&sw, plan))
    } else {
        copy_filter(&sw, plan)
    };
    a.extend([s("-vf"), filter]);
    a.extend(video_encoder(
        HwBackend::Software,
        plan.vbit,
        gop_for(plan.fps),
        &plan.preset,
    ));
    a.extend([
        s("-fps_mode"),
        s("passthrough"),
        s("-pass"),
        pass.to_string(),
        s("-passlogfile"),
        p(passlog),
    ]);
    if pass == 1 {
        a.extend([s("-an"), s("-f"), s("null"), s("-")]);
    } else {
        a.extend(copy_audio_args(&plan.audio));
        a.extend(mp4_out());
    }
    a
}

/// The default poster: the first frame from `at` within `window` seconds whose average luma is
/// above black; no frame means no output, and the caller falls back to the frame at `at`.
pub fn poster_bright(input: &Path, at: f64, window: f64) -> Vec<String> {
    vec![
        s("-ss"),
        format!("{at:.3}"),
        s("-t"),
        format!("{window:.3}"),
        s("-i"),
        p(input),
        s("-vf"),
        s("signalstats,metadata=select:key=lavfi.signalstats.YAVG:value=28:function=greater,scale=1280:-2"),
        s("-fps_mode"),
        s("passthrough"),
        s("-frames:v"),
        s("1"),
        s("-q:v"),
        s("3"),
        s("-f"),
        s("image2"),
    ]
}

pub fn poster(input: &Path, at: f64) -> Vec<String> {
    vec![
        s("-ss"),
        format!("{at:.3}"),
        s("-i"),
        p(input),
        s("-frames:v"),
        s("1"),
        s("-vf"),
        s("scale=1280:-2"),
        s("-q:v"),
        s("3"),
        s("-f"),
        s("image2"),
    ]
}

/// An animated picture of a version: a GIF through a palette made from the clip, dithered, or a
/// lossy WebP at quality 70; both loop and have no sound.
pub fn anim(
    input: &Path,
    format: reel_core::recipe::AnimFormat,
    (w, h): (u32, u32),
    fps: u32,
) -> Vec<String> {
    let scaled = format!("fps={fps},scale={w}:{h}:flags=lanczos");
    let mut a = vec![
        s("-i"),
        p(input),
        s("-an"),
        s("-sn"),
        s("-map_metadata"),
        s("-1"),
    ];
    match format {
        reel_core::recipe::AnimFormat::Gif => a.extend([
            s("-vf"),
            format!("{scaled},split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a"),
            s("-loop"),
            s("0"),
            s("-f"),
            s("gif"),
        ]),
        reel_core::recipe::AnimFormat::Webp => a.extend([
            s("-vf"),
            scaled,
            s("-c:v"),
            s("libwebp_anim"),
            s("-lossless"),
            s("0"),
            s("-quality"),
            s("70"),
            s("-compression_level"),
            s("4"),
            s("-loop"),
            s("0"),
            s("-f"),
            s("webp"),
        ]),
    }
    a
}

/// One clip of a montage: its current version's file, whether it has sound, how long it runs.
pub struct MontageInput<'a> {
    pub path: &'a Path,
    pub audio: bool,
    /// The seconds of it used: the silence of a clip without sound, and the fades' offsets.
    pub seconds: f64,
    /// Where its part starts, when not at the version's start.
    pub start: Option<f64>,
    /// The part ends before the version does: only `seconds` of it are read.
    pub cut_end: bool,
    /// Its sound: 1 as it is, 0 silent.
    pub volume: f64,
}

pub struct MontageStyle {
    pub join: reel_core::recipe::MontageJoin,
    pub fit: reel_core::recipe::MontageFit,
    /// The loudness every clip's sound is brought to; none keeps each as it is.
    pub loudness: Option<i32>,
    pub kbit: u32,
}

/// A montage: each version's part scaled into the frame at one rate, its sound at its volume and
/// one loudness, joined by concat or by xfade and acrossfade overlapping by the join's seconds.
pub fn montage(
    enc: &Encoder,
    inputs: &[MontageInput],
    (w, h): (u32, u32),
    fps: u32,
    style: &MontageStyle,
) -> Vec<String> {
    let part = |i: &MontageInput| {
        let mut before = Vec::new();
        if let Some(start) = i.start.filter(|t| *t > 0.0005) {
            before.extend([s("-ss"), format!("{start:.3}")]);
        }
        if i.cut_end {
            before.extend([s("-t"), format!("{:.3}", i.seconds)]);
        }
        before
    };
    let mut a = match inputs.first() {
        Some(first) => input_args(enc, &part(first), first.path),
        None => vec![],
    };
    for i in inputs.iter().skip(1) {
        a.extend(part(i));
        a.extend([s("-i"), p(i.path)]);
    }
    let fit = match style.fit {
        reel_core::recipe::MontageFit::Bars => format!(
            "scale={w}:{h}:force_original_aspect_ratio=decrease:flags=lanczos,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black"
        ),
        reel_core::recipe::MontageFit::Fill => format!(
            "scale={w}:{h}:force_original_aspect_ratio=increase:flags=lanczos,crop={w}:{h}"
        ),
    };
    let mut graph = Vec::new();
    for (n, i) in inputs.iter().enumerate() {
        graph.push(format!(
            "[{n}:v:0]{fit},setsar=1,fps={fps},format=yuv420p,setpts=PTS-STARTPTS[v{n}]"
        ));
        graph.push(if i.audio {
            let mut chain = format!("[{n}:a:0]");
            if let Some(loudness) = style.loudness {
                chain.push_str(&format!("loudnorm=I={loudness}:TP=-1.5:LRA=11,"));
            }
            if (i.volume - 1.0).abs() > 1e-6 {
                chain.push_str(&format!("volume={:.2},", i.volume.clamp(0.0, 2.0)));
            }
            if i.volume > 1.0 + 1e-6 {
                // 0.89 is a true peak of about -1 dB, the ceiling a cut's boost has too
                chain.push_str("alimiter=limit=0.89:attack=5:release=50:level=0,");
            }
            // the sound exactly as long as the picture: a version's sound runs some milliseconds
            // over or under it, which concat evened out and a chain of crossfades adds up
            chain.push_str(&format!(
                "aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad,atrim=duration={:.3},asetpts=PTS-STARTPTS[a{n}]",
                i.seconds.max(0.1)
            ));
            chain
        } else {
            format!(
                "anullsrc=r=48000:cl=stereo,atrim=duration={:.3},aformat=sample_fmts=fltp:channel_layouts=stereo[a{n}]",
                i.seconds.max(0.1)
            )
        });
    }
    match style.join.kind.xfade() {
        Some(transition) if inputs.len() > 1 => {
            let d = style.join.overlap();
            let (mut v, mut au, mut offset) = (s("v0"), s("a0"), 0.0);
            for n in 1..inputs.len() {
                offset += inputs[n - 1].seconds - d;
                let (vo, ao) = if n + 1 == inputs.len() {
                    (s("vc"), s("ac"))
                } else {
                    (format!("x{n}"), format!("y{n}"))
                };
                graph.push(format!(
                    "[{v}][v{n}]xfade=transition={transition}:duration={d:.3}:offset={offset:.3}[{vo}]"
                ));
                graph.push(format!(
                    "[{au}][a{n}]acrossfade=d={d:.3}:c1=tri:c2=tri[{ao}]"
                ));
                (v, au) = (vo, ao);
            }
        }
        _ => {
            let pairs: String = (0..inputs.len()).map(|n| format!("[v{n}][a{n}]")).collect();
            graph.push(format!("{pairs}concat=n={}:v=1:a=1[vc][ac]", inputs.len()));
        }
    }
    graph.push(format!("[vc]{}[vout]", video_filter(enc, None)));
    a.extend([
        s("-filter_complex"),
        graph.join(";"),
        s("-map"),
        s("[vout]"),
        s("-map"),
        s("[ac]"),
    ]);
    a.extend(video_encoder(
        enc.hw,
        enc.mbit as u64 * 1_000_000,
        gop_for(f64::from(fps)),
        "medium",
    ));
    a.extend([s("-c:a"), s("aac"), s("-b:a"), format!("{}k", style.kbit)]);
    a.extend(mp4_out());
    a
}

/// A file of reel's again, every stream copied, without the tags it carried.
pub fn without_tags(input: &Path) -> Vec<String> {
    let mut a = vec![
        s("-i"),
        p(input),
        s("-map"),
        s("0:v?"),
        s("-map"),
        s("0:a?"),
        s("-c"),
        s("copy"),
    ];
    a.extend(mp4_out());
    a
}

pub fn hover_loop(input: &Path, at: f64, seconds: f64) -> Vec<String> {
    vec![
        s("-ss"),
        format!("{at:.3}"),
        s("-t"),
        format!("{seconds:.1}"),
        s("-i"),
        p(input),
        s("-map"),
        s("0:v:0"),
        s("-an"),
        s("-vf"),
        s("scale=-2:480,format=yuv420p"),
        s("-c:v"),
        s("libx264"),
        s("-preset"),
        s("veryfast"),
        s("-crf"),
        s("28"),
        s("-g"),
        s("60"),
        s("-map_metadata"),
        s("-1"),
        s("-movflags"),
        s("+faststart"),
        s("-f"),
        s("mp4"),
    ]
}

pub fn filmstrip(input: &Path, duration: f64, frames: u32) -> Vec<String> {
    let fps = frames as f64 / duration.max(0.5);
    vec![
        s("-i"),
        p(input),
        s("-map"),
        s("0:v:0"),
        s("-vf"),
        format!("fps={fps:.6},scale=320:-2,tile={frames}x1"),
        s("-frames:v"),
        s("1"),
        s("-q:v"),
        s("4"),
        s("-f"),
        s("image2"),
    ]
}

/// A one-second encode through the backend, software decode, the frames as a real encode
/// gives them: proves the device and the encoder.
pub fn hw_self_test(hw: HwBackend, device: &str, windows: bool) -> Vec<String> {
    let enc = Encoder {
        hw,
        device: device.into(),
        windows,
        mbit: 1,
        fps: 30.0,
        hw_decoder: None,
        source: (320, 240),
    };
    let mut a = input_args(
        &enc,
        &[s("-f"), s("lavfi")],
        Path::new("testsrc2=size=320x240:rate=30"),
    );
    a.extend([s("-t"), s("1"), s("-vf"), video_filter(&enc, None)]);
    a.extend(video_encoder(hw, 1_000_000, 60, "medium"));
    a.extend([s("-an"), s("-f"), s("null"), s("-")]);
    a
}

/// A half-second sample of one codec, no text overlay, for the decode probe.
pub fn probe_sample(encoder: &str, output: &Path) -> Vec<String> {
    let mut a = vec![
        s("-f"),
        s("lavfi"),
        s("-i"),
        s("testsrc2=size=320x240:rate=30"),
        s("-t"),
        s("0.5"),
        s("-pix_fmt"),
        s("yuv420p"),
        s("-c:v"),
        s(encoder),
    ];
    match encoder {
        "libx264" => a.extend([s("-preset"), s("veryfast")]),
        "libx265" => a.extend([
            s("-preset"),
            s("ultrafast"),
            s("-x265-params"),
            s("log-level=error"),
        ]),
        "libvpx-vp9" => a.extend([s("-deadline"), s("realtime"), s("-cpu-used"), s("8")]),
        "libsvtav1" => a.extend([s("-preset"), s("12")]),
        "libaom-av1" => a.extend([s("-cpu-used"), s("8"), s("-usage"), s("realtime")]),
        _ => {}
    }
    a.extend([s("-f"), s("matroska"), p(output)]);
    a
}

/// Decode a sample on the GPU and discard it: proves the hardware decoder for its codec.
pub fn hw_decode_test(hw: HwBackend, device: &str, decoder: &str, sample: &Path) -> Vec<String> {
    let enc = Encoder {
        hw,
        device: device.into(),
        windows: cfg!(windows),
        mbit: 1,
        fps: 30.0,
        hw_decoder: Some(decoder.into()),
        source: (320, 240),
    };
    let mut a = input_args(&enc, &[], sample);
    a.extend([
        s("-vf"),
        video_filter(&enc, None),
        s("-f"),
        s("null"),
        s("-"),
    ]);
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(hw: HwBackend, decoder: Option<&str>) -> Encoder {
        Encoder {
            hw,
            device: "/dev/dri/renderD128".into(),
            windows: false,
            mbit: 10,
            fps: 60.0,
            hw_decoder: decoder.map(String::from),
            source: (2560, 1440),
        }
    }

    fn on_windows(hw: HwBackend, decoder: Option<&str>) -> Encoder {
        Encoder {
            device: String::new(),
            windows: true,
            ..enc(hw, decoder)
        }
    }
    #[test]
    fn a_montage_concatenates_scaled_padded_parts_with_one_loudness() {
        let (a, b) = (Path::new("/l/a.mp4"), Path::new("/l/b.mp4"));
        let parts = [
            MontageInput {
                path: a,
                audio: true,
                seconds: 4.0,
                start: None,
                cut_end: false,
                volume: 1.0,
            },
            MontageInput {
                path: b,
                audio: false,
                seconds: 2.5,
                start: None,
                cut_end: false,
                volume: 1.0,
            },
        ];
        let style = MontageStyle {
            join: Default::default(),
            fit: Default::default(),
            loudness: Some(-16),
            kbit: 160,
        };
        let args = montage(
            &enc(HwBackend::Software, None),
            &parts,
            (1920, 1080),
            60,
            &style,
        );
        let joined = args.join(" ");
        assert!(joined.starts_with("-i /l/a.mp4 -i /l/b.mp4 -filter_complex"));
        let graph = &args[args.iter().position(|x| x == "-filter_complex").unwrap() + 1];
        assert!(graph.contains("[0:v:0]scale=1920:1080:force_original_aspect_ratio=decrease"));
        assert!(graph.contains("pad=1920:1080:(ow-iw)/2:(oh-ih)/2"));
        assert!(graph.contains("fps=60"));
        assert!(
            graph.contains("[0:a:0]loudnorm=I=-16"),
            "each sound to one loudness"
        );
        assert!(
            graph.contains("anullsrc=r=48000:cl=stereo,atrim=duration=2.500"),
            "silence for a clip without sound"
        );
        assert!(
            graph.contains("[v0][a0][v1][a1]concat=n=2:v=1:a=1[vc][ac]"),
            "hard cuts, in order"
        );
        assert!(graph.ends_with("[vc]format=yuv420p[vout]"));
        assert!(joined.contains("-map [vout] -map [ac] -c:v libx264"));
        let vaapi = montage(
            &enc(HwBackend::Vaapi, None),
            &parts,
            (1280, 720),
            30,
            &style,
        )
        .join(" ");
        assert!(
            vaapi.starts_with("-vaapi_device"),
            "the device for the hardware encoder"
        );
        assert!(vaapi.contains("[vc]format=nv12,hwupload[vout]") && vaapi.contains("h264_vaapi"));
        let qsv = montage(
            &on_windows(HwBackend::Qsv, None),
            &parts,
            (1280, 720),
            30,
            &style,
        )
        .join(" ");
        assert!(qsv.starts_with("-i /l/a.mp4"), "{qsv}");
        assert!(qsv.contains("[vc]format=nv12[vout]") && qsv.contains("-c:v h264_qsv"));
    }

    #[test]
    fn a_montage_reads_each_part_and_joins_them_with_fades() {
        use reel_core::recipe::{JoinKind, MontageFit, MontageJoin};
        let (a, b, c) = (
            Path::new("/l/a.mp4"),
            Path::new("/l/b.mp4"),
            Path::new("/l/c.mp4"),
        );
        let part = |path, start, seconds, cut_end, volume| MontageInput {
            path,
            audio: true,
            seconds,
            start,
            cut_end,
            volume,
        };
        let parts = [
            part(a, Some(2.0), 5.0, true, 1.0),
            part(b, None, 6.0, false, 0.0),
            part(c, Some(1.5), 4.0, false, 1.5),
        ];
        let style = MontageStyle {
            join: MontageJoin {
                kind: JoinKind::Fade,
                seconds: 0.5,
            },
            fit: MontageFit::Fill,
            loudness: None,
            kbit: 160,
        };
        let args = montage(
            &enc(HwBackend::Software, None),
            &parts,
            (1080, 1920),
            60,
            &style,
        );
        let joined = args.join(" ");
        assert!(
            joined.starts_with(
                "-ss 2.000 -t 5.000 -i /l/a.mp4 -i /l/b.mp4 -ss 1.500 -i /l/c.mp4 -filter_complex"
            ),
            "each part read from its start for its length: {joined}"
        );
        let graph = &args[args.iter().position(|x| x == "-filter_complex").unwrap() + 1];
        assert!(
            graph.contains("force_original_aspect_ratio=increase:flags=lanczos,crop=1080:1920"),
            "the frame filled"
        );
        assert!(!graph.contains("loudnorm"), "each sound as it is");
        assert!(
            graph.contains("[1:a:0]volume=0.00,") && graph.contains("[2:a:0]volume=1.50,alimiter=")
        );
        assert!(
            !graph.contains("[1:a:0]volume=0.00,alimiter"),
            "a quieter part needs no limit"
        );
        assert!(
            graph.contains("apad,atrim=duration=5.000,asetpts=PTS-STARTPTS[a0]"),
            "each part's sound as long as its picture"
        );
        assert!(graph.contains("[v0][v1]xfade=transition=fade:duration=0.500:offset=4.500[x1]"));
        assert!(graph.contains("[x1][v2]xfade=transition=fade:duration=0.500:offset=10.000[vc]"));
        assert!(graph.contains("[a0][a1]acrossfade=d=0.500:c1=tri:c2=tri[y1]"));
        assert!(graph.contains("[y1][a2]acrossfade=d=0.500:c1=tri:c2=tri[ac]"));
        assert!(!graph.contains("concat"));
        let black = MontageStyle {
            join: MontageJoin {
                kind: JoinKind::Black,
                seconds: 1.0,
            },
            ..style
        };
        let graph = montage(
            &enc(HwBackend::Software, None),
            &parts[..2],
            (1080, 1920),
            60,
            &black,
        )
        .join(" ");
        assert!(
            graph.contains("[v0][v1]xfade=transition=fadeblack:duration=1.000:offset=4.000[vc]")
        );
    }

    fn encode_plan() -> Plan {
        Plan {
            video: VideoPlan::Encode {
                max_height: 1080,
                mbit: 10,
            },
            audio: AudioPlan::Encode {
                kbit: 320,
                normalize: None,
            },
            reason: String::new(),
        }
    }

    #[test]
    fn master_remux_copies_both_streams() {
        let plan = Plan {
            video: VideoPlan::Copy,
            audio: AudioPlan::Copy,
            reason: String::new(),
        };
        let joined = master(&plan, &enc(HwBackend::Software, None), Path::new("in.mkv")).join(" ");
        assert_eq!(
            joined,
            "-i in.mkv -map 0:v:0 -map 0:a? -c:v copy -c:a copy -map_metadata -1 -movflags +faststart -f mp4"
        );
        let normalized = Plan {
            video: VideoPlan::Copy,
            audio: AudioPlan::Encode {
                kbit: 320,
                normalize: Some(-16),
            },
            reason: String::new(),
        };
        let n = master(
            &normalized,
            &enc(HwBackend::Software, None),
            Path::new("in.mkv"),
        )
        .join(" ");
        assert!(
            n.contains("-c:v copy")
                && n.contains("-c:a aac -b:a 320000")
                && n.contains("loudnorm=I=-16:TP=-1.5:LRA=11"),
            "{n}"
        );
        let opus = Plan {
            video: VideoPlan::Copy,
            audio: AudioPlan::Encode {
                kbit: 128,
                normalize: None,
            },
            reason: String::new(),
        };
        let o = master(&opus, &enc(HwBackend::Software, None), Path::new("in.mkv")).join(" ");
        assert!(o.contains("-b:a 128000") && !o.contains("loudnorm"), "{o}");
        let silent = Plan {
            video: VideoPlan::Copy,
            audio: AudioPlan::None,
            reason: String::new(),
        };
        assert!(master(
            &silent,
            &enc(HwBackend::Software, None),
            Path::new("in.mkv")
        )
        .join(" ")
        .contains("-an"));
    }

    #[test]
    fn master_software_route() {
        let sw = master(
            &encode_plan(),
            &enc(HwBackend::Software, None),
            Path::new("in.mkv"),
        )
        .join(" ");
        assert!(sw.starts_with("-i in.mkv"));
        assert!(sw.contains("-c:v libx264"));
        assert!(sw.contains("-b:v 10000000"));
        assert!(sw.contains("-g 120"));
        assert!(sw.contains("format=yuv420p"));
    }

    #[test]
    fn nvenc_and_amf_take_system_frames() {
        for (hw, name) in [
            (HwBackend::Nvenc, "h264_nvenc"),
            (HwBackend::Amf, "h264_amf"),
        ] {
            for windows in [false, true] {
                let a = hw_self_test(hw, "", windows).join(" ");
                assert!(a.starts_with("-f lavfi -i testsrc2"), "{a}");
                assert!(a.contains(&format!("format=yuv420p -c:v {name}")), "{a}");
                assert!(!a.contains("hwupload"), "{a}");
                assert!(a.ends_with("-f null -"), "{a}");
                assert!(hw_decoder(hw, Some("h264"), windows).is_none());
            }
            let e = enc(hw, None);
            assert_eq!(e.route(), Route::EncodeOnly);
            assert_eq!(e.label(), format!("{}, software decode", hw.name()));
        }
        assert_eq!(
            hw_self_test(HwBackend::Nvenc, "", true),
            hw_self_test(HwBackend::Nvenc, "", false)
        );
        assert_eq!(
            hw_self_test(HwBackend::Nvenc, "", true).join(" "),
            "-f lavfi -i testsrc2=size=320x240:rate=30 -t 1 -vf format=yuv420p -c:v h264_nvenc -preset p4 -tune hq -rc vbr -profile:v high -b:v 1000000 -maxrate 1000000 -bufsize 2000000 -g 60 -bf 2 -an -f null -"
        );
    }

    #[test]
    fn on_linux_quick_sync_and_vaapi_upload_to_the_device() {
        let dev = "/dev/dri/renderD128";
        let input = Path::new("in.mkv");
        assert_eq!(
            hw_self_test(HwBackend::Qsv, dev, false).join(" "),
            "-init_hw_device qsv=hw:/dev/dri/renderD128 -filter_hw_device hw -f lavfi -i testsrc2=size=320x240:rate=30 -t 1 -vf format=nv12,hwupload=extra_hw_frames=64 -c:v h264_qsv -preset medium -b:v 1000000 -maxrate 1000000 -bufsize 2000000 -g 60 -an -f null -"
        );
        assert_eq!(
            hw_self_test(HwBackend::Vaapi, dev, false).join(" "),
            "-vaapi_device /dev/dri/renderD128 -f lavfi -i testsrc2=size=320x240:rate=30 -t 1 -vf format=nv12,hwupload -c:v h264_vaapi -b:v 1000000 -maxrate 1000000 -bufsize 2000000 -g 60 -an -f null -"
        );
        assert_eq!(
            master(&encode_plan(), &enc(HwBackend::Qsv, None), input).join(" "),
            "-init_hw_device qsv=hw:/dev/dri/renderD128 -filter_hw_device hw -i in.mkv -map 0:v:0 -map 0:a? -vf scale='if(gt(ih,1080),-2,iw)':'if(gt(ih,1080),1080,ih)':flags=lanczos,format=nv12,hwupload=extra_hw_frames=64 -c:v h264_qsv -preset medium -b:v 10000000 -maxrate 10000000 -bufsize 20000000 -g 120 -c:a aac -b:a 320000 -ar 48000 -ac 2 -map_metadata -1 -movflags +faststart -f mp4"
        );
        assert_eq!(
            master(&encode_plan(), &enc(HwBackend::Qsv, Some("hevc_qsv")), input).join(" "),
            "-init_hw_device qsv=hw:/dev/dri/renderD128 -filter_hw_device hw -hwaccel qsv -hwaccel_output_format qsv -c:v hevc_qsv -i in.mkv -map 0:v:0 -map 0:a? -vf vpp_qsv=w=1920:h=1080:format=nv12 -c:v h264_qsv -preset medium -b:v 10000000 -maxrate 10000000 -bufsize 20000000 -g 120 -c:a aac -b:a 320000 -ar 48000 -ac 2 -map_metadata -1 -movflags +faststart -f mp4"
        );
        let copy = plan(1080, 60.0, true, true, AudioChoice::Copy { bits: 160_000 });
        assert_eq!(
            embed(&copy, &enc(HwBackend::Qsv, None), input).join(" "),
            "-init_hw_device qsv=hw:/dev/dri/renderD128 -filter_hw_device hw -i in.mkv -map 0:v:0 -map 0:a:0? -vf scale=-2:1080:flags=lanczos,format=nv12,hwupload=extra_hw_frames=64 -r 60 -c:v h264_qsv -preset medium -b:v 3000000 -maxrate 3000000 -bufsize 6000000 -g 120 -c:a copy -map_metadata -1 -movflags +faststart -f mp4"
        );
        assert_eq!(
            hw_decode_test(HwBackend::Qsv, dev, "hevc_qsv", Path::new("s.mkv")).join(" "),
            "-init_hw_device qsv=hw:/dev/dri/renderD128 -filter_hw_device hw -hwaccel qsv -hwaccel_output_format qsv -c:v hevc_qsv -i s.mkv -vf vpp_qsv=format=nv12 -f null -"
        );
    }

    #[test]
    fn on_windows_quick_sync_takes_frames_from_memory_as_nvenc_and_amf_do() {
        let input = Path::new("in.mkv");
        assert_eq!(
            hw_self_test(HwBackend::Qsv, "", true).join(" "),
            "-f lavfi -i testsrc2=size=320x240:rate=30 -t 1 -vf format=nv12 -c:v h264_qsv -preset medium -b:v 1000000 -maxrate 1000000 -bufsize 2000000 -g 60 -an -f null -"
        );
        assert_eq!(
            master(&encode_plan(), &on_windows(HwBackend::Qsv, None), input).join(" "),
            "-i in.mkv -map 0:v:0 -map 0:a? -vf scale='if(gt(ih,1080),-2,iw)':'if(gt(ih,1080),1080,ih)':flags=lanczos,format=nv12 -c:v h264_qsv -preset medium -b:v 10000000 -maxrate 10000000 -bufsize 20000000 -g 120 -c:a aac -b:a 320000 -ar 48000 -ac 2 -map_metadata -1 -movflags +faststart -f mp4"
        );
        let copy = plan(1080, 60.0, true, true, AudioChoice::Copy { bits: 160_000 });
        assert_eq!(
            embed(&copy, &on_windows(HwBackend::Qsv, None), input).join(" "),
            "-i in.mkv -map 0:v:0 -map 0:a:0? -vf scale=-2:1080:flags=lanczos,format=nv12 -r 60 -c:v h264_qsv -preset medium -b:v 3000000 -maxrate 3000000 -bufsize 6000000 -g 120 -c:a copy -map_metadata -1 -movflags +faststart -f mp4"
        );
        for codec in ["h264", "hevc", "av1", "vp9"] {
            assert_eq!(hw_decoder(HwBackend::Qsv, Some(codec), true), None);
        }
        let given = on_windows(HwBackend::Qsv, Some("hevc_qsv"));
        assert_eq!(given.route(), Route::EncodeOnly, "no full route on Windows");
        assert_eq!(given.label(), "qsv, software decode");
        assert!(HwBackend::Qsv.takes_system_frames(true));
        assert!(!HwBackend::Qsv.takes_system_frames(false));
        assert!(!HwBackend::Vaapi.takes_system_frames(true));
    }

    #[test]
    fn qsv_full_route_keeps_frames_on_the_gpu() {
        let a = master(
            &encode_plan(),
            &enc(HwBackend::Qsv, Some("hevc_qsv")),
            Path::new("in.mkv"),
        )
        .join(" ");
        assert!(a.starts_with("-init_hw_device qsv=hw:/dev/dri/renderD128 -filter_hw_device hw -hwaccel qsv -hwaccel_output_format qsv -c:v hevc_qsv -i in.mkv"), "{a}");
        assert!(a.contains("-vf vpp_qsv=w=1920:h=1080:format=nv12"), "{a}");
        assert!(
            !a.contains("hwupload"),
            "no upload: frames never leave the GPU"
        );
        assert!(a.contains("-c:v h264_qsv"));
    }

    #[test]
    fn qsv_encode_only_route_is_the_software_decode_path() {
        let a = master(
            &encode_plan(),
            &enc(HwBackend::Qsv, None),
            Path::new("in.mkv"),
        )
        .join(" ");
        assert!(
            a.starts_with(
                "-init_hw_device qsv=hw:/dev/dri/renderD128 -filter_hw_device hw -i in.mkv"
            ),
            "{a}"
        );
        assert!(!a.contains("-hwaccel"));
        assert!(a.contains("scale='if(gt(ih,1080),-2,iw)':'if(gt(ih,1080),1080,ih)':flags=lanczos,format=nv12,hwupload=extra_hw_frames=64"));
        assert!(a.contains("-c:v h264_qsv"));
    }

    #[test]
    fn vaapi_full_route() {
        let a = master(
            &encode_plan(),
            &enc(HwBackend::Vaapi, Some("vaapi")),
            Path::new("in.mkv"),
        )
        .join(" ");
        assert!(a.starts_with("-vaapi_device /dev/dri/renderD128 -hwaccel vaapi -hwaccel_output_format vaapi -i in.mkv"), "{a}");
        assert!(a.contains("-vf scale_vaapi=w=1920:h=1080:format=nv12"));
        assert!(a.contains("-c:v h264_vaapi"));
        let e = master(
            &encode_plan(),
            &enc(HwBackend::Vaapi, None),
            Path::new("in.mkv"),
        )
        .join(" ");
        assert!(e.contains("format=nv12,hwupload") && !e.contains("-hwaccel"));
    }

    #[test]
    fn no_scaling_when_the_source_fits() {
        let mut e = enc(HwBackend::Qsv, Some("h264_qsv"));
        e.source = (1920, 1080);
        let a = master(&encode_plan(), &e, Path::new("in.mkv")).join(" ");
        assert!(a.contains("-vf vpp_qsv=format=nv12"), "{a}");
        assert_eq!(target_size((2560, 1440), Some(1080)), Some((1920, 1080)));
        assert_eq!(target_size((3440, 1440), Some(1080)), Some((2580, 1080)));
        assert_eq!(target_size((1920, 1080), Some(1080)), None);
        assert_eq!(target_size((1920, 1080), None), None);
    }

    #[test]
    fn decoders_per_backend_and_codec() {
        assert_eq!(
            hw_decoder(HwBackend::Qsv, Some("hevc"), false).as_deref(),
            Some("hevc_qsv")
        );
        assert_eq!(
            hw_decoder(HwBackend::Qsv, Some("av1"), false).as_deref(),
            Some("av1_qsv")
        );
        assert_eq!(
            hw_decoder(HwBackend::Qsv, Some("vp9"), false).as_deref(),
            Some("vp9_qsv")
        );
        assert_eq!(hw_decoder(HwBackend::Qsv, Some("mpeg2video"), false), None);
        assert_eq!(
            hw_decoder(HwBackend::Vaapi, Some("h264"), false).as_deref(),
            Some("vaapi")
        );
        assert_eq!(hw_decoder(HwBackend::Software, Some("h264"), false), None);
        assert_eq!(hw_decoder(HwBackend::Qsv, None, false), None);
    }

    #[test]
    fn routes_and_fallbacks() {
        let full = enc(HwBackend::Qsv, Some("hevc_qsv"));
        assert_eq!(full.route(), Route::Full);
        assert_eq!(full.software_decode().route(), Route::EncodeOnly);
        assert_eq!(full.software().route(), Route::Software);
        assert_eq!(full.label(), "qsv, hardware decode");
        assert_eq!(full.software_decode().label(), "qsv, software decode");
        assert_eq!(full.software().label(), "libx264");
    }

    #[test]
    fn a_reframed_faded_cut_crops_and_fades_before_the_encoder() {
        use reel_core::recipe::{Reframe, ReframeShape};
        let r = CutRecipe {
            start: Some(2.0),
            end: Some(12.0),
            reframe: Some(Reframe {
                shape: ReframeShape::Tall,
                x: 0.25,
            }),
            fade_in: 0.5,
            fade_out: 1.0,
            ..Default::default()
        };
        let a = cut_with(
            &r,
            false,
            &enc(HwBackend::Software, None),
            Path::new("m.mp4"),
            CutAudio::None,
            Some(10.0),
        );
        let vf = &a[a.iter().position(|x| x == "-vf").unwrap() + 1];
        assert!(
            vf.starts_with("crop=w='trunc(min(iw,ih*9/16)/2)*2':h='trunc(min(ih,iw*16/9)/2)*2':x='(iw-ow)*0.250'"),
            "{vf}"
        );
        assert!(
            vf.contains(",fade=t=in:st=0:d=0.500,fade=t=out:st=9.000:d=1.000,"),
            "{vf}"
        );
        assert!(
            vf.ends_with("format=yuv420p"),
            "the encoder's own after: {vf}"
        );
        assert_eq!(
            audio_fades(&r, 10.0).unwrap(),
            "afade=t=in:st=0:d=0.500,afade=t=out:st=9.000:d=1.000"
        );
        assert_eq!(picture_filter(&CutRecipe::default(), Some(10.0)), None);
    }

    #[test]
    fn cut_copy_and_reencode() {
        let r = CutRecipe {
            start: Some(8.5),
            end: Some(56.0),
            ..Default::default()
        };
        let sw = enc(HwBackend::Software, None);
        let c = cut(&r, true, &sw, Path::new("m.mp4"), CutAudio::Copy(0)).join(" ");
        assert_eq!(
            c,
            "-ss 8.500 -i m.mp4 -t 47.500 -map 0:v:0 -map 0:a:0? -c:v copy -c:a copy -map_metadata -1 -movflags +faststart -f mp4"
        );
        let e = cut(&r, false, &sw, Path::new("m.mp4"), CutAudio::Copy(0)).join(" ");
        assert!(e.contains("-c:v libx264"));
        let q = cut(
            &r,
            false,
            &enc(HwBackend::Qsv, Some("h264_qsv")),
            Path::new("m.mp4"),
            CutAudio::Copy(0),
        )
        .join(" ");
        assert!(
            q.contains("-hwaccel qsv -hwaccel_output_format qsv -c:v h264_qsv -ss 8.500 -i m.mp4"),
            "{q}"
        );
        assert!(q.contains("-vf vpp_qsv=format=nv12"));
        let muted = CutRecipe {
            mute: true,
            ..Default::default()
        };
        let m = cut(&muted, true, &sw, Path::new("m.mp4"), CutAudio::None).join(" ");
        assert!(m.contains("-an") && !m.contains("0:a"), "{m}");
    }

    #[test]
    fn a_cut_with_its_audio_made_apart_copies_the_video_and_maps_the_audio_file() {
        let r = CutRecipe {
            gain_db: 6.0,
            ..Default::default()
        };
        let a = cut(
            &r,
            true,
            &enc(HwBackend::Software, None),
            Path::new("m.mp4"),
            CutAudio::File(Path::new("a.part")),
        )
        .join(" ");
        assert_eq!(
            a,
            "-i m.mp4 -i a.part -map 0:v:0 -map 1:a -c:v copy -c:a copy -map_metadata -1 -movflags +faststart -f mp4"
        );
    }

    fn gain(db: f64) -> CutRecipe {
        CutRecipe {
            gain_db: db,
            ..Default::default()
        }
    }

    #[test]
    fn the_recipe_gives_the_audio_filter() {
        assert_eq!(
            audio_filter(&gain(-6.5), None, FIRST_AIM).as_deref(),
            Some("volume=-6.5dB")
        );
        assert_eq!(
            audio_filter(&gain(6.0), None, FIRST_AIM).as_deref(),
            Some("volume=6.0dB,aresample=192000,alimiter=limit=0.84140:attack=5:release=50:level=0:latency=1,aresample=48000")
        );
        assert_eq!(
            audio_filter(&gain(20.0), None, -2.0).as_deref(),
            Some("volume=20.0dB,aresample=192000,alimiter=limit=0.79433:attack=5:release=50:level=0:latency=1,aresample=48000")
        );
        let measured = Loudnorm {
            input_i: -21.76,
            input_tp: -16.32,
            input_lra: 0.1,
            input_thresh: -31.76,
            target_offset: 0.01,
        };
        let norm = CutRecipe {
            loudness: Some(-16.0),
            ..Default::default()
        };
        assert_eq!(
            audio_filter(&norm, Some(&measured), FIRST_AIM).as_deref(),
            Some("loudnorm=I=-16.0:TP=-1.50:LRA=20.0:measured_I=-21.76:measured_TP=-16.32:measured_LRA=0.10:measured_thresh=-31.76:offset=0.01:linear=true")
        );
        assert_eq!(
            audio_filter(&norm, None, FIRST_AIM).as_deref(),
            Some("anull")
        );
        let muted = CutRecipe {
            mute: true,
            gain_db: 6.0,
            loudness: Some(-16.0),
            ..Default::default()
        };
        assert_eq!(audio_filter(&muted, Some(&measured), FIRST_AIM), None);
        assert_eq!(audio_filter(&gain(0.0), None, FIRST_AIM), None);
        let trim_only = CutRecipe {
            start: Some(2.0),
            ..Default::default()
        };
        assert_eq!(audio_filter(&trim_only, None, FIRST_AIM), None);
    }

    #[test]
    fn the_audio_step_and_the_loudness_pass_mix_the_tracks_over_the_trim() {
        let r = CutRecipe {
            start: Some(2.0),
            end: Some(7.0),
            loudness: Some(-16.0),
            ..Default::default()
        };
        assert_eq!(
            loudnorm_pass(&r, Path::new("m.mp4"), &[(0, 0.0), (2, -6.0)], FIRST_AIM).join(" "),
            "-ss 2.000 -i m.mp4 -t 5.000 -filter_complex [0:a:0]volume=0.0dB[t0];[0:a:2]volume=-6.0dB[t2];[t0][t2]amix=inputs=2:normalize=0:duration=longest,loudnorm=I=-16.0:TP=-1.50:LRA=20.0:print_format=json[out] -map [out] -f null -"
        );
        let a = cut_audio(
            &r,
            "[0:a:1]volume=3.0dB,anull[out]",
            320,
            Path::new("m.mp4"),
        )
        .join(" ");
        assert_eq!(
            a,
            "-ss 2.000 -i m.mp4 -t 5.000 -filter_complex [0:a:1]volume=3.0dB,anull[out] -map [out] -c:a aac -b:a 320000 -ar 48000 -f mp4"
        );
        assert_eq!(
            level(Path::new("m.mp4"), 0).join(" "),
            "-i m.mp4 -map 0:a:0 -af ebur128=peak=true:framelog=verbose -f null -"
        );
    }

    #[test]
    fn tracks_mix_into_one_with_the_limiter_when_they_can_add_up() {
        use reel_core::recipe::TrackEdit;
        let t = |on: bool, g: f64| TrackEdit { on, gain_db: g };
        let two = CutRecipe {
            tracks: Some(vec![t(true, 0.0), t(false, 0.0), t(true, -6.0)]),
            ..Default::default()
        };
        let post = audio_filter(&two, None, FIRST_AIM).unwrap();
        assert!(
            post.starts_with("aresample=192000,alimiter="),
            "two tracks summed are limited: {post}"
        );
        assert_eq!(
            mix_graph(&two.active_tracks(), &post),
            format!("[0:a:0]volume=0.0dB[t0];[0:a:2]volume=-6.0dB[t2];[t0][t2]amix=inputs=2:normalize=0:duration=longest,{post}[out]")
        );
        let quieter = CutRecipe {
            tracks: Some(vec![t(false, 0.0), t(true, -3.0)]),
            ..Default::default()
        };
        assert_eq!(
            audio_filter(&quieter, None, FIRST_AIM).as_deref(),
            Some("anull")
        );
        assert_eq!(
            mix_graph(&quieter.active_tracks(), "anull"),
            "[0:a:1]volume=-3.0dB,anull[out]"
        );
        assert_eq!(
            mix_graph(&[(0, 0.0)], "volume=6.0dB"),
            "[0:a:0]volume=6.0dB[out]"
        );
    }

    fn plan(h: u32, fps: f64, scales: bool, drops: bool, audio: AudioChoice) -> CopyPlan {
        CopyPlan {
            width: h * 16 / 9,
            height: h,
            fps,
            scales,
            drops_fps: drops,
            vbit: 3_000_000,
            audio,
            audio_lowered: false,
            preset: "slow".into(),
            reason: String::new(),
        }
    }

    #[test]
    fn embed_scales_on_the_gpu_when_it_can_and_halves_the_rate_with_r() {
        let pl = plan(720, 30.0, true, true, AudioChoice::Copy { bits: 320_000 });
        let mut e = enc(HwBackend::Qsv, Some("h264_qsv"));
        e.source = (1920, 1080);
        let a = embed(&pl, &e, Path::new("m.mp4")).join(" ");
        assert!(a.contains("-vf vpp_qsv=w=1280:h=720:format=nv12"), "{a}");
        assert!(a.contains("-r 30") && a.contains("-c:a copy"), "{a}");
        let sw = embed(&pl, &e.software(), Path::new("m.mp4")).join(" ");
        assert!(
            sw.contains("scale=-2:720:flags=lanczos,format=yuv420p"),
            "{sw}"
        );
        let same = plan(1080, 60.0, false, false, AudioChoice::Encode { kbit: 192 });
        let b = embed(&same, &e.software(), Path::new("m.mp4")).join(" ");
        assert!(
            b.contains("-vf format=yuv420p") && !b.contains("-r ") && b.contains("-b:a 192000"),
            "{b}"
        );
    }

    #[test]
    fn copy_passes_stay_software_with_the_preset_and_the_audio_as_planned() {
        let pl = plan(480, 60.0, true, false, AudioChoice::Copy { bits: 320_000 });
        let p1 = copy_pass(1, &pl, Path::new("in.mp4"), Path::new("/tmp/log")).join(" ");
        assert!(p1.contains("-pass 1") && p1.contains("-f null") && p1.contains("libx264"));
        assert!(
            p1.contains("-fps_mode passthrough"),
            "both passes take the frames as they come: {p1}"
        );
        assert!(
            p1.contains("-preset slow") && p1.contains("scale=-2:480:flags=lanczos"),
            "{p1}"
        );
        let p2 = copy_pass(2, &pl, Path::new("in.mp4"), Path::new("/tmp/log")).join(" ");
        assert!(
            p2.contains("-pass 2") && p2.contains("-c:a copy") && p2.contains("+faststart"),
            "{p2}"
        );
        assert!(!p2.contains("qsv") && !p2.contains("vaapi"));
        let low = plan(480, 30.0, true, true, AudioChoice::Encode { kbit: 128 });
        let p3 = copy_pass(2, &low, Path::new("in.mp4"), Path::new("/tmp/log")).join(" ");
        assert!(
            p3.contains("-vf fps=30,scale") && !p3.contains("-r 30") && p3.contains("-b:a 128000"),
            "the rate halved by a filter, the same in both passes: {p3}"
        );
    }

    #[test]
    fn probe_arguments() {
        let t = hw_decode_test(
            HwBackend::Qsv,
            "/dev/dri/renderD128",
            "av1_qsv",
            Path::new("s.mkv"),
        )
        .join(" ");
        assert!(t.contains("-hwaccel qsv -hwaccel_output_format qsv -c:v av1_qsv -i s.mkv -vf vpp_qsv=format=nv12 -f null -"), "{t}");
        let v = hw_decode_test(
            HwBackend::Vaapi,
            "/dev/dri/renderD128",
            "vaapi",
            Path::new("s.mkv"),
        )
        .join(" ");
        assert!(
            v.contains(
                "-hwaccel vaapi -hwaccel_output_format vaapi -i s.mkv -vf scale_vaapi=format=nv12"
            ),
            "{v}"
        );
        let sample = probe_sample("libx265", Path::new("s.mkv")).join(" ");
        assert!(
            sample.contains("-c:v libx265")
                && sample.ends_with("-f matroska s.mkv")
                && !sample.contains("drawtext")
        );
    }
}
