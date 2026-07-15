use crate::model::*;
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;

#[cfg(feature = "ssr")]
use ep_core::{server_err, AppTimezone, CalendarDate};
#[cfg(feature = "ssr")]
use sqlx::SqlitePool;

#[cfg(feature = "ssr")]
const MAX_NAME_CHARS: usize = 80;
#[cfg(feature = "ssr")]
const MAX_MERCHANT_CHARS: usize = 128;
#[cfg(feature = "ssr")]
const MAX_NOTE_CHARS: usize = 2_000;
#[cfg(feature = "ssr")]
const MAX_ICON_CHARS: usize = 16;
#[cfg(feature = "ssr")]
const MAX_CURRENCY_CODE_CHARS: usize = 8;
#[cfg(feature = "ssr")]
const MAX_CURRENCY_SYMBOL_CHARS: usize = 8;

#[cfg(feature = "ssr")]
fn positive_id(id: i64, field: &str) -> Result<i64, ServerFnError> {
    if id > 0 {
        Ok(id)
    } else {
        Err(ep_i18n::err_with("finance.err.id_invalid", field))
    }
}

#[cfg(feature = "ssr")]
fn clean_required(value: &str, field: &str, max: usize) -> Result<String, ServerFnError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max {
        return Err(ep_i18n::err_with("finance.err.field_invalid", field));
    }
    Ok(value.to_string())
}

#[cfg(feature = "ssr")]
fn clean_optional(value: Option<&str>, max: usize) -> Result<Option<String>, ServerFnError> {
    let value = value.and_then(ep_core::trim_to_option);
    if value
        .as_deref()
        .is_some_and(|value| value.chars().count() > max)
    {
        return Err(ep_i18n::err_with("finance.err.note_too_long", max));
    }
    Ok(value)
}

#[cfg(feature = "ssr")]
fn validate_amount(amount: MinorAmount) -> Result<MinorAmount, ServerFnError> {
    if amount.is_within_input_limit() {
        Ok(amount)
    } else {
        Err(ep_i18n::err_with(
            "finance.err.amount_invalid",
            amount.to_string(),
        ))
    }
}

#[cfg(feature = "ssr")]
pub fn parse_signed_minor(input: &str, decimals: u8) -> Result<MinorAmount, ServerFnError> {
    let amount = crate::amount::parse_minor(input, decimals)
        .ok_or_else(|| ep_i18n::err_with("finance.err.amount_invalid", input.trim()))?;
    validate_amount(amount)
}

#[cfg(feature = "ssr")]
fn parse_positive_minor(input: &str, decimals: u8) -> Result<MinorAmount, ServerFnError> {
    let amount = parse_signed_minor(input, decimals)?;
    if !amount.is_positive() {
        return Err(ep_i18n::err("finance.err.amount_must_be_positive"));
    }
    Ok(amount)
}

#[cfg(feature = "ssr")]
fn checked_sum(
    values: impl IntoIterator<Item = MinorAmount>,
) -> Result<MinorAmount, ServerFnError> {
    MinorAmount::try_sum(values).ok_or_else(|| server_err("finance amount overflow"))
}

#[cfg(feature = "ssr")]
fn normalize_period(period: &str) -> Result<String, ServerFnError> {
    let period = period.trim();
    let Some((year, month)) = period.split_once('-') else {
        return Err(ep_i18n::err_with("finance.err.period_format", period));
    };
    let valid = year.len() == 4
        && month.len() == 2
        && year.bytes().all(|b| b.is_ascii_digit())
        && month.bytes().all(|b| b.is_ascii_digit())
        && year.parse::<u16>().is_ok_and(|v| v > 0)
        && month.parse::<u8>().is_ok_and(|v| (1..=12).contains(&v));
    if !valid {
        return Err(ep_i18n::err_with("finance.err.period_format", period));
    }
    Ok(period.to_string())
}

#[cfg(feature = "ssr")]
fn business_date(timezone: AppTimezone, occurred_at: i64) -> Result<String, ServerFnError> {
    if !ep_core::is_valid_app_timestamp(occurred_at) {
        return Err(ep_i18n::err_with("finance.err.date_format", occurred_at));
    }
    timezone
        .date(occurred_at)
        .map(ep_core::CalendarDate::ymd)
        .ok_or_else(|| ep_i18n::err_with("finance.err.date_format", occurred_at))
}

#[cfg(feature = "ssr")]
fn current_period(timezone: AppTimezone, now: i64) -> Result<String, ServerFnError> {
    timezone
        .date(now)
        .map(ep_core::CalendarDate::ym)
        .ok_or_else(|| server_err("finance current month is outside the supported date range"))
}

#[cfg(feature = "ssr")]
fn period_date_bounds(period: &str) -> Result<(String, String, String), ServerFnError> {
    let period = normalize_period(period)?;
    let (year, month) = period
        .split_once('-')
        .and_then(|(year, month)| Some((year.parse::<u16>().ok()?, month.parse::<u8>().ok()?)))
        .filter(|(year, _)| (1..=9998).contains(year))
        .ok_or_else(|| ep_i18n::err_with("finance.err.period_format", &period))?;
    let (end_year, end_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    Ok((
        period.clone(),
        format!("{period}-01"),
        format!("{end_year:04}-{end_month:02}-01"),
    ))
}

#[cfg(feature = "ssr")]
pub fn parse_occurred_at(timezone: AppTimezone, input: &str) -> Result<Option<i64>, ServerFnError> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(None);
    }
    let Some((year, month, day)) = ep_core::parse_ymd(input) else {
        return Err(ep_i18n::err_with("finance.err.date_format", input));
    };
    let timestamp = timezone
        .date_midpoint(CalendarDate { year, month, day })
        .ok_or_else(|| ep_i18n::err_with("finance.err.date_format", input))?;
    if !ep_core::is_valid_app_timestamp(timestamp) {
        return Err(ep_i18n::err_with("finance.err.date_format", input));
    }
    Ok(Some(timestamp))
}

// -------------------------------------------------------------------------
// Row queries
// -------------------------------------------------------------------------

#[cfg(feature = "ssr")]
const CURRENCY_SELECT: &str =
    "SELECT id, code, symbol, remark, decimals, is_primary, sort_order, created_at
       FROM fin_currency";

#[cfg(feature = "ssr")]
const ACCOUNT_SELECT: &str = "SELECT a.id, a.currency_id, c.code AS currency_code, a.name, a.type,
            a.tone, a.balance, a.archived, a.created_at
       FROM fin_account a JOIN fin_currency c ON c.id = a.currency_id";

#[cfg(feature = "ssr")]
const CATEGORY_SELECT: &str = "SELECT x.id, x.currency_id, c.code AS currency_code, x.name, x.icon,
            x.tone, x.sort_order, x.archived, x.created_at
       FROM fin_category x JOIN fin_currency c ON c.id = x.currency_id";

#[cfg(feature = "ssr")]
const TXN_SELECT: &str = "SELECT t.id, t.currency_id, c.code AS currency_code, t.occurred_at,
            t.occurred_on AS occurred_date,
            t.merchant, t.category_id, x.name AS category_name, t.account_id,
            a.name AS account_name, t.amount, t.tag, t.note, t.transfer_id,
            t.transfer_role, t.created_at, t.updated_at
       FROM fin_txn t
       JOIN fin_currency c ON c.id = t.currency_id
       JOIN fin_account a ON a.id = t.account_id
       LEFT JOIN fin_category x ON x.id = t.category_id";

#[cfg(feature = "ssr")]
const TRANSFER_SELECT: &str = "SELECT f.id, f.occurred_at, f.occurred_on AS occurred_date,
            f.from_account_id, fa.name AS from_account_name,
            fa.currency_id AS from_currency_id, fc.code AS from_currency_code,
            f.to_account_id, ta.name AS to_account_name,
            ta.currency_id AS to_currency_id, tc.code AS to_currency_code,
            f.from_amount, f.to_amount, f.note, f.created_at
       FROM fin_transfer f
       JOIN fin_account fa ON fa.id = f.from_account_id
       JOIN fin_currency fc ON fc.id = fa.currency_id
       JOIN fin_account ta ON ta.id = f.to_account_id
       JOIN fin_currency tc ON tc.id = ta.currency_id";

