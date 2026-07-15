use crate::model::*;
use crate::server_fns::*;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::{Extension, Json, Router};
use ep_auth::{require_scope, AuthPat};
use ep_core::{ApiJson, ApiQuery, AppState, EntityId};
use leptos::server_fn::ServerFnError;
use serde::Deserialize;

pub fn open_api(_state: AppState) -> Router<AppState> {
    Router::<AppState>::new()
        .route("/currencies", get(list_currencies).post(create_currency))
        .route(
            "/currencies/:id",
            patch(update_currency).delete(delete_currency),
        )
        .route("/currencies/:id/primary", post(set_primary_currency))
        .route("/accounts", get(list_accounts).post(create_account))
        .route(
            "/accounts/:id",
            patch(update_account).delete(delete_account),
        )
        .route("/categories", get(list_categories).post(create_category))
        .route(
            "/categories/:id",
            patch(update_category).delete(delete_category),
        )
        .route(
            "/transactions",
            get(list_transactions).post(create_transaction),
        )
        .route(
            "/transactions/:id",
            patch(update_transaction).delete(delete_transaction),
        )
        .route("/transfers", get(list_transfers).post(create_transfer))
        .route("/transfers/:id", delete(delete_transfer))
        .route("/budgets", get(list_budgets).post(upsert_budget))
        .route("/budgets/:id", delete(delete_budget))
        .route("/summary", get(get_summary))
        .route("/reports/months", get(get_months))
        .route("/export.csv", get(export_csv))
}

/// Cookie-session-protected browser downloads. The application merges this
/// router inside its normal web authentication boundary.
pub fn browser_router() -> Router<AppState> {
    Router::<AppState>::new().route("/finance/export.csv", get(download_csv))
}

#[allow(
    clippy::result_large_err,
    reason = "axum handlers use Response as their shared rejection"
)]
fn require_read(pat: &AuthPat) -> Result<(), Response> {
    require_scope(pat, crate::SCOPE_READ)
}

#[allow(
    clippy::result_large_err,
    reason = "axum handlers use Response as their shared rejection"
)]
fn require_write(pat: &AuthPat) -> Result<(), Response> {
    require_scope(pat, crate::SCOPE_WRITE)
}

#[allow(
    clippy::result_large_err,
    reason = "axum handlers use Response as their shared rejection"
)]
fn positive_path_id(id: i64, resource: &str) -> Result<i64, Response> {
    if id > 0 {
        Ok(id)
    } else {
        Err(ep_core::api_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_id",
            format!("{resource} id must be a positive integer"),
        ))
    }
}

fn server_error(error: ServerFnError) -> Response {
    ep_i18n::i18n_error_response(error, "finance open api")
}

fn db_error(error: impl std::fmt::Display) -> Response {
    ep_i18n::db_error_response(error, "finance open api")
}

fn page_error(error: crate::page::PageError) -> Response {
    ep_core::api_error_response(
        StatusCode::BAD_REQUEST,
        "invalid_pagination",
        error.to_string(),
    )
}

fn not_found(resource: &str, id: i64) -> Response {
    ep_core::api_error_response(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("{resource} {id} was not found"),
    )
}

// -------------------------------------------------------------------------
// Currencies
// -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CurrencyInput {
    code: String,
    symbol: String,
    #[serde(default)]
    remark: String,
    decimals: i64,
    #[serde(default)]
    sort_order: i64,
}

