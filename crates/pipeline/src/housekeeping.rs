use crate::Ctx;
use jiff::{Span, Timestamp};
use std::time::Duration;

pub async fn run(ctx: Ctx) {
    loop {
        if let Err(e) = once(&ctx).await {
            tracing::warn!(error = %e, "housekeeping");
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(600)) => {},
            _ = ctx.shutdown.cancelled() => break,
        }
    }
}

/// Deletes the trashed clips older than the cutoff for good, or every trashed clip without
/// one. How many went.
pub async fn purge_trash(ctx: &Ctx, cutoff: Option<&str>) -> anyhow::Result<usize> {
    let before = match cutoff {
        Some(c) => c.to_string(),
        None => Timestamp::now()
            .checked_add(Span::new().hours(1))?
            .to_string(),
    };
    crate::library::ensure(ctx).await?;
    let mut purged = 0;
    for clip in ctx.store.trashed_before(&before).await? {
        purge_clip(ctx, &clip).await?;
        purged += 1;
    }
    Ok(purged)
}

/// One clip in the trash deleted for good: its folder, its copies under a size, its rows.
pub async fn purge_clip(ctx: &Ctx, clip: &reel_store::models::Clip) -> anyhow::Result<()> {
    let _held = ctx.library.moves.lock().await;
    let dir = crate::library::trashed_dir(&ctx.config(), &clip.id);
    if dir.exists() {
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
    for r in ctx.store.renditions_of(&clip.id).await? {
        if matches!(r.kind.as_str(), "discord" | "anim") {
            if let Some(p) = &r.path {
                let _ = tokio::fs::remove_file(ctx.config().abs(p)).await;
            }
        }
    }
    ctx.store.delete_clip_rows(&clip.id).await?;
    tracing::info!(clip = %clip.id, "purged from trash");
    Ok(())
}

/// Drops the size-limited copies down to the cap, oldest first; with a cap of zero, all of
/// them. How many went and how many bytes.
pub async fn trim_discord_copies(ctx: &Ctx, cap: u64) -> anyhow::Result<(usize, u64)> {
    let copies = ctx.store.discord_renditions().await?;
    let mut total: u64 = copies.iter().map(|r| r.size.unwrap_or(0) as u64).sum();
    let (mut gone, mut freed) = (0, 0u64);
    for r in copies {
        if total <= cap {
            break;
        }
        if let Some(p) = &r.path {
            let _ = tokio::fs::remove_file(ctx.config().abs(p)).await;
        }
        let size = r.size.unwrap_or(0) as u64;
        total = total.saturating_sub(size);
        freed += size;
        gone += 1;
        ctx.store.delete_rendition(&r.id).await?;
    }
    Ok((gone, freed))
}

/// Drops the web copies no live link plays any more and the ones made for another rate or
/// height; with `all`, every one. How many went and how many bytes.
pub async fn sweep_web_copies(ctx: &Ctx, all: bool) -> anyhow::Result<(usize, u64)> {
    let served: std::collections::HashSet<String> =
        ctx.store.served_versions().await?.into_iter().collect();
    let want = reel_core::recipe::WebRecipe::current(&ctx.config()).json();
    let (mut gone, mut freed) = (0, 0u64);
    for r in ctx.store.web_renditions().await? {
        if !matches!(r.state.as_str(), "ready" | "failed") {
            continue;
        }
        let live = r
            .derived_from
            .as_deref()
            .is_some_and(|f| served.contains(f))
            && r.recipe.as_deref() == Some(want.as_str());
        if live && !all {
            continue;
        }
        if let Some(p) = &r.path {
            let _ = tokio::fs::remove_file(ctx.config().abs(p)).await;
        }
        ctx.store.delete_rendition(&r.id).await?;
        if r.state == "ready" {
            freed += r.size.unwrap_or(0) as u64;
            gone += 1;
        }
    }
    if gone > 0 {
        tracing::info!(gone, freed, "web copies removed");
    }
    Ok((gone, freed))
}

/// While the storage and the cleanup are on, a link never posted or opened is revoked its days
/// after it was made, and its preview leaves the storage. How many went.
pub async fn cleanup_links(ctx: &Ctx) -> anyhow::Result<usize> {
    let cfg = ctx.config();
    if !(cfg.external_on() && cfg.share.cleanup) {
        return Ok(0);
    }
    let now = Timestamp::now();
    let mut gone = 0;
    for row in ctx.store.shares().await? {
        let s = row.share;
        if s.cleanup_at(cfg.share.cleanup_days)
            .is_some_and(|at| at <= now)
        {
            ctx.store.revoke_share(&s.id).await?;
            tracing::info!(share = %s.id, clip = %s.clip_id, made = %s.created_at, "a link never shared was revoked by the cleanup");
            gone += 1;
        }
    }
    if gone > 0 {
        ctx.external.kick();
        sweep_web_copies(ctx, false).await?;
    }
    Ok(gone)
}

const NOT_GAMES_PASS: &str = "not_games_cleared";

/// Once: the clips that an app that is no game named (a browser, Explorer, an editor,
/// Discord, OBS) lose that game; their titles stay. How many.
pub async fn clear_not_games_once(ctx: &Ctx) -> anyhow::Result<u64> {
    if ctx.store.setting(NOT_GAMES_PASS).await?.is_some() {
        return Ok(0);
    }
    let n = ctx.store.clear_games(reel_core::games::NOT_GAMES).await?;
    ctx.store
        .set_setting(
            NOT_GAMES_PASS,
            &format!("{n} at {}", reel_core::format::now()),
        )
        .await?;
    if n > 0 {
        tracing::info!(
            cleared = n,
            "clips named after an app that is no game lost that game"
        );
    }
    Ok(n)
}

pub const PERSONAL_TAGS_PASS: &str = "personal_tags_removed";

/// Once: every file reel made before it left out its sources' tags is read, and the ones that
/// still carry a place, a device or a person's name (`personal_tag`) are copied again without
/// them, the streams untouched. Their previews leave the external storage and go up again.
/// How many files were read and how many copied again; nothing while the library is away.
pub async fn remove_personal_tags_once(ctx: &Ctx) -> anyhow::Result<(u64, u64)> {
    if ctx.store.setting(PERSONAL_TAGS_PASS).await?.is_some() || ctx.library.away().is_some() {
        return Ok((0, 0));
    }
    let cfg = ctx.config();
    let (mut read, mut copied) = (0u64, 0u64);
    for r in ctx.store.ready_video_files().await? {
        let video = cfg.abs(r.path.as_deref().unwrap_or(""));
        let mut files = vec![video.clone()];
        if r.is_version() {
            files.push(crate::derived::paths(&video).hover);
        }
        for f in files {
            if ctx.shutdown.is_cancelled() {
                return Ok((read, copied));
            }
            if !f.is_file() {
                continue;
            }
            read += 1;
            let names = match ctx.ff.tag_names(&f).await {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(file = %f.display(), error = %e, "the tags were not read");
                    continue;
                }
            };
            if !names.iter().any(|n| reel_core::probe::personal_tag(n)) {
                continue;
            }
            let args = reel_media::args::without_tags(&f);
            if let Err(e) = ctx
                .ff
                .run_to(&args, &f, r.duration.unwrap_or(1.0), None)
                .await
            {
                tracing::warn!(file = %f.display(), error = %e, "a file kept its tags");
                continue;
            }
            copied += 1;
            if f != video {
                continue;
            }
            if let Ok(m) = tokio::fs::metadata(&f).await {
                ctx.store.set_rendition_size(&r.id, m.len() as i64).await?;
            }
            for n in 0..reel_core::recipe::TRACKS_MAX {
                let _ = tokio::fs::remove_file(f.with_extension(format!("a{n}.m4a"))).await;
            }
            for o in ctx.store.external_objects().await? {
                if o.file_id == r.id && o.state == "stored" {
                    ctx.store
                        .set_external_object_state(&o.id, "deleting", None)
                        .await?;
                }
            }
        }
    }
    ctx.store
        .set_setting(
            PERSONAL_TAGS_PASS,
            &format!("{copied} of {read} at {}", reel_core::format::now()),
        )
        .await?;
    tracing::info!(
        read,
        copied,
        "files reel made, read for a place, a device or a name in their tags; those with one copied again without"
    );
    if copied > 0 {
        ctx.external.kick();
    }
    Ok((read, copied))
}

