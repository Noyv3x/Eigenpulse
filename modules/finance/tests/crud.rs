//! Integration tests for finance CRUD helpers; SSR-only (sqlx).
//! Each test owns a fresh in-memory pool — no shared state, no cleanup.

#![cfg(feature = "ssr")]

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn server_fn_error_text_en(e: &leptos::server_fn::ServerFnError) -> String {
    let Some((code, payload)) = ep_i18n::parse_err(e) else {
        return e.to_string();
    };
    match payload {
        Some(payload) => ep_i18n::tf(ep_i18n::Locale::En, code, &[("payload", payload)]),
        None => ep_i18n::t(ep_i18n::Locale::En, code).to_string(),
    }
}

#[derive(Debug)]
struct NoopNotifyBus;

#[async_trait::async_trait]
impl ep_core::NotifyBusTrait for NoopNotifyBus {
    async fn dispatch(&self, _msg: ep_core::NotifyMessage) -> anyhow::Result<i64> {
        Ok(0)
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ep_core::NotifyMessage> {
        let (_tx, rx) = tokio::sync::broadcast::channel(1);
        rx
    }
}

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

    // 2. Finance module migrations — applied via the same idempotent ledger
    //    code-path that production uses.
    ep_core::run_module_migrations(&pool, ep_finance::MODULE).await?;

    // 3. Ensure each test starts from a known-empty fixture. The baseline
    //    migration no longer ships demo data, so these deletes are defensive
    //    cleanup for tests that add their own rows. We do NOT touch
    //    `_ep_module_migration` — the ledger must keep saying every migration
    //    applied so a subsequent `run_module_migrations` call is a no-op.
    sqlx::query("DELETE FROM fin_txn").execute(&pool).await?;
    sqlx::query("DELETE FROM fin_budget").execute(&pool).await?;
    sqlx::query("DELETE FROM fin_account")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM fin_category")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM activity WHERE module = 'FIN'")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM seq WHERE module = 'FIN'")
        .execute(&pool)
        .await?;

    Ok(pool)
}

/// Insert one `fin_account` row at the given starting balance, returning
/// nothing — call-sites assert on `fetch_balance` post-state. `tone = ''`
/// mirrors the seed defaults.
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

    // These columns used to be added by a follow-up migration and now live in
    // the squashed baseline; if the baseline is incomplete, the SELECT fails
    // with "no such column".
    let row: (i64, i64) = sqlx::query_as(
        "SELECT \
            (SELECT COUNT(*) FROM pragma_table_info('fin_account') WHERE name = 'created_at'), \
            (SELECT COUNT(*) FROM pragma_table_info('fin_category') WHERE name = 'archived')",
    )
    .fetch_one(&pool)
    .await
    .expect("schema check");
    assert_eq!(row, (1, 1), "expected baseline CRUD columns to exist");

    // Migrations should be ledger'd as applied.
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _ep_module_migration WHERE module = 'FIN'")
            .fetch_one(&pool)
            .await
            .expect("ledger");
    assert_eq!(n, 1, "expected finance baseline migration to be ledgered");
}

/// Idempotency: running the migrations a second time should be a no-op,
/// not a duplicate-key error. Mirrors what the registry does on every
/// boot.
#[tokio::test]
async fn pool_helper_is_idempotent_on_double_apply() {
    let pool = make_test_pool().await.expect("first apply");
    ep_core::run_module_migrations(&pool, ep_finance::MODULE)
        .await
        .expect("second apply must be no-op");
    // Still exactly one ledger row.
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _ep_module_migration WHERE module = 'FIN'")
            .fetch_one(&pool)
            .await
            .expect("ledger");
    assert_eq!(n, 1);
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

#[tokio::test]
async fn post_txn_api_persists_linked_doc_id_everywhere() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-T", 100.0)
        .await
        .expect("seed account");
    seed_category(&pool, "EXP", "Expense")
        .await
        .expect("seed category");
    let state = ep_core::AppState {
        db: pool.clone(),
        cookie_key: cookie::Key::generate(),
        notify: Arc::new(NoopNotifyBus),
        leptos_options: Default::default(),
    };
    let app = ep_finance::open_api(state.clone())
        .layer(axum::Extension(ep_auth::AuthPat {
            id: 1,
            name: "test".into(),
            scopes: vec!["fin:write".into()],
        }))
        .with_state(state);

    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/txn")
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "merchant": "Linked expense",
                "category_code": "EXP",
                "account_code": "ACC-T",
                "amount": -42.0,
                "tag": "exp",
                "note": "handler path",
                "linked_doc_id": " FIT-26001 ",
                "occurred_at": 1_700_000_000_i64
            })
            .to_string(),
        ))
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 16 * 1024)
        .await
        .expect("body");
    let created: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let doc_id = created
        .get("doc_id")
        .and_then(|v| v.as_str())
        .expect("doc_id");

    let linked: Option<String> =
        sqlx::query_scalar("SELECT linked_doc_id FROM fin_txn WHERE doc_id = ?1")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .expect("fin_txn linked_doc_id");
    assert_eq!(linked.as_deref(), Some("FIT-26001"));

    let activity_link: Option<String> =
        sqlx::query_scalar("SELECT link_doc FROM activity WHERE module = 'FIN' AND doc_id = ?1")
            .bind(doc_id)
            .fetch_one(&pool)
            .await
            .expect("activity link_doc");
    assert_eq!(activity_link.as_deref(), Some("FIT-26001"));

    let link_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM module_link
          WHERE source_doc = ?1 AND target_doc = 'FIT-26001' AND kind = 'ref'",
    )
    .bind(doc_id)
    .fetch_one(&pool)
    .await
    .expect("module_link ref");
    assert_eq!(link_count, 1);
}