async fn list_currencies(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<Currency>>, Response> {
    require_read(&pat)?;
    Ok(Json(
        list_currencies_inner(&state.db)
            .await
            .map_err(server_error)?,
    ))
}

async fn create_currency(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<CurrencyInput>,
) -> Result<(StatusCode, Json<Currency>), Response> {
    require_write(&pat)?;
    let currency = create_currency_inner(
        &state.db,
        input.code,
        input.symbol,
        input.remark,
        input.decimals,
        input.sort_order,
    )
    .await
    .map_err(server_error)?;
    Ok((StatusCode::CREATED, Json(currency)))
}

#[derive(Debug, Deserialize)]
struct CurrencyPatch {
    symbol: Option<String>,
    remark: Option<String>,
    decimals: Option<i64>,
    sort_order: Option<i64>,
}

async fn update_currency(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<CurrencyPatch>,
) -> Result<Json<Currency>, Response> {
    require_write(&pat)?;
    let id = positive_path_id(id, "currency")?;
    Ok(Json(
        patch_currency_inner(
            &state.db,
            id,
            CurrencyPatchFields {
                symbol: input.symbol,
                remark: input.remark,
                decimals: input.decimals,
                sort_order: input.sort_order,
            },
        )
        .await
        .map_err(server_error)?,
    ))
}

async fn delete_currency(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<EntityId>, Response> {
    require_write(&pat)?;
    let id = positive_path_id(id, "currency")?;
    if !delete_currency_inner(&state.db, id)
        .await
        .map_err(server_error)?
    {
        return Err(not_found("currency", id));
    }
    Ok(Json(EntityId::new(id)))
}

async fn set_primary_currency(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<EntityId>, Response> {
    require_write(&pat)?;
    let id = positive_path_id(id, "currency")?;
    set_primary_currency_inner(&state.db, id)
        .await
        .map_err(server_error)?;
    Ok(Json(EntityId::new(id)))
}

// -------------------------------------------------------------------------
// Accounts
// -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct AccountQuery {
    currency_id: Option<i64>,
    #[serde(default)]
    include_archived: bool,
}

async fn list_accounts(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(query): ApiQuery<AccountQuery>,
) -> Result<Json<Vec<Account>>, Response> {
    require_read(&pat)?;
    Ok(Json(
        list_accounts_inner(&state.db, query.currency_id, query.include_archived)
            .await
            .map_err(server_error)?,
    ))
}

#[derive(Debug, Deserialize)]
struct AccountInput {
    currency_id: i64,
    name: String,
    #[serde(rename = "type")]
    account_type: String,
    #[serde(default)]
    tone: String,
    #[serde(default = "zero_amount")]
    opening_balance: String,
}

fn zero_amount() -> String {
    "0".into()
}

async fn create_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<AccountInput>,
) -> Result<(StatusCode, Json<Account>), Response> {
    require_write(&pat)?;
    let currency = resolve_currency(&state.db, input.currency_id)
        .await
        .map_err(server_error)?;
    let opening =
        parse_signed_minor(&input.opening_balance, currency.decimals).map_err(server_error)?;
    let account = create_account_inner(
        &state.db,
        currency.id,
        input.name,
        input.account_type,
        input.tone,
        opening,
    )
    .await
    .map_err(server_error)?;
    Ok((StatusCode::CREATED, Json(account)))
}

#[derive(Debug, Deserialize)]
struct AccountPatch {
    name: Option<String>,
    #[serde(rename = "type")]
    account_type: Option<String>,
    tone: Option<String>,
    archived: Option<bool>,
}

async fn update_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<AccountPatch>,
) -> Result<Json<Account>, Response> {
    require_write(&pat)?;
    let id = positive_path_id(id, "account")?;
    Ok(Json(
        patch_account_inner(
            &state.db,
            id,
            AccountPatchFields {
                name: input.name,
                r#type: input.account_type,
                tone: input.tone,
                archived: input.archived,
            },
        )
        .await
        .map_err(server_error)?,
    ))
}

async fn delete_account(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<EntityId>, Response> {
    require_write(&pat)?;
    let id = positive_path_id(id, "account")?;
    if !delete_account_inner(&state.db, id)
        .await
        .map_err(server_error)?
    {
        return Err(not_found("account", id));
    }
    Ok(Json(EntityId::new(id)))
}

// -------------------------------------------------------------------------
// Categories
// -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CategoryQuery {
    currency_id: i64,
    #[serde(default)]
    include_archived: bool,
}

