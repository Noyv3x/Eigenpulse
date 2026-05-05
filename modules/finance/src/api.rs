use crate::model::{Tag, Txn};
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, post};
use axum::{Extension, Json, Router};
use ep_auth::{pat::require_scope, AuthPat};
use ep_core::{AppState, NotifyMessage, Severity};
use serde::{Deserialize, Serialize};

pub fn open_api(_state: AppState) -> Router<AppState> {
    Router::new()
        .route("/txn", post(post_txn).get(list_txn))
        .route("/txn/{doc_id}", delete(delete_txn))
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

#[derive(Debug, Serialize)]
pub struct TxnDeleted { pub doc_id: String }

async fn post_txn(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Json(input): Json<TxnInput>,
) -> Result<Json<TxnCreated>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    let merchant = input.merchant.trim().to_string();
    if merchant.is_empty() {
        return Err(error_json(StatusCode::BAD_REQUEST, "bad_request", "merchant is required"));
    }
    let tag_kind = match Tag::parse(input.tag.trim()) {
        Some(k) => k,
        None => return Err(error_json(StatusCode::BAD_REQUEST, "bad_request",
            &format!("tag must be one of exp/inc/tfr, got '{}'", input.tag))),
    };
    if !input.amount.is_finite() {
        return Err(error_json(StatusCode::BAD_REQUEST, "bad_request",
            "amount must be a finite number"));
    }
    if input.account_code.trim().is_empty() {
        return Err(error_json(StatusCode::BAD_REQUEST, "bad_request", "account_code is required"));
    }
    if input.category_code.trim().is_empty() {
        return Err(error_json(StatusCode::BAD_REQUEST, "bad_request", "category_code is required"));
    }
    let (cat_exists, acc_exists): (i64, i64) = tokio::try_join!(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = ?1)")
            .bind(&input.category_code).fetch_one(&state.db),
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_account WHERE code = ?1 AND archived = 0)")
            .bind(&input.account_code).fetch_one(&state.db),
    ).map_err(db_err_response)?;
    if cat_exists == 0 {
        return Err(error_json(StatusCode::BAD_REQUEST, "bad_request",
            &format!("unknown category_code '{}'", input.category_code)));
    }
    if acc_exists == 0 {
        return Err(error_json(StatusCode::BAD_REQUEST, "bad_request",
            &format!("unknown or archived account_code '{}'", input.account_code)));
    }

    let occurred = input.occurred_at.unwrap_or_else(|| time::OffsetDateTime::now_utc().unix_timestamp());

    let mut tx = state.db.begin().await.map_err(db_err_response)?;
    let doc_id = ep_core::next_doc_id(&mut tx, "FIN", ep_core::DocIdShape::YearSerial5)
        .await.map_err(db_err_response)?;
    sqlx::query(
        "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )
    .bind(&doc_id).bind(occurred).bind(&merchant).bind(&input.category_code)
    .bind(&input.account_code).bind(input.amount).bind(tag_kind.as_str())
    .bind(&input.note).bind(&input.linked_doc_id)
    .execute(&mut *tx).await.map_err(db_err_response)?;
    sqlx::query("UPDATE fin_account SET balance = balance + ?1 WHERE code = ?2")
        .bind(input.amount).bind(&input.account_code)
        .execute(&mut *tx).await.map_err(db_err_response)?;
    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc)
         VALUES (?1, 'FIN', ?2, ?3, ?4, ?5)"
    )
    .bind(occurred).bind(&doc_id).bind(&merchant).bind(input.amount).bind(&input.linked_doc_id)
    .execute(&mut *tx).await.map_err(db_err_response)?;
    if let Some(link) = &input.linked_doc_id {
        sqlx::query("INSERT OR IGNORE INTO module_link (source_doc, target_doc, kind) VALUES (?1, ?2, 'ref')")
            .bind(&doc_id).bind(link).execute(&mut *tx).await.map_err(db_err_response)?;
    }
    tx.commit().await.map_err(db_err_response)?;

    if input.amount < -500.0 {
        let n = NotifyMessage {
            severity: Severity::Warn,
            module: Some("FIN".into()),
            title: format!("大额支出 · {}", merchant),
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
    ).fetch_all(&state.db).await.map_err(db_err_response)?;
    let txns = rows.into_iter().map(|r| Txn {
        doc_id: r.0, occurred_at: r.1, merchant: r.2, category_code: r.3, account_code: r.4,
        amount: r.5, tag: r.6, note: r.7, linked_doc_id: r.8,
    }).collect();
    Ok(Json(txns))
}

async fn delete_txn(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
) -> Result<Json<TxnDeleted>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    let mut tx = state.db.begin().await.map_err(db_err_response)?;
    // Match the internal server fn: revert the balance change in the same
    // transaction as the row deletion so concurrent reads can't observe a
    // half-rolled-back state.
    let row: Option<(f64, String)> = sqlx::query_as(
        "SELECT amount, account_code FROM fin_txn WHERE doc_id = ?1"
    ).bind(&doc_id).fetch_optional(&mut *tx).await.map_err(db_err_response)?;
    let Some((amount, account_code)) = row else {
        return Err(error_json(StatusCode::NOT_FOUND, "not_found",
            &format!("no fin_txn with doc_id '{}'", doc_id)));
    };
    sqlx::query("UPDATE fin_account SET balance = balance - ?1 WHERE code = ?2")
        .bind(amount).bind(&account_code)
        .execute(&mut *tx).await.map_err(db_err_response)?;
    sqlx::query("DELETE FROM fin_txn WHERE doc_id = ?1")
        .bind(&doc_id).execute(&mut *tx).await.map_err(db_err_response)?;
    sqlx::query("DELETE FROM activity WHERE module = 'FIN' AND doc_id = ?1")
        .bind(&doc_id).execute(&mut *tx).await.map_err(db_err_response)?;
    sqlx::query("DELETE FROM module_link WHERE source_doc = ?1")
        .bind(&doc_id).execute(&mut *tx).await.map_err(db_err_response)?;
    tx.commit().await.map_err(db_err_response)?;
    Ok(Json(TxnDeleted { doc_id }))
}

/// Wraps a Display error in a 500 response. Logs the underlying error
/// server-side so PAT callers don't see raw sqlite messages (which can leak
/// SQL or connection details).
fn db_err_response<E: std::fmt::Display>(e: E) -> Response {
    tracing::warn!(error = %e, "fin api db error");
    error_json(StatusCode::INTERNAL_SERVER_ERROR, "internal", "database error")
}

fn error_json(status: StatusCode, code: &str, message: &str) -> Response {
    let body = serde_json::json!({
        "error": { "code": code, "message": message }
    });
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    ).into_response()
}
