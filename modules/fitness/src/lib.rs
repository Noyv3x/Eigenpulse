mod model;
mod server_fns;
mod view;

#[cfg(feature = "ssr")]
mod api;
#[cfg(feature = "ssr")]
mod media;

pub use model::DESCRIPTOR;
pub use server_fns::load_home_summary;
pub use view::FitnessView;

pub(crate) const SCOPE_READ: &str = "fitness:read";
pub(crate) const SCOPE_WRITE: &str = "fitness:write";

#[cfg(feature = "ssr")]
pub use media::{media_router, validate_media_store};

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{AppState, Module, ModuleDescriptor};

    struct FitnessModule;
    pub static MODULE: &dyn Module = &FitnessModule;

    impl Module for FitnessModule {
        fn descriptor(&self) -> &'static ModuleDescriptor {
            &super::DESCRIPTOR
        }

        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[("001_fitness", include_str!("../migrations/001_fitness.sql"))]
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
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE _ep_module_migration (
                    module TEXT NOT NULL,
                    name TEXT NOT NULL,
                    checksum TEXT NOT NULL DEFAULT '',
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
        async fn starts_with_empty_exercise_library_and_default_settings() {
            let pool = migrated_pool().await;
            let exercises: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fit_exercise")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(exercises, 0);

            let units: String =
                sqlx::query_scalar("SELECT unit_system FROM fit_settings WHERE id = 1")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(units, "metric");
        }

        #[tokio::test]
        async fn schema_has_no_cross_module_foreign_keys() {
            let pool = migrated_pool().await;
            let tables: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_schema
                   WHERE type = 'table' AND name LIKE 'fit_%'",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            for table in tables {
                let targets: Vec<String> =
                    sqlx::query_scalar("SELECT \"table\" FROM pragma_foreign_key_list(?1)")
                        .bind(&table)
                        .fetch_all(&pool)
                        .await
                        .unwrap();
                assert!(
                    targets.iter().all(|target| target.starts_with("fit_")),
                    "{table} has cross-module FK: {targets:?}"
                );
            }
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::MODULE;
