use crate::args;
use crate::ffmpeg::Ffmpeg;
use reel_core::config::{Hwaccel, MediaConfig};
use serde_json::json;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwBackend {
    Qsv,
    Vaapi,
    Nvenc,
    Amf,
    Software,
}

impl HwBackend {
    pub fn name(&self) -> &'static str {
        match self {
            HwBackend::Qsv => "qsv",
            HwBackend::Vaapi => "vaapi",
            HwBackend::Nvenc => "nvenc",
            HwBackend::Amf => "amf",
            HwBackend::Software => "none",
        }
    }

    pub fn encoder(&self) -> &'static str {
        match self {
            HwBackend::Qsv => "h264_qsv",
            HwBackend::Vaapi => "h264_vaapi",
            HwBackend::Nvenc => "h264_nvenc",
            HwBackend::Amf => "h264_amf",
            HwBackend::Software => "libx264",
        }
    }

    /// Whether the encoder takes its frames from system memory. NVENC, AMF and libx264 do, and
    /// so does Quick Sync on Windows, where a pool of frames on its D3D11 device fails on some
    /// Intel drivers. The others upload to their device, open it by its path (the DRI node) and
    /// can decode on it.
    pub fn takes_system_frames(&self, windows: bool) -> bool {
        match self {
            HwBackend::Nvenc | HwBackend::Amf | HwBackend::Software => true,
            HwBackend::Qsv => windows,
            HwBackend::Vaapi => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecoderStatus {
    pub codec: String,
    pub decoder: String,
    pub state: DecodeState,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeState {
    Ok,
    Failed,
    Untested,
}

impl DecodeState {
    pub fn name(&self) -> &'static str {
        match self {
            DecodeState::Ok => "ok",
            DecodeState::Failed => "failed",
            DecodeState::Untested => "untested",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HwStatus {
    pub backend: HwBackend,
    pub device: String,
    pub ffmpeg_version: Option<String>,
    pub notes: Vec<String>,
    pub decoders: Vec<DecoderStatus>,
    /// Whether this ffmpeg encodes animated WebP (libwebp_anim): the Send panel offers it then.
    pub anim_webp: bool,
}

impl HwStatus {
    pub fn software(device: &str, ffmpeg_version: Option<String>, notes: Vec<String>) -> Self {
        Self {
            backend: HwBackend::Software,
            device: device.into(),
            ffmpeg_version,
            notes,
            decoders: vec![],
            anim_webp: false,
        }
    }

    /// The animated pictures the Send panel can make here, in words.
    pub fn anim_line(&self) -> String {
        if self.anim_webp {
            "GIF and WebP (libwebp_anim)".into()
        } else {
            "GIF; no WebP: this ffmpeg has no libwebp_anim".into()
        }
    }

    /// The backend's hardware decoder for a codec, unless the start-up probe showed it failing.
    pub fn decoder_for(&self, vcodec: Option<&str>) -> Option<String> {
        let decoder = args::hw_decoder(self.backend, vcodec, cfg!(windows))?;
        let probed = self
            .decoders
            .iter()
            .find(|d| Some(d.codec.as_str()) == vcodec);
        match probed {
            Some(d) if d.state == DecodeState::Failed => None,
            _ => Some(decoder),
        }
    }

    pub fn decoders_line(&self) -> String {
        if self.backend == HwBackend::Software {
            return "none (software backend)".into();
        }
        if self.decoders.is_empty() {
            return format!("none, the CPU decodes for {}", self.backend.name());
        }
        self.decoders
            .iter()
            .map(|d| match d.state {
                DecodeState::Ok => format!("{} ok ({})", d.codec, d.decoder),
                DecodeState::Failed => format!("{} failed: {}", d.codec, d.note),
                DecodeState::Untested => format!("{} untested: {}", d.codec, d.note),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "backend": self.backend.name(),
            "device": self.device,
            "ffmpeg": self.ffmpeg_version,
            "notes": self.notes,
            "decoders": self.decoders.iter().map(|d| json!({ "codec": d.codec, "decoder": d.decoder, "state": d.state.name(), "note": d.note })).collect::<Vec<_>>(),
        })
    }
}

const PROBE_CODECS: [(&str, &[&str]); 4] = [
    ("h264", &["libx264"]),
    ("hevc", &["libx265"]),
    ("vp9", &["libvpx-vp9"]),
    ("av1", &["libsvtav1", "libaom-av1"]),
];

pub async fn detect(ff: &Ffmpeg, cfg: &MediaConfig) -> HwStatus {
    let mut status = detect_backend(ff, cfg).await;
    status.anim_webp = ff.encoders().await.iter().any(|e| e == "libwebp_anim");
    status
}

async fn detect_backend(ff: &Ffmpeg, cfg: &MediaConfig) -> HwStatus {
    let mut notes = Vec::new();
    let ffmpeg_version = ff.version().await;
    if ffmpeg_version.is_none() {
        notes.push(format!("ffmpeg not found at {}", ff.ffmpeg));
        return HwStatus::software(&cfg.render_device, ffmpeg_version, notes);
    }
    let mut wanted: Vec<HwBackend> = match cfg.hwaccel {
        Hwaccel::None => vec![],
        Hwaccel::Qsv => vec![HwBackend::Qsv],
        Hwaccel::Vaapi => vec![HwBackend::Vaapi],
        Hwaccel::Nvenc => vec![HwBackend::Nvenc],
        Hwaccel::Amf => vec![HwBackend::Amf],
        Hwaccel::Auto if cfg!(windows) => vec![HwBackend::Nvenc, HwBackend::Amf, HwBackend::Qsv],
        Hwaccel::Auto => vec![HwBackend::Qsv, HwBackend::Vaapi],
    };
    if wanted.is_empty() {
        notes.push("hardware encoding disabled by config".into());
        return HwStatus::software(&cfg.render_device, ffmpeg_version, notes);
    }
    let uploads = |b: &HwBackend| !b.takes_system_frames(cfg!(windows));
    if wanted.iter().any(uploads) && !Path::new(&cfg.render_device).exists() {
        notes.push(format!("{} not present", cfg.render_device));
        wanted.retain(|b| !uploads(b));
        if wanted.is_empty() {
            notes.push("using libx264".into());
            return HwStatus::software(&cfg.render_device, ffmpeg_version, notes);
        }
    }
    let encoders = ff.encoders().await;
    for b in wanted {
        let enc = b.encoder();
        if !encoders.iter().any(|e| e == enc) {
            notes.push(format!("ffmpeg has no {enc}"));
            continue;
        }
        let a = args::hw_self_test(b, &cfg.render_device, cfg!(windows));
        match ff.run(&a, 1.0, None).await {
            Ok(()) => {
                notes.push(format!("{} test encode ok", b.name()));
                let decoders = probe_decoders(ff, b, &cfg.render_device, &encoders).await;
                return HwStatus {
                    backend: b,
                    device: cfg.render_device.clone(),
                    ffmpeg_version,
                    notes,
                    decoders,
                    anim_webp: false,
                };
            }
            Err(e) => notes.push(format!("{} test encode failed: {e}", b.name())),
        }
    }
    notes.push("falling back to libx264".into());
    HwStatus::software(&cfg.render_device, ffmpeg_version, notes)
}

/// One sample per codec decoded on the GPU, so the ladder knows what the device can take.
pub async fn probe_decoders(
    ff: &Ffmpeg,
    backend: HwBackend,
    device: &str,
    encoders: &[String],
) -> Vec<DecoderStatus> {
    let dir = std::env::temp_dir().join(format!("reel-hwprobe-{}", std::process::id()));
    let _ = tokio::fs::create_dir_all(&dir).await;
    let mut out = Vec::new();
    for (codec, sample_encoders) in PROBE_CODECS {
        let Some(decoder) = args::hw_decoder(backend, Some(codec), cfg!(windows)) else {
            continue;
        };
        let Some(sw) = sample_encoders
            .iter()
            .find(|e| encoders.iter().any(|have| have == *e))
        else {
            out.push(DecoderStatus {
                codec: codec.into(),
                decoder,
                state: DecodeState::Untested,
                note: "no encoder in this ffmpeg to make a sample".into(),
            });
            continue;
        };
        let sample = dir.join(format!("{codec}.mkv"));
        if let Err(e) = ff.run(&args::probe_sample(sw, &sample), 0.5, None).await {
            out.push(DecoderStatus {
                codec: codec.into(),
                decoder,
                state: DecodeState::Untested,
                note: format!("sample encode failed: {e}"),
            });
            continue;
        }
        let status = match ff
            .run(
                &args::hw_decode_test(backend, device, &decoder, &sample),
                0.5,
                None,
            )
            .await
        {
            Ok(()) => DecoderStatus {
                codec: codec.into(),
                decoder,
                state: DecodeState::Ok,
                note: String::new(),
            },
            Err(e) => DecoderStatus {
                codec: codec.into(),
                decoder,
                state: DecodeState::Failed,
                note: e.to_string(),
            },
        };
        out.push(status);
    }
    let _ = tokio::fs::remove_dir_all(&dir).await;
    out
}
