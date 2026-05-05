//! Integration tests for finance CRUD helpers; SSR-only (sqlx).
//! Each test owns a fresh in-memory pool — no shared state, no cleanup.

#![cfg(feature = "ssr")]

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

/// Spin up a fresh in-memory SQLite pool and apply both the global core
/// migrations and the finance module migrations against it. Mirrors
/// `crates/db/src/pool.rs::open_pool` but uses `sqlite::memory:` so each
/// test gets a clean slate; we deliberately do NOT touch
/// `data/eigenpulse.db`.
///
/// `max_connections = 1` is load-bearing: SQLite `:memory:` is per-connection
/// (each connection sees its own empty DB), so a multi-conn pool would route
/// queries to different in-memory instances and tests would observe an empty
/// schema on whichever conn they happened to grab. The blanket `?cache=shared`
/// trick exists but is finicky on `sqlx 0.8` + tokio runtimes; pinning to one
/// conn is the simpler, well-understood path for unit-style tests.
async fn make_test_pool() -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")?
        .journal_mode(SqliteJournalMode::Memory)
        .synchronous(SqliteSynchronous::Off)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(2))
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(opts)
        .await?;

    // 1. Core migrations (app_user, session, seq, _ep_module_migration,
    //    module_link, activity, notification, …) — the global migrator.
    ep_db::CORE_MIGRATOR.run(&pool).await?;

    // 2. Finance module migrations (001_finance + 002_finance_crud) — applied
    //    via the same idempotent ledger code-path that production uses.
    //    We call `MODULE.migrations()` directly rather than building a full
    //    `ModuleRegistry` because the registry pulls in axum + the rest of
    //    the SSR app and we don't need any of that here.
    apply_module_migrations(&pool, ep_finance::MODULE).await?;

    // 3. Strip seed data so each test starts from a known-empty fixture.
    //    `001_finance.sql` ships demo accounts/categories/budgets/txns to
    //    make a fresh `cargo leptos watch` look populated; for unit tests
    //    that's noise. We drop everything and let the test seed exactly
    //    what it needs. We do NOT touch `_ep_module_migration` — the ledger
    //    must keep saying "002 applied" so a subsequent
    //    `apply_module_migrations` call (idempotency check) is a no-op.
    sqlx::query("DELETE FROM fin_txn").execute(&pool).await?;
    sqlx::query("DELETE FROM fin_budget").execute(&pool).await?;
    sqlx::query("DELETE FROM fin_account").execute(&pool).await?;
    sqlx::query("DELETE FROM fin_category").execute(&pool).await?;
    sqlx::query("DELETE FROM activity WHERE module = 'FIN'").execute(&pool).await?;
    sqlx::query("DELETE FROM seq WHERE module = 'FIN'").execute(&pool).await?;

    Ok(pool)
}