async fn list_categories(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(query): ApiQuery<CategoryQuery>,
) -> Result<Json<Vec<Category>>, Response> {
    require_read(&pat)?;
    Ok(Json(
        list_categories_inner(&state.db, query.currency_id, query.include_archived)
            .await
            .map_err(server_error)?,
    ))
}

#[derive(Debug, Deserialize)]
struct CategoryInput {
    currency_id: i64,
    name: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    tone: String,
    #[serde(default)]
    sort_order: i64,
}

async fn create_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<CategoryInput>,
) -> Result<(StatusCode, Json<Category>), Response> {
    require_write(&pat)?;
    let category = create_category_inner(
        &state.db,
        input.currency_id,
        input.name,
        input.icon,
        input.tone,
        input.sort_order,
    )
    .await
    .map_err(server_error)?;
    Ok((StatusCode::CREATED, Json(category)))
}

#[derive(Debug, Deserialize)]
struct CategoryPatch {
    name: Option<String>,
    icon: Option<String>,
    tone: Option<String>,
    sort_order: Option<i64>,
    archived: Option<bool>,
}

async fn update_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<CategoryPatch>,
) -> Result<Json<Category>, Response> {
    require_write(&pat)?;
    let id = positive_path_id(id, "category")?;
    Ok(Json(
        patch_category_inner(
            &state.db,
            id,
            CategoryPatchFields {
                name: input.name,
                icon: input.icon,
                tone: input.tone,
                sort_order: input.sort_order,
                archived: input.archived,
            },
        )
        .await
        .map_err(server_error)?,
    ))
}

async fn delete_category(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<EntityId>, Response> {
    require_write(&pat)?;
    let id = positive_path_id(id, "category")?;
    if !delete_category_inner(&state.db, id)
        .await
        .map_err(server_error)?
    {
        return Err(not_found("category", id));
    }
    Ok(Json(EntityId::new(id)))
}

// -------------------------------------------------------------------------
// Transactions
// -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TransactionInput {
    currency_id: i64,
    merchant: String,
    category_id: i64,
    account_id: i64,
    /// Signed major-unit amount. Expenses must be negative.
    amount: String,
    tag: String,
    note: Option<String>,
    occurred_at: Option<i64>,
}

async fn create_transaction(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<TransactionInput>,
) -> Result<(StatusCode, Json<Txn>), Response> {
    require_write(&pat)?;
    let timezone = state.timezone();
    let now = ep_core::unix_now();
    let currency = resolve_currency(&state.db, input.currency_id)
        .await
        .map_err(server_error)?;
    let amount = parse_signed_minor(&input.amount, currency.decimals).map_err(server_error)?;
    let txn = add_txn_inner(
        &state.db,
        timezone,
        AddTxnFields {
            currency_id: currency.id,
            merchant: input.merchant,
            category_id: input.category_id,
            account_id: input.account_id,
            amount,
            tag: input.tag,
            note: input.note,
            occurred_at: input.occurred_at.unwrap_or(now),
        },
    )
    .await
    .map_err(server_error)?;
    dispatch_large_expense_notification(&state.notify, &currency, &txn).await;
    Ok((StatusCode::CREATED, Json(txn)))
}

#[derive(Debug, Deserialize)]
struct TransactionQuery {
    currency_id: i64,
    limit: Option<u32>,
    cursor: Option<String>,
}

