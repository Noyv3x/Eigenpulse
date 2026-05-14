use crate::model::{Account, Category, Txn};
use crate::server_fns::{
    add_transfer_inner, add_txn_inner, create_account_inner, create_category_inner,
    delete_account_inner, delete_category_inner, dispatch_large_expense_notification,
    list_accounts_inner, set_budget_inner, update_account_inner_with, update_category_inner_with,
    update_txn_inner, AddTxnFields, UpdateTxnFields,
};
use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::{delete, get, patch, post};
use axum::{Extension, Json, Router};
use ep_auth::{require_scope, AuthPat};
use ep_core::{ApiJson, ApiQuery, AppState};
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

pub fn open_api(_state: AppState) -> Router<AppState> {
    // axum 0.7 / matchit 0.7 uses `:param`; the `{param}` form is axum 0.8.
    Router::new()
        .route("/txn", post(post_txn).get(list_txn))
        .route("/txn/:doc_id", delete(delete_txn).patch(patch_txn))
        .route("/transfer", post(post_transfer))
        .route("/account", get(list_account).post(post_account))
        .route(
            "/account/:code",
            patch(patch_account).delete(delete_account),
        )
        .route("/category", get(list_category).post(post_category))
        .route(
            "/category/:code",
            patch(patch_category).delete(delete_category),
        )
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
pub struct TxnCreated {
    pub doc_id: String,
}

#[derive(Debug, Serialize)]
pub struct TxnDeleted {
    pub doc_id: String,
}

async fn post_txn(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<TxnInput>,
) -> Result<Json<TxnCreated>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let occurred = input.occurred_at.unwrap_or_else(ep_core::unix_now);

    let txn = add_txn_inner(
        &state.db,
        AddTxnFields {
            merchant: input.merchant,
            category_code: input.category_code,
            account_code: input.account_code,
            amount: input.amount,
            tag: input.tag,
            note: input.note,
            linked_doc_id: input.linked_doc_id,
            occurred_at: occurred,
        },
    )
    .await
    .map_err(server_err_to_response)?;
    dispatch_large_expense_notification(&state.notify, &txn).await;

    Ok(Json(TxnCreated { doc_id: txn.doc_id }))
}

async fn list_txn(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<Txn>>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_READ)?;
    let txns = sqlx::query_as::<_, Txn>(
        "SELECT doc_id, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id
           FROM fin_txn ORDER BY occurred_at DESC LIMIT 50"
    ).fetch_all(&state.db).await.map_err(db_err_response)?;
    Ok(Json(txns))
}

async fn delete_txn(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
) -> Result<Json<TxnDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let doc_id = crate::server_fns::normalize_doc_id(&doc_id).map_err(server_err_to_response)?;
    let existed = crate::server_fns::delete_txn_inner(&state.db, &doc_id)
        .await
        .map_err(db_err_response)?;
    if !existed {
        return Err(server_err_to_response(ep_i18n::err_with(
            "finance.err.txn_not_found",
            &doc_id,
        )));
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
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    pub note: Option<Option<String>>,
    pub occurred_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct TxnUpdated {
    doc_id: String,
}

async fn patch_txn(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
    ApiJson(input): ApiJson<PatchTxnInput>,
) -> Result<Json<TxnUpdated>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let doc_id = crate::server_fns::normalize_doc_id(&doc_id).map_err(server_err_to_response)?;
    type Row = (String, String, String, f64, Option<String>);
    let cur: Option<Row> = sqlx::query_as(
        "SELECT merchant, category_code, account_code, amount, note
           FROM fin_txn WHERE doc_id = ?1",
    )
    .bind(&doc_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err_response)?;
    let Some((cm, cc, cac, ca, cn)) = cur else {
        return Err(server_err_to_response(ep_i18n::err_with(
            "finance.err.txn_not_found",
            &doc_id,
        )));
    };
    let new_note: Option<String> = match input.note {
        Some(Some(s)) => Some(s),
        Some(None) => None,
        None => cn,
    };
    let fields = UpdateTxnFields {
        merchant: input.merchant.unwrap_or(cm),
        category_code: input.category_code.unwrap_or(cc),
        account_code: input.account_code.unwrap_or(cac),
        amount: input.amount.unwrap_or(ca),
        note: new_note,
        occurred_at_input: input.occurred_at.unwrap_or_default(),
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
struct TransferCreated {
    from_doc: String,
    to_doc: String,
}

async fn post_transfer(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<TransferInput>,
) -> Result<Json<TransferCreated>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    // Validation (FK + finite + distinct + non-empty) lives in
    // add_transfer_inner, so this handler is just request-shape mapping.
    let occurred_input = input.occurred_at.unwrap_or_default();
    let occurred = crate::server_fns::parse_occurred_at(&state.db, &occurred_input)
        .await
        .map_err(server_err_to_response)?
        .unwrap_or_else(ep_core::unix_now);

    let note = input.note.as_deref();
    let (from_txn, to_txn) = add_transfer_inner(
        &state.db,
        &input.from_account,
        &input.to_account,
        input.amount,
        note,
        occurred,
    )
    .await
    .map_err(server_err_to_response)?;
    Ok(Json(TransferCreated {
        from_doc: from_txn.doc_id,
        to_doc: to_txn.doc_id,
    }))
}

// ---------------------------------------------------------------------------
// Account routes
// ---------------------------------------------------------------------------

async fn list_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<Account>>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_READ)?;
    let rows = list_accounts_inner(&state.db)
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
struct AccountCreated {
    code: String,
}

