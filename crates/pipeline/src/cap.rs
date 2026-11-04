//! The library cap: the oldest clips go to the trash, those without a live link first, never
//! a starred one, one in a collection or, standalone, one on no server.
use crate::Ctx;
use reel_core::format;
use reel_store::models::CapRow;

/// What the trash page names as having deleted a clip the cap sent there.
pub const CAP: &str = "the library cap";

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Count {
    pub clips: usize,
    pub bytes: u64,
}

impl Count {
    fn add(&mut self, bytes: u64) {
        self.clips += 1;
        self.bytes += bytes;
    }
}

/// What a run of the cap does: what the library holds, what goes, what is never moved.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Plan {
    pub cap: u64,
    pub holds: Count,
    /// The clips that go, in the order they go.
    pub go: Vec<CapRow>,
    pub moved: Count,
    pub starred: Count,
    pub in_sets: Count,
    pub unsent: Count,
    /// What the library holds once they have gone.
    pub left: u64,
}

impl Plan {
    pub fn over(&self) -> bool {
        self.cap > 0 && self.holds.bytes > self.cap
    }
}

/// The plan for `rows` (the oldest first) under `cap` bytes, 0 for none.
pub fn plan(rows: &[CapRow], cap: u64, keeps_unsent: bool) -> Plan {
    let mut p = Plan {
        cap,
        ..Plan::default()
    };
    for r in rows {
        p.holds.add(r.bytes.max(0) as u64);
    }
    p.left = p.holds.bytes;
    let mut order: Vec<&CapRow> = rows.iter().filter(|r| !r.linked).collect();
    order.extend(rows.iter().filter(|r| r.linked));
    for r in order {
        let bytes = r.bytes.max(0) as u64;
        if r.starred {
            p.starred.add(bytes);
        } else if r.in_set {
            p.in_sets.add(bytes);
        } else if keeps_unsent && r.uploaded_at.is_none() {
            p.unsent.add(bytes);
        } else if cap > 0 && p.left > cap {
            p.go.push(r.clone());
            p.moved.add(bytes);
            p.left -= bytes;
        }
    }
    p
}

fn clips(n: usize) -> String {
    match n {
        1 => "1 clip".into(),
        n => format!("{n} clips"),
    }
}

/// When the clips that go were recorded: "recorded Jan 3", "recorded Jan 3 to Mar 9".
fn span(go: &[CapRow]) -> String {
    let mut ats: Vec<&str> = go.iter().map(|r| r.at.as_str()).collect();
    ats.sort();
    match (ats.first(), ats.last()) {
        (Some(a), Some(b)) if format::date(a) == format::date(b) => {
            format!("recorded {}", format::date(a))
        }
        (Some(a), Some(b)) => format!("recorded {} to {}", format::date(a), format::date(b)),
        _ => String::new(),
    }
}

/// What is never moved, "3 starred (2.1 GB), 10 in collections (12 GB)", empty for nothing.
fn spared(p: &Plan) -> String {
    let parts: Vec<String> = [
        (p.starred, "starred"),
        (p.in_sets, "in collections"),
        (p.unsent, "on no server yet"),
    ]
    .iter()
    .filter(|(c, _)| c.clips > 0)
    .map(|(c, what)| format!("{} {what} ({})", c.clips, format::size(c.bytes)))
    .collect();
    parts.join(", ")
}

/// The settings page's line under Library cap: what the library holds, and what the cap moves
/// at its next run, before it is set.
pub fn line(p: &Plan, trash_days: u64) -> String {
    let holds = format!(
        "The library holds {} in {}",
        format::size(p.holds.bytes),
        clips(p.holds.clips)
    );
    let never = spared(p);
    let never = if never.is_empty() {
        String::new()
    } else {
        format!(" Never moved, whatever their age: {never}.")
    };
    if p.cap == 0 {
        return format!("{holds}, and no cap.{never}");
    }
    let cap = format::size(p.cap);
    if !p.over() {
        return format!("{holds}, under the {cap} cap: nothing to move.{never}");
    }
    let mut out = format!("{holds}, over the {cap} cap");
    if p.moved.clips > 0 {
        out.push_str(&format!(
            ": at its next run (housekeeping, every 10 minutes) the cap moves {}, {}, {}, to the trash, the oldest first, the ones without a live link before the others. The trash keeps them {trash_days} days; the disk has their room back then.",
            clips(p.moved.clips),
            format::size(p.moved.bytes),
            span(&p.go),
        ));
    } else {
        out.push_str(": every clip is spared, so nothing moves.");
    }
    out.push_str(&never);
    if p.left > p.cap {
        out.push_str(&format!(
            " That leaves {}: the spared clips keep the library over its cap.",
            format::size(p.left)
        ));
    }
    out
}

