use crate::model::*;
#[cfg(feature = "ssr")]
use ep_core::server_err;
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use sqlx::SqlitePool;

pub(crate) const MAX_TXN_MERCHANT_CHARS: usize = 128;
pub(crate) const MAX_TXN_NOTE_CHARS: usize = 2_000;
#[cfg(feature = "ssr")]
pub(crate) const MIN_ACCOUNT_CODE_CHARS: usize = 2;
#[cfg(feature = "ssr")]
pub(crate) const MAX_ACCOUNT_CODE_CHARS: usize = 16;
pub(crate) const MAX_ACCOUNT_NAME_CHARS: usize = 64;
#[cfg(feature = "ssr")]
pub(crate) const MAX_CATEGORY_CODE_CHARS: usize = 8;
pub(crate) const MAX_CATEGORY_NAME_CHARS: usize = 32;
pub(crate) const MAX_CATEGORY_ICON_CHARS: usize = 16;
#[cfg(feature = "ssr")]
pub(crate) const MIN_CURRENCY_CODE_CHARS: usize = 2;
#[cfg(feature = "ssr")]
pub(crate) const MAX_CURRENCY_CODE_CHARS: usize = 8;
#[cfg(feature = "ssr")]
pub(crate) const MAX_CURRENCY_SYMBOL_CHARS: usize = 8;
#[cfg(feature = "ssr")]
pub(crate) const MAX_CURRENCY_REMARK_CHARS: usize = 32;
/// Upper bound on a currency's minor-unit precision. 18 covers ETH / many
/// token ledgers; amounts are stored as exact decimal TEXT and held in i128.
#[cfg(feature = "ssr")]
pub(crate) const MAX_CURRENCY_DECIMALS: u8 = 18;

#[cfg(feature = "ssr")]
#[derive(Debug)]
struct NormalizedTxnFields {
    merchant: String,
    category_code: String,
    account_code: String,
    note: Option<String>,
}

/// Fully-resolved fields for `add_txn_inner`. `amount` is already signed
/// minor units in `currency_code`'s precision; `currency_code` is the
/// currency the account and category both belong to.
#[cfg(feature = "ssr")]
pub struct AddTxnFields {
    pub currency_code: String,
    pub merchant: String,
    pub category_code: String,
    pub account_code: String,
    pub amount: ep_core::MinorAmount,
    pub tag: String,
    pub note: Option<String>,
    pub linked_doc_id: Option<String>,
    pub occurred_at: i64,
}

#[cfg(feature = "ssr")]
fn normalize_txn_fields(
    merchant: &str,
    category_code: &str,
    account_code: &str,
    note: Option<&str>,
) -> Result<NormalizedTxnFields, ServerFnError> {
    let merchant = merchant.trim().to_string();
    if merchant.is_empty() {
        return Err(ep_i18n::err("finance.err.merchant_required"));
    }
    if merchant.chars().count() > MAX_TXN_MERCHANT_CHARS {
        return Err(ep_i18n::err_with(
            "finance.err.merchant_too_long",
            MAX_TXN_MERCHANT_CHARS,
        ));
    }

    let category_code = category_code.trim().to_string();
    if category_code.is_empty() {
        return Err(ep_i18n::err("finance.err.category_code_required"));
    }

    let account_code = account_code.trim().to_string();
    if account_code.is_empty() {
        return Err(ep_i18n::err("finance.err.account_code_required"));
    }

    Ok(NormalizedTxnFields {
        merchant,
        category_code,
        account_code,
        note: normalize_txn_note(note)?,
    })
}

#[cfg(feature = "ssr")]
fn normalize_txn_note(note: Option<&str>) -> Result<Option<String>, ServerFnError> {
    let note = note.and_then(ep_core::trim_to_option);
    if note
        .as_deref()
        .is_some_and(|note| note.chars().count() > MAX_TXN_NOTE_CHARS)
    {
        return Err(ep_i18n::err_with(
            "finance.err.note_too_long",
            MAX_TXN_NOTE_CHARS,
        ));
    }
    Ok(note)
}

#[cfg(feature = "ssr")]
pub(crate) fn normalize_doc_id(doc_id: &str) -> Result<String, ServerFnError> {
    match ep_core::normalize_doc_id_input(doc_id) {
        Ok(doc_id) => Ok(doc_id),
        Err(ep_core::DocIdInputError::Required) => Err(ep_i18n::err("finance.err.doc_id_required")),
        Err(ep_core::DocIdInputError::Invalid(doc_id)) => {
            Err(ep_i18n::err_with("finance.err.doc_id_invalid", &doc_id))
        }
    }
}

#[cfg(feature = "ssr")]
pub(crate) fn normalize_budget_period(period: &str) -> Result<String, ServerFnError> {
    let period = period.trim();
    let Some((year, month)) = period.split_once('-') else {
        return Err(ep_i18n::err_with("finance.err.period_format", period));
    };
    if year.len() != 4
        || month.len() != 2
        || !year.bytes().all(|b| b.is_ascii_digit())
        || !month.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(ep_i18n::err_with("finance.err.period_format", period));
    }
    let month: u8 = month
        .parse()
        .map_err(|_| ep_i18n::err_with("finance.err.period_format", period))?;
    if !(1..=12).contains(&month) {
        return Err(ep_i18n::err_with("finance.err.period_format", period));
    }
    Ok(period.to_string())
}

/// Parse an amount string (e.g. `"42.50"`) into signed-free minor units at
/// `decimals` precision. Rejects blank / malformed input but keeps the sign —
/// API callers (PAT path) pass pre-signed exp/inc amounts. Pairs with
/// [`parse_positive_minor`] for the form path.
#[cfg(feature = "ssr")]
pub(crate) fn parse_signed_minor(
    input: &str,
    decimals: u8,
) -> Result<ep_core::MinorAmount, ServerFnError> {
    ep_core::parse_minor(input, decimals)
        .ok_or_else(|| ep_i18n::err_with("finance.err.amount_invalid", input.trim()))
}

/// As [`parse_signed_minor`] but additionally rejects non-positive values —
/// every amount the UI's `<ActionForm>` submits is a positive magnitude.
#[cfg(feature = "ssr")]
fn parse_positive_minor(input: &str, decimals: u8) -> Result<ep_core::MinorAmount, ServerFnError> {
    let amount = parse_signed_minor(input, decimals)?;
    if !amount.is_positive() {
        return Err(ep_i18n::err("finance.err.amount_must_be_positive"));
    }
    Ok(amount)
}

/// Split a `"{currency_code}/{account_code}"` form value (the option value the
/// transfer picker emits) into its parts. Account codes never contain `/`.
#[cfg(feature = "ssr")]
fn split_currency_ref(value: &str) -> Result<(String, String), ServerFnError> {
    let (currency, code) = value
        .split_once('/')
        .ok_or_else(|| ep_i18n::err_with("finance.err.account_ref_invalid", value))?;
    let currency = currency.trim();
    let code = code.trim();
    if currency.is_empty() || code.is_empty() {
        return Err(ep_i18n::err_with("finance.err.account_ref_invalid", value));
    }
    Ok((currency.to_string(), code.to_string()))
}

#[cfg(feature = "ssr")]
async fn adjust_account_balance(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    currency_code: &str,
    account_code: &str,
    delta: ep_core::MinorAmount,
) -> Result<(), ServerFnError> {
    let current: Option<ep_core::MinorAmount> = sqlx::query_scalar(
        "SELECT balance FROM fin_account WHERE currency_code = ?1 AND code = ?2",
    )
    .bind(currency_code)
    .bind(account_code)
    .fetch_optional(&mut **tx)
    .await
    .map_err(server_err)?;
    let Some(current) = current else {
        return Err(ep_i18n::err_with(
            "finance.err.account_not_found",
            account_code,
        ));
    };
    let next = current
        .checked_add(delta)
        .ok_or_else(|| server_err("finance amount overflow"))?;
    sqlx::query("UPDATE fin_account SET balance = ?1 WHERE currency_code = ?2 AND code = ?3")
        .bind(next)
        .bind(currency_code)
        .bind(account_code)
        .execute(&mut **tx)
        .await
        .map_err(server_err)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Currency resolution + validation
// ---------------------------------------------------------------------------

/// Resolve a currency by code. An empty or unknown code falls back to the
/// primary currency, so a stale / deleted selection degrades gracefully
/// instead of erroring. Errors only when the registry is somehow empty
/// (the 002 migration always seeds one).
#[cfg(feature = "ssr")]
pub async fn resolve_currency(pool: &SqlitePool, code: &str) -> Result<Currency, ServerFnError> {
    let code = code.trim();
    if !code.is_empty() {
        if let Some(c) = sqlx::query_as::<_, Currency>(
            "SELECT code, symbol, remark, decimals, is_primary, sort_order
               FROM fin_currency WHERE code = ?1",
        )
        .bind(code)
        .fetch_optional(pool)
        .await
        .map_err(server_err)?
        {
            return Ok(c);
        }
    }
    sqlx::query_as::<_, Currency>(
        "SELECT code, symbol, remark, decimals, is_primary, sort_order
           FROM fin_currency ORDER BY is_primary DESC, sort_order, code LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(server_err)?
    .ok_or_else(|| ep_i18n::err("finance.err.no_currency"))
}

/// Pure validator for currency input. Returns the trimmed, upper-cased code
/// plus the validated symbol / remark / decimals / sort_order.
#[cfg(feature = "ssr")]
fn validate_currency_input(
    code: &str,
    symbol: &str,
    remark: &str,
    decimals: i64,
    sort_order: i64,
) -> Result<(String, String, String, u8, i64), ServerFnError> {
    let code = code.trim().to_uppercase();
    let symbol = symbol.trim().to_string();
    let remark = remark.trim().to_string();
    if code.len() < MIN_CURRENCY_CODE_CHARS || code.len() > MAX_CURRENCY_CODE_CHARS {
        return Err(ep_i18n::err("finance.err.currency_code_format"));
    }
    if !code
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err(ep_i18n::err("finance.err.currency_code_charset"));
    }
    if symbol.is_empty() || symbol.chars().count() > MAX_CURRENCY_SYMBOL_CHARS {
        return Err(ep_i18n::err("finance.err.currency_symbol_format"));
    }
    if remark.chars().count() > MAX_CURRENCY_REMARK_CHARS {
        return Err(ep_i18n::err_with(
            "finance.err.currency_remark_format",
            MAX_CURRENCY_REMARK_CHARS,
        ));
    }
    if !(0..=i64::from(MAX_CURRENCY_DECIMALS)).contains(&decimals) {
        return Err(ep_i18n::err_with(
            "finance.err.currency_decimals_range",
            MAX_CURRENCY_DECIMALS,
        ));
    }
    if sort_order < 0 {
        return Err(ep_i18n::err("finance.err.currency_sort_order_invalid"));
    }
    Ok((code, symbol, remark, decimals as u8, sort_order))
}

#[cfg(feature = "ssr")]
pub async fn list_currencies_inner(pool: &SqlitePool) -> sqlx::Result<Vec<Currency>> {
    sqlx::query_as::<_, Currency>(
        "SELECT code, symbol, remark, decimals, is_primary, sort_order
           FROM fin_currency ORDER BY is_primary DESC, sort_order, code",
    )
    .fetch_all(pool)
    .await
}

#[cfg(feature = "ssr")]
pub async fn create_currency_inner(
    pool: &SqlitePool,
    code: String,
    symbol: String,
    remark: String,
    decimals: i64,
    sort_order: i64,
) -> Result<Currency, ServerFnError> {
    let (code, symbol, remark, decimals, sort_order) =
        validate_currency_input(&code, &symbol, &remark, decimals, sort_order)?;
    let mut tx = pool.begin().await.map_err(server_err)?;
    let exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_currency WHERE code = ?1)")
            .bind(&code)
            .fetch_one(&mut *tx)
            .await
            .map_err(server_err)?;
    if exists != 0 {
        return Err(ep_i18n::err_with("finance.err.currency_code_taken", &code));
    }
    // The very first currency is primary by default; later ones are not.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_currency")
        .fetch_one(&mut *tx)
        .await
        .map_err(server_err)?;
    let is_primary = i64::from(count == 0);
    let res = sqlx::query(
        "INSERT INTO fin_currency (code, symbol, remark, decimals, is_primary, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&code)
    .bind(&symbol)
    .bind(&remark)
    .bind(decimals)
    .bind(is_primary)
    .bind(sort_order)
    .execute(&mut *tx)
    .await;
    if let Err(e) = res {
        if is_unique_violation(&e) {
            return Err(ep_i18n::err_with("finance.err.currency_code_taken", &code));
        }
        return Err(server_err(e));
    }
    // Every currency owns a transfer category — `add_transfer_inner` files
    // both legs of a transfer under it.
    sqlx::query(
        "INSERT INTO fin_category (currency_code, code, name, tone, sort_order, archived, created_at)
         VALUES (?1, ?2, 'Transfer', '', 999, 0, unixepoch())",
    )
    .bind(&code)
    .bind(TRANSFER_CATEGORY_CODE)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(Currency {
        code,
        symbol,
        remark,
        decimals,
        is_primary: is_primary == 1,
        sort_order,
    })
}

#[cfg(feature = "ssr")]
pub async fn update_currency_inner(
    pool: &SqlitePool,
    code: String,
    symbol: String,
    remark: String,
    decimals: i64,
    sort_order: i64,
) -> Result<Currency, ServerFnError> {
    let (code, symbol, remark, decimals, sort_order) =
        validate_currency_input(&code, &symbol, &remark, decimals, sort_order)?;
    let mut tx = pool.begin().await.map_err(server_err)?;
    let current: Option<i64> =
        sqlx::query_scalar("SELECT decimals FROM fin_currency WHERE code = ?1")
            .bind(&code)
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let Some(current_decimals) = current else {
        return Err(ep_i18n::err_with("finance.err.currency_not_found", &code));
    };
    // Changing the precision would silently rescale every stored minor-unit
    // amount, so it is only allowed while the currency has no accounts yet
    // (and therefore no balances, transactions, or budgets).
    if current_decimals != i64::from(decimals) {
        let has_accounts: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_account WHERE currency_code = ?1)")
                .bind(&code)
                .fetch_one(&mut *tx)
                .await
                .map_err(server_err)?;
        if has_accounts != 0 {
            return Err(ep_i18n::err("finance.err.currency_decimals_locked"));
        }
    }
    sqlx::query(
        "UPDATE fin_currency SET symbol = ?1, remark = ?2, decimals = ?3, sort_order = ?4
          WHERE code = ?5",
    )
    .bind(&symbol)
    .bind(&remark)
    .bind(decimals)
    .bind(sort_order)
    .bind(&code)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    sqlx::query_as::<_, Currency>(
        "SELECT code, symbol, remark, decimals, is_primary, sort_order
           FROM fin_currency WHERE code = ?1",
    )
    .bind(&code)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

/// Make `code` the single primary currency. `is_primary = (code = ?1)` flips
/// exactly one row to `1` and every other row to `0` in one statement.
#[cfg(feature = "ssr")]
pub async fn set_primary_currency_inner(
    pool: &SqlitePool,
    code: String,
) -> Result<(), ServerFnError> {
    let code = code.trim().to_string();
    let mut tx = pool.begin().await.map_err(server_err)?;
    let exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_currency WHERE code = ?1)")
            .bind(&code)
            .fetch_one(&mut *tx)
            .await
            .map_err(server_err)?;
    if exists == 0 {
        return Err(ep_i18n::err_with("finance.err.currency_not_found", &code));
    }
    sqlx::query("UPDATE fin_currency SET is_primary = (code = ?1)")
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(())
}

