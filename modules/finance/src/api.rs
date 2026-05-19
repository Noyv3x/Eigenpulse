use crate::model::{Account, Category, Currency, Txn, TRANSFER_CATEGORY_CODE};
use crate::server_fns::{
    add_transfer_inner, add_txn_inner, create_account_inner, create_category_inner,
    create_currency_inner, delete_account_inner, delete_category_inner, delete_currency_inner,
    dispatch_large_expense_notification, list_accounts_inner, list_currencies_inner,
    resolve_currency, set_budget_inner, set_primary_currency_inner, update_account_inner_with,
    update_category_inner_with, update_currency_inner, update_txn_inner, AddTransferFields,
    AddTxnFields, UpdateTxnFields,
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
    // Accounts / categories / budgets are keyed per-currency, so their item
    // routes carry the currency segment too.
    Router::<AppState>::new()
        .route("/currency", get(list_currency).post(post_currency))
        .route(
            "/currency/:code",
            patch(patch_currency).delete(delete_currency),
        )
        .route("/currency/:code/primary", post(post_currency_primary))
        .route("/txn", post(post_txn).get(list_txn))
        .route("/txn/:doc_id", delete(delete_txn).patch(patch_txn))
        .route("/transfer", post(post_transfer))
        .route("/account", get(list_account).post(post_account))
        .route(
            "/account/:currency_code/:code",
            patch(patch_account).delete(delete_account),
        )
        .route("/category", get(list_category).post(post_category))
        .route(
            "/category/:currency_code/:code",
            patch(patch_category).delete(delete_category),
        )
        .route("/budget", get(list_budget).post(post_budget))
        .route(
            "/budget/:currency_code/:period/:category_code",
            delete(delete_budget),
        )
}

/// Parse a wire amount string into minor units at `decimals` precision,
/// mapping a bad shape to a 4xx response. The sign is preserved — callers
/// that need a magnitude apply `.abs()`.
#[allow(
    clippy::result_large_err,
    reason = "Response is the module-wide axum error type; boxing it would only complicate ? call sites"
)]
fn parse_api_amount(input: &str, decimals: u8) -> Result<ep_core::MinorAmount, Response> {
    crate::server_fns::parse_signed_minor(input, decimals).map_err(server_err_to_response)
}

// ---------------------------------------------------------------------------
// Currency routes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CurrencyInput {
    pub code: String,
    pub symbol: String,
    pub remark: String,
    pub decimals: i64,
    #[serde(default)]
    pub sort_order: i64,
}

async fn list_currency(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<Currency>>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_READ)?;
    let rows = list_currencies_inner(&state.db)
        .await
        .map_err(db_err_response)?;
    Ok(Json(rows))
}

async fn post_currency(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<CurrencyInput>,
) -> Result<Json<Currency>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let c = create_currency_inner(
        &state.db,
        input.code,
        input.symbol,
        input.remark,
        input.decimals,
        input.sort_order,
    )
    .await
    .map_err(server_err_to_response)?;
    Ok(Json(c))
}

#[derive(Debug, Deserialize)]
struct PatchCurrencyInput {
    pub symbol: Option<String>,
    pub remark: Option<String>,
    pub decimals: Option<i64>,
    pub sort_order: Option<i64>,
}

async fn patch_currency(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(code): Path<String>,
    ApiJson(input): ApiJson<PatchCurrencyInput>,
) -> Result<Json<Currency>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let code = code.trim().to_string();
    type Row = (String, String, i64, i64);
    let cur: Option<Row> = sqlx::query_as(
        "SELECT symbol, remark, decimals, sort_order FROM fin_currency WHERE code = ?1",
    )
    .bind(&code)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err_response)?;
    let Some((cur_symbol, cur_remark, cur_decimals, cur_sort)) = cur else {
        return Err(server_err_to_response(ep_i18n::err_with(
            "finance.err.currency_not_found",
            &code,
        )));
    };
    let c = update_currency_inner(
        &state.db,
        code,
        input.symbol.unwrap_or(cur_symbol),
        input.remark.unwrap_or(cur_remark),
        input.decimals.unwrap_or(cur_decimals),
        input.sort_order.unwrap_or(cur_sort),
    )
    .await
    .map_err(server_err_to_response)?;
    Ok(Json(c))
}

#[derive(Debug, Serialize)]
struct CurrencyDeleted {
    code: String,
}

async fn delete_currency(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(code): Path<String>,
) -> Result<Json<CurrencyDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let code = code.trim().to_string();
    delete_currency_inner(&state.db, code.clone())
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(CurrencyDeleted { code }))
}

