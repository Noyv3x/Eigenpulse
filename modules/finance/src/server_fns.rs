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
#[allow(dead_code)]
pub(crate) const MIN_ACCOUNT_CODE_CHARS: usize = 2;
#[allow(dead_code)]
pub(crate) const MAX_ACCOUNT_CODE_CHARS: usize = 16;
pub(crate) const MAX_ACCOUNT_NAME_CHARS: usize = 64;
#[allow(dead_code)]
pub(crate) const MAX_CATEGORY_CODE_CHARS: usize = 8;
pub(crate) const MAX_CATEGORY_NAME_CHARS: usize = 32;

#[cfg(feature = "ssr")]
#[derive(Debug)]
struct NormalizedTxnFields {
    merchant: String,
    category_code: String,
    account_code: String,
    note: Option<String>,
}

#[cfg(feature = "ssr")]
pub struct AddTxnFields {
    pub merchant: String,
    pub category_code: String,
    pub account_code: String,
    pub amount: f64,
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
            validate_category_input("", "Food", "").unwrap(),
            ("".to_string(), "Food".to_string(), "".to_string())
        );
        assert!(
            validate_category_input(&"A".repeat(MAX_CATEGORY_CODE_CHARS + 1), "Food", "").is_err()
        );
        assert!(
            validate_category_input("FOOD", &"x".repeat(MAX_CATEGORY_NAME_CHARS + 1), "").is_err()
        );
        assert_eq!(
            validate_category_input(" F&B ", " Food ", "amber").unwrap(),
            ("F&B".to_string(), "Food".to_string(), "amber".to_string())
        );
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

/// One transactional payload for the entire `/finance` page. Bundles every
/// aggregate the view needs so the SSR pass fires a single `tokio::try_join!`
/// instead of N round-trips, and the hydrate side pays one network request
/// for the whole page rather than one per tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerData {
    pub accounts: Vec<Account>,
    /// Index-aligned with `accounts`: account_stats[i] describes accounts[i].
    pub account_stats: Vec<AccountStats>,
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
    /// the categories management table. `serde(default)` for backward-compat
    /// with any cached client/server versions that pre-date this field.
    #[serde(default)]
    pub category_usage: std::collections::HashMap<String, i64>,
}