const UTC_TITLES_PASS: &str = "utc_titles_remade";

/// Once: a title made from the recording time as UTC's wall clock is made again in the pages'
/// zone; any other title stays. How many.
pub async fn remake_utc_titles_once(ctx: &Ctx) -> anyhow::Result<u64> {
    remake_utc_titles_once_in(ctx, &reel_core::format::timezone()).await
}

/// The same in a given zone (the tests' own).
pub async fn remake_utc_titles_once_in(
    ctx: &Ctx,
    zone: &jiff::tz::TimeZone,
) -> anyhow::Result<u64> {
    if ctx.store.setting(UTC_TITLES_PASS).await?.is_some() {
        return Ok(0);
    }
    let year = reel_core::names::this_year();
    let mut n = 0;
    for (id, title, recorded, game, source_name) in ctx.store.timed_titles().await? {
        let Some(at) = reel_core::format::parse(&recorded) else {
            continue;
        };
        let named = reel_core::names::parse_name(&source_name).game;
        let games: Vec<&str> = [game.as_deref(), named.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        if let Some(t) = reel_core::names::utc_title_in(&title, &games, at, zone, year) {
            ctx.store.update_clip_text(&id, Some(&t), None).await?;
            tracing::info!(clip = %id, from = %title, to = %t, "a title made in UTC, made again in the pages' zone");
            n += 1;
        }
    }
    ctx.store
        .set_setting(
            UTC_TITLES_PASS,
            &format!("{n} at {}", reel_core::format::now()),
        )
        .await?;
    Ok(n)
}

const NOT_GAME_TITLES_PASS: &str = "not_game_titles_remade";

/// Once: a title the server made from an app that is no game and its moment (`Zen, Oct 10,
/// 21:51`) keeps the moment alone; any other title stays. How many.
pub async fn remake_not_game_titles_once(ctx: &Ctx) -> anyhow::Result<u64> {
    if ctx.store.setting(NOT_GAME_TITLES_PASS).await?.is_some() {
        return Ok(0);
    }
    let mut n = 0;
    for (id, title, recorded) in ctx.store.gameless_titles().await? {
        let Some(d) = recorded
            .as_deref()
            .and_then(reel_core::format::local_datetime)
        else {
            continue;
        };
        if let Some(moment) = reel_core::names::without_not_a_game(&title, &d) {
            ctx.store.update_clip_text(&id, Some(&moment), None).await?;
            n += 1;
        }
    }
    ctx.store
        .set_setting(
            NOT_GAME_TITLES_PASS,
            &format!("{n} at {}", reel_core::format::now()),
        )
        .await?;
    if n > 0 {
        tracing::info!(
            remade = n,
            "titles made from an app that is no game keep their moment alone"
        );
    }
    Ok(n)
}

/// Removes the device uploads nothing touched for the expiry, with their files. How many went.
pub async fn expire_device_uploads(ctx: &Ctx, cutoff: &str) -> anyhow::Result<usize> {
    let mut gone = 0;
    for u in ctx.store.stale_uploads(cutoff).await? {
        let dir = ctx.config().device_uploads_dir().join(&u.id);
        if dir.exists() {
            let _ = tokio::fs::remove_dir_all(&dir).await;
        }
        ctx.store.delete_upload(&u.id).await?;
        tracing::info!(upload = %u.id, device = %u.device_id, name = %u.file_name, "unfinished upload expired");
        gone += 1;
    }
    Ok(gone)
}

/// The same for uploads from the browser in pieces.
pub async fn expire_web_uploads(ctx: &Ctx, cutoff: &str) -> anyhow::Result<usize> {
    let mut gone = 0;
    for u in ctx.store.stale_web_uploads(cutoff).await? {
        let dir = ctx.config().web_uploads_dir().join(&u.id);
        if dir.exists() {
            let _ = tokio::fs::remove_dir_all(&dir).await;
        }
        ctx.store.delete_web_upload(&u.id).await?;
        tracing::info!(upload = %u.id, user = %u.user_id, name = %u.file_name, "unfinished upload from the browser expired");
        gone += 1;
    }
    Ok(gone)
}

/// The library's part waits while the library is away; the data folder's goes on.
pub async fn once(ctx: &Ctx) -> anyhow::Result<()> {
    let library = crate::library::ensure(ctx).await.is_ok();
    let cutoff = Timestamp::now()
        .checked_sub(Span::new().hours(24 * ctx.config().share.trash_days as i64))?;
    if library {
        purge_trash(ctx, Some(&cutoff.to_string())).await?;
    }
    let stale = Timestamp::now()
        .checked_sub(Span::new().hours(ctx.config().device.upload_expiry_hours.max(1) as i64))?;
    expire_device_uploads(ctx, &stale.to_string()).await?;
    expire_web_uploads(ctx, &stale.to_string()).await?;
    if let Ok(day_ago) = Timestamp::now().checked_sub(Span::new().hours(24)) {
        let _ = ctx
            .store
            .purge_pair_codes_before(&day_ago.to_string())
            .await;
    }
    trim_discord_copies(ctx, ctx.config().share.discord_cache_gb * 1_000_000_000).await?;
    sweep_web_copies(ctx, false).await?;
    cleanup_links(ctx).await?;
    if library {
        let storage = ctx.config().storage.clone();
        crate::cap::enforce(
            ctx,
            storage.cap_gb * 1_000_000_000,
            storage.cap_keeps_unsent && storage.standalone,
        )
        .await?;
        sweep_parts(&ctx.config().library_dir()).await;
    }
    for base in [
        ctx.config().cache_dir(),
        ctx.config().web_cache_dir(),
        ctx.config().uploads_dir(),
    ] {
        sweep_parts(&base).await;
    }
    crate::notify::disk_check(ctx).await;
    let _ = ctx.store.purge_finished_jobs(100).await;
    if let Ok(week_ago) = Timestamp::now().checked_sub(Span::new().hours(24 * 7)) {
        let _ = ctx.store.purge_attempts_before(&week_ago.to_string()).await;
    }
    if !library {
        return Ok(());
    }
    for r in ctx.store.ready_versions().await? {
        let video = ctx.config().abs(r.path.as_deref().unwrap_or(""));
        let d = crate::derived::paths(&video);
        if d.poster.exists() && !d.tint.exists() {
            crate::derived::tint(ctx, &video).await;
        }
    }
    Ok(())
}

/// What the trash page names as having deleted a clip the cap sent there.
pub const CAP: &str = crate::cap::CAP;

/// The cap's run with these numbers; how many clips it moved to the trash.
pub async fn enforce_cap(ctx: &Ctx, cap_bytes: u64, keeps_unsent: bool) -> anyhow::Result<usize> {
    Ok(crate::cap::enforce(ctx, cap_bytes, keeps_unsent)
        .await?
        .moved
        .clips)
}

async fn sweep_parts(base: &std::path::Path) {
    let mut stack = vec![base.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(mut rd) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(e)) = rd.next_entry().await {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().and_then(|x| x.to_str()) == Some("part") {
                if let Ok(m) = e.metadata().await {
                    if m.modified()
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .map(|d| d > Duration::from_secs(3600))
                        .unwrap_or(false)
                    {
                        let _ = tokio::fs::remove_file(&p).await;
                    }
                }
            }
        }
    }
}