#[tokio::test]
async fn post_txn_api_rejects_bad_sign_with_i18n_error_json() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-T", 100.0)
        .await
        .expect("seed account");
    seed_category(&pool, "EXP", "Expense")
        .await
        .expect("seed category");
    let state = ep_core::AppState {
        db: pool.clone(),
        cookie_key: cookie::Key::generate(),
        notify: Arc::new(NoopNotifyBus),
        leptos_options: Default::default(),
    };
    let app = ep_finance::open_api(state.clone())
        .layer(axum::Extension(ep_auth::AuthPat {
            id: 1,
            name: "test".into(),
            scopes: vec!["fin:write".into()],
        }))
        .with_state(state);

    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/txn")
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(
            serde_json::json!({
                "merchant": "Wrong sign",
                "category_code": "EXP",
                "account_code": "ACC-T",
                "amount": 42.0,
                "tag": "exp"
            })
            .to_string(),
        ))
        .expect("request");
    let resp = app.oneshot(req).await.expect("response");
    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), 16 * 1024)
        .await
        .expect("body");
    let err: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        err.pointer("/error/code").and_then(|v| v.as_str()),
        Some("finance.err.amount_sign_invalid")
    );
    assert_eq!(count_txns(&pool).await.unwrap(), 0);
    assert_eq!(fetch_balance(&pool, "ACC-T").await.unwrap(), Some(100.0));
}

#[tokio::test]
async fn add_txn_inner_trims_and_persists_side_effects() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-T", 100.0)
        .await
        .expect("seed account");
    seed_category(&pool, "EXP", "Expense")
        .await
        .expect("seed category");

    let txn = ep_finance::add_txn_inner(
        &pool,
        ep_finance::AddTxnFields {
            merchant: "  Coffee  ".into(),
            category_code: " EXP ".into(),
            account_code: " ACC-T ".into(),
            amount: -12.5,
            tag: " exp ".into(),
            note: Some("  morning  ".into()),
            linked_doc_id: Some(" FIT-S-0001 ".into()),
            occurred_at: 1_700_000_000,
        },
    )
    .await
    .expect("add txn");

    assert_eq!(txn.merchant, "Coffee");
    assert_eq!(txn.category_code, "EXP");
    assert_eq!(txn.account_code, "ACC-T");
    assert_eq!(txn.note.as_deref(), Some("morning"));
    assert_eq!(txn.linked_doc_id.as_deref(), Some("FIT-S-0001"));
    assert_eq!(fetch_balance(&pool, "ACC-T").await.unwrap(), Some(87.5));

    let activity_link: Option<String> =
        sqlx::query_scalar("SELECT link_doc FROM activity WHERE module = 'FIN' AND doc_id = ?1")
            .bind(&txn.doc_id)
            .fetch_one(&pool)
            .await
            .expect("activity link_doc");
    assert_eq!(activity_link.as_deref(), Some("FIT-S-0001"));
}

#[tokio::test]
async fn add_txn_inner_rejects_invalid_contract_before_insert() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-T", 100.0)
        .await
        .expect("seed account");
    seed_category(&pool, "EXP", "Expense")
        .await
        .expect("seed category");

    let err = ep_finance::add_txn_inner(
        &pool,
        ep_finance::AddTxnFields {
            merchant: "Coffee".into(),
            category_code: "EXP".into(),
            account_code: "ACC-T".into(),
            amount: 12.5,
            tag: "exp".into(),
            note: None,
            linked_doc_id: None,
            occurred_at: 1_700_000_000,
        },
    )
    .await
    .expect_err("bad sign");
    assert_eq!(
        ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
        Some(("finance.err.amount_sign_invalid", ""))
    );

    let err = ep_finance::add_txn_inner(
        &pool,
        ep_finance::AddTxnFields {
            merchant: "Move money".into(),
            category_code: "EXP".into(),
            account_code: "ACC-T".into(),
            amount: -12.5,
            tag: "tfr".into(),
            note: None,
            linked_doc_id: None,
            occurred_at: 1_700_000_000,
        },
    )
    .await
    .expect_err("single-leg transfer");
    assert_eq!(
        ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
        Some(("finance.err.tfr_requires_transfer", ""))
    );

    let err = ep_finance::add_txn_inner(
        &pool,
        ep_finance::AddTxnFields {
            merchant: "Coffee".into(),
            category_code: "EXP".into(),
            account_code: "ACC-T".into(),
            amount: -12.5,
            tag: "exp".into(),
            note: None,
            linked_doc_id: Some("../FIT-S-0001".into()),
            occurred_at: 1_700_000_000,
        },
    )
    .await
    .expect_err("invalid linked doc id");
    assert_eq!(
        ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
        Some(("finance.err.linked_doc_id_invalid", "../FIT-S-0001"))
    );

    assert_eq!(count_txns(&pool).await.unwrap(), 0);
    assert_eq!(fetch_balance(&pool, "ACC-T").await.unwrap(), Some(100.0));
}

#[tokio::test]
async fn delete_unused_account_removes_row() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-DEL", 12.0).await.unwrap();

    ep_finance::delete_account_inner(&pool, "ACC-DEL".into())
        .await
        .expect("delete unused account");

    assert_eq!(fetch_balance(&pool, "ACC-DEL").await.unwrap(), None);
}

#[tokio::test]
async fn finance_open_api_trims_account_path_code_before_patch_and_response() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-T", 12.0).await.unwrap();
    let state = ep_core::AppState {
        db: pool.clone(),
        cookie_key: cookie::Key::generate(),
        notify: Arc::new(NoopNotifyBus),
        leptos_options: Default::default(),
    };
    let app = ep_finance::open_api(state.clone())
        .layer(axum::Extension(ep_auth::AuthPat {
            id: 1,
            name: "writer".into(),
            scopes: vec!["fin:write".into()],
        }))
        .with_state(state);

    let patch = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PATCH")
                .uri("/account/%20ACC-T%20")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({"name": "Trimmed"}).to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("patch response");
    assert_eq!(patch.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(patch.into_body(), 16 * 1024)
        .await
        .expect("body");
    let account: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        account.pointer("/code").and_then(|v| v.as_str()),
        Some("ACC-T")
    );
    assert_eq!(
        account.pointer("/name").and_then(|v| v.as_str()),
        Some("Trimmed")
    );

    let delete = app
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/account/%20ACC-T%20")
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("delete response");
    assert_eq!(delete.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(delete.into_body(), 16 * 1024)
        .await
        .expect("body");
    let deleted: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        deleted.pointer("/code").and_then(|v| v.as_str()),
        Some("ACC-T")
    );
    assert_eq!(fetch_balance(&pool, "ACC-T").await.unwrap(), None);
}

