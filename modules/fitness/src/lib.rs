mod model;
mod server_fns;
mod view;

pub use view::FitnessView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::Module;

    pub struct FitnessModule;
    pub static MODULE: &dyn Module = &FitnessModule;

    impl Module for FitnessModule {
        fn code(&self) -> &'static str {
            "FIT"
        }
        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[
                ("001_fitness", include_str!("../migrations/001_fitness.sql")),
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
        async fn migrations_remove_demo_workouts_but_keep_sequence() {
            let pool = migrated_pool().await;
            let workouts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fit_workout")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(workouts, 0);

            let seq: i64 = sqlx::query_scalar(
                "SELECT last_value FROM seq WHERE module = 'FIT' AND kind = 'type:S'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(seq, 412);
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{FitnessModule, MODULE};