async fn post_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<CreateAccountInput>,
) -> Result<Json<AccountCreated>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let acc = create_account_inner(
        &state.db,
        input.code.clone(),
        input.name,
        input.r#type,
        input.tone,
        input.opening_balance,
    )
    .await
    .map_err(server_err_to_response)?;
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
    ApiJson(input): ApiJson<PatchAccountInput>,
) -> Result<Json<Account>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let code = code.trim().to_string();
    type Row = (String, String, String);
    let cur: Option<Row> =
        sqlx::query_as("SELECT name, type, tone FROM fin_account WHERE code = ?1")
            .bind(&code)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err_response)?;
    let Some((cur_name, cur_type, cur_tone)) = cur else {
        return Err(server_err_to_response(ep_i18n::err_with(
            "finance.err.account_not_found",
            &code,
        )));
    };
    // External API consumers (PATs / Shortcuts) addressed the account by
    // its current `code`; renaming the row out from under them would break
    // those callers, so PATCH always leaves the code in place even when
    // the name changes. The UI's server fn uses the rename-enabled wrapper
    // — see `update_account_inner`.
    let acc = update_account_inner_with(
        &state.db,
        code,
        input.name.unwrap_or(cur_name),
        input.r#type.unwrap_or(cur_type),
        input.tone.unwrap_or(cur_tone),
        false,
    )
    .await
    .map_err(server_err_to_response)?;
    Ok(Json(acc))
}

#[derive(Debug, Serialize)]
struct AccountDeleted {
    code: String,
}

async fn delete_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(code): Path<String>,
) -> Result<Json<AccountDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let code = code.trim().to_string();
    delete_account_inner(&state.db, code.clone())
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(AccountDeleted { code }))
}

// ---------------------------------------------------------------------------
// Category routes
// ---------------------------------------------------------------------------

async fn list_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<Category>>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_READ)?;
    let cats = sqlx::query_as::<_, Category>(
        "SELECT code, name, tone, sort_order, archived, created_at
           FROM fin_category
          ORDER BY sort_order ASC, code ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_err_response)?;
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
struct CategoryCreated {
    code: String,
}