async fn list_transactions(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(query): ApiQuery<TransactionQuery>,
) -> Result<Json<crate::page::Page<Txn>>, Response> {
    require_read(&pat)?;
    let currency_id = positive_path_id(query.currency_id, "currency")?;
    let page = crate::page::PageQuery {
        limit: query.limit,
        cursor: query.cursor,
    };
    let limit = page.validated_limit().map_err(page_error)?;
    let scope = format!("finance.transactions:{currency_id}");
    let cursor = page.decode_cursor(&scope).map_err(page_error)?;
    let mut rows = if let Some(cursor) = cursor {
        let cursor_id = cursor.tie_breaker.parse::<i64>().map_err(|_| {
            ep_core::api_error_response(
                StatusCode::BAD_REQUEST,
                "invalid_pagination",
                "invalid transaction cursor",
            )
        })?;
        sqlx::query_as::<_, Txn>(
            "SELECT t.id, t.currency_id, c.code AS currency_code, t.occurred_at,
                    t.occurred_on AS occurred_date,
                    t.merchant, t.category_id, x.name AS category_name, t.account_id,
                    a.name AS account_name, t.amount, t.tag, t.note, t.transfer_id,
                    t.transfer_role, t.created_at, t.updated_at
               FROM fin_txn t
               JOIN fin_currency c ON c.id = t.currency_id
               JOIN fin_account a ON a.id = t.account_id
               LEFT JOIN fin_category x ON x.id = t.category_id
              WHERE t.currency_id = ?1 AND (t.occurred_at, t.id) < (?2, ?3)
              ORDER BY t.occurred_at DESC, t.id DESC LIMIT ?4",
        )
        .bind(currency_id)
        .bind(cursor.sort_value)
        .bind(cursor_id)
        .bind(limit + 1)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?
    } else {
        sqlx::query_as::<_, Txn>(
            "SELECT t.id, t.currency_id, c.code AS currency_code, t.occurred_at,
                    t.occurred_on AS occurred_date,
                    t.merchant, t.category_id, x.name AS category_name, t.account_id,
                    a.name AS account_name, t.amount, t.tag, t.note, t.transfer_id,
                    t.transfer_role, t.created_at, t.updated_at
               FROM fin_txn t
               JOIN fin_currency c ON c.id = t.currency_id
               JOIN fin_account a ON a.id = t.account_id
               LEFT JOIN fin_category x ON x.id = t.category_id
              WHERE t.currency_id = ?1
              ORDER BY t.occurred_at DESC, t.id DESC LIMIT ?2",
        )
        .bind(currency_id)
        .bind(limit + 1)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?
    };
    let has_more = rows.len() > limit as usize;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = has_more.then(|| {
        let last = rows.last().expect("page with overflow has a returned item");
        crate::page::encode_cursor(&scope, last.occurred_at, &last.id.to_string())
    });
    Ok(Json(crate::page::Page::new(rows, next_cursor)))
}

#[derive(Debug, Deserialize)]
struct TransactionPatch {
    merchant: Option<String>,
    category_id: Option<i64>,
    account_id: Option<i64>,
    /// Positive major-unit magnitude; the existing transaction type supplies
    /// the sign.
    amount: Option<String>,
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    note: Option<Option<String>>,
    occurred_at: Option<i64>,
}

async fn update_transaction(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<TransactionPatch>,
) -> Result<Json<Txn>, Response> {
    require_write(&pat)?;
    let timezone = state.timezone();
    let id = positive_path_id(id, "transaction")?;
    Ok(Json(
        patch_txn_inner(
            &state.db,
            timezone,
            id,
            TxnPatchFields {
                merchant: input.merchant,
                category_id: input.category_id,
                account_id: input.account_id,
                amount: input.amount,
                note: input.note,
                occurred_at: input.occurred_at,
            },
        )
        .await
        .map_err(server_error)?,
    ))
}

async fn delete_transaction(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<EntityId>, Response> {
    require_write(&pat)?;
    let id = positive_path_id(id, "transaction")?;
    if !delete_txn_inner(&state.db, id)
        .await
        .map_err(server_error)?
    {
        return Err(not_found("transaction", id));
    }
    Ok(Json(EntityId::new(id)))
}

// -------------------------------------------------------------------------
// Transfers
// -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TransferInput {
    from_account_id: i64,
    to_account_id: i64,
    from_amount: String,
    to_amount: String,
    note: Option<String>,
    occurred_at: Option<i64>,
}

