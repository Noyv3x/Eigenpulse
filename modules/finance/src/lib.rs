mod model;
mod server_fns;
mod suggestions;
mod view;

#[cfg(feature = "ssr")]
mod api;

pub use model::{CategorySummary, MonthBucket};
pub use view::{render_net_strip, FinanceView};

#[cfg(feature = "ssr")]
pub use api::open_api;
#[cfg(feature = "ssr")]
pub use server_fns::{
    add_transfer_inner, add_txn_inner, delete_account_inner, delete_category_inner,
    delete_txn_inner, load_month_buckets_12, parse_occurred_at, set_budget_inner, update_txn_inner,
    AddTxnFields, UpdateTxnFields,
};

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{AppState, Module};

    pub struct FinanceModule;

    pub static MODULE: &dyn Module = &FinanceModule;

    impl Module for FinanceModule {
        fn code(&self) -> &'static str {
            "FIN"
        }

        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[("001_finance", include_str!("../migrations/001_finance.sql"))]
        }

        fn open_api(&self, state: AppState) -> axum::Router<AppState> {
            super::api::open_api(state)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::MODULE;

        #[test]
        fn every_migration_file_is_registered() {
            ep_core::assert_module_migrations_registered!(MODULE);
        }

        async fn migrated_pool() -> sqlx::SqlitePool {
            let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
            sqlx::query(
                "CREATE TABLE seq (
                    module TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    last_value INTEGER NOT NULL,
                    PRIMARY KEY (module, kind)
                )",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE activity (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    occurred_at INTEGER NOT NULL,
                    module TEXT NOT NULL,
                    doc_id TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    status TEXT,
                    amount REAL,
                    link_doc TEXT
                )",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TABLE _ep_module_migration (
                    module TEXT NOT NULL,
                    name TEXT NOT NULL,
                    applied_at INTEGER NOT NULL DEFAULT (unixepoch()),
                    PRIMARY KEY (module, name)
                )",
            )
            .execute(&pool)
            .await
            .unwrap();
            ep_core::run_module_migrations(&pool, MODULE)
                .await
                .expect("module migrations");
            pool
        }

        #[tokio::test]
        async fn initial_migration_has_empty_finance_records_but_keeps_sequence() {
            let pool = migrated_pool().await;
            let counts = [
                (
                    "fin_txn",
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fin_txn")
                        .fetch_one(&pool)
                        .await
                        .unwrap(),
                ),
                (
                    "fin_budget",
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fin_budget")
                        .fetch_one(&pool)
                        .await
                        .unwrap(),
                ),
                (
                    "fin_account",
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fin_account")
                        .fetch_one(&pool)
                        .await
                        .unwrap(),
                ),
                (
                    "fin_category",
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fin_category")
                        .fetch_one(&pool)
                        .await
                        .unwrap(),
                ),
            ];
            for (table, count) in counts {
                assert_eq!(count, 0, "{table} should start empty");
            }

            let activity_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM activity WHERE module = 'FIN'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                activity_count, 0,
                "FIN activity should start empty"
            );

            let seq: i64 = sqlx::query_scalar(
                "SELECT last_value FROM seq WHERE module = 'FIN' AND kind = 'doc:y24'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(seq, 91);
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{FinanceModule, MODULE};