/// The notification of a run that moved clips: one a run, with the count and the size.
pub fn note_words(p: &Plan, trash_days: u64) -> (String, String) {
    (
        format!(
            "the library cap moved {} to the trash",
            clips(p.moved.clips)
        ),
        format!(
            "{}, {}, {}; the library holds {} of its {} cap now. They stay in the trash {trash_days} days.",
            clips(p.moved.clips),
            format::size(p.moved.bytes),
            span(&p.go),
            format::size(p.left),
            format::size(p.cap)
        ),
    )
}

pub async fn preview(ctx: &Ctx, cap: u64) -> anyhow::Result<Plan> {
    let cfg = ctx.config();
    let keeps = cfg.storage.cap_keeps_unsent && cfg.storage.standalone;
    Ok(plan(&ctx.store.cap_rows().await?, cap, keeps))
}

/// A run: the clips of the plan go to the trash, and a notification says how many and how much.
pub async fn enforce(ctx: &Ctx, cap: u64, keeps_unsent: bool) -> anyhow::Result<Plan> {
    if cap == 0 {
        return Ok(Plan::default());
    }
    let p = plan(&ctx.store.cap_rows().await?, cap, keeps_unsent);
    if let Some((told, at)) = ctx.store.cap_hold().await? {
        let cap_gb = (cap / 1_000_000_000) as i64;
        if p.go.is_empty() || ctx.config().storage.standalone || at.is_some_and(|a| a != cap_gb) {
            ctx.store.end_cap_hold().await?;
            tracing::info!("the library cap acts from now on");
        } else {
            if !told {
                hold(ctx, &p, cap_gb).await?;
            }
            return Ok(Plan::default());
        }
    }
    let mut done = Plan {
        go: Vec::new(),
        moved: Count::default(),
        left: p.holds.bytes,
        ..p.clone()
    };
    for r in &p.go {
        if let Some(clip) = ctx
            .store
            .clip(&r.id)
            .await?
            .filter(|c| c.deleted_at.is_none())
        {
            tracing::info!(clip = %clip.id, title = %clip.title, "the library cap sends the oldest clip to the trash");
            crate::enqueue::trash_clip(ctx, &clip).await?;
            ctx.store.set_deleted_by(&clip.id, CAP).await?;
            let bytes = r.bytes.max(0) as u64;
            done.moved.add(bytes);
            done.left = done.left.saturating_sub(bytes);
            done.go.push(r.clone());
        }
    }
    if done.moved.clips > 0 {
        let cfg = ctx.config();
        let (title, body) = note_words(&done, cfg.share.trash_days);
        tracing::info!(
            clips = done.moved.clips,
            bytes = done.moved.bytes,
            "the library cap moved clips to the trash"
        );
        crate::notify::send_later(
            ctx,
            crate::notify::Note::new(
                "library_cap",
                format!("{}: {title}", cfg.site_name),
                body,
                format!("{}/trash", cfg.public_url.trim_end_matches('/')),
            ),
        );
    }
    Ok(done)
}

/// Words for the settings page while the cap waits after the update.
pub const WAITS: &str =
    "It waits after the update: save the cap, or change it, to let it act on the clips.";