#[derive(Debug, Serialize)]
struct CurrencyPrimary {
    code: String,
    is_primary: bool,
}

async fn post_currency_primary(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(code): Path<String>,
) -> Result<Json<CurrencyPrimary>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let code = code.trim().to_string();
    set_primary_currency_inner(&state.db, code.clone())
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(CurrencyPrimary {
        code,
        is_primary: true,
    }))
}

// ---------------------------------------------------------------------------
// Transaction routes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TxnInput {
    /// Currency the account and category live in; empty / omitted → primary.
    #[serde(default)]
    pub currency_code: String,
    pub merchant: String,
    pub category_code: String,
    pub account_code: String,
    /// Pre-signed amount string (e.g. `"-42.50"` for an expense), parsed at
    /// the resolved currency's precision.
    pub amount: String,
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
    let currency = resolve_currency(&state.db, &input.currency_code)
        .await
        .map_err(server_err_to_response)?;
    // `add_txn_inner` accepts pre-signed exp/inc amounts; keep the sign.
    let amount = parse_api_amount(&input.amount, currency.decimals)?;
    let occurred = input.occurred_at.unwrap_or_else(ep_core::unix_now);

    let txn = add_txn_inner(
        &state.db,
        AddTxnFields {
            currency_code: currency.code.clone(),
            merchant: input.merchant,
            category_code: input.category_code,
            account_code: input.account_code,
            amount,
            tag: input.tag,
            note: input.note,
            linked_doc_id: input.linked_doc_id,
            occurred_at: occurred,
        },
    )
    .await
    .map_err(server_err_to_response)?;
    dispatch_large_expense_notification(&state.notify, &currency, &txn).await;

    Ok(Json(TxnCreated { doc_id: txn.doc_id }))
}

#[derive(Debug, Deserialize)]
struct ListTxnQuery {
    currency_code: Option<String>,
}

async fn list_txn(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(q): ApiQuery<ListTxnQuery>,
) -> Result<Json<Vec<Txn>>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_READ)?;
    let currency = resolve_currency(&state.db, q.currency_code.as_deref().unwrap_or(""))
        .await
        .map_err(server_err_to_response)?;
    let txns = sqlx::query_as::<_, Txn>(
        "SELECT doc_id, currency_code, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id
           FROM fin_txn WHERE currency_code = ?1 ORDER BY occurred_at DESC LIMIT 50"
    ).bind(&currency.code).fetch_all(&state.db).await.map_err(db_err_response)?;
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
    /// New amount magnitude (string, parsed at the txn's currency precision);
    /// the sign is re-derived from the immutable `tag`.
    pub amount: Option<String>,
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
    // JOIN to fin_currency for the precision; one round-trip carries both
    // the current row fields and the decimals needed to parse `input.amount`.
    type Row = (
        u8,
        String,
        String,
        String,
        ep_core::MinorAmount,
        Option<String>,
    );
    let cur: Option<Row> = sqlx::query_as(
        "SELECT c.decimals, t.merchant, t.category_code, t.account_code, t.amount, t.note
           FROM fin_txn t
           JOIN fin_currency c ON c.code = t.currency_code
          WHERE t.doc_id = ?1",
    )
    .bind(&doc_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err_response)?;
    let Some((decimals, cm, cc, cac, ca, cn)) = cur else {
        return Err(server_err_to_response(ep_i18n::err_with(
            "finance.err.txn_not_found",
            &doc_id,
        )));
    };
    // `update_txn_inner` wants a positive magnitude; re-sign happens there.
    let amount = match input.amount.as_deref() {
        Some(s) => parse_api_amount(s, decimals)?.abs(),
        None => ca.abs(),
    };
    let new_note = ep_core::apply_nullable_patch(input.note, cn);
    let fields = UpdateTxnFields {
        merchant: input.merchant.unwrap_or(cm),
        category_code: input.category_code.unwrap_or(cc),
        account_code: input.account_code.unwrap_or(cac),
        amount,
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
    /// Currency / account of each leg. Empty currency → primary. The two legs
    /// may live in different currencies — there is no conversion, each side
    /// carries its own amount.
    #[serde(default)]
    pub from_currency: String,
    pub from_account: String,
    #[serde(default)]
    pub to_currency: String,
    pub to_account: String,
    pub from_amount: String,
    pub to_amount: String,
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
    let (from_cur, to_cur) = tokio::try_join!(
        resolve_currency(&state.db, &input.from_currency),
        resolve_currency(&state.db, &input.to_currency),
    )
    .map_err(server_err_to_response)?;
    let from_amount = parse_api_amount(&input.from_amount, from_cur.decimals)?;
    let to_amount = parse_api_amount(&input.to_amount, to_cur.decimals)?;
    let occurred_input = input.occurred_at.unwrap_or_default();
    let occurred = crate::server_fns::parse_occurred_at(&state.db, &occurred_input)
        .await
        .map_err(server_err_to_response)?
        .unwrap_or_else(ep_core::unix_now);

    let (from_txn, to_txn) = add_transfer_inner(
        &state.db,
        AddTransferFields {
            from_currency: from_cur.code,
            from_account: input.from_account,
            to_currency: to_cur.code,
            to_account: input.to_account,
            from_amount,
            to_amount,
            note: input.note,
            occurred_at: occurred,
        },
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

#[derive(Debug, Deserialize)]
struct ListAccountQuery {
    currency_code: Option<String>,
}

async fn list_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(q): ApiQuery<ListAccountQuery>,
) -> Result<Json<Vec<Account>>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_READ)?;
    let currency = resolve_currency(&state.db, q.currency_code.as_deref().unwrap_or(""))
        .await
        .map_err(server_err_to_response)?;
    let rows = list_accounts_inner(&state.db, &currency.code)
        .await
        .map_err(db_err_response)?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct CreateAccountInput {
    #[serde(default)]
    pub currency_code: String,
    pub code: String,
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub tone: String,
    /// Optional opening balance string (may be negative); blank / omitted → 0.
    pub opening_balance: Option<String>,
}

