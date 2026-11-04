//! Actions on picked clips: one request however many, the ones not acted on named with why.
use crate::auth::User;
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::extract::State;
use axum::{Extension, Json};
use reel_core::events::Event;
use reel_store::models::{Clip, Collection, Share};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The most clips one request takes: every clip of a large library, picked with Ctrl+A.
const MOST: usize = 20_000;

#[derive(Deserialize)]
pub struct Picked {
    pub ids: Vec<String>,
}

/// A clip that was not acted on, and why, in the words the bar shows.
#[derive(Serialize, Debug, PartialEq)]
pub struct Skipped {
    pub id: String,
    pub why: &'static str,
}

pub const NOT_YOURS: &str = "not yours: only who uploaded a clip, or an admin, changes it";
pub const GONE: &str = "not there any more";
pub const NOT_READY: &str = "not ready yet";
pub const NO_LINK: &str = "no link, and not yours to make one";

pub fn picked(body: &Picked) -> AppResult<Vec<String>> {
    if body.ids.len() > MOST {
        return Err(AppError::BadRequest(format!(
            "at most {MOST} clips at once"
        )));
    }
    let mut seen = std::collections::HashSet::new();
    Ok(body
        .ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect())
}

/// Runs a pick's work to its end as its own task: a large pick can outlast a proxy's timeout, and
/// a dropped request must not leave a clip marked as trashed with its folder still in the library.
async fn to_the_end<T: Send + 'static>(
    work: impl std::future::Future<Output = AppResult<T>> + Send + 'static,
) -> AppResult<T> {
    tokio::spawn(work)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("a pick's work stopped: {e}")))?
}

/// The picked clips in the library (not in the trash) the user may change, and the others.
async fn mine(st: &AppState, me: &User, ids: &[String]) -> AppResult<(Vec<Clip>, Vec<Skipped>)> {
    let mut ok = Vec::new();
    let mut skipped = Vec::new();
    for id in ids {
        match st
            .ctx
            .store
            .clip(id)
            .await?
            .filter(|c| c.deleted_at.is_none())
        {
            None => skipped.push(Skipped {
                id: id.clone(),
                why: GONE,
            }),
            Some(c) if !me.may_edit(c.owner_id.as_deref()) => skipped.push(Skipped {
                id: id.clone(),
                why: NOT_YOURS,
            }),
            Some(c) => ok.push(c),
        }
    }
    Ok((ok, skipped))
}

/// Delete: the picked clips to the trash, as the clip page's Delete does each.
pub async fn delete(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<Picked>,
) -> AppResult<Json<Value>> {
    let ids = picked(&body)?;
    crate::error::library_ready(&st.ctx).await?;
    to_the_end(async move {
        let (clips, skipped) = mine(&st, &me, &ids).await?;
        let mut done = Vec::new();
        for clip in &clips {
            reel_pipeline::enqueue::trash_clip(&st.ctx, clip).await?;
            st.ctx.store.set_deleted_by(&clip.id, &me.name).await?;
            done.push(clip.id.clone());
        }
        if !done.is_empty() {
            crate::routes::api::sweep_web_later(&st);
            tracing::info!(clips = done.len(), by = %me.name, "picked clips sent to the trash");
        }
        Ok(Json(
            json!({ "done": done, "skipped": skipped, "trash_days": st.ctx.config().share.trash_days }),
        ))
    })
    .await
}

/// Restore: the picked clips of the trash back in the library, with their links.
pub async fn restore(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<Picked>,
) -> AppResult<Json<Value>> {
    let ids = picked(&body)?;
    crate::error::library_ready(&st.ctx).await?;
    to_the_end(async move {
        let mut done = Vec::new();
        let mut skipped = Vec::new();
        for id in ids {
            match st.ctx.store.trashed_clip(&id).await? {
                None => skipped.push(Skipped { id, why: GONE }),
                Some(c) if !me.may_edit(c.owner_id.as_deref()) => {
                    skipped.push(Skipped { id, why: NOT_YOURS })
                }
                Some(c) => {
                    if reel_pipeline::enqueue::restore_clip(&st.ctx, &c.id).await? {
                        st.ctx.emit(Event::Clip {
                            id: c.id.clone(),
                            state: "ready".into(),
                            title: c.title,
                        });
                        done.push(c.id);
                    } else {
                        skipped.push(Skipped { id, why: GONE });
                    }
                }
            }
        }
        Ok(Json(json!({ "done": done, "skipped": skipped })))
    })
    .await
}

/// Delete now: the picked clips of the trash deleted for good, as the trash page's Delete now.
pub async fn purge(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<Picked>,
) -> AppResult<Json<Value>> {
    let ids = picked(&body)?;
    crate::error::library_ready(&st.ctx).await?;
    to_the_end(async move {
    let mut done = Vec::new();
    let mut skipped = Vec::new();
    for id in ids {
        match st.ctx.store.trashed_clip(&id).await? {
            None => skipped.push(Skipped { id, why: GONE }),
            Some(c) if !me.may_edit(c.owner_id.as_deref()) => {
                skipped.push(Skipped { id, why: NOT_YOURS })
            }
            Some(c) => {
                reel_pipeline::housekeeping::purge_clip(&st.ctx, &c).await?;
                st.ctx.emit(Event::Clip {
                    id: c.id.clone(),
                    state: "gone".into(),
                    title: c.title,
                });
                done.push(c.id);
            }
        }
    }
    if !done.is_empty() {
        tracing::info!(clips = done.len(), by = %me.name, "picked clips deleted for good from the trash");
    }
    Ok(Json(json!({ "done": done, "skipped": skipped })))
    })
    .await
}