/// Delete a currency. Refused while it still owns user data (accounts,
/// transactions, budgets, or non-`TFR` categories), while it is the primary
/// currency, or while it is the last currency standing. The auto-provisioned
/// `TFR` category is removed alongside it.
#[cfg(feature = "ssr")]
pub async fn delete_currency_inner(pool: &SqlitePool, code: String) -> Result<(), ServerFnError> {
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(ep_i18n::err("finance.err.currency_code_format"));
    }
    let mut tx = pool.begin().await.map_err(server_err)?;
    let is_primary: Option<bool> =
        sqlx::query_scalar("SELECT is_primary FROM fin_currency WHERE code = ?1")
            .bind(&code)
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let Some(is_primary) = is_primary else {
        return Err(ep_i18n::err_with("finance.err.currency_not_found", &code));
    };
    if is_primary {
        return Err(ep_i18n::err("finance.err.currency_primary_undeletable"));
    }
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_currency")
        .fetch_one(&mut *tx)
        .await
        .map_err(server_err)?;
    if total <= 1 {
        return Err(ep_i18n::err("finance.err.currency_last_undeletable"));
    }
    // One short-circuit `EXISTS … OR EXISTS …` chain instead of four COUNT
    // round-trips. The auto-provisioned TFR category doesn't count as user
    // data, so the category branch excludes it.
    let in_use: i64 = sqlx::query_scalar(
        "SELECT
            EXISTS(SELECT 1 FROM fin_txn      WHERE currency_code = ?1) OR
            EXISTS(SELECT 1 FROM fin_account  WHERE currency_code = ?1) OR
            EXISTS(SELECT 1 FROM fin_budget   WHERE currency_code = ?1) OR
            EXISTS(SELECT 1 FROM fin_category WHERE currency_code = ?1 AND code <> ?2)",
    )
    .bind(&code)
    .bind(TRANSFER_CATEGORY_CODE)
    .fetch_one(&mut *tx)
    .await
    .map_err(server_err)?;
    if in_use != 0 {
        return Err(ep_i18n::err("finance.err.currency_in_use"));
    }
    sqlx::query("DELETE FROM fin_category WHERE currency_code = ?1")
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    sqlx::query("DELETE FROM fin_currency WHERE code = ?1")
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(())
}

#[server(ListCurrencies, "/api/_internal/fin", "Url", "list_currencies")]
pub async fn list_currencies() -> Result<Vec<Currency>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        list_currencies_inner(&state.db).await.map_err(server_err)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(CreateCurrency, "/api/_internal/fin", "Url", "create_currency")]
pub async fn create_currency(
    code: String,
    symbol: String,
    remark: String,
    decimals: i64,
    sort_order: i64,
) -> Result<Currency, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        create_currency_inner(&state.db, code, symbol, remark, decimals, sort_order).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(UpdateCurrency, "/api/_internal/fin", "Url", "update_currency")]
pub async fn update_currency(
    code: String,
    symbol: String,
    remark: String,
    decimals: i64,
    sort_order: i64,
) -> Result<Currency, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        update_currency_inner(&state.db, code, symbol, remark, decimals, sort_order).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    SetPrimaryCurrency,
    "/api/_internal/fin",
    "Url",
    "set_primary_currency"
)]
pub async fn set_primary_currency(code: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        set_primary_currency_inner(&state.db, code).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(DeleteCurrency, "/api/_internal/fin", "Url", "delete_currency")]
pub async fn delete_currency(code: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        delete_currency_inner(&state.db, code).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    #[test]
    fn normalize_doc_id_trims_and_rejects_blank() {
        assert_eq!(normalize_doc_id("  FIN-26092  ").unwrap(), "FIN-26092");
        assert!(normalize_doc_id("   ").is_err());
    }

    #[test]
    fn normalize_doc_id_rejects_invalid_shape() {
        let err = normalize_doc_id("../FIN-26092").expect_err("invalid doc id");

        assert_eq!(
            ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("finance.err.doc_id_invalid", "../FIN-26092"))
        );
    }

    #[test]
    fn normalize_budget_period_accepts_only_real_year_months() {
        assert_eq!(normalize_budget_period(" 2026-05 ").unwrap(), "2026-05");

        for bad in [
            "", "2026", "2026-00", "2026-13", "2026-5", "26-05", "abcd-05",
        ] {
            assert!(normalize_budget_period(bad).is_err(), "bad={bad}");
        }
    }

    #[test]
    fn normalize_txn_fields_enforces_text_lengths() {
        let merchant_err =
            normalize_txn_fields(&"x".repeat(MAX_TXN_MERCHANT_CHARS + 1), "EXP", "ACC", None)
                .expect_err("long merchant should fail");
        assert_eq!(
            ep_i18n::parse_err(&merchant_err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("finance.err.merchant_too_long", "128"))
        );

        let note_err = normalize_txn_fields(
            "Coffee",
            "EXP",
            "ACC",
            Some(&"x".repeat(MAX_TXN_NOTE_CHARS + 1)),
        )
        .expect_err("long note should fail");
        assert_eq!(
            ep_i18n::parse_err(&note_err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("finance.err.note_too_long", "2000"))
        );
    }

    #[test]
    fn normalize_txn_note_trims_and_enforces_length() {
        assert_eq!(
            normalize_txn_note(Some("  memo  ")).unwrap().as_deref(),
            Some("memo")
        );
        assert_eq!(normalize_txn_note(Some("   ")).unwrap(), None);
        assert!(normalize_txn_note(Some(&"x".repeat(MAX_TXN_NOTE_CHARS + 1))).is_err());
    }

    #[test]
    fn validate_account_input_enforces_shared_field_limits() {
        assert!(validate_account_input("A", "Cash", "Cash", "").is_err());
        assert!(validate_account_input(
            &"A".repeat(MAX_ACCOUNT_CODE_CHARS + 1),
            "Cash",
            "Cash",
            ""
        )
        .is_err());
        assert!(
            validate_account_input("ACC", &"x".repeat(MAX_ACCOUNT_NAME_CHARS + 1), "Cash", "")
                .is_err()
        );
        assert_eq!(
            validate_account_input(" ACC ", " Cash ", "Cash", "green").unwrap(),
            (
                "ACC".to_string(),
                "Cash".to_string(),
                "Cash".to_string(),
                "green".to_string()
            )
        );
    }

    #[test]
    fn validate_category_input_enforces_shared_field_limits() {
        // Empty code is now allowed — the helper auto-generates one later.
        assert_eq!(
            validate_category_input("", "Food", "🍜", "").unwrap(),
            (
                "".to_string(),
                "Food".to_string(),
                "🍜".to_string(),
                "".to_string()
            )
        );
        assert!(
            validate_category_input(&"A".repeat(MAX_CATEGORY_CODE_CHARS + 1), "Food", "", "")
                .is_err()
        );
        assert!(
            validate_category_input("FOOD", &"x".repeat(MAX_CATEGORY_NAME_CHARS + 1), "", "")
                .is_err()
        );
        assert!(validate_category_input(
            "FOOD",
            "Food",
            &"x".repeat(MAX_CATEGORY_ICON_CHARS + 1),
            ""
        )
        .is_err());
        assert_eq!(
            validate_category_input(" F&B ", " Food ", " 🍜 ", "amber").unwrap(),
            (
                "F&B".to_string(),
                "Food".to_string(),
                "🍜".to_string(),
                "amber".to_string()
            )
        );
    }

    #[test]
    fn validate_currency_input_enforces_shape_and_ranges() {
        // Code is upper-cased and constrained to [A-Z0-9], 2..=8 chars.
        assert_eq!(
            validate_currency_input(" usd ", "$", "US Dollar", 2, 0).unwrap(),
            (
                "USD".to_string(),
                "$".to_string(),
                "US Dollar".to_string(),
                2,
                0
            )
        );
        assert!(validate_currency_input("U", "$", "x", 2, 0).is_err()); // too short
        assert!(validate_currency_input("US-D", "$", "x", 2, 0).is_err()); // bad charset
        assert!(validate_currency_input("USD", "", "x", 2, 0).is_err()); // empty symbol
        assert_eq!(
            validate_currency_input("USD", "$", "", 2, 0)
                .expect("empty remark is valid")
                .2,
            ""
        );
        assert!(validate_currency_input(
            "USD",
            "$",
            &"x".repeat(MAX_CURRENCY_REMARK_CHARS + 1),
            2,
            0
        )
        .is_err()); // remark too long
        assert!(validate_currency_input("USD", "$", "x", -1, 0).is_err()); // decimals < 0
        assert_eq!(
            validate_currency_input("ETH", "ETH", "Ethereum", 18, 0)
                .unwrap()
                .3,
            18
        );
        assert!(validate_currency_input("USD", "$", "x", 19, 0).is_err()); // decimals > max
        assert!(validate_currency_input("USD", "$", "x", 2, -1).is_err()); // sort < 0
    }

    #[test]
    fn slugify_to_code_handles_ascii_and_chinese() {
        assert_eq!(
            slugify_to_code("Cash Wallet", MAX_ACCOUNT_CODE_CHARS),
            "CASH-WALLET"
        );
        assert_eq!(
            slugify_to_code("My  Savings Acct!!", MAX_ACCOUNT_CODE_CHARS),
            "MY-SAVINGS-ACCT"
        );
        // Chinese-only names produce empty slugs — callers fall back to ACC-N.
        assert_eq!(slugify_to_code("招行储蓄", MAX_ACCOUNT_CODE_CHARS), "");
        // Truncation should not leave a trailing dash.
        assert_eq!(slugify_to_code("Foo-Bar-Baz-Qux", 8), "FOO-BAR");
    }

    #[test]
    fn validate_category_sort_order_rejects_negative_values() {
        assert!(validate_category_sort_order(-1).is_err());
        assert_eq!(validate_category_sort_order(0).unwrap(), 0);
        assert_eq!(validate_category_sort_order(42).unwrap(), 42);
    }

    #[test]
    fn split_currency_ref_splits_on_first_slash() {
        assert_eq!(
            split_currency_ref("CNY/ACC-1").unwrap(),
            ("CNY".to_string(), "ACC-1".to_string())
        );
        assert!(split_currency_ref("ACC-1").is_err());
        assert!(split_currency_ref("/ACC-1").is_err());
        assert!(split_currency_ref("CNY/").is_err());
    }
}

/// `""` → `Ok(None)`; `"YYYY-MM-DD"` → `Ok(Some(ts))` at 12:00 local (noon
/// dodges DST midnight); else `Args`. SQLite handles the localtime → unix
/// conversion because `time/local-offset` isn't enabled in this workspace.
#[cfg(feature = "ssr")]
pub async fn parse_occurred_at(
    pool: &SqlitePool,
    input: &str,
) -> Result<Option<i64>, ServerFnError> {
    let s = input.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if time::Date::parse(s, time::macros::format_description!("[year]-[month]-[day]")).is_err() {
        return Err(ep_i18n::err_with("finance.err.date_format", s));
    }
    let ts: i64 = sqlx::query_scalar("SELECT unixepoch(?1 || ' 12:00:00', 'utc')")
        .bind(s)
        .fetch_one(pool)
        .await
        .map_err(server_err)?;
    Ok(Some(ts))
}

