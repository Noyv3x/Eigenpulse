use crate::model::{Account, Category, Tag, Txn};
use crate::server_fns::{
    add_transfer_inner, archive_account_inner, archive_category_inner,
    create_account_inner, create_category_inner, list_accounts_inner,
    set_budget_inner, update_account_inner, update_category_inner,
    update_txn_inner, UpdateTxnFields,
};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Extension, Json, Router};
use ep_auth::{pat::require_scope, AuthPat};
use ep_core::{AppState, NotifyMessage, Severity};
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

pub fn open_api(_state: AppState) -> Router<AppState> {
    // axum 0.7 / matchit 0.7 uses `:param`; the `{param}` form is axum 0.8.
    Router::new()
        .route("/txn", post(post_txn).get(list_txn))
        .route("/txn/:doc_id", delete(delete_txn).patch(patch_txn))
        .route("/transfer", post(post_transfer))
        .route("/account", get(list_account).post(post_account))
        .route("/account/:code", patch(patch_account))
        .route("/account/:code/archive", post(post_account_archive))
        .route("/category", get(list_category).post(post_category))
        .route("/category/:code", patch(patch_category))
        .route("/category/:code/archive", post(post_category_archive))
        .route("/budget", get(list_budget).post(post_budget))
        .route("/budget/:period/:category_code", delete(delete_budget))
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
    let existed = crate::server_fns::delete_txn_inner(&state.db, &doc_id)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "fin delete_txn helper error");
            error_json(StatusCode::INTERNAL_SERVER_ERROR, "internal", "database error")
        })?;
    if !existed {
        return Err(error_json(
            StatusCode::NOT_FOUND,
            "not_found",
            &format!("no fin_txn with doc_id '{}'", doc_id),
        ));
    }
    Ok(Json(TxnDeleted { doc_id }))
}

// ---------------------------------------------------------------------------
// Txn PATCH + Transfer
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PatchTxnInput {
    pub merchant: Option<String>,
    pub category_code: Option<String>,
    pub account_code: Option<String>,
    pub amount: Option<f64>,
    pub note: Option<String>,
    pub occurred_at: Option<String>,
    pub linked_doc_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct TxnUpdated { doc_id: String }

async fn patch_txn(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
    Json(input): Json<PatchTxnInput>,
) -> Result<Json<TxnUpdated>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    type Row = (String, String, String, f64, Option<String>, Option<String>);
    let cur: Option<Row> = sqlx::query_as(
        "SELECT merchant, category_code, account_code, amount, note, linked_doc_id
           FROM fin_txn WHERE doc_id = ?1"
    ).bind(&doc_id).fetch_optional(&state.db).await.map_err(db_err_response)?;
    let Some((cm, cc, cac, ca, cn, cl)) = cur else {
        return Err(error_json(StatusCode::NOT_FOUND, "not_found",
            &format!("交易 '{doc_id}' 不存在")));
    };
    // For Optional<String> fields like note/linked_doc_id we can't tell
    // "field omitted" from "field set to empty string" in JSON. Convention:
    // missing key → keep current value; empty string → clear (note=NULL,
    // linked=NULL). Implement by inspecting input.note: client sending null
    // is the same as omission via Option<String>; sending "" clears.
    let new_note: Option<String> = match input.note {
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
        None => cn,
    };
    let new_linked: Option<String> = match input.linked_doc_id {
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
        None => cl,
    };
    let fields = UpdateTxnFields {
        merchant: input.merchant.unwrap_or(cm),
        category_code: input.category_code.unwrap_or(cc),
        account_code: input.account_code.unwrap_or(cac),
        amount: input.amount.unwrap_or(ca),
        note: new_note,
        occurred_at_input: input.occurred_at.unwrap_or_default(),
        linked_doc_id: new_linked,
    };
    update_txn_inner(&state.db, &doc_id, fields)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(TxnUpdated { doc_id }))
}

#[derive(Debug, Deserialize)]
struct TransferInput {
    pub from_account: String,
    pub to_account: String,
    pub amount: f64,
    pub note: Option<String>,
    pub occurred_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct TransferCreated { from_doc: String, to_doc: String }

async fn post_transfer(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Json(input): Json<TransferInput>,
) -> Result<Json<TransferCreated>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    // Validation (FK + archived + finite + distinct + non-empty) lives in
    // add_transfer_inner, so this handler is just request-shape mapping.
    let occurred_input = input.occurred_at.unwrap_or_default();
    let occurred = crate::server_fns::parse_occurred_at(&state.db, &occurred_input)
        .await
        .map_err(server_err_to_response)?
        .unwrap_or_else(|| time::OffsetDateTime::now_utc().unix_timestamp());

