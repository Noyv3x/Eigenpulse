mod amount;
mod charts;
mod model;
mod server_fns;
mod view;

#[cfg(all(test, feature = "ssr"))]
mod crud_tests;

#[cfg(feature = "ssr")]
mod api;
#[cfg(feature = "ssr")]
mod page;

pub use server_fns::load_home_summary;
pub use view::FinanceView;

pub(crate) const SCOPE_READ: &str = "finance:read";
pub(crate) const SCOPE_WRITE: &str = "finance:write";

/// Hydration-safe, compile-time module metadata. This is the only information
/// the shared shell needs; it contains no registry or database coupling.
pub static DESCRIPTOR: ep_core::ModuleDescriptor = ep_core::ModuleDescriptor {
    slug: "finance",
    route: "/finance",
    name_key: "finance.module.name",
    description_key: "finance.module.description",
    icon: ep_core::IconKind::Finance,
    read_scope: SCOPE_READ,
    write_scope: SCOPE_WRITE,
    read_scope_label_key: "app.settings.security.scope.fin_read",
    write_scope_label_key: "app.settings.security.scope.fin_write",
};

#[cfg(feature = "ssr")]
pub use api::browser_router;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{AppState, Module, ModuleDescriptor};

    struct FinanceModule;
    pub static MODULE: &dyn Module = &FinanceModule;

    impl Module for FinanceModule {
        fn descriptor(&self) -> &'static ModuleDescriptor {
            &super::DESCRIPTOR
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
        fn every_migration_is_registered() {
            ep_core::assert_module_migrations_registered!(MODULE);
        }

        #[tokio::test]
        async fn schema_is_module_local_and_empty() {
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
                .expect("finance migrations");

            let currencies: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_currency")
                .fetch_one(&pool)
                .await
                .unwrap();
            let accounts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_account")
                .fetch_one(&pool)
                .await
                .unwrap();
            let txns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM fin_txn")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(currencies, 1);
            assert_eq!(accounts, 0);
            assert_eq!(txns, 0);

            let tables: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_schema
                  WHERE type = 'table' AND name LIKE 'fin_%'
                  ORDER BY name",
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(
                tables,
                [
                    "fin_account",
                    "fin_budget",
                    "fin_category",
                    "fin_currency",
                    "fin_transfer",
                    "fin_txn"
                ]
            );
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::MODULE;