/// One transactional payload for the entire `/finance` page, scoped to a
/// single currency. Bundles every aggregate the view needs so the SSR pass
/// fires a single `tokio::try_join!` instead of N round-trips, and the
/// hydrate side pays one network request per currency switch rather than one
/// per tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerData {
    /// The currency this ledger payload is scoped to (resolved server-side;
    /// an empty / unknown request code resolves to the primary currency).
    pub currency: Currency,
    /// Every currency, for the page's currency switcher.
    pub currencies: Vec<Currency>,
    pub accounts: Vec<Account>,
    /// Index-aligned with `accounts`: account_stats[i] describes accounts[i].
    pub account_stats: Vec<AccountStats>,
    /// Every account across *all* currencies, slim-projected for the
    /// cross-currency transfer picker (only `currency_code`, `code`, and
    /// `name` — the form ignores balance / tone / etc.).
    pub transfer_accounts: Vec<TransferAccountRef>,
    /// User categories in this currency — the auto `TFR` category is excluded.
    pub categories: Vec<Category>,
    /// Most recent 50 txns (descending). The full month count is in
    /// `month.total_txn_count`.
    pub txns: Vec<Txn>,
    pub category_summary: Vec<CategorySummary>,
    pub month: MonthSummary,
    /// Per-category budget entries for the current period (joined with
    /// month-to-date expenses). Drives the budget tab and the rule-based
    /// suggestions card.
    pub budgets: Vec<BudgetEntry>,
    /// 12-month income/expense trend, oldest → newest, with a dense frame
    /// (months with no activity render as zero-height bars rather than
    /// vanishing).
    pub months_12: Vec<MonthBucket>,
    /// All-time txn count per category code, used by the usage column on
    /// the categories management table.
    #[serde(default)]
    pub category_usage: std::collections::HashMap<String, i64>,
}

/// Dense 12-month income/expense bucket for one currency, oldest -> newest.
///
/// Expense totals deliberately include only `tag = 'exp'`; transfer from-legs
/// are negative too, but they are internal money movement rather than spend.
#[cfg(feature = "ssr")]
pub async fn load_month_buckets_12(
    pool: &sqlx::SqlitePool,
    currency_code: &str,
) -> sqlx::Result<Vec<MonthBucket>> {
    type MonthTxnRow = (String, ep_core::MinorAmount, String);
    let months_q = sqlx::query_as::<_, MonthTxnRow>(
        "SELECT strftime('%Y-%m', occurred_at, 'unixepoch', 'localtime') AS period,
                amount,
                tag
           FROM fin_txn
          WHERE currency_code = ?1
            AND occurred_at >= unixepoch('now','localtime','start of month','-11 months','utc')
          ORDER BY period ASC",
    )
    .bind(currency_code)
    .fetch_all(pool);
    let frame_q = sqlx::query_scalar::<_, String>(
        "WITH RECURSIVE months(p, n) AS (
            SELECT strftime('%Y-%m','now','localtime','start of month','-11 months'), 0
            UNION ALL
            SELECT strftime('%Y-%m','now','localtime','start of month',
                            printf('-%d months', 11 - n - 1)), n + 1
              FROM months
             WHERE n + 1 < 12
         )
         SELECT p FROM months ORDER BY p ASC",
    )
    .fetch_all(pool);

    let (months_rows, frame) = tokio::try_join!(months_q, frame_q)?;
    Ok(month_buckets_from_rows(frame, months_rows))
}

#[cfg(feature = "ssr")]
fn month_buckets_from_rows(
    frame: Vec<String>,
    rows: Vec<(String, ep_core::MinorAmount, String)>,
) -> Vec<MonthBucket> {
    let mut by_period: std::collections::HashMap<
        String,
        (ep_core::MinorAmount, ep_core::MinorAmount),
    > = std::collections::HashMap::new();
    for (period, amount, tag) in rows {
        let (income, expense) = by_period.entry(period).or_default();
        match tag.as_str() {
            "inc" if amount.is_positive() => *income += amount,
            "exp" if amount.is_negative() => *expense += amount.abs(),
            _ => {}
        }
    }
    frame
        .into_iter()
        .map(|period| {
            let (income, expense) = by_period
                .get(&period)
                .copied()
                .unwrap_or((ep_core::MinorAmount::ZERO, ep_core::MinorAmount::ZERO));
            MonthBucket {
                period,
                income,
                expense,
                net: income - expense,
            }
        })
        .collect()
}

