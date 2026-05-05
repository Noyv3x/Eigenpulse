use crate::model::*;
#[cfg(feature = "ssr")]
use ep_core::server_err;
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use sqlx::SqlitePool;

/// `Args(...)` wrapper that pins `E = NoCustomError` so callsites don't
/// need turbofish to satisfy `ServerFnError`'s generic.
#[cfg(feature = "ssr")]
fn args_err(msg: impl Into<String>) -> ServerFnError {
    ServerFnError::Args(msg.into())
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
    if time::Date::parse(
        s,
        time::macros::format_description!("[year]-[month]-[day]"),
    ).is_err() {
        return Err(args_err(format!(
            "日期格式应为 YYYY-MM-DD,收到 '{s}'"
        )));
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
    /// All-time txn count per category code, used by the "在用" column on
    /// the categories management table. `serde(default)` for backward-compat
    /// with any cached client/server versions that pre-date this field.
    #[serde(default)]
    pub category_usage: std::collections::HashMap<String, i64>,
}

#[server(LoadLedger, "/api/_internal/fin", "Url", "load_ledger")]
pub async fn load_ledger() -> Result<LedgerData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        let pool = &state.db;

        type AccRow  = (String, String, String, String, f64, bool, i64);
        type CatRow  = (String, String, String, i64, bool, i64);
        type TxnRow  = (String, i64, String, String, String, f64, String, Option<String>, Option<String>);
        type SumRow  = (String, f64);

        // Accounts: return all (including archived) — UI filters in dropdowns
        // and the management Card. Mirrors the categories convention so the
        // hydrate side has full ground truth without a second fetch.
        // `liquid_balance` (below) still scopes to archived = 0 because that
        // KPI semantically excludes parked / closed accounts.
        let accounts_q = sqlx::query_as::<_, AccRow>(
            "SELECT code, name, type, tone, balance, archived, created_at
               FROM fin_account ORDER BY archived ASC, code"
        ).fetch_all(pool);
        // Categories: return all (including archived) — UI filters in dropdowns.
        let categories_q = sqlx::query_as::<_, CatRow>(
            "SELECT code, name, tone, sort_order, archived, created_at
               FROM fin_category ORDER BY sort_order"
        ).fetch_all(pool);
        let txns_q = sqlx::query_as::<_, TxnRow>(
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
              WHERE tag = 'exp'
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
                                 AND t.tag = 'exp'
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
                SUM(CASE WHEN tag = 'inc' AND amount > 0 THEN amount
                         WHEN tag = 'exp' AND amount < 0 THEN amount
                         ELSE 0.0 END), 0.0)
               FROM fin_txn
              WHERE occurred_at >= unixepoch('now','localtime','-7 days','utc')"
        ).fetch_one(pool);

        // 3-month rolling expense total, used for emergency-fund coverage and
        // the next-month-budget planner. 90-day window approximates 3 calendar
        // months without the awkward "what's my -3 month boundary" arithmetic.
        let expense_90d_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn
              WHERE tag = 'exp'
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
              WHERE tag = 'exp'
                AND occurred_at >= unixepoch('now','localtime','-13 days','start of day','utc')
              GROUP BY account_code, days_ago"
        ).fetch_all(pool);

        // 12-month income/expense bucket. Mirror reports.rs but scoped to a
        // single try_join so the finance page is one round-trip.
        type MonthRow = (String, f64, f64);
        let months_12_q = sqlx::query_as::<_, MonthRow>(
            "SELECT strftime('%Y-%m', occurred_at, 'unixepoch', 'localtime') AS period,
                    COALESCE(SUM(CASE WHEN tag='inc' AND amount > 0 THEN amount ELSE 0.0 END), 0.0) AS income,
                    COALESCE(SUM(CASE WHEN tag='exp' AND amount < 0 THEN -amount ELSE 0.0 END), 0.0) AS expense
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

        // All-time per-category txn count — drives the "在用" column on the
        // categories management table. Aggregating in SQL avoids shipping
        // every txn's category code over the wire just to count them.
        type CatUsageRow = (String, i64);
        let cat_usage_q = sqlx::query_as::<_, CatUsageRow>(
            "SELECT category_code, COUNT(*) FROM fin_txn GROUP BY category_code"
        ).fetch_all(pool);

        let (
            accounts, categories, txns_rows, cat_rows,
            income, expense, budget_total, budget_rows,
            week_net, expense_90d, liquid_balance, total_count,
            last_seen_rows, history_14d_rows,
            months_rows, months_frame, ctx, cat_usage_rows,
        ) = tokio::try_join!(
            accounts_q, categories_q, txns_q, cat_sum_q,
            income_q, expense_q, budget_q, budgets_q,
            week_net_q, expense_90d_q, liquid_balance_q, total_count_q,
            last_seen_q, history_14d_q,
            months_12_q, months_frame_q, context_q, cat_usage_q,
        ).map_err(server_err)?;

        let accounts = accounts.into_iter()
            .map(|r| Account { code: r.0, name: r.1, r#type: r.2, tone: r.3, balance: r.4, archived: r.5, created_at: r.6 })
            .collect::<Vec<_>>();
        let categories = categories.into_iter()
            .map(|r| Category { code: r.0, name: r.1, tone: r.2, sort_order: r.3, archived: r.4, created_at: r.5 })
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

        // Net worth excludes archived rows — those are closed/abandoned and
        // shouldn't pull the figure up or down. Mirrors the comment on
        // MonthSummary::balance ("Sum of every non-archived account's
        // current balance.").
        let balance: f64 = accounts.iter().filter(|a| !a.archived).map(|a| a.balance).sum();

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

        let category_usage: std::collections::HashMap<String, i64> =
            cat_usage_rows.into_iter().collect();

        Ok(LedgerData {
            accounts, account_stats, categories, txns, category_summary, budgets, months_12,
            category_usage,
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
    occurred_at: String,
) -> Result<Txn, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;

        let merchant = merchant.trim().to_string();
        if merchant.is_empty() {
            return Err(args_err("merchant is required"));
        }
        let tag_kind = match crate::model::Tag::parse(&tag) {
            Some(k) => k,
            None => return Err(args_err(format!("tag must be exp/inc/tfr, got '{tag}'"))),
        };
        if category_code.trim().is_empty() {
            return Err(args_err("category_code is required"));
        }
        if account_code.trim().is_empty() {
            return Err(args_err("account_code is required"));
        }
        // Form contract: positive amount, `tag` carries the sign (matches the
        // seed convention). `/api/v1/fin/txn` is a *separate* code path that
        // accepts pre-signed amounts — don't conflate the two.
        if !amount.is_finite() || amount < 0.005 {
            return Err(args_err("amount must be a positive number"));
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
            return Err(args_err(format!("unknown category_code '{category_code}'")));
        }
        if acc_exists == 0 {
            return Err(args_err(format!("unknown or archived account_code '{account_code}'")));
        }

        let occurred = parse_occurred_at(pool, &occurred_at)
            .await?
            .unwrap_or_else(|| time::OffsetDateTime::now_utc().unix_timestamp());

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
           FROM fin_txn WHERE doc_id = ?1"
    ).bind(doc_id).fetch_optional(&mut **tx).await.map_err(server_err)?;
    let (amount, account_code, tag, linked_doc_id) = match row {
        Some(r) => r,
        None => return Ok(None),
    };
    sqlx::query("UPDATE fin_account SET balance = balance - ?1 WHERE code = ?2")
        .bind(amount).bind(&account_code)
        .execute(&mut **tx).await.map_err(server_err)?;
    sqlx::query("DELETE FROM fin_txn WHERE doc_id = ?1")
        .bind(doc_id).execute(&mut **tx).await.map_err(server_err)?;
    sqlx::query("DELETE FROM activity WHERE module = 'FIN' AND doc_id = ?1")
        .bind(doc_id).execute(&mut **tx).await.map_err(server_err)?;
    // 'ref' is asymmetric (source-side only); 'tfr-pair' is symmetric.
    // One OR-query handles both vs two separate DELETEs.
    sqlx::query(
        "DELETE FROM module_link
          WHERE (source_doc = ?1 AND kind IN ('ref', 'tfr-pair'))
             OR (target_doc = ?1 AND kind = 'tfr-pair')"
    ).bind(doc_id).execute(&mut **tx).await.map_err(server_err)?;
    Ok(Some((tag, linked_doc_id)))
}

/// Delete a fin_txn row, undo its side effects, and cascade to the transfer
/// partner if one exists. Cascade authority is `module_link.kind='tfr-pair'`
/// (only `add_transfer_inner` writes those rows); single-leg `tag='tfr'`
/// from `add_txn` uses `kind='ref'` and is not a partner.
#[cfg(feature = "ssr")]
pub async fn delete_txn_inner(
    pool: &SqlitePool,
    doc_id: &str,
) -> Result<bool, ServerFnError> {
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
         )"
    ).bind(doc_id).fetch_all(&mut *tx).await.map_err(server_err)?;
    let pair_partner: Option<String> = match partners.len() {
        0 => None,
        1 => partners.into_iter().next(),
        _ => {
            tracing::error!(
                doc_id, partners = ?partners,
                "tfr-pair link table corrupt: multiple distinct partners"
            );
            return Err(server_err(format!(
                "transfer-pair links for '{doc_id}' point at {} distinct partners (expected 1); manual repair required",
                partners.len()
            )));
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
                return Err(server_err(format!(
                    "transfer partner '{partner_doc}' is missing; data drift detected, manual repair required before delete"
                )));
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
        let state: ep_core::AppState = expect_context();
        let _ = delete_txn_inner(&state.db, &doc_id).await?;
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

/// `amount <= 0` deletes the row (treats "set budget to zero" as remove).
#[cfg(feature = "ssr")]
pub async fn set_budget_inner(
    pool: &SqlitePool,
    period: &str,
    category_code: &str,
    amount: f64,
) -> Result<(), ServerFnError> {
    let period = period.trim();
    let category_code = category_code.trim();
    if period.len() != 7 || !period.chars().nth(4).map(|c| c == '-').unwrap_or(false) {
        return Err(args_err(format!("period must be YYYY-MM, got '{period}'")));
    }
    if category_code.is_empty() {
        return Err(args_err("category_code is required"));
    }
    if !amount.is_finite() {
        return Err(args_err("amount must be a finite number"));
    }
    if amount <= 0.0 {
        sqlx::query("DELETE FROM fin_budget WHERE period = ?1 AND category_code = ?2")
            .bind(period).bind(category_code)
            .execute(pool).await.map_err(server_err)?;
    } else {
        // ON CONFLICT updates the amount in place. Composite PK is
        // (period, category_code) per migrations/001_finance.sql.
        sqlx::query(
            "INSERT INTO fin_budget (period, category_code, amount) VALUES (?1, ?2, ?3)
             ON CONFLICT(period, category_code) DO UPDATE SET amount = excluded.amount"
        )
        .bind(period).bind(category_code).bind(amount)
        .execute(pool).await.map_err(server_err)?;
    }
    Ok(())
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
        let state: ep_core::AppState = expect_context();
        set_budget_inner(&state.db, &period, &category_code, amount).await
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
            return Err(args_err("source and target periods must differ"));
        }
        for p in [&source_period, &target_period] {
            if p.len() != 7 || !p.chars().nth(4).map(|c| c == '-').unwrap_or(false) {
                return Err(args_err(format!("period must be YYYY-MM, got '{p}'")));
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
    pub linked_doc_id: Option<String>,
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
    let merchant = fields.merchant.trim().to_string();
    if merchant.is_empty() {
        return Err(args_err("merchant is required"));
    }
    if !fields.amount.is_finite() {
        return Err(args_err("amount must be a finite number"));
    }
    if fields.category_code.trim().is_empty() {
        return Err(args_err("category_code is required"));
    }
    if fields.account_code.trim().is_empty() {
        return Err(args_err("account_code is required"));
    }

    let mut tx = pool.begin().await.map_err(server_err)?;

    // Read the existing row (and lock it implicitly under SQLite's deferred
    // tx). Capture old amount/account/tag/occurred so we can both refuse
    // tfr edits and compute balance deltas.
    type OldRow = (f64, String, String, i64, Option<String>);
    let old: Option<OldRow> = sqlx::query_as(
        "SELECT amount, account_code, tag, occurred_at, linked_doc_id
           FROM fin_txn WHERE doc_id = ?1"
    ).bind(doc_id).fetch_optional(&mut *tx).await.map_err(server_err)?;
    let (old_amount, old_account, old_tag, old_occurred, old_linked) = match old {
        Some(r) => r,
        None => return Err(args_err(format!("交易 '{doc_id}' 不存在"))),
    };
    if old_tag == "tfr" {
        return Err(args_err("转账记录不可编辑,请删除后重建"));
    }

    // tag is immutable, so amount sign is fully determined by old_tag.
    // UI sends abs (input min=0.01); coercing here keeps the invariant.
    let signed_amount = if old_tag == "exp" {
        -fields.amount.abs()
    } else {
        fields.amount.abs()
    };

    // Validate FK / archived constraints. New category may be archived
    // (allow re-categorizing into an archived bucket); new account must
    // exist AND not be archived. Sequential because we share one tx —
    // try_join would alias-borrow the connection.
    let cat_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = ?1)"
    ).bind(&fields.category_code)
     .fetch_one(&mut *tx).await.map_err(server_err)?;
    let acc_ok: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM fin_account WHERE code = ?1 AND archived = 0)"
    ).bind(&fields.account_code)
     .fetch_one(&mut *tx).await.map_err(server_err)?;
    if cat_exists == 0 {
        return Err(args_err(format!(
            "unknown category_code '{}'",
            fields.category_code
        )));
    }
    if acc_ok == 0 {
        return Err(args_err(format!(
            "unknown or archived account_code '{}'",
            fields.account_code
        )));
    }

    // occurred_at: empty → keep existing.
    let new_occurred = match parse_occurred_at(pool, &fields.occurred_at_input).await? {
        Some(ts) => ts,
        None => old_occurred,
    };

    // Balance delta uses `signed_amount`, not the raw input. SQLite forbids
    // running queries on the pool concurrently while a tx is open on it,
    // so do these sequentially.
    if old_account == fields.account_code {
        sqlx::query(
            "UPDATE fin_account SET balance = balance + (?1 - ?2) WHERE code = ?3"
        )
        .bind(signed_amount).bind(old_amount).bind(&fields.account_code)
        .execute(&mut *tx).await.map_err(server_err)?;
    } else {
        sqlx::query("UPDATE fin_account SET balance = balance - ?1 WHERE code = ?2")
            .bind(old_amount).bind(&old_account)
            .execute(&mut *tx).await.map_err(server_err)?;
        sqlx::query("UPDATE fin_account SET balance = balance + ?1 WHERE code = ?2")
            .bind(signed_amount).bind(&fields.account_code)
            .execute(&mut *tx).await.map_err(server_err)?;
    }

    sqlx::query(
        "UPDATE fin_txn
            SET merchant = ?1, category_code = ?2, account_code = ?3,
                amount = ?4, note = ?5, occurred_at = ?6, linked_doc_id = ?7
          WHERE doc_id = ?8"
    )
    .bind(&merchant)
    .bind(&fields.category_code)
    .bind(&fields.account_code)
    .bind(signed_amount)
    .bind(&fields.note)
    .bind(new_occurred)
    .bind(&fields.linked_doc_id)
    .bind(doc_id)
    .execute(&mut *tx).await.map_err(server_err)?;

    sqlx::query(
        "UPDATE activity SET summary = ?1, amount = ?2, link_doc = ?3, occurred_at = ?4
          WHERE module = 'FIN' AND doc_id = ?5"
    )
    .bind(&merchant)
    .bind(signed_amount)
    .bind(&fields.linked_doc_id)
    .bind(new_occurred)
    .bind(doc_id)
    .execute(&mut *tx).await.map_err(server_err)?;

    // module_link: edit only kind='ref' rows; kind='tfr-pair' is delete-only.
    if old_linked != fields.linked_doc_id {
        sqlx::query(
            "DELETE FROM module_link WHERE source_doc = ?1 AND kind = 'ref'"
        ).bind(doc_id).execute(&mut *tx).await.map_err(server_err)?;
        if let Some(target) = &fields.linked_doc_id {
            if !target.trim().is_empty() {
                sqlx::query(
                    "INSERT OR IGNORE INTO module_link (source_doc, target_doc, kind)
                     VALUES (?1, ?2, 'ref')"
                ).bind(doc_id).bind(target).execute(&mut *tx).await.map_err(server_err)?;
            }
        }
    }

    tx.commit().await.map_err(server_err)?;

    Ok(Txn {
        doc_id: doc_id.to_string(),
        occurred_at: new_occurred,
        merchant,
        category_code: fields.category_code,
        account_code: fields.account_code,
        amount: signed_amount,
        tag: old_tag,
        note: fields.note,
        linked_doc_id: fields.linked_doc_id,
    })
}

