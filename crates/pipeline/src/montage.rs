//! A montage: clips one after the other in one new clip, each clip's current version left as
//! it is, its sources kept in its master's recipe.
use crate::enqueue::{ensure_preview, request_loop_and_strip};
use crate::{derived, encode, level, Ctx};
use anyhow::{anyhow, Context};
use reel_core::ids::new_id;
use reel_core::recipe::{
    montage_frame, montage_length, MontageFit, MontageJoin, MontagePart, MontageRecipe,
    MontageShape, JOIN_SECONDS, MONTAGE_MOST, PART_SHORTEST,
};
use reel_core::Event;
use reel_media::args::{self, MontageInput, MontageStyle};
use reel_store::models::*;
use serde_json::json;

/// Why a montage was not made, in the page's words.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum Refused {
    #[error("a montage needs two clips or more")]
    TooFew,
    #[error("a montage takes at most {MONTAGE_MOST} clips")]
    TooMany,
    #[error("{0} is not ready, or not in the library")]
    NotReady(String),
    #[error("the clips come to {0}, more than the library cap of {1}")]
    OverCap(String, String),
    #[error("the clips run for {0}, longer than the {1} a montage takes")]
    TooLong(String, String),
    #[error("{0}: a part of a clip runs for half a second at least")]
    PartTooShort(String),
    #[error("{0} runs for {1}, too short for its fades: shorten them or use more of it")]
    ShorterThanJoin(String, String),
}

/// One clip asked for, with the part of it and its sound; with none of them set, the whole
/// clip as it is.
#[derive(Debug, Clone, Default)]
pub struct AskedPart {
    pub clip: String,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub volume: Option<f64>,
}

/// What the editor asks for.
#[derive(Debug, Clone)]
pub struct Asked {
    pub parts: Vec<AskedPart>,
    pub title: Option<String>,
    pub join: MontageJoin,
    pub shape: MontageShape,
    pub fit: MontageFit,
    pub even_loudness: bool,
    /// A montage this one replaces: to the trash once this one is made.
    pub replaces: Option<String>,
    /// Who asked, for the trash's "deleted by".
    pub by: String,
}

impl Asked {
    /// The clips whole, with hard cuts, bars and one loudness.
    pub fn whole(clips: &[String], title: Option<String>, by: String) -> Self {
        Self {
            parts: clips
                .iter()
                .map(|c| AskedPart {
                    clip: c.clone(),
                    ..Default::default()
                })
                .collect(),
            title,
            join: MontageJoin::default(),
            shape: MontageShape::Wide,
            fit: MontageFit::Bars,
            even_loudness: true,
            replaces: None,
            by,
        }
    }
}

/// A part's own words for the job's line and the clip's reason.
fn join_words(recipe: &MontageRecipe) -> String {
    let join = match recipe.join.kind {
        reel_core::recipe::JoinKind::Cut => recipe.join.kind.words().to_string(),
        k => format!("{} of {:.1} s", k.words(), recipe.join.seconds),
    };
    let sound = if recipe.even_loudness {
        "one loudness"
    } else {
        "each clip's own loudness"
    };
    format!("{} clips, {join}, {sound}", recipe.parts.len())
}

/// The longest a montage may run: its render is one ffmpeg run under the job's timeout, at a
/// cut's priority, which holds the other jobs back while it runs.
pub const LONGEST_SECONDS: f64 = 30.0 * 60.0;

/// The montage this one replaces goes to the trash once this one is made, signed by who asked;
/// one gone or in the trash already is left.
async fn replace(ctx: &Ctx, old_id: &str, by: &str, new_id: &str) {
    let Ok(Some(old)) = ctx.store.clip(old_id).await else {
        return;
    };
    if old.deleted_at.is_some() || old.id == new_id {
        return;
    }
    match crate::enqueue::trash_clip(ctx, &old).await {
        Ok(()) => {
            let _ = ctx.store.set_deleted_by(&old.id, by).await;
            // its web copies go at once, as a Delete's do
            let _ = crate::housekeeping::sweep_web_copies(ctx, false).await;
            tracing::info!(old = %old.id, new = %new_id, "a montage replaced, the old one to the trash");
        }
        Err(e) => tracing::warn!(old = %old.id, error = %e, "the replaced montage stays"),
    }
}

