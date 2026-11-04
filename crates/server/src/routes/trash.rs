use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::routes::html;
use crate::views::Who;
use crate::AppState;
use askama::Template;
use axum::extract::{Path, State};
use axum::response::Response;
use axum::{Extension, Json};
use jiff::{Span, Timestamp};
use reel_core::{format, Event};
use serde_json::{json, Value};

pub struct TrashRow {
    pub id: String,
    pub title: String,
    pub poster: Option<String>,
    pub deleted: String,
    pub by: String,
    pub size: String,
    pub left: String,
    pub until: String,
    pub soon: bool,
    pub can_edit: bool,
}

#[derive(Template)]
#[template(path = "trash.html")]
pub struct TrashPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub rows: Vec<TrashRow>,
    pub summary: String,
    pub disk: String,
    pub trash_days: u64,
    pub admin: bool,
    pub editable: bool,
}

/// How long a clip deleted at `deleted_at` has before housekeeping deletes it for good, in
/// words and as a date, and whether that is within a day.
pub fn time_left(deleted_at: &str, days: u64, now: Timestamp) -> (String, String, bool) {
    let Some(until) = format::parse(deleted_at)
        .and_then(|at| at.checked_add(Span::new().hours(24 * days as i64)).ok())
    else {
        return (String::new(), String::new(), false);
    };
    let secs = until.as_second() - now.as_second();
    let words = match secs {
        s if s <= 0 => "goes within minutes".to_string(),
        s if s < 86_400 => "under a day left".to_string(),
        s => {
            let d = (s + 43_200) / 86_400;
            format!("{d} day{} left", if d == 1 { "" } else { "s" })
        }
    };
    (words, format::when(&until.to_string()), secs < 86_400)
}

pub async fn page(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Response> {
    let cfg = st.ctx.config();
    let clips = st.ctx.store.trashed().await?;
    let ids: Vec<String> = clips.iter().map(|r| r.clip.id.clone()).collect();
    let c = cfg.clone();
    let sizes: Vec<u64> = tokio::task::spawn_blocking(move || {
        ids.iter()
            .map(|id| crate::disk::dir_size(&reel_pipeline::library::trashed_dir(&c, id)))
            .collect()
    })
    .await
    .map_err(|e| anyhow::anyhow!(e))?;
    let now = Timestamp::now();
    let rows: Vec<TrashRow> = clips
        .iter()
        .zip(&sizes)
        .map(|(r, size)| {
            let c = &r.clip;
            let deleted_at = c.deleted_at.as_deref().unwrap_or_default();
            let (left, until, soon) = time_left(deleted_at, cfg.share.trash_days, now);
            TrashRow {
                id: c.id.clone(),
                title: c.title.clone(),
                poster: r
                    .rendition_id
                    .as_ref()
                    .map(|id| format!("/media/renditions/{id}.jpg")),
                deleted: format::when(deleted_at),
                by: c.deleted_by.clone().unwrap_or_default(),
                size: format::size(*size),
                left,
                until,
                soon,
                can_edit: me.may_edit(c.owner_id.as_deref()),
            }
        })
        .collect();
    let info = st.disk.get(&st.ctx.effective(), false).await;
    let bytes = info.trash;
    let summary = match rows.len() {
        0 => "empty".to_string(),
        1 => format!("1 clip, {} on disk", format::size(bytes)),
        n => format!("{n} clips, {} on disk", format::size(bytes)),
    };
    let disk = info
        .disks
        .iter()
        .find(|d| d.holds.contains(&"library"))
        .map(|d| format!("in the library, on the {}", d.line(info.library_elsewhere)))
        .unwrap_or_default();
    Ok(html(TrashPage {
        who: Who::at(&st, &me).await,
        nav: "trash",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        editable: rows.iter().any(|r| r.can_edit),
        rows,
        summary,
        disk,
        trash_days: cfg.share.trash_days,
        admin: me.admin,
    }))
}

/// One clip in the trash deleted for good now, by who may delete it.
pub async fn purge(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let clip = st
        .ctx
        .store
        .trashed_clip(&id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !me.may_edit(clip.owner_id.as_deref()) {
        return Err(AppError::Forbidden(
            "only who uploaded this clip, or an admin, deletes it".into(),
        ));
    }
    crate::error::library_ready(&st.ctx).await?;
    reel_pipeline::housekeeping::purge_clip(&st.ctx, &clip).await?;
    tracing::info!(clip = %id, by = %me.name, "deleted for good from the trash");
    st.ctx.emit(Event::Clip {
        id: clip.id,
        state: "gone".into(),
        title: clip.title,
    });
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_days_left_count_up_to_the_purge() {
        let now: Timestamp = "2026-10-29T12:00:00Z".parse().unwrap();
        let (w, _, soon) = time_left("2026-10-29T12:00:00Z", 7, now);
        assert_eq!(w, "7 days left", "just deleted, the whole trash time");
        assert!(!soon);
        assert_eq!(time_left("2026-10-23T14:00:00Z", 7, now).0, "1 day left");
        assert_eq!(time_left("2026-10-25T00:00:00Z", 7, now).0, "3 days left");
        let (w, _, soon) = time_left("2026-10-22T13:00:00Z", 7, now);
        assert_eq!(w, "under a day left");
        assert!(soon);
        assert_eq!(
            time_left("2026-10-01T12:00:00Z", 7, now).0,
            "goes within minutes"
        );
        assert_eq!(time_left("not a time", 7, now).0, "");
    }
}