/// Dense 12-month income/expense bucket, oldest -> newest.
///
/// Expense totals deliberately include only `tag = 'exp'`; transfer from-legs
/// are negative too, but they are internal money movement rather than spend.
#[cfg(feature = "ssr")]
pub async fn load_month_buckets_12(pool: &sqlx::SqlitePool) -> sqlx::Result<Vec<MonthBucket>> {
    type MonthRow = (String, f64, f64);
    let months_q = sqlx::query_as::<_, MonthRow>(
        "SELECT strftime('%Y-%m', occurred_at, 'unixepoch', 'localtime') AS period,
                COALESCE(SUM(CASE WHEN tag='inc' AND amount > 0 THEN amount ELSE 0.0 END), 0.0) AS income,
                COALESCE(SUM(CASE WHEN tag='exp' AND amount < 0 THEN -amount ELSE 0.0 END), 0.0) AS expense
           FROM fin_txn
          WHERE occurred_at >= unixepoch('now','localtime','start of month','-11 months','utc')
          GROUP BY period
          ORDER BY period ASC",
    )
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
fn month_buckets_from_rows(frame: Vec<String>, rows: Vec<(String, f64, f64)>) -> Vec<MonthBucket> {
    let mut by_period: std::collections::HashMap<String, (f64, f64)> =
        std::collections::HashMap::new();
    for (period, income, expense) in rows {
        by_period.insert(period, (income, expense));
    }
    frame
        .into_iter()
        .map(|period| {
            let (income, expense) = by_period.get(&period).copied().unwrap_or((0.0, 0.0));
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
pub async fn load_ledger() -> Result<LedgerData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let pool = &state.db;

        // Full-row SELECTs decode straight into the model structs via
        // `sqlx::FromRow`; only the derived/aggregate queries below still need
        // ad-hoc tuple rows.
        type SumRow = (String, f64);

        let accounts_q = sqlx::query_as::<_, Account>(
            "SELECT code, name, type, tone, balance, archived, created_at
               FROM fin_account ORDER BY code",
        )
        .fetch_all(pool);
        let categories_q = sqlx::query_as::<_, Category>(
            "SELECT code, name, tone, sort_order, archived, created_at
               FROM fin_category ORDER BY sort_order",
        )
        .fetch_all(pool);
        let txns_q = sqlx::query_as::<_, Txn>(
            "SELECT doc_id, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id
               FROM fin_txn ORDER BY occurred_at DESC LIMIT 50"
        ).fetch_all(pool);
        // Every "expense" aggregation filters `tag = 'exp'` (not just
        // `amount < 0`): transfer rows have `tag='tfr'` and the from-leg
        // is amount<0, which would otherwise pollute expense / category /
        // budget / 90d / 12-month / weekly net totals. tfr is internal
        // money movement, not spending.
        let cat_sum_q = sqlx::query_as::<_, SumRow>(
            "SELECT category_code, SUM(-amount)
               FROM fin_txn
              WHERE tag = 'exp'
                AND occurred_at >= unixepoch('now','localtime','start of month','utc')
              GROUP BY category_code",
        )
        .fetch_all(pool);
        // COALESCE / CASE-ELSE defaults are `0.0` (not `0`): sqlite types
        // the fallback by literal, and an INTEGER `0` would trip sqlx's
        // f64 decoder when SUM is NULL on an empty table.
        let income_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(amount), 0.0) FROM fin_txn
              WHERE amount > 0 AND tag = 'inc'
                AND occurred_at >= unixepoch('now','localtime','start of month','utc')",
        )
        .fetch_one(pool);
        let expense_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn
              WHERE tag = 'exp'
                AND occurred_at >= unixepoch('now','localtime','start of month','utc')",
        )
        .fetch_one(pool);
        let budget_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(amount), 0.0) FROM fin_budget
              WHERE period = strftime('%Y-%m','now','localtime')",
        )
        .fetch_one(pool);
        // Per-category budget vs MTD usage, used by the budget tab and
        // `suggestions::compute_suggestions`.
        type BudgetRow = (String, f64, f64);
        let budgets_q = sqlx::query_as::<_, BudgetRow>(
            "SELECT b.category_code, b.amount,
                    COALESCE((SELECT SUM(-t.amount) FROM fin_txn t
                               WHERE t.category_code = b.category_code
                                 AND t.tag = 'exp'
                                 AND t.occurred_at >= unixepoch('now','localtime','start of month','utc')), 0.0) AS used
               FROM fin_budget b
              WHERE b.period = strftime('%Y-%m','now','localtime')
              ORDER BY b.category_code"
        ).fetch_all(pool);

        // Last-7-day net (income - expense). Used by the banner's weekly badge
        // and the suggestions card. Single round-trip via `CASE` so we don't
        // need two scalar queries.
        let week_net_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(
                SUM(CASE WHEN tag = 'inc' AND amount > 0 THEN amount
                         WHEN tag = 'exp' AND amount < 0 THEN amount
                         ELSE 0.0 END), 0.0)
               FROM fin_txn
              WHERE occurred_at >= unixepoch('now','localtime','-7 days','utc')",
        )
        .fetch_one(pool);

        // 3-month rolling expense total, used for emergency-fund coverage and
        // the next-month-budget planner. 90-day window approximates 3 calendar
        // months without the awkward "what's my -3 month boundary" arithmetic.
        let expense_90d_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn
              WHERE tag = 'exp'
                AND occurred_at >= unixepoch('now','localtime','-90 days','utc')",
        )
        .fetch_one(pool);

        // Liquid balance — the denominator for `emergency_months`.
        let liquid_balance_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(CASE WHEN type IN ('Checking','Savings','Cash') THEN balance ELSE 0.0 END), 0.0)
               FROM fin_account"
        ).fetch_one(pool);

        // Total fin_txn count for the current month (independent of the
        // 50-row LIMIT on `txns_q`).
        let total_count_q = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM fin_txn
              WHERE occurred_at >= unixepoch('now','localtime','start of month','utc')",
        )
        .fetch_one(pool);

        // Per-account most-recent occurred_at, used by the last-activity line.
        type LastSeenRow = (String, i64);
        let last_seen_q = sqlx::query_as::<_, LastSeenRow>(
            "SELECT account_code, MAX(occurred_at) FROM fin_txn GROUP BY account_code",
        )
        .fetch_all(pool);

        // Per-account, per-day expense magnitude over the last 14 days.
        // `'start of day'` on both sides anchors the diff to whole-day-aligned
        // local calendar days (same load-bearing trick the lrn heatmap uses
        // — without it sub-day fractions push 02:00-vs-22:00 same-day rows
        // into different buckets).
        type DailyHistoryRow = (String, i64, f64);
        let history_14d_q = sqlx::query_as::<_, DailyHistoryRow>(
            "SELECT account_code,
                    CAST(julianday('now','localtime','start of day')
                         - julianday(occurred_at,'unixepoch','localtime','start of day') AS INTEGER) AS days_ago,
                    SUM(-amount)
               FROM fin_txn
              WHERE tag = 'exp'
                AND occurred_at >= unixepoch('now','localtime','-13 days','start of day','utc')
              GROUP BY account_code, days_ago"
        ).fetch_all(pool);

        let months_12_q = load_month_buckets_12(pool);

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
        // categories management table. Aggregating in SQL avoids shipping
        // every txn's category code over the wire just to count them.
        type CatUsageRow = (String, i64);
        let cat_usage_q = sqlx::query_as::<_, CatUsageRow>(
            "SELECT category_code, COUNT(*) FROM fin_txn GROUP BY category_code",
        )
        .fetch_all(pool);

        let (
            accounts,
            categories,
            txns,
            cat_rows,
            income,
            expense,
            budget_total,
            budget_rows,
            week_net,
            expense_90d,
            liquid_balance,
            total_count,
            last_seen_rows,
            history_14d_rows,
            months_12,
            ctx,
            cat_usage_rows,
        ) = tokio::try_join!(
            accounts_q,
            categories_q,
            txns_q,
            cat_sum_q,
            income_q,
            expense_q,
            budget_q,
            budgets_q,
            week_net_q,
            expense_90d_q,
            liquid_balance_q,
            total_count_q,
            last_seen_q,
            history_14d_q,
            months_12_q,
            context_q,
            cat_usage_q,
        )
        .map_err(server_err)?;

        let total_spent: f64 = cat_rows.iter().map(|(_, v)| *v).sum();
        let category_summary = cat_rows
            .into_iter()
            .map(|(code, value)| {
                let cat = categories.iter().find(|c| c.code == code);
                CategorySummary {
                    code: code.clone(),
                    name: cat.map(|c| c.name.clone()).unwrap_or_default(),
                    tone: cat.map(|c| c.tone.clone()).unwrap_or_default(),
                    value,
                    pct: if total_spent > 0.0 {
                        (value / total_spent * 1000.0).round() / 10.0
                    } else {
                        0.0
                    },
                }
            })
            .collect::<Vec<_>>();

        let balance: f64 = accounts.iter().map(|a| a.balance).sum();

        let budgets: Vec<BudgetEntry> = budget_rows
            .into_iter()
            .map(|(category_code, amount, used)| BudgetEntry {
                category_code,
                amount,
                used,
            })
            .collect();

        let mut last_seen_map: std::collections::HashMap<String, i64> =
            last_seen_rows.into_iter().collect();
        // `'start of day'` on both sides of the SQL diff anchors days_ago
        // to 0..=13; index 0 is the oldest day, 13 is today. The clamp
        // is paranoia against a future SQL edit.
        let mut history_map: std::collections::HashMap<String, Vec<f64>> =
            std::collections::HashMap::new();
        for (account_code, days_ago, magnitude) in history_14d_rows {
            let slot = history_map
                .entry(account_code)
                .or_insert_with(|| vec![0.0_f64; 14]);
            let idx = (13 - days_ago.clamp(0, 13)) as usize;
            if let Some(cell) = slot.get_mut(idx) {
                *cell += magnitude;
            }
        }
        let account_stats: Vec<AccountStats> = accounts
            .iter()
            .map(|a| AccountStats {
                last_seen_at: last_seen_map.remove(&a.code),
                history_14d: history_map
                    .remove(&a.code)
                    .unwrap_or_else(|| vec![0.0_f64; 14]),
            })
            .collect();

        let (period, day_of_month) = ctx;
        let days_elapsed = (day_of_month as u32).max(1);
        let avg_expense_3m = expense_90d / 3.0;
        let savings_rate = if income > 0.0 {
            (((income - expense) / income).clamp(0.0, 1.0)) as f32
        } else {
            0.0
        };
        // Floor avg at 1.0 so emergency_months doesn't explode on a fresh DB.
        let emergency_months = if avg_expense_3m > 1.0 {
            (liquid_balance / avg_expense_3m).clamp(0.0, 99.0) as f32
        } else {
            0.0
        };

        let category_usage: std::collections::HashMap<String, i64> =
            cat_usage_rows.into_iter().collect();

        Ok(LedgerData {
            accounts,
            account_stats,
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
        Err(ep_core::server_err("ssr-only"))
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Leptos ActionForm fields map to server-fn parameters"
)]
#[server(AddTxn, "/api/_internal/fin", "Url", "add_txn")]
pub async fn add_txn(
    merchant: String,
    category_code: String,
    account_code: String,
    amount: f64,
    tag: String,
    note: String,
    occurred_at: String,
) -> Result<Txn, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;

        let tag = tag.trim();
        let tag_kind = match crate::model::Tag::parse(tag) {
            Some(k) => k,
            None => return Err(ep_i18n::err_with("finance.err.tag_invalid", tag)),
        };
        if !tag_kind.is_single_entry() {
            return Err(ep_i18n::err("finance.err.tfr_requires_transfer"));
        }
        // Form contract: positive amount, `tag` carries the sign (matches the
        // UI convention). `/api/v1/fin/txn` is a separate code path that accepts
        // pre-signed exp/inc amounts; paired transfers go through add_transfer.
        if !amount.is_finite() || amount < 0.005 {
            return Err(ep_i18n::err("finance.err.amount_must_be_positive"));
        }
        let amount = if tag_kind == crate::model::Tag::Exp {
            -amount
        } else {
            amount
        };

        let state = ep_core::app_state_context()?;
        let pool = &state.db;

        let occurred = parse_occurred_at(pool, &occurred_at)
            .await?
            .unwrap_or_else(ep_core::unix_now);

        let txn = add_txn_inner(
            pool,
            AddTxnFields {
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
        dispatch_large_expense_notification(&state.notify, &txn).await;
        Ok(txn)
    }
    #[cfg(not(feature = "ssr"))]
    {
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
    let tag_raw = fields.tag.trim();
    let tag_kind = match crate::model::Tag::parse(tag_raw) {
        Some(k) => k,
        None => return Err(ep_i18n::err_with("finance.err.tag_invalid", tag_raw)),
    };
    if !tag_kind.is_single_entry() {
        return Err(ep_i18n::err("finance.err.tfr_requires_transfer"));
    }
    if !fields.amount.is_finite() {
        return Err(ep_i18n::err("finance.err.amount_must_be_finite"));
    }
    match tag_kind {
        crate::model::Tag::Exp if fields.amount < -0.005 => {}
        crate::model::Tag::Inc if fields.amount > 0.005 => {}
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
    // than opaque sqlite FK violations.
    let (cat_exists, acc_exists): (i64, i64) = tokio::try_join!(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = ?1)")
            .bind(&normalized.category_code)
            .fetch_one(pool),
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_account WHERE code = ?1)")
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
        "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )
    .bind(&doc_id)
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

    sqlx::query("UPDATE fin_account SET balance = balance + ?1 WHERE code = ?2")
        .bind(fields.amount)
        .bind(&normalized.account_code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;

    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc)
         VALUES (?1, 'FIN', ?2, ?3, ?4, ?5)",
    )
    .bind(fields.occurred_at)
    .bind(&doc_id)
    .bind(&normalized.merchant)
    .bind(fields.amount)
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

#[cfg(feature = "ssr")]
pub async fn dispatch_large_expense_notification(notify: &ep_core::NotifyBusHandle, txn: &Txn) {
    if txn.amount >= -500.0 {
        return;
    }
    let n = ep_core::NotifyMessage::warn(format!("Large expense · {}", txn.merchant))
        .module("FIN")
        .body(format!("¥{:.2} ({})", txn.amount.abs(), txn.category_code))
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
    let row: Option<(f64, String, String, Option<String>)> = sqlx::query_as(
        "SELECT amount, account_code, tag, linked_doc_id
           FROM fin_txn WHERE doc_id = ?1",
    )
    .bind(doc_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(server_err)?;
    let (amount, account_code, tag, linked_doc_id) = match row {
        Some(r) => r,
        None => return Ok(None),
    };
    sqlx::query("UPDATE fin_account SET balance = balance - ?1 WHERE code = ?2")
        .bind(amount)
        .bind(&account_code)
        .execute(&mut **tx)
        .await
        .map_err(server_err)?;
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
        Err(ep_core::server_err("ssr-only"))
    }
}

/// `amount <= 0` deletes the row (treats "set budget to zero" as remove).
#[cfg(feature = "ssr")]
pub async fn set_budget_inner(
    pool: &SqlitePool,
    period: &str,
    category_code: &str,
    amount: f64,
) -> Result<(), ServerFnError> {
    let period = normalize_budget_period(period)?;
    let category_code = category_code.trim();
    if category_code.is_empty() {
        return Err(ep_i18n::err("finance.err.category_code_required"));
    }
    if !amount.is_finite() {
        return Err(ep_i18n::err("finance.err.amount_must_be_finite"));
    }
    if amount > 0.0 {
        let exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = ?1)")
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
    if amount <= 0.0 {
        sqlx::query("DELETE FROM fin_budget WHERE period = ?1 AND category_code = ?2")
            .bind(&period)
            .bind(category_code)
            .execute(pool)
            .await
            .map_err(server_err)?;
    } else {
        // ON CONFLICT updates the amount in place. Composite PK is
        // (period, category_code) per migrations/001_finance.sql.
        sqlx::query(
            "INSERT INTO fin_budget (period, category_code, amount) VALUES (?1, ?2, ?3)
             ON CONFLICT(period, category_code) DO UPDATE SET amount = excluded.amount",
        )
        .bind(period)
        .bind(category_code)
        .bind(amount)
        .execute(pool)
        .await
        .map_err(server_err)?;
    }
    Ok(())
}

/// Upsert a per-period, per-category budget. `amount <= 0` deletes the row
/// (treats "set budget to zero" as "remove budget"). `period` must be a real
/// `YYYY-MM` month.
#[server(SetBudget, "/api/_internal/fin", "Url", "set_budget")]
pub async fn set_budget(
    period: String,
    category_code: String,
    amount: f64,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        set_budget_inner(&state.db, &period, &category_code, amount).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

/// Copy every row of `fin_budget` from `source_period` into `target_period`,
/// overwriting any existing target rows. Drives the prior-budget import affordance
/// shown when the current period has no budgets yet.
#[server(ImportBudgetsFrom, "/api/_internal/fin", "Url", "import_budgets_from")]
pub async fn import_budgets_from(
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
        let res = sqlx::query(
            "INSERT INTO fin_budget (period, category_code, amount)
             SELECT ?1, category_code, amount FROM fin_budget WHERE period = ?2
             ON CONFLICT(period, category_code) DO UPDATE SET amount = excluded.amount",
        )
        .bind(&target_period)
        .bind(&source_period)
        .execute(pool)
        .await
        .map_err(server_err)?;
        Ok(res.rows_affected() as i64)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

// ---------------------------------------------------------------------------
// Txn update
// ---------------------------------------------------------------------------

/// Mutable fields for `update_txn_inner`. Bundle so the function signature
/// stays sane and the PAT axum handler can build a single value from JSON.
#[cfg(feature = "ssr")]
pub struct UpdateTxnFields {
    pub merchant: String,
    pub category_code: String,
    pub account_code: String,
    pub amount: f64,
    pub note: Option<String>,
    /// Wire form: empty → "keep existing", `"YYYY-MM-DD"` → that day 12:00
    /// local. Bad format → Args error.
    pub occurred_at_input: String,
}

/// `tag` and `doc_id` are immutable. To change `tag`, delete and re-create
/// the txn. Transfer rows (`tag='tfr'`) reject any update — delete the
/// pair via `delete_txn` and re-create via `add_transfer`.
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
    if !fields.amount.is_finite() {
        return Err(ep_i18n::err("finance.err.amount_must_be_finite"));
    }

    let mut tx = pool.begin().await.map_err(server_err)?;

    // Read the existing row (and lock it implicitly under SQLite's deferred
    // tx). Capture old amount/account/tag/occurred so we can both refuse
    // tfr edits and compute balance deltas.
    type OldRow = (f64, String, String, i64);
    let old: Option<OldRow> = sqlx::query_as(
        "SELECT amount, account_code, tag, occurred_at
           FROM fin_txn WHERE doc_id = ?1",
    )
    .bind(doc_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(server_err)?;
    let (old_amount, old_account, old_tag, old_occurred) = match old {
        Some(r) => r,
        None => return Err(ep_i18n::err_with("finance.err.txn_not_found", doc_id)),
    };
    if old_tag == "tfr" {
        return Err(ep_i18n::err("finance.err.tfr_not_editable"));
    }

    // tag is immutable, so amount sign is fully determined by old_tag.
    // UI sends abs (input min=0.01); coercing here keeps the invariant.
    let signed_amount = if old_tag == "exp" {
        -fields.amount.abs()
    } else {
        fields.amount.abs()
    };

    // Validate FK constraints. Sequential because we share one tx — try_join
    // would alias-borrow the connection.
    let cat_exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = ?1)")
            .bind(&normalized.category_code)
            .fetch_one(&mut *tx)
            .await
            .map_err(server_err)?;
    let acc_ok: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_account WHERE code = ?1)")
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
        sqlx::query("UPDATE fin_account SET balance = balance + (?1 - ?2) WHERE code = ?3")
            .bind(signed_amount)
            .bind(old_amount)
            .bind(&normalized.account_code)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
    } else {
        sqlx::query("UPDATE fin_account SET balance = balance - ?1 WHERE code = ?2")
            .bind(old_amount)
            .bind(&old_account)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
        sqlx::query("UPDATE fin_account SET balance = balance + ?1 WHERE code = ?2")
            .bind(signed_amount)
            .bind(&normalized.account_code)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
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
    amount: f64,
    note: String,
    occurred_at: String,
) -> Result<Txn, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let state = ep_core::app_state_context()?;
        update_txn_inner(
            &state.db,
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
        Err(ep_core::server_err("ssr-only"))
    }
}