#[tokio::test]
async fn delete_account_rejects_rows_with_transactions() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-BUSY", 0.0).await.unwrap();
    seed_category(&pool, "EXP", "Expense").await.unwrap();
    seed_txn_directly(&pool, "FIN-DEL-A", "ACC-BUSY", "EXP", -8.0, "exp")
        .await
        .unwrap();

    let res = ep_finance::delete_account_inner(&pool, "ACC-BUSY".into()).await;
    assert!(res.is_err(), "account with txns must not be deleted");
    assert_eq!(fetch_balance(&pool, "ACC-BUSY").await.unwrap(), Some(-8.0));
}

#[tokio::test]
async fn delete_txn_inner_reports_missing_doc() {
    let pool = make_test_pool().await.expect("pool");

    let deleted = ep_finance::delete_txn_inner(&pool, "FIN-MISSING")
        .await
        .expect("missing delete should not error");

    assert!(!deleted);
}

#[tokio::test]
async fn delete_txn_inner_clears_external_references_to_fin_doc() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-T", 100.0).await.unwrap();
    seed_category(&pool, "EXP", "Expense").await.unwrap();
    seed_txn_directly(&pool, "FIN-REF-TARGET", "ACC-T", "EXP", -20.0, "exp")
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO module_link (source_doc, target_doc, kind)
         VALUES ('LRN-N-0001', 'FIN-REF-TARGET', 'ref')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, link_doc)
         VALUES (1_700_000_000, 'LRN', 'LRN-N-0001', 'linked note', 'FIN-REF-TARGET')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let deleted = ep_finance::delete_txn_inner(&pool, "FIN-REF-TARGET")
        .await
        .expect("delete txn");

    assert!(deleted);
    let link_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM module_link
          WHERE source_doc = 'FIN-REF-TARGET'
             OR target_doc = 'FIN-REF-TARGET'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(link_count, 0);

    let link_doc: Option<String> =
        sqlx::query_scalar("SELECT link_doc FROM activity WHERE doc_id = 'LRN-N-0001'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(link_doc, None);
}

#[tokio::test]
async fn delete_unused_category_removes_budgets_too() {
    let pool = make_test_pool().await.expect("pool");
    seed_category(&pool, "CAT", "Category").await.unwrap();
    ep_finance::set_budget_inner(&pool, "2026-05", "CAT", 500.0)
        .await
        .expect("budget");

    ep_finance::delete_category_inner(&pool, "CAT".into())
        .await
        .expect("delete unused category");

    let cat_exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = 'CAT')")
            .fetch_one(&pool)
            .await
            .unwrap();
    let budget_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fin_budget WHERE category_code = 'CAT'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cat_exists, 0);
    assert_eq!(budget_count, 0);
}

#[tokio::test]
async fn finance_open_api_trims_category_path_code_before_patch_and_response() {
    let pool = make_test_pool().await.expect("pool");
    seed_category(&pool, "EXP", "Expense").await.unwrap();
    let state = ep_core::AppState {
        db: pool.clone(),
        cookie_key: cookie::Key::generate(),
        notify: Arc::new(NoopNotifyBus),
        leptos_options: Default::default(),
    };
    let app = ep_finance::open_api(state.clone())
        .layer(axum::Extension(ep_auth::AuthPat {
            id: 1,
            name: "writer".into(),
            scopes: vec!["fin:write".into()],
        }))
        .with_state(state);

    let patch = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("PATCH")
                .uri("/category/%20EXP%20")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({"name": "Groceries"}).to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("patch response");
    assert_eq!(patch.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(patch.into_body(), 16 * 1024)
        .await
        .expect("body");
    let category: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        category.pointer("/code").and_then(|v| v.as_str()),
        Some("EXP")
    );
    assert_eq!(
        category.pointer("/name").and_then(|v| v.as_str()),
        Some("Groceries")
    );

    let delete = app
        .oneshot(
            axum::http::Request::builder()
                .method("DELETE")
                .uri("/category/%20EXP%20")
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("delete response");
    assert_eq!(delete.status(), axum::http::StatusCode::OK);
    let body = axum::body::to_bytes(delete.into_body(), 16 * 1024)
        .await
        .expect("body");
    let deleted: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        deleted.pointer("/code").and_then(|v| v.as_str()),
        Some("EXP")
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_category WHERE code = 'EXP'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn finance_open_api_rejects_negative_category_sort_order() {
    let pool = make_test_pool().await.expect("pool");
    let state = ep_core::AppState {
        db: pool.clone(),
        cookie_key: cookie::Key::generate(),
        notify: Arc::new(NoopNotifyBus),
        leptos_options: Default::default(),
    };
    let app = ep_finance::open_api(state.clone())
        .layer(axum::Extension(ep_auth::AuthPat {
            id: 1,
            name: "writer".into(),
            scopes: vec!["fin:write".into()],
        }))
        .with_state(state);

    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/category")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({
                        "code": "NEG",
                        "name": "Negative",
                        "tone": "rose",
                        "sort_order": -1
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), 16 * 1024)
        .await
        .expect("body");
    let err: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        err.pointer("/error/code").and_then(|v| v.as_str()),
        Some("finance.err.category_sort_order_invalid")
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_category WHERE code = 'NEG'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn set_budget_rejects_unknown_category_before_insert() {
    let pool = make_test_pool().await.expect("pool");

    let err = ep_finance::set_budget_inner(&pool, "2026-05", "MISSING", 500.0)
        .await
        .expect_err("unknown category should be a domain error");

    assert_eq!(
        ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
        Some(("finance.err.category_not_found", "MISSING"))
    );
}

#[tokio::test]
async fn delete_category_rejects_rows_with_transactions() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-CAT", 0.0).await.unwrap();
    seed_category(&pool, "CAT", "Category").await.unwrap();
    seed_txn_directly(&pool, "FIN-DEL-C", "ACC-CAT", "CAT", -8.0, "exp")
        .await
        .unwrap();

    let res = ep_finance::delete_category_inner(&pool, "CAT".into()).await;
    assert!(res.is_err(), "category with txns must not be deleted");
    let cat_exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fin_category WHERE code = 'CAT')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cat_exists, 1);
}