    let note = input.note.as_deref();
    let (from_txn, to_txn) = add_transfer_inner(
        &state.db, &input.from_account, &input.to_account, input.amount, note, occurred,
    ).await.map_err(server_err_to_response)?;
    Ok(Json(TransferCreated {
        from_doc: from_txn.doc_id,
        to_doc: to_txn.doc_id,
    }))
}

// ---------------------------------------------------------------------------
// Account routes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListAccountQuery {
    /// Wire form: `0` or `1`. Anything truthy enables archived.
    include_archived: Option<u8>,
}

async fn list_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Query(q): Query<ListAccountQuery>,
) -> Result<Json<Vec<Account>>, Response> {
    if let Err(r) = require_scope(&pat, "fin:read") { return Err(r); }
    let include = q.include_archived.unwrap_or(0) != 0;
    let rows = list_accounts_inner(&state.db, include)
        .await
        .map_err(db_err_response)?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct CreateAccountInput {
    pub code: String,
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub tone: String,
    pub opening_balance: f64,
}

#[derive(Debug, Serialize)]
struct AccountCreated { code: String }

async fn post_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Json(input): Json<CreateAccountInput>,
) -> Result<Json<AccountCreated>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    let acc = create_account_inner(
        &state.db,
        input.code.clone(),
        input.name,
        input.r#type,
        input.tone,
        input.opening_balance,
    ).await.map_err(server_err_to_response)?;
    Ok(Json(AccountCreated { code: acc.code }))
}

#[derive(Debug, Deserialize)]
struct PatchAccountInput {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub r#type: Option<String>,
    pub tone: Option<String>,
}

async fn patch_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(code): Path<String>,
    Json(input): Json<PatchAccountInput>,
) -> Result<Json<Account>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    type Row = (String, String, String);
    let cur: Option<Row> = sqlx::query_as(
        "SELECT name, type, tone FROM fin_account WHERE code = ?1"
    ).bind(&code).fetch_optional(&state.db).await.map_err(db_err_response)?;
    let Some((cur_name, cur_type, cur_tone)) = cur else {
        return Err(error_json(StatusCode::NOT_FOUND, "not_found",
            &format!("账户 '{code}' 不存在")));
    };
    let acc = update_account_inner(
        &state.db,
        code,
        input.name.unwrap_or(cur_name),
        input.r#type.unwrap_or(cur_type),
        input.tone.unwrap_or(cur_tone),
    ).await.map_err(server_err_to_response)?;
    Ok(Json(acc))
}

#[derive(Debug, Deserialize)]
struct ArchiveInput { archived: bool }

#[derive(Debug, Serialize)]
struct ArchiveResult { code: String, archived: bool }

async fn post_account_archive(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(code): Path<String>,
    Json(input): Json<ArchiveInput>,
) -> Result<Json<ArchiveResult>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    archive_account_inner(&state.db, code.clone(), input.archived)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(ArchiveResult { code, archived: input.archived }))
}

// ---------------------------------------------------------------------------
// Category routes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListCategoryQuery {
    include_archived: Option<u8>,
}

async fn list_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Query(q): Query<ListCategoryQuery>,
) -> Result<Json<Vec<Category>>, Response> {
    if let Err(r) = require_scope(&pat, "fin:read") { return Err(r); }
    let include = q.include_archived.unwrap_or(0) != 0;
    let flag: i64 = if include { 1 } else { 0 };
    type Row = (String, String, String, i64, bool, i64);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT code, name, tone, sort_order, archived, created_at
           FROM fin_category
          WHERE ?1 = 1 OR archived = 0
          ORDER BY sort_order ASC, code ASC"
    ).bind(flag).fetch_all(&state.db).await.map_err(db_err_response)?;
    let cats = rows.into_iter().map(|r| Category {
        code: r.0, name: r.1, tone: r.2, sort_order: r.3,
        archived: r.4, created_at: r.5,
    }).collect();
    Ok(Json(cats))
}

