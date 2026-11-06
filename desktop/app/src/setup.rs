//! Which encoders the bundled OBS can use on this PC, by a short test encode with the bundled
//! ffmpeg. H.264 throughout, so reel keeps the clips untouched.
use reel_media::{HwBackend, MediaError};
use serde::Serialize;
use std::time::{Duration, Instant};

/// A test encode takes under a second; a driver's first start (a laptop's sleeping graphics
/// card waking, a first CUDA context) can take a few more. A driver that hangs costs this.
pub const LIMIT: Duration = Duration::from_secs(10);

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
    test_with(
        &crate::standalone::sidecar("ffmpeg").display().to_string(),
        &crate::standalone::sidecar("ffprobe").display().to_string(),
        LIMIT,
    )
    .await
}

async fn test_with(ffmpeg: &str, ffprobe: &str, limit: Duration) -> Vec<EncoderTest> {
    let mut ff = reel_media::Ffmpeg::new(ffmpeg, ffprobe, 0, 1);
    ff.timeout = limit;
    let have = ff.encoders().await;
    let mut out = Vec::new();
    for (backend, id, word, device) in GPU {
        let enc = backend.encoder();
        let t = Instant::now();
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
                Err(MediaError::Timeout { .. }) => {
                    (false, format!("did not answer in {} s", limit.as_secs()))
                }
                Err(e) => (
                    false,
                    format!(
                        "the test encode failed: {}",
                        e.to_string().lines().next().unwrap_or("")
                    ),
                ),
            }
        };
        let ms = t.elapsed().as_millis();
        tracing::info!(encoder = id, ok, ms, line = %line, "setup: encoder tested");
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_test_that_hangs_ends_at_the_limit() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("ffmpeg");
        std::fs::write(
            &exe,
            "#!/bin/sh\ncase \"$*\" in\n  *-encoders*) printf ' V....D h264_nvenc NVIDIA\\n V....D h264_vaapi VAAPI\\n' ;;\n  *) exec sleep 30 ;;\nesac\n",
        )
        .unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let t = Instant::now();
        let rows = test_with(
            &exe.display().to_string(),
            "ffprobe",
            Duration::from_secs(1),
        )
        .await;
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "took {:?}",
            t.elapsed()
        );
        let lines: Vec<&str> = rows.iter().map(|r| r.line.as_str()).collect();
        assert_eq!(
            lines,
            [
                "did not answer in 1 s",
                "did not answer in 1 s",
                "always there; the GPU's encoders leave the game more room"
            ]
        );
    }
}