// parse_occurred_at three-state: empty / valid / malformed.

#[tokio::test]
async fn parse_occurred_at_empty_returns_none() {
    let pool = make_test_pool().await.expect("pool");
    let got = ep_finance::parse_occurred_at(&pool, "")
        .await
        .expect("empty ok");
    assert_eq!(
        got, None,
        "empty → Ok(None) so caller picks 'now' or 'keep'"
    );
    let got2 = ep_finance::parse_occurred_at(&pool, "   ")
        .await
        .expect("ws ok");
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
async fn finance_open_api_enforces_read_and_write_scopes() {
    let pool = make_test_pool().await.expect("pool");
    let state = ep_core::AppState {
        db: pool.clone(),
        cookie_key: cookie::Key::generate(),
        notify: Arc::new(NoopNotifyBus),
        leptos_options: Default::default(),
    };

    let read_with_write_only = ep_finance::open_api(state.clone())
        .layer(axum::Extension(ep_auth::AuthPat {
            id: 1,
            name: "writer".into(),
            scopes: vec!["fin:write".into()],
        }))
        .with_state(state.clone());
    let resp = read_with_write_only
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/txn")
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);

    let write_with_read_only = ep_finance::open_api(state.clone())
        .layer(axum::Extension(ep_auth::AuthPat {
            id: 2,
            name: "reader".into(),
            scopes: vec!["fin:read".into()],
        }))
        .with_state(state);
    let resp = write_with_read_only
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/txn")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({
                        "merchant": "Blocked",
                        "category_code": "EXP",
                        "account_code": "ACC-T",
                        "amount": -1.0,
                        "tag": "exp"
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);
    assert_eq!(count_txns(&pool).await.unwrap(), 0);
}

#[tokio::test]
async fn finance_open_api_returns_json_error_for_malformed_json_body() {
    let pool = make_test_pool().await.expect("pool");
    let state = ep_core::AppState {
        db: pool,
        cookie_key: cookie::Key::generate(),
        notify: Arc::new(NoopNotifyBus),
        leptos_options: Default::default(),
    };
    let app = ep_finance::open_api(state.clone())
        .layer(axum::Extension(ep_auth::AuthPat {
            id: 1,
            name: "writer".into(),
            scopes: vec!["fin:write".into()],
        }))
        .with_state(state);

    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/txn")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from("{"))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.starts_with("application/json")),
        Some(true)
    );
    let body = axum::body::to_bytes(resp.into_body(), 16 * 1024)
        .await
        .expect("body");
    let err: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        err.pointer("/error/code").and_then(|v| v.as_str()),
        Some("bad_request")
    );
    assert!(
        err.pointer("/error/message")
            .and_then(|v| v.as_str())
            .is_some(),
        "missing error message: {err}"
    );
}

#[tokio::test]
async fn finance_open_api_returns_json_error_for_bad_query_params() {
    let pool = make_test_pool().await.expect("pool");
    let state = ep_core::AppState {
        db: pool,
        cookie_key: cookie::Key::generate(),
        notify: Arc::new(NoopNotifyBus),
        leptos_options: Default::default(),
    };
    let app = ep_finance::open_api(state.clone())
        .layer(axum::Extension(ep_auth::AuthPat {
            id: 1,
            name: "reader".into(),
            scopes: vec!["fin:read".into()],
        }))
        .with_state(state);

    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/budget")
                .body(axum::body::Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(resp.status(), axum::http::StatusCode::BAD_REQUEST);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.starts_with("application/json")),
        Some(true)
    );
    let body = axum::body::to_bytes(resp.into_body(), 16 * 1024)
        .await
        .expect("body");
    let err: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        err.pointer("/error/code").and_then(|v| v.as_str()),
        Some("bad_request")
    );
}