#[derive(Debug, Deserialize)]
struct CreateCategoryInput {
    pub code: String,
    pub name: String,
    pub tone: String,
    pub sort_order: i64,
}

#[derive(Debug, Serialize)]
struct CategoryCreated { code: String }

async fn post_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Json(input): Json<CreateCategoryInput>,
) -> Result<Json<CategoryCreated>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    let cat = create_category_inner(
        &state.db,
        input.code,
        input.name,
        input.tone,
        input.sort_order,
    ).await.map_err(server_err_to_response)?;
    Ok(Json(CategoryCreated { code: cat.code }))
}

#[derive(Debug, Deserialize)]
struct PatchCategoryInput {
    pub name: Option<String>,
    pub tone: Option<String>,
    pub sort_order: Option<i64>,
}

async fn patch_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(code): Path<String>,
    Json(input): Json<PatchCategoryInput>,
) -> Result<Json<Category>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    type Row = (String, String, i64);
    let cur: Option<Row> = sqlx::query_as(
        "SELECT name, tone, sort_order FROM fin_category WHERE code = ?1"
    ).bind(&code).fetch_optional(&state.db).await.map_err(db_err_response)?;
    let Some((cur_name, cur_tone, cur_sort)) = cur else {
        return Err(error_json(StatusCode::NOT_FOUND, "not_found",
            &format!("分类 '{code}' 不存在")));
    };
    let cat = update_category_inner(
        &state.db,
        code,
        input.name.unwrap_or(cur_name),
        input.tone.unwrap_or(cur_tone),
        input.sort_order.unwrap_or(cur_sort),
    ).await.map_err(server_err_to_response)?;
    Ok(Json(cat))
}

async fn post_category_archive(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(code): Path<String>,
    Json(input): Json<ArchiveInput>,
) -> Result<Json<ArchiveResult>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    archive_category_inner(&state.db, code.clone(), input.archived)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(ArchiveResult { code, archived: input.archived }))
}

// ---------------------------------------------------------------------------
// Budget routes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListBudgetQuery { period: String }

#[derive(Debug, Serialize)]
struct BudgetRow {
    period: String,
    category_code: String,
    amount: f64,
}

async fn list_budget(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Query(q): Query<ListBudgetQuery>,
) -> Result<Json<Vec<BudgetRow>>, Response> {
    if let Err(r) = require_scope(&pat, "fin:read") { return Err(r); }
    type Row = (String, String, f64);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT period, category_code, amount
           FROM fin_budget WHERE period = ?1 ORDER BY category_code"
    ).bind(&q.period).fetch_all(&state.db).await.map_err(db_err_response)?;
    let out = rows.into_iter().map(|r| BudgetRow {
        period: r.0, category_code: r.1, amount: r.2,
    }).collect();
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
struct PostBudgetInput {
    period: String,
    category_code: String,
    amount: f64,
}

async fn post_budget(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Json(input): Json<PostBudgetInput>,
) -> Result<Json<BudgetRow>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    set_budget_inner(&state.db, &input.period, &input.category_code, input.amount)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(BudgetRow {
        period: input.period,
        category_code: input.category_code,
        amount: input.amount,
    }))
}

#[derive(Debug, Serialize)]
struct BudgetDeleted { period: String, category_code: String }

async fn delete_budget(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path((period, category_code)): Path<(String, String)>,
) -> Result<Json<BudgetDeleted>, Response> {
    if let Err(r) = require_scope(&pat, "fin:write") { return Err(r); }
    sqlx::query("DELETE FROM fin_budget WHERE period = ?1 AND category_code = ?2")
        .bind(&period).bind(&category_code)
        .execute(&state.db).await.map_err(db_err_response)?;
    Ok(Json(BudgetDeleted { period, category_code }))
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

/// `Args` → 400 (user-visible message); everything else → logged 500.
fn server_err_to_response(e: ServerFnError) -> Response {
    if let ServerFnError::Args(msg) = &e {
        return error_json(StatusCode::BAD_REQUEST, "bad_request", msg);
    }
    tracing::warn!(error = %e, "fin api helper error");
    error_json(StatusCode::INTERNAL_SERVER_ERROR, "internal", "database error")
}

/// 500 wrapper that logs server-side; the response message stays generic.
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