#[derive(Deserialize)]
pub struct Starring {
    pub ids: Vec<String>,
    /// Star them, or take their stars off.
    pub on: bool,
}

/// Star or Unstar the picked clips; anyone may star any clip they see.
pub async fn star(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<Starring>,
) -> AppResult<Json<Value>> {
    let ids = picked(&Picked {
        ids: body.ids.clone(),
    })?;
    let mut done = Vec::new();
    let mut skipped = Vec::new();
    for id in ids {
        let here = if body.on {
            st.ctx.store.star(&me.id, &id).await?
        } else {
            st.ctx.store.unstar(&me.id, &id).await?;
            true
        };
        if here {
            done.push(id);
        } else {
            skipped.push(Skipped { id, why: GONE });
        }
    }
    Ok(Json(json!({ "done": done, "skipped": skipped })))
}

/// Copy links: each picked clip's newest live link, or one made with the defaults where the
/// user may make one, in the order picked.
pub async fn links(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<Picked>,
) -> AppResult<Json<Value>> {
    let ids = picked(&body)?;
    let base = st.ctx.config().public_url.trim_end_matches('/').to_string();
    let defaults = crate::settings::client(&st.ctx.config());
    let mut links = Vec::new();
    let mut skipped = Vec::new();
    let mut made = 0;
    for id in ids {
        let Some(clip) = st
            .ctx
            .store
            .clip(&id)
            .await?
            .filter(|c| c.deleted_at.is_none())
        else {
            skipped.push(Skipped { id, why: GONE });
            continue;
        };
        if clip.state != "ready" {
            skipped.push(Skipped { id, why: NOT_READY });
            continue;
        }
        let live = st
            .ctx
            .store
            .shares_of(&id)
            .await?
            .into_iter()
            .find(|s| s.state() == "active");
        let share = match live {
            Some(s) => s,
            None if me.may_edit(clip.owner_id.as_deref()) => {
                let share = Share::new(
                    clip.id.clone(),
                    String::new(),
                    defaults.share_download_default,
                    crate::routes::api::expiry_from(&defaults.share_expiry_default)?,
                    None,
                );
                st.ctx.store.insert_share(&share).await?;
                if let Some(r) = st
                    .ctx
                    .store
                    .renditions_of(&clip.id)
                    .await?
                    .iter()
                    .find(|r| Some(r.id.as_str()) == clip.current_id.as_deref())
                {
                    let _ = reel_pipeline::enqueue::ensure_web(&st.ctx, r).await;
                }
                made += 1;
                share
            }
            None => {
                skipped.push(Skipped { id, why: NO_LINK });
                continue;
            }
        };
        links.push(json!({ "id": clip.id, "title": clip.title, "url": format!("{base}/s/{}", share.token) }));
    }
    if made > 0 {
        // the new links' previews on the external storage
        st.ctx.external.kick();
        tracing::info!(links = made, by = %me.name, "links made for picked clips");
        reel_pipeline::notify::send_later(
            &st.ctx,
            reel_pipeline::notify::Note::new(
                "link_made",
                format!("{}: links were made", st.ctx.config().site_name),
                format!(
                    "{made} {} by {}",
                    if made == 1 { "link" } else { "links" },
                    me.name
                ),
                base.clone(),
            ),
        );
    }
    Ok(Json(
        json!({ "links": links, "made": made, "skipped": skipped }),
    ))
}

#[derive(Deserialize)]
pub struct Collect {
    pub ids: Vec<String>,
    /// A collection the user may change, or none with `name` for a new one.
    pub collection: Option<String>,
    pub name: Option<String>,
}

/// Add to a collection, or to a new one made for them; the right is on the collection.
pub async fn collect(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
    Json(body): Json<Collect>,
) -> AppResult<Json<Value>> {
    let ids = picked(&Picked {
        ids: body.ids.clone(),
    })?;
    let set = match (&body.collection, &body.name) {
        (Some(id), _) => crate::routes::collections::by_hand(&st, &me, id).await?,
        (None, Some(name)) => {
            let c = Collection::new(
                &crate::routes::collections::clean_name(name)?,
                Some(me.id.clone()),
            );
            st.ctx.store.insert_collection(&c).await?;
            c
        }
        (None, None) => {
            return Err(AppError::BadRequest(
                "a collection, or a name for a new one".into(),
            ))
        }
    };
    let mut added = 0;
    let mut skipped = Vec::new();
    for id in ids {
        if st
            .ctx
            .store
            .clip(&id)
            .await?
            .filter(|c| c.deleted_at.is_none())
            .is_none()
        {
            skipped.push(Skipped { id, why: GONE });
            continue;
        }
        st.ctx.store.add_to_collection(&set.id, &id).await?;
        added += 1;
    }
    st.ctx.external.kick();
    Ok(Json(
        json!({ "collection": { "id": set.id, "name": set.name }, "added": added, "skipped": skipped }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_counts_each_clip_once_and_has_a_ceiling() {
        let ids = |v: &[&str]| Picked {
            ids: v.iter().map(|s| s.to_string()).collect(),
        };
        assert_eq!(picked(&ids(&["a", "b", "a"])).unwrap(), vec!["a", "b"]);
        let many = Picked {
            ids: (0..=MOST).map(|i| i.to_string()).collect(),
        };
        assert!(picked(&many).is_err());
    }
}
