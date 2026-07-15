#[cfg(feature = "ssr")]
pub mod backup;

#[cfg(feature = "ssr")]
mod archive;

#[cfg(feature = "ssr")]
mod lock;

#[cfg(feature = "ssr")]
mod pool;

#[cfg(feature = "ssr")]
pub use lock::{acquire_database_lock, DatabaseLock};
#[cfg(feature = "ssr")]
pub use pool::{
    database_path_from_url, open_pool, pre_migration_snapshot, unbacked_migration_allowed,
};

#[cfg(feature = "ssr")]
pub(crate) const CURRENT_SCHEMA_GENERATION: u32 = 2;

#[cfg(feature = "ssr")]
pub(crate) static CORE_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::{CORE_MIGRATOR, CURRENT_SCHEMA_GENERATION};
    use std::collections::BTreeSet;

    #[tokio::test]
    async fn core_migrations_create_current_platform_schema() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");

        CORE_MIGRATOR.run(&pool).await.expect("core migrations");

        let generation: String =
            sqlx::query_scalar("SELECT value FROM ep_meta WHERE key = 'schema_generation'")
                .fetch_one(&pool)
                .await
                .expect("generation");
        assert_eq!(generation, CURRENT_SCHEMA_GENERATION.to_string());

        let tables = sqlx::query_scalar::<_, String>(
            "SELECT name FROM sqlite_schema
              WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_all(&pool)
        .await
        .expect("platform table list")
        .into_iter()
        .collect::<BTreeSet<_>>();
        let expected = [
            "_ep_module_migration",
            "_sqlx_migrations",
            "app_user",
            "ep_meta",
            "notification",
            "notify_channel",
            "notify_delivery",
            "notify_outbox",
            "pat",
            "session",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
        assert_eq!(
            tables, expected,
            "platform migrations own only platform tables"
        );

        let notification_columns = sqlx::query_scalar::<_, String>(
            "SELECT name FROM pragma_table_info('notification') ORDER BY cid",
        )
        .fetch_all(&pool)
        .await
        .expect("notification columns");
        assert_eq!(
            notification_columns,
            [
                "id",
                "created_at",
                "severity",
                "source",
                "title",
                "body",
                "link",
                "read_at"
            ]
        );
    }
}