/// Stripped-down clone of `ModuleRegistry::run_migrations` for one module —
/// re-applies the ledger contract (`_ep_module_migration` filename-keyed
/// idempotent INSERT). Kept inline so the test harness doesn't depend on
/// the full registry → that pulls axum, leptos_axum, and a half-dozen other
/// crates we don't need for sqlite-only tests.
async fn apply_module_migrations(
    pool: &SqlitePool,
    module: &dyn ep_core::Module,
) -> anyhow::Result<()> {
    for (name, sql) in module.migrations() {
        let already: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM _ep_module_migration WHERE module = ?1 AND name = ?2",
        )
        .bind(module.code())
        .bind(*name)
        .fetch_optional(pool)
        .await?;
        if already.is_some() {
            continue;
        }
        let mut tx = pool.begin().await?;
        // Strip `--` line comments first — the prod registry uses a fully
        // quote-and-comment-aware splitter, but lifting that requires an
        // `AppState`. Stripping line comments is enough for the migrations
        // we ship; none of them contain `;` inside a string literal or
        // block comment. (If that ever changes, lift `split_sql` to a
        // `pub fn` in `ep_core` and call it here.) The `;`-inside-line-
        // comment foot-gun is real: 002_finance_crud's "follow-up UPDATE."
        // sentence trips the naive splitter.
        let stripped = strip_line_comments(sql);
        for stmt in stripped.split(';') {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            sqlx::query(stmt).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO _ep_module_migration(module, name) VALUES (?1, ?2)")
            .bind(module.code())
            .bind(*name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    Ok(())
}

/// Drop every `--`…end-of-line comment in `sql`. We can't do this in the
/// migration files themselves (the prod registry's quote-aware splitter
/// keeps comments verbatim, which is the right call for the production
/// path). For the test harness's simple `;`-splitter, removing line
/// comments before the split is the cheapest way to handle a migration
/// like `002_finance_crud.sql` whose comment text contains a stray `;`.
fn strip_line_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    for line in sql.lines() {
        match line.find("--") {
            Some(idx) => {
                out.push_str(&line[..idx]);
                out.push('\n');
            }
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out
}

/// Insert one `fin_account` row at the given starting balance, returning
/// nothing — call-sites assert on `fetch_balance` post-state. `archived = 0`,
/// `tone = ''` mirror the seed defaults.
async fn seed_account(pool: &SqlitePool, code: &str, balance: f64) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO fin_account (code, name, type, tone, balance, archived) \
         VALUES (?1, ?2, 'Checking', '', ?3, 0)",
    )
    .bind(code)
    .bind(format!("Test {}", code))
    .bind(balance)
    .execute(pool)
    .await?;
    Ok(())
}

/// Insert one `fin_category` row. Tests need at least one expense category
/// and (for transfers) the canonical `TFR` category — we let the call-site
/// pick.
async fn seed_category(pool: &SqlitePool, code: &str, name: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO fin_category (code, name, tone, sort_order, archived) \
         VALUES (?1, ?2, '', 0, 0)",
    )
    .bind(code)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Read back the current balance of an account. Returns `None` if the row
/// is absent (lets transfer-cascade tests assert "row didn't accidentally
/// vanish").
async fn fetch_balance(pool: &SqlitePool, code: &str) -> anyhow::Result<Option<f64>> {
    let bal: Option<f64> = sqlx::query_scalar("SELECT balance FROM fin_account WHERE code = ?1")
        .bind(code)
        .fetch_optional(pool)
        .await?;
    Ok(bal)
}

/// Count the number of `fin_txn` rows. Used to assert "deletion took
/// effect" / "transfer added exactly two rows".
async fn count_txns(pool: &SqlitePool) -> anyhow::Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_txn")
        .fetch_one(pool)
        .await?;
    Ok(n)
}

/// Count `module_link` rows of a particular `kind`. The transfer-pair test
/// verifies both legs got their `kind = 'tfr-pair'` entry, then verifies
/// they're cleaned up after delete.
async fn count_links_by_kind(pool: &SqlitePool, kind: &str) -> anyhow::Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM module_link WHERE kind = ?1")
        .bind(kind)
        .fetch_one(pool)
        .await?;
    Ok(n)
}

// Sanity / scaffold checks.

/// The pool helper itself is non-trivial — make sure it compiles and the
/// migrations apply cleanly before any of the real tests try to use it.
#[tokio::test]
async fn pool_helper_applies_finance_migrations() {
    let pool = make_test_pool().await.expect("pool init");

    // 002_finance_crud added these columns; if 002 didn't run, the SELECT
    // would fail with "no such column".
    let row: (i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM pragma_table_info('fin_account') WHERE name = 'created_at'), \
            (SELECT COUNT(*) FROM pragma_table_info('fin_category') WHERE name = 'archived')",
    )
    .fetch_one(&pool)
    .await
    .expect("schema check");
    assert_eq!(row, (1, 1), "expected 002_finance_crud columns to exist");

    // Migrations should be ledger'd as applied.
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM _ep_module_migration WHERE module = 'FIN'",
    )
    .fetch_one(&pool)
    .await
    .expect("ledger");
    assert_eq!(n, 2, "expected both finance migrations to be ledgered");
}