#[tokio::test]
async fn finance_open_api_budget_unknown_category_is_domain_error() {
    let pool = make_test_pool().await.expect("pool");
    let state = ep_core::AppState {
        db: pool,
        cookie_key: cookie::Key::generate(),
        notify: Arc::new(NoopNotifyBus),
        leptos_options: Default::default(),
    };
    let app = ep_finance::open_api(state.clone())
        .layer(axum::Extension(ep_auth::AuthPat {
            id: 1,
            name: "writer".into(),
            scopes: vec!["fin:write".into()],
        }))
        .with_state(state);

    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/budget")
                .header(axum::http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(
                    serde_json::json!({
                        "period": "2026-05",
                        "category_code": "MISSING",
                        "amount": 500.0
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(resp.status(), axum::http::StatusCode::NOT_FOUND);
    let body = axum::body::to_bytes(resp.into_body(), 16 * 1024)
        .await
        .expect("body");
    let err: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        err.pointer("/error/code").and_then(|v| v.as_str()),
        Some("finance.err.category_not_found")
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

#[tokio::test]
async fn update_txn_trims_identifiers_and_optional_note() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-T", 0.0).await.unwrap();
    seed_category(&pool, "EXP", "Expense").await.unwrap();
    seed_txn_directly(&pool, "FIN-99003", "ACC-T", "EXP", -100.0, "exp")
        .await
        .unwrap();

    let updated = ep_finance::update_txn_inner(
        &pool,
        "FIN-99003",
        ep_finance::UpdateTxnFields {
            merchant: "  trimmed merchant  ".into(),
            category_code: " EXP ".into(),
            account_code: " ACC-T ".into(),
            amount: 25.0,
            note: Some("  reviewed  ".into()),
            occurred_at_input: String::new(),
        },
    )
    .await
    .expect("trimmed update ok");

    assert_eq!(updated.merchant, "trimmed merchant");
    assert_eq!(updated.category_code, "EXP");
    assert_eq!(updated.account_code, "ACC-T");
    assert_eq!(updated.note.as_deref(), Some("reviewed"));
    assert_eq!(updated.amount, -25.0);

    let row: (String, String, String, Option<String>) = sqlx::query_as(
        "SELECT merchant, category_code, account_code, note FROM fin_txn WHERE doc_id = 'FIN-99003'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            "trimmed merchant".into(),
            "EXP".into(),
            "ACC-T".into(),
            Some("reviewed".into())
        )
    );
    assert_eq!(fetch_balance(&pool, "ACC-T").await.unwrap(), Some(-25.0));
}

#[tokio::test]
async fn update_txn_clears_existing_cross_module_reference() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-T", 0.0).await.unwrap();
    seed_category(&pool, "EXP", "Expense").await.unwrap();
    seed_txn_directly(&pool, "FIN-99004", "ACC-T", "EXP", -100.0, "exp")
        .await
        .unwrap();
    sqlx::query("UPDATE fin_txn SET linked_doc_id = 'FIT-S-0001' WHERE doc_id = 'FIN-99004'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, amount, link_doc)
         VALUES (1700000000, 'FIN', 'FIN-99004', 'seed', -100.0, 'FIT-S-0001')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO module_link (source_doc, target_doc, kind)
         VALUES ('FIN-99004', 'FIT-S-0001', 'ref')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let updated = ep_finance::update_txn_inner(
        &pool,
        "FIN-99004",
        ep_finance::UpdateTxnFields {
            merchant: "edited".into(),
            category_code: "EXP".into(),
            account_code: "ACC-T".into(),
            amount: 80.0,
            note: None,
            occurred_at_input: String::new(),
        },
    )
    .await
    .expect("update ok");

    assert_eq!(updated.linked_doc_id, None);
    let txn_link: Option<String> =
        sqlx::query_scalar("SELECT linked_doc_id FROM fin_txn WHERE doc_id = 'FIN-99004'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(txn_link, None);
    let activity_link: Option<String> =
        sqlx::query_scalar("SELECT link_doc FROM activity WHERE doc_id = 'FIN-99004'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(activity_link, None);
    let ref_links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM module_link WHERE source_doc = 'FIN-99004'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(ref_links, 0);
}

// add_transfer pair + delete cascade.

#[tokio::test]
async fn add_transfer_creates_pair_and_delete_cascades() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    let occurred_at = 1_700_000_000_i64;
    let (from_txn, to_txn) =
        ep_finance::add_transfer_inner(&pool, "ACC-FROM", "ACC-TO", 300.0, None, occurred_at)
            .await
            .expect("transfer ok");

    // Two fin_txn rows + two symmetric `tfr-pair` link rows, balances move.
    assert_eq!(
        count_txns(&pool).await.unwrap(),
        2,
        "transfer creates 2 fin_txn rows"
    );
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
    assert_eq!(from_txn.note, None);

    // Delete the *from* leg → cascade to the *to* leg.
    let deleted = ep_finance::delete_txn_inner(&pool, &from_txn.doc_id)
        .await
        .expect("cascade ok");
    assert!(
        deleted,
        "delete_txn_inner returned true for an existing row"
    );

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

#[tokio::test]
async fn add_transfer_trims_optional_note() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    let (from_txn, _to_txn) = ep_finance::add_transfer_inner(
        &pool,
        "ACC-FROM",
        "ACC-TO",
        25.0,
        Some("  monthly sweep  "),
        1_700_000_000,
    )
    .await
    .expect("transfer ok");

    assert_eq!(from_txn.note.as_deref(), Some("monthly sweep"));

    let note: Option<String> = sqlx::query_scalar("SELECT note FROM fin_txn WHERE doc_id = ?1")
        .bind(&from_txn.doc_id)
        .fetch_one(&pool)
        .await
        .expect("note");
    assert_eq!(note.as_deref(), Some("monthly sweep"));

    let (blank_from, _blank_to) = ep_finance::add_transfer_inner(
        &pool,
        "ACC-FROM",
        "ACC-TO",
        10.0,
        Some("   "),
        1_700_000_001,
    )
    .await
    .expect("blank-note transfer ok");
    assert_eq!(blank_from.note, None);
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

    let (_from_txn, to_txn) =
        ep_finance::add_transfer_inner(&pool, "ACC-FROM", "ACC-TO", 250.0, None, 1_700_000_000)
            .await
            .expect("transfer ok");

    let _ = ep_finance::delete_txn_inner(&pool, &to_txn.doc_id)
        .await
        .expect("delete to-leg ok");

    assert_eq!(count_txns(&pool).await.unwrap(), 0);
    assert_eq!(count_links_by_kind(&pool, "tfr-pair").await.unwrap(), 0);
    assert_eq!(
        fetch_balance(&pool, "ACC-FROM").await.unwrap(),
        Some(1000.0)
    );
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
    let now = ep_core::unix_now();
    sqlx::query(
        "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code,
                              account_code, amount, tag, note, linked_doc_id)
         VALUES ('FIN-T-EXP', ?1, 'coffee', 'F&B', 'ACC-FROM', -40.0, 'exp', NULL, NULL)",
    )
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    // Transfer ¥500 ACC-FROM → ACC-TO; from-leg is amount=-500 tag='tfr'.
    let _ = ep_finance::add_transfer_inner(&pool, "ACC-FROM", "ACC-TO", 500.0, None, now)
        .await
        .expect("transfer ok");

    // 1) Month expense — must equal 40 (NOT 540).
    let month_expense: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn
          WHERE tag = 'exp'
            AND occurred_at >= unixepoch('now','localtime','start of month','utc')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        (month_expense - 40.0).abs() < 1e-6,
        "month expense should be 40 (not 540 — transfer not counted), got {month_expense}"
    );

    // 2) Category share — TFR must NOT appear; F&B must show 40.
    let cat_rows: Vec<(String, f64)> = sqlx::query_as(
        "SELECT category_code, SUM(-amount) FROM fin_txn
          WHERE tag = 'exp'
            AND occurred_at >= unixepoch('now','localtime','start of month','utc')
          GROUP BY category_code",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        !cat_rows.iter().any(|(c, _)| c == "TFR"),
        "TFR must not appear in category share; rows = {cat_rows:?}"
    );
    let fb = cat_rows
        .iter()
        .find(|(c, _)| c == "F&B")
        .map(|(_, v)| *v)
        .unwrap_or(0.0);
    assert!((fb - 40.0).abs() < 1e-6, "F&B share should be 40, got {fb}");

    // 3) 90-day rolling — also 40, drives `avg_expense_3m` / emergency_months.
    let expense_90d: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn
          WHERE tag = 'exp'
            AND occurred_at >= unixepoch('now','localtime','-90 days','utc')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        (expense_90d - 40.0).abs() < 1e-6,
        "90-day expense should be 40, got {expense_90d}"
    );

    // 4) Week net — should be -40 (income 0 + true exp -40), NOT -540.
    let week_net: f64 = sqlx::query_scalar(
        "SELECT COALESCE(
            SUM(CASE WHEN tag = 'inc' AND amount > 0 THEN amount
                     WHEN tag = 'exp' AND amount < 0 THEN amount
                     ELSE 0.0 END), 0.0)
           FROM fin_txn
          WHERE occurred_at >= unixepoch('now','localtime','-7 days','utc')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        (week_net + 40.0).abs() < 1e-6,
        "week net should be -40, got {week_net}"
    );

    // 5) Shared 12-month trend helper — reports and finance page both use
    // this path, so it must keep the same transfer-exclusion invariant.
    let months = ep_finance::load_month_buckets_12(&pool)
        .await
        .expect("month buckets");
    assert_eq!(months.len(), 12);
    let current_period: String = sqlx::query_scalar("SELECT strftime('%Y-%m','now','localtime')")
        .fetch_one(&pool)
        .await
        .unwrap();
    let current = months
        .iter()
        .find(|m| m.period == current_period)
        .expect("current month bucket");
    assert!(
        (current.expense - 40.0).abs() < 1e-6,
        "month bucket expense should be 40, got {} in {current:?}",
        current.expense
    );
    assert!(
        (current.net + 40.0).abs() < 1e-6,
        "month bucket net should be -40, got {} in {current:?}",
        current.net
    );
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
         VALUES ('FIN-EXP-1', 1, 'pizza', 'F&B', 'ACC-B', -200.0, 'exp', NULL, NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Legacy/drifted single-leg tfr: tag='tfr', linked_doc_id points at
    // FIN-EXP-1, kind='ref' link (NOT 'tfr-pair'). The public write paths now
    // reject this shape, but delete must still clean it up without cascading.
    sqlx::query(
        "INSERT INTO fin_txn (doc_id, occurred_at, merchant, category_code,
                              account_code, amount, tag, note, linked_doc_id)
         VALUES ('FIN-TFR-X', 2, 'tfr leg', 'TFR', 'ACC-A', -100.0, 'tfr', NULL, 'FIN-EXP-1')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO module_link (source_doc, target_doc, kind)
         VALUES ('FIN-TFR-X', 'FIN-EXP-1', 'ref')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let deleted = ep_finance::delete_txn_inner(&pool, "FIN-TFR-X")
        .await
        .expect("delete ok");
    assert!(deleted, "single-leg tfr was deleted");

    // The unrelated exp must survive.
    let exp_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fin_txn WHERE doc_id = 'FIN-EXP-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        exp_count, 1,
        "FIN-EXP-1 must NOT be cascade-deleted by single-leg-tfr removal"
    );

    // The 'ref' module_link from FIN-TFR-X to FIN-EXP-1 should be cleaned
    // up as part of the deleted leg's own teardown (kind='ref' from
    // source side), but the target row stays intact.
    let ref_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM module_link
          WHERE source_doc = 'FIN-TFR-X' OR target_doc = 'FIN-TFR-X'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ref_count, 0,
        "deleted leg's module_link rows should be gone"
    );
}