/// Nothing moves while the cap waits after the update; a note says once what would, even with
/// its event unticked, since the update changed what the cap does.
async fn hold(ctx: &Ctx, p: &Plan, cap_gb: i64) -> anyhow::Result<()> {
    ctx.store.cap_hold_told(cap_gb).await?;
    let cfg = ctx.config();
    tracing::warn!(
        clips = p.moved.clips,
        bytes = p.moved.bytes,
        "the library cap waits after the update: it would move clips to the trash; save it under Settings to let it act"
    );
    let note = crate::notify::Note::new(
        "library_cap",
        format!("{}: the library cap waits", cfg.site_name),
        format!(
            "After the update the cap acts on a server's clips too, and it would move {} ({}) to \
             the trash. Nothing moved: save the cap under Settings, Trash and caches, or change it, \
             to let it act.",
            clips(p.moved.clips),
            format::size(p.moved.bytes)
        ),
        format!("{}/settings", cfg.public_url.trim_end_matches('/')),
    );
    tokio::spawn(async move {
        if let Err(e) = crate::notify::deliver(&cfg, &note).await {
            tracing::debug!(error = %e, "the cap's waiting note");
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: i64 = 1_000_000_000;

    fn row(id: &str, at: &str, gb: i64) -> CapRow {
        CapRow {
            id: id.into(),
            title: id.into(),
            at: at.into(),
            bytes: gb * GB,
            uploaded_at: Some("2026-01-01T00:00:00Z".into()),
            linked: false,
            in_set: false,
            starred: false,
        }
    }

    fn ids(p: &Plan) -> Vec<&str> {
        p.go.iter().map(|r| r.id.as_str()).collect()
    }

    #[test]
    fn the_oldest_go_until_the_library_is_under_its_cap() {
        let rows = vec![
            row("a", "2026-01-01T00:00:00Z", 4),
            row("b", "2026-02-01T00:00:00Z", 4),
            row("c", "2026-03-01T00:00:00Z", 4),
        ];
        let p = plan(&rows, 7 * GB as u64, false);
        assert_eq!(ids(&p), vec!["a", "b"]);
        assert_eq!(
            (p.holds.bytes, p.moved.bytes, p.left),
            (12 * GB as u64, 8 * GB as u64, 4 * GB as u64)
        );
        assert!(
            plan(&rows, 12 * GB as u64, false).go.is_empty(),
            "at the cap nothing goes"
        );
        assert!(plan(&rows, 0, false).go.is_empty(), "0 is no cap");
    }

    #[test]
    fn a_live_link_goes_after_every_clip_without_one() {
        let mut rows = vec![
            row("old-linked", "2026-01-01T00:00:00Z", 4),
            row("b", "2026-02-01T00:00:00Z", 4),
            row("c", "2026-03-01T00:00:00Z", 4),
        ];
        rows[0].linked = true;
        assert_eq!(ids(&plan(&rows, 7 * GB as u64, false)), vec!["b", "c"]);
        assert_eq!(
            ids(&plan(&rows, 3 * GB as u64, false)),
            vec!["b", "c", "old-linked"],
            "and goes when nothing else is left"
        );
    }

    #[test]
    fn starred_collected_and_unsent_clips_are_never_moved() {
        let mut rows = vec![
            row("starred", "2026-01-01T00:00:00Z", 4),
            row("collected", "2026-01-02T00:00:00Z", 4),
            row("unsent", "2026-01-03T00:00:00Z", 4),
            row("free", "2026-01-04T00:00:00Z", 4),
        ];
        rows[0].starred = true;
        rows[1].in_set = true;
        rows[2].uploaded_at = None;
        let p = plan(&rows, GB as u64, true);
        assert_eq!(ids(&p), vec!["free"]);
        assert_eq!(
            (p.starred.clips, p.in_sets.clips, p.unsent.clips),
            (1, 1, 1)
        );
        assert_eq!(p.left, 12 * GB as u64, "the spared keep it over its cap");
        assert_eq!(
            ids(&plan(&rows, GB as u64, false)),
            vec!["unsent", "free"],
            "on a server every clip is on a server"
        );
    }

    #[test]
    fn the_line_says_what_the_cap_does_before_it_is_set() {
        let mut rows = vec![
            row("a", "2025-01-03T12:00:00Z", 4),
            row("b", "2025-03-09T12:00:00Z", 4),
            row("c", "2025-06-01T12:00:00Z", 4),
        ];
        rows[2].in_set = true;
        let words = line(&plan(&rows, 0, false), 7);
        assert!(
            words.starts_with("The library holds 12.0 GB in 3 clips, and no cap."),
            "{words}"
        );
        let words = line(&plan(&rows, 3 * GB as u64, false), 7);
        assert!(
            words.contains("over the 3.0 GB cap: at its next run"),
            "{words}"
        );
        assert!(
            words.contains(
                "moves 2 clips, 8.0 GB, recorded Jan 3, 2025 to Mar 9, 2025, to the trash"
            ),
            "{words}"
        );
        assert!(
            words.contains("Never moved, whatever their age: 1 in collections (4.0 GB)."),
            "{words}"
        );
        assert!(words.contains("The trash keeps them 7 days"), "{words}");
        assert!(
            words.ends_with("That leaves 4.0 GB: the spared clips keep the library over its cap."),
            "{words}"
        );
        let words = line(&plan(&rows, 20 * GB as u64, false), 7);
        assert!(
            words.contains("under the 20.0 GB cap: nothing to move."),
            "{words}"
        );
    }

    #[test]
    fn the_notification_says_how_many_and_how_much() {
        let rows = vec![
            row("a", "2025-01-03T12:00:00Z", 4),
            row("b", "2025-01-03T13:00:00Z", 4),
        ];
        let (title, body) = note_words(&plan(&rows, 5 * GB as u64, false), 7);
        assert_eq!(title, "the library cap moved 1 clip to the trash");
        assert!(body.starts_with("1 clip, 4.0 GB, recorded Jan 3, 2025; the library holds 4.0 GB of its 5.0 GB cap now."), "{body}");
    }
}
