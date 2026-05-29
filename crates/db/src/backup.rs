//! Database backup / integrity helpers.
//!
//! These run server-side only (`ep-db` is an ssr-only crate). The app layer
//! calls these to snapshot the SQLite file, run an integrity check, or report
//! the on-disk size. `VACUUM INTO` produces a compact, fully-consistent copy
//! of the database (including WAL contents) at the destination path.

use sqlx::SqlitePool;
use std::path::Path;

/// Take a consistent snapshot of the live database into `dest` via
/// `VACUUM INTO ?`. Parent directories of `dest` are created if missing.
///
/// `VACUUM INTO` refuses to overwrite an existing file, so the caller is
/// responsible for choosing a fresh destination (or removing a stale one).
pub async fn snapshot(pool: &SqlitePool, dest: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(parent).await?;
    }
    let dest_str = dest
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("snapshot destination path is not valid UTF-8"))?;
    sqlx::query("VACUUM INTO ?")
        .bind(dest_str)
        .execute(pool)
        .await?;
    Ok(())
}

/// Run `PRAGMA quick_check` and return whether the database reports `ok`.
pub async fn integrity_check(pool: &SqlitePool) -> anyhow::Result<bool> {
    let result: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(pool)
        .await?;
    Ok(result.eq_ignore_ascii_case("ok"))
}

/// On-disk logical size of the database in bytes: `page_count * page_size`.
pub async fn db_size_bytes(pool: &SqlitePool) -> anyhow::Result<i64> {
    let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(pool)
        .await?;
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(pool)
        .await?;
    Ok(page_count * page_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqliteConnectOptions;
    use std::str::FromStr;

    async fn file_pool(path: &Path) -> SqlitePool {
        let url = format!("sqlite://{}", path.display());
        let opts = SqliteConnectOptions::from_str(&url)
            .expect("opts")
            .create_if_missing(true);
        SqlitePool::connect_with(opts).await.expect("pool")
    }

    #[tokio::test]
    async fn snapshot_creates_a_readable_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("source.db");
        let pool = file_pool(&src).await;

        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY, label TEXT)")
            .execute(&pool)
            .await
            .expect("create table");
        sqlx::query("INSERT INTO t (label) VALUES ('alpha'), ('beta')")
            .execute(&pool)
            .await
            .expect("insert");

        // Destination lives inside a not-yet-existing subdir to exercise the
        // parent-dir creation path.
        let dest = dir.path().join("backups").join("snap.db");
        snapshot(&pool, &dest).await.expect("snapshot");
        assert!(dest.exists(), "snapshot file should exist");
        assert!(
            tokio::fs::metadata(&dest).await.expect("meta").len() > 0,
            "snapshot file should be non-empty"
        );

        // The copy should be a valid SQLite db carrying the same rows.
        let copy = file_pool(&dest).await;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM t")
            .fetch_one(&copy)
            .await
            .expect("count");
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn integrity_check_passes_on_fresh_db() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("integ.db");
        let pool = file_pool(&src).await;
        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .expect("create table");
        assert!(integrity_check(&pool).await.expect("integrity_check"));
    }

    #[tokio::test]
    async fn db_size_bytes_is_positive_and_page_aligned() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("size.db");
        let pool = file_pool(&src).await;
        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY, blob BLOB)")
            .execute(&pool)
            .await
            .expect("create table");

        let size = db_size_bytes(&pool).await.expect("db_size_bytes");
        assert!(size > 0, "size should be positive, got {size}");
        // Logical size is always a whole number of pages.
        let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
            .fetch_one(&pool)
            .await
            .expect("page_size");
        assert_eq!(size % page_size, 0, "size should be page-aligned");
    }
}
