mod model;
mod server_fns;
mod view;

#[cfg(feature = "ssr")]
mod api;

pub use view::LearningView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{AppState, Module};

    pub struct LearningModule;
    pub static MODULE: &dyn Module = &LearningModule;

    impl Module for LearningModule {
        fn code(&self) -> &'static str {
            "LRN"
        }
        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[(
                "001_learning",
                include_str!("../migrations/001_learning.sql"),
            )]
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
        async fn initial_migration_has_empty_learning_records_but_keeps_sequences() {
            let pool = migrated_pool().await;
            let counts = [
                (
                    "lrn_note",
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM lrn_note")
                        .fetch_one(&pool)
                        .await
                        .unwrap(),
                ),
                (
                    "lrn_book",
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM lrn_book")
                        .fetch_one(&pool)
                        .await
                        .unwrap(),
                ),
                (
                    "lrn_course",
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM lrn_course")
                        .fetch_one(&pool)
                        .await
                        .unwrap(),
                ),
            ];
            for (table, count) in counts {
                assert_eq!(count, 0, "{table} should start empty");
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
