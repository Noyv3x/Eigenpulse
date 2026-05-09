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
            &[
                ("001_finance", include_str!("../migrations/001_finance.sql")),
                (
                    "002_finance_crud",
                    include_str!("../migrations/002_finance_crud.sql"),
                ),
                (
                    "003_finance_remove_archive_usage",
                    include_str!("../migrations/003_finance_remove_archive_usage.sql"),
                ),
                (
                    "004_remove_demo_seed",
                    include_str!("../migrations/004_remove_demo_seed.sql"),
                ),
            ]
        }

        fn open_api(&self, state: AppState) -> axum::Router<AppState> {
            super::api::open_api(state)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::MODULE;
        use std::collections::BTreeSet;

        #[test]
        fn every_migration_file_is_registered() {
            let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let migration_dir = manifest_dir.join("migrations");
            let files: BTreeSet<String> = std::fs::read_dir(&migration_dir)
                .unwrap_or_else(|e| panic!("read {}: {e}", migration_dir.display()))
                .map(|entry| {
                    entry
                        .expect("migration dir entry")
                        .path()
                        .file_stem()
                        .expect("migration file stem")
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            let registered: BTreeSet<String> = MODULE
                .migrations()
                .iter()
                .map(|(name, _)| (*name).to_string())
                .collect();

            assert_eq!(registered, files);
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
        async fn migrations_remove_demo_finance_records_but_keep_sequence() {
            let pool = migrated_pool().await;
            for table in ["fin_txn", "fin_budget", "fin_account", "fin_category"] {
                let sql = format!("SELECT COUNT(*) FROM {table}");
                let count: i64 = sqlx::query_scalar(&sql).fetch_one(&pool).await.unwrap();
                assert_eq!(count, 0, "{table} should not retain demo rows");
            }

            let activity_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM activity WHERE module = 'FIN'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                activity_count, 0,
                "FIN activity should not retain demo rows"
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
