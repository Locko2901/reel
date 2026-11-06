//! Which encoders the bundled OBS can use on this PC, by a short test encode with the bundled
//! ffmpeg. H.264 throughout, so reel keeps the clips untouched.
use reel_media::HwBackend;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct EncoderTest {
    /// OBS's encoder id.
    pub id: String,
    pub word: String,
    pub ok: bool,
    pub line: String,
}

/// Per encoder: the ffmpeg backend of its test, OBS's id, its name, the test's device.
#[cfg(windows)]
const GPU: &[(HwBackend, &str, &str, &str)] = &[
    (
        HwBackend::Nvenc,
        "obs_nvenc_h264_tex",
        "NVIDIA NVENC H.264",
        "",
    ),
    (HwBackend::Amf, "h264_texture_amf", "AMD AMF H.264", ""),
    (HwBackend::Qsv, "obs_qsv11_v2", "Intel Quick Sync H.264", ""),
];
#[cfg(not(windows))]
const GPU: &[(HwBackend, &str, &str, &str)] = &[
    (
        HwBackend::Nvenc,
        "obs_nvenc_h264_tex",
        "NVIDIA NVENC H.264",
        "",
    ),
    (
        HwBackend::Vaapi,
        "ffmpeg_vaapi_tex",
        "VAAPI H.264 (AMD, Intel)",
        "/dev/dri/renderD128",
    ),
];

/// The encoders in the order the setup tries them; x264 on the CPU always works and comes last.
pub async fn test_encoders() -> Vec<EncoderTest> {
    let ff = reel_media::Ffmpeg::new(
        &crate::standalone::sidecar("ffmpeg").display().to_string(),
        &crate::standalone::sidecar("ffprobe").display().to_string(),
        0,
        1,
    );
    let have = ff.encoders().await;
    let mut out = Vec::new();
    for (backend, id, word, device) in GPU {
        let enc = backend.encoder();
        let (ok, line) = if !have.iter().any(|e| e == enc) {
            (false, format!("ffmpeg has no {enc}"))
        } else {
            match ff
                .run(
                    &reel_media::args::hw_self_test(*backend, device, cfg!(windows)),
                    1.0,
                    None,
                )
                .await
            {
                Ok(()) => (true, "a test encode worked".to_string()),
                Err(e) => (
                    false,
                    format!(
                        "the test encode failed: {}",
                        e.to_string().lines().next().unwrap_or("")
                    ),
                ),
            }
        };
        tracing::info!(encoder = id, ok, line = %line, "setup: encoder tested");
        out.push(EncoderTest {
            id: id.to_string(),
            word: word.to_string(),
            ok,
            line,
        });
    }
    out.push(EncoderTest {
        id: "obs_x264".into(),
        word: "x264 on the CPU".into(),
        ok: true,
        line: "always there; the GPU's encoders leave the game more room".into(),
    });
    out
}
