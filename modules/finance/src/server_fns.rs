use crate::model::*;
#[cfg(feature = "ssr")]
use ep_core::server_err;
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

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
}

#[server(LoadLedger, "/api/_internal/fin", "Url", "load_ledger")]
pub async fn load_ledger() -> Result<LedgerData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        let pool = &state.db;

        type AccRow  = (String, String, String, String, f64);
        type CatRow  = (String, String, String, i64);
        type TxnRow  = (String, i64, String, String, String, f64, String, Option<String>, Option<String>);
        type SumRow  = (String, f64);

        let accounts_q = sqlx::query_as::<_, AccRow>(
            "SELECT code, name, type, tone, balance FROM fin_account WHERE archived = 0 ORDER BY code"
        ).fetch_all(pool);
        let categories_q = sqlx::query_as::<_, CatRow>(
            "SELECT code, name, tone, sort_order FROM fin_category ORDER BY sort_order"
        ).fetch_all(pool);
        let txns_q = sqlx::query_as::<_, TxnRow>(
            "SELECT doc_id, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id
               FROM fin_txn ORDER BY occurred_at DESC LIMIT 50"
        ).fetch_all(pool);
        let cat_sum_q = sqlx::query_as::<_, SumRow>(
            "SELECT category_code, SUM(-amount)
               FROM fin_txn
              WHERE amount < 0 AND occurred_at >= unixepoch('now','localtime','start of month','utc')
              GROUP BY category_code"
        ).fetch_all(pool);
        // COALESCE / CASE-ELSE defaults are `0.0` (not `0`): sqlite types
        // the fallback by literal, and an INTEGER `0` would trip sqlx's
        // f64 decoder when SUM is NULL on an empty table.
        let income_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(amount), 0.0) FROM fin_txn
              WHERE amount > 0 AND tag = 'inc'
                AND occurred_at >= unixepoch('now','localtime','start of month','utc')"
        ).fetch_one(pool);
        let expense_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn
              WHERE amount < 0
                AND occurred_at >= unixepoch('now','localtime','start of month','utc')"
        ).fetch_one(pool);
        let budget_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(amount), 0.0) FROM fin_budget
              WHERE period = strftime('%Y-%m','now','localtime')"
        ).fetch_one(pool);
        // Per-category budget vs MTD usage, used by the budget tab and
        // `suggestions::compute_suggestions`.
        type BudgetRow = (String, f64, f64);
        let budgets_q = sqlx::query_as::<_, BudgetRow>(
            "SELECT b.category_code, b.amount,
                    COALESCE((SELECT SUM(-t.amount) FROM fin_txn t
                               WHERE t.category_code = b.category_code
                                 AND t.amount < 0
                                 AND t.occurred_at >= unixepoch('now','localtime','start of month','utc')), 0.0) AS used
               FROM fin_budget b
              WHERE b.period = strftime('%Y-%m','now','localtime')
              ORDER BY b.category_code"
        ).fetch_all(pool);

        // Last-7-day net (income - expense). Used by the banner's "本周" badge
        // and the suggestions card. Single round-trip via `CASE` so we don't
        // need two scalar queries.
        let week_net_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(
                SUM(CASE WHEN amount > 0 AND tag = 'inc' THEN amount
                         WHEN amount < 0 THEN amount
                         ELSE 0.0 END), 0.0)
               FROM fin_txn
              WHERE occurred_at >= unixepoch('now','localtime','-7 days','utc')"
        ).fetch_one(pool);

        // 3-month rolling expense total, used for emergency-fund coverage and
        // the next-month-budget planner. 90-day window approximates 3 calendar
        // months without the awkward "what's my -3 month boundary" arithmetic.
        let expense_90d_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn
              WHERE amount < 0
                AND occurred_at >= unixepoch('now','localtime','-90 days','utc')"
        ).fetch_one(pool);

        // Liquid balance — the denominator for `emergency_months`.
        let liquid_balance_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(CASE WHEN type IN ('Checking','Savings','Cash') THEN balance ELSE 0.0 END), 0.0)
               FROM fin_account WHERE archived = 0"
        ).fetch_one(pool);

        // Total fin_txn count for the current month (independent of the
        // 50-row LIMIT on `txns_q`).
        let total_count_q = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM fin_txn
              WHERE occurred_at >= unixepoch('now','localtime','start of month','utc')"
        ).fetch_one(pool);

        // Per-account most-recent occurred_at — drives the "最近活动" line.
        type LastSeenRow = (String, i64);
        let last_seen_q = sqlx::query_as::<_, LastSeenRow>(
            "SELECT account_code, MAX(occurred_at) FROM fin_txn GROUP BY account_code"
        ).fetch_all(pool);

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
              WHERE amount < 0
                AND occurred_at >= unixepoch('now','localtime','-13 days','start of day','utc')
              GROUP BY account_code, days_ago"
        ).fetch_all(pool);

        // 12-month income/expense bucket. Mirror reports.rs but scoped to a
        // single try_join so the finance page is one round-trip.
        type MonthRow = (String, f64, f64);
        let months_12_q = sqlx::query_as::<_, MonthRow>(
            "SELECT strftime('%Y-%m', occurred_at, 'unixepoch', 'localtime') AS period,
                    COALESCE(SUM(CASE WHEN tag='inc' AND amount > 0 THEN amount ELSE 0.0 END), 0.0) AS income,
                    COALESCE(SUM(CASE WHEN amount < 0 THEN -amount ELSE 0.0 END), 0.0) AS expense
               FROM fin_txn
              WHERE occurred_at >= unixepoch('now','localtime','start of month','-11 months','utc')
              GROUP BY period
              ORDER BY period ASC"
        ).fetch_all(pool);
        // Dense 12-month frame so months with no activity show as zero bars.
        let months_frame_q = sqlx::query_scalar::<_, String>(
            "WITH RECURSIVE months(p, n) AS (
                SELECT strftime('%Y-%m','now','localtime','start of month','-11 months'), 0
                UNION ALL
                SELECT strftime('%Y-%m','now','localtime','start of month',
                                printf('-%d months', 11 - n - 1)), n + 1
                  FROM months
                 WHERE n + 1 < 12
             )
             SELECT p FROM months ORDER BY p ASC"
        ).fetch_all(pool);

        // The page's wall-clock context: current period label and elapsed
        // days. Sent in the same join so rendering uses a single self-
        // consistent snapshot (period = "2026-05" pairs with days_elapsed
        // computed against the same `now`).
        type ContextRow = (String, i64);
        let context_q = sqlx::query_as::<_, ContextRow>(
            "SELECT strftime('%Y-%m','now','localtime') AS period,
                    CAST(strftime('%d','now','localtime') AS INTEGER) AS day_of_month"
        ).fetch_one(pool);

        let (
            accounts, categories, txns_rows, cat_rows,
            income, expense, budget_total, budget_rows,
            week_net, expense_90d, liquid_balance, total_count,
            last_seen_rows, history_14d_rows,
            months_rows, months_frame, ctx,
        ) = tokio::try_join!(
            accounts_q, categories_q, txns_q, cat_sum_q,
            income_q, expense_q, budget_q, budgets_q,
            week_net_q, expense_90d_q, liquid_balance_q, total_count_q,
            last_seen_q, history_14d_q,
            months_12_q, months_frame_q, context_q,
        ).map_err(server_err)?;

        let accounts = accounts.into_iter()
            .map(|r| Account { code: r.0, name: r.1, r#type: r.2, tone: r.3, balance: r.4 })
            .collect::<Vec<_>>();
        let categories = categories.into_iter()
            .map(|r| Category { code: r.0, name: r.1, tone: r.2, sort_order: r.3 })
            .collect::<Vec<_>>();
        let txns = txns_rows.into_iter().map(|r| Txn {
            doc_id: r.0, occurred_at: r.1, merchant: r.2, category_code: r.3, account_code: r.4,
            amount: r.5, tag: r.6, note: r.7, linked_doc_id: r.8,
        }).collect::<Vec<_>>();

        let total_spent: f64 = cat_rows.iter().map(|(_, v)| *v).sum();
        let category_summary = cat_rows.into_iter().map(|(code, value)| {
            let cat = categories.iter().find(|c| c.code == code);
            CategorySummary {
                code: code.clone(),
                name: cat.map(|c| c.name.clone()).unwrap_or_default(),
                tone: cat.map(|c| c.tone.clone()).unwrap_or_default(),
                value,
                pct: if total_spent > 0.0 { (value / total_spent * 1000.0).round() / 10.0 } else { 0.0 },
            }
        }).collect::<Vec<_>>();

        let balance: f64 = accounts.iter().map(|a| a.balance).sum();

        let budgets: Vec<BudgetEntry> = budget_rows.into_iter()
            .map(|(category_code, amount, used)| BudgetEntry { category_code, amount, used })
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
            if let Some(cell) = slot.get_mut(idx) { *cell += magnitude; }
        }
        let account_stats: Vec<AccountStats> = accounts.iter().map(|a| AccountStats {
            last_seen_at: last_seen_map.remove(&a.code),
            history_14d: history_map.remove(&a.code).unwrap_or_else(|| vec![0.0_f64; 14]),
        }).collect();

        let mut by_period: std::collections::HashMap<String, (f64, f64)> =
            std::collections::HashMap::new();
        for (p, income_m, expense_m) in months_rows {
            by_period.insert(p, (income_m, expense_m));
        }
        let months_12: Vec<MonthBucket> = months_frame.into_iter().map(|period| {
            let (income_m, expense_m) = by_period.get(&period).copied().unwrap_or((0.0, 0.0));
            MonthBucket { period, income: income_m, expense: expense_m, net: income_m - expense_m }
        }).collect();

        let (period, day_of_month) = ctx;
        let days_elapsed = (day_of_month as u32).max(1);
        let avg_expense_3m = expense_90d / 3.0;
        let savings_rate = if income > 0.0 {
            (((income - expense) / income).clamp(0.0, 1.0)) as f32
        } else { 0.0 };
        // Floor avg at 1.0 so emergency_months doesn't explode on a fresh DB.
        let emergency_months = if avg_expense_3m > 1.0 {
            (liquid_balance / avg_expense_3m).clamp(0.0, 99.0) as f32
        } else { 0.0 };

        Ok(LedgerData {
            accounts, account_stats, categories, txns, category_summary, budgets, months_12,
            month: MonthSummary {
                income, expense,
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
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

#[server(AddTxn, "/api/_internal/fin", "Url", "add_txn")]
pub async fn add_txn(
    merchant: String,
    category_code: String,
    account_code: String,
    amount: f64,
    tag: String,
    note: String,
    linked_doc_id: String,
) -> Result<Txn, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;

        let merchant = merchant.trim().to_string();
        if merchant.is_empty() {
            return Err(ServerFnError::Args("merchant is required".into()));
        }
        let tag_kind = match crate::model::Tag::parse(&tag) {
            Some(k) => k,
            None => return Err(ServerFnError::Args(format!("tag must be exp/inc/tfr, got '{tag}'"))),
        };
        if category_code.trim().is_empty() {
            return Err(ServerFnError::Args("category_code is required".into()));
        }
        if account_code.trim().is_empty() {
            return Err(ServerFnError::Args("account_code is required".into()));
        }
        // Form contract: positive amount, `tag` carries the sign (matches the
        // seed convention). `/api/v1/fin/txn` is a *separate* code path that
        // accepts pre-signed amounts — don't conflate the two.
        if !amount.is_finite() || amount < 0.005 {
            return Err(ServerFnError::Args("amount must be a positive number".into()));
        }
        let amount = if tag_kind == crate::model::Tag::Exp { -amount } else { amount };
        let note_opt = if note.trim().is_empty() { None } else { Some(note.clone()) };
        let linked_opt = if linked_doc_id.trim().is_empty() { None } else { Some(linked_doc_id.clone()) };

        let state: ep_core::AppState = expect_context();
        let pool = &state.db;

        // Pre-validate FKs concurrently so the user gets a clear "category
        // not found" rather than the opaque sqlite FK violation message.
        let (cat_exists, acc_exists): (i64, i64) = tokio::try_join!(
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = ?1)")
                .bind(&category_code).fetch_one(pool),
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_account WHERE code = ?1 AND archived = 0)")
                .bind(&account_code).fetch_one(pool),
        ).map_err(server_err)?;
        if cat_exists == 0 {
            return Err(ServerFnError::Args(format!("unknown category_code '{category_code}'")));
        }
        if acc_exists == 0 {
            return Err(ServerFnError::Args(format!("unknown or archived account_code '{account_code}'")));
        }

        let occurred = time::OffsetDateTime::now_utc().unix_timestamp();

        let mut tx = pool.begin().await.map_err(server_err)?;
        let doc_id = ep_core::next_doc_id(&mut tx, "FIN", ep_core::DocIdShape::YearSerial5)
            .await.map_err(server_err)?;
        sqlx::query(
            "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code, account_code, amount, tag, note, linked_doc_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        )
        .bind(&doc_id).bind(occurred).bind(&merchant).bind(&category_code)
        .bind(&account_code).bind(amount).bind(tag_kind.as_str())
        .bind(&note_opt).bind(&linked_opt)
        .execute(&mut *tx).await.map_err(server_err)?;

        sqlx::query(
            "UPDATE fin_account SET balance = balance + ?1 WHERE code = ?2"
        ).bind(amount).bind(&account_code).execute(&mut *tx).await.map_err(server_err)?;

        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc)
             VALUES (?1, 'FIN', ?2, ?3, ?4, ?5)"
        ).bind(occurred).bind(&doc_id).bind(&merchant).bind(amount).bind(&linked_opt)
         .execute(&mut *tx).await.map_err(server_err)?;

        if let Some(link) = &linked_opt {
            sqlx::query("INSERT OR IGNORE INTO module_link (source_doc, target_doc, kind) VALUES (?1, ?2, 'ref')")
                .bind(&doc_id).bind(link).execute(&mut *tx).await.map_err(server_err)?;
        }

        tx.commit().await.map_err(server_err)?;

        if amount < -500.0 {
            let n = ep_core::NotifyMessage::warn(format!("大额支出 · {merchant}"))
                .module("FIN")
                .body(format!("¥{:.2} ({})", amount.abs(), category_code))
                .doc_ref(doc_id.clone())
                .link("/finance");
            let _ = state.notify.dispatch(n).await;
        }

        Ok(Txn {
            doc_id, occurred_at: occurred, merchant,
            category_code, account_code,
            amount, tag: tag_kind.as_str().to_string(),
            note: note_opt, linked_doc_id: linked_opt,
        })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

#[server(DeleteTxn, "/api/_internal/fin", "Url", "delete_txn")]
pub async fn delete_txn(doc_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        let pool = &state.db;
        let mut tx = pool.begin().await.map_err(server_err)?;
        // Reverse the balance change first.
        let row: Option<(f64, String)> = sqlx::query_as(
            "SELECT amount, account_code FROM fin_txn WHERE doc_id = ?1"
        ).bind(&doc_id).fetch_optional(&mut *tx).await.map_err(server_err)?;
        if let Some((amount, account_code)) = row {
            sqlx::query("UPDATE fin_account SET balance = balance - ?1 WHERE code = ?2")
                .bind(amount).bind(&account_code).execute(&mut *tx).await.map_err(server_err)?;
        }
        sqlx::query("DELETE FROM fin_txn WHERE doc_id = ?1").bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        sqlx::query("DELETE FROM activity WHERE module = 'FIN' AND doc_id = ?1").bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        sqlx::query("DELETE FROM module_link WHERE source_doc = ?1").bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

/// Upsert a per-period, per-category budget. `amount <= 0` deletes the row
/// (treats "set budget to zero" as "remove budget"). `period` must look like
/// `YYYY-MM`; we accept whatever the form sends and trust the UI's <input
/// type="month">.
#[server(SetBudget, "/api/_internal/fin", "Url", "set_budget")]
pub async fn set_budget(
    period: String,
    category_code: String,
    amount: f64,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let period = period.trim().to_string();
        let category_code = category_code.trim().to_string();
        // Cheap sanity on period — full strict validation isn't worth a
        // regex dependency. Anything that doesn't look like YYYY-MM gets
        // rejected; the SQL would also fail-soft via "no current rows
        // match" but a clean error is friendlier.
        if period.len() != 7 || !period.chars().nth(4).map(|c| c == '-').unwrap_or(false) {
            return Err(ServerFnError::Args(format!("period must be YYYY-MM, got '{period}'")));
        }
        if category_code.is_empty() {
            return Err(ServerFnError::Args("category_code is required".into()));
        }
        if !amount.is_finite() {
            return Err(ServerFnError::Args("amount must be a finite number".into()));
        }

        let state: ep_core::AppState = expect_context();
        let pool = &state.db;

        if amount <= 0.0 {
            sqlx::query("DELETE FROM fin_budget WHERE period = ?1 AND category_code = ?2")
                .bind(&period).bind(&category_code)
                .execute(pool).await.map_err(server_err)?;
        } else {
            // ON CONFLICT updates the amount in place. Composite PK is
            // (period, category_code) per migrations/001_finance.sql.
            sqlx::query(
                "INSERT INTO fin_budget (period, category_code, amount) VALUES (?1, ?2, ?3)
                 ON CONFLICT(period, category_code) DO UPDATE SET amount = excluded.amount"
            )
            .bind(&period).bind(&category_code).bind(amount)
            .execute(pool).await.map_err(server_err)?;
        }
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

/// Copy every row of `fin_budget` from `source_period` into `target_period`,
/// overwriting any existing target rows. Drives the "导入上月预算" affordance
/// shown when the current period has no budgets yet.
#[server(ImportBudgetsFrom, "/api/_internal/fin", "Url", "import_budgets_from")]
pub async fn import_budgets_from(
    source_period: String,
    target_period: String,
) -> Result<i64, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let source_period = source_period.trim().to_string();
        let target_period = target_period.trim().to_string();
        if source_period == target_period {
            return Err(ServerFnError::Args("source and target periods must differ".into()));
        }
        for p in [&source_period, &target_period] {
            if p.len() != 7 || !p.chars().nth(4).map(|c| c == '-').unwrap_or(false) {
                return Err(ServerFnError::Args(format!("period must be YYYY-MM, got '{p}'")));
            }
        }
        let state: ep_core::AppState = expect_context();
        let pool = &state.db;
        let res = sqlx::query(
            "INSERT INTO fin_budget (period, category_code, amount)
             SELECT ?1, category_code, amount FROM fin_budget WHERE period = ?2
             ON CONFLICT(period, category_code) DO UPDATE SET amount = excluded.amount"
        )
        .bind(&target_period).bind(&source_period)
        .execute(pool).await.map_err(server_err)?;
        Ok(res.rows_affected() as i64)
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}
