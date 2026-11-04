//! Clips still titled with their file name get the title the name parser gives it today.
//! A typed title never equals the file name's stem, so it is never touched.
use crate::Ctx;
use reel_core::names;

/// The name without its extension, and without ShadowPlay's ".02.DVR" tail, which the
/// titles never carried either.
fn stems(name: &str) -> [String; 2] {
    let plain = name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(name)
        .trim()
        .to_string();
    let dvr = plain
        .strip_suffix(".DVR")
        .map(|s| {
            s.rsplit_once('.')
                .filter(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                .map(|(h, _)| h)
                .unwrap_or(s)
        })
        .unwrap_or(&plain)
        .to_string();
    [plain, dvr]
}

pub async fn sweep(ctx: &Ctx) -> anyhow::Result<usize> {
    let mut n = 0;
    for row in ctx.store.clips("", reel_store::Sort::Newest).await? {
        let c = &row.clip;
        let named = stems(&c.source_name)
            .iter()
            .any(|s| c.title == *s || c.title == s.replace('_', " "));
        if !named {
            continue;
        }
        let parsed = names::parse_name(&c.source_name);
        if parsed.recorded_at.is_none() {
            continue;
        }
        let title = crate::enqueue::parsed_title(&parsed, c.recorded_at.as_deref());
        if title == c.title {
            continue;
        }
        ctx.store
            .update_clip_text(&c.id, Some(&title), None)
            .await?;
        if let Some(g) = parsed.game.as_deref() {
            if c.game.as_deref().map(str::trim).unwrap_or("").is_empty() {
                ctx.store.set_game_if_none(&c.id, g).await?;
            }
        }
        tracing::info!(clip = %c.id, from = %c.title, to = %title, "titled from the name");
        n += 1;
    }
    if n > 0 {
        tracing::info!(
            clips = n,
            "titles from the recording time, for clips named by their file"
        );
    }
    Ok(n)
}