#[server(LoadLedger, "/api/_internal/fin", "Url", "load_ledger")]
pub async fn load_ledger(currency_code: String) -> Result<LedgerData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let pool = &state.db;

        // Resolve the requested currency first; everything below is scoped to
        // it. An empty / stale code resolves to the primary currency.
        let currency = resolve_currency(pool, &currency_code).await?;
        let cc = currency.code.clone();

        let accounts_q = sqlx::query_as::<_, Account>(
            "SELECT currency_code, code, name, type, tone, balance, archived, created_at
               FROM fin_account WHERE currency_code = ?1 ORDER BY code",
        )
        .bind(&cc)
        .fetch_all(pool);
        // The auto `TFR` category is internal plumbing — never offered as a
        // ledger category, so it stays out of this list.
        let categories_q = sqlx::query_as::<_, Category>(
            "SELECT currency_code, code, name, icon, tone, sort_order, archived, created_at
               FROM fin_category WHERE currency_code = ?1 AND code <> ?2 ORDER BY sort_order",
        )
        .bind(&cc)
        .bind(TRANSFER_CATEGORY_CODE)
        .fetch_all(pool);
        let txns_q = sqlx::query_as::<_, Txn>(
            "SELECT doc_id, currency_code, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id
               FROM fin_txn WHERE currency_code = ?1 ORDER BY occurred_at DESC LIMIT 50"
        ).bind(&cc).fetch_all(pool);
        // Every "expense" aggregation filters `tag = 'exp'` (not just
        // `amount < 0`): transfer rows have `tag='tfr'` and the from-leg
        // is amount<0, which would otherwise pollute expense / category /
        // budget / 90d / 12-month / weekly net totals. tfr is internal
        // money movement, not spending.
        type MtdTxnRow = (String, ep_core::MinorAmount, String);
        let mtd_txns_q = sqlx::query_as::<_, MtdTxnRow>(
            "SELECT category_code, amount, tag
               FROM fin_txn
              WHERE currency_code = ?1
                AND occurred_at >= unixepoch('now','localtime','start of month','utc')",
        )
        .bind(&cc)
        .fetch_all(pool);
        // Per-category budget vs MTD usage, used by the budget tab and
        // `suggestions::compute_suggestions`; usage is joined in Rust because
        // amount columns are exact TEXT and SQLite SUM would coerce them.
        type BudgetRow = (String, ep_core::MinorAmount);
        let budgets_q = sqlx::query_as::<_, BudgetRow>(
            "SELECT b.category_code, b.amount
               FROM fin_budget b
              WHERE b.currency_code = ?1
                AND b.period = strftime('%Y-%m','now','localtime')
              ORDER BY b.category_code",
        )
        .bind(&cc)
        .fetch_all(pool);

        // Last-7-day net (income - expense). Used by the banner's weekly badge
        // and the suggestions card.
        type TimedAmountRow = (ep_core::MinorAmount, String);
        let week_net_q = sqlx::query_as::<_, TimedAmountRow>(
            "SELECT amount, tag
               FROM fin_txn
              WHERE currency_code = ?1
                AND occurred_at >= unixepoch('now','localtime','-7 days','utc')",
        )
        .bind(&cc)
        .fetch_all(pool);

        // 3-month rolling expense total, used for emergency-fund coverage and
        // the next-month-budget planner. 90-day window approximates 3 calendar
        // months without the awkward "what's my -3 month boundary" arithmetic.
        let expense_90d_q = sqlx::query_scalar::<_, ep_core::MinorAmount>(
            "SELECT amount FROM fin_txn
              WHERE currency_code = ?1 AND tag = 'exp'
                AND occurred_at >= unixepoch('now','localtime','-90 days','utc')",
        )
        .bind(&cc)
        .fetch_all(pool);

        // Total fin_txn count for the current month (independent of the
        // 50-row LIMIT on `txns_q`).
        let total_count_q = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM fin_txn
              WHERE currency_code = ?1
                AND occurred_at >= unixepoch('now','localtime','start of month','utc')",
        )
        .bind(&cc)
        .fetch_one(pool);

        // Per-account most-recent occurred_at, used by the last-activity line.
        type LastSeenRow = (String, i64);
        let last_seen_q = sqlx::query_as::<_, LastSeenRow>(
            "SELECT account_code, MAX(occurred_at) FROM fin_txn
              WHERE currency_code = ?1 GROUP BY account_code",
        )
        .bind(&cc)
        .fetch_all(pool);

        // Per-account, per-day expense magnitude over the last 14 days.
        // `'start of day'` on both sides anchors the diff to whole-day-aligned
        // local calendar days (same load-bearing trick the lrn heatmap uses
        // — without it sub-day fractions push 02:00-vs-22:00 same-day rows
        // into different buckets).
        type DailyHistoryRow = (String, i64, ep_core::MinorAmount);
        let history_14d_q = sqlx::query_as::<_, DailyHistoryRow>(
            "SELECT account_code,
                    CAST(julianday('now','localtime','start of day')
                         - julianday(occurred_at,'unixepoch','localtime','start of day') AS INTEGER) AS days_ago,
                    amount
               FROM fin_txn
              WHERE currency_code = ?1 AND tag = 'exp'
                AND occurred_at >= unixepoch('now','localtime','-13 days','start of day','utc')"
        ).bind(&cc).fetch_all(pool);

        let months_12_q = load_month_buckets_12(pool, &cc);

        // The page's wall-clock context: current period label and elapsed
        // days. Sent in the same join so rendering uses a single self-
        // consistent snapshot (period = "2026-05" pairs with days_elapsed
        // computed against the same `now`).
        type ContextRow = (String, i64);
        let context_q = sqlx::query_as::<_, ContextRow>(
            "SELECT strftime('%Y-%m','now','localtime') AS period,
                    CAST(strftime('%d','now','localtime') AS INTEGER) AS day_of_month",
        )
        .fetch_one(pool);

        // All-time per-category txn count, used by the usage column on the
        // categories management table.
        type CatUsageRow = (String, i64);
        let cat_usage_q = sqlx::query_as::<_, CatUsageRow>(
            "SELECT category_code, COUNT(*) FROM fin_txn
              WHERE currency_code = ?1 GROUP BY category_code",
        )
        .bind(&cc)
        .fetch_all(pool);

        let currencies_q = list_currencies_inner(pool);

        // Every *active* account, every currency — the transfer picker spans
        // currencies. Archived accounts are dropped here so they can't be
        // chosen as a transfer leg; only the three columns the form renders.
        let transfer_accounts_q = sqlx::query_as::<_, TransferAccountRef>(
            "SELECT currency_code, code, name
               FROM fin_account WHERE archived = 0 ORDER BY currency_code, code",
        )
        .fetch_all(pool);

        let (
            accounts,
            categories,
            txns,
            mtd_txn_rows,
            budget_rows,
            week_net_rows,
            expense_90d_rows,
            total_count,
            last_seen_rows,
            history_14d_rows,
            months_12,
            ctx,
            cat_usage_rows,
            currencies,
            transfer_accounts,
        ) = tokio::try_join!(
            accounts_q,
            categories_q,
            txns_q,
            mtd_txns_q,
            budgets_q,
            week_net_q,
            expense_90d_q,
            total_count_q,
            last_seen_q,
            history_14d_q,
            months_12_q,
            context_q,
            cat_usage_q,
            currencies_q,
            transfer_accounts_q,
        )
        .map_err(server_err)?;

        let mut income = ep_core::MinorAmount::ZERO;
        let mut category_spend: std::collections::HashMap<String, ep_core::MinorAmount> =
            std::collections::HashMap::new();
        for (category_code, amount, tag) in &mtd_txn_rows {
            match tag.as_str() {
                "inc" if amount.is_positive() => income += *amount,
                "exp" if amount.is_negative() => {
                    *category_spend.entry(category_code.clone()).or_default() += amount.abs();
                }
                _ => {}
            }
        }
        let expense: ep_core::MinorAmount = category_spend.values().copied().sum();
        let category_summary = category_spend
            .into_iter()
            .map(|(code, value)| {
                let cat = categories.iter().find(|c| c.code == code);
                CategorySummary {
                    code: code.clone(),
                    name: cat.map(|c| c.name.clone()).unwrap_or_default(),
                    icon: cat.map(|c| c.icon.clone()).unwrap_or_default(),
                    tone: cat.map(|c| c.tone.clone()).unwrap_or_default(),
                    value,
                    pct: if expense.is_positive() {
                        (value.to_f64() / expense.to_f64() * 1000.0).round() / 10.0
                    } else {
                        0.0
                    },
                }
            })
            .collect::<Vec<_>>();

        let balance: ep_core::MinorAmount = accounts.iter().map(|a| a.balance).sum();
        let liquid_balance: ep_core::MinorAmount = accounts
            .iter()
            .filter(|a| matches!(a.r#type.as_str(), "Checking" | "Savings" | "Cash"))
            .map(|a| a.balance)
            .sum();

        let budget_total: ep_core::MinorAmount =
            budget_rows.iter().map(|(_, amount)| *amount).sum();

        let budgets: Vec<BudgetEntry> = budget_rows
            .into_iter()
            .map(|(category_code, amount)| {
                let used = mtd_txn_rows
                    .iter()
                    .filter(|(cat, amount, tag)| {
                        cat == &category_code && tag == "exp" && amount.is_negative()
                    })
                    .map(|(_, amount, _)| amount.abs())
                    .sum();
                BudgetEntry {
                    category_code,
                    amount,
                    used,
                }
            })
            .collect();

        let mut last_seen_map: std::collections::HashMap<String, i64> =
            last_seen_rows.into_iter().collect();
        // `'start of day'` on both sides of the SQL diff anchors days_ago
        // to 0..=13; index 0 is the oldest day, 13 is today. The clamp
        // is paranoia against a future SQL edit.
        let mut history_map: std::collections::HashMap<String, Vec<ep_core::MinorAmount>> =
            std::collections::HashMap::new();
        for (account_code, days_ago, magnitude) in history_14d_rows {
            let slot = history_map
                .entry(account_code)
                .or_insert_with(|| vec![ep_core::MinorAmount::ZERO; 14]);
            let idx = (13 - days_ago.clamp(0, 13)) as usize;
            if let Some(cell) = slot.get_mut(idx) {
                *cell += magnitude.abs();
            }
        }
        let account_stats: Vec<AccountStats> = accounts
            .iter()
            .map(|a| AccountStats {
                last_seen_at: last_seen_map.remove(&a.code),
                history_14d: history_map
                    .remove(&a.code)
                    .unwrap_or_else(|| vec![ep_core::MinorAmount::ZERO; 14]),
            })
            .collect();

        let (period, day_of_month) = ctx;
        let days_elapsed = (day_of_month as u32).max(1);
        let expense_90d: ep_core::MinorAmount = expense_90d_rows.into_iter().map(|a| a.abs()).sum();
        let avg_expense_3m = expense_90d / 3;
        let week_net: ep_core::MinorAmount = week_net_rows
            .into_iter()
            .map(|(amount, tag)| match tag.as_str() {
                "inc" if amount.is_positive() => amount,
                "exp" if amount.is_negative() => amount,
                _ => ep_core::MinorAmount::ZERO,
            })
            .sum();
        let savings_rate = if income.is_positive() {
            (((income - expense).to_f64() / income.to_f64()).clamp(0.0, 1.0)) as f32
        } else {
            0.0
        };
        // Guard the divide-by-zero on a fresh DB; clamp keeps KPI sane.
        let emergency_months = if avg_expense_3m.is_positive() {
            (liquid_balance.to_f64() / avg_expense_3m.to_f64()).clamp(0.0, 99.0) as f32
        } else {
            0.0
        };

        let category_usage: std::collections::HashMap<String, i64> =
            cat_usage_rows.into_iter().collect();

        Ok(LedgerData {
            currency,
            currencies,
            accounts,
            account_stats,
            transfer_accounts,
            categories,
            txns,
            category_summary,
            budgets,
            months_12,
            category_usage,
            month: MonthSummary {
                income,
                expense,
                savings: income - expense,
                balance,
                balance_delta: week_net,
                budget_total,
                savings_rate,
                emergency_months,
                liquid_balance,
                days_elapsed,
                avg_expense_3m,
                total_txn_count: total_count,
                period,
            },
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = currency_code;
        Err(ep_core::server_err("ssr-only"))
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Leptos ActionForm fields map to server-fn parameters"
)]
#[server(AddTxn, "/api/_internal/fin", "Url", "add_txn")]
pub async fn add_txn(
    currency_code: String,
    merchant: String,
    category_code: String,
    account_code: String,
    amount: String,
    tag: String,
    note: String,
    occurred_at: String,
) -> Result<Txn, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let pool = &state.db;

        let currency = resolve_currency(pool, &currency_code).await?;

        let tag = tag.trim();
        let tag_kind = match crate::model::Tag::parse(tag) {
            Some(k) => k,
            None => return Err(ep_i18n::err_with("finance.err.tag_invalid", tag)),
        };
        if !tag_kind.is_single_entry() {
            return Err(ep_i18n::err("finance.err.tfr_requires_transfer"));
        }
        // Form contract: positive amount, `tag` carries the sign (matches the
        // UI convention). `/api/v1/fin/txn` is a separate code path that
        // accepts pre-signed exp/inc amounts; paired transfers go through
        // add_transfer.
        let magnitude = parse_positive_minor(&amount, currency.decimals)?;
        let amount = if tag_kind == crate::model::Tag::Exp {
            -magnitude
        } else {
            magnitude
        };

        let occurred = parse_occurred_at(pool, &occurred_at)
            .await?
            .unwrap_or_else(ep_core::unix_now);

        let txn = add_txn_inner(
            pool,
            AddTxnFields {
                currency_code: currency.code.clone(),
                merchant,
                category_code,
                account_code,
                amount,
                tag: tag_kind.as_str().to_string(),
                note: Some(note),
                linked_doc_id: None,
                occurred_at: occurred,
            },
        )
        .await?;
        dispatch_large_expense_notification(&state.notify, &currency, &txn).await;
        Ok(txn)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (
            currency_code,
            merchant,
            category_code,
            account_code,
            amount,
            tag,
            note,
            occurred_at,
        );
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
pub async fn add_txn_inner(pool: &SqlitePool, fields: AddTxnFields) -> Result<Txn, ServerFnError> {
    let normalized = normalize_txn_fields(
        &fields.merchant,
        &fields.category_code,
        &fields.account_code,
        fields.note.as_deref(),
    )?;
    let currency_code = fields.currency_code.trim().to_string();
    if currency_code.is_empty() {
        return Err(ep_i18n::err("finance.err.no_currency"));
    }
    let tag_raw = fields.tag.trim();
    let tag_kind = match crate::model::Tag::parse(tag_raw) {
        Some(k) => k,
        None => return Err(ep_i18n::err_with("finance.err.tag_invalid", tag_raw)),
    };
    if !tag_kind.is_single_entry() {
        return Err(ep_i18n::err("finance.err.tfr_requires_transfer"));
    }
    match tag_kind {
        crate::model::Tag::Exp if fields.amount.is_negative() => {}
        crate::model::Tag::Inc if fields.amount.is_positive() => {}
        _ => return Err(ep_i18n::err("finance.err.amount_sign_invalid")),
    }

    let linked_doc_id = match fields
        .linked_doc_id
        .as_deref()
        .and_then(ep_core::trim_to_option)
    {
        Some(doc_id) if ep_core::safe_doc_id(&doc_id).is_some() => Some(doc_id),
        Some(doc_id) => {
            return Err(ep_i18n::err_with(
                "finance.err.linked_doc_id_invalid",
                &doc_id,
            ))
        }
        None => None,
    };

    // Pre-validate FKs concurrently so callers get clear domain errors rather
    // than opaque sqlite FK violations. Both the category and the account
    // must live in the transaction's currency.
    let (cat_exists, acc_exists): (i64, i64) = tokio::try_join!(
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fin_category WHERE currency_code = ?1 AND code = ?2)"
        )
        .bind(&currency_code)
        .bind(&normalized.category_code)
        .fetch_one(pool),
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fin_account WHERE currency_code = ?1 AND code = ?2)"
        )
        .bind(&currency_code)
        .bind(&normalized.account_code)
        .fetch_one(pool),
    )
    .map_err(server_err)?;
    if cat_exists == 0 {
        return Err(ep_i18n::err_with(
            "finance.err.category_not_found",
            &normalized.category_code,
        ));
    }
    if acc_exists == 0 {
        return Err(ep_i18n::err_with(
            "finance.err.account_not_found",
            &normalized.account_code,
        ));
    }

    let mut tx = pool.begin().await.map_err(server_err)?;
    let doc_id = ep_core::next_doc_id(&mut tx, "FIN", ep_core::DocIdShape::YearSerial5)
        .await
        .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO fin_txn (doc_id, currency_code, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
    )
    .bind(&doc_id)
    .bind(&currency_code)
    .bind(fields.occurred_at)
    .bind(&normalized.merchant)
    .bind(&normalized.category_code)
    .bind(&normalized.account_code)
    .bind(fields.amount)
    .bind(tag_kind.as_str())
    .bind(&normalized.note)
    .bind(&linked_doc_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;

    adjust_account_balance(
        &mut tx,
        &currency_code,
        &normalized.account_code,
        fields.amount,
    )
    .await?;

    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, currency_code, link_doc)
         VALUES (?1, 'FIN', ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(fields.occurred_at)
    .bind(&doc_id)
    .bind(&normalized.merchant)
    .bind(fields.amount)
    .bind(&currency_code)
    .bind(&linked_doc_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;

    if let Some(linked_doc_id) = &linked_doc_id {
        sqlx::query(
            "INSERT INTO module_link (source_doc, target_doc, kind)
             VALUES (?1, ?2, 'ref')",
        )
        .bind(&doc_id)
        .bind(linked_doc_id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    }

    tx.commit().await.map_err(server_err)?;

    Ok(Txn {
        doc_id,
        currency_code,
        occurred_at: fields.occurred_at,
        merchant: normalized.merchant,
        category_code: normalized.category_code,
        account_code: normalized.account_code,
        amount: fields.amount,
        tag: tag_kind.as_str().to_string(),
        note: normalized.note,
        linked_doc_id,
    })
}

/// Fire a notification for an unusually large expense. The threshold is "500
/// major units" scaled into the currency's own minor units, so it adapts to
/// each currency's precision.
#[cfg(feature = "ssr")]
pub async fn dispatch_large_expense_notification(
    notify: &ep_core::NotifyBusHandle,
    currency: &Currency,
    txn: &Txn,
) {
    let threshold = ep_core::major_to_minor(500, currency.decimals);
    if txn.amount >= -threshold {
        return;
    }
    let n = ep_core::NotifyMessage::warn(format!("Large expense · {}", txn.merchant))
        .module("FIN")
        .body(format!(
            "{}{} ({})",
            currency.symbol,
            ep_core::fmt_minor(txn.amount.abs(), currency.decimals),
            txn.category_code
        ))
        .doc_ref(txn.doc_id.clone())
        .link("/finance");
    if let Err(e) = notify.dispatch(n).await {
        tracing::warn!(error = %e, doc_id = %txn.doc_id, "large expense notification failed");
    }
}

/// Result of `delete_one_leg`: `Some((tag, linked_doc_id))` if a row was
/// removed, `None` if the doc_id had no matching row (caller decides whether
/// that's an error).
#[cfg(feature = "ssr")]
async fn delete_one_leg(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    doc_id: &str,
) -> Result<Option<(String, Option<String>)>, ServerFnError> {
    let row: Option<(ep_core::MinorAmount, String, String, String, Option<String>)> =
        sqlx::query_as(
            "SELECT amount, currency_code, account_code, tag, linked_doc_id
           FROM fin_txn WHERE doc_id = ?1",
        )
        .bind(doc_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(server_err)?;
    let (amount, currency_code, account_code, tag, linked_doc_id) = match row {
        Some(r) => r,
        None => return Ok(None),
    };
    adjust_account_balance(tx, &currency_code, &account_code, -amount).await?;
    sqlx::query("DELETE FROM fin_txn WHERE doc_id = ?1")
        .bind(doc_id)
        .execute(&mut **tx)
        .await
        .map_err(server_err)?;
    ep_core::delete_doc_activity_and_references(tx, "FIN", doc_id)
        .await
        .map_err(server_err)?;
    Ok(Some((tag, linked_doc_id)))
}

/// Delete a fin_txn row, undo its side effects, and cascade to the transfer
/// partner if one exists. Cascade authority is `module_link.kind='tfr-pair'`
/// (only `add_transfer_inner` writes those rows); single-leg `tag='tfr'`
/// from `add_txn` uses `kind='ref'` and is not a partner.
#[cfg(feature = "ssr")]
pub async fn delete_txn_inner(pool: &SqlitePool, doc_id: &str) -> Result<bool, ServerFnError> {
    let mut tx = pool.begin().await.map_err(server_err)?;

    // Resolve partner first — delete_one_leg clears module_link.
    // Walk both directions of the symmetric `tfr-pair` link; UNION dedupes
    // a healthy pair to one row. >1 distinct partner = corrupt link table:
    // refuse rather than cascade-delete an unrelated row.
    let partners: Vec<String> = sqlx::query_scalar(
        "SELECT partner FROM (
            SELECT target_doc AS partner FROM module_link
              WHERE source_doc = ?1 AND kind = 'tfr-pair'
            UNION
            SELECT source_doc AS partner FROM module_link
              WHERE target_doc = ?1 AND kind = 'tfr-pair'
         )",
    )
    .bind(doc_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(server_err)?;
    let pair_partner: Option<String> = match partners.len() {
        0 => None,
        1 => partners.into_iter().next(),
        _ => {
            tracing::error!(
                doc_id, partners = ?partners,
                "tfr-pair link table corrupt: multiple distinct partners"
            );
            return Err(ep_i18n::err("finance.err.tfr_pair_multiple_partners"));
        }
    };

    let first = delete_one_leg(&mut tx, doc_id).await?;
    if first.is_none() {
        return Ok(false);
    }

    if let Some(partner_doc) = pair_partner {
        match delete_one_leg(&mut tx, &partner_doc).await {
            Ok(Some(_)) => {}
            // Orphan: link survived but partner row didn't. We can't reverse
            // partner's balance (module_link doesn't carry amount/account),
            // so committing would leak a phantom balance. Refuse.
            Ok(None) => {
                tracing::error!(
                    doc_id, partner = %partner_doc,
                    "tfr-pair partner row missing — refusing half-rollback"
                );
                return Err(ep_i18n::err_with(
                    "finance.err.tfr_pair_partner_missing",
                    &partner_doc,
                ));
            }
            Err(e) => {
                tracing::error!(
                    doc_id, partner = %partner_doc, error = %e,
                    "tfr-pair partner cleanup errored — aborting"
                );
                return Err(e);
            }
        }
    }
    tx.commit().await.map_err(server_err)?;
    Ok(true)
}

#[server(DeleteTxn, "/api/_internal/fin", "Url", "delete_txn")]
pub async fn delete_txn(doc_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let state = ep_core::app_state_context()?;
        if !delete_txn_inner(&state.db, &doc_id).await? {
            return Err(ep_i18n::err_with("finance.err.txn_not_found", &doc_id));
        }
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = doc_id;
        Err(ep_core::server_err("ssr-only"))
    }
}

/// `amount <= 0` deletes the row (treats "set budget to zero" as remove).
#[cfg(feature = "ssr")]
pub async fn set_budget_inner(
    pool: &SqlitePool,
    currency_code: &str,
    period: &str,
    category_code: &str,
    amount: ep_core::MinorAmount,
) -> Result<(), ServerFnError> {
    let currency_code = currency_code.trim();
    if currency_code.is_empty() {
        return Err(ep_i18n::err("finance.err.no_currency"));
    }
    let period = normalize_budget_period(period)?;
    let category_code = category_code.trim();
    if category_code.is_empty() {
        return Err(ep_i18n::err("finance.err.category_code_required"));
    }
    if amount.is_positive() {
        let exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fin_category WHERE currency_code = ?1 AND code = ?2)",
        )
        .bind(currency_code)
        .bind(category_code)
        .fetch_one(pool)
        .await
        .map_err(server_err)?;
        if exists == 0 {
            return Err(ep_i18n::err_with(
                "finance.err.category_not_found",
                category_code,
            ));
        }
    }
    if !amount.is_positive() {
        sqlx::query(
            "DELETE FROM fin_budget WHERE currency_code = ?1 AND period = ?2 AND category_code = ?3",
        )
        .bind(currency_code)
        .bind(&period)
        .bind(category_code)
        .execute(pool)
        .await
        .map_err(server_err)?;
    } else {
        // ON CONFLICT updates the amount in place. Composite PK is
        // (currency_code, period, category_code) per 002_multi_currency.sql.
        sqlx::query(
            "INSERT INTO fin_budget (currency_code, period, category_code, amount)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(currency_code, period, category_code)
             DO UPDATE SET amount = excluded.amount",
        )
        .bind(currency_code)
        .bind(period)
        .bind(category_code)
        .bind(amount)
        .execute(pool)
        .await
        .map_err(server_err)?;
    }
    Ok(())
}

