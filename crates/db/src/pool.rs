use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

pub async fn open_pool(url: &str) -> anyhow::Result<SqlitePool> {
    // url like `sqlite:///data/eigenpulse.db?mode=rwc`
    let opts = SqliteConnectOptions::from_str(url)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(opts)
        .await?;

    // Run core migrations (the global ones in /migrations).
    crate::CORE_MIGRATOR.run(&pool).await?;
    tracing::info!("db pool ready, core migrations applied");
    Ok(pool)
}
