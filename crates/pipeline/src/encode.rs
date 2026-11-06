use crate::{worker::progress_sink, Ctx};
use anyhow::anyhow;
use reel_media::args::{Encoder, Route};
use reel_store::models::Job;
use std::path::Path;

/// The routes to try for one clip, best first: full hardware, software decode with hardware
/// encode, libx264. A route that is not available is left out.
pub fn candidates(enc: &Encoder) -> Vec<Encoder> {
    let mut out = vec![enc.clone()];
    if enc.route() == Route::Full {
        out.push(enc.software_decode());
    }
    if enc.route() != Route::Software {
        out.push(enc.software());
    }
    out
}

pub async fn run_ladder<F>(
    ctx: &Ctx,
    job: &Job,
    enc: &Encoder,
    build: F,
    out: &Path,
    duration: f64,
) -> anyhow::Result<Encoder>
where
    F: Fn(&Encoder) -> Vec<String>,
{
    let cancel = ctx.cancel_token(&job.id).await;
    let mut attempt = |e: &Encoder| {
        let args = build(e);
        let cancel = cancel.clone();
        async move {
            ctx.ff
                .run_to_with(
                    &args,
                    out,
                    duration,
                    Some(progress_sink(ctx, job)),
                    cancel.as_ref(),
                )
                .await
        }
    };
    let used = try_routes(candidates(enc), &mut attempt).await?;
    let _ = ctx.store.job_route(&job.id, &used.label()).await;
    Ok(used)
}

pub async fn try_routes<F, Fut>(routes: Vec<Encoder>, attempt: &mut F) -> anyhow::Result<Encoder>
where
    F: FnMut(&Encoder) -> Fut,
    Fut: std::future::Future<Output = Result<(), reel_media::MediaError>>,
{
    let last = routes.len() - 1;
    for (i, route) in routes.iter().enumerate() {
        match attempt(route).await {
            Ok(()) => return Ok(route.clone()),
            Err(reel_media::MediaError::Cancelled) => return Err(anyhow!("cancelled")),
            Err(e) if i < last => {
                tracing::warn!(route = %route.label(), next = %routes[i + 1].label(), error = %e, "encode route failed, falling back");
            }
            Err(e) => {
                return Err(anyhow!(
                    "encode failed on every route, last ({}): {e}",
                    route.label()
                ))
            }
        }
    }
    Err(anyhow!("no encode route"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reel_media::HwBackend;

    fn enc(hw: HwBackend, decoder: Option<&str>) -> Encoder {
        Encoder {
            hw,
            device: "/dev/dri/renderD128".into(),
            windows: false,
            mbit: 10,
            fps: 60.0,
            hw_decoder: decoder.map(String::from),
            source: (1920, 1080),
        }
    }

    #[test]
    fn ladders_per_route() {
        let labels = |e: Encoder| candidates(&e).iter().map(|c| c.label()).collect::<Vec<_>>();
        assert_eq!(
            labels(enc(HwBackend::Qsv, Some("hevc_qsv"))),
            ["qsv, hardware decode", "qsv, software decode", "libx264"]
        );
        assert_eq!(
            labels(enc(HwBackend::Qsv, None)),
            ["qsv, software decode", "libx264"]
        );
        assert_eq!(
            labels(enc(HwBackend::Vaapi, Some("vaapi"))),
            [
                "vaapi, hardware decode",
                "vaapi, software decode",
                "libx264"
            ]
        );
        assert_eq!(labels(enc(HwBackend::Software, None)), ["libx264"]);
    }

    #[tokio::test]
    async fn falls_through_to_the_first_route_that_works() {
        let routes = candidates(&enc(HwBackend::Qsv, Some("hevc_qsv")));
        let mut tried = Vec::new();
        let used = try_routes(routes, &mut |e: &Encoder| {
            tried.push(e.label());
            let fail = e.route() != Route::Software;
            async move {
                if fail {
                    Err(reel_media::MediaError::Failed {
                        tool: "ffmpeg".into(),
                        code: 1,
                        stderr: "no device".into(),
                    })
                } else {
                    Ok(())
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(used.route(), Route::Software);
        assert_eq!(
            tried,
            ["qsv, hardware decode", "qsv, software decode", "libx264"]
        );
        let err = try_routes(
            candidates(&enc(HwBackend::Software, None)),
            &mut |_: &Encoder| async {
                Err(reel_media::MediaError::Failed {
                    tool: "ffmpeg".into(),
                    code: 1,
                    stderr: "bad input".into(),
                })
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("every route") && err.to_string().contains("bad input"));
    }
}
