use reel_media::synth::Synth;
use reel_pipeline::Ctx;

pub async fn run(ctx: &Ctx, count: usize) -> anyhow::Result<()> {
    let dir = ctx.config().ingest.incoming_dir.clone();
    tokio::fs::create_dir_all(&dir).await?;
    let specs: Vec<(String, Synth)> = vec![
        (
            "Overwatch 2 2026.10.07 - 21.14.03.02.DVR.mp4".into(),
            Synth::h264_1080p(12.0, "ace, last round"),
        ),
        (
            "2026-10-06 21-14-03.mkv".into(),
            Synth::hevc_1440p(8.0, "the bridge jump"),
        ),
        (
            "Desktop 2026.10.05 - 23.41.09.mkv".into(),
            Synth::h264_high_bitrate(6.0, "no scope across mid"),
        ),
        (
            "Valorant 2026.10.05 - 22.03.55.mp4".into(),
            Synth::h264_1080p(20.0, "nade through the window"),
        ),
        (
            "rocket league overtime.mp4".into(),
            Synth::small(9.0, "rocket league overtime"),
        ),
        (
            "the worst parking job.mkv".into(),
            Synth::h264_1080p(5.0, "the worst parking job"),
        ),
        (
            "Overwatch 2 2026.10.04 - 20.58.12.01.DVR.mp4".into(),
            Synth::h264_1080p(15.0, "double kill no scope"),
        ),
        (
            "2026-10-03 19-12-44.mkv".into(),
            Synth::hevc_1440p(5.0, "the clutch"),
        ),
    ];
    let mut made = 0;
    for (name, spec) in specs.into_iter().take(count) {
        let out = dir.join(&name);
        if out.exists() {
            continue;
        }
        let part = reel_media::ffmpeg::part_path(&out);
        let args = spec.args(&part);
        ctx.ff.run(&args, spec.seconds, None).await?;
        tokio::fs::rename(&part, &out).await?;
        made += 1;
        println!("wrote {}", out.display());
    }
    let broken = dir.join("Overwatch 2 2026.10.03 - 18.40.01.03.DVR.mkv");
    if !broken.exists() {
        tokio::fs::write(&broken, b"this is not a video, the relay cut it off").await?;
        println!("wrote {} (unreadable on purpose)", broken.display());
        made += 1;
    }
    println!("{made} files in {}", dir.display());
    Ok(())
}