// ---------------------------------------------------------------------------
// Transfer (paired tfr txns)
// ---------------------------------------------------------------------------

/// Writes two paired `tag='tfr'` `fin_txn` rows + symmetric `module_link`
/// `kind='tfr-pair'` rows in one tx. Both legs share `occurred_at` and the
/// `'TFR'` category. `delete_txn_inner` cascades via the `tfr-pair` links.
///
/// Validates inputs (non-empty / distinct accounts / finite positive amount,
/// FK check on both accounts and TFR category). Wrappers don't need to
/// re-validate.
#[cfg(feature = "ssr")]
pub async fn add_transfer_inner(
    pool: &SqlitePool,
    from_account: &str,
    to_account: &str,
    amount: f64,
    note: Option<&str>,
    occurred_at: i64,
) -> Result<(Txn, Txn), ServerFnError> {
    let from_account = from_account.trim();
    let to_account = to_account.trim();
    if from_account.is_empty() || to_account.is_empty() {
        return Err(ep_i18n::err("finance.err.transfer_accounts_required"));
    }
    if from_account == to_account {
        return Err(ep_i18n::err("finance.err.transfer_accounts_same"));
    }
    if !amount.is_finite() || amount < 0.005 {
        return Err(ep_i18n::err("finance.err.amount_must_be_positive"));
    }
    let (from_ok, to_ok, tfr_ok): (i64, i64, i64) = tokio::try_join!(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_account WHERE code = ?1)")
            .bind(from_account)
            .fetch_one(pool),
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_account WHERE code = ?1)")
            .bind(to_account)
            .fetch_one(pool),
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = 'TFR')")
            .fetch_one(pool),
    )
    .map_err(server_err)?;
    if from_ok == 0 {
        return Err(ep_i18n::err_with(
            "finance.err.account_not_found",
            from_account,
        ));
    }
    if to_ok == 0 {
        return Err(ep_i18n::err_with(
            "finance.err.account_not_found",
            to_account,
        ));
    }
    if tfr_ok == 0 {
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
    let note_owned = normalize_txn_note(note)?;

    sqlx::query(
        "INSERT INTO fin_txn
            (doc_id, occurred_at, merchant, category_code, account_code,
             amount, tag, note, linked_doc_id)
         VALUES (?1, ?2, ?3, 'TFR', ?4, ?5, 'tfr', ?6, ?7)",
    )
    .bind(&from_doc)
    .bind(occurred_at)
    .bind(&from_merchant)
    .bind(from_account)
    .bind(-amount)
    .bind(&note_owned)
    .bind(&to_doc)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO fin_txn
            (doc_id, occurred_at, merchant, category_code, account_code,
             amount, tag, note, linked_doc_id)
         VALUES (?1, ?2, ?3, 'TFR', ?4, ?5, 'tfr', NULL, ?6)",
    )
    .bind(&to_doc)
    .bind(occurred_at)
    .bind(&to_merchant)
    .bind(to_account)
    .bind(amount)
    .bind(&from_doc)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;

    sqlx::query("UPDATE fin_account SET balance = balance - ?1 WHERE code = ?2")
        .bind(amount)
        .bind(from_account)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    sqlx::query("UPDATE fin_account SET balance = balance + ?1 WHERE code = ?2")
        .bind(amount)
        .bind(to_account)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;

    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc)
         VALUES (?1, 'FIN', ?2, ?3, ?4, ?5)",
    )
    .bind(occurred_at)
    .bind(&from_doc)
    .bind(&from_merchant)
    .bind(-amount)
    .bind(&to_doc)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc)
         VALUES (?1, 'FIN', ?2, ?3, ?4, ?5)",
    )
    .bind(occurred_at)
    .bind(&to_doc)
    .bind(&to_merchant)
    .bind(amount)
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
            occurred_at,
            merchant: from_merchant,
            category_code: "TFR".into(),
            account_code: from_account.into(),
            amount: -amount,
            tag: "tfr".into(),
            note: note_owned.clone(),
            linked_doc_id: Some(to_doc.clone()),
        },
        Txn {
            doc_id: to_doc,
            occurred_at,
            merchant: to_merchant,
            category_code: "TFR".into(),
            account_code: to_account.into(),
            amount,
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
    amount: f64,
    note: String,
    occurred_at: String,
) -> Result<(Txn, Txn), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let pool = &state.db;

        let occurred = parse_occurred_at(pool, &occurred_at)
            .await?
            .unwrap_or_else(ep_core::unix_now);

        add_transfer_inner(
            pool,
            &from_account,
            &to_account,
            amount,
            Some(&note),
            occurred,
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
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
/// (optional) `exclude` code. Used by [`unique_account_code`] so a
/// rename that lands on the same slug as the current row's own code is
/// treated as available.
#[cfg(feature = "ssr")]
async fn account_code_taken(
    conn: &mut sqlx::SqliteConnection,
    candidate: &str,
    exclude: Option<&str>,
) -> Result<bool, ServerFnError> {
    if exclude == Some(candidate) {
        return Ok(false);
    }
    let r: Option<i64> = sqlx::query_scalar("SELECT 1 FROM fin_account WHERE code = ?1 LIMIT 1")
        .bind(candidate)
        .fetch_optional(&mut *conn)
        .await
        .map_err(server_err)?;
    Ok(r.is_some())
}

/// Pick a unique account code that mirrors `name`. Tries the ASCII slug
/// first (e.g. "Cash Wallet" → "CASH-WALLET"). When the slug is empty
/// (non-ASCII names like "招行储蓄"), seeds the search from a stable
/// fingerprint hashed off `name` rather than always starting at `ACC-1`
/// — that way renaming "招行储蓄" → "工行储蓄" actually rotates the
/// generated code (`ACC-3742` → `ACC-5891`) instead of leaving the row
/// stuck on its original fallback slot regardless of how many times the
/// user edits the name. The `exclude` argument lets updates keep their
/// current row's code "available" against themselves.
#[cfg(feature = "ssr")]
async fn unique_account_code(
    conn: &mut sqlx::SqliteConnection,
    name: &str,
    exclude: Option<&str>,
) -> Result<String, ServerFnError> {
    let slug = slugify_to_code(name, MAX_ACCOUNT_CODE_CHARS);
    if !slug.is_empty() && !account_code_taken(conn, &slug, exclude).await? {
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
        if !account_code_taken(conn, &candidate, exclude).await? {
            return Ok(candidate);
        }
    }
    Err(ep_i18n::err("finance.err.account_code_format"))
}

/// `true` when a category row with `candidate` exists *other* than the
/// (optional) `exclude` code.
#[cfg(feature = "ssr")]
async fn category_code_taken(
    conn: &mut sqlx::SqliteConnection,
    candidate: &str,
    exclude: Option<&str>,
) -> Result<bool, ServerFnError> {
    if exclude == Some(candidate) {
        return Ok(false);
    }
    let r: Option<i64> = sqlx::query_scalar("SELECT 1 FROM fin_category WHERE code = ?1 LIMIT 1")
        .bind(candidate)
        .fetch_optional(&mut *conn)
        .await
        .map_err(server_err)?;
    Ok(r.is_some())
}

/// Pick a unique category code that mirrors `name`. Slug character class
/// is `[A-Z&]` (matches `validate_category_input`); fallback (when the
/// name has no ASCII letters to slugify) is `CATN`, seeded from a stable
/// fingerprint of `name` so renaming "餐饮" → "饮食" rotates `CAT37` to
/// some other slot instead of leaving the row stuck on the same code.
#[cfg(feature = "ssr")]
async fn unique_category_code(
    conn: &mut sqlx::SqliteConnection,
    name: &str,
    exclude: Option<&str>,
) -> Result<String, ServerFnError> {
    let slug: String = slugify_to_code(name, MAX_CATEGORY_CODE_CHARS)
        .chars()
        .filter(|c| c.is_ascii_uppercase() || *c == '&')
        .collect();
    if !slug.is_empty() && !category_code_taken(conn, &slug, exclude).await? {
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
        if !category_code_taken(conn, &candidate, exclude).await? {
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
/// The intentional simplicity (FNV-1a style mix) is fine here: we only need
/// a deterministic shuffle, not a cryptographic hash.
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
    code: String,
    name: String,
    r#type: String,
    tone: String,
    opening_balance: f64,
) -> Result<Account, ServerFnError> {
    if !opening_balance.is_finite() {
        return Err(ep_i18n::err("finance.err.opening_balance_invalid"));
    }
    let (code, name, r#type, tone) = validate_account_input(&code, &name, &r#type, &tone)?;
    let code = if code.is_empty() {
        let mut conn = pool.acquire().await.map_err(server_err)?;
        unique_account_code(&mut conn, &name, None).await?
    } else {
        code
    };
    let res = sqlx::query(
        "INSERT INTO fin_account (code, name, type, tone, balance, archived, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, unixepoch())",
    )
    .bind(&code)
    .bind(&name)
    .bind(&r#type)
    .bind(&tone)
    .bind(opening_balance)
    .execute(pool)
    .await;
    if let Err(e) = res {
        if is_unique_violation(&e) {
            return Err(ep_i18n::err_with("finance.err.account_code_taken", &code));
        }
        return Err(server_err(e));
    }
    sqlx::query_as::<_, Account>(
        "SELECT code, name, type, tone, balance, archived, created_at
           FROM fin_account WHERE code = ?1",
    )
    .bind(&code)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn update_account_inner(
    pool: &SqlitePool,
    code: String,
    name: String,
    r#type: String,
    tone: String,
) -> Result<Account, ServerFnError> {
    update_account_inner_with(pool, code, name, r#type, tone, /* rename_code */ true).await
}

/// Internal counterpart of [`update_account_inner`] that lets the caller
/// opt out of the name-driven code rename. The OpenAPI PATCH handler
/// passes `rename_code = false` so external API consumers (PATs / Shortcuts)
/// keep a stable key for the resource they just touched. The UI passes
/// `true` so renaming "Cash" → "Wallet" keeps the internal `code` aligned
/// with the human-visible name.
#[cfg(feature = "ssr")]
pub async fn update_account_inner_with(
    pool: &SqlitePool,
    code: String,
    name: String,
    r#type: String,
    tone: String,
    rename_code: bool,
) -> Result<Account, ServerFnError> {
    let (code, name, r#type, tone) = validate_account_input(&code, &name, &r#type, &tone)?;
    if code.is_empty() {
        return Err(ep_i18n::err("finance.err.account_code_format"));
    }
    let mut tx = pool.begin().await.map_err(server_err)?;
    // Re-derive the canonical code from the new name when the caller asks
    // for it. The UI never shows the code, so leaving a stale slug ("CASH"
    // for an account renamed to "Wallet") would only surface in CSV
    // exports / PAT API responses. The SQLite schema lacks
    // ON UPDATE CASCADE, so we walk every referencing table
    // (`fin_txn.account_code`) inside the same transaction.
    //
    // `defer_foreign_keys = ON` lets the FK check run at COMMIT time rather
    // than after every statement, so updating `fin_account.code` before
    // patching the `fin_txn.account_code` rows that still reference the
    // old key doesn't trip a constraint failure mid-transaction.
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    // Read the current name so we can tell whether this update actually
    // changes it; renaming the internal code on a tone-only edit (or any
    // other field-only edit) would gratuitously invalidate any manually
    // assigned code (PATCH /api/v1/fin/account/{code} can install one).
    let cur_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM fin_account WHERE code = ?1")
            .bind(&code)
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let name_changed = cur_name.as_deref() != Some(name.as_str());
    let new_code = if rename_code && name_changed {
        unique_account_code(&mut tx, &name, Some(&code)).await?
    } else {
        code.clone()
    };
    let res = if new_code != code {
        let res = sqlx::query(
            "UPDATE fin_account SET code = ?1, name = ?2, type = ?3, tone = ?4 WHERE code = ?5",
        )
        .bind(&new_code)
        .bind(&name)
        .bind(&r#type)
        .bind(&tone)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
        if res.rows_affected() == 0 {
            return Err(ep_i18n::err_with("finance.err.account_not_found", &code));
        }
        sqlx::query("UPDATE fin_txn SET account_code = ?1 WHERE account_code = ?2")
            .bind(&new_code)
            .bind(&code)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
        res
    } else {
        sqlx::query("UPDATE fin_account SET name = ?1, type = ?2, tone = ?3 WHERE code = ?4")
            .bind(&name)
            .bind(&r#type)
            .bind(&tone)
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
        "SELECT code, name, type, tone, balance, archived, created_at
           FROM fin_account WHERE code = ?1",
    )
    .bind(&new_code)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn delete_account_inner(pool: &SqlitePool, code: String) -> Result<(), ServerFnError> {
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(ep_i18n::err("finance.err.account_code_format"));
    }
    let txn_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_txn WHERE account_code = ?1")
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
    let res = sqlx::query("DELETE FROM fin_account WHERE code = ?1")
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
    code: String,
    name: String,
    r#type: String,
    tone: String,
    opening_balance: f64,
) -> Result<Account, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        create_account_inner(&state.db, code, name, r#type, tone, opening_balance).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(UpdateAccount, "/api/_internal/fin", "Url", "update_account")]
pub async fn update_account(
    code: String,
    name: String,
    r#type: String,
    tone: String,
) -> Result<Account, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        update_account_inner(&state.db, code, name, r#type, tone).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
pub async fn list_accounts_inner(pool: &SqlitePool) -> sqlx::Result<Vec<Account>> {
    sqlx::query_as::<_, Account>(
        "SELECT code, name, type, tone, balance, archived, created_at
           FROM fin_account
          ORDER BY code ASC",
    )
    .fetch_all(pool)
    .await
}

#[server(ListAccounts, "/api/_internal/fin", "Url", "list_accounts")]
pub async fn list_accounts() -> Result<Vec<Account>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        list_accounts_inner(&state.db).await.map_err(server_err)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(DeleteAccount, "/api/_internal/fin", "Url", "delete_account")]
pub async fn delete_account(code: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        delete_account_inner(&state.db, code).await
    }
    #[cfg(not(feature = "ssr"))]
    {
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
    tone: &str,
) -> Result<(String, String, String), ServerFnError> {
    let code = code.trim().to_string();
    let name = name.trim().to_string();
    let tone = tone.trim().to_string();
    // Inline char-class check (no regex dep). Accepts `&` for seed code
    // F&B and ASCII digits so the `CATN` fallback codes generated for
    // non-ASCII names (e.g. "餐饮") survive a round-trip through this
    // validator on update. An empty `code` is allowed at the input
    // boundary and gets a generated value before insert (see
    // `unique_category_code`).
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
    if !tone.is_empty() && !TONES.contains(&tone.as_str()) {
        return Err(ep_i18n::err_with(
            "finance.err.tone_invalid",
            format!("{TONES:?}"),
        ));
    }
    Ok((code, name, tone))
}

#[cfg(feature = "ssr")]
fn validate_category_sort_order(sort_order: i64) -> Result<i64, ServerFnError> {
    if sort_order < 0 {
        return Err(ep_i18n::err("finance.err.category_sort_order_invalid"));
    }
    Ok(sort_order)
}

#[cfg(feature = "ssr")]
pub async fn create_category_inner(
    pool: &SqlitePool,
    code: String,
    name: String,
    tone: String,
    sort_order: i64,
) -> Result<Category, ServerFnError> {
    let (code, name, tone) = validate_category_input(&code, &name, &tone)?;
    let code = if code.is_empty() {
        let mut conn = pool.acquire().await.map_err(server_err)?;
        unique_category_code(&mut conn, &name, None).await?
    } else {
        code
    };
    let sort_order = validate_category_sort_order(sort_order)?;
    let res = sqlx::query(
        "INSERT INTO fin_category (code, name, tone, sort_order, archived, created_at)
         VALUES (?1, ?2, ?3, ?4, 0, unixepoch())",
    )
    .bind(&code)
    .bind(&name)
    .bind(&tone)
    .bind(sort_order)
    .execute(pool)
    .await;
    if let Err(e) = res {
        if is_unique_violation(&e) {
            return Err(ep_i18n::err_with("finance.err.category_code_taken", &code));
        }
        return Err(server_err(e));
    }
    sqlx::query_as::<_, Category>(
        "SELECT code, name, tone, sort_order, archived, created_at
           FROM fin_category WHERE code = ?1",
    )
    .bind(&code)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn update_category_inner(
    pool: &SqlitePool,
    code: String,
    name: String,
    tone: String,
    sort_order: i64,
) -> Result<Category, ServerFnError> {
    update_category_inner_with(
        pool, code, name, tone, sort_order, /* rename_code */ true,
    )
    .await
}

/// Internal counterpart of [`update_category_inner`]; mirrors
/// [`update_account_inner_with`]. UI callers (`#[server] update_category`)
/// opt into the rename; OpenAPI PATCH consumers opt out so external clients
/// continue to address the resource by its original code.
#[cfg(feature = "ssr")]
pub async fn update_category_inner_with(
    pool: &SqlitePool,
    code: String,
    name: String,
    tone: String,
    sort_order: i64,
    rename_code: bool,
) -> Result<Category, ServerFnError> {
    let (code, name, tone) = validate_category_input(&code, &name, &tone)?;
    if code.is_empty() {
        return Err(ep_i18n::err("finance.err.category_code_format"));
    }
    let sort_order = validate_category_sort_order(sort_order)?;
    let mut tx = pool.begin().await.map_err(server_err)?;
    // Same cascade logic as `update_account_inner_with`: the UI never shows
    // the category code, so renaming "Food" → "Dining" needs to bring the
    // internal `code` along too. Walk `fin_txn` and `fin_budget` inside
    // this transaction (SQLite schema has no ON UPDATE CASCADE), and defer
    // FK checks to COMMIT so the parent-then-children order is allowed.
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    // Same guard as `update_account_inner_with`: only re-derive a code
    // when the human-readable name actually changed. A tone-only or
    // sort-order-only edit must leave a manually assigned code (set via
    // the OpenAPI PATCH path) alone.
    let cur_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM fin_category WHERE code = ?1")
            .bind(&code)
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let name_changed = cur_name.as_deref() != Some(name.as_str());
    let new_code = if rename_code && name_changed {
        unique_category_code(&mut tx, &name, Some(&code)).await?
    } else {
        code.clone()
    };
    let res = if new_code != code {
        let res = sqlx::query(
            "UPDATE fin_category SET code = ?1, name = ?2, tone = ?3, sort_order = ?4 WHERE code = ?5",
        )
        .bind(&new_code)
        .bind(&name)
        .bind(&tone)
        .bind(sort_order)
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
        if res.rows_affected() == 0 {
            return Err(ep_i18n::err_with("finance.err.category_not_found", &code));
        }
        sqlx::query("UPDATE fin_txn SET category_code = ?1 WHERE category_code = ?2")
            .bind(&new_code)
            .bind(&code)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
        sqlx::query("UPDATE fin_budget SET category_code = ?1 WHERE category_code = ?2")
            .bind(&new_code)
            .bind(&code)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
        res
    } else {
        sqlx::query("UPDATE fin_category SET name = ?1, tone = ?2, sort_order = ?3 WHERE code = ?4")
            .bind(&name)
            .bind(&tone)
            .bind(sort_order)
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
        "SELECT code, name, tone, sort_order, archived, created_at
           FROM fin_category WHERE code = ?1",
    )
    .bind(&new_code)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub async fn delete_category_inner(pool: &SqlitePool, code: String) -> Result<(), ServerFnError> {
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(ep_i18n::err("finance.err.category_code_format"));
    }
    let txn_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fin_txn WHERE category_code = ?1")
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
    sqlx::query("DELETE FROM fin_budget WHERE category_code = ?1")
        .bind(&code)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    let res = sqlx::query("DELETE FROM fin_category WHERE code = ?1")
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

#[server(CreateCategory, "/api/_internal/fin", "Url", "create_category")]
pub async fn create_category(
    code: String,
    name: String,
    tone: String,
    sort_order: i64,
) -> Result<Category, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        create_category_inner(&state.db, code, name, tone, sort_order).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(UpdateCategory, "/api/_internal/fin", "Url", "update_category")]
pub async fn update_category(
    code: String,
    name: String,
    tone: String,
    sort_order: i64,
) -> Result<Category, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        update_category_inner(&state.db, code, name, tone, sort_order).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(DeleteCategory, "/api/_internal/fin", "Url", "delete_category")]
pub async fn delete_category(code: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        delete_category_inner(&state.db, code).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}