#[cfg(feature = "ssr")]
pub async fn list_currencies_inner(pool: &SqlitePool) -> Result<Vec<Currency>, ServerFnError> {
    sqlx::query_as::<_, Currency>(&format!(
        "{CURRENCY_SELECT} ORDER BY is_primary DESC, sort_order, code"
    ))
    .fetch_all(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn resolve_currency(pool: &SqlitePool, id: i64) -> Result<Currency, ServerFnError> {
    let sql = if id > 0 {
        format!("{CURRENCY_SELECT} WHERE id = ?1")
    } else {
        format!("{CURRENCY_SELECT} ORDER BY is_primary DESC, sort_order, id LIMIT 1")
    };
    let mut query = sqlx::query_as::<_, Currency>(&sql);
    if id > 0 {
        query = query.bind(id);
    }
    query
        .fetch_optional(pool)
        .await
        .map_err(server_err)?
        .ok_or_else(|| ep_i18n::err_with("finance.err.currency_not_found", id))
}

#[cfg(feature = "ssr")]
async fn fetch_currency_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> Result<Currency, ServerFnError> {
    sqlx::query_as::<_, Currency>(&format!("{CURRENCY_SELECT} WHERE id = ?1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(server_err)?
        .ok_or_else(|| ep_i18n::err_with("finance.err.currency_not_found", id))
}

#[cfg(feature = "ssr")]
pub(crate) async fn fetch_account(pool: &SqlitePool, id: i64) -> Result<Account, ServerFnError> {
    sqlx::query_as::<_, Account>(&format!("{ACCOUNT_SELECT} WHERE a.id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(server_err)?
        .ok_or_else(|| ep_i18n::err_with("finance.err.account_not_found", id))
}

#[cfg(feature = "ssr")]
pub(crate) async fn fetch_category(pool: &SqlitePool, id: i64) -> Result<Category, ServerFnError> {
    sqlx::query_as::<_, Category>(&format!("{CATEGORY_SELECT} WHERE x.id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(server_err)?
        .ok_or_else(|| ep_i18n::err_with("finance.err.category_not_found", id))
}

#[cfg(feature = "ssr")]
pub(crate) async fn fetch_txn(pool: &SqlitePool, id: i64) -> Result<Txn, ServerFnError> {
    sqlx::query_as::<_, Txn>(&format!("{TXN_SELECT} WHERE t.id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(server_err)?
        .ok_or_else(|| ep_i18n::err_with("finance.err.txn_not_found", id))
}

#[cfg(feature = "ssr")]
async fn fetch_account_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> Result<Account, ServerFnError> {
    sqlx::query_as::<_, Account>(&format!("{ACCOUNT_SELECT} WHERE a.id = ?1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(server_err)?
        .ok_or_else(|| ep_i18n::err_with("finance.err.account_not_found", id))
}

#[cfg(feature = "ssr")]
async fn fetch_category_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> Result<Category, ServerFnError> {
    sqlx::query_as::<_, Category>(&format!("{CATEGORY_SELECT} WHERE x.id = ?1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(server_err)?
        .ok_or_else(|| ep_i18n::err_with("finance.err.category_not_found", id))
}

#[cfg(feature = "ssr")]
async fn fetch_txn_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> Result<Txn, ServerFnError> {
    sqlx::query_as::<_, Txn>(&format!("{TXN_SELECT} WHERE t.id = ?1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(server_err)?
        .ok_or_else(|| ep_i18n::err_with("finance.err.txn_not_found", id))
}

#[cfg(feature = "ssr")]
pub(crate) async fn fetch_transfer(pool: &SqlitePool, id: i64) -> Result<Transfer, ServerFnError> {
    sqlx::query_as::<_, Transfer>(&format!("{TRANSFER_SELECT} WHERE f.id = ?1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(server_err)?
        .ok_or_else(|| ep_i18n::err_with("finance.err.transfer_not_found", id))
}

// -------------------------------------------------------------------------
// Currency CRUD
// -------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub async fn create_currency_inner(
    pool: &SqlitePool,
    code: String,
    symbol: String,
    remark: String,
    decimals: i64,
    sort_order: i64,
) -> Result<Currency, ServerFnError> {
    let code = clean_required(&code.to_ascii_uppercase(), "code", MAX_CURRENCY_CODE_CHARS)?;
    if !code
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Err(ep_i18n::err_with("finance.err.field_invalid", "code"));
    }
    let symbol = clean_required(&symbol, "symbol", MAX_CURRENCY_SYMBOL_CHARS)?;
    let remark = remark
        .trim()
        .chars()
        .take(MAX_NAME_CHARS)
        .collect::<String>();
    let decimals = u8::try_from(decimals)
        .ok()
        .filter(|v| *v <= 18)
        .ok_or_else(|| ep_i18n::err_with("finance.err.field_invalid", "decimals"))?;
    let result = sqlx::query(
        "INSERT INTO fin_currency (code, symbol, remark, decimals, is_primary, sort_order)
         VALUES (?1, ?2, ?3, ?4, 0, ?5)",
    )
    .bind(&code)
    .bind(&symbol)
    .bind(&remark)
    .bind(decimals)
    .bind(sort_order)
    .execute(pool)
    .await
    .map_err(|error| {
        if error.to_string().contains("UNIQUE") {
            ep_i18n::err_with("finance.err.currency_exists", &code)
        } else {
            server_err(error)
        }
    })?;
    resolve_currency(pool, result.last_insert_rowid()).await
}

#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Default)]
pub struct CurrencyPatchFields {
    pub symbol: Option<String>,
    pub remark: Option<String>,
    pub decimals: Option<i64>,
    pub sort_order: Option<i64>,
}

#[cfg(feature = "ssr")]
pub async fn patch_currency_inner(
    pool: &SqlitePool,
    id: i64,
    fields: CurrencyPatchFields,
) -> Result<Currency, ServerFnError> {
    let id = positive_id(id, "currency_id")?;
    let symbol = fields
        .symbol
        .as_deref()
        .map(|value| clean_required(value, "symbol", MAX_CURRENCY_SYMBOL_CHARS))
        .transpose()?;
    let remark = fields.remark.map(|value| {
        value
            .trim()
            .chars()
            .take(MAX_NAME_CHARS)
            .collect::<String>()
    });
    let decimals = fields
        .decimals
        .map(|value| {
            u8::try_from(value)
                .ok()
                .filter(|value| *value <= 18)
                .ok_or_else(|| ep_i18n::err_with("finance.err.field_invalid", "decimals"))
        })
        .transpose()?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let current = fetch_currency_tx(&mut tx, id).await?;
    // Decimal precision defines the meaning of every stored minor-unit value.
    // It is immutable after currency creation; delete an unused currency and
    // recreate it instead of reinterpreting historical amounts.
    if decimals.is_some_and(|decimals| decimals != current.decimals) {
        return Err(ep_i18n::err("finance.err.currency_decimals_locked"));
    }
    let symbol = symbol.unwrap_or(current.symbol);
    let remark = remark.unwrap_or(current.remark);
    let sort_order = fields.sort_order.unwrap_or(current.sort_order);
    let result = sqlx::query(
        "UPDATE fin_currency SET symbol = ?1, remark = ?2, sort_order = ?3 WHERE id = ?4",
    )
    .bind(symbol)
    .bind(remark)
    .bind(sort_order)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    if result.rows_affected() == 0 {
        return Err(ep_i18n::err_with("finance.err.currency_not_found", id));
    }
    let currency = fetch_currency_tx(&mut tx, id).await?;
    tx.commit().await.map_err(server_err)?;
    Ok(currency)
}

#[cfg(feature = "ssr")]
pub async fn set_primary_currency_inner(pool: &SqlitePool, id: i64) -> Result<(), ServerFnError> {
    let id = positive_id(id, "currency_id")?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_currency WHERE id = ?1)")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(server_err)?;
    if exists == 0 {
        return Err(ep_i18n::err_with("finance.err.currency_not_found", id));
    }
    sqlx::query("UPDATE fin_currency SET is_primary = 0 WHERE is_primary = 1")
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    sqlx::query("UPDATE fin_currency SET is_primary = 1 WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    tx.commit().await.map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn delete_currency_inner(pool: &SqlitePool, id: i64) -> Result<bool, ServerFnError> {
    let id = positive_id(id, "currency_id")?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let primary: Option<bool> =
        sqlx::query_scalar("SELECT is_primary FROM fin_currency WHERE id = ?1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let primary = primary.ok_or_else(|| ep_i18n::err_with("finance.err.currency_not_found", id))?;
    if primary {
        return Err(ep_i18n::err("finance.err.currency_primary_delete"));
    }
    let used: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fin_account WHERE currency_id = ?1)
             OR EXISTS(SELECT 1 FROM fin_category WHERE currency_id = ?1)
             OR EXISTS(SELECT 1 FROM fin_txn WHERE currency_id = ?1)",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(server_err)?;
    if used != 0 {
        return Err(ep_i18n::err("finance.err.currency_in_use"));
    }
    let result = sqlx::query("DELETE FROM fin_currency WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    let deleted = result.rows_affected() != 0;
    tx.commit().await.map_err(server_err)?;
    Ok(deleted)
}

// -------------------------------------------------------------------------
// Account/category CRUD
// -------------------------------------------------------------------------

#[cfg(feature = "ssr")]
async fn adjust_account_balance(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: i64,
    delta: MinorAmount,
) -> Result<(), ServerFnError> {
    let current: Option<MinorAmount> =
        sqlx::query_scalar("SELECT balance FROM fin_account WHERE id = ?1")
            .bind(account_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(server_err)?;
    let current =
        current.ok_or_else(|| ep_i18n::err_with("finance.err.account_not_found", account_id))?;
    let next = current
        .checked_add(delta)
        .ok_or_else(|| server_err("finance amount overflow"))?;
    sqlx::query("UPDATE fin_account SET balance = ?1 WHERE id = ?2")
        .bind(next)
        .bind(account_id)
        .execute(&mut **tx)
        .await
        .map_err(server_err)?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn list_accounts_inner(
    pool: &SqlitePool,
    currency_id: Option<i64>,
    include_archived: bool,
) -> Result<Vec<Account>, ServerFnError> {
    let sql = match (currency_id, include_archived) {
        (Some(_), true) => format!("{ACCOUNT_SELECT} WHERE a.currency_id = ?1 ORDER BY a.archived, a.created_at, a.id"),
        (Some(_), false) => format!("{ACCOUNT_SELECT} WHERE a.currency_id = ?1 AND a.archived = 0 ORDER BY a.created_at, a.id"),
        (None, true) => format!("{ACCOUNT_SELECT} ORDER BY c.sort_order, a.archived, a.created_at, a.id"),
        (None, false) => format!("{ACCOUNT_SELECT} WHERE a.archived = 0 ORDER BY c.sort_order, a.created_at, a.id"),
    };
    let mut query = sqlx::query_as::<_, Account>(&sql);
    if let Some(currency_id) = currency_id {
        query = query.bind(positive_id(currency_id, "currency_id")?);
    }
    query.fetch_all(pool).await.map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn create_account_inner(
    pool: &SqlitePool,
    currency_id: i64,
    name: String,
    r#type: String,
    tone: String,
    opening_balance: MinorAmount,
) -> Result<Account, ServerFnError> {
    let currency_id = positive_id(currency_id, "currency_id")?;
    let _ = resolve_currency(pool, currency_id).await?;
    let name = clean_required(&name, "name", MAX_NAME_CHARS)?;
    let r#type = clean_required(&r#type, "type", 32)?;
    if !ACCOUNT_TYPES.contains(&r#type.as_str()) {
        return Err(ep_i18n::err_with(
            "finance.err.account_type_invalid",
            r#type,
        ));
    }
    let tone = tone.trim().to_string();
    if !tone.is_empty() && !TONES.contains(&tone.as_str()) {
        return Err(ep_i18n::err_with("finance.err.tone_invalid", tone));
    }
    let opening_balance = validate_amount(opening_balance)?;
    let result = sqlx::query(
        "INSERT INTO fin_account (currency_id, name, type, tone, balance)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(currency_id)
    .bind(&name)
    .bind(&r#type)
    .bind(&tone)
    .bind(opening_balance)
    .execute(pool)
    .await
    .map_err(|error| {
        if error.to_string().contains("UNIQUE") {
            ep_i18n::err_with("finance.err.account_name_taken", &name)
        } else {
            server_err(error)
        }
    })?;
    fetch_account(pool, result.last_insert_rowid()).await
}

#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Default)]
pub struct AccountPatchFields {
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub tone: Option<String>,
    pub archived: Option<bool>,
}

#[cfg(feature = "ssr")]
pub async fn patch_account_inner(
    pool: &SqlitePool,
    id: i64,
    fields: AccountPatchFields,
) -> Result<Account, ServerFnError> {
    let id = positive_id(id, "account_id")?;
    let name = fields
        .name
        .as_deref()
        .map(|value| clean_required(value, "name", MAX_NAME_CHARS))
        .transpose()?;
    let r#type = fields
        .r#type
        .as_deref()
        .map(|value| clean_required(value, "type", 32))
        .transpose()?;
    if r#type
        .as_deref()
        .is_some_and(|value| !ACCOUNT_TYPES.contains(&value))
    {
        return Err(ep_i18n::err_with(
            "finance.err.account_type_invalid",
            r#type.as_deref().unwrap_or_default(),
        ));
    }
    let tone = fields.tone.map(|value| value.trim().to_string());
    if tone
        .as_deref()
        .is_some_and(|value| !value.is_empty() && !TONES.contains(&value))
    {
        return Err(ep_i18n::err_with(
            "finance.err.tone_invalid",
            tone.as_deref().unwrap_or_default(),
        ));
    }

    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let current = fetch_account_tx(&mut tx, id).await?;
    let name = name.unwrap_or(current.name);
    let r#type = r#type.unwrap_or(current.r#type);
    let tone = tone.unwrap_or(current.tone);
    let archived = fields.archived.unwrap_or(current.archived);
    let result = sqlx::query(
        "UPDATE fin_account
            SET name = ?1, type = ?2, tone = ?3, archived = ?4
          WHERE id = ?5",
    )
    .bind(&name)
    .bind(r#type)
    .bind(tone)
    .bind(archived)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        if error.to_string().contains("UNIQUE") {
            ep_i18n::err_with("finance.err.account_name_taken", &name)
        } else {
            server_err(error)
        }
    })?;
    if result.rows_affected() == 0 {
        return Err(ep_i18n::err_with("finance.err.account_not_found", id));
    }
    let account = fetch_account_tx(&mut tx, id).await?;
    tx.commit().await.map_err(server_err)?;
    Ok(account)
}

#[cfg(feature = "ssr")]
pub async fn delete_account_inner(pool: &SqlitePool, id: i64) -> Result<bool, ServerFnError> {
    let id = positive_id(id, "account_id")?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let balance: Option<MinorAmount> =
        sqlx::query_scalar("SELECT balance FROM fin_account WHERE id = ?1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let balance = balance.ok_or_else(|| ep_i18n::err_with("finance.err.account_not_found", id))?;
    let used: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fin_txn WHERE account_id = ?1)
             OR EXISTS(SELECT 1 FROM fin_transfer WHERE from_account_id = ?1 OR to_account_id = ?1)",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(server_err)?;
    if used != 0 || balance != MinorAmount::ZERO {
        return Err(ep_i18n::err("finance.err.account_in_use"));
    }
    let deleted = sqlx::query("DELETE FROM fin_account WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?
        .rows_affected()
        != 0;
    tx.commit().await.map_err(server_err)?;
    Ok(deleted)
}

#[cfg(feature = "ssr")]
pub async fn list_categories_inner(
    pool: &SqlitePool,
    currency_id: i64,
    include_archived: bool,
) -> Result<Vec<Category>, ServerFnError> {
    let suffix = if include_archived {
        "WHERE x.currency_id = ?1 ORDER BY x.archived, x.sort_order, x.id"
    } else {
        "WHERE x.currency_id = ?1 AND x.archived = 0 ORDER BY x.sort_order, x.id"
    };
    sqlx::query_as::<_, Category>(&format!("{CATEGORY_SELECT} {suffix}"))
        .bind(positive_id(currency_id, "currency_id")?)
        .fetch_all(pool)
        .await
        .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn create_category_inner(
    pool: &SqlitePool,
    currency_id: i64,
    name: String,
    icon: String,
    tone: String,
    sort_order: i64,
) -> Result<Category, ServerFnError> {
    let currency_id = positive_id(currency_id, "currency_id")?;
    let _ = resolve_currency(pool, currency_id).await?;
    let name = clean_required(&name, "name", MAX_NAME_CHARS)?;
    let icon = icon.trim().chars().take(MAX_ICON_CHARS).collect::<String>();
    let tone = tone.trim().to_string();
    if !tone.is_empty() && !TONES.contains(&tone.as_str()) {
        return Err(ep_i18n::err_with("finance.err.tone_invalid", tone));
    }
    let result = sqlx::query(
        "INSERT INTO fin_category (currency_id, name, icon, tone, sort_order)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(currency_id)
    .bind(&name)
    .bind(icon)
    .bind(tone)
    .bind(sort_order)
    .execute(pool)
    .await
    .map_err(|error| {
        if error.to_string().contains("UNIQUE") {
            ep_i18n::err_with("finance.err.category_name_taken", &name)
        } else {
            server_err(error)
        }
    })?;
    fetch_category(pool, result.last_insert_rowid()).await
}

#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Default)]
pub struct CategoryPatchFields {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub tone: Option<String>,
    pub sort_order: Option<i64>,
    pub archived: Option<bool>,
}

#[cfg(feature = "ssr")]
pub async fn patch_category_inner(
    pool: &SqlitePool,
    id: i64,
    fields: CategoryPatchFields,
) -> Result<Category, ServerFnError> {
    let id = positive_id(id, "category_id")?;
    let name = fields
        .name
        .as_deref()
        .map(|value| clean_required(value, "name", MAX_NAME_CHARS))
        .transpose()?;
    let icon = fields.icon.map(|value| {
        value
            .trim()
            .chars()
            .take(MAX_ICON_CHARS)
            .collect::<String>()
    });
    let tone = fields.tone.map(|value| value.trim().to_string());
    if tone
        .as_deref()
        .is_some_and(|value| !value.is_empty() && !TONES.contains(&value))
    {
        return Err(ep_i18n::err_with(
            "finance.err.tone_invalid",
            tone.as_deref().unwrap_or_default(),
        ));
    }

    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let current = fetch_category_tx(&mut tx, id).await?;
    let name = name.unwrap_or(current.name);
    let icon = icon.unwrap_or(current.icon);
    let tone = tone.unwrap_or(current.tone);
    let sort_order = fields.sort_order.unwrap_or(current.sort_order);
    let archived = fields.archived.unwrap_or(current.archived);
    let result = sqlx::query(
        "UPDATE fin_category
            SET name = ?1, icon = ?2, tone = ?3, sort_order = ?4, archived = ?5
          WHERE id = ?6",
    )
    .bind(&name)
    .bind(icon)
    .bind(tone)
    .bind(sort_order)
    .bind(archived)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        if error.to_string().contains("UNIQUE") {
            ep_i18n::err_with("finance.err.category_name_taken", &name)
        } else {
            server_err(error)
        }
    })?;
    if result.rows_affected() == 0 {
        return Err(ep_i18n::err_with("finance.err.category_not_found", id));
    }
    let category = fetch_category_tx(&mut tx, id).await?;
    tx.commit().await.map_err(server_err)?;
    Ok(category)
}

#[cfg(feature = "ssr")]
pub async fn delete_category_inner(pool: &SqlitePool, id: i64) -> Result<bool, ServerFnError> {
    let id = positive_id(id, "category_id")?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let used: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_txn WHERE category_id = ?1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(server_err)?;
    if used != 0 {
        return Err(ep_i18n::err("finance.err.category_in_use"));
    }
    sqlx::query("DELETE FROM fin_budget WHERE category_id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    let deleted = sqlx::query("DELETE FROM fin_category WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?
        .rows_affected()
        != 0;
    tx.commit().await.map_err(server_err)?;
    Ok(deleted)
}

// -------------------------------------------------------------------------
// Transactions and module-owned transfers
// -------------------------------------------------------------------------

#[cfg(feature = "ssr")]
#[derive(Debug, Clone)]
pub struct AddTxnFields {
    pub currency_id: i64,
    pub merchant: String,
    pub category_id: i64,
    pub account_id: i64,
    pub amount: MinorAmount,
    pub tag: String,
    pub note: Option<String>,
    pub occurred_at: i64,
}

/// A partial Open API transaction mutation. `amount`, when supplied, is a
/// major-unit magnitude and inherits the existing transaction's sign. The
/// nested `note` option distinguishes omission from an explicit JSON `null`.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Default)]
pub struct TxnPatchFields {
    pub merchant: Option<String>,
    pub category_id: Option<i64>,
    pub account_id: Option<i64>,
    pub amount: Option<String>,
    pub note: Option<Option<String>>,
    pub occurred_at: Option<i64>,
}

#[cfg(feature = "ssr")]
enum TxnAmountPatch {
    Preserve,
    Major(String),
}

#[cfg(feature = "ssr")]
struct TxnMutationFields {
    merchant: Option<String>,
    category_id: Option<i64>,
    account_id: Option<i64>,
    amount: TxnAmountPatch,
    note: Option<Option<String>>,
    occurred_at: Option<i64>,
}

#[cfg(feature = "ssr")]
async fn validate_txn_refs_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    currency_id: i64,
    category_id: i64,
    account_id: i64,
) -> Result<(), ServerFnError> {
    let category_ok: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fin_category
          WHERE id = ?1 AND currency_id = ?2 AND archived = 0)",
    )
    .bind(category_id)
    .bind(currency_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(server_err)?;
    let account_ok: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fin_account
          WHERE id = ?1 AND currency_id = ?2 AND archived = 0)",
    )
    .bind(account_id)
    .bind(currency_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(server_err)?;
    if category_ok == 0 {
        return Err(ep_i18n::err_with(
            "finance.err.category_not_found",
            category_id,
        ));
    }
    if account_ok == 0 {
        return Err(ep_i18n::err_with(
            "finance.err.account_not_found",
            account_id,
        ));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn add_txn_inner(
    pool: &SqlitePool,
    timezone: AppTimezone,
    fields: AddTxnFields,
) -> Result<Txn, ServerFnError> {
    let currency_id = positive_id(fields.currency_id, "currency_id")?;
    let category_id = positive_id(fields.category_id, "category_id")?;
    let account_id = positive_id(fields.account_id, "account_id")?;
    let occurred_on = business_date(timezone, fields.occurred_at)?;
    let merchant = clean_required(&fields.merchant, "merchant", MAX_MERCHANT_CHARS)?;
    let note = clean_optional(fields.note.as_deref(), MAX_NOTE_CHARS)?;
    let tag = Tag::parse(fields.tag.trim())
        .filter(|tag| tag.is_single_entry())
        .ok_or_else(|| ep_i18n::err_with("finance.err.tag_invalid", fields.tag.trim()))?;
    let amount = validate_amount(fields.amount)?;
    match tag {
        Tag::Exp if amount.is_negative() => {}
        Tag::Inc if amount.is_positive() => {}
        _ => return Err(ep_i18n::err("finance.err.amount_sign_invalid")),
    }
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    validate_txn_refs_tx(&mut tx, currency_id, category_id, account_id).await?;
    let result = sqlx::query(
        "INSERT INTO fin_txn
            (currency_id, occurred_at, occurred_on, merchant, category_id, account_id,
             amount, tag, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(currency_id)
    .bind(fields.occurred_at)
    .bind(occurred_on)
    .bind(merchant)
    .bind(category_id)
    .bind(account_id)
    .bind(amount)
    .bind(tag.as_str())
    .bind(note)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    let id = result.last_insert_rowid();
    adjust_account_balance(&mut tx, account_id, amount).await?;
    tx.commit().await.map_err(server_err)?;
    fetch_txn(pool, id).await
}

#[cfg(feature = "ssr")]
pub async fn patch_txn_inner(
    pool: &SqlitePool,
    timezone: AppTimezone,
    id: i64,
    fields: TxnPatchFields,
) -> Result<Txn, ServerFnError> {
    mutate_txn_inner(
        pool,
        timezone,
        id,
        TxnMutationFields {
            merchant: fields.merchant,
            category_id: fields.category_id,
            account_id: fields.account_id,
            amount: fields
                .amount
                .map_or(TxnAmountPatch::Preserve, TxnAmountPatch::Major),
            note: fields.note,
            occurred_at: fields.occurred_at,
        },
    )
    .await
}

#[cfg(feature = "ssr")]
async fn mutate_txn_inner(
    pool: &SqlitePool,
    timezone: AppTimezone,
    id: i64,
    fields: TxnMutationFields,
) -> Result<Txn, ServerFnError> {
    let id = positive_id(id, "transaction_id")?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let current = fetch_txn_tx(&mut tx, id).await?;
    if current.tag == Tag::Tfr.as_str() {
        return Err(ep_i18n::err("finance.err.transfer_edit_forbidden"));
    }

    let merchant = clean_required(
        fields.merchant.as_deref().unwrap_or(&current.merchant),
        "merchant",
        MAX_MERCHANT_CHARS,
    )?;
    let category_id = fields
        .category_id
        .or(current.category_id)
        .ok_or_else(|| ep_i18n::err_with("finance.err.category_not_found", 0))?;
    let category_id = positive_id(category_id, "category_id")?;
    let account_id = positive_id(
        fields.account_id.unwrap_or(current.account_id),
        "account_id",
    )?;
    let note = clean_optional(
        fields
            .note
            .as_ref()
            .map_or(current.note.as_deref(), |note| note.as_deref()),
        MAX_NOTE_CHARS,
    )?;
    let (occurred_at, occurred_on) = match fields.occurred_at {
        Some(occurred_at) if occurred_at == current.occurred_at => {
            (occurred_at, current.occurred_date.clone())
        }
        Some(occurred_at) => (occurred_at, business_date(timezone, occurred_at)?),
        None => (current.occurred_at, current.occurred_date.clone()),
    };
    let amount = match fields.amount {
        TxnAmountPatch::Preserve => current.amount,
        TxnAmountPatch::Major(value) => {
            let decimals: u8 =
                sqlx::query_scalar("SELECT decimals FROM fin_currency WHERE id = ?1")
                    .bind(current.currency_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(server_err)?;
            let magnitude = parse_signed_minor(&value, decimals)?.abs();
            if current.tag == Tag::Exp.as_str() {
                magnitude
                    .checked_neg()
                    .ok_or_else(|| server_err("finance amount overflow"))?
            } else {
                magnitude
            }
        }
    };
    match current.tag.as_str() {
        "exp" if amount.is_negative() => {}
        "inc" if amount.is_positive() => {}
        _ => return Err(ep_i18n::err("finance.err.amount_sign_invalid")),
    }
    validate_txn_refs_tx(&mut tx, current.currency_id, category_id, account_id).await?;
    let inverse = current
        .amount
        .checked_neg()
        .ok_or_else(|| server_err("finance amount overflow"))?;
    adjust_account_balance(&mut tx, current.account_id, inverse).await?;
    adjust_account_balance(&mut tx, account_id, amount).await?;
    let updated = sqlx::query(
        "UPDATE fin_txn
            SET merchant = ?1, category_id = ?2, account_id = ?3, amount = ?4,
                note = ?5, occurred_at = ?6, occurred_on = ?7, updated_at = unixepoch()
          WHERE id = ?8 AND transfer_id IS NULL",
    )
    .bind(merchant)
    .bind(category_id)
    .bind(account_id)
    .bind(amount)
    .bind(note)
    .bind(occurred_at)
    .bind(occurred_on)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?
    .rows_affected();
    if updated != 1 {
        return Err(ep_i18n::err_with("finance.err.txn_not_found", id));
    }
    let transaction = fetch_txn_tx(&mut tx, id).await?;
    tx.commit().await.map_err(server_err)?;
    Ok(transaction)
}

#[cfg(feature = "ssr")]
pub async fn delete_txn_inner(pool: &SqlitePool, id: i64) -> Result<bool, ServerFnError> {
    let id = positive_id(id, "transaction_id")?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let Some(row) = sqlx::query_as::<_, (MinorAmount, i64, Option<i64>)>(
        "SELECT amount, account_id, transfer_id FROM fin_txn WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(server_err)?
    else {
        return Ok(false);
    };
    if let Some(transfer_id) = row.2 {
        let deleted = delete_transfer_in_tx(&mut tx, transfer_id).await?;
        tx.commit().await.map_err(server_err)?;
        return Ok(deleted);
    }
    adjust_account_balance(
        &mut tx,
        row.1,
        row.0
            .checked_neg()
            .ok_or_else(|| server_err("finance amount overflow"))?,
    )
    .await?;
    sqlx::query("DELETE FROM fin_txn WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(true)
}

#[cfg(feature = "ssr")]
#[derive(Debug, Clone)]
pub struct AddTransferFields {
    pub from_account_id: i64,
    pub to_account_id: i64,
    pub from_amount: MinorAmount,
    pub to_amount: MinorAmount,
    pub note: Option<String>,
    pub occurred_at: i64,
}

#[cfg(feature = "ssr")]
pub async fn add_transfer_inner(
    pool: &SqlitePool,
    timezone: AppTimezone,
    fields: AddTransferFields,
) -> Result<Transfer, ServerFnError> {
    let from_account_id = positive_id(fields.from_account_id, "from_account_id")?;
    let to_account_id = positive_id(fields.to_account_id, "to_account_id")?;
    if from_account_id == to_account_id {
        return Err(ep_i18n::err("finance.err.transfer_accounts_same"));
    }
    let occurred_on = business_date(timezone, fields.occurred_at)?;
    let from_amount = validate_amount(fields.from_amount)?;
    let to_amount = validate_amount(fields.to_amount)?;
    if !from_amount.is_positive() || !to_amount.is_positive() {
        return Err(ep_i18n::err("finance.err.amount_must_be_positive"));
    }
    let note = clean_optional(fields.note.as_deref(), MAX_NOTE_CHARS)?;
    let signed_from = from_amount
        .checked_neg()
        .ok_or_else(|| server_err("finance amount overflow"))?;

    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let from_account: Option<(i64, String, bool)> =
        sqlx::query_as("SELECT currency_id, name, archived FROM fin_account WHERE id = ?1")
            .bind(from_account_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let from_account = from_account
        .ok_or_else(|| ep_i18n::err_with("finance.err.account_not_found", from_account_id))?;
    let to_account: Option<(i64, String, bool)> =
        sqlx::query_as("SELECT currency_id, name, archived FROM fin_account WHERE id = ?1")
            .bind(to_account_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let to_account = to_account
        .ok_or_else(|| ep_i18n::err_with("finance.err.account_not_found", to_account_id))?;
    if from_account.2 || to_account.2 {
        return Err(ep_i18n::err("finance.err.transfer_account_archived"));
    }
    let transfer_id = sqlx::query(
        "INSERT INTO fin_transfer
            (occurred_at, occurred_on, from_account_id, to_account_id,
             from_amount, to_amount, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(fields.occurred_at)
    .bind(&occurred_on)
    .bind(from_account_id)
    .bind(to_account_id)
    .bind(from_amount)
    .bind(to_amount)
    .bind(&note)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?
    .last_insert_rowid();
    sqlx::query(
        "INSERT INTO fin_txn
            (currency_id, occurred_at, occurred_on, merchant, category_id, account_id,
             amount, tag, note, transfer_id, transfer_role)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, 'tfr', ?7, ?8, 'out')",
    )
    .bind(from_account.0)
    .bind(fields.occurred_at)
    .bind(&occurred_on)
    .bind(format!("Transfer to {}", to_account.1))
    .bind(from_account_id)
    .bind(signed_from)
    .bind(&note)
    .bind(transfer_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO fin_txn
            (currency_id, occurred_at, occurred_on, merchant, category_id, account_id,
             amount, tag, note, transfer_id, transfer_role)
         VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, 'tfr', NULL, ?7, 'in')",
    )
    .bind(to_account.0)
    .bind(fields.occurred_at)
    .bind(&occurred_on)
    .bind(format!("Transfer from {}", from_account.1))
    .bind(to_account_id)
    .bind(to_amount)
    .bind(transfer_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    adjust_account_balance(&mut tx, from_account_id, signed_from).await?;
    adjust_account_balance(&mut tx, to_account_id, to_amount).await?;
    tx.commit().await.map_err(server_err)?;
    fetch_transfer(pool, transfer_id).await
}

#[cfg(feature = "ssr")]
pub async fn delete_transfer_inner(pool: &SqlitePool, id: i64) -> Result<bool, ServerFnError> {
    let id = positive_id(id, "transfer_id")?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let deleted = delete_transfer_in_tx(&mut tx, id).await?;
    tx.commit().await.map_err(server_err)?;
    Ok(deleted)
}

#[cfg(feature = "ssr")]
async fn delete_transfer_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> Result<bool, ServerFnError> {
    let Some((from_account_id, to_account_id, from_amount, to_amount)) =
        sqlx::query_as::<_, (i64, i64, MinorAmount, MinorAmount)>(
            "SELECT from_account_id, to_account_id, from_amount, to_amount
               FROM fin_transfer WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(server_err)?
    else {
        return Ok(false);
    };
    adjust_account_balance(tx, from_account_id, from_amount).await?;
    adjust_account_balance(
        tx,
        to_account_id,
        to_amount
            .checked_neg()
            .ok_or_else(|| server_err("finance amount overflow"))?,
    )
    .await?;
    sqlx::query("DELETE FROM fin_transfer WHERE id = ?1")
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(server_err)?;
    Ok(true)
}

// -------------------------------------------------------------------------
// Budgets, reports, summary and CSV
// -------------------------------------------------------------------------

#[cfg(feature = "ssr")]
pub async fn set_budget_inner(
    pool: &SqlitePool,
    currency_id: i64,
    period: &str,
    category_id: i64,
    amount: MinorAmount,
) -> Result<Option<Budget>, ServerFnError> {
    let currency_id = positive_id(currency_id, "currency_id")?;
    let category_id = positive_id(category_id, "category_id")?;
    let period = normalize_period(period)?;
    let amount = validate_amount(amount)?;
    let category = fetch_category(pool, category_id).await?;
    if category.currency_id != currency_id {
        return Err(ep_i18n::err_with(
            "finance.err.category_not_found",
            category_id,
        ));
    }
    if !amount.is_positive() {
        sqlx::query(
            "DELETE FROM fin_budget WHERE currency_id = ?1 AND period = ?2 AND category_id = ?3",
        )
        .bind(currency_id)
        .bind(period)
        .bind(category_id)
        .execute(pool)
        .await
        .map_err(server_err)?;
        return Ok(None);
    }
    sqlx::query(
        "INSERT INTO fin_budget (currency_id, period, category_id, amount)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(currency_id, period, category_id)
         DO UPDATE SET amount = excluded.amount, updated_at = unixepoch()",
    )
    .bind(currency_id)
    .bind(&period)
    .bind(category_id)
    .bind(amount)
    .execute(pool)
    .await
    .map_err(server_err)?;
    let (_, start, end) = period_date_bounds(&period)?;
    let expense_rows: Vec<MinorAmount> = sqlx::query_scalar(
        "SELECT amount FROM fin_txn
          WHERE currency_id = ?1 AND category_id = ?2 AND tag = 'exp'
            AND occurred_on >= ?3 AND occurred_on < ?4",
    )
    .bind(currency_id)
    .bind(category_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let used = checked_sum(expense_rows.into_iter().map(|v| v.abs()))?;
    sqlx::query_as::<_, Budget>(
        "SELECT b.id, b.currency_id, c.code AS currency_code, b.period,
                b.category_id, x.name AS category_name, b.amount, ?4 AS used,
                b.created_at, b.updated_at
           FROM fin_budget b
           JOIN fin_currency c ON c.id = b.currency_id
           JOIN fin_category x ON x.id = b.category_id
          WHERE b.currency_id = ?1 AND b.period = ?2 AND b.category_id = ?3",
    )
    .bind(currency_id)
    .bind(period)
    .bind(category_id)
    .bind(used)
    .fetch_optional(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
async fn load_budgets(
    pool: &SqlitePool,
    currency_id: i64,
    period: &str,
    start: &str,
    end: &str,
) -> Result<Vec<Budget>, ServerFnError> {
    type BudgetRow = (i64, i64, String, String, i64, String, MinorAmount, i64, i64);
    let rows: Vec<BudgetRow> = sqlx::query_as(
        "SELECT b.id, b.currency_id, c.code, b.period, b.category_id, x.name,
                    b.amount, b.created_at, b.updated_at
               FROM fin_budget b
               JOIN fin_currency c ON c.id = b.currency_id
               JOIN fin_category x ON x.id = b.category_id
              WHERE b.currency_id = ?1 AND b.period = ?2
              ORDER BY x.sort_order, x.id",
    )
    .bind(currency_id)
    .bind(period)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let expenses: Vec<(i64, MinorAmount)> = sqlx::query_as(
        "SELECT category_id, amount FROM fin_txn
          WHERE currency_id = ?1 AND tag = 'exp'
            AND occurred_on >= ?2 AND occurred_on < ?3",
    )
    .bind(currency_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let mut used = std::collections::HashMap::<i64, Vec<MinorAmount>>::new();
    for (category_id, amount) in expenses {
        used.entry(category_id).or_default().push(amount.abs());
    }
    rows.into_iter()
        .map(|row| {
            let category_used = checked_sum(used.remove(&row.4).unwrap_or_default())?;
            Ok(Budget {
                id: row.0,
                currency_id: row.1,
                currency_code: row.2,
                period: row.3,
                category_id: row.4,
                category_name: row.5,
                amount: row.6,
                used: category_used,
                created_at: row.7,
                updated_at: row.8,
            })
        })
        .collect()
}

#[cfg(feature = "ssr")]
pub async fn list_budgets_inner(
    pool: &SqlitePool,
    currency_id: i64,
    period: &str,
) -> Result<Vec<Budget>, ServerFnError> {
    let currency_id = positive_id(currency_id, "currency_id")?;
    let (period, start, end) = period_date_bounds(period)?;
    load_budgets(pool, currency_id, &period, &start, &end).await
}

#[cfg(feature = "ssr")]
pub async fn load_month_buckets_12(
    pool: &SqlitePool,
    timezone: AppTimezone,
    now: i64,
    currency_id: i64,
) -> Result<Vec<MonthBucket>, ServerFnError> {
    let frame = timezone
        .recent_months(now, 12)
        .filter(|ranges| ranges.len() == 12)
        .ok_or_else(|| server_err("finance month chart is outside the supported date range"))?;
    let start = frame
        .first()
        .map(|range| format!("{}-01", range.label))
        .ok_or_else(|| server_err("finance month chart has no start"))?;
    let end = frame
        .last()
        .map(|range| period_date_bounds(&range.label).map(|(_, _, end)| end))
        .transpose()?
        .ok_or_else(|| server_err("finance month chart has no end"))?;
    let rows: Vec<(String, MinorAmount, String)> = sqlx::query_as(
        "SELECT occurred_on, amount, tag
           FROM fin_txn
          WHERE currency_id = ?1
            AND occurred_on >= ?2 AND occurred_on < ?3
            AND tag IN ('exp','inc')
          ORDER BY occurred_on, occurred_at, id",
    )
    .bind(positive_id(currency_id, "currency_id")?)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let mut grouped =
        std::collections::HashMap::<String, (Vec<MinorAmount>, Vec<MinorAmount>)>::new();
    for (occurred_on, amount, tag) in rows {
        let period = occurred_on
            .get(..7)
            .ok_or_else(|| server_err("finance transaction business date is invalid"))?
            .to_string();
        let entry = grouped.entry(period).or_default();
        if tag == "inc" && amount.is_positive() {
            entry.0.push(amount);
        } else if tag == "exp" && amount.is_negative() {
            entry.1.push(amount.abs());
        }
    }
    frame
        .into_iter()
        .map(|range| {
            let period = range.label;
            let (income_rows, expense_rows) = grouped.remove(&period).unwrap_or_default();
            let income = checked_sum(income_rows)?;
            let expense = checked_sum(expense_rows)?;
            let net = income
                .checked_sub(expense)
                .ok_or_else(|| server_err("finance amount overflow"))?;
            Ok(MonthBucket {
                period,
                income,
                expense,
                net,
            })
        })
        .collect()
}

#[cfg(feature = "ssr")]
pub async fn load_month_summary(
    pool: &SqlitePool,
    timezone: AppTimezone,
    now: i64,
    currency_id: i64,
) -> Result<MonthSummary, ServerFnError> {
    let currency = resolve_currency(pool, currency_id).await?;
    let period = current_period(timezone, now)?;
    let (_, start, end) = period_date_bounds(&period)?;
    let rows: Vec<(MinorAmount, String)> = sqlx::query_as(
        "SELECT amount, tag FROM fin_txn
          WHERE currency_id = ?1 AND occurred_on >= ?2 AND occurred_on < ?3
            AND tag IN ('exp','inc')",
    )
    .bind(currency.id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let count = i64::try_from(rows.len()).unwrap_or(i64::MAX);
    let income = checked_sum(
        rows.iter()
            .filter(|(_, tag)| tag == "inc")
            .map(|(amount, _)| *amount),
    )?;
    let expense = checked_sum(
        rows.iter()
            .filter(|(_, tag)| tag == "exp")
            .map(|(amount, _)| amount.abs()),
    )?;
    let balance = checked_sum(
        sqlx::query_scalar::<_, MinorAmount>(
            "SELECT balance FROM fin_account WHERE currency_id = ?1",
        )
        .bind(currency.id)
        .fetch_all(pool)
        .await
        .map_err(server_err)?,
    )?;
    let budget_total = checked_sum(
        sqlx::query_scalar::<_, MinorAmount>(
            "SELECT amount FROM fin_budget WHERE currency_id = ?1 AND period = ?2",
        )
        .bind(currency.id)
        .bind(&period)
        .fetch_all(pool)
        .await
        .map_err(server_err)?,
    )?;
    Ok(MonthSummary {
        currency_id: currency.id,
        currency_code: currency.code,
        period,
        income,
        expense,
        savings: income
            .checked_sub(expense)
            .ok_or_else(|| server_err("finance amount overflow"))?,
        balance,
        budget_total,
        transaction_count: count,
    })
}

#[cfg(feature = "ssr")]
fn csv_escape(value: &str) -> String {
    let value = if value
        .chars()
        .find(|ch| !matches!(ch, ' ' | '\t'))
        .is_some_and(|ch| matches!(ch, '=' | '+' | '-' | '@'))
    {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if !value
        .bytes()
        .any(|b| matches!(b, b',' | b'"' | b'\n' | b'\r'))
    {
        return value;
    }
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(feature = "ssr")]
fn make_csv(timezone: AppTimezone, txns: &[Txn], decimals: u8) -> String {
    use std::fmt::Write as _;
    let mut csv = String::from(
        "id,occurred_on,occurred_at,merchant,category,account,currency,amount,type,note,transfer_id\n",
    );
    for txn in txns {
        let _ = writeln!(
            csv,
            "{},{},{},{},{},{},{},{},{},{},{}",
            txn.id,
            txn.occurred_date,
            timezone.fmt_rfc3339(txn.occurred_at),
            csv_escape(&txn.merchant),
            csv_escape(txn.category_name.as_deref().unwrap_or("")),
            csv_escape(&txn.account_name),
            txn.currency_code,
            crate::amount::fmt_minor_raw(txn.amount, decimals),
            txn.tag,
            csv_escape(txn.note.as_deref().unwrap_or("")),
            txn.transfer_id
                .map_or_else(String::new, |id| id.to_string()),
        );
    }
    csv
}

#[cfg(feature = "ssr")]
pub async fn export_csv_inner(
    pool: &SqlitePool,
    timezone: AppTimezone,
    currency_id: i64,
) -> Result<String, ServerFnError> {
    let currency = resolve_currency(pool, currency_id).await?;
    let transactions = sqlx::query_as::<_, Txn>(&format!(
        "{TXN_SELECT} WHERE t.currency_id = ?1 ORDER BY t.occurred_at DESC, t.id DESC"
    ))
    .bind(currency.id)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    Ok(make_csv(timezone, &transactions, currency.decimals))
}

#[cfg(feature = "ssr")]
pub async fn load_finance_data_inner(
    pool: &SqlitePool,
    timezone: AppTimezone,
    now: i64,
    currency_id: i64,
) -> Result<FinanceData, ServerFnError> {
    let currency = resolve_currency(pool, currency_id).await?;
    let currencies = list_currencies_inner(pool).await?;
    let accounts = list_accounts_inner(pool, Some(currency.id), true).await?;
    let transfer_accounts = sqlx::query_as::<_, TransferAccountRef>(
        "SELECT a.id, a.currency_id, c.code AS currency_code, a.name, a.archived
           FROM fin_account a JOIN fin_currency c ON c.id = a.currency_id
          ORDER BY c.sort_order, a.archived, a.created_at, a.id",
    )
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let categories = list_categories_inner(pool, currency.id, true).await?;
    let transactions = sqlx::query_as::<_, Txn>(&format!(
        "{TXN_SELECT} WHERE t.currency_id = ?1 ORDER BY t.occurred_at DESC, t.id DESC LIMIT 100"
    ))
    .bind(currency.id)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let transfers = sqlx::query_as::<_, Transfer>(&format!(
        "{TRANSFER_SELECT}
          WHERE fa.currency_id = ?1 OR ta.currency_id = ?1
          ORDER BY f.occurred_at DESC, f.id DESC LIMIT 50"
    ))
    .bind(currency.id)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let period = current_period(timezone, now)?;
    let (_, start, end) = period_date_bounds(&period)?;
    let budgets = load_budgets(pool, currency.id, &period, &start, &end).await?;
    let month = load_month_summary(pool, timezone, now, currency.id).await?;
    let months_12 = load_month_buckets_12(pool, timezone, now, currency.id).await?;
    // The ledger table intentionally shows only the latest 100 rows, but
    // reports must cover the whole current month. Query report inputs
    // independently so summary cards never drift once the ledger is longer.
    let report_expenses: Vec<(i64, MinorAmount)> = sqlx::query_as(
        "SELECT category_id, amount FROM fin_txn
          WHERE currency_id = ?1 AND tag = 'exp'
            AND occurred_on >= ?2 AND occurred_on < ?3",
    )
    .bind(currency.id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let mut grouped = std::collections::HashMap::<i64, Vec<MinorAmount>>::new();
    for (category_id, amount) in report_expenses {
        grouped.entry(category_id).or_default().push(amount.abs());
    }
    let total = checked_sum(grouped.values().flatten().copied())?;
    let mut category_summary = Vec::new();
    for category in &categories {
        let value = checked_sum(grouped.remove(&category.id).unwrap_or_default())?;
        if value == MinorAmount::ZERO {
            continue;
        }
        category_summary.push(CategorySummary {
            category_id: category.id,
            name: category.name.clone(),
            icon: category.icon.clone(),
            tone: category.tone.clone(),
            value,
            pct: if total.is_positive() {
                value.to_f64() / total.to_f64()
            } else {
                0.0
            },
        });
    }
    category_summary.sort_by_key(|item| std::cmp::Reverse(item.value));
    Ok(FinanceData {
        currency,
        currencies,
        accounts,
        transfer_accounts,
        categories,
        transactions,
        transfers,
        budgets,
        month,
        months_12,
        category_summary,
    })
}

#[cfg(feature = "ssr")]
pub async fn dispatch_large_expense_notification(
    notify: &ep_core::NotifyBusHandle,
    currency: &Currency,
    txn: &Txn,
) {
    let Some(threshold) = crate::amount::major_to_minor(500, currency.decimals) else {
        tracing::error!(currency = %currency.code, "large expense threshold overflow");
        return;
    };
    if txn.tag != "exp" || txn.amount.abs() <= threshold {
        return;
    }
    let message = ep_core::NotifyMessage::warn(format!("Large expense · {}", txn.merchant))
        .source("finance")
        .body(format!(
            "{}{} · {}",
            currency.symbol,
            crate::amount::fmt_minor(txn.amount.abs(), currency.decimals),
            txn.category_name.as_deref().unwrap_or("Uncategorized")
        ))
        .link("/finance");
    if let Err(error) = notify.dispatch(message).await {
        tracing::warn!(error = %error, transaction_id = txn.id, "large expense notification failed");
    }
}

// -------------------------------------------------------------------------
// Hydration-safe server functions used by the module UI/home shell
// -------------------------------------------------------------------------

#[server(LoadFinanceData, "/api/_internal/finance", "Url", "load")]
pub async fn load_finance_data(currency_id: i64) -> Result<FinanceData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let timezone = state.timezone();
        let now = ep_core::unix_now();
        load_finance_data_inner(&state.db, timezone, now, currency_id).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = currency_id;
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(LoadHomeSummary, "/api/_internal/finance", "Url", "summary")]
pub async fn load_home_summary() -> Result<ep_core::ModuleSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let timezone = state.timezone();
        let now = ep_core::unix_now();
        let currency = resolve_currency(&state.db, 0).await?;
        let summary = load_month_summary(&state.db, timezone, now, currency.id).await?;
        let months = load_month_buckets_12(&state.db, timezone, now, currency.id).await?;
        let fmt = |amount: MinorAmount| {
            crate::charts::format_money(&currency.symbol, currency.decimals, amount)
        };
        Ok(ep_core::ModuleSummary {
            slug: "finance".into(),
            state: if summary.transaction_count == 0 {
                ep_core::ModuleSummaryState::Empty
            } else {
                ep_core::ModuleSummaryState::Ready
            },
            metrics: vec![
                ep_core::SummaryMetric {
                    label_key: "finance.summary.income".into(),
                    value: fmt(summary.income),
                    detail: Some(summary.period.clone()),
                },
                ep_core::SummaryMetric {
                    label_key: "finance.summary.expense".into(),
                    value: fmt(summary.expense),
                    detail: Some(summary.period.clone()),
                },
                ep_core::SummaryMetric {
                    label_key: "finance.summary.savings".into(),
                    value: fmt(summary.savings),
                    detail: Some(currency.code),
                },
            ],
            trend: crate::charts::summary_net_trend(&months, &currency.symbol, currency.decimals),
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    CreateFinanceAccount,
    "/api/_internal/finance",
    "Url",
    "accounts/create"
)]
pub async fn create_account(
    currency_id: i64,
    name: String,
    r#type: String,
    tone: String,
    opening_balance: String,
) -> Result<Account, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let currency = resolve_currency(&state.db, currency_id).await?;
        let balance = parse_signed_minor(&opening_balance, currency.decimals)?;
        create_account_inner(&state.db, currency.id, name, r#type, tone, balance).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (currency_id, name, r#type, tone, opening_balance);
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    CreateFinanceCategory,
    "/api/_internal/finance",
    "Url",
    "categories/create"
)]
pub async fn create_category(
    currency_id: i64,
    name: String,
    icon: String,
    tone: String,
) -> Result<Category, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        create_category_inner(&state.db, currency_id, name, icon, tone, 0).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (currency_id, name, icon, tone);
        Err(ep_core::server_err("ssr-only"))
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the fields map directly to one Leptos ActionForm"
)]
#[server(AddFinanceTxn, "/api/_internal/finance", "Url", "transactions/create")]
pub async fn add_txn(
    currency_id: i64,
    merchant: String,
    category_id: i64,
    account_id: i64,
    amount: String,
    tag: String,
    note: String,
    occurred_at: String,
) -> Result<Txn, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let timezone = state.timezone();
        let now = ep_core::unix_now();
        let currency = resolve_currency(&state.db, currency_id).await?;
        let magnitude = parse_positive_minor(&amount, currency.decimals)?;
        let tag_kind = Tag::parse(tag.trim())
            .filter(|kind| kind.is_single_entry())
            .ok_or_else(|| ep_i18n::err_with("finance.err.tag_invalid", tag.trim()))?;
        let signed = if tag_kind == Tag::Exp {
            magnitude
                .checked_neg()
                .ok_or_else(|| server_err("finance amount overflow"))?
        } else {
            magnitude
        };
        let txn = add_txn_inner(
            &state.db,
            timezone,
            AddTxnFields {
                currency_id: currency.id,
                merchant,
                category_id,
                account_id,
                amount: signed,
                tag: tag_kind.as_str().into(),
                note: Some(note),
                occurred_at: parse_occurred_at(timezone, &occurred_at)?.unwrap_or(now),
            },
        )
        .await?;
        dispatch_large_expense_notification(&state.notify, &currency, &txn).await;
        Ok(txn)
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (
            currency_id,
            merchant,
            category_id,
            account_id,
            amount,
            tag,
            note,
            occurred_at,
        );
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    DeleteFinanceTxn,
    "/api/_internal/finance",
    "Url",
    "transactions/delete"
)]
pub async fn delete_txn(id: i64) -> Result<ep_core::EntityId, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        if !delete_txn_inner(&state.db, id).await? {
            return Err(ep_i18n::err_with("finance.err.txn_not_found", id));
        }
        Ok(ep_core::EntityId::new(id))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = id;
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    AddFinanceTransfer,
    "/api/_internal/finance",
    "Url",
    "transfers/create"
)]
pub async fn add_transfer(
    from_account_id: i64,
    to_account_id: i64,
    from_amount: String,
    to_amount: String,
    note: String,
    occurred_at: String,
) -> Result<Transfer, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let timezone = state.timezone();
        let now = ep_core::unix_now();
        let (from_account, to_account) = tokio::try_join!(
            fetch_account(&state.db, from_account_id),
            fetch_account(&state.db, to_account_id),
        )?;
        let from_currency = resolve_currency(&state.db, from_account.currency_id).await?;
        let to_currency = resolve_currency(&state.db, to_account.currency_id).await?;
        add_transfer_inner(
            &state.db,
            timezone,
            AddTransferFields {
                from_account_id,
                to_account_id,
                from_amount: parse_positive_minor(&from_amount, from_currency.decimals)?,
                to_amount: parse_positive_minor(&to_amount, to_currency.decimals)?,
                note: Some(note),
                occurred_at: parse_occurred_at(timezone, &occurred_at)?.unwrap_or(now),
            },
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (
            from_account_id,
            to_account_id,
            from_amount,
            to_amount,
            note,
            occurred_at,
        );
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(SetFinanceBudget, "/api/_internal/finance", "Url", "budgets/set")]
pub async fn set_budget(
    currency_id: i64,
    period: String,
    category_id: i64,
    amount: String,
) -> Result<ep_core::EntityId, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let currency = resolve_currency(&state.db, currency_id).await?;
        let amount = if amount.trim().is_empty() {
            MinorAmount::ZERO
        } else {
            parse_signed_minor(&amount, currency.decimals)?
        };
        let budget = set_budget_inner(&state.db, currency.id, &period, category_id, amount).await?;
        Ok(ep_core::EntityId::new(budget.map_or(0, |budget| budget.id)))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = (currency_id, period, category_id, amount);
        Err(ep_core::server_err("ssr-only"))
    }
}