/// Idempotency: running the migrations a second time should be a no-op,
/// not a duplicate-key error. Mirrors what the registry does on every
/// boot.
#[tokio::test]
async fn pool_helper_is_idempotent_on_double_apply() {
    let pool = make_test_pool().await.expect("first apply");
    apply_module_migrations(&pool, ep_finance::MODULE)
        .await
        .expect("second apply must be no-op");
    // Still exactly two ledger rows.
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM _ep_module_migration WHERE module = 'FIN'",
    )
    .fetch_one(&pool)
    .await
    .expect("ledger");
    assert_eq!(n, 2);
}

/// `seed_account` + `fetch_balance` round-trip. Cheap fixture-helper test
/// to lock in their behaviour before the real CRUD tests rely on them.
#[tokio::test]
async fn fixture_helpers_round_trip() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-T", 100.0).await.expect("seed");
    seed_category(&pool, "EXP", "Expense").await.expect("seed");
    assert_eq!(fetch_balance(&pool, "ACC-T").await.unwrap(), Some(100.0));
    assert_eq!(fetch_balance(&pool, "ACC-MISSING").await.unwrap(), None);
    assert_eq!(count_txns(&pool).await.unwrap(), 0);
}

// parse_occurred_at three-state: empty / valid / malformed.

#[tokio::test]
async fn parse_occurred_at_empty_returns_none() {
    let pool = make_test_pool().await.expect("pool");
    let got = ep_finance::parse_occurred_at(&pool, "").await.expect("empty ok");
    assert_eq!(got, None, "empty → Ok(None) so caller picks 'now' or 'keep'");
    let got2 = ep_finance::parse_occurred_at(&pool, "   ").await.expect("ws ok");
    assert_eq!(got2, None, "all-whitespace also Ok(None)");
}

#[tokio::test]
async fn parse_occurred_at_iso_date_returns_some_within_local_day() {
    let pool = make_test_pool().await.expect("pool");
    let got = ep_finance::parse_occurred_at(&pool, "2024-05-01")
        .await
        .expect("iso ok")
        .expect("Some(_)");
    // 2024-05-01 12:00 local clock; absolute unix depends on $TZ. The whole
    // calendar day in UTC is 1714521600 .. 1714608000. Allow ±18h around
    // the UTC-noon anchor (1714564800) to cover any timezone offset.
    let utc_noon = 1_714_564_800_i64;
    let delta = (got - utc_noon).abs();
    assert!(
        delta <= 18 * 3600,
        "expected within ±18h of UTC-noon ({utc_noon}); got {got} (delta {delta}s)"
    );
}

#[tokio::test]
async fn parse_occurred_at_rejects_malformed_inputs() {
    let pool = make_test_pool().await.expect("pool");
    for bad in ["not-a-date", "2024-13-01", "2024/05/01", "abc", "2024-05"] {
        let res = ep_finance::parse_occurred_at(&pool, bad).await;
        assert!(
            res.is_err(),
            "expected Err for malformed input '{bad}', got {res:?}"
        );
    }
}

// update_txn balance delta — same-account and cross-account.

/// Insert a fin_txn directly + nudge the account balance accordingly. Used
/// by the update_txn tests as a "previous state" setup primitive.
async fn seed_txn_directly(
    pool: &SqlitePool,
    doc_id: &str,
    account_code: &str,
    category_code: &str,
    amount: f64,
    tag: &str,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code, account_code,
                              amount, tag, note, linked_doc_id)
         VALUES (?1, 1700000000, 'seed', ?2, ?3, ?4, ?5, NULL, NULL)",
    )
    .bind(doc_id)
    .bind(category_code)
    .bind(account_code)
    .bind(amount)
    .bind(tag)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE fin_account SET balance = balance + ?1 WHERE code = ?2")
        .bind(amount)
        .bind(account_code)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[tokio::test]
async fn update_txn_balance_delta_same_account() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-T", 0.0).await.unwrap();
    seed_category(&pool, "EXP", "Expense").await.unwrap();
    seed_txn_directly(&pool, "FIN-99001", "ACC-T", "EXP", -100.0, "exp")
        .await
        .unwrap();
    assert_eq!(fetch_balance(&pool, "ACC-T").await.unwrap(), Some(-100.0));

    // Change amount -100 → -50. Delta = -50 - (-100) = +50.
    ep_finance::update_txn_inner(
        &pool,
        "FIN-99001",
        ep_finance::UpdateTxnFields {
            merchant: "edited".into(),
            category_code: "EXP".into(),
            account_code: "ACC-T".into(),
            amount: -50.0,
            note: None,
            occurred_at_input: String::new(),
            linked_doc_id: None,
        },
    )
    .await
    .expect("update ok");

    assert_eq!(
        fetch_balance(&pool, "ACC-T").await.unwrap(),
        Some(-50.0),
        "same-account balance should track delta = new_amount - old_amount"
    );
}

