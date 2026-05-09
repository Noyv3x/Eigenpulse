#[cfg(feature = "ssr")]
mod pool;

#[cfg(feature = "ssr")]
pub use pool::open_pool;

#[cfg(feature = "ssr")]
pub static CORE_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::CORE_MIGRATOR;

    #[tokio::test]
    async fn core_migrations_create_activity_reference_indexes() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");

        CORE_MIGRATOR.run(&pool).await.expect("core migrations");

        for name in ["activity_module_doc", "activity_link_doc"] {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*)
                   FROM sqlite_schema
                  WHERE type = 'index' AND name = ?1",
            )
            .bind(name)
            .fetch_one(&pool)
            .await
            .expect("index lookup");

            assert_eq!(exists, 1, "missing index {name}");
        }
    }
}