// Orphan tfr-pair (partner row gone) must reject — we can't reverse the
// partner's balance from inside delete_txn_inner without amount/account_code.

#[tokio::test]
async fn delete_rejects_orphan_tfr_pair_link() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    let (from_txn, to_txn) =
        ep_finance::add_transfer_inner(&pool, "ACC-FROM", "ACC-TO", 100.0, None, 1_700_000_000)
            .await
            .expect("transfer ok");
    // Post-transfer state: ACC-FROM=900, ACC-TO=100, both rows + 2 tfr-pair
    // links.

    // Out-of-band drift: manually drop only the to-leg row, leaving the
    // tfr-pair links pointing at it AND ACC-TO still carrying its +100.
    sqlx::query("DELETE FROM fin_txn WHERE doc_id = ?1")
        .bind(&to_txn.doc_id)
        .execute(&pool)
        .await
        .unwrap();

    // delete_txn_inner must refuse and propagate the error so sqlx
    // rolls back the first-leg delete it had already started.
    let result = ep_finance::delete_txn_inner(&pool, &from_txn.doc_id).await;
    assert!(
        result.is_err(),
        "orphan partner must reject — committing would leave ACC-TO with a phantom balance"
    );
    let err = result.unwrap_err();
    let msg = server_fn_error_text_en(&err);
    assert!(
        msg.contains("missing") && msg.contains(&to_txn.doc_id),
        "error should name the missing partner doc; got: {msg}"
    );

    // Tx rolled back: from-leg + its tfr-pair links survive, ACC-FROM still
    // at 900 (transferred-out state), ACC-TO still at 100 (drift state).
    // The drift is preserved untouched, ready for manual repair.
    assert_eq!(
        count_txns(&pool).await.unwrap(),
        1,
        "from-leg preserved by rollback"
    );
    assert_eq!(
        count_links_by_kind(&pool, "tfr-pair").await.unwrap(),
        2,
        "tfr-pair links preserved by rollback"
    );
    assert_eq!(
        fetch_balance(&pool, "ACC-FROM").await.unwrap(),
        Some(900.0),
        "ACC-FROM stays at transferred-out balance (rollback)"
    );
    assert_eq!(
        fetch_balance(&pool, "ACC-TO").await.unwrap(),
        Some(100.0),
        "ACC-TO drift unchanged — operator must repair before retry"
    );
}

// Partner lookup walks both directions; one-direction drift still cascades.

