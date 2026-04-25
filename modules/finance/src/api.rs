use crate::model::Txn;
use axum::{extract::State, routing::{post}, Extension, Json, Router};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use ep_auth::{AuthPat, pat::require_scope};
use ep_core::{AppState, NotifyMessage, Severity};
use serde::{Deserialize, Serialize};

pub fn open_api(_state: AppState) -> Router<AppState> {
    Router::new().route("/txn", post(post_txn).get(list_txn))
}

#[derive(Debug, Deserialize)]
pub struct TxnInput {
    pub merchant: String,
    pub category_code: String,
    pub account_code: String,
    pub amount: f64,
    pub tag: String,
    pub note: Option<String>,
    pub linked_doc_id: Option<String>,
    pub occurred_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TxnCreated { pub doc_id: String }

async fn post_txn(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Json(input): Json<TxnInput>,
) -> Result<Json<TxnCreated>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    if input.merchant.trim().is_empty() {
        return Err(error_json(StatusCode::BAD_REQUEST, "bad_request", "merchant is required"));
    }
    let occurred = input.occurred_at.unwrap_or_else(|| time::OffsetDateTime::now_utc().unix_timestamp());

    let mut tx = state.db.begin().await.map_err(|e| db_err(&e))?;
    let doc_id = ep_core::next_doc_id(&mut tx, "FIN", ep_core::DocIdShape::YearSerial5)
        .await.map_err(|e| db_err(&e))?;
    sqlx::query(
        "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )
    .bind(&doc_id).bind(occurred).bind(&input.merchant).bind(&input.category_code)
    .bind(&input.account_code).bind(input.amount).bind(&input.tag)
    .bind(&input.note).bind(&input.linked_doc_id)
    .execute(&mut *tx).await.map_err(|e| db_err(&e))?;
    sqlx::query("UPDATE fin_account SET balance = balance + ?1 WHERE code = ?2")
        .bind(input.amount).bind(&input.account_code)
        .execute(&mut *tx).await.map_err(|e| db_err(&e))?;
    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc)
         VALUES (?1, 'FIN', ?2, ?3, ?4, ?5)"
    )
    .bind(occurred).bind(&doc_id).bind(&input.merchant).bind(input.amount).bind(&input.linked_doc_id)
    .execute(&mut *tx).await.map_err(|e| db_err(&e))?;
    if let Some(link) = &input.linked_doc_id {
        sqlx::query("INSERT OR IGNORE INTO module_link (source_doc, target_doc, kind) VALUES (?1, ?2, 'ref')")
            .bind(&doc_id).bind(link).execute(&mut *tx).await.map_err(|e| db_err(&e))?;
    }
    tx.commit().await.map_err(|e| db_err(&e))?;

    if input.amount < -500.0 {
        let n = NotifyMessage {
            severity: Severity::Warn,
            module: Some("FIN".into()),
            title: format!("大额支出 · {}", input.merchant),
            body: Some(format!("¥{:.2} ({})", input.amount.abs(), input.category_code)),
            link: Some("/finance".into()),
            doc_ref: Some(doc_id.clone()),
        };
        let _ = state.notify.dispatch(n).await;
    }

    Ok(Json(TxnCreated { doc_id }))
}

async fn list_txn(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<Txn>>, Response> {
    if let Err(r) = require_scope(&pat, "fin:read") { return Err(r); }
    let rows: Vec<(String, i64, String, String, String, f64, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT doc_id, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id
           FROM fin_txn ORDER BY occurred_at DESC LIMIT 50"
    ).fetch_all(&state.db).await.map_err(|e| db_err(&e))?;
    let txns = rows.into_iter().map(|r| Txn {
        doc_id: r.0, occurred_at: r.1, merchant: r.2, category_code: r.3, account_code: r.4,
        amount: r.5, tag: r.6, note: r.7, linked_doc_id: r.8,
    }).collect();
    Ok(Json(txns))
}

fn db_err<E: std::fmt::Display>(e: &E) -> Response {
    error_json(StatusCode::INTERNAL_SERVER_ERROR, "internal", &e.to_string())
}

fn error_json(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        format!(r#"{{"error":{{"code":"{}","message":"{}"}}}}"#, code, message.replace('"', "\\\"")),
    ).into_response()
}