#[derive(Debug, Serialize)]
struct AccountCreated {
    currency_code: String,
    code: String,
}

async fn post_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<CreateAccountInput>,
) -> Result<Json<AccountCreated>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let currency = resolve_currency(&state.db, &input.currency_code)
        .await
        .map_err(server_err_to_response)?;
    let opening_balance = match input.opening_balance.as_deref() {
        Some(s) if !s.trim().is_empty() => parse_api_amount(s, currency.decimals)?,
        _ => ep_core::MinorAmount::ZERO,
    };
    let acc = create_account_inner(
        &state.db,
        currency.code,
        input.code,
        input.name,
        input.r#type,
        input.tone,
        opening_balance,
    )
    .await
    .map_err(server_err_to_response)?;
    Ok(Json(AccountCreated {
        currency_code: acc.currency_code,
        code: acc.code,
    }))
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
    Path((currency_code, code)): Path<(String, String)>,
    ApiJson(input): ApiJson<PatchAccountInput>,
) -> Result<Json<Account>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let currency_code = currency_code.trim().to_string();
    let code = code.trim().to_string();
    type Row = (String, String, String);
    let cur: Option<Row> = sqlx::query_as(
        "SELECT name, type, tone FROM fin_account WHERE currency_code = ?1 AND code = ?2",
    )
    .bind(&currency_code)
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
    // External API consumers addressed the account by its current `code`;
    // renaming the row out from under them would break those callers, so
    // PATCH always leaves the code in place even when the name changes.
    let acc = update_account_inner_with(
        &state.db,
        currency_code,
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
    currency_code: String,
    code: String,
}

async fn delete_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path((currency_code, code)): Path<(String, String)>,
) -> Result<Json<AccountDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let currency_code = currency_code.trim().to_string();
    let code = code.trim().to_string();
    delete_account_inner(&state.db, currency_code.clone(), code.clone())
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(AccountDeleted {
        currency_code,
        code,
    }))
}

// ---------------------------------------------------------------------------
// Category routes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListCategoryQuery {
    currency_code: Option<String>,
}

async fn list_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(q): ApiQuery<ListCategoryQuery>,
) -> Result<Json<Vec<Category>>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_READ)?;
    let currency = resolve_currency(&state.db, q.currency_code.as_deref().unwrap_or(""))
        .await
        .map_err(server_err_to_response)?;
    // The reserved TFR category is module plumbing — never offered through
    // the public API.
    let cats = sqlx::query_as::<_, Category>(
        "SELECT currency_code, code, name, tone, sort_order, archived, created_at
           FROM fin_category
          WHERE currency_code = ?1 AND code <> ?2
          ORDER BY sort_order ASC, code ASC",
    )
    .bind(&currency.code)
    .bind(TRANSFER_CATEGORY_CODE)
    .fetch_all(&state.db)
    .await
    .map_err(db_err_response)?;
    Ok(Json(cats))
}

#[derive(Debug, Deserialize)]
struct CreateCategoryInput {
    #[serde(default)]
    pub currency_code: String,
    pub code: String,
    pub name: String,
    pub tone: String,
    pub sort_order: i64,
}

#[derive(Debug, Serialize)]
struct CategoryCreated {
    currency_code: String,
    code: String,
}

