//! App-level admin / maintenance server functions: a status panel, a one-shot
//! database backup, and a full data export (with a documented import stub).
//!
//! Every fn is OWNER-gated via [`ep_auth::require_user_for_server_fn`] and is
//! SSR-only — the `#[cfg(not(feature = "ssr"))]` arms are pure stubs that never
//! touch the DB so the hydrate/wasm bundle stays free of `sqlx`/`tokio::fs`.
//!
//! Secret hygiene: none of the DTOs returned here carry secret-bearing columns.
//! [`export_all`] explicitly skips `pat.hash`, `notify_channel.config_json`, and
//! `app_user.password_hash` (see [`SECRET_COLUMNS`]).

use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

/// Status snapshot for the settings "Data" panel. No secrets, no clock-on-wasm:
/// every timestamp/derived value is computed server-side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminStatus {
    /// `CARGO_PKG_VERSION` of the running binary.
    pub version: String,
    /// Logical on-disk size (`page_count * page_size`).
    pub db_size_bytes: i64,
    /// `PRAGMA quick_check == "ok"`.
    pub integrity_ok: bool,
    /// Live rows in `session`.
    pub session_count: i64,
    /// Live rows in `notification`.
    pub notification_count: i64,
    /// Most recent backup under the data dir, if any.
    pub last_backup_path: Option<String>,
    pub last_backup_exists: bool,
    /// Size of the most recent backup file in bytes, if present.
    pub last_backup_bytes: Option<i64>,
}

/// Result of [`run_backup`]: where the snapshot landed and how big it is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub path: String,
    pub bytes: i64,
}

/// Full user-data export. A flat map of `table name -> rows`, where each row is
/// a JSON object of `column -> value`. Secret columns are omitted at the source
/// (see [`export_all`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataExport {
    pub version: String,
    /// Server-side capture time (unix seconds). Computed in the `#[server]` body
    /// — never on wasm.
    pub exported_at: i64,
    /// Per-table row arrays, keyed by table name.
    pub tables: std::collections::BTreeMap<String, Vec<serde_json::Value>>,
}

#[server(AdminStatusFn, "/api/_internal/admin", "Url", "admin_status")]
pub async fn admin_status() -> Result<AdminStatus, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        ssr::status(&state.db).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(RunBackupFn, "/api/_internal/admin", "Url", "run_backup")]
pub async fn run_backup() -> Result<BackupInfo, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        ssr::run_backup(&state.db).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(ExportAllFn, "/api/_internal/admin", "Url", "export_all")]
pub async fn export_all() -> Result<DataExport, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        ssr::export_all(&state.db).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