#[tokio::test]
async fn update_txn_cross_account() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-A", 0.0).await.unwrap();
    seed_account(&pool, "ACC-B", 0.0).await.unwrap();
    seed_category(&pool, "EXP", "Expense").await.unwrap();
    seed_txn_directly(&pool, "FIN-99002", "ACC-A", "EXP", -100.0, "exp")
        .await
        .unwrap();
    assert_eq!(fetch_balance(&pool, "ACC-A").await.unwrap(), Some(-100.0));
    assert_eq!(fetch_balance(&pool, "ACC-B").await.unwrap(), Some(0.0));

    // Move the txn from ACC-A → ACC-B, same amount.
    ep_finance::update_txn_inner(
        &pool,
        "FIN-99002",
        ep_finance::UpdateTxnFields {
            merchant: "moved".into(),
            category_code: "EXP".into(),
            account_code: "ACC-B".into(),
            amount: -100.0,
            note: None,
            occurred_at_input: String::new(),
            linked_doc_id: None,
        },
    )
    .await
    .expect("cross-account update ok");

    assert_eq!(
        fetch_balance(&pool, "ACC-A").await.unwrap(),
        Some(0.0),
        "ACC-A reverts: balance -= old_amount (-100) → +100, net 0"
    );
    assert_eq!(
        fetch_balance(&pool, "ACC-B").await.unwrap(),
        Some(-100.0),
        "ACC-B picks up: balance += new_amount (-100)"
    );
}

// add_transfer pair + delete cascade.

#[tokio::test]
async fn add_transfer_creates_pair_and_delete_cascades() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    let occurred_at = 1_700_000_000_i64;
    let (from_txn, to_txn) = ep_finance::add_transfer_inner(
        &pool,
        "ACC-FROM",
        "ACC-TO",
        300.0,
        None,
        occurred_at,
    )
    .await
    .expect("transfer ok");

    // Two fin_txn rows + two symmetric `tfr-pair` link rows, balances move.
    assert_eq!(count_txns(&pool).await.unwrap(), 2, "transfer creates 2 fin_txn rows");
    assert_eq!(
        count_links_by_kind(&pool, "tfr-pair").await.unwrap(),
        2,
        "two symmetric (from→to, to→from) tfr-pair links"
    );
    assert_eq!(fetch_balance(&pool, "ACC-FROM").await.unwrap(), Some(700.0));
    assert_eq!(fetch_balance(&pool, "ACC-TO").await.unwrap(), Some(300.0));

    // Sanity-check the returned Txn shape — both sides are tag='tfr' and
    // share the occurred_at the caller picked.
    assert_eq!(from_txn.tag, "tfr");
    assert_eq!(to_txn.tag, "tfr");
    assert_eq!(from_txn.amount, -300.0);
    assert_eq!(to_txn.amount, 300.0);
    assert_eq!(from_txn.occurred_at, occurred_at);

    // Delete the *from* leg → cascade to the *to* leg.
    let deleted = ep_finance::delete_txn_inner(&pool, &from_txn.doc_id)
        .await
        .expect("cascade ok");
    assert!(deleted, "delete_txn_inner returned true for an existing row");

    assert_eq!(
        count_txns(&pool).await.unwrap(),
        0,
        "deleting one leg of a transfer must cascade-delete the partner"
    );
    assert_eq!(
        count_links_by_kind(&pool, "tfr-pair").await.unwrap(),
        0,
        "both tfr-pair links cleaned up"
    );
    assert_eq!(
        fetch_balance(&pool, "ACC-FROM").await.unwrap(),
        Some(1000.0),
        "ACC-FROM balance reverts to pre-transfer state"
    );
    assert_eq!(
        fetch_balance(&pool, "ACC-TO").await.unwrap(),
        Some(0.0),
        "ACC-TO balance reverts to pre-transfer state"
    );
}

