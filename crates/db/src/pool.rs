use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

pub async fn open_pool(url: &str) -> anyhow::Result<SqlitePool> {
    // url like `sqlite:///data/eigenpulse.db?mode=rwc`
    if let Some(path) = sqlite_file_path(url) {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
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

fn sqlite_file_path(url: &str) -> Option<PathBuf> {
    let rest = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))?;
    let path = rest.split_once('?').map(|(path, _)| path).unwrap_or(rest);
    if path.is_empty() || path == ":memory:" || path.starts_with("file:") {
        return None;
    }
    Some(Path::new(path).to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::sqlite_file_path;
    use std::path::PathBuf;

    #[test]
    fn sqlite_file_path_extracts_relative_and_absolute_paths() {
        assert_eq!(
            sqlite_file_path("sqlite://data/eigenpulse.db?mode=rwc"),
            Some(PathBuf::from("data/eigenpulse.db"))
        );
        assert_eq!(
            sqlite_file_path("sqlite:///data/eigenpulse.db?mode=rwc"),
            Some(PathBuf::from("/data/eigenpulse.db"))
        );
        assert_eq!(
            sqlite_file_path("sqlite:data/dev.db"),
            Some(PathBuf::from("data/dev.db"))
        );
    }

    #[test]
    fn sqlite_file_path_ignores_memory_and_non_sqlite_urls() {
        assert_eq!(sqlite_file_path("sqlite::memory:"), None);
        assert_eq!(sqlite_file_path("sqlite://:memory:"), None);
        assert_eq!(sqlite_file_path("postgres://example"), None);
    }
}
