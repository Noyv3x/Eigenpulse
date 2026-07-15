//! Database backup / integrity helpers.
//!
//! These run server-side only (`ep-db` is an ssr-only crate). The app layer
//! calls these to snapshot the SQLite file, run an integrity check, or report
//! the on-disk size. `VACUUM INTO` produces a compact, fully-consistent copy
//! of the database (including WAL contents) at the destination path.

use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub use crate::archive::{
    create_epbackup, restore_epbackup_offline, BackupEntry, BackupManifest, RestoreLimits,
};

static LAST_SNAPSHOT_ID: AtomicU64 = AtomicU64::new(0);
static PUBLISH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Return a lexicographically sortable, process-unique snapshot destination.
///
/// The fixed-width id is based on unix nanoseconds and is forced to increase
/// monotonically within the process, even if the system clock moves backwards.
/// Callers should still publish through [`snapshot_atomic`], which refuses to
/// replace an existing destination.
pub fn unique_snapshot_path(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let id = next_snapshot_id();
    dir.join(format!("{stem}-{id:020}.{extension}"))
}

fn next_snapshot_id() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(u64::MAX as u128) as u64;

    let mut previous = LAST_SNAPSHOT_ID.load(Ordering::Relaxed);
    loop {
        let next = now.max(previous.saturating_add(1));
        match LAST_SNAPSHOT_ID.compare_exchange_weak(
            previous,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return next,
            Err(observed) => previous = observed,
        }
    }
}

/// Take a consistent snapshot of the live database into `dest` via
/// `VACUUM INTO ?`. Parent directories of `dest` are created if missing.
///
/// `VACUUM INTO` refuses to overwrite an existing file, so the caller is
/// responsible for choosing a fresh destination (or removing a stale one).
pub(crate) async fn snapshot(pool: &SqlitePool, dest: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        let existed = tokio::fs::try_exists(parent).await?;
        tokio::fs::create_dir_all(parent).await?;
        if !existed || is_managed_backup_dir(parent) {
            set_mode(parent, 0o700).await?;
        }
    }
    let dest_str = dest
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("snapshot destination path is not valid UTF-8"))?;
    sqlx::query("VACUUM INTO ?")
        .bind(dest_str)
        .execute(pool)
        .await?;
    set_mode(dest, 0o600).await?;
    Ok(())
}

fn is_managed_backup_dir(path: &Path) -> bool {
    path == Path::new("/data")
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "data" | "backups"))
}

/// Build a snapshot beside `dest`, sync it, and atomically publish it. A hard
/// link provides strict no-replace semantics where supported; NAS filesystems
/// that reject hard links fall back to a same-directory rename serialized by
/// the process-wide data/publish locks. A failed snapshot leaves previous
/// backups intact and cleans up its temp file.
pub(crate) async fn snapshot_atomic(pool: &SqlitePool, dest: &Path) -> anyhow::Result<()> {
    let parent = dest.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(parent) = parent {
        let existed = tokio::fs::try_exists(parent).await?;
        tokio::fs::create_dir_all(parent).await?;
        if !existed || is_managed_backup_dir(parent) {
            set_mode(parent, 0o700).await?;
        }
    }

    let file_name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("snapshot destination path is not valid UTF-8"))?;
    let temp = dest.with_file_name(format!(".{file_name}.tmp-{:020}", next_snapshot_id()));

    let result = async {
        snapshot(pool, &temp).await?;
        tokio::fs::File::open(&temp).await?.sync_all().await?;
        publish_temp_noreplace(&temp, dest).await?;
        Ok(())
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp).await;
    }
    result
}

/// Publish a fully-synced temporary file beside `dest` without intentionally
/// replacing an existing destination. Callers that can run in separate
/// processes must also hold [`crate::DatabaseLock`].
pub(crate) async fn publish_temp_noreplace(temp: &Path, dest: &Path) -> anyhow::Result<()> {
    let _publish_guard = PUBLISH_LOCK.lock().await;
    if tokio::fs::try_exists(dest).await? {
        anyhow::bail!("destination already exists: {}", dest.display());
    }
    match tokio::fs::hard_link(temp, dest).await {
        Ok(()) => {
            set_mode(dest, 0o600).await?;
            tokio::fs::remove_file(temp).await?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(error.into());
        }
        Err(hard_link_error) => {
            // CIFS, exFAT, and several NAS-backed mounts reject hard links but
            // still provide atomic same-directory rename. The process-wide
            // lock closes the in-process check/rename race; DatabaseLock does
            // the same across Eigenpulse processes.
            if tokio::fs::try_exists(dest).await? {
                anyhow::bail!("destination already exists: {}", dest.display());
            }
            tracing::debug!(
                error = %hard_link_error,
                destination = %dest.display(),
                "hard-link publication unavailable; using atomic rename"
            );
            tokio::fs::rename(temp, dest).await?;
            set_mode(dest, 0o600).await?;
        }
    }
    if let Some(parent) = dest.parent().filter(|path| !path.as_os_str().is_empty()) {
        sync_directory(parent).await?;
    }
    Ok(())
}