#[tokio::test]
async fn delete_walks_both_tfr_pair_directions() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    let (from_txn, to_txn) =
        ep_finance::add_transfer_inner(&pool, "ACC-FROM", "ACC-TO", 100.0, None, 1_700_000_000)
            .await
            .expect("transfer ok");

    // Drift: drop only the OUTGOING link from to-leg (the (to_doc→from_doc)
    // row), preserving the incoming (from_doc→to_doc) row. A naive lookup
    // keyed on `source_doc = to_doc` finds nothing and skips cascade.
    let dropped = sqlx::query(
        "DELETE FROM module_link
          WHERE source_doc = ?1 AND target_doc = ?2 AND kind = 'tfr-pair'",
    )
    .bind(&to_txn.doc_id)
    .bind(&from_txn.doc_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        dropped.rows_affected(),
        1,
        "outgoing-from-to-leg link dropped"
    );
    let surviving: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM module_link WHERE kind = 'tfr-pair'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(surviving, 1, "only the from→to direction remains");

    // Delete the to-leg. With the bidirectional UNION lookup, partner is
    // discovered via the incoming link and cascade fires correctly.
    let deleted = ep_finance::delete_txn_inner(&pool, &to_txn.doc_id)
        .await
        .expect("delete should still cascade despite one-direction drift");
    assert!(deleted);

    assert_eq!(
        count_txns(&pool).await.unwrap(),
        0,
        "both legs deleted (cascade walked the surviving incoming link)"
    );
    assert_eq!(
        count_links_by_kind(&pool, "tfr-pair").await.unwrap(),
        0,
        "remaining tfr-pair link cleaned up by from-leg's delete_one_leg"
    );
    assert_eq!(
        fetch_balance(&pool, "ACC-FROM").await.unwrap(),
        Some(1000.0),
        "ACC-FROM reverted via cascade"
    );
    assert_eq!(
        fetch_balance(&pool, "ACC-TO").await.unwrap(),
        Some(0.0),
        "ACC-TO reverted by direct delete"
    );
}

// >1 distinct partner = corrupt link table; reject rather than pick one.

#[tokio::test]
async fn delete_rejects_when_multiple_distinct_tfr_partners() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-FROM", 1000.0).await.unwrap();
    seed_account(&pool, "ACC-TO", 0.0).await.unwrap();
    seed_category(&pool, "TFR", "Transfer").await.unwrap();

    let (from_txn, _to_txn) =
        ep_finance::add_transfer_inner(&pool, "ACC-FROM", "ACC-TO", 100.0, None, 1_700_000_000)
            .await
            .expect("transfer ok");

    // Inject corruption: a stray tfr-pair link from a bogus doc into the
    // from-leg. Now the bidirectional UNION lookup returns BOTH `to_doc`
    // (legit partner via outgoing) and `BOGUS-DOC` (via incoming).
    sqlx::query(
        "INSERT INTO module_link (source_doc, target_doc, kind)
         VALUES ('BOGUS-DOC', ?1, 'tfr-pair')",
    )
    .bind(&from_txn.doc_id)
    .execute(&pool)
    .await
    .unwrap();

    let result = ep_finance::delete_txn_inner(&pool, &from_txn.doc_id).await;
    assert!(
        result.is_err(),
        "must reject when partner lookup returns multiple distinct candidates"
    );
    let err = result.unwrap_err();
    let msg = server_fn_error_text_en(&err);
    assert!(
        msg.contains("distinct partners") && msg.contains("manual repair"),
        "error should call out corruption + manual repair; got: {msg}"
    );

    // Tx rolled back: from-leg, both real tfr-pair rows, and the corrupt
    // injected row all preserved as-is for operator inspection.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_txn WHERE doc_id = ?1")
        .bind(&from_txn.doc_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "from-leg preserved by rollback");
    assert_eq!(
        fetch_balance(&pool, "ACC-FROM").await.unwrap(),
        Some(900.0),
        "ACC-FROM stays at transferred-out balance"
    );
}