/// Whether a montage about this big fits under the library cap (none at 0): one bigger than the
/// cap by itself would send the whole library to the trash.
pub fn under_cap(bytes: u64, cap_gb: u64) -> bool {
    cap_gb == 0 || bytes <= cap_gb * 1_000_000_000
}

/// The montage's own title: "Montage, Nov 1".
pub fn title(now: &str) -> String {
    format!("Montage, {}", reel_core::format::title_time(now))
}

/// The montage's game: the one every clip shares, else none.
pub fn shared_game(games: &[Option<String>]) -> Option<String> {
    let first = games.first()?.as_deref()?.trim();
    if first.is_empty() {
        return None;
    }
    games
        .iter()
        .all(|g| {
            g.as_deref()
                .is_some_and(|g| g.trim().eq_ignore_ascii_case(first))
        })
        .then(|| first.to_string())
}

/// A new clip for the montage of these clips, in this order, each its part at its volume, with
/// its master queued at a cut's priority: the person waits for it.
pub async fn request(
    ctx: &Ctx,
    asked: &Asked,
    owner_id: Option<String>,
) -> anyhow::Result<Result<(Clip, Job), Refused>> {
    let n = asked.parts.len();
    if n < 2 {
        return Ok(Err(Refused::TooFew));
    }
    if n > MONTAGE_MOST {
        return Ok(Err(Refused::TooMany));
    }
    let join = match asked.join.kind {
        reel_core::recipe::JoinKind::Cut => MontageJoin::default(),
        kind => MontageJoin {
            kind,
            seconds: if asked.join.seconds.is_finite() {
                asked.join.seconds.clamp(JOIN_SECONDS.0, JOIN_SECONDS.1)
            } else {
                JOIN_SECONDS.0
            },
        },
    };
    let mut parts = Vec::new();
    let mut shapes = Vec::new();
    let mut games = Vec::new();
    let mut lengths = Vec::new();
    let mut bytes = 0u64;
    let mut others = false;
    for (i, ask) in asked.parts.iter().enumerate() {
        let id = &ask.clip;
        let clip = ctx
            .store
            .clip(id)
            .await?
            .filter(|c| c.deleted_at.is_none() && c.state == "ready");
        let current = match &clip {
            Some(c) => match c.current_id.as_deref() {
                Some(r) => ctx.store.rendition(r).await?.filter(|r| r.is_ready()),
                None => None,
            },
            None => None,
        };
        let (Some(clip), Some(current)) = (clip, current) else {
            return Ok(Err(Refused::NotReady(id.clone())));
        };
        others |= clip.others_footage || clip.owner_id != owner_id;
        shapes.push((
            current.width.unwrap_or(0) as u32,
            current.height.unwrap_or(0) as u32,
            current.fps.unwrap_or(30.0),
        ));
        games.push(clip.game.clone());
        let duration = current.duration.unwrap_or(0.0).max(0.0);
        let at = |t: Option<f64>| t.filter(|t| t.is_finite()).map(|t| t.clamp(0.0, duration));
        let part = MontagePart {
            clip: clip.id.clone(),
            rendition: current.id.clone(),
            start: at(ask.start).filter(|t| *t > 0.0005),
            end: at(ask.end).filter(|t| *t < duration - 0.01),
            volume: ask
                .volume
                .filter(|v| v.is_finite())
                .map(|v| v.clamp(0.0, 2.0))
                .unwrap_or(1.0),
        };
        let length = part.length(duration);
        if length < PART_SHORTEST {
            return Ok(Err(Refused::PartTooShort(clip.title)));
        }
        // a clip in the middle crosses into both its neighbours, one at an end only into one
        let sides = if i == 0 || i + 1 == n { 1.0 } else { 2.0 };
        if join.overlap() > 0.0 && length < sides * join.overlap() + 0.1 {
            return Ok(Err(Refused::ShorterThanJoin(
                clip.title,
                reel_core::format::duration(length),
            )));
        }
        // the part's share of its version's size: what the montage is about to weigh
        let size = current.size.unwrap_or(0).max(0) as f64;
        bytes += (size * (length / duration.max(0.001)).min(1.0)) as u64;
        lengths.push(length);
        parts.push(part);
    }
    let seconds = montage_length(&lengths, &join);
    if seconds > LONGEST_SECONDS {
        return Ok(Err(Refused::TooLong(
            reel_core::format::duration(seconds),
            reel_core::format::duration(LONGEST_SECONDS),
        )));
    }
    let cap_gb = ctx.config().storage.cap_gb;
    if !under_cap(bytes, cap_gb) {
        return Ok(Err(Refused::OverCap(
            reel_core::format::size(bytes),
            reel_core::format::size(cap_gb * 1_000_000_000),
        )));
    }
    let (width, height, fps) = montage_frame(&shapes, asked.shape);
    let recipe = MontageRecipe {
        parts,
        width,
        height,
        fps,
        join,
        fit: asked.fit,
        even_loudness: asked.even_loudness,
    };
    let now = reel_core::format::now();
    let name = asked
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| title(&now));
    let mut clip = Clip::new(
        new_id(),
        name.chars().take(200).collect(),
        "montage",
        "montage.mp4".into(),
        Some(now),
    );
    clip.owner_id = owner_id;
    clip.others_footage = others;
    clip.game = shared_game(&games);
    ctx.store.insert_clip(&clip).await?;
    let r = Rendition::queued(
        new_id(),
        clip.id.clone(),
        "master",
        None,
        Some(recipe.json()),
        0,
    );
    ctx.store.insert_rendition(&r).await?;
    let payload = match &asked.replaces {
        Some(old) => json!({ "replaces": old, "by": asked.by }),
        None => json!({}),
    };
    let job = Job::new(
        "montage",
        Some(clip.id.clone()),
        Some(r.id.clone()),
        payload,
        PRIORITY_CUT,
    );
    ctx.store.enqueue(&job).await?;
    ctx.emit(Event::Clip {
        id: clip.id.clone(),
        state: "processing".into(),
        title: clip.title.clone(),
    });
    tracing::info!(clip = %clip.id, parts = recipe.parts.len(), width, height, fps, how = %join_words(&recipe), "a montage queued");
    Ok(Ok((clip, job)))
}