/// Upsert a per-currency, per-period, per-category budget. `amount <= 0`
/// deletes the row (treats "set budget to zero" as "remove budget").
/// `period` must be a real `YYYY-MM` month.
#[server(SetBudget, "/api/_internal/fin", "Url", "set_budget")]
pub async fn set_budget(
    currency_code: String,
    period: String,
    category_code: String,
    amount: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let pool = &state.db;
        let currency = resolve_currency(pool, &currency_code).await?;
        // A blank amount means "remove" — treat it as 0 rather than erroring.
        let amount = if amount.trim().is_empty() {
            ep_core::MinorAmount::ZERO
        } else {
            ep_core::parse_minor(&amount, currency.decimals)
                .ok_or_else(|| ep_i18n::err_with("finance.err.amount_invalid", amount.trim()))?
        };
        set_budget_inner(pool, &currency.code, &period, &category_code, amount).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (currency_code, period, category_code, amount);
        Err(ep_core::server_err("ssr-only"))
    }
}

/// Copy every row of `fin_budget` from `source_period` into `target_period`
/// within one currency, overwriting any existing target rows. Drives the
/// prior-budget import affordance shown when the current period has no
/// budgets yet.
#[server(ImportBudgetsFrom, "/api/_internal/fin", "Url", "import_budgets_from")]
pub async fn import_budgets_from(
    currency_code: String,
    source_period: String,
    target_period: String,
) -> Result<i64, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let source_period = normalize_budget_period(&source_period)?;
        let target_period = normalize_budget_period(&target_period)?;
        if source_period == target_period {
            return Err(ep_i18n::err("finance.err.budget_periods_same"));
        }
        let state = ep_core::app_state_context()?;
        let pool = &state.db;
        let currency = resolve_currency(pool, &currency_code).await?;
        let res = sqlx::query(
            "INSERT INTO fin_budget (currency_code, period, category_code, amount)
             SELECT ?1, ?2, category_code, amount
               FROM fin_budget WHERE currency_code = ?1 AND period = ?3
             ON CONFLICT(currency_code, period, category_code)
             DO UPDATE SET amount = excluded.amount",
        )
        .bind(&currency.code)
        .bind(&target_period)
        .bind(&source_period)
        .execute(pool)
        .await
        .map_err(server_err)?;
        Ok(res.rows_affected() as i64)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (currency_code, source_period, target_period);
        Err(ep_core::server_err("ssr-only"))
    }
}

// ---------------------------------------------------------------------------
// Txn update
// ---------------------------------------------------------------------------

/// Mutable fields for `update_txn_inner`. `amount` is a positive magnitude in
/// the transaction's own currency's minor units — the sign is re-derived from
/// the (immutable) `tag`. Bundled so the function signature stays sane and
/// the PAT axum handler can build a single value from JSON.
#[cfg(feature = "ssr")]
pub struct UpdateTxnFields {
    pub merchant: String,
    pub category_code: String,
    pub account_code: String,
    pub amount: ep_core::MinorAmount,
    pub note: Option<String>,
    /// Wire form: empty → "keep existing", `"YYYY-MM-DD"` → that day 12:00
    /// local. Bad format → Args error.
    pub occurred_at_input: String,
}