async fn create_transfer(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<TransferInput>,
) -> Result<(StatusCode, Json<Transfer>), Response> {
    require_write(&pat)?;
    let timezone = state.timezone();
    let (from_account, to_account) = tokio::try_join!(
        fetch_account(&state.db, input.from_account_id),
        fetch_account(&state.db, input.to_account_id),
    )
    .map_err(server_error)?;
    let (from_currency, to_currency) = tokio::try_join!(
        resolve_currency(&state.db, from_account.currency_id),
        resolve_currency(&state.db, to_account.currency_id),
    )
    .map_err(server_error)?;
    let from_amount =
        parse_signed_minor(&input.from_amount, from_currency.decimals).map_err(server_error)?;
    let to_amount =
        parse_signed_minor(&input.to_amount, to_currency.decimals).map_err(server_error)?;
    if !from_amount.is_positive() || !to_amount.is_positive() {
        return Err(server_error(ep_i18n::err(
            "finance.err.amount_must_be_positive",
        )));
    }
    let transfer = add_transfer_inner(
        &state.db,
        timezone,
        AddTransferFields {
            from_account_id: input.from_account_id,
            to_account_id: input.to_account_id,
            from_amount,
            to_amount,
            note: input.note,
            occurred_at: input.occurred_at.unwrap_or_else(ep_core::unix_now),
        },
    )
    .await
    .map_err(server_error)?;
    Ok((StatusCode::CREATED, Json(transfer)))
}

#[derive(Debug, Deserialize)]
struct TransferQuery {
    account_id: Option<i64>,
    #[serde(default = "default_list_limit")]
    limit: u32,
}

fn default_list_limit() -> u32 {
    50
}