// ---------------------------------------------------------------------------
// Cascade rename: when a category/account is renamed, its derived `code`
// should follow and every referencing row (`fin_txn`, `fin_budget`) should
// be remapped in the same transaction. The Rust schema lacks
// `ON UPDATE CASCADE`, so this is implemented in code; tests pin the
// invariant.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_category_renames_code_and_cascades_to_txns_and_budgets() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "ACC-1", 1_000.0).await.unwrap();

    // Create a category with empty code so the helper generates one from
    // the name (slug "FOOD").
    let cat =
        ep_finance::create_category_inner(&pool, String::new(), "Food".into(), String::new(), 0)
            .await
            .expect("create category");
    assert_eq!(cat.code, "FOOD", "initial slug from English name");

    // Drop one transaction + one budget pointing at FOOD.
    let txn = ep_finance::add_txn_inner(
        &pool,
        ep_finance::AddTxnFields {
            merchant: "Diner".into(),
            category_code: "FOOD".into(),
            account_code: "ACC-1".into(),
            amount: -42.0,
            tag: "exp".into(),
            note: None,
            linked_doc_id: None,
            occurred_at: 1_700_000_000,
        },
    )
    .await
    .expect("add txn");
    ep_finance::set_budget_inner(&pool, "2026-05", "FOOD", 1_000.0)
        .await
        .expect("set budget");

    // Rename to "Dining" → slug "DINING" is fresh, so the cascade renames.
    let renamed =
        ep_finance::update_category_inner(&pool, "FOOD".into(), "Dining".into(), String::new(), 0)
            .await
            .expect("rename category");
    assert_eq!(renamed.code, "DINING");

    // fin_category: old slug gone, new slug present.
    let old: Option<i64> = sqlx::query_scalar("SELECT 1 FROM fin_category WHERE code = 'FOOD'")
        .fetch_optional(&pool)
        .await
        .unwrap();
    assert!(old.is_none(), "old code FOOD must be gone");
    let new: i64 = sqlx::query_scalar("SELECT 1 FROM fin_category WHERE code = 'DINING'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(new, 1);

    // fin_txn: cascade updated the txn we just inserted.
    let txn_cat: String = sqlx::query_scalar("SELECT category_code FROM fin_txn WHERE doc_id = ?1")
        .bind(&txn.doc_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(txn_cat, "DINING", "fin_txn.category_code follows");

    // fin_budget: cascade updated the budget row as well.
    let budget_cat: String =
        sqlx::query_scalar("SELECT category_code FROM fin_budget WHERE period = '2026-05'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(budget_cat, "DINING", "fin_budget.category_code follows");
}

#[tokio::test]
async fn update_account_renames_code_and_cascades_to_txns() {
    let pool = make_test_pool().await.expect("pool");
    seed_category(&pool, "FOOD", "Food").await.unwrap();
    let acct = ep_finance::create_account_inner(
        &pool,
        String::new(),
        "Cash Wallet".into(),
        "Cash".into(),
        String::new(),
        500.0,
    )
    .await
    .expect("create account");
    assert_eq!(acct.code, "CASH-WALLET");

    let txn = ep_finance::add_txn_inner(
        &pool,
        ep_finance::AddTxnFields {
            merchant: "Lunch".into(),
            category_code: "FOOD".into(),
            account_code: "CASH-WALLET".into(),
            amount: -12.0,
            tag: "exp".into(),
            note: None,
            linked_doc_id: None,
            occurred_at: 1_700_000_000,
        },
    )
    .await
    .expect("add txn");

    let renamed = ep_finance::update_account_inner(
        &pool,
        "CASH-WALLET".into(),
        "Petty Cash".into(),
        "Cash".into(),
        String::new(),
    )
    .await
    .expect("rename account");
    assert_eq!(renamed.code, "PETTY-CASH");

    let txn_acct: String = sqlx::query_scalar("SELECT account_code FROM fin_txn WHERE doc_id = ?1")
        .bind(&txn.doc_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(txn_acct, "PETTY-CASH", "fin_txn.account_code follows");
}

#[tokio::test]
async fn update_category_keeps_code_when_slug_unchanged() {
    let pool = make_test_pool().await.expect("pool");
    let cat =
        ep_finance::create_category_inner(&pool, String::new(), "Food".into(), String::new(), 0)
            .await
            .expect("create category");
    assert_eq!(cat.code, "FOOD");

    // Renaming to a name whose slug is identical ("FOOD") must keep the
    // existing code (no spurious "FOOD-1" generation).
    let same =
        ep_finance::update_category_inner(&pool, "FOOD".into(), "Food".into(), "amber".into(), 2)
            .await
            .expect("update");
    assert_eq!(same.code, "FOOD");
    assert_eq!(same.tone, "amber");
    assert_eq!(same.sort_order, 2);
}

#[tokio::test]
async fn update_category_falls_back_to_numbered_when_slug_taken() {
    let pool = make_test_pool().await.expect("pool");

    // Pre-seed a sibling category occupying the slug we'd otherwise pick.
    let first =
        ep_finance::create_category_inner(&pool, String::new(), "Food".into(), String::new(), 0)
            .await
            .expect("seed");
    assert_eq!(first.code, "FOOD");

    let second =
        ep_finance::create_category_inner(&pool, String::new(), "Dining".into(), String::new(), 0)
            .await
            .expect("seed");
    assert_eq!(second.code, "DINING");

    // Rename `DINING` to "Food" — slug "FOOD" is taken by the first row,
    // so the helper must fall back to a numbered code (CAT1 / CAT2 / …).
    let renamed =
        ep_finance::update_category_inner(&pool, "DINING".into(), "Food".into(), String::new(), 0)
            .await
            .expect("rename");
    assert!(
        renamed.code != "DINING" && renamed.code != "FOOD",
        "should pick a fresh fallback code, got {}",
        renamed.code
    );
    assert!(
        renamed.code.starts_with("CAT"),
        "fallback follows CATN scheme, got {}",
        renamed.code
    );
}
#[tokio::test]
async fn update_category_rotates_fallback_code_when_name_changes_in_non_ascii() {
    // When the human-readable name has no ASCII letters the slugifier
    // returns "". Older versions of the helper then walked CAT1 / CAT2
    // / … and gave the same fallback slot back to the row on every
    // rename. The current implementation seeds the search from a stable
    // fingerprint hashed off the new name, so editing the name actually
    // produces a different fallback code.
    let pool = make_test_pool().await.expect("pool");
    let first =
        ep_finance::create_category_inner(&pool, String::new(), "餐饮".into(), String::new(), 0)
            .await
            .expect("create category");
    assert!(
        first.code.starts_with("CAT"),
        "Chinese name should fall back to CATN, got {}",
        first.code
    );

    let renamed = ep_finance::update_category_inner(
        &pool,
        first.code.clone(),
        "饮食".into(),
        String::new(),
        0,
    )
    .await
    .expect("rename");
    assert!(
        renamed.code.starts_with("CAT"),
        "still falls back to CATN, got {}",
        renamed.code
    );
    assert_ne!(
        renamed.code, first.code,
        "rename must rotate the fallback code so the slot stops being sticky"
    );
}
#[tokio::test]
async fn update_category_keeps_manual_code_when_name_unchanged() {
    // Simulates a row whose code was set out-of-band (e.g. via the OpenAPI
    // PATCH path, or a SQL-level migration). A subsequent UI edit that
    // only changes tone / sort_order — leaving `name` alone — must not
    // rotate the code to a slug derived from the unchanged name.
    let pool = make_test_pool().await.expect("pool");
    seed_category(&pool, "MYCAT", "Food").await.unwrap();

    let renamed =
        ep_finance::update_category_inner(&pool, "MYCAT".into(), "Food".into(), "amber".into(), 5)
            .await
            .expect("update tone+sort_order only");
    assert_eq!(
        renamed.code, "MYCAT",
        "unchanged name must leave a manually-assigned code in place"
    );
    assert_eq!(renamed.tone, "amber");
    assert_eq!(renamed.sort_order, 5);
}

#[tokio::test]
async fn update_account_keeps_manual_code_when_name_unchanged() {
    let pool = make_test_pool().await.expect("pool");
    seed_account(&pool, "MYACC", 100.0).await.unwrap();

    let renamed = ep_finance::update_account_inner(
        &pool,
        "MYACC".into(),
        "Test MYACC".into(),
        "Cash".into(),
        "green".into(),
    )
    .await
    .expect("update type+tone only");
    assert_eq!(
        renamed.code, "MYACC",
        "unchanged name must leave a manually-assigned code in place"
    );
    assert_eq!(renamed.tone, "green");
    assert_eq!(renamed.r#type, "Cash");
}