/// `tag`, `doc_id` and `currency_code` are immutable. To change `tag` or move
/// a transaction between currencies, delete and re-create it. Transfer rows
/// (`tag='tfr'`) reject any update — delete the pair via `delete_txn` and
/// re-create via `add_transfer`.
#[cfg(feature = "ssr")]
pub async fn update_txn_inner(
    pool: &SqlitePool,
    doc_id: &str,
    fields: UpdateTxnFields,
) -> Result<Txn, ServerFnError> {
    let normalized = normalize_txn_fields(
        &fields.merchant,
        &fields.category_code,
        &fields.account_code,
        fields.note.as_deref(),
    )?;
    if !fields.amount.is_positive() {
        return Err(ep_i18n::err("finance.err.amount_must_be_positive"));
    }

    let mut tx = pool.begin().await.map_err(server_err)?;

    // Read the existing row (and lock it implicitly under SQLite's deferred
    // tx). Capture old amount/currency/account/tag/occurred so we can both
    // refuse tfr edits and compute balance deltas.
    type OldRow = (ep_core::MinorAmount, String, String, String, i64);
    let old: Option<OldRow> = sqlx::query_as(
        "SELECT amount, currency_code, account_code, tag, occurred_at
           FROM fin_txn WHERE doc_id = ?1",
    )
    .bind(doc_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(server_err)?;
    let (old_amount, currency_code, old_account, old_tag, old_occurred) = match old {
        Some(r) => r,
        None => return Err(ep_i18n::err_with("finance.err.txn_not_found", doc_id)),
    };
    if old_tag == "tfr" {
        return Err(ep_i18n::err("finance.err.tfr_not_editable"));
    }

    // tag is immutable, so amount sign is fully determined by old_tag.
    // The UI sends a positive magnitude; coercing here keeps the invariant.
    let signed_amount = if old_tag == "exp" {
        -fields.amount
    } else {
        fields.amount
    };

    // Validate FK constraints — the new category and account must live in the
    // transaction's (immutable) currency. Sequential because we share one tx —
    // try_join would alias-borrow the connection.
    let cat_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fin_category WHERE currency_code = ?1 AND code = ?2)",
    )
    .bind(&currency_code)
    .bind(&normalized.category_code)
    .fetch_one(&mut *tx)
    .await
    .map_err(server_err)?;
    let acc_ok: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fin_account WHERE currency_code = ?1 AND code = ?2)",
    )
    .bind(&currency_code)
    .bind(&normalized.account_code)
    .fetch_one(&mut *tx)
    .await
    .map_err(server_err)?;
    if cat_exists == 0 {
        return Err(ep_i18n::err_with(
            "finance.err.category_not_found",
            &normalized.category_code,
        ));
    }
    if acc_ok == 0 {
        return Err(ep_i18n::err_with(
            "finance.err.account_not_found",
            &normalized.account_code,
        ));
    }

    // occurred_at: empty → keep existing.
    let new_occurred = match parse_occurred_at(pool, &fields.occurred_at_input).await? {
        Some(ts) => ts,
        None => old_occurred,
    };

    // Balance delta uses `signed_amount`, not the raw input. SQLite forbids
    // running queries on the pool concurrently while a tx is open on it,
    // so do these sequentially.
    if old_account == normalized.account_code {
        adjust_account_balance(
            &mut tx,
            &currency_code,
            &normalized.account_code,
            signed_amount - old_amount,
        )
        .await?;
    } else {
        adjust_account_balance(&mut tx, &currency_code, &old_account, -old_amount).await?;
        adjust_account_balance(
            &mut tx,
            &currency_code,
            &normalized.account_code,
            signed_amount,
        )
        .await?;
    }

    sqlx::query(
        "UPDATE fin_txn
            SET merchant = ?1, category_code = ?2, account_code = ?3,
                amount = ?4, note = ?5, occurred_at = ?6, linked_doc_id = NULL
          WHERE doc_id = ?7",
    )
    .bind(&normalized.merchant)
    .bind(&normalized.category_code)
    .bind(&normalized.account_code)
    .bind(signed_amount)
    .bind(&normalized.note)
    .bind(new_occurred)
    .bind(doc_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;

    sqlx::query(
        "UPDATE activity SET summary = ?1, amount = ?2, link_doc = NULL, occurred_at = ?3
          WHERE module = 'FIN' AND doc_id = ?4",
    )
    .bind(&normalized.merchant)
    .bind(signed_amount)
    .bind(new_occurred)
    .bind(doc_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;

    sqlx::query("DELETE FROM module_link WHERE source_doc = ?1 AND kind = 'ref'")
        .bind(doc_id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;

    tx.commit().await.map_err(server_err)?;

    Ok(Txn {
        doc_id: doc_id.to_string(),
        currency_code,
        occurred_at: new_occurred,
        merchant: normalized.merchant,
        category_code: normalized.category_code,
        account_code: normalized.account_code,
        amount: signed_amount,
        tag: old_tag,
        note: normalized.note,
        linked_doc_id: None,
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "Leptos ActionForm fields map to server-fn parameters"
)]
#[server(UpdateTxn, "/api/_internal/fin", "Url", "update_txn")]
pub async fn update_txn(
    doc_id: String,
    merchant: String,
    category_code: String,
    account_code: String,
    amount: String,
    note: String,
    occurred_at: String,
) -> Result<Txn, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let state = ep_core::app_state_context()?;
        let pool = &state.db;
        // Resolve the transaction's currency to parse the amount at the right
        // precision; `update_txn_inner` re-reads the row for the rest.
        // One JOIN to get the txn's currency precision; `update_txn_inner`
        // re-reads the row for the rest of the update.
        let decimals: Option<u8> = sqlx::query_scalar(
            "SELECT c.decimals FROM fin_txn t
               JOIN fin_currency c ON c.code = t.currency_code
              WHERE t.doc_id = ?1",
        )
        .bind(&doc_id)
        .fetch_optional(pool)
        .await
        .map_err(server_err)?;
        let Some(decimals) = decimals else {
            return Err(ep_i18n::err_with("finance.err.txn_not_found", &doc_id));
        };
        let amount = parse_positive_minor(&amount, decimals)?;
        update_txn_inner(
            pool,
            &doc_id,
            UpdateTxnFields {
                merchant,
                category_code,
                account_code,
                amount,
                note: Some(note),
                occurred_at_input: occurred_at,
            },
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (
            doc_id,
            merchant,
            category_code,
            account_code,
            amount,
            note,
            occurred_at,
        );
        Err(ep_core::server_err("ssr-only"))
    }
}

// ---------------------------------------------------------------------------
// Transfer (paired tfr txns)
// ---------------------------------------------------------------------------

/// Input bundle for a paired transfer. Each leg carries its own currency,
/// account, and positive minor-unit amount; no conversion is implied.
#[cfg(feature = "ssr")]
pub struct AddTransferFields {
    pub from_currency: String,
    pub from_account: String,
    pub to_currency: String,
    pub to_account: String,
    pub from_amount: ep_core::MinorAmount,
    pub to_amount: ep_core::MinorAmount,
    pub note: Option<String>,
    pub occurred_at: i64,
}

/// Writes two paired `tag='tfr'` `fin_txn` rows + symmetric `module_link`
/// `kind='tfr-pair'` rows in one tx. The legs may live in different
/// currencies — there is no conversion, the caller supplies an explicit
/// amount for each side. Both legs share `occurred_at` and file under their
/// own currency's `TFR` category. `delete_txn_inner` cascades via the
/// `tfr-pair` links.
///
/// Validates inputs (non-empty / distinct accounts / positive amounts, FK
/// check on both accounts and both `TFR` categories). Wrappers don't need to
/// re-validate.
#[cfg(feature = "ssr")]
pub async fn add_transfer_inner(
    pool: &SqlitePool,
    fields: AddTransferFields,
) -> Result<(Txn, Txn), ServerFnError> {
    let AddTransferFields {
        from_currency,
        from_account,
        to_currency,
        to_account,
        from_amount,
        to_amount,
        note,
        occurred_at,
    } = fields;
    let from_currency = from_currency.trim().to_string();
    let to_currency = to_currency.trim().to_string();
    let from_account = from_account.trim().to_string();
    let to_account = to_account.trim().to_string();
    if from_currency.is_empty()
        || to_currency.is_empty()
        || from_account.is_empty()
        || to_account.is_empty()
    {
        return Err(ep_i18n::err("finance.err.transfer_accounts_required"));
    }
    // Same currency *and* same account would be a no-op self-transfer.
    if from_currency == to_currency && from_account == to_account {
        return Err(ep_i18n::err("finance.err.transfer_accounts_same"));
    }
    if !from_amount.is_positive() || !to_amount.is_positive() {
        return Err(ep_i18n::err("finance.err.amount_must_be_positive"));
    }
    let (from_ok, to_ok, from_tfr_ok, to_tfr_ok): (i64, i64, i64, i64) = tokio::try_join!(
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fin_account WHERE currency_code = ?1 AND code = ?2)"
        )
        .bind(&from_currency)
        .bind(&from_account)
        .fetch_one(pool),
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fin_account WHERE currency_code = ?1 AND code = ?2)"
        )
        .bind(&to_currency)
        .bind(&to_account)
        .fetch_one(pool),
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fin_category WHERE currency_code = ?1 AND code = ?2)"
        )
        .bind(&from_currency)
        .bind(TRANSFER_CATEGORY_CODE)
        .fetch_one(pool),
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM fin_category WHERE currency_code = ?1 AND code = ?2)"
        )
        .bind(&to_currency)
        .bind(TRANSFER_CATEGORY_CODE)
        .fetch_one(pool),
    )
    .map_err(server_err)?;
    if from_ok == 0 {
        return Err(ep_i18n::err_with(
            "finance.err.account_not_found",
            &from_account,
        ));
    }
    if to_ok == 0 {
        return Err(ep_i18n::err_with(
            "finance.err.account_not_found",
            &to_account,
        ));
    }
    if from_tfr_ok == 0 || to_tfr_ok == 0 {
        return Err(ep_i18n::err("finance.err.tfr_category_missing"));
    }

    let mut tx = pool.begin().await.map_err(server_err)?;
    let from_doc = ep_core::next_doc_id(&mut tx, "FIN", ep_core::DocIdShape::YearSerial5)
        .await
        .map_err(server_err)?;
    let to_doc = ep_core::next_doc_id(&mut tx, "FIN", ep_core::DocIdShape::YearSerial5)
        .await
        .map_err(server_err)?;

    let from_merchant = format!("Transfer out → {to_account}");
    let to_merchant = format!("Transfer in ← {from_account}");
    let note_owned = normalize_txn_note(note.as_deref())?;

    sqlx::query(
        "INSERT INTO fin_txn
            (doc_id, currency_code, occurred_at, merchant, category_code, account_code,
             amount, tag, note, linked_doc_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'tfr', ?8, ?9)",
    )
    .bind(&from_doc)
    .bind(&from_currency)
    .bind(occurred_at)
    .bind(&from_merchant)
    .bind(TRANSFER_CATEGORY_CODE)
    .bind(&from_account)
    .bind(-from_amount)
    .bind(&note_owned)
    .bind(&to_doc)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO fin_txn
            (doc_id, currency_code, occurred_at, merchant, category_code, account_code,
             amount, tag, note, linked_doc_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'tfr', NULL, ?8)",
    )
    .bind(&to_doc)
    .bind(&to_currency)
    .bind(occurred_at)
    .bind(&to_merchant)
    .bind(TRANSFER_CATEGORY_CODE)
    .bind(&to_account)
    .bind(to_amount)
    .bind(&from_doc)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;

    adjust_account_balance(&mut tx, &from_currency, &from_account, -from_amount).await?;
    adjust_account_balance(&mut tx, &to_currency, &to_account, to_amount).await?;

    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, currency_code, link_doc)
         VALUES (?1, 'FIN', ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(occurred_at)
    .bind(&from_doc)
    .bind(&from_merchant)
    .bind(-from_amount)
    .bind(&from_currency)
    .bind(&to_doc)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, currency_code, link_doc)
         VALUES (?1, 'FIN', ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(occurred_at)
    .bind(&to_doc)
    .bind(&to_merchant)
    .bind(to_amount)
    .bind(&to_currency)
    .bind(&from_doc)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;

    // Symmetric pair so the cascade lookup walks either direction.
    sqlx::query(
        "INSERT INTO module_link (source_doc, target_doc, kind) VALUES (?1, ?2, 'tfr-pair')",
    )
    .bind(&from_doc)
    .bind(&to_doc)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO module_link (source_doc, target_doc, kind) VALUES (?1, ?2, 'tfr-pair')",
    )
    .bind(&to_doc)
    .bind(&from_doc)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;

    tx.commit().await.map_err(server_err)?;

    Ok((
        Txn {
            doc_id: from_doc.clone(),
            currency_code: from_currency.clone(),
            occurred_at,
            merchant: from_merchant,
            category_code: TRANSFER_CATEGORY_CODE.to_string(),
            account_code: from_account.clone(),
            amount: -from_amount,
            tag: "tfr".into(),
            note: note_owned.clone(),
            linked_doc_id: Some(to_doc.clone()),
        },
        Txn {
            doc_id: to_doc,
            currency_code: to_currency,
            occurred_at,
            merchant: to_merchant,
            category_code: TRANSFER_CATEGORY_CODE.to_string(),
            account_code: to_account,
            amount: to_amount,
            tag: "tfr".into(),
            note: None,
            linked_doc_id: Some(from_doc),
        },
    ))
}

#[server(AddTransfer, "/api/_internal/fin", "Url", "add_transfer")]
pub async fn add_transfer(
    from_account: String,
    to_account: String,
    from_amount: String,
    to_amount: String,
    note: String,
    occurred_at: String,
) -> Result<(Txn, Txn), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let pool = &state.db;

        // The transfer picker emits "{currency}/{account}" option values.
        let (from_currency, from_account) = split_currency_ref(&from_account)?;
        let (to_currency, to_account) = split_currency_ref(&to_account)?;
        let (from_cur, to_cur) = tokio::try_join!(
            resolve_currency(pool, &from_currency),
            resolve_currency(pool, &to_currency),
        )?;
        let from_minor = parse_positive_minor(&from_amount, from_cur.decimals)?;
        let to_minor = parse_positive_minor(&to_amount, to_cur.decimals)?;

        let occurred = parse_occurred_at(pool, &occurred_at)
            .await?
            .unwrap_or_else(ep_core::unix_now);

        add_transfer_inner(
            pool,
            AddTransferFields {
                from_currency: from_cur.code,
                from_account,
                to_currency: to_cur.code,
                to_account,
                from_amount: from_minor,
                to_amount: to_minor,
                note: Some(note),
                occurred_at: occurred,
            },
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (
            from_account,
            to_account,
            from_amount,
            to_amount,
            note,
            occurred_at,
        );
        Err(ep_core::server_err("ssr-only"))
    }
}

// ---------------------------------------------------------------------------
// Account CRUD
// ---------------------------------------------------------------------------

/// Pure validator for account input. Run in both server fn (so `Args` errors
/// surface friendly messages) and in the helper (defense-in-depth for PAT
/// callers that bypass the server fn). Returns the trimmed values back.
#[cfg(feature = "ssr")]
fn validate_account_input(
    code: &str,
    name: &str,
    r#type: &str,
    tone: &str,
) -> Result<(String, String, String, String), ServerFnError> {
    let code = code.trim().to_string();
    let name = name.trim().to_string();
    let r#type = r#type.trim().to_string();
    let tone = tone.trim().to_string();
    if !code.is_empty()
        && (code.len() < MIN_ACCOUNT_CODE_CHARS || code.len() > MAX_ACCOUNT_CODE_CHARS)
    {
        return Err(ep_i18n::err("finance.err.account_code_format"));
    }
    if !code.is_empty()
        && !code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ep_i18n::err("finance.err.account_code_charset"));
    }
    if name.is_empty() || name.chars().count() > MAX_ACCOUNT_NAME_CHARS {
        return Err(ep_i18n::err("finance.err.account_name_format"));
    }
    if !ACCOUNT_TYPES.contains(&r#type.as_str()) {
        return Err(ep_i18n::err_with(
            "finance.err.account_type_invalid",
            format!("{ACCOUNT_TYPES:?}"),
        ));
    }
    if !tone.is_empty() && !TONES.contains(&tone.as_str()) {
        return Err(ep_i18n::err_with(
            "finance.err.tone_invalid",
            format!("{TONES:?}"),
        ));
    }
    Ok((code, name, r#type, tone))
}

/// `true` when an account row with `candidate` exists *other* than the
/// (optional) `exclude` code, within `currency_code`. Account codes are only
/// unique per-currency now, so the uniqueness search is currency-scoped.
#[cfg(feature = "ssr")]
async fn account_code_taken(
    conn: &mut sqlx::SqliteConnection,
    currency_code: &str,
    candidate: &str,
    exclude: Option<&str>,
) -> Result<bool, ServerFnError> {
    if exclude == Some(candidate) {
        return Ok(false);
    }
    let r: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM fin_account WHERE currency_code = ?1 AND code = ?2 LIMIT 1",
    )
    .bind(currency_code)
    .bind(candidate)
    .fetch_optional(&mut *conn)
    .await
    .map_err(server_err)?;
    Ok(r.is_some())
}

/// Pick a unique account code (within `currency_code`) that mirrors `name`.
/// Tries the ASCII slug first (e.g. "Cash Wallet" → "CASH-WALLET"). When the
/// slug is empty (non-ASCII names like "招行储蓄"), seeds the search from a
/// stable fingerprint hashed off `name` rather than always starting at
/// `ACC-1`. The `exclude` argument lets updates keep their current row's code
/// "available" against themselves.
#[cfg(feature = "ssr")]
async fn unique_account_code(
    conn: &mut sqlx::SqliteConnection,
    currency_code: &str,
    name: &str,
    exclude: Option<&str>,
) -> Result<String, ServerFnError> {
    let slug = slugify_to_code(name, MAX_ACCOUNT_CODE_CHARS);
    if !slug.is_empty() && !account_code_taken(conn, currency_code, &slug, exclude).await? {
        return Ok(slug);
    }
    let limit = fallback_seed_range("ACC-", MAX_ACCOUNT_CODE_CHARS);
    let seed = name_fingerprint(name, limit);
    for offset in 0..limit {
        let n = ((seed + offset) % limit) + 1;
        let candidate = format!("ACC-{n}");
        if candidate.len() > MAX_ACCOUNT_CODE_CHARS {
            continue;
        }
        if !account_code_taken(conn, currency_code, &candidate, exclude).await? {
            return Ok(candidate);
        }
    }
    Err(ep_i18n::err("finance.err.account_code_format"))
}

/// `true` when a category row with `candidate` exists *other* than the
/// (optional) `exclude` code, within `currency_code`.
#[cfg(feature = "ssr")]
async fn category_code_taken(
    conn: &mut sqlx::SqliteConnection,
    currency_code: &str,
    candidate: &str,
    exclude: Option<&str>,
) -> Result<bool, ServerFnError> {
    if exclude == Some(candidate) {
        return Ok(false);
    }
    let r: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM fin_category WHERE currency_code = ?1 AND code = ?2 LIMIT 1",
    )
    .bind(currency_code)
    .bind(candidate)
    .fetch_optional(&mut *conn)
    .await
    .map_err(server_err)?;
    Ok(r.is_some())
}