/// Keep only the newest automatically-generated snapshots matching the exact
/// stem/extension convention from [`unique_snapshot_path`]. Manual backups use
/// a different stem and are never removed by this helper.
pub async fn prune_snapshots(
    dir: &Path,
    stem: &str,
    extension: &str,
    keep: usize,
) -> anyhow::Result<usize> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    let prefix = format!("{stem}-");
    let suffix = format!(".{extension}");
    let mut candidates = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let file_type = entry.file_type().await?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if file_type.is_file() && name.starts_with(&prefix) && name.ends_with(&suffix) {
            candidates.push(entry.path());
        }
    }
    candidates.sort();
    let remove_count = candidates.len().saturating_sub(keep.max(1));
    for path in candidates.into_iter().take(remove_count) {
        tokio::fs::remove_file(path).await?;
    }
    if remove_count > 0 {
        sync_directory(dir).await?;
    }
    Ok(remove_count)
}

#[cfg(unix)]
async fn set_mode(path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn set_mode(_path: &Path, _mode: u32) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
async fn sync_directory(path: &Path) -> anyhow::Result<()> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || std::fs::File::open(path)?.sync_all()).await??;
    Ok(())
}

#[cfg(not(unix))]
async fn sync_directory(_path: &Path) -> anyhow::Result<()> {
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
    async fn atomic_snapshot_publishes_a_readable_copy_without_temp_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("source.db");
        let pool = file_pool(&src).await;
        sqlx::query("CREATE TABLE t (value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .expect("create table");
        sqlx::query("INSERT INTO t VALUES ('kept')")
            .execute(&pool)
            .await
            .expect("insert");

        let backup_dir = dir.path().join("backups");
        let dest = unique_snapshot_path(&backup_dir, "eigenpulse", "db");
        snapshot_atomic(&pool, &dest).await.expect("snapshot");

        let copy = file_pool(&dest).await;
        let value: String = sqlx::query_scalar("SELECT value FROM t")
            .fetch_one(&copy)
            .await
            .expect("read copy");
        assert_eq!(value, "kept");
        let names = std::fs::read_dir(&backup_dir)
            .expect("read backups")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(names, vec![dest.file_name().unwrap()]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&backup_dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[tokio::test]
    async fn atomic_snapshot_never_replaces_an_existing_backup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("source.db");
        let pool = file_pool(&src).await;
        sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .expect("create table");

        let dest = dir.path().join("existing.db");
        tokio::fs::write(&dest, b"known-good")
            .await
            .expect("seed destination");
        snapshot_atomic(&pool, &dest)
            .await
            .expect_err("existing destination must be rejected");
        assert_eq!(tokio::fs::read(&dest).await.expect("read"), b"known-good");
    }

    #[tokio::test]
    async fn concurrent_atomic_snapshots_cannot_clobber_each_other() {
        let dir = tempfile::tempdir().expect("tempdir");
        let src = dir.path().join("source.db");
        let pool = file_pool(&src).await;
        sqlx::query("CREATE TABLE t (value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .expect("create table");
        sqlx::query("INSERT INTO t VALUES ('stable')")
            .execute(&pool)
            .await
            .expect("insert");
        let dest = dir.path().join("same.db");

        let (first, second) =
            tokio::join!(snapshot_atomic(&pool, &dest), snapshot_atomic(&pool, &dest));
        assert_eq!(
            usize::from(first.is_ok()) + usize::from(second.is_ok()),
            1,
            "exactly one concurrent publisher must win: {first:?}, {second:?}"
        );

        let copy = file_pool(&dest).await;
        let value: String = sqlx::query_scalar("SELECT value FROM t")
            .fetch_one(&copy)
            .await
            .expect("read copy");
        assert_eq!(value, "stable");
    }

    #[tokio::test]
    async fn automatic_snapshot_retention_keeps_newest_files_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        for _ in 0..4 {
            let path = unique_snapshot_path(dir.path(), "auto", "bak");
            tokio::fs::write(path, b"snapshot").await.expect("write");
        }
        tokio::fs::write(dir.path().join("manual-0001.bak"), b"manual")
            .await
            .expect("manual");

        assert_eq!(
            prune_snapshots(dir.path(), "auto", "bak", 2)
                .await
                .expect("prune"),
            2
        );
        let mut names = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        names.sort();
        assert_eq!(names.len(), 3);
        assert!(names.iter().any(|name| name == "manual-0001.bak"));
    }

    #[test]
    fn unique_snapshot_names_sort_in_creation_order() {
        let dir = Path::new("backups");
        let first = unique_snapshot_path(dir, "eigenpulse", "db");
        let second = unique_snapshot_path(dir, "eigenpulse", "db");
        assert!(
            first < second,
            "fixed-width names must sort chronologically"
        );
        assert_ne!(first, second);
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
