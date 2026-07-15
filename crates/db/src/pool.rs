use anyhow::Context as _;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

pub async fn open_pool(url: &str) -> anyhow::Result<SqlitePool> {
    // url like `sqlite:///data/eigenpulse.db?mode=rwc`
    let sqlite_path_hint = sqlite_file_path(url);
    let mut existing_database_had_content = false;
    if let Some(path) = sqlite_path_hint.as_deref() {
        // Existing databases are generation-gated before chmod, PRAGMAs, or
        // migrations. A rejected incompatible file must be left untouched.
        if tokio::fs::try_exists(path).await? {
            validate_regular_file(path).await?;
            existing_database_had_content = tokio::fs::metadata(path).await?.len() > 0;
            reject_unsupported_schema_generation(url, path).await?;
        }
        prepare_sqlite_path(path).await?;
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

    // `pragma_database_list` is SQLite's authority after URI parsing and
    // percent-decoding. Never base migration safety or chmod on the raw URL:
    // `file:` URIs and `%20` paths otherwise point our checks at no file (or
    // the wrong one) while SQLx opens a real database elsewhere.
    let sqlite_path = connected_database_path(&pool).await?;
    if let Some(path) = sqlite_path.as_deref() {
        if let Some(parent) = path
            .parent()
            .filter(|parent| is_managed_private_dir(parent))
        {
            set_private_mode(parent, 0o700).await?;
        }
        secure_sqlite_files(path).await?;
    }

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
    // database with pending core migrations, copy it next to the original.
    // Every snapshot receives a sortable unique name and is atomically
    // published, so a failed attempt can never remove the previous good copy.
    // A migration without a recoverable snapshot is unsafe. Fail closed unless
    // the operator explicitly enables the documented emergency override.
    let migrations_pending = core_migrations_pending(&pool).await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "could not inspect migration state; taking a safety snapshot");
        true
    });
    if migrations_pending && existing_database_had_content {
        match pre_migration_snapshot(&pool).await {
            Ok(Some(snapshot_path)) => tracing::info!(
                snapshot = %snapshot_path.display(),
                "pre-migration snapshot written"
            ),
            Ok(None) => {}
            Err(e) if unbacked_migration_allowed() => tracing::warn!(
                error = %e,
                "pre-migration snapshot failed — emergency override allows migration without backup"
            ),
            Err(e) => return Err(e.context(
                "pre-migration snapshot failed; refusing to migrate without EP_ALLOW_UNBACKED_MIGRATION=1",
            )),
        }
    }

    // Run the current platform migrations. Module-owned business tables are
    // migrated separately by the module registry.
    crate::CORE_MIGRATOR.run(&pool).await?;
    tracing::info!("db pool ready, core migrations applied");
    Ok(pool)
}

/// Inspect an existing disk database through a strictly read-only connection
/// before the normal WAL connection is opened. Setting journal mode or running
/// a migration can modify SQLite state, so the generation gate must happen
/// before either operation.
///
/// Empty files are new databases and are allowed through to the migrator. Any
/// non-empty database must explicitly identify itself as the current schema
/// generation; incompatible development databases are never upgraded in place.
async fn reject_unsupported_schema_generation(url: &str, path: &Path) -> anyhow::Result<()> {
    if tokio::fs::metadata(path).await?.len() == 0 {
        return Ok(());
    }

    // Do not set `immutable=1`: after an unclean shutdown the generation row
    // may exist only in the WAL, and immutable SQLite connections ignore that
    // recovery state. `read_only` observes the existing WAL without permitting
    // writes to the database.
    let options = SqliteConnectOptions::from_str(url)?
        .read_only(true)
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(5));
    let inspection_pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .with_context(|| {
            format!(
                "could not inspect existing database {} without modifying it",
                path.display()
            )
        })?;

    let generation = async {
        let meta_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM sqlite_schema
                  WHERE type = 'table' AND name = 'ep_meta'
             )",
        )
        .fetch_one(&inspection_pool)
        .await?;
        if !meta_exists {
            return Ok::<Option<String>, sqlx::Error>(None);
        }

        sqlx::query_scalar::<_, String>("SELECT value FROM ep_meta WHERE key = 'schema_generation'")
            .fetch_optional(&inspection_pool)
            .await
    }
    .await;
    inspection_pool.close().await;

    let generation = generation.with_context(|| {
        format!(
            "could not read schema generation from existing database {}",
            path.display()
        )
    })?;
    let expected = crate::CURRENT_SCHEMA_GENERATION.to_string();
    if generation.as_deref() != Some(expected.as_str()) {
        let found = generation.as_deref().unwrap_or("unmarked schema");
        anyhow::bail!(
            "database schema generation is unsupported ({found}); expected generation {expected}. \
             Eigenpulse does not migrate incompatible development databases automatically. \
             Back up and move the existing database at {}, then restart to initialize a fresh database",
            path.display()
        );
    }

    Ok(())
}