pub async fn run(ctx: &Ctx, job: &Job) -> anyhow::Result<()> {
    let rid = job
        .rendition_id
        .clone()
        .ok_or_else(|| anyhow!("montage job without rendition"))?;
    let r = ctx
        .store
        .rendition(&rid)
        .await?
        .ok_or_else(|| anyhow!("rendition vanished"))?;
    let clip = ctx
        .store
        .clip(&r.clip_id)
        .await?
        .ok_or_else(|| anyhow!("clip vanished"))?;
    let recipe: MontageRecipe =
        serde_json::from_str(r.recipe.as_deref().unwrap_or("{}")).context("montage recipe")?;
    let mut files = Vec::new();
    for part in &recipe.parts {
        let v = ctx
            .store
            .rendition(&part.rendition)
            .await?
            .filter(|v| v.is_ready())
            .ok_or_else(|| anyhow!("a version the montage is made of is gone"))?;
        let path = ctx.config().abs(
            v.path
                .as_deref()
                .ok_or_else(|| anyhow!("a version has no file"))?,
        );
        let duration = v.duration.unwrap_or(0.0).max(0.0);
        files.push((
            path,
            v.acodec.is_some(),
            part.length(duration),
            duration,
            part,
        ));
    }
    let lengths: Vec<f64> = files.iter().map(|f| f.2).collect();
    let total = montage_length(&lengths, &recipe.join);
    ctx.wait_while_held(job).await?;
    ctx.store.rendition_state(&rid, "rendering", None).await?;
    let dir = ctx.config().clip_dir(&clip.id);
    tokio::fs::create_dir_all(&dir).await?;
    let out = dir.join("master.mp4");
    let inputs: Vec<MontageInput> = files
        .iter()
        .map(|(path, audio, seconds, duration, part)| MontageInput {
            path,
            audio: *audio,
            seconds: *seconds,
            start: part.start,
            cut_end: part.end.is_some_and(|e| e < duration - 0.01),
            volume: part.volume,
        })
        .collect();
    let cfg = ctx.config();
    let size = (recipe.width, recipe.height);
    let style = MontageStyle {
        join: recipe.join,
        fit: recipe.fit,
        loudness: recipe.even_loudness.then_some(cfg.media.loudness_target),
        kbit: cfg.media.audio_kbit,
    };
    let enc = ctx
        .encoder_for(None, size, f64::from(recipe.fps))
        .await
        .software_decode();
    encode::run_ladder(
        ctx,
        job,
        &enc,
        |e| args::montage(e, &inputs, size, recipe.fps, &style),
        &out,
        total.max(0.1),
    )
    .await
    .map_err(|e| anyhow!("the montage did not render: {e}"))?;
    let probe = ctx
        .ff
        .probe(&out)
        .await
        .map_err(|e| anyhow!("the montage does not probe: {e}"))?;
    ctx.store
        .rendition_ready(
            &rid,
            &format!("library/{}/master.mp4", clip.id),
            &probe,
            false,
        )
        .await?;
    let fresh = ctx
        .store
        .rendition(&rid)
        .await?
        .ok_or_else(|| anyhow!("master row missing"))?;
    ensure_preview(ctx, &fresh).await?;
    let poster_time = derived::poster_default(ctx, &out, probe.duration).await?;
    level::measure_quietly(ctx, &fresh).await;
    let reason = join_words(&recipe);
    ctx.store
        .publish_clip(
            &clip.id,
            &rid,
            poster_time,
            None,
            &serde_json::to_string(&probe)?,
            &reason,
        )
        .await?;
    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&job.payload) {
        if let Some(old) = payload.get("replaces").and_then(|v| v.as_str()) {
            let by = payload.get("by").and_then(|v| v.as_str()).unwrap_or("");
            replace(ctx, old, by, &clip.id).await;
        }
    }
    request_loop_and_strip(ctx, &clip.id, false).await?;
    ctx.emit(Event::Clip {
        id: clip.id.clone(),
        state: "ready".into(),
        title: clip.title.clone(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_montage_has_its_date_and_the_game_all_its_clips_share() {
        assert!(title("2026-11-01T14:00:00Z").starts_with("Montage, "));
        let g = |v: &[Option<&str>]| {
            shared_game(&v.iter().map(|x| x.map(String::from)).collect::<Vec<_>>())
        };
        assert_eq!(
            g(&[Some("Valorant"), Some("valorant ")]),
            Some("Valorant".into())
        );
        assert_eq!(g(&[Some("Valorant"), Some("Overwatch 2")]), None);
        assert_eq!(g(&[Some("Valorant"), None]), None);
        assert_eq!(g(&[None, None]), None);
    }

    #[test]
    fn a_montage_bigger_than_the_cap_is_refused() {
        assert!(under_cap(500_000_000_000, 0), "no cap");
        assert!(under_cap(2_000_000_000, 2));
        assert!(!under_cap(2_000_000_001, 2));
    }
}
