//! Every token and component of reel's pages on one page; built only with `style-page`.
use crate::auth::User;
use crate::error::AppResult;
use crate::routes::html;
use crate::views::Who;
use crate::AppState;
use askama::Template;
use axum::extract::State;
use axum::response::Response;
use axum::Extension;

#[derive(Template)]
#[template(path = "style.html")]
pub struct StylePage {
    pub who: Who,
    pub nav: &'static str,
    pub q: String,
    pub settings_json: String,
    pub swatches: Vec<(&'static str, &'static str)>,
}

pub async fn page(
    State(st): State<AppState>,
    Extension(me): Extension<User>,
) -> AppResult<Response> {
    Ok(html(StylePage {
        who: Who::at(&st, &me).await,
        nav: "style",
        q: String::new(),
        settings_json: crate::settings::client_json(&st).await?,
        swatches: vec![
            ("bg", "--bg"),
            ("panel", "--panel"),
            ("raised", "--raised"),
            ("raised-2", "--raised-2"),
            ("line", "--line"),
            ("line-2", "--line-2"),
            ("text", "--text"),
            ("text-2", "--text-2"),
            ("text-3", "--text-3"),
            ("accent", "--accent"),
            ("primary", "--primary"),
            ("ok", "--ok"),
            ("warn", "--warn"),
            ("err", "--err"),
        ],
    }))
}