#[server(UpdateTxn, "/api/_internal/fin", "Url", "update_txn")]
pub async fn update_txn(
    doc_id: String,
    merchant: String,
    category_code: String,
    account_code: String,
    amount: f64,
    note: String,
    occurred_at: String,
    linked_doc_id: String,
) -> Result<Txn, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        let note_opt = if note.trim().is_empty() { None } else { Some(note) };
        let linked_opt = if linked_doc_id.trim().is_empty() { None } else { Some(linked_doc_id) };
        update_txn_inner(
            &state.db,
            &doc_id,
            UpdateTxnFields {
                merchant,
                category_code,
                account_code,
                amount,
                note: note_opt,
                occurred_at_input: occurred_at,
                linked_doc_id: linked_opt,
            },
        ).await
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

// ---------------------------------------------------------------------------
// Transfer (paired tfr txns)
// ---------------------------------------------------------------------------

/// Writes two paired `tag='tfr'` `fin_txn` rows + symmetric `module_link`
/// `kind='tfr-pair'` rows in one tx. Both legs share `occurred_at` and the
/// `'TFR'` category. `delete_txn_inner` cascades via the `tfr-pair` links.
///
/// Validates inputs (non-empty / distinct accounts / finite positive amount,
/// FK + archived check on both accounts and TFR category). Wrappers don't
/// need to re-validate.
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
        return Err(args_err("from_account / to_account 都必填"));
    }
    if from_account == to_account {
        return Err(args_err("转出与转入账户不能相同"));
    }
    if !amount.is_finite() || amount < 0.005 {
        return Err(args_err("amount 必须是正数"));
    }
    let (from_ok, to_ok, tfr_ok): (i64, i64, i64) = tokio::try_join!(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_account WHERE code = ?1 AND archived = 0)")
            .bind(from_account).fetch_one(pool),
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_account WHERE code = ?1 AND archived = 0)")
            .bind(to_account).fetch_one(pool),
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = 'TFR' AND archived = 0)")
            .fetch_one(pool),
    ).map_err(server_err)?;
    if from_ok == 0 {
        return Err(args_err(format!("unknown or archived account_code '{from_account}'")));
    }
    if to_ok == 0 {
        return Err(args_err(format!("unknown or archived account_code '{to_account}'")));
    }
    if tfr_ok == 0 {
        return Err(args_err("分类 TFR 不存在或已归档；请到分类管理新建/取消归档"));
    }

    let mut tx = pool.begin().await.map_err(server_err)?;
    let from_doc = ep_core::next_doc_id(&mut tx, "FIN", ep_core::DocIdShape::YearSerial5)
        .await.map_err(server_err)?;
    let to_doc = ep_core::next_doc_id(&mut tx, "FIN", ep_core::DocIdShape::YearSerial5)
        .await.map_err(server_err)?;

    let from_merchant = format!("转出 → {to_account}");
    let to_merchant = format!("转入 ← {from_account}");
    let note_owned = note.map(|s| s.to_string());

    sqlx::query(
        "INSERT INTO fin_txn
            (doc_id, occurred_at, merchant, category_code, account_code,
             amount, tag, note, linked_doc_id)
         VALUES (?1, ?2, ?3, 'TFR', ?4, ?5, 'tfr', ?6, ?7)"
    )
    .bind(&from_doc).bind(occurred_at).bind(&from_merchant).bind(from_account)
    .bind(-amount).bind(&note_owned).bind(&to_doc)
    .execute(&mut *tx).await.map_err(server_err)?;
    sqlx::query(
        "INSERT INTO fin_txn
            (doc_id, occurred_at, merchant, category_code, account_code,
             amount, tag, note, linked_doc_id)
         VALUES (?1, ?2, ?3, 'TFR', ?4, ?5, 'tfr', NULL, ?6)"
    )
    .bind(&to_doc).bind(occurred_at).bind(&to_merchant).bind(to_account)
    .bind(amount).bind(&from_doc)
    .execute(&mut *tx).await.map_err(server_err)?;

    sqlx::query("UPDATE fin_account SET balance = balance - ?1 WHERE code = ?2")
        .bind(amount).bind(from_account)
        .execute(&mut *tx).await.map_err(server_err)?;
    sqlx::query("UPDATE fin_account SET balance = balance + ?1 WHERE code = ?2")
        .bind(amount).bind(to_account)
        .execute(&mut *tx).await.map_err(server_err)?;

    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc)
         VALUES (?1, 'FIN', ?2, ?3, ?4, ?5)"
    )
    .bind(occurred_at).bind(&from_doc).bind(&from_merchant).bind(-amount).bind(&to_doc)
    .execute(&mut *tx).await.map_err(server_err)?;
    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc)
         VALUES (?1, 'FIN', ?2, ?3, ?4, ?5)"
    )
    .bind(occurred_at).bind(&to_doc).bind(&to_merchant).bind(amount).bind(&from_doc)
    .execute(&mut *tx).await.map_err(server_err)?;

    // Symmetric pair so the cascade lookup walks either direction.
    sqlx::query(
        "INSERT INTO module_link (source_doc, target_doc, kind) VALUES (?1, ?2, 'tfr-pair')"
    ).bind(&from_doc).bind(&to_doc).execute(&mut *tx).await.map_err(server_err)?;
    sqlx::query(
        "INSERT INTO module_link (source_doc, target_doc, kind) VALUES (?1, ?2, 'tfr-pair')"
    ).bind(&to_doc).bind(&from_doc).execute(&mut *tx).await.map_err(server_err)?;

    tx.commit().await.map_err(server_err)?;

    Ok((
        Txn {
            doc_id: from_doc.clone(), occurred_at,
            merchant: from_merchant,
            category_code: "TFR".into(), account_code: from_account.into(),
            amount: -amount, tag: "tfr".into(),
            note: note_owned.clone(), linked_doc_id: Some(to_doc.clone()),
        },
        Txn {
            doc_id: to_doc, occurred_at,
            merchant: to_merchant,
            category_code: "TFR".into(), account_code: to_account.into(),
            amount, tag: "tfr".into(),
            note: None, linked_doc_id: Some(from_doc),
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
        let state: ep_core::AppState = expect_context();
        let pool = &state.db;

        let occurred = parse_occurred_at(pool, &occurred_at)
            .await?
            .unwrap_or_else(|| time::OffsetDateTime::now_utc().unix_timestamp());

        let note_opt = if note.trim().is_empty() { None } else { Some(note) };
        add_transfer_inner(
            pool,
            &from_account,
            &to_account,
            amount,
            note_opt.as_deref(),
            occurred,
        ).await
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
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
    if code.len() < 2 || code.len() > 16 {
        return Err(args_err("code 必须 2..=16 字符,且只允许大写字母/数字/连字符"));
    }
    if !code.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-') {
        return Err(args_err("code 只允许大写字母/数字/连字符"));
    }
    if name.is_empty() || name.chars().count() > 64 {
        return Err(args_err("name 必填且长度不超过 64 字符"));
    }
    if !ACCOUNT_TYPES.contains(&r#type.as_str()) {
        return Err(args_err(format!(
            "type 必须是 {:?} 之一",
            ACCOUNT_TYPES,
        )));
    }
    if !tone.is_empty() && !TONES.contains(&tone.as_str()) {
        return Err(args_err(format!("tone 必须为空或 {:?} 之一", TONES)));
    }
    Ok((code, name, r#type, tone))
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
        return Err(args_err("opening_balance 必须为有限数"));
    }
    let (code, name, r#type, tone) = validate_account_input(&code, &name, &r#type, &tone)?;
    let res = sqlx::query(
        "INSERT INTO fin_account (code, name, type, tone, balance, archived, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, unixepoch())"
    )
    .bind(&code).bind(&name).bind(&r#type).bind(&tone).bind(opening_balance)
    .execute(pool).await;
    if let Err(e) = res {
        if is_unique_violation(&e) {
            return Err(args_err(format!("账户 code '{code}' 已存在")));
        }
        return Err(server_err(e));
    }
    let row: (String, String, String, String, f64, bool, i64) = sqlx::query_as(
        "SELECT code, name, type, tone, balance, archived, created_at
           FROM fin_account WHERE code = ?1"
    ).bind(&code).fetch_one(pool).await.map_err(server_err)?;
    Ok(Account {
        code: row.0, name: row.1, r#type: row.2, tone: row.3,
        balance: row.4, archived: row.5, created_at: row.6,
    })
}

#[cfg(feature = "ssr")]
pub async fn update_account_inner(
    pool: &SqlitePool,
    code: String,
    name: String,
    r#type: String,
    tone: String,
) -> Result<Account, ServerFnError> {
    let (code, name, r#type, tone) = validate_account_input(&code, &name, &r#type, &tone)?;
    let res = sqlx::query(
        "UPDATE fin_account SET name = ?1, type = ?2, tone = ?3 WHERE code = ?4"
    )
    .bind(&name).bind(&r#type).bind(&tone).bind(&code)
    .execute(pool).await.map_err(server_err)?;
    if res.rows_affected() == 0 {
        return Err(args_err(format!("账户 '{code}' 不存在")));
    }
    let row: (String, String, String, String, f64, bool, i64) = sqlx::query_as(
        "SELECT code, name, type, tone, balance, archived, created_at
           FROM fin_account WHERE code = ?1"
    ).bind(&code).fetch_one(pool).await.map_err(server_err)?;
    Ok(Account {
        code: row.0, name: row.1, r#type: row.2, tone: row.3,
        balance: row.4, archived: row.5, created_at: row.6,
    })
}

#[cfg(feature = "ssr")]
pub async fn archive_account_inner(
    pool: &SqlitePool,
    code: String,
    archived: bool,
) -> Result<(), ServerFnError> {
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(args_err("code is required"));
    }
    // No-op short-circuit: skip the UPDATE (and the action.version() tick
    // it triggers via the caller) when the row is already in target state.
    let res = sqlx::query(
        "UPDATE fin_account SET archived = ?1 WHERE code = ?2 AND archived <> ?1"
    ).bind(archived).bind(&code)
        .execute(pool).await.map_err(server_err)?;
    if res.rows_affected() == 0 {
        let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_account WHERE code = ?1)")
            .bind(&code).fetch_one(pool).await.map_err(server_err)?;
        if exists == 0 {
            return Err(args_err(format!("账户 '{code}' 不存在")));
        }
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
        let state: ep_core::AppState = expect_context();
        create_account_inner(&state.db, code, name, r#type, tone, opening_balance).await
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
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
        let state: ep_core::AppState = expect_context();
        update_account_inner(&state.db, code, name, r#type, tone).await
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

#[cfg(feature = "ssr")]
pub async fn list_accounts_inner(
    pool: &SqlitePool,
    include_archived: bool,
) -> sqlx::Result<Vec<Account>> {
    let flag: i64 = if include_archived { 1 } else { 0 };
    type Row = (String, String, String, String, f64, bool, i64);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT code, name, type, tone, balance, archived, created_at
           FROM fin_account
          WHERE ?1 = 1 OR archived = 0
          ORDER BY archived ASC, code ASC"
    ).bind(flag).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| Account {
        code: r.0, name: r.1, r#type: r.2, tone: r.3,
        balance: r.4, archived: r.5, created_at: r.6,
    }).collect())
}

#[server(ListAccounts, "/api/_internal/fin", "Url", "list_accounts")]
pub async fn list_accounts(include_archived: bool) -> Result<Vec<Account>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        list_accounts_inner(&state.db, include_archived).await.map_err(server_err)
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

#[server(ArchiveAccount, "/api/_internal/fin", "Url", "archive_account")]
pub async fn archive_account(
    code: String,
    archived: bool,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        archive_account_inner(&state.db, code, archived).await
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
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
    // Inline char-class check (no regex dep). Accepts `&` for seed code F&B.
    if code.is_empty() || code.len() > 8
        || !code.chars().all(|c| c.is_ascii_uppercase() || c == '&')
    {
        return Err(args_err("code 必须 1..=8 字符,只允许大写字母和 '&'"));
    }
    if name.is_empty() || name.chars().count() > 32 {
        return Err(args_err("name 必填且长度不超过 32 字符"));
    }
    if !tone.is_empty() && !TONES.contains(&tone.as_str()) {
        return Err(args_err(format!("tone 必须为空或 {:?} 之一", TONES)));
    }
    Ok((code, name, tone))
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
    let res = sqlx::query(
        "INSERT INTO fin_category (code, name, tone, sort_order, archived, created_at)
         VALUES (?1, ?2, ?3, ?4, 0, unixepoch())"
    )
    .bind(&code).bind(&name).bind(&tone).bind(sort_order)
    .execute(pool).await;
    if let Err(e) = res {
        if is_unique_violation(&e) {
            return Err(args_err(format!("分类 code '{code}' 已存在")));
        }
        return Err(server_err(e));
    }
    let row: (String, String, String, i64, bool, i64) = sqlx::query_as(
        "SELECT code, name, tone, sort_order, archived, created_at
           FROM fin_category WHERE code = ?1"
    ).bind(&code).fetch_one(pool).await.map_err(server_err)?;
    Ok(Category {
        code: row.0, name: row.1, tone: row.2, sort_order: row.3,
        archived: row.4, created_at: row.5,
    })
}

#[cfg(feature = "ssr")]
pub async fn update_category_inner(
    pool: &SqlitePool,
    code: String,
    name: String,
    tone: String,
    sort_order: i64,
) -> Result<Category, ServerFnError> {
    let (code, name, tone) = validate_category_input(&code, &name, &tone)?;
    let res = sqlx::query(
        "UPDATE fin_category SET name = ?1, tone = ?2, sort_order = ?3 WHERE code = ?4"
    )
    .bind(&name).bind(&tone).bind(sort_order).bind(&code)
    .execute(pool).await.map_err(server_err)?;
    if res.rows_affected() == 0 {
        return Err(args_err(format!("分类 '{code}' 不存在")));
    }
    let row: (String, String, String, i64, bool, i64) = sqlx::query_as(
        "SELECT code, name, tone, sort_order, archived, created_at
           FROM fin_category WHERE code = ?1"
    ).bind(&code).fetch_one(pool).await.map_err(server_err)?;
    Ok(Category {
        code: row.0, name: row.1, tone: row.2, sort_order: row.3,
        archived: row.4, created_at: row.5,
    })
}

#[cfg(feature = "ssr")]
pub async fn archive_category_inner(
    pool: &SqlitePool,
    code: String,
    archived: bool,
) -> Result<(), ServerFnError> {
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(args_err("code is required"));
    }
    let res = sqlx::query(
        "UPDATE fin_category SET archived = ?1 WHERE code = ?2 AND archived <> ?1"
    ).bind(archived).bind(&code)
        .execute(pool).await.map_err(server_err)?;
    if res.rows_affected() == 0 {
        let exists: i64 = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = ?1)")
            .bind(&code).fetch_one(pool).await.map_err(server_err)?;
        if exists == 0 {
            return Err(args_err(format!("分类 '{code}' 不存在")));
        }
    }
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
        let state: ep_core::AppState = expect_context();
        create_category_inner(&state.db, code, name, tone, sort_order).await
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
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
        let state: ep_core::AppState = expect_context();
        update_category_inner(&state.db, code, name, tone, sort_order).await
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

#[server(ArchiveCategory, "/api/_internal/fin", "Url", "archive_category")]
pub async fn archive_category(
    code: String,
    archived: bool,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        archive_category_inner(&state.db, code, archived).await
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}