/// Bonus assertion — deleting the *to* leg (instead of the from) must work
/// the same way. The cascade walks `linked_doc_id`, which is only set on
/// the from-side row; the to-side row points back via the symmetric
/// `module_link.tfr-pair` row. If the cascade walks the wrong column the
/// to-side delete leaks the from-side row.
#[tokio::test]
async fn delete_to_leg_also_cascades() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    let (_from_txn, to_txn) = ep_finance::add_transfer_inner(
        &pool, "ACC-FROM", "ACC-TO", 250.0, None, 1_700_000_000,
    ).await.expect("transfer ok");

    let _ = ep_finance::delete_txn_inner(&pool, &to_txn.doc_id)
        .await
        .expect("delete to-leg ok");

    assert_eq!(count_txns(&pool).await.unwrap(), 0);
    assert_eq!(count_links_by_kind(&pool, "tfr-pair").await.unwrap(), 0);
    assert_eq!(fetch_balance(&pool, "ACC-FROM").await.unwrap(), Some(1000.0));
    assert_eq!(fetch_balance(&pool, "ACC-TO").await.unwrap(), Some(0.0));
}

// Transfer rows (tag='tfr') must not be counted by `tag='exp'` aggregates.

#[tokio::test]
async fn transfer_rows_do_not_pollute_expense_aggregates() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 10_000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();
    seed_category(&pool, "F&B", "Food").await.unwrap();

    // Real expense: ¥40 on F&B from ACC-FROM.
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query(
        "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code,
                              account_code, amount, tag, note, linked_doc_id)
         VALUES ('FIN-T-EXP', ?1, 'coffee', 'F&B', 'ACC-FROM', -40.0, 'exp', NULL, NULL)"
    ).bind(now).execute(&pool).await.unwrap();
    // Transfer ¥500 ACC-FROM → ACC-TO; from-leg is amount=-500 tag='tfr'.
    let _ = ep_finance::add_transfer_inner(
        &pool, "ACC-FROM", "ACC-TO", 500.0, None, now,
    ).await.expect("transfer ok");

    // 1) Month expense — must equal 40 (NOT 540).
    let month_expense: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn
          WHERE tag = 'exp'
            AND occurred_at >= unixepoch('now','localtime','start of month','utc')"
    ).fetch_one(&pool).await.unwrap();
    assert!((month_expense - 40.0).abs() < 1e-6,
        "month expense should be 40 (not 540 — transfer not counted), got {month_expense}");

    // 2) Category share — TFR must NOT appear; F&B must show 40.
    let cat_rows: Vec<(String, f64)> = sqlx::query_as(
        "SELECT category_code, SUM(-amount) FROM fin_txn
          WHERE tag = 'exp'
            AND occurred_at >= unixepoch('now','localtime','start of month','utc')
          GROUP BY category_code"
    ).fetch_all(&pool).await.unwrap();
    assert!(!cat_rows.iter().any(|(c, _)| c == "TFR"),
        "TFR must not appear in category share; rows = {cat_rows:?}");
    let fb = cat_rows.iter().find(|(c, _)| c == "F&B").map(|(_, v)| *v).unwrap_or(0.0);
    assert!((fb - 40.0).abs() < 1e-6, "F&B share should be 40, got {fb}");

    // 3) 90-day rolling — also 40, drives `avg_expense_3m` / emergency_months.
    let expense_90d: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn
          WHERE tag = 'exp'
            AND occurred_at >= unixepoch('now','localtime','-90 days','utc')"
    ).fetch_one(&pool).await.unwrap();
    assert!((expense_90d - 40.0).abs() < 1e-6,
        "90-day expense should be 40, got {expense_90d}");

    // 4) Week net — should be -40 (income 0 + true exp -40), NOT -540.
    let week_net: f64 = sqlx::query_scalar(
        "SELECT COALESCE(
            SUM(CASE WHEN tag = 'inc' AND amount > 0 THEN amount
                     WHEN tag = 'exp' AND amount < 0 THEN amount
                     ELSE 0.0 END), 0.0)
           FROM fin_txn
          WHERE occurred_at >= unixepoch('now','localtime','-7 days','utc')"
    ).fetch_one(&pool).await.unwrap();
    assert!((week_net + 40.0).abs() < 1e-6,
        "week net should be -40, got {week_net}");
}