async fn post_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<CreateCategoryInput>,
) -> Result<Json<CategoryCreated>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let currency = resolve_currency(&state.db, &input.currency_code)
        .await
        .map_err(server_err_to_response)?;
    let cat = create_category_inner(
        &state.db,
        currency.code,
        input.code,
        input.name,
        input.tone,
        input.sort_order,
    )
    .await
    .map_err(server_err_to_response)?;
    Ok(Json(CategoryCreated {
        currency_code: cat.currency_code,
        code: cat.code,
    }))
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
    Path((currency_code, code)): Path<(String, String)>,
    ApiJson(input): ApiJson<PatchCategoryInput>,
) -> Result<Json<Category>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let currency_code = currency_code.trim().to_string();
    let code = code.trim().to_string();
    type Row = (String, String, i64);
    let cur: Option<Row> = sqlx::query_as(
        "SELECT name, tone, sort_order FROM fin_category WHERE currency_code = ?1 AND code = ?2",
    )
    .bind(&currency_code)
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
    let cat = update_category_inner_with(
        &state.db,
        currency_code,
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
    currency_code: String,
    code: String,
}

async fn delete_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path((currency_code, code)): Path<(String, String)>,
) -> Result<Json<CategoryDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let currency_code = currency_code.trim().to_string();
    let code = code.trim().to_string();
    delete_category_inner(&state.db, currency_code.clone(), code.clone())
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(CategoryDeleted {
        currency_code,
        code,
    }))
}

// ---------------------------------------------------------------------------
// Budget routes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListBudgetQuery {
    currency_code: Option<String>,
    period: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct BudgetRow {
    currency_code: String,
    period: String,
    category_code: String,
    /// Budgeted amount in the currency's minor units.
    amount: ep_core::MinorAmount,
}

async fn list_budget(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(q): ApiQuery<ListBudgetQuery>,
) -> Result<Json<Vec<BudgetRow>>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_READ)?;
    let currency = resolve_currency(&state.db, q.currency_code.as_deref().unwrap_or(""))
        .await
        .map_err(server_err_to_response)?;
    let period =
        crate::server_fns::normalize_budget_period(&q.period).map_err(server_err_to_response)?;
    let out = sqlx::query_as::<_, BudgetRow>(
        "SELECT currency_code, period, category_code, amount
           FROM fin_budget WHERE currency_code = ?1 AND period = ?2 ORDER BY category_code",
    )
    .bind(&currency.code)
    .bind(&period)
    .fetch_all(&state.db)
    .await
    .map_err(db_err_response)?;
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
struct PostBudgetInput {
    #[serde(default)]
    currency_code: String,
    period: String,
    category_code: String,
    /// Budget amount string; `"0"` / blank removes the budget row.
    amount: String,
}

async fn post_budget(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<PostBudgetInput>,
) -> Result<Json<BudgetRow>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let currency = resolve_currency(&state.db, &input.currency_code)
        .await
        .map_err(server_err_to_response)?;
    let period = crate::server_fns::normalize_budget_period(&input.period)
        .map_err(server_err_to_response)?;
    let category_code = input.category_code.trim().to_string();
    let amount = if input.amount.trim().is_empty() {
        ep_core::MinorAmount::ZERO
    } else {
        parse_api_amount(&input.amount, currency.decimals)?
    };
    set_budget_inner(&state.db, &currency.code, &period, &category_code, amount)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(BudgetRow {
        currency_code: currency.code,
        period,
        category_code,
        amount,
    }))
}

#[derive(Debug, Serialize)]
struct BudgetDeleted {
    currency_code: String,
    period: String,
    category_code: String,
}

async fn delete_budget(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path((currency_code, period, category_code)): Path<(String, String, String)>,
) -> Result<Json<BudgetDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIN_WRITE)?;
    let currency = resolve_currency(&state.db, &currency_code)
        .await
        .map_err(server_err_to_response)?;
    let period =
        crate::server_fns::normalize_budget_period(&period).map_err(server_err_to_response)?;
    let category_code = ep_core::trim_to_option(&category_code).ok_or_else(|| {
        server_err_to_response(ep_i18n::err("finance.err.category_code_required"))
    })?;
    sqlx::query(
        "DELETE FROM fin_budget WHERE currency_code = ?1 AND period = ?2 AND category_code = ?3",
    )
    .bind(&currency.code)
    .bind(&period)
    .bind(&category_code)
    .execute(&state.db)
    .await
    .map_err(db_err_response)?;
    Ok(Json(BudgetDeleted {
        currency_code: currency.code,
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