pub fn unbacked_migration_allowed() -> bool {
    std::env::var("EP_ALLOW_UNBACKED_MIGRATION")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

async fn prepare_sqlite_path(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        let existed = tokio::fs::try_exists(parent).await?;
        tokio::fs::create_dir_all(parent).await?;
        if !existed || is_managed_private_dir(parent) {
            set_private_mode(parent, 0o700).await?;
        }
    }

    if !tokio::fs::try_exists(path).await? {
        create_private_file(path).await?;
    }
    validate_regular_file(path).await?;
    set_private_mode(path, 0o600).await
}

async fn secure_sqlite_files(path: &Path) -> anyhow::Result<()> {
    validate_regular_file(path).await?;
    set_private_mode(path, 0o600).await?;
    let raw = path.as_os_str().to_string_lossy();
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{raw}{suffix}"));
        if tokio::fs::try_exists(&sidecar).await? {
            validate_regular_file(&sidecar).await?;
            set_private_mode(&sidecar, 0o600).await?;
        }
    }
    Ok(())
}

fn is_managed_private_dir(path: &Path) -> bool {
    path == Path::new("/data")
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "data" | "backups"))
}

async fn validate_regular_file(path: &Path) -> anyhow::Result<()> {
    let metadata = tokio::fs::symlink_metadata(path).await?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!(
            "sensitive path must be a regular non-symlink file: {}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(unix)]
async fn create_private_file(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map(|_| ())
    })
    .await??;
    Ok(())
}

#[cfg(not(unix))]
async fn create_private_file(path: &Path) -> anyhow::Result<()> {
    tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    Ok(())
}

#[cfg(unix)]
async fn set_private_mode(path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let result = tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).await;
    match result {
        Ok(()) => Ok(()),
        Err(error) if insecure_permissions_allowed() => {
            tracing::warn!(path = %path.display(), %error, "could not enforce private permissions; emergency override active");
            Ok(())
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "could not set private permissions on {}; use EP_ALLOW_INSECURE_FILE_PERMISSIONS=1 only if the filesystem enforces equivalent ACLs",
                path.display()
            )
        }),
    }
}

#[cfg(not(unix))]
async fn set_private_mode(_path: &Path, _mode: u32) -> anyhow::Result<()> {
    Ok(())
}