/// Pick a unique category code (within `currency_code`) that mirrors `name`.
/// Slug character class is `[A-Z&]` (matches `validate_category_input`);
/// fallback (when the name has no ASCII letters to slugify) is `CATN`, seeded
/// from a stable fingerprint of `name`.
#[cfg(feature = "ssr")]
async fn unique_category_code(
    conn: &mut sqlx::SqliteConnection,
    currency_code: &str,
    name: &str,
    exclude: Option<&str>,
) -> Result<String, ServerFnError> {
    let slug: String = slugify_to_code(name, MAX_CATEGORY_CODE_CHARS)
        .chars()
        .filter(|c| c.is_ascii_uppercase() || *c == '&')
        .collect();
    if !slug.is_empty() && !category_code_taken(conn, currency_code, &slug, exclude).await? {
        return Ok(slug);
    }
    let limit = fallback_seed_range("CAT", MAX_CATEGORY_CODE_CHARS);
    let seed = name_fingerprint(name, limit);
    for offset in 0..limit {
        let n = ((seed + offset) % limit) + 1;
        let candidate = format!("CAT{n}");
        if candidate.len() > MAX_CATEGORY_CODE_CHARS {
            continue;
        }
        if !category_code_taken(conn, currency_code, &candidate, exclude).await? {
            return Ok(candidate);
        }
    }
    Err(ep_i18n::err("finance.err.category_code_format"))
}

/// Largest `N` we'll try for a `{prefix}{N}` fallback code that still fits
/// `max_chars`. Returns the count of valid candidates (so callers can use
/// `1..=count`).
#[cfg(feature = "ssr")]
fn fallback_seed_range(prefix: &str, max_chars: usize) -> u64 {
    let digit_budget = max_chars.saturating_sub(prefix.len());
    if digit_budget == 0 {
        return 0;
    }
    let mut max: u64 = 0;
    for _ in 0..digit_budget {
        max = max * 10 + 9;
    }
    max
}

/// Stable per-name fingerprint mapped into `[0, modulus)`. Used to seed the
/// fallback `{prefix}{N}` search so different names land on different
/// starting slots even when the slugifier can't extract ASCII letters.
#[cfg(feature = "ssr")]
fn name_fingerprint(name: &str, modulus: u64) -> u64 {
    if modulus == 0 {
        return 0;
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for byte in name.as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x100000001b3);
    }
    h % modulus
}

/// Build an ASCII slug from a name suitable for use as an account/category
/// code. Strips non-ASCII (so Chinese names → empty slug, falling back to
/// the numbered scheme), uppercases ASCII letters, replaces runs of
/// non-alphanumeric runs with a single `-`, trims dashes.
#[cfg(feature = "ssr")]
fn slugify_to_code(name: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_dash = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            for c in ch.to_uppercase() {
                out.push(c);
            }
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > max_chars {
        out.truncate(max_chars);
        while out.ends_with('-') {
            out.pop();
        }
    }
    out
}

#[cfg(feature = "ssr")]
fn is_unique_violation(e: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = e {
        let code = db_err.code().map(|s| s.into_owned()).unwrap_or_default();
        return code == "2067" || code == "1555" || db_err.message().contains("UNIQUE");
    }
    false
}

