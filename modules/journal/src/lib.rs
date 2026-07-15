mod model;
mod server_fns;
mod view;

#[cfg(feature = "ssr")]
mod api;

#[cfg(all(test, feature = "ssr"))]
mod crud_tests;

pub use model::DESCRIPTOR;
pub use server_fns::load_home_summary;
pub use view::JournalView;

pub(crate) const SCOPE_READ: &str = "journal:read";
pub(crate) const SCOPE_WRITE: &str = "journal:write";

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{AppState, Module, ModuleDescriptor};

    struct JournalModule;
    pub static MODULE: &dyn Module = &JournalModule;

    impl Module for JournalModule {
        fn descriptor(&self) -> &'static ModuleDescriptor {
            &super::DESCRIPTOR
        }

        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[("001_journal", include_str!("../migrations/001_journal.sql"))]
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

        #[tokio::test]
        async fn schema_is_empty_and_module_local() {
            let pool = crate::crud_tests::migrated_pool().await;
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jrn_entry")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 0);

            let tables: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_schema
                   WHERE type = 'table' AND name LIKE 'jrn_%' ORDER BY name",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(tables, ["jrn_entry"]);

            for table in tables {
                let targets: Vec<String> =
                    sqlx::query_scalar("SELECT \"table\" FROM pragma_foreign_key_list(?1)")
                        .bind(&table)
                        .fetch_all(&pool)
                        .await
                        .unwrap();
                assert!(
                    targets.iter().all(|target| target.starts_with("jrn_")),
                    "{table} has cross-module FK: {targets:?}"
                );
            }
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::MODULE;
