use axum::{extract::State, Json};
use ep_core::{fmt_ts_hm, AppState};
use serde::Serialize;

use crate::errors::ApiError;

#[derive(Serialize)]
pub struct TodayResp {
    pub date: String,
    pub items: Vec<TodayItemDto>,
}
#[derive(Serialize)]
pub struct TodayItemDto {
    pub time: String,
    pub state: String,
    pub text: String,
    pub doc_ref: String,
}

/// MVP: returns recent activity rows as today's items. Modules can override later via `Module::today_items`.
pub async fn handler(State(state): State<AppState>) -> Result<Json<TodayResp>, ApiError> {
    let today = time::OffsetDateTime::now_utc().date();
    let date = format!("{}", today);
    let rows: Vec<(i64, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT occurred_at, module, doc_id, summary, link_doc
           FROM activity
          WHERE occurred_at >= unixepoch('now','-1 day')
          ORDER BY occurred_at DESC
          LIMIT 50"
    )
    .fetch_all(&state.db)
    .await?;
    let items = rows.into_iter().map(|(ts, module, doc_id, summary, _link)| TodayItemDto {
        time: fmt_ts_hm(Some(ts)),
        state: "pending".into(),
        text: format!("{} · {}", module, summary),
        doc_ref: doc_id,
    }).collect();
    Ok(Json(TodayResp { date, items }))
}
