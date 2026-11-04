use crate::auth::User;
use crate::error::AppResult;
use crate::routes::html;
use crate::stats::{fold, Figures};
use crate::views::Who;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::response::Response;
use axum::Extension;
use reel_core::format;

pub struct LinkLine {
    pub clip_id: String,
    pub title: String,
    pub label: String,
    pub views: i64,
    pub last: String,
    pub live: bool,
}

#[derive(Template)]
#[template(path = "stats.html")]
pub struct StatsPage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub figures: Figures,
    pub links: Vec<LinkLine>,
}

pub fn game_href(key: &str) -> String {
    format!(
        "/?game={}",
        percent_encoding::utf8_percent_encode(key, percent_encoding::NON_ALPHANUMERIC)
    )
}

pub async fn page(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Response> {
    let figures = fold(&st.ctx.store.stat_rows().await?);
    let links = st
        .ctx
        .store
        .most_watched(10)
        .await?
        .into_iter()
        .map(|l| LinkLine {
            clip_id: l.clip_id,
            title: l.title,
            label: if l.label.trim().is_empty() {
                format!("/s/{}...", &l.token[..l.token.len().min(6)])
            } else {
                l.label
            },
            views: l.views,
            last: l
                .last_viewed_at
                .as_deref()
                .map(format::when)
                .unwrap_or_default(),
            live: l.live,
        })
        .collect();
    Ok(html(StatsPage {
        who: Who::at(&st, &me).await,
        nav: "library",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        figures,
        links,
    }))
}
