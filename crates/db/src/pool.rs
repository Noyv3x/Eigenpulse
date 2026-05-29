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

    // Fail fast on a corrupt database before we touch it with migrations.
    // `quick_check` is the cheaper sibling of `integrity_check`; anything other
    // than the single "ok" row means structural damage.
    let quick_check: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&pool)
        .await?;
    if !quick_check.eq_ignore_ascii_case("ok") {
        tracing::error!(
            result = %quick_check,
            "sqlite quick_check failed — refusing to run migrations on a corrupt database"
        );
        anyhow::bail!("sqlite integrity check failed: {quick_check}");
    }

    // Pre-migration snapshot: if there is an existing, non-empty on-disk
    // database, copy it next to the original before applying core migrations.
    // The suffix is the current schema_version (PRAGMA), so the name is
    // deterministic (no clock access — unavailable on this runtime) and a
    // re-run at the same version simply refreshes the snapshot. Failure here
    // is non-fatal (e.g. a read-only FS) but is surfaced as a warning.
    if let Some(path) = sqlite_file_path(url) {
        let non_empty = tokio::fs::metadata(&path)
            .await
            .map(|m| m.len() > 0)
            .unwrap_or(false);
        if non_empty {
            let schema_version: i64 = sqlx::query_scalar("PRAGMA schema_version")
                .fetch_one(&pool)
                .await
                .unwrap_or(0);
            let mut file_name = path
                .file_name()
                .map(|n| n.to_os_string())
                .unwrap_or_else(|| std::ffi::OsStr::new("eigenpulse.db").to_os_string());
            file_name.push(format!(".pre-migration-{schema_version}.bak"));
            let snapshot_path = path.with_file_name(file_name);
            // `VACUUM INTO` refuses to overwrite, so clear any stale snapshot
            // for this schema version first.
            let _ = tokio::fs::remove_file(&snapshot_path).await;
            match crate::backup::snapshot(&pool, &snapshot_path).await {
                Ok(()) => tracing::info!(
                    snapshot = %snapshot_path.display(),
                    schema_version,
                    "pre-migration snapshot written"
                ),
                Err(e) => tracing::warn!(
                    error = %e,
                    snapshot = %snapshot_path.display(),
                    "pre-migration snapshot failed — continuing without backup"
                ),
            }
        }
    }

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
