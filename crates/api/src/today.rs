use axum::{extract::State, Extension, Json};
use ep_auth::{require_scope, AuthPat};
use ep_core::{fmt_ts_hm, AppState, TodayActivityOrder, TodayActivityRow, SCOPE_ACTIVITY_READ};
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
    pub module: String,
    pub summary: String,
    pub text: String,
    pub doc_ref: String,
    /// Signed finance minor units. Pair with `currency_code` for precision.
    /// Serialized as a string to preserve crypto-scale values.
    pub amount: Option<ep_core::MinorAmount>,
    pub currency_code: Option<String>,
    pub link_doc: Option<String>,
}

/// Returns recent activity rows as today's API items.
pub async fn handler(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<TodayResp>, ApiError> {
    if require_scope(&pat, SCOPE_ACTIVITY_READ).is_err() {
        return Err(ApiError::Forbidden(format!(
            "requires scope: {SCOPE_ACTIVITY_READ}"
        )));
    }
    let today = ep_core::load_today_activity(&state.db, TodayActivityOrder::Desc, Some(50)).await?;
    let items = today.rows.into_iter().map(activity_row_to_item).collect();
    Ok(Json(TodayResp {
        date: today.date,
        items,
    }))
}

fn activity_row_to_item(row: TodayActivityRow) -> TodayItemDto {
    TodayItemDto {
        time: fmt_ts_hm(Some(row.occurred_at)),
        state: row.status.unwrap_or_else(|| "done".into()),
        text: format!("{} · {}", row.module, row.summary),
        module: row.module,
        summary: row.summary,
        doc_ref: row.doc_id,
        amount: row.amount,
        currency_code: row.currency_code,
        link_doc: row.link_doc,
    }
}

#[cfg(test)]
mod tests {
    use super::{activity_row_to_item, handler};
    use crate::errors::ApiError;
    use crate::test_support::{app_state, noop_notify};
    use axum::{extract::State, Extension};
    use ep_auth::AuthPat;
    use ep_core::TodayActivityRow;

    #[test]
    fn activity_row_mapping_preserves_structured_today_fields() {
        let item = activity_row_to_item(TodayActivityRow {
            occurred_at: 1_700_000_000,
            module: "FIN".into(),
            doc_id: "FIN-26001".into(),
            summary: "coffee".into(),
            amount: Some(ep_core::MinorAmount::from(-1850)),
            currency_code: Some("CNY".into()),
            status: None,
            link_doc: Some("FIT-26001".into()),
        });

        assert_eq!(item.time, "22:13");
        assert_eq!(item.state, "done");
        assert_eq!(item.module, "FIN");
        assert_eq!(item.summary, "coffee");
        assert_eq!(item.text, "FIN · coffee");
        assert_eq!(item.doc_ref, "FIN-26001");
        assert_eq!(item.amount, Some(ep_core::MinorAmount::from(-1850)));
        assert_eq!(item.currency_code.as_deref(), Some("CNY"));
        assert_eq!(item.link_doc.as_deref(), Some("FIT-26001"));
    }

    #[tokio::test]
    async fn handler_returns_today_activity_rows_with_structured_fields() {
        let db = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::query(
            "CREATE TABLE activity (
                occurred_at INTEGER NOT NULL,
                module TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                summary TEXT NOT NULL,
                amount TEXT,
                currency_code TEXT,
                status TEXT,
                link_doc TEXT
            )",
        )
        .execute(&db)
        .await
        .expect("schema");
        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, currency_code, status, link_doc)
             VALUES (unixepoch('now'), 'FIN', 'FIN-26001', 'coffee', '-1850', 'CNY', NULL, 'FIT-26001')",
        )
        .execute(&db)
        .await
        .expect("activity");

        let pat = AuthPat {
            id: 1,
            name: "reader".into(),
            scopes: vec![ep_core::SCOPE_ACTIVITY_READ.into()],
        };
        let axum::Json(resp) = handler(State(app_state(db, noop_notify())), Extension(pat))
            .await
            .expect("today response");

        assert_eq!(resp.date.len(), 10);
        assert_eq!(resp.items.len(), 1);
        let item = &resp.items[0];
        assert_eq!(item.state, "done");
        assert_eq!(item.module, "FIN");
        assert_eq!(item.summary, "coffee");
        assert_eq!(item.text, "FIN · coffee");
        assert_eq!(item.doc_ref, "FIN-26001");
        assert_eq!(item.amount, Some(ep_core::MinorAmount::from(-1850)));
        assert_eq!(item.currency_code.as_deref(), Some("CNY"));
        assert_eq!(item.link_doc.as_deref(), Some("FIT-26001"));
    }

    #[tokio::test]
    async fn handler_requires_activity_read_scope() {
        let db = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        let pat = AuthPat {
            id: 1,
            name: "notify-only".into(),
            scopes: vec![ep_core::SCOPE_NOTIFY_WRITE.into()],
        };

        let err = match handler(State(app_state(db, noop_notify())), Extension(pat)).await {
            Ok(_) => panic!("missing activity scope should fail"),
            Err(err) => err,
        };

        assert!(matches!(err, ApiError::Forbidden(_)));
    }
}