async fn list_transfers(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(query): ApiQuery<TransferQuery>,
) -> Result<Json<Vec<Transfer>>, Response> {
    require_read(&pat)?;
    let limit = query.limit.clamp(1, crate::page::MAX_PAGE_LIMIT);
    let rows = if let Some(account_id) = query.account_id {
        sqlx::query_as::<_, Transfer>(
            "SELECT f.id, f.occurred_at, f.occurred_on AS occurred_date,
                    f.from_account_id, fa.name AS from_account_name,
                    fa.currency_id AS from_currency_id, fc.code AS from_currency_code,
                    f.to_account_id, ta.name AS to_account_name,
                    ta.currency_id AS to_currency_id, tc.code AS to_currency_code,
                    f.from_amount, f.to_amount, f.note, f.created_at
               FROM fin_transfer f
               JOIN fin_account fa ON fa.id = f.from_account_id
               JOIN fin_currency fc ON fc.id = fa.currency_id
               JOIN fin_account ta ON ta.id = f.to_account_id
               JOIN fin_currency tc ON tc.id = ta.currency_id
              WHERE f.from_account_id = ?1 OR f.to_account_id = ?1
              ORDER BY f.occurred_at DESC, f.id DESC LIMIT ?2",
        )
        .bind(positive_path_id(account_id, "account")?)
        .bind(limit)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?
    } else {
        sqlx::query_as::<_, Transfer>(
            "SELECT f.id, f.occurred_at, f.occurred_on AS occurred_date,
                    f.from_account_id, fa.name AS from_account_name,
                    fa.currency_id AS from_currency_id, fc.code AS from_currency_code,
                    f.to_account_id, ta.name AS to_account_name,
                    ta.currency_id AS to_currency_id, tc.code AS to_currency_code,
                    f.from_amount, f.to_amount, f.note, f.created_at
               FROM fin_transfer f
               JOIN fin_account fa ON fa.id = f.from_account_id
               JOIN fin_currency fc ON fc.id = fa.currency_id
               JOIN fin_account ta ON ta.id = f.to_account_id
               JOIN fin_currency tc ON tc.id = ta.currency_id
              ORDER BY f.occurred_at DESC, f.id DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?
    };
    Ok(Json(rows))
}

async fn delete_transfer(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<EntityId>, Response> {
    require_write(&pat)?;
    let id = positive_path_id(id, "transfer")?;
    if !delete_transfer_inner(&state.db, id)
        .await
        .map_err(server_error)?
    {
        return Err(not_found("transfer", id));
    }
    Ok(Json(EntityId::new(id)))
}

// -------------------------------------------------------------------------
// Budgets, summaries, reports and CSV
// -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct BudgetQuery {
    currency_id: i64,
    period: String,
}

async fn list_budgets(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(query): ApiQuery<BudgetQuery>,
) -> Result<Json<Vec<Budget>>, Response> {
    require_read(&pat)?;
    Ok(Json(
        list_budgets_inner(&state.db, query.currency_id, &query.period)
            .await
            .map_err(server_error)?,
    ))
}

#[derive(Debug, Deserialize)]
struct BudgetInput {
    currency_id: i64,
    period: String,
    category_id: i64,
    amount: String,
}

async fn upsert_budget(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<BudgetInput>,
) -> Result<Json<Option<Budget>>, Response> {
    require_write(&pat)?;
    let currency = resolve_currency(&state.db, input.currency_id)
        .await
        .map_err(server_error)?;
    let amount = parse_signed_minor(&input.amount, currency.decimals).map_err(server_error)?;
    Ok(Json(
        set_budget_inner(
            &state.db,
            currency.id,
            &input.period,
            input.category_id,
            amount,
        )
        .await
        .map_err(server_error)?,
    ))
}

async fn delete_budget(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<EntityId>, Response> {
    require_write(&pat)?;
    let id = positive_path_id(id, "budget")?;
    let result = sqlx::query("DELETE FROM fin_budget WHERE id = ?1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    if result.rows_affected() == 0 {
        return Err(not_found("budget", id));
    }
    Ok(Json(EntityId::new(id)))
}

#[derive(Debug, Deserialize)]
struct CurrencyIdQuery {
    #[serde(default)]
    currency_id: i64,
}

async fn get_summary(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(query): ApiQuery<CurrencyIdQuery>,
) -> Result<Json<MonthSummary>, Response> {
    require_read(&pat)?;
    let timezone = state.timezone();
    let now = ep_core::unix_now();
    let currency = resolve_currency(&state.db, query.currency_id)
        .await
        .map_err(server_error)?;
    Ok(Json(
        load_month_summary(&state.db, timezone, now, currency.id)
            .await
            .map_err(server_error)?,
    ))
}

async fn get_months(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(query): ApiQuery<CurrencyIdQuery>,
) -> Result<Json<Vec<MonthBucket>>, Response> {
    require_read(&pat)?;
    let timezone = state.timezone();
    let now = ep_core::unix_now();
    let currency = resolve_currency(&state.db, query.currency_id)
        .await
        .map_err(server_error)?;
    Ok(Json(
        load_month_buckets_12(&state.db, timezone, now, currency.id)
            .await
            .map_err(server_error)?,
    ))
}

async fn export_csv(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiQuery(query): ApiQuery<CurrencyIdQuery>,
) -> Result<Response, Response> {
    require_read(&pat)?;
    csv_response(&state, query.currency_id).await
}

async fn download_csv(
    State(state): State<AppState>,
    Query(query): Query<CurrencyIdQuery>,
) -> Result<Response, Response> {
    csv_response(&state, query.currency_id).await
}

async fn csv_response(state: &AppState, currency_id: i64) -> Result<Response, Response> {
    let timezone = state.timezone();
    let currency = resolve_currency(&state.db, currency_id)
        .await
        .map_err(server_error)?;
    let csv = export_csv_inner(&state.db, timezone, currency.id)
        .await
        .map_err(server_error)?;
    let mut response = csv.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=finance-{}.csv",
            currency.code
        ))
        .map_err(db_error)?,
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_patch_keeps_three_note_states() {
        let omitted: TransactionPatch = serde_json::from_value(serde_json::json!({})).unwrap();
        let cleared: TransactionPatch =
            serde_json::from_value(serde_json::json!({"note": null})).unwrap();
        let replaced: TransactionPatch =
            serde_json::from_value(serde_json::json!({"note": "memo"})).unwrap();
        assert_eq!(omitted.note, None);
        assert_eq!(cleared.note, Some(None));
        assert_eq!(replaced.note, Some(Some("memo".into())));
    }
}