/// Import a previously-exported [`DataExport`].
///
/// DELIBERATE STUB. A safe import must upsert into ~20 inter-FK'd tables inside
/// a single transaction, validate the schema/version, and reconcile the `seq`
/// counters so future doc IDs don't collide — a destructive operation that is
/// not safe to half-implement. Until that lands it returns a localized
/// "not yet implemented" error and performs **no** writes. Export is fully
/// functional and round-trippable as JSON in the meantime.
#[server(ImportAllFn, "/api/_internal/admin", "Url", "import_all")]
pub async fn import_all(_payload: DataExport) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        // No DB writes — see the doc comment. The error CODE is the i18n key;
        // the client renders it via `ep_i18n::server_fn_error_text`.
        Err(ep_i18n::err("app.admin.err_import_unimplemented"))
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = _payload;
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
mod ssr {
    use super::{AdminStatus, BackupInfo, DataExport};
    use ep_core::server_err;
    use leptos::server_fn::ServerFnError;
    use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};
    use std::path::{Path, PathBuf};

    /// Column names that must never leave the server. Matched case-sensitively
    /// against `sqlx` column names; see AGENTS.md "Secret hygiene".
    const SECRET_COLUMNS: &[&str] = &["hash", "config_json", "password_hash"];

    /// Domain tables included in a full export, in FK-friendly order (parents
    /// before children). Excludes ops/infra tables (`session`, `seq`,
    /// `_sqlx_migrations`, `_ep_module_migration`, `pat`) — those are either
    /// secrets, regenerable, or device-local.
    const EXPORT_TABLES: &[&str] = &[
        "app_user",
        "module_link",
        "activity",
        "notification",
        "notify_channel",
        "notify_delivery",
        "fin_currency",
        "fin_account",
        "fin_category",
        "fin_txn",
        "fin_budget",
        "fit_workout",
        "fit_set",
        "lrn_course",
        "lrn_book",
        "lrn_note",
    ];

    pub async fn status(pool: &SqlitePool) -> Result<AdminStatus, ServerFnError> {
        let db_size_bytes = ep_db::backup::db_size_bytes(pool)
            .await
            .map_err(server_err)?;
        let integrity_ok = ep_db::backup::integrity_check(pool)
            .await
            .map_err(server_err)?;
        let session_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session")
            .fetch_one(pool)
            .await
            .map_err(server_err)?;
        let notification_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification")
            .fetch_one(pool)
            .await
            .map_err(server_err)?;

        let (last_backup_path, last_backup_exists, last_backup_bytes) =
            match latest_backup().await.map_err(server_err)? {
                Some((path, bytes)) => (Some(path.display().to_string()), true, Some(bytes)),
                None => (None, false, None),
            };

        Ok(AdminStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            db_size_bytes,
            integrity_ok,
            session_count,
            notification_count,
            last_backup_path,
            last_backup_exists,
            last_backup_bytes,
        })
    }

    pub async fn run_backup(pool: &SqlitePool) -> Result<BackupInfo, ServerFnError> {
        let dir = backup_dir();
        tokio::fs::create_dir_all(&dir).await.map_err(server_err)?;
        // Deterministic, monotonic filename keyed on SQLite's user_version so a
        // schema change always lands in a fresh file. `VACUUM INTO` refuses to
        // overwrite, so we remove any stale same-name snapshot first.
        let user_version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(pool)
            .await
            .map_err(server_err)?;
        let dest = dir.join(format!("eigenpulse-v{user_version}.db"));
        if tokio::fs::try_exists(&dest).await.unwrap_or(false) {
            tokio::fs::remove_file(&dest).await.map_err(server_err)?;
        }
        ep_db::backup::snapshot(pool, &dest)
            .await
            .map_err(server_err)?;
        let bytes = tokio::fs::metadata(&dest).await.map_err(server_err)?.len() as i64;
        Ok(BackupInfo {
            path: dest.display().to_string(),
            bytes,
        })
    }

    pub async fn export_all(pool: &SqlitePool) -> Result<DataExport, ServerFnError> {
        let mut tables = std::collections::BTreeMap::new();
        for &table in EXPORT_TABLES {
            let rows = export_table(pool, table).await?;
            tables.insert(table.to_string(), rows);
        }
        Ok(DataExport {
            version: env!("CARGO_PKG_VERSION").to_string(),
            exported_at: ep_core::unix_now(),
            tables,
        })
    }

    /// Read every row of `table` as JSON objects, dropping any [`SECRET_COLUMNS`]
    /// at the source. `table` is a fixed crate-internal literal (never user
    /// input), so interpolating it into the query is injection-safe.
    async fn export_table(
        pool: &SqlitePool,
        table: &str,
    ) -> Result<Vec<serde_json::Value>, ServerFnError> {
        let rows = sqlx::query(&format!("SELECT * FROM {table}"))
            .fetch_all(pool)
            .await
            .map_err(server_err)?;
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut obj = serde_json::Map::new();
            for col in row.columns() {
                let name = col.name();
                if SECRET_COLUMNS.contains(&name) {
                    continue;
                }
                obj.insert(name.to_string(), sqlite_value_to_json(row, col.ordinal())?);
            }
            out.push(serde_json::Value::Object(obj));
        }
        Ok(out)
    }

    /// Convert a single SQLite cell to a JSON value by its declared type.
    /// SQLite's storage classes are NULL/INTEGER/REAL/TEXT/BLOB; BLOBs (none in
    /// the exported tables today) are hex-encoded so the export stays valid JSON.
    fn sqlite_value_to_json(
        row: &sqlx::sqlite::SqliteRow,
        idx: usize,
    ) -> Result<serde_json::Value, ServerFnError> {
        let raw = row.try_get_raw(idx).map_err(server_err)?;
        if raw.is_null() {
            return Ok(serde_json::Value::Null);
        }
        let type_name = raw.type_info().name().to_uppercase();
        let value = match type_name.as_str() {
            "INTEGER" | "BIGINT" | "INT" => {
                let v: i64 = row.try_get(idx).map_err(server_err)?;
                serde_json::Value::from(v)
            }
            "REAL" | "DOUBLE" | "FLOAT" => {
                let v: f64 = row.try_get(idx).map_err(server_err)?;
                serde_json::Value::from(v)
            }
            "BLOB" => {
                let v: Vec<u8> = row.try_get(idx).map_err(server_err)?;
                serde_json::Value::from(hex::encode(v))
            }
            // TEXT and anything else decode losslessly as a string.
            _ => {
                let v: String = row.try_get(idx).map_err(server_err)?;
                serde_json::Value::from(v)
            }
        };
        Ok(value)
    }

    /// `<data-dir>/backups`. The data dir is the parent of the SQLite file
    /// derived from `DATABASE_URL` (default `data/eigenpulse.db`), falling back
    /// to `data/` for non-file URLs (e.g. `:memory:` in tests).
    fn backup_dir() -> PathBuf {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://data/eigenpulse.db?mode=rwc".into());
        let data_dir = sqlite_file_path(&url)
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| PathBuf::from("data"));
        data_dir.join("backups")
    }

    /// The newest `eigenpulse-*.db` snapshot under [`backup_dir`], with its
    /// size in bytes. Ordering is by filename (the `vN` user_version suffix is
    /// monotonic), then by modified time as a tiebreak.
    async fn latest_backup() -> anyhow::Result<Option<(PathBuf, i64)>> {
        let dir = backup_dir();
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let mut best: Option<(PathBuf, i64, std::time::SystemTime)> = None;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let is_snapshot = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("eigenpulse-") && n.ends_with(".db"));
            if !is_snapshot {
                continue;
            }
            let meta = entry.metadata().await?;
            let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            let bytes = meta.len() as i64;
            let better = match &best {
                None => true,
                Some((bp, _, bm)) => (path.as_path(), modified) > (bp.as_path(), *bm),
            };
            if better {
                best = Some((path, bytes, modified));
            }
        }
        Ok(best.map(|(p, b, _)| (p, b)))
    }

    /// Mirror of `ep_db`'s private `sqlite_file_path`: extract the on-disk file
    /// path from a `sqlite://` URL, or `None` for `:memory:` / non-sqlite URLs.
    fn sqlite_file_path(url: &str) -> Option<PathBuf> {
        let rest = url
            .strip_prefix("sqlite://")
            .or_else(|| url.strip_prefix("sqlite:"))?;
        let path = rest.split('?').next().unwrap_or(rest);
        if path.is_empty() || path == ":memory:" {
            return None;
        }
        Some(PathBuf::from(path))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use sqlx::sqlite::SqliteConnectOptions;
        use std::str::FromStr;

        async fn seeded_pool() -> SqlitePool {
            let pool = SqlitePool::connect("sqlite::memory:").await.expect("pool");
            // Minimal subset of the real schema with a secret column to prove
            // SECRET_COLUMNS filtering.
            sqlx::query(
                "CREATE TABLE pat (id INTEGER PRIMARY KEY, prefix TEXT, hash TEXT NOT NULL)",
            )
            .execute(&pool)
            .await
            .expect("pat table");
            sqlx::query(
                "CREATE TABLE notify_channel (id INTEGER PRIMARY KEY, kind TEXT, config_json TEXT)",
            )
            .execute(&pool)
            .await
            .expect("channel table");
            sqlx::query("INSERT INTO notify_channel (kind, config_json) VALUES ('smtp', '{\"password\":\"secret\"}')")
                .execute(&pool)
                .await
                .expect("insert channel");
            pool
        }

        #[test]
        fn sqlite_file_path_parses_and_ignores_memory() {
            assert_eq!(
                sqlite_file_path("sqlite://data/eigenpulse.db?mode=rwc"),
                Some(PathBuf::from("data/eigenpulse.db"))
            );
            assert_eq!(
                sqlite_file_path("sqlite:///data/eigenpulse.db"),
                Some(PathBuf::from("/data/eigenpulse.db"))
            );
            assert_eq!(sqlite_file_path("sqlite::memory:"), None);
            assert_eq!(sqlite_file_path("postgres://x"), None);
        }

        #[tokio::test]
        async fn export_table_drops_secret_columns() {
            let pool = seeded_pool().await;
            let rows = export_table(&pool, "notify_channel").await.expect("export");
            assert_eq!(rows.len(), 1);
            let obj = rows[0].as_object().expect("object");
            assert!(obj.contains_key("kind"));
            assert!(!obj.contains_key("config_json"), "secret column leaked");
        }

        #[tokio::test]
        async fn sqlite_value_to_json_handles_types_and_null() {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("t.db");
            let url = format!("sqlite://{}", path.display());
            let opts = SqliteConnectOptions::from_str(&url)
                .expect("opts")
                .create_if_missing(true);
            let pool = SqlitePool::connect_with(opts).await.expect("pool");
            sqlx::query("CREATE TABLE t (i INTEGER, r REAL, s TEXT, n TEXT)")
                .execute(&pool)
                .await
                .expect("create");
            sqlx::query("INSERT INTO t (i, r, s, n) VALUES (42, 1.5, 'hi', NULL)")
                .execute(&pool)
                .await
                .expect("insert");
            let rows = export_table(&pool, "t").await.expect("export");
            let obj = rows[0].as_object().expect("object");
            assert_eq!(obj["i"], serde_json::json!(42));
            assert_eq!(obj["r"], serde_json::json!(1.5));
            assert_eq!(obj["s"], serde_json::json!("hi"));
            assert_eq!(obj["n"], serde_json::Value::Null);
        }
    }
}