fn insecure_permissions_allowed() -> bool {
    std::env::var("EP_ALLOW_INSECURE_FILE_PERMISSIONS")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Atomically snapshot an existing on-disk database immediately before a
/// caller applies migrations. In-memory and empty files have no user state to
/// preserve and return `None`.
pub async fn pre_migration_snapshot(pool: &SqlitePool) -> anyhow::Result<Option<PathBuf>> {
    let Some(path) = connected_database_path(pool).await? else {
        return Ok(None);
    };
    // A non-empty SQLite main path is a disk database. Metadata failures are
    // safety failures, not evidence of an empty DB; fail closed so migrations
    // cannot proceed after an unlink/permission/path race.
    if tokio::fs::metadata(&path).await?.len() == 0 {
        return Ok(None);
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("eigenpulse.db");
    let stem = format!("{file_name}.pre-migration");
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    // VACUUM creates its destination before chmod. Put both the temporary and
    // published files inside a 0700 managed directory so a crash cannot leave
    // a secret-bearing 0644 snapshot in a traversable custom DB parent.
    let backup_dir = parent.join("backups");
    let snapshot_path = crate::backup::unique_snapshot_path(&backup_dir, &stem, "bak");
    crate::backup::snapshot_atomic(pool, &snapshot_path).await?;
    crate::backup::prune_snapshots(&backup_dir, &stem, "bak", 10).await?;
    Ok(Some(snapshot_path))
}

async fn connected_database_path(pool: &SqlitePool) -> anyhow::Result<Option<PathBuf>> {
    let path: String =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
            .fetch_one(pool)
            .await?;
    Ok((!path.is_empty()).then(|| PathBuf::from(path)))
}

async fn core_migrations_pending(pool: &SqlitePool) -> anyhow::Result<bool> {
    let ledger_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await?;
    if !ledger_exists {
        return Ok(crate::CORE_MIGRATOR.iter().next().is_some());
    }

    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = TRUE")
            .fetch_all(pool)
            .await?;
    Ok(crate::CORE_MIGRATOR
        .iter()
        .any(|migration| !applied.contains(&migration.version)))
}

fn sqlite_file_path(url: &str) -> Option<PathBuf> {
    let rest = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"))?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    if query
        .split('&')
        .any(|part| part.eq_ignore_ascii_case("mode=memory"))
    {
        return None;
    }
    let path = path.strip_prefix("file:").unwrap_or(path);
    if path.is_empty() || path == ":memory:" {
        return None;
    }
    let path = percent_encoding::percent_decode_str(path)
        .decode_utf8()
        .ok()?;
    Some(Path::new(path.as_ref()).to_path_buf())
}

/// Extract the on-disk SQLite path from a `DATABASE_URL` without opening it.
/// Memory databases and non-SQLite URLs return `None`.
pub fn database_path_from_url(url: &str) -> Option<PathBuf> {
    sqlite_file_path(url)
}

#[cfg(test)]
mod tests {
    use super::{
        connected_database_path, core_migrations_pending, open_pool, pre_migration_snapshot,
        reject_unsupported_schema_generation, sqlite_file_path,
    };
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
    use sqlx::SqlitePool;
    use std::path::PathBuf;
    use std::str::FromStr;

    const CURRENT_GENERATION: u32 = crate::CURRENT_SCHEMA_GENERATION;

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
        assert_eq!(
            sqlite_file_path("sqlite:file:/data/eigenpulse.db?mode=rwc"),
            Some(PathBuf::from("/data/eigenpulse.db"))
        );
        assert_eq!(
            sqlite_file_path("sqlite://data/my%20db.db?mode=rwc"),
            Some(PathBuf::from("data/my db.db"))
        );
    }

    #[test]
    fn sqlite_file_path_ignores_memory_and_non_sqlite_urls() {
        assert_eq!(sqlite_file_path("sqlite::memory:"), None);
        assert_eq!(sqlite_file_path("sqlite://:memory:"), None);
        assert_eq!(
            sqlite_file_path("sqlite:file:shared?mode=memory&cache=shared"),
            None
        );
        assert_eq!(sqlite_file_path("postgres://example"), None);
    }

    #[tokio::test]
    async fn pending_migrations_are_detected_from_the_sqlx_ledger() {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("pool");
        assert!(core_migrations_pending(&pool).await.expect("pending"));

        crate::CORE_MIGRATOR.run(&pool).await.expect("migrate");
        assert!(!core_migrations_pending(&pool).await.expect("current"));
    }

    #[tokio::test]
    async fn open_pool_rejects_unmarked_database_without_modifying_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("unmarked.db");
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let options = SqliteConnectOptions::from_str(&url)
            .expect("options")
            .create_if_missing(true);
        let existing = SqlitePool::connect_with(options)
            .await
            .expect("existing pool");
        sqlx::query("CREATE TABLE existing_data (id INTEGER PRIMARY KEY)")
            .execute(&existing)
            .await
            .expect("unmarked schema");
        existing.close().await;

        let before = tokio::fs::read(&path).await.expect("read before");
        let error = open_pool(&url)
            .await
            .expect_err("unmarked database must be rejected");
        let after = tokio::fs::read(&path).await.expect("read after");

        assert_eq!(before, after, "generation rejection must not write the db");
        let message = format!("{error:#}");
        assert!(
            message.contains(&format!("expected generation {CURRENT_GENERATION}")),
            "{message}"
        );
        assert!(
            message.contains("does not migrate incompatible"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn open_pool_rejects_other_generation_without_modifying_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("other-generation.db");
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let options = SqliteConnectOptions::from_str(&url)
            .expect("options")
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.expect("seed pool");
        let other = CURRENT_GENERATION + 1;
        sqlx::raw_sql(&format!(
            "CREATE TABLE ep_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);\
             INSERT INTO ep_meta VALUES ('schema_generation', '{other}');"
        ))
        .execute(&pool)
        .await
        .expect("other generation");
        pool.close().await;

        let before = tokio::fs::read(&path).await.expect("read before");
        let error = open_pool(&url)
            .await
            .expect_err("other generation must be rejected");
        let after = tokio::fs::read(&path).await.expect("read after");
        assert_eq!(before, after, "generation rejection must not write the db");
        let message = format!("{error:#}");
        assert!(
            message.contains(&format!("unsupported ({other})")),
            "{message}"
        );
    }

    #[tokio::test]
    async fn open_pool_accepts_existing_current_generation_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("current.db");
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let options = SqliteConnectOptions::from_str(&url)
            .expect("options")
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.expect("seed pool");
        crate::CORE_MIGRATOR
            .run(&pool)
            .await
            .expect("current migrations");
        pool.close().await;

        let reopened = open_pool(&url).await.expect("open current pool");
        let generation: String =
            sqlx::query_scalar("SELECT value FROM ep_meta WHERE key = 'schema_generation'")
                .fetch_one(&reopened)
                .await
                .expect("generation");
        assert_eq!(generation, CURRENT_GENERATION.to_string());
    }

    #[tokio::test]
    async fn first_boot_does_not_snapshot_the_new_empty_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("fresh.db");
        let url = format!("sqlite://{}?mode=rwc", path.display());

        let pool = open_pool(&url).await.expect("first boot");
        pool.close().await;

        assert!(path.is_file());
        assert!(
            !dir.path().join("backups").exists(),
            "a brand-new database has no user state worth snapshotting"
        );
    }

    #[tokio::test]
    async fn generation_gate_reads_a_generation_row_that_is_only_in_wal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("wal-current.db");
        let url = format!("sqlite://{}?mode=rwc", path.display());

        // Establish the main database, then keep the current generation marker
        // in an uncheckpointed WAL to model a process crash.
        let seed = SqlitePool::connect_with(
            SqliteConnectOptions::from_str(&url)
                .expect("options")
                .create_if_missing(true),
        )
        .await
        .expect("seed pool");
        sqlx::query("CREATE TABLE seed(id INTEGER PRIMARY KEY)")
            .execute(&seed)
            .await
            .expect("seed schema");
        seed.close().await;

        let writer = SqlitePool::connect_with(
            SqliteConnectOptions::from_str(&url)
                .expect("options")
                .journal_mode(SqliteJournalMode::Wal),
        )
        .await
        .expect("writer");
        sqlx::query("PRAGMA wal_autocheckpoint = 0")
            .execute(&writer)
            .await
            .expect("disable checkpoint");
        sqlx::raw_sql(&format!(
            "CREATE TABLE ep_meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);\
             INSERT INTO ep_meta VALUES ('schema_generation', '{CURRENT_GENERATION}');"
        ))
        .execute(&writer)
        .await
        .expect("generation in WAL");
        assert!(std::path::PathBuf::from(format!("{}-wal", path.display())).exists());

        reject_unsupported_schema_generation(&url, &path)
            .await
            .expect("valid WAL-backed generation must be accepted");
        writer.close().await;
    }

    #[tokio::test]
    async fn migration_snapshots_are_unique_and_never_replace_the_previous_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("eigenpulse.db");
        let url = format!("sqlite://{}", path.display());
        let options = SqliteConnectOptions::from_str(&url)
            .expect("options")
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.expect("pool");
        sqlx::query("CREATE TABLE data (value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .expect("create");
        sqlx::query("INSERT INTO data VALUES ('before')")
            .execute(&pool)
            .await
            .expect("insert");

        let first = pre_migration_snapshot(&pool)
            .await
            .expect("first snapshot")
            .expect("file path");
        let second = pre_migration_snapshot(&pool)
            .await
            .expect("second snapshot")
            .expect("file path");

        assert!(first.exists());
        assert!(second.exists());
        assert_ne!(first, second);
        assert!(first < second);
        assert_eq!(
            first
                .parent()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str()),
            Some("backups")
        );
    }

    #[tokio::test]
    async fn file_uri_is_treated_as_a_real_disk_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("uri.db");
        let url = format!("sqlite:file:{}?mode=rwc", path.display());
        let pool = open_pool(&url).await.expect("open file URI");

        assert_eq!(
            connected_database_path(&pool).await.expect("database path"),
            Some(path.clone())
        );
        assert!(path.is_file());
        assert!(
            pre_migration_snapshot(&pool)
                .await
                .expect("snapshot")
                .is_some(),
            "a file: URI must not bypass migration snapshots"
        );
    }

    #[tokio::test]
    async fn percent_encoded_filename_uses_sqlites_decoded_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("my database.db");
        let encoded = path.to_string_lossy().replace(' ', "%20");
        let encoded_path = PathBuf::from(&encoded);
        let url = format!("sqlite://{encoded}?mode=rwc");
        let pool = open_pool(&url).await.expect("open encoded path");

        assert_eq!(
            connected_database_path(&pool).await.expect("database path"),
            Some(path.clone())
        );
        assert!(path.is_file());
        assert_ne!(encoded_path, path);
        assert!(
            !encoded_path.exists(),
            "must not create the raw %20 filename"
        );
    }

    #[tokio::test]
    async fn file_uri_memory_database_has_no_snapshot_path() {
        let pool = open_pool("sqlite:file:ep-db-memory-test?mode=memory&cache=shared")
            .await
            .expect("open URI memory database");
        assert_eq!(
            connected_database_path(&pool).await.expect("database path"),
            None
        );
        assert!(pre_migration_snapshot(&pool)
            .await
            .expect("memory snapshot check")
            .is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn open_pool_creates_private_managed_directory_and_database() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("tempdir");
        let data = root.path().join("data");
        let path = data.join("private.db");
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let pool = open_pool(&url).await.expect("open pool");
        pool.close().await;

        assert_eq!(
            std::fs::metadata(&data).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn open_pool_rejects_database_symlink() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("target.db");
        std::fs::write(&target, []).expect("target");
        let link = root.path().join("linked.db");
        symlink(&target, &link).expect("symlink");
        let url = format!("sqlite://{}?mode=rwc", link.display());
        let error = open_pool(&url).await.expect_err("symlink must be rejected");
        assert!(error.to_string().contains("non-symlink"));
    }
}