#[cfg(feature = "ssr")]
pub async fn create_account_inner(
    pool: &SqlitePool,
    currency_code: String,
    code: String,
    name: String,
    r#type: String,
    tone: String,
    opening_balance: ep_core::MinorAmount,
) -> Result<Account, ServerFnError> {
    // Callers (server-fn wrapper, PAT handler) already `resolve_currency`,
    // so `currency_code` is expected to be a real registry row; the composite
    // FK on `fin_account.currency_code` is the structural backstop.
    let currency_code = currency_code.trim().to_string();
    if currency_code.is_empty() {
        return Err(ep_i18n::err("finance.err.no_currency"));
    }
    let (code, name, r#type, tone) = validate_account_input(&code, &name, &r#type, &tone)?;
    // Code auto-generation reads `fin_account` to pick a free slug, then the
    // INSERT writes it. Run both inside one transaction so a concurrent create
    // can't slip a row in between the search and the write (TOCTOU): a racing
    // INSERT either blocks until COMMIT or trips the unique violation below.
    let mut tx = pool.begin().await.map_err(server_err)?;
    let code = if code.is_empty() {
        unique_account_code(&mut tx, &currency_code, &name, None).await?
    } else {
        code
    };
    let res = sqlx::query(
        "INSERT INTO fin_account (currency_code, code, name, type, tone, balance, archived, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, unixepoch())",
    )
    .bind(&currency_code)
    .bind(&code)
    .bind(&name)
    .bind(&r#type)
    .bind(&tone)
    .bind(opening_balance)
    .execute(&mut *tx)
    .await;
    if let Err(e) = res {
        if is_unique_violation(&e) {
            return Err(ep_i18n::err_with("finance.err.account_code_taken", &code));
        }
        return Err(server_err(e));
    }
    tx.commit().await.map_err(server_err)?;
    sqlx::query_as::<_, Account>(
        "SELECT currency_code, code, name, type, tone, balance, archived, created_at
           FROM fin_account WHERE currency_code = ?1 AND code = ?2",
    )
    .bind(&currency_code)
    .bind(&code)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn update_account_inner(
    pool: &SqlitePool,
    currency_code: String,
    code: String,
    name: String,
    r#type: String,
    tone: String,
) -> Result<Account, ServerFnError> {
    update_account_inner_with(
        pool,
        currency_code,
        code,
        name,
        r#type,
        tone,
        /* rename_code */ true,
    )
    .await
}

/// Internal counterpart of [`update_account_inner`] that lets the caller opt
/// out of the name-driven code rename. The OpenAPI PATCH handler passes
/// `rename_code = false` so external API consumers keep a stable key for the
/// resource they just touched. An account's `currency_code` is immutable —
/// to move money between currencies, create a new account and transfer.
#[cfg(feature = "ssr")]
pub async fn update_account_inner_with(
    pool: &SqlitePool,
    currency_code: String,
    code: String,
    name: String,
    r#type: String,
    tone: String,
    rename_code: bool,
) -> Result<Account, ServerFnError> {
    let currency_code = currency_code.trim().to_string();
    if currency_code.is_empty() {
        return Err(ep_i18n::err("finance.err.no_currency"));
    }
    let (code, name, r#type, tone) = validate_account_input(&code, &name, &r#type, &tone)?;
    if code.is_empty() {
        return Err(ep_i18n::err("finance.err.account_code_format"));
    }
    let mut tx = pool.begin().await.map_err(server_err)?;
    // Re-derive the canonical code from the new name when the caller asks
    // for it. `defer_foreign_keys = ON` lets the FK check run at COMMIT time
    // rather than after every statement, so updating `fin_account.code`
    // before patching the `fin_txn.account_code` rows that still reference
    // the old key doesn't trip a constraint failure mid-transaction.
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    let cur_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM fin_account WHERE currency_code = ?1 AND code = ?2")
            .bind(&currency_code)
            .bind(&code)
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let name_changed = cur_name.as_deref() != Some(name.as_str());
    let new_code = if rename_code && name_changed {
        unique_account_code(&mut tx, &currency_code, &name, Some(&code)).await?
    } else {
        code.clone()
    };
    let res = if new_code != code {
        let res = sqlx::query(
            "UPDATE fin_account SET code = ?1, name = ?2, type = ?3, tone = ?4
              WHERE currency_code = ?5 AND code = ?6",
        )
        .bind(&new_code)
        .bind(&name)
        .bind(&r#type)
        .bind(&tone)
        .bind(&currency_code)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
        if res.rows_affected() == 0 {
            return Err(ep_i18n::err_with("finance.err.account_not_found", &code));
        }
        sqlx::query(
            "UPDATE fin_txn SET account_code = ?1 WHERE currency_code = ?2 AND account_code = ?3",
        )
        .bind(&new_code)
        .bind(&currency_code)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
        res
    } else {
        sqlx::query(
            "UPDATE fin_account SET name = ?1, type = ?2, tone = ?3
              WHERE currency_code = ?4 AND code = ?5",
        )
        .bind(&name)
        .bind(&r#type)
        .bind(&tone)
        .bind(&currency_code)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?
    };
    if res.rows_affected() == 0 {
        return Err(ep_i18n::err_with("finance.err.account_not_found", &code));
    }
    tx.commit().await.map_err(server_err)?;
    sqlx::query_as::<_, Account>(
        "SELECT currency_code, code, name, type, tone, balance, archived, created_at
           FROM fin_account WHERE currency_code = ?1 AND code = ?2",
    )
    .bind(&currency_code)
    .bind(&new_code)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn delete_account_inner(
    pool: &SqlitePool,
    currency_code: String,
    code: String,
) -> Result<(), ServerFnError> {
    let currency_code = currency_code.trim().to_string();
    let code = code.trim().to_string();
    if currency_code.is_empty() || code.is_empty() {
        return Err(ep_i18n::err("finance.err.account_code_format"));
    }
    let txn_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fin_txn WHERE currency_code = ?1 AND account_code = ?2",
    )
    .bind(&currency_code)
    .bind(&code)
    .fetch_one(pool)
    .await
    .map_err(server_err)?;
    if txn_count > 0 {
        return Err(ep_i18n::err_with(
            "finance.err.account_in_use",
            txn_count.to_string(),
        ));
    }
    let res = sqlx::query("DELETE FROM fin_account WHERE currency_code = ?1 AND code = ?2")
        .bind(&currency_code)
        .bind(&code)
        .execute(pool)
        .await
        .map_err(server_err)?;
    if res.rows_affected() == 0 {
        return Err(ep_i18n::err_with("finance.err.account_not_found", &code));
    }
    Ok(())
}

/// Flip an account's `archived` flag. Archiving keeps the row (and its
/// balance / transaction history) intact but drops it from the active pickers
/// and account grid — the graceful alternative to a delete that
/// `delete_account_inner` refuses once the account has transactions.
#[cfg(feature = "ssr")]
pub async fn set_account_archived_inner(
    pool: &SqlitePool,
    currency_code: String,
    code: String,
    archived: bool,
) -> Result<(), ServerFnError> {
    let currency_code = currency_code.trim().to_string();
    let code = code.trim().to_string();
    if currency_code.is_empty() || code.is_empty() {
        return Err(ep_i18n::err("finance.err.account_code_format"));
    }
    let res =
        sqlx::query("UPDATE fin_account SET archived = ?1 WHERE currency_code = ?2 AND code = ?3")
            .bind(i64::from(archived))
            .bind(&currency_code)
            .bind(&code)
            .execute(pool)
            .await
            .map_err(server_err)?;
    if res.rows_affected() == 0 {
        return Err(ep_i18n::err_with("finance.err.account_not_found", &code));
    }
    Ok(())
}

#[server(CreateAccount, "/api/_internal/fin", "Url", "create_account")]
pub async fn create_account(
    currency_code: String,
    code: String,
    name: String,
    r#type: String,
    tone: String,
    opening_balance: String,
) -> Result<Account, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let pool = &state.db;
        let currency = resolve_currency(pool, &currency_code).await?;
        // Opening balance is optional and may be negative (a credit card
        // starts in the red); a blank field means zero.
        let opening_balance = if opening_balance.trim().is_empty() {
            ep_core::MinorAmount::ZERO
        } else {
            ep_core::parse_minor(&opening_balance, currency.decimals).ok_or_else(|| {
                ep_i18n::err_with("finance.err.amount_invalid", opening_balance.trim())
            })?
        };
        create_account_inner(
            pool,
            currency.code,
            code,
            name,
            r#type,
            tone,
            opening_balance,
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (currency_code, code, name, r#type, tone, opening_balance);
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(UpdateAccount, "/api/_internal/fin", "Url", "update_account")]
pub async fn update_account(
    currency_code: String,
    code: String,
    name: String,
    r#type: String,
    tone: String,
) -> Result<Account, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        update_account_inner(&state.db, currency_code, code, name, r#type, tone).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (currency_code, code, name, r#type, tone);
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
pub async fn list_accounts_inner(
    pool: &SqlitePool,
    currency_code: &str,
) -> sqlx::Result<Vec<Account>> {
    sqlx::query_as::<_, Account>(
        "SELECT currency_code, code, name, type, tone, balance, archived, created_at
           FROM fin_account
          WHERE currency_code = ?1
          ORDER BY code ASC",
    )
    .bind(currency_code)
    .fetch_all(pool)
    .await
}

#[server(ListAccounts, "/api/_internal/fin", "Url", "list_accounts")]
pub async fn list_accounts(currency_code: String) -> Result<Vec<Account>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let currency = resolve_currency(&state.db, &currency_code).await?;
        list_accounts_inner(&state.db, &currency.code)
            .await
            .map_err(server_err)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = currency_code;
        Err(ep_core::server_err("ssr-only"))
    }
}

/// `account_ref` is the `"{currency_code}/{code}"` value the management UI's
/// delete control emits — a single field keeps it `RowDeleteAction`-shaped.
#[server(DeleteAccount, "/api/_internal/fin", "Url", "delete_account")]
pub async fn delete_account(account_ref: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let (currency_code, code) = split_currency_ref(&account_ref)?;
        delete_account_inner(&state.db, currency_code, code).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = account_ref;
        Err(ep_core::server_err("ssr-only"))
    }
}

/// Archive or unarchive an account. `account_ref` is the `"{currency}/{code}"`
/// value the management UI emits; `archived` is `"1"` to archive or `"0"` to
/// restore (form values arrive as strings — anything other than `"1"`/`"true"`
/// reads as unarchive).
#[server(
    SetAccountArchived,
    "/api/_internal/fin",
    "Url",
    "set_account_archived"
)]
pub async fn set_account_archived(
    account_ref: String,
    archived: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let (currency_code, code) = split_currency_ref(&account_ref)?;
        let archived = matches!(archived.trim(), "1" | "true");
        set_account_archived_inner(&state.db, currency_code, code, archived).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (account_ref, archived);
        Err(ep_core::server_err("ssr-only"))
    }
}

// ---------------------------------------------------------------------------
// Category CRUD
// ---------------------------------------------------------------------------

#[cfg(feature = "ssr")]
fn validate_category_input(
    code: &str,
    name: &str,
    icon: &str,
    tone: &str,
) -> Result<(String, String, String, String), ServerFnError> {
    let code = code.trim().to_string();
    let name = name.trim().to_string();
    let icon = icon.trim().to_string();
    let tone = tone.trim().to_string();
    // Inline char-class check (no regex dep). Accepts `&` for seed code
    // F&B and ASCII digits so the `CATN` fallback codes generated for
    // non-ASCII names survive a round-trip through this validator on update.
    // An empty `code` is allowed at the input boundary and gets a generated
    // value before insert (see `unique_category_code`).
    if !code.is_empty()
        && (code.len() > MAX_CATEGORY_CODE_CHARS
            || !code
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '&'))
    {
        return Err(ep_i18n::err("finance.err.category_code_format"));
    }
    if name.is_empty() || name.chars().count() > MAX_CATEGORY_NAME_CHARS {
        return Err(ep_i18n::err("finance.err.category_name_format"));
    }
    if icon.chars().count() > MAX_CATEGORY_ICON_CHARS {
        return Err(ep_i18n::err_with(
            "finance.err.category_icon_format",
            MAX_CATEGORY_ICON_CHARS,
        ));
    }
    if !tone.is_empty() && !TONES.contains(&tone.as_str()) {
        return Err(ep_i18n::err_with(
            "finance.err.tone_invalid",
            format!("{TONES:?}"),
        ));
    }
    Ok((code, name, icon, tone))
}

#[cfg(feature = "ssr")]
fn validate_category_sort_order(sort_order: i64) -> Result<i64, ServerFnError> {
    if sort_order < 0 {
        return Err(ep_i18n::err("finance.err.category_sort_order_invalid"));
    }
    Ok(sort_order)
}

/// Reject the reserved transfer-category code for user-facing category CRUD.
/// The `TFR` category per currency is module plumbing, not a user category.
#[cfg(feature = "ssr")]
fn reject_reserved_category(code: &str) -> Result<(), ServerFnError> {
    if code.eq_ignore_ascii_case(TRANSFER_CATEGORY_CODE) {
        return Err(ep_i18n::err("finance.err.category_reserved"));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn create_category_inner(
    pool: &SqlitePool,
    currency_code: String,
    code: String,
    name: String,
    icon: String,
    tone: String,
    sort_order: i64,
) -> Result<Category, ServerFnError> {
    // Callers already `resolve_currency`; the composite FK on
    // `fin_category.currency_code` is the structural backstop here.
    let currency_code = currency_code.trim().to_string();
    if currency_code.is_empty() {
        return Err(ep_i18n::err("finance.err.no_currency"));
    }
    let (code, name, icon, tone) = validate_category_input(&code, &name, &icon, &tone)?;
    if !code.is_empty() {
        reject_reserved_category(&code)?;
    }
    let sort_order = validate_category_sort_order(sort_order)?;
    // Same TOCTOU window as `create_account_inner`: auto code-gen searches the
    // table then the INSERT writes the chosen code, so both run inside one
    // transaction to commit atomically.
    let mut tx = pool.begin().await.map_err(server_err)?;
    let code = if code.is_empty() {
        unique_category_code(&mut tx, &currency_code, &name, None).await?
    } else {
        code
    };
    let res = sqlx::query(
        "INSERT INTO fin_category (currency_code, code, name, icon, tone, sort_order, archived, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, unixepoch())",
    )
    .bind(&currency_code)
    .bind(&code)
    .bind(&name)
    .bind(&icon)
    .bind(&tone)
    .bind(sort_order)
    .execute(&mut *tx)
    .await;
    if let Err(e) = res {
        if is_unique_violation(&e) {
            return Err(ep_i18n::err_with("finance.err.category_code_taken", &code));
        }
        return Err(server_err(e));
    }
    tx.commit().await.map_err(server_err)?;
    sqlx::query_as::<_, Category>(
        "SELECT currency_code, code, name, icon, tone, sort_order, archived, created_at
           FROM fin_category WHERE currency_code = ?1 AND code = ?2",
    )
    .bind(&currency_code)
    .bind(&code)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn update_category_inner(
    pool: &SqlitePool,
    currency_code: String,
    code: String,
    name: String,
    icon: String,
    tone: String,
    sort_order: i64,
) -> Result<Category, ServerFnError> {
    update_category_inner_with(
        pool,
        currency_code,
        code,
        name,
        icon,
        tone,
        sort_order,
        /* rename_code */ true,
    )
    .await
}

/// Internal counterpart of [`update_category_inner`]; mirrors
/// [`update_account_inner_with`]. A category's `currency_code` is immutable.
#[cfg(feature = "ssr")]
#[allow(
    clippy::too_many_arguments,
    reason = "internal update helper; parameters mirror the category form fields"
)]
pub async fn update_category_inner_with(
    pool: &SqlitePool,
    currency_code: String,
    code: String,
    name: String,
    icon: String,
    tone: String,
    sort_order: i64,
    rename_code: bool,
) -> Result<Category, ServerFnError> {
    let currency_code = currency_code.trim().to_string();
    if currency_code.is_empty() {
        return Err(ep_i18n::err("finance.err.no_currency"));
    }
    let (code, name, icon, tone) = validate_category_input(&code, &name, &icon, &tone)?;
    if code.is_empty() {
        return Err(ep_i18n::err("finance.err.category_code_format"));
    }
    // The reserved TFR category is not user-editable.
    reject_reserved_category(&code)?;
    let sort_order = validate_category_sort_order(sort_order)?;
    let mut tx = pool.begin().await.map_err(server_err)?;
    // Same cascade logic as `update_account_inner_with`: walk `fin_txn` and
    // `fin_budget` inside this transaction (SQLite has no ON UPDATE CASCADE),
    // and defer FK checks to COMMIT so the parent-then-children order works.
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    let cur_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM fin_category WHERE currency_code = ?1 AND code = ?2")
            .bind(&currency_code)
            .bind(&code)
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let name_changed = cur_name.as_deref() != Some(name.as_str());
    let new_code = if rename_code && name_changed {
        unique_category_code(&mut tx, &currency_code, &name, Some(&code)).await?
    } else {
        code.clone()
    };
    let res = if new_code != code {
        let res = sqlx::query(
            "UPDATE fin_category SET code = ?1, name = ?2, icon = ?3, tone = ?4, sort_order = ?5
              WHERE currency_code = ?6 AND code = ?7",
        )
        .bind(&new_code)
        .bind(&name)
        .bind(&icon)
        .bind(&tone)
        .bind(sort_order)
        .bind(&currency_code)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
        if res.rows_affected() == 0 {
            return Err(ep_i18n::err_with("finance.err.category_not_found", &code));
        }
        sqlx::query(
            "UPDATE fin_txn SET category_code = ?1 WHERE currency_code = ?2 AND category_code = ?3",
        )
        .bind(&new_code)
        .bind(&currency_code)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
        sqlx::query(
            "UPDATE fin_budget SET category_code = ?1 WHERE currency_code = ?2 AND category_code = ?3",
        )
        .bind(&new_code)
        .bind(&currency_code)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
        res
    } else {
        sqlx::query(
            "UPDATE fin_category SET name = ?1, icon = ?2, tone = ?3, sort_order = ?4
              WHERE currency_code = ?5 AND code = ?6",
        )
        .bind(&name)
        .bind(&icon)
        .bind(&tone)
        .bind(sort_order)
        .bind(&currency_code)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?
    };
    if res.rows_affected() == 0 {
        return Err(ep_i18n::err_with("finance.err.category_not_found", &code));
    }
    tx.commit().await.map_err(server_err)?;
    sqlx::query_as::<_, Category>(
        "SELECT currency_code, code, name, icon, tone, sort_order, archived, created_at
           FROM fin_category WHERE currency_code = ?1 AND code = ?2",
    )
    .bind(&currency_code)
    .bind(&new_code)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn delete_category_inner(
    pool: &SqlitePool,
    currency_code: String,
    code: String,
) -> Result<(), ServerFnError> {
    let currency_code = currency_code.trim().to_string();
    let code = code.trim().to_string();
    if currency_code.is_empty() || code.is_empty() {
        return Err(ep_i18n::err("finance.err.category_code_format"));
    }
    // The reserved TFR category cannot be deleted — transfers depend on it.
    reject_reserved_category(&code)?;
    let txn_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fin_txn WHERE currency_code = ?1 AND category_code = ?2",
    )
    .bind(&currency_code)
    .bind(&code)
    .fetch_one(pool)
    .await
    .map_err(server_err)?;
    if txn_count > 0 {
        return Err(ep_i18n::err_with(
            "finance.err.category_in_use",
            txn_count.to_string(),
        ));
    }
    let mut tx = pool.begin().await.map_err(server_err)?;
    sqlx::query("DELETE FROM fin_budget WHERE currency_code = ?1 AND category_code = ?2")
        .bind(&currency_code)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    let res = sqlx::query("DELETE FROM fin_category WHERE currency_code = ?1 AND code = ?2")
        .bind(&currency_code)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    if res.rows_affected() == 0 {
        return Err(ep_i18n::err_with("finance.err.category_not_found", &code));
    }
    tx.commit().await.map_err(server_err)?;
    Ok(())
}

/// Flip a category's `archived` flag. Mirrors [`set_account_archived_inner`]:
/// archiving keeps the row and its budget / transaction history but drops it
/// from the active pickers and dropdowns — the graceful alternative to a
/// delete that `delete_category_inner` refuses once the category is in use.
/// The reserved `TFR` category is module plumbing and cannot be archived.
#[cfg(feature = "ssr")]
pub async fn set_category_archived_inner(
    pool: &SqlitePool,
    currency_code: String,
    code: String,
    archived: bool,
) -> Result<(), ServerFnError> {
    let currency_code = currency_code.trim().to_string();
    let code = code.trim().to_string();
    if currency_code.is_empty() || code.is_empty() {
        return Err(ep_i18n::err("finance.err.category_code_format"));
    }
    reject_reserved_category(&code)?;
    let res =
        sqlx::query("UPDATE fin_category SET archived = ?1 WHERE currency_code = ?2 AND code = ?3")
            .bind(i64::from(archived))
            .bind(&currency_code)
            .bind(&code)
            .execute(pool)
            .await
            .map_err(server_err)?;
    if res.rows_affected() == 0 {
        return Err(ep_i18n::err_with("finance.err.category_not_found", &code));
    }
    Ok(())
}

#[server(CreateCategory, "/api/_internal/fin", "Url", "create_category")]
pub async fn create_category(
    currency_code: String,
    code: String,
    name: String,
    icon: String,
    tone: String,
    sort_order: i64,
) -> Result<Category, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        // Wrappers always resolve so the empty / unknown → primary fallback
        // lives in one place and `create_category_inner` can trust its input.
        let currency = resolve_currency(&state.db, &currency_code).await?;
        create_category_inner(&state.db, currency.code, code, name, icon, tone, sort_order).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (currency_code, code, name, icon, tone, sort_order);
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(UpdateCategory, "/api/_internal/fin", "Url", "update_category")]
pub async fn update_category(
    currency_code: String,
    code: String,
    name: String,
    icon: String,
    tone: String,
    sort_order: i64,
) -> Result<Category, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        update_category_inner(&state.db, currency_code, code, name, icon, tone, sort_order).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (currency_code, code, name, icon, tone, sort_order);
        Err(ep_core::server_err("ssr-only"))
    }
}

/// `category_ref` is the `"{currency_code}/{code}"` value the management UI's
/// delete control emits — a single field keeps it `RowDeleteAction`-shaped.
#[server(DeleteCategory, "/api/_internal/fin", "Url", "delete_category")]
pub async fn delete_category(category_ref: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let (currency_code, code) = split_currency_ref(&category_ref)?;
        delete_category_inner(&state.db, currency_code, code).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = category_ref;
        Err(ep_core::server_err("ssr-only"))
    }
}

/// Archive or unarchive a category. `category_ref` is the `"{currency}/{code}"`
/// value the management UI emits; `archived` is `"1"` to archive or `"0"` to
/// restore. The reserved `TFR` category is rejected server-side.
#[server(
    SetCategoryArchived,
    "/api/_internal/fin",
    "Url",
    "set_category_archived"
)]
pub async fn set_category_archived(
    category_ref: String,
    archived: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let (currency_code, code) = split_currency_ref(&category_ref)?;
        let archived = matches!(archived.trim(), "1" | "true");
        set_category_archived_inner(&state.db, currency_code, code, archived).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (category_ref, archived);
        Err(ep_core::server_err("ssr-only"))
    }
}