async fn post_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<CreateCategoryInput>,
) -> Result<Json<CategoryCreated>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let cat = create_category_inner(
        &state.db,
        input.code,
        input.name,
        input.tone,
        input.sort_order,
    )
    .await
    .map_err(server_err_to_response)?;
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
    ApiJson(input): ApiJson<PatchCategoryInput>,
) -> Result<Json<Category>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let code = code.trim().to_string();
    type Row = (String, String, i64);
    let cur: Option<Row> =
        sqlx::query_as("SELECT name, tone, sort_order FROM fin_category WHERE code = ?1")
            .bind(&code)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err_response)?;
    let Some((cur_name, cur_tone, cur_sort)) = cur else {
        return Err(server_err_to_response(ep_i18n::err_with(
            "finance.err.category_not_found",
            &code,
        )));
    };
    // See `update_account` handler — same rationale: external API callers
    // get a stable resource key, the UI gets cascade renaming.
    let cat = update_category_inner_with(
        &state.db,
        code,
        input.name.unwrap_or(cur_name),
        input.tone.unwrap_or(cur_tone),
        input.sort_order.unwrap_or(cur_sort),
        false,
    )
    .await
    .map_err(server_err_to_response)?;
    Ok(Json(cat))
}

#[derive(Debug, Serialize)]
struct CategoryDeleted {
    code: String,
}

async fn delete_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(code): Path<String>,
) -> Result<Json<CategoryDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let code = code.trim().to_string();
    delete_category_inner(&state.db, code.clone())
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(CategoryDeleted { code }))
}

// ---------------------------------------------------------------------------
// Budget routes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListBudgetQuery {
    period: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct BudgetRow {
    period: String,
    category_code: String,
    amount: f64,
}

async fn list_budget(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(q): ApiQuery<ListBudgetQuery>,
) -> Result<Json<Vec<BudgetRow>>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_READ)?;
    let period =
        crate::server_fns::normalize_budget_period(&q.period).map_err(server_err_to_response)?;
    let out = sqlx::query_as::<_, BudgetRow>(
        "SELECT period, category_code, amount
           FROM fin_budget WHERE period = ?1 ORDER BY category_code",
    )
    .bind(&period)
    .fetch_all(&state.db)
    .await
    .map_err(db_err_response)?;
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
    ApiJson(input): ApiJson<PostBudgetInput>,
) -> Result<Json<BudgetRow>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let period = crate::server_fns::normalize_budget_period(&input.period)
        .map_err(server_err_to_response)?;
    let category_code = input.category_code.trim().to_string();
    set_budget_inner(&state.db, &period, &category_code, input.amount)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(BudgetRow {
        period,
        category_code,
        amount: input.amount,
    }))
}

#[derive(Debug, Serialize)]
struct BudgetDeleted {
    period: String,
    category_code: String,
}

async fn delete_budget(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path((period, category_code)): Path<(String, String)>,
) -> Result<Json<BudgetDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let period =
        crate::server_fns::normalize_budget_period(&period).map_err(server_err_to_response)?;
    let category_code = ep_core::trim_to_option(&category_code).ok_or_else(|| {
        server_err_to_response(ep_i18n::err("finance.err.category_code_required"))
    })?;
    sqlx::query("DELETE FROM fin_budget WHERE period = ?1 AND category_code = ?2")
        .bind(&period)
        .bind(&category_code)
        .execute(&state.db)
        .await
        .map_err(db_err_response)?;
    Ok(Json(BudgetDeleted {
        period,
        category_code,
    }))
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

// Both helpers delegate to the shared implementation in `ep_i18n::api_error`;
// the `context` label is what distinguishes finance logs from other modules.

fn server_err_to_response(e: ServerFnError) -> Response {
    ep_i18n::i18n_error_response(e, "finance open api")
}

fn db_err_response<E: std::fmt::Display>(e: E) -> Response {
    ep_i18n::db_error_response(e, "finance open api")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_txn_note_distinguishes_omitted_null_and_value() {
        let omitted: PatchTxnInput =
            serde_json::from_value(serde_json::json!({})).expect("omitted note should deserialize");
        assert_eq!(omitted.note, None);

        let cleared: PatchTxnInput = serde_json::from_value(serde_json::json!({"note": null}))
            .expect("null note should deserialize");
        assert_eq!(cleared.note, Some(None));

        let replaced: PatchTxnInput = serde_json::from_value(serde_json::json!({"note": "memo"}))
            .expect("string note should deserialize");
        assert_eq!(replaced.note, Some(Some("memo".into())));
    }
}