// Single-leg tfr (kind='ref' link, not 'tfr-pair') must not cascade.

#[tokio::test]
async fn single_leg_tfr_delete_does_not_cascade_unrelated_doc() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-A", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-B", 500.0).await.unwrap();
    seed_category(&pool, "F&B", "Food").await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    // Real exp row on ACC-B that the single-leg tfr will reference.
    sqlx::query(
        "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code,
                              account_code, amount, tag, note, linked_doc_id)
         VALUES ('FIN-EXP-1', 1, 'pizza', 'F&B', 'ACC-B', -200.0, 'exp', NULL, NULL)"
    ).execute(&pool).await.unwrap();

    // Single-leg tfr (escape-hatch via add_txn shape): tag='tfr',
    // linked_doc_id points at FIN-EXP-1, kind='ref' link (NOT 'tfr-pair').
    sqlx::query(
        "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code,
                              account_code, amount, tag, note, linked_doc_id)
         VALUES ('FIN-TFR-X', 2, 'tfr leg', 'TFR', 'ACC-A', -100.0, 'tfr', NULL, 'FIN-EXP-1')"
    ).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO module_link (source_doc, target_doc, kind)
         VALUES ('FIN-TFR-X', 'FIN-EXP-1', 'ref')"
    ).execute(&pool).await.unwrap();

    let deleted = ep_finance::delete_txn_inner(&pool, "FIN-TFR-X")
        .await
        .expect("delete ok");
    assert!(deleted, "single-leg tfr was deleted");

    // The unrelated exp must survive.
    let exp_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fin_txn WHERE doc_id = 'FIN-EXP-1'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(exp_count, 1,
        "FIN-EXP-1 must NOT be cascade-deleted by single-leg-tfr removal");

    // The 'ref' module_link from FIN-TFR-X to FIN-EXP-1 should be cleaned
    // up as part of the deleted leg's own teardown (kind='ref' from
    // source side), but the target row stays intact.
    let ref_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM module_link
          WHERE source_doc = 'FIN-TFR-X' OR target_doc = 'FIN-TFR-X'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(ref_count, 0, "deleted leg's module_link rows should be gone");
}

// Orphan tfr-pair (partner row gone) must reject — we can't reverse the
// partner's balance from inside delete_txn_inner without amount/account_code.

#[tokio::test]
async fn delete_rejects_orphan_tfr_pair_link() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    let (from_txn, to_txn) = ep_finance::add_transfer_inner(
        &pool, "ACC-FROM", "ACC-TO", 100.0, None, 1_700_000_000,
    ).await.expect("transfer ok");
    // Post-transfer state: ACC-FROM=900, ACC-TO=100, both rows + 2 tfr-pair
    // links.

    // Out-of-band drift: manually drop only the to-leg row, leaving the
    // tfr-pair links pointing at it AND ACC-TO still carrying its +100.
    sqlx::query("DELETE FROM fin_txn WHERE doc_id = ?1")
        .bind(&to_txn.doc_id)
        .execute(&pool).await.unwrap();

    // delete_txn_inner must refuse and propagate the error so sqlx
    // rolls back the first-leg delete it had already started.
    let result = ep_finance::delete_txn_inner(&pool, &from_txn.doc_id).await;
    assert!(result.is_err(),
        "orphan partner must reject — committing would leave ACC-TO with a phantom balance");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("missing") && msg.contains(&to_txn.doc_id),
        "error should name the missing partner doc; got: {msg}");

    // Tx rolled back: from-leg + its tfr-pair links survive, ACC-FROM still
    // at 900 (transferred-out state), ACC-TO still at 100 (drift state).
    // The drift is preserved untouched, ready for manual repair.
    assert_eq!(count_txns(&pool).await.unwrap(), 1,
        "from-leg preserved by rollback");
    assert_eq!(count_links_by_kind(&pool, "tfr-pair").await.unwrap(), 2,
        "tfr-pair links preserved by rollback");
    assert_eq!(fetch_balance(&pool, "ACC-FROM").await.unwrap(), Some(900.0),
        "ACC-FROM stays at transferred-out balance (rollback)");
    assert_eq!(fetch_balance(&pool, "ACC-TO").await.unwrap(), Some(100.0),
        "ACC-TO drift unchanged — operator must repair before retry");
}

