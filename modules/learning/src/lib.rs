mod model;
mod server_fns;
mod view;

pub use view::LearningView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::Module;

    pub struct LearningModule;
    pub static MODULE: &dyn Module = &LearningModule;

    impl Module for LearningModule {
        fn code(&self) -> &'static str {
            "LRN"
        }
        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[
                (
                    "001_learning",
                    include_str!("../migrations/001_learning.sql"),
                ),
                (
                    "002_remove_demo_seed",
                    include_str!("../migrations/002_remove_demo_seed.sql"),
                ),
            ]
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
        async fn migrations_remove_demo_learning_records_but_keep_sequences() {
            let pool = migrated_pool().await;
            for table in ["lrn_note", "lrn_book", "lrn_course"] {
                let sql = format!("SELECT COUNT(*) FROM {table}");
                let count: i64 = sqlx::query_scalar(&sql).fetch_one(&pool).await.unwrap();
                assert_eq!(count, 0, "{table} should not retain demo rows");
            }

            let seqs: Vec<(String, i64)> = sqlx::query_as(
                "SELECT kind, last_value FROM seq WHERE module = 'LRN' ORDER BY kind",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(
                seqs,
                vec![
                    ("type:B".to_string(), 14),
                    ("type:C".to_string(), 11),
                    ("type:N".to_string(), 221),
                ]
            );
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{LearningModule, MODULE};