// Partner lookup walks both directions; one-direction drift still cascades.

#[tokio::test]
async fn delete_walks_both_tfr_pair_directions() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    let (from_txn, to_txn) = ep_finance::add_transfer_inner(
        &pool, "ACC-FROM", "ACC-TO", 100.0, None, 1_700_000_000,
    ).await.expect("transfer ok");

    // Drift: drop only the OUTGOING link from to-leg (the (to_doc→from_doc)
    // row), preserving the incoming (from_doc→to_doc) row. A naive lookup
    // keyed on `source_doc = to_doc` finds nothing and skips cascade.
    let dropped = sqlx::query(
        "DELETE FROM module_link
          WHERE source_doc = ?1 AND target_doc = ?2 AND kind = 'tfr-pair'"
    )
    .bind(&to_txn.doc_id)
    .bind(&from_txn.doc_id)
    .execute(&pool).await.unwrap();
    assert_eq!(dropped.rows_affected(), 1, "outgoing-from-to-leg link dropped");
    let surviving: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM module_link WHERE kind = 'tfr-pair'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(surviving, 1, "only the from→to direction remains");

    // Delete the to-leg. With the bidirectional UNION lookup, partner is
    // discovered via the incoming link and cascade fires correctly.
    let deleted = ep_finance::delete_txn_inner(&pool, &to_txn.doc_id)
        .await
        .expect("delete should still cascade despite one-direction drift");
    assert!(deleted);

    assert_eq!(count_txns(&pool).await.unwrap(), 0,
        "both legs deleted (cascade walked the surviving incoming link)");
    assert_eq!(count_links_by_kind(&pool, "tfr-pair").await.unwrap(), 0,
        "remaining tfr-pair link cleaned up by from-leg's delete_one_leg");
    assert_eq!(fetch_balance(&pool, "ACC-FROM").await.unwrap(), Some(1000.0),
        "ACC-FROM reverted via cascade");
    assert_eq!(fetch_balance(&pool, "ACC-TO").await.unwrap(), Some(0.0),
        "ACC-TO reverted by direct delete");
}

// >1 distinct partner = corrupt link table; reject rather than pick one.

#[tokio::test]
async fn delete_rejects_when_multiple_distinct_tfr_partners() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    let (from_txn, _to_txn) = ep_finance::add_transfer_inner(
        &pool, "ACC-FROM", "ACC-TO", 100.0, None, 1_700_000_000,
    ).await.expect("transfer ok");

    // Inject corruption: a stray tfr-pair link from a bogus doc into the
    // from-leg. Now the bidirectional UNION lookup returns BOTH `to_doc`
    // (legit partner via outgoing) and `BOGUS-DOC` (via incoming).
    sqlx::query(
        "INSERT INTO module_link (source_doc, target_doc, kind)
         VALUES ('BOGUS-DOC', ?1, 'tfr-pair')"
    ).bind(&from_txn.doc_id).execute(&pool).await.unwrap();

    let result = ep_finance::delete_txn_inner(&pool, &from_txn.doc_id).await;
    assert!(result.is_err(),
        "must reject when partner lookup returns multiple distinct candidates");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("distinct partners") && msg.contains("manual repair"),
        "error should call out corruption + manual repair; got: {msg}"
    );

    // Tx rolled back: from-leg, both real tfr-pair rows, and the corrupt
    // injected row all preserved as-is for operator inspection.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fin_txn WHERE doc_id = ?1"
    ).bind(&from_txn.doc_id).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1, "from-leg preserved by rollback");
    assert_eq!(fetch_balance(&pool, "ACC-FROM").await.unwrap(), Some(900.0),
        "ACC-FROM stays at transferred-out balance");
}
