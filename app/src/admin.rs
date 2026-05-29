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

/// Per-table row counts written by a successful [`import_all`], keyed by table
/// name. Returned so the UI can confirm what landed. No secrets — just counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSummary {
    /// `table name -> rows inserted`, sorted by the import (forward FK) order.
    pub tables: std::collections::BTreeMap<String, i64>,
    /// Total rows inserted across all tables.
    pub total_rows: i64,
}

/// Import a previously-exported [`DataExport`], **replacing all user data**.
///
/// This is a destructive whole-database restore: every domain table is wiped
/// and repopulated from `payload` inside a single transaction. A failure at any
/// point rolls the whole thing back, so the database is never left half-imported.
/// See [`ssr::import_all`] for the validation/ordering/seq-reconciliation rules.
#[server(ImportAllFn, "/api/_internal/admin", "Url", "import_all")]
pub async fn import_all(payload: DataExport) -> Result<ImportSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        ssr::import_all(&state.db, payload).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = payload;
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
mod ssr {
    use super::{AdminStatus, BackupInfo, DataExport, ImportSummary};
    use ep_core::server_err;
    use leptos::server_fn::ServerFnError;
    use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};
    use std::collections::{BTreeMap, BTreeSet};
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

    /// Tables that are EXPORTED but whose live rows are PRESERVED across an
    /// import (neither deleted nor re-inserted), because the export strips a
    /// `NOT NULL` secret column that makes a faithful re-insert impossible — and
    /// because clobbering them would harm the running install:
    /// - `app_user.password_hash` is `NOT NULL` with no default; re-inserting a
    ///   secret-stripped row would both violate the constraint AND lock the
    ///   currently-authenticated owner out. The live single-owner row is kept.
    /// - `notify_channel.config_json` is `NOT NULL` and secret; re-inserting
    ///   without it is impossible, so existing channels are preserved as-is.
    const IMPORT_PRESERVE_TABLES: &[&str] = &["app_user", "notify_channel"];

    /// Tables that are wiped during an import but NOT re-inserted from the
    /// payload. `notify_delivery` is a regenerable audit log carrying an FK to
    /// the preserved `notify_channel(id)`; importing the source db's delivery
    /// rows against this install's differently-keyed channels would dangle, so
    /// the log is simply cleared (its `notification_id` FK would be invalidated
    /// by the notification wipe anyway).
    const IMPORT_NO_INSERT_TABLES: &[&str] = &["notify_delivery"];

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

    /// Restore a [`DataExport`], replacing all user data inside a single
    /// transaction.
    ///
    /// ## Validation (before any write)
    /// 1. The export `version` must share our **major** version (`0.x` ↔ `0.y`
    ///    are compatible; `1.x` is not). Mismatch → localized error, no writes.
    /// 2. Every table key in the payload must be a known [`EXPORT_TABLES`] name.
    ///    An unknown table → localized error (a foreign / hand-edited file).
    ///
    /// ## Ordering & FK safety
    /// We run with `PRAGMA foreign_keys` left at the connection default. To keep
    /// per-row FK checks satisfied we DELETE the existing domain rows in
    /// **reverse** [`EXPORT_TABLES`] order (children before parents), then INSERT
    /// the imported rows in **forward** order (parents before children).
    /// [`EXPORT_TABLES`] is already declared parent-before-child.
    ///
    /// ## Column safety
    /// INSERT statements are built dynamically from each row object's keys, but
    /// the table names are fixed crate-internal literals (injection-safe). Column
    /// names from the export are validated against the live table's real columns
    /// (`PRAGMA table_info`); unknown columns (and the [`SECRET_COLUMNS`], which
    /// the export omits anyway) are skipped. Column **identifiers** are quoted
    /// with `"…"` so they can't break out of the statement either.
    ///
    /// ## seq reconciliation
    /// `seq` is intentionally **not** in [`EXPORT_TABLES`] (it is infra, and the
    /// running schema already seeds baseline counters). After importing we scan
    /// every imported string value for doc-id-shaped tokens (`FIN-26092`,
    /// `FIT-S-0412`, …), decompose each into its `(module, kind, serial)`, and
    /// raise the matching `seq.last_value` to **at least** the max serial we saw.
    /// Scanning every string column (rather than a hardcoded doc-id column list)
    /// is deliberately conservative: over-counting a non-doc-id string merely
    /// skips a few serials (harmless), whereas missing a real doc_id would let a
    /// future insert collide. Counters are only ever raised, never lowered.
    pub async fn import_all(
        pool: &SqlitePool,
        payload: DataExport,
    ) -> Result<ImportSummary, ServerFnError> {
        // 1. Version compatibility — same major component.
        let our_version = env!("CARGO_PKG_VERSION");
        if major_of(&payload.version) != major_of(our_version) {
            return Err(ep_i18n::err_with(
                "app.admin.err_import_version",
                &payload.version,
            ));
        }

        // 2. Only known tables may appear.
        let known: BTreeSet<&str> = EXPORT_TABLES.iter().copied().collect();
        for table in payload.tables.keys() {
            if !known.contains(table.as_str()) {
                return Err(ep_i18n::err_with(
                    "app.admin.err_import_unknown_table",
                    table,
                ));
            }
        }

        let mut tx = pool.begin().await.map_err(server_err)?;

        // 3a. Wipe existing domain rows children-first (reverse FK order),
        // skipping only the preserve-list (secret-bearing / credential tables).
        for &table in EXPORT_TABLES.iter().rev() {
            if IMPORT_PRESERVE_TABLES.contains(&table) {
                continue;
            }
            sqlx::query(&format!("DELETE FROM {table}"))
                .execute(&mut *tx)
                .await
                .map_err(server_err)?;
        }

        // 3b. Re-insert parents-first (forward FK order). Track doc-id serials
        // for seq reconciliation as we go.
        let mut counts: BTreeMap<String, i64> = BTreeMap::new();
        let mut total_rows: i64 = 0;
        let mut max_serials: BTreeMap<(String, String), i64> = BTreeMap::new();
        for &table in EXPORT_TABLES {
            if IMPORT_PRESERVE_TABLES.contains(&table) || IMPORT_NO_INSERT_TABLES.contains(&table) {
                continue;
            }
            let Some(rows) = payload.tables.get(table) else {
                continue;
            };
            let live_cols = table_columns(&mut tx, table).await?;
            let mut inserted: i64 = 0;
            for row in rows {
                let obj = row
                    .as_object()
                    .ok_or_else(|| ep_i18n::err_with("app.admin.err_import_bad_row", table))?;
                insert_row(&mut tx, table, obj, &live_cols).await?;
                collect_doc_serials(obj, &mut max_serials);
                inserted += 1;
            }
            counts.insert(table.to_string(), inserted);
            total_rows += inserted;
        }

        // 4. Reconcile seq counters: raise each touched (module, kind) to the
        // max serial observed. INSERT-or-raise so a fresh counter is created and
        // an existing one is only ever bumped up (MAX), never lowered.
        for ((module, kind), serial) in &max_serials {
            sqlx::query(
                r#"
                INSERT INTO seq(module, kind, last_value) VALUES (?1, ?2, ?3)
                ON CONFLICT(module, kind)
                    DO UPDATE SET last_value = MAX(last_value, excluded.last_value)
                "#,
            )
            .bind(module)
            .bind(kind)
            .bind(serial)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
        }

        tx.commit().await.map_err(server_err)?;

        Ok(ImportSummary {
            tables: counts,
            total_rows,
        })
    }

    /// Major version component (`"0.1.0"` → `0`). Non-numeric / empty → `0`.
    fn major_of(version: &str) -> u64 {
        version
            .split('.')
            .next()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    /// The live column names of `table`, via `PRAGMA table_info`. `table` is a
    /// fixed crate-internal literal, so the interpolation is injection-safe.
    async fn table_columns(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        table: &str,
    ) -> Result<BTreeSet<String>, ServerFnError> {
        let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(&mut **tx)
            .await
            .map_err(server_err)?;
        let mut cols = BTreeSet::new();
        for row in &rows {
            let name: String = row.try_get("name").map_err(server_err)?;
            cols.insert(name);
        }
        Ok(cols)
    }

    /// INSERT one exported row object into `table`. Column names are validated
    /// against `live_cols` (unknown / secret columns are silently skipped — the
    /// export omits secrets, so their absence is tolerated). Identifiers are
    /// double-quoted; values are bound by JSON storage class so SQLite stores
    /// the same class it exported (mirror of [`sqlite_value_to_json`]).
    async fn insert_row(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        table: &str,
        obj: &serde_json::Map<String, serde_json::Value>,
        live_cols: &BTreeSet<String>,
    ) -> Result<(), ServerFnError> {
        let mut columns: Vec<&str> = Vec::with_capacity(obj.len());
        let mut values: Vec<&serde_json::Value> = Vec::with_capacity(obj.len());
        for (name, value) in obj {
            if SECRET_COLUMNS.contains(&name.as_str()) || !live_cols.contains(name) {
                continue;
            }
            columns.push(name.as_str());
            values.push(value);
        }
        if columns.is_empty() {
            // Nothing to write for this row (all columns unknown/secret). Skip
            // rather than emit an invalid `INSERT INTO t () VALUES ()`.
            return Ok(());
        }

        let col_list = columns
            .iter()
            .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholders = (1..=columns.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("INSERT INTO {table} ({col_list}) VALUES ({placeholders})");

        let mut q = sqlx::query(&sql);
        for value in values {
            q = match value {
                serde_json::Value::Null => q.bind(Option::<String>::None),
                serde_json::Value::Bool(b) => q.bind(*b as i64),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        q.bind(i)
                    } else if let Some(f) = n.as_f64() {
                        q.bind(f)
                    } else {
                        // u64 too large for i64: store as text (lossless).
                        q.bind(n.to_string())
                    }
                }
                serde_json::Value::String(s) => q.bind(s.clone()),
                // Arrays/objects never appear in an export cell, but bind them
                // as their JSON text rather than failing the whole restore.
                other => q.bind(other.to_string()),
            };
        }
        q.execute(&mut **tx).await.map_err(server_err)?;
        Ok(())
    }

    /// Scan every string value of an imported row for doc-id-shaped tokens and
    /// fold each into `max_serials` keyed by the `(module, seq-kind)` it implies.
    fn collect_doc_serials(
        obj: &serde_json::Map<String, serde_json::Value>,
        max_serials: &mut BTreeMap<(String, String), i64>,
    ) {
        for value in obj.values() {
            let serde_json::Value::String(s) = value else {
                continue;
            };
            if let Some((module, kind, serial)) = decompose_doc_id(s) {
                let entry = max_serials.entry((module, kind)).or_insert(0);
                *entry = (*entry).max(serial);
            }
        }
    }

    /// Decompose a doc-id string into `(module, seq-kind, serial)`, mirroring
    /// [`ep_core::DocIdShape`]'s `format_doc_id` / `sequence_kind`:
    /// - `FIN-26092` (prefix + one numeric segment) → `YearSerial5`: the leading
    ///   two digits are the year `YY` (kind `doc:yYY`), the rest is the serial.
    /// - `FIT-S-0412` (prefix + alpha type + numeric segment) → `TypeSerial4`:
    ///   kind `type:S`, last segment is the serial.
    ///
    /// Returns `None` for anything that isn't one of those two shapes, so
    /// arbitrary strings (codes, tags, notes) never inflate a counter.
    fn decompose_doc_id(s: &str) -> Option<(String, String, i64)> {
        let s = ep_core::safe_doc_id(s)?;
        let parts: Vec<&str> = s.split('-').collect();
        match parts.as_slice() {
            // YearSerial5: PREFIX-{YY}{NNN}. Need at least 3 digits (YY + serial).
            [module, tail] if tail.len() >= 3 && tail.bytes().all(|b| b.is_ascii_digit()) => {
                let yy = &tail[..2];
                let serial: i64 = tail[2..].parse().ok()?;
                Some((module.to_string(), format!("doc:y{yy}"), serial))
            }
            // TypeSerial4: PREFIX-{TYPE}-{NNNN}.
            [module, kind, tail]
                if !kind.is_empty()
                    && kind.bytes().all(|b| b.is_ascii_uppercase())
                    && !tail.is_empty()
                    && tail.bytes().all(|b| b.is_ascii_digit()) =>
            {
                let serial: i64 = tail.parse().ok()?;
                Some((module.to_string(), format!("type:{kind}"), serial))
            }
            _ => None,
        }
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

        #[test]
        fn decompose_doc_id_recognizes_both_shapes() {
            assert_eq!(
                decompose_doc_id("FIN-26092"),
                Some(("FIN".into(), "doc:y26".into(), 92))
            );
            assert_eq!(
                decompose_doc_id("FIT-S-0412"),
                Some(("FIT".into(), "type:S".into(), 412))
            );
            assert_eq!(
                decompose_doc_id("LRN-B-0014"),
                Some(("LRN".into(), "type:B".into(), 14))
            );
            // Widened serials past the nominal width still decompose.
            assert_eq!(
                decompose_doc_id("FIN-261000"),
                Some(("FIN".into(), "doc:y26".into(), 1000))
            );
            // Non-doc-id strings never inflate a counter.
            assert_eq!(decompose_doc_id("Blue Bottle"), None);
            assert_eq!(decompose_doc_id("CNY"), None);
        }

        /// Build the real domain schema (the slice that `import_all` touches)
        /// on a fresh on-disk SQLite db with `foreign_keys = ON`, so the
        /// reverse-delete / forward-insert ordering is genuinely exercised.
        async fn schema_pool() -> (tempfile::TempDir, SqlitePool) {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("ep.db");
            let url = format!("sqlite://{}", path.display());
            let opts = SqliteConnectOptions::from_str(&url)
                .expect("opts")
                .create_if_missing(true)
                .foreign_keys(true);
            let pool = SqlitePool::connect_with(opts).await.expect("pool");
            for sql in [
                "CREATE TABLE app_user (id INTEGER PRIMARY KEY CHECK (id=1), handle TEXT NOT NULL, \
                    password_hash TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT 0)",
                "CREATE TABLE notify_channel (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, \
                    config_json TEXT NOT NULL)",
                "CREATE TABLE notify_delivery (id INTEGER PRIMARY KEY AUTOINCREMENT, channel_id INTEGER \
                    NOT NULL REFERENCES notify_channel(id))",
                "CREATE TABLE module_link (source_doc TEXT, target_doc TEXT)",
                "CREATE TABLE activity (id INTEGER PRIMARY KEY AUTOINCREMENT, doc_id TEXT NOT NULL, \
                    summary TEXT NOT NULL)",
                "CREATE TABLE notification (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL)",
                "CREATE TABLE fin_currency (code TEXT PRIMARY KEY, symbol TEXT NOT NULL)",
                "CREATE TABLE fin_account (currency_code TEXT NOT NULL REFERENCES fin_currency(code), \
                    code TEXT NOT NULL, name TEXT NOT NULL, PRIMARY KEY (currency_code, code))",
                "CREATE TABLE fin_category (currency_code TEXT NOT NULL REFERENCES fin_currency(code), \
                    code TEXT NOT NULL, name TEXT NOT NULL, PRIMARY KEY (currency_code, code))",
                "CREATE TABLE fin_txn (doc_id TEXT PRIMARY KEY, currency_code TEXT NOT NULL, \
                    account_code TEXT NOT NULL, category_code TEXT NOT NULL, amount TEXT NOT NULL, \
                    FOREIGN KEY (currency_code, account_code) REFERENCES fin_account(currency_code, code), \
                    FOREIGN KEY (currency_code, category_code) REFERENCES fin_category(currency_code, code))",
                "CREATE TABLE fin_budget (currency_code TEXT NOT NULL, period TEXT NOT NULL, \
                    category_code TEXT NOT NULL, amount TEXT NOT NULL, \
                    PRIMARY KEY (currency_code, period, category_code))",
                "CREATE TABLE fit_workout (doc_id TEXT PRIMARY KEY, duration_m INTEGER NOT NULL)",
                "CREATE TABLE fit_set (id INTEGER PRIMARY KEY AUTOINCREMENT, workout_doc TEXT NOT NULL \
                    REFERENCES fit_workout(doc_id))",
                "CREATE TABLE lrn_course (doc_id TEXT PRIMARY KEY, name TEXT NOT NULL)",
                "CREATE TABLE lrn_book (doc_id TEXT PRIMARY KEY, name TEXT NOT NULL)",
                "CREATE TABLE lrn_note (doc_id TEXT PRIMARY KEY, title TEXT NOT NULL, \
                    course_doc TEXT REFERENCES lrn_course(doc_id))",
                "CREATE TABLE seq (module TEXT NOT NULL, kind TEXT NOT NULL, last_value INTEGER NOT NULL, \
                    PRIMARY KEY (module, kind))",
            ] {
                sqlx::query(sql).execute(&pool).await.expect("create schema");
            }
            (dir, pool)
        }

        /// Seed a representative dataset: a logged-in owner (with a real
        /// password hash), a secret notify channel, and FK-linked finance /
        /// fitness / learning rows whose doc-ids the seq must learn from.
        async fn seed_domain(pool: &SqlitePool) {
            for sql in [
                "INSERT INTO app_user (id, handle, password_hash) VALUES (1, 'leo', 'argon2-hash')",
                "INSERT INTO notify_channel (kind, config_json) VALUES ('smtp', '{\"password\":\"s3cret\"}')",
                "INSERT INTO notify_delivery (channel_id) VALUES (1)",
                "INSERT INTO notification (title) VALUES ('hello')",
                "INSERT INTO activity (doc_id, summary) VALUES ('FIN-26092', 'coffee')",
                "INSERT INTO fin_currency (code, symbol) VALUES ('CNY', '¥')",
                "INSERT INTO fin_account (currency_code, code, name) VALUES ('CNY', 'CASH', 'Cash')",
                "INSERT INTO fin_category (currency_code, code, name) VALUES ('CNY', 'FOOD', 'Food')",
                "INSERT INTO fin_txn (doc_id, currency_code, account_code, category_code, amount) \
                    VALUES ('FIN-26092', 'CNY', 'CASH', 'FOOD', '1250')",
                "INSERT INTO fit_workout (doc_id, duration_m) VALUES ('FIT-S-0412', 45)",
                "INSERT INTO lrn_course (doc_id, name) VALUES ('LRN-C-0011', 'Rust')",
                "INSERT INTO lrn_note (doc_id, title, course_doc) VALUES ('LRN-N-0221', 'note', 'LRN-C-0011')",
                // Baseline seq counters lower than the imported serials, to prove
                // they get raised.
                "INSERT INTO seq (module, kind, last_value) VALUES ('FIN', 'doc:y26', 1)",
                "INSERT INTO seq (module, kind, last_value) VALUES ('FIT', 'type:S', 1)",
            ] {
                sqlx::query(sql).execute(pool).await.expect("seed");
            }
        }

        async fn count(pool: &SqlitePool, table: &str) -> i64 {
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(pool)
                .await
                .expect("count")
        }

        async fn seq_value(pool: &SqlitePool, module: &str, kind: &str) -> i64 {
            sqlx::query_scalar("SELECT last_value FROM seq WHERE module = ?1 AND kind = ?2")
                .bind(module)
                .bind(kind)
                .fetch_one(pool)
                .await
                .expect("seq")
        }

        #[tokio::test]
        async fn import_round_trips_and_reconciles_seq() {
            // Source: full dataset → export.
            let (_src_dir, src) = schema_pool().await;
            seed_domain(&src).await;
            let export = export_all(&src).await.expect("export");

            // The export must already be secret-free (config_json / password_hash
            // / hash never appear). app_user IS exported, but without its hash.
            let user_rows = &export.tables["app_user"];
            assert!(!user_rows[0]
                .as_object()
                .unwrap()
                .contains_key("password_hash"));
            assert!(!export.tables["notify_channel"][0]
                .as_object()
                .unwrap()
                .contains_key("config_json"));

            // Target: a DIFFERENT fresh db with its own owner + channel, and a
            // single stale finance row that the import must replace.
            let (_dst_dir, dst) = schema_pool().await;
            sqlx::query(
                "INSERT INTO app_user (id, handle, password_hash) VALUES (1, 'owner', 'KEEP-ME')",
            )
            .execute(&dst)
            .await
            .expect("owner");
            sqlx::query("INSERT INTO notify_channel (kind, config_json) VALUES ('bark', '{\"key\":\"keep\"}')")
                .execute(&dst)
                .await
                .expect("channel");
            sqlx::query("INSERT INTO fin_currency (code, symbol) VALUES ('USD', '$')")
                .execute(&dst)
                .await
                .expect("stale currency");

            let summary = import_all(&dst, export).await.expect("import");

            // Row counts match the source for imported tables.
            assert_eq!(count(&dst, "fin_txn").await, 1);
            assert_eq!(count(&dst, "fin_account").await, 1);
            assert_eq!(count(&dst, "fit_workout").await, 1);
            assert_eq!(count(&dst, "lrn_note").await, 1);
            assert_eq!(count(&dst, "activity").await, 1);
            // The stale USD currency was wiped; only the imported CNY remains.
            assert_eq!(count(&dst, "fin_currency").await, 1);
            let cur: String = sqlx::query_scalar("SELECT code FROM fin_currency")
                .fetch_one(&dst)
                .await
                .unwrap();
            assert_eq!(cur, "CNY");

            // Round-trip value fidelity (TEXT money column preserved verbatim).
            let amount: String =
                sqlx::query_scalar("SELECT amount FROM fin_txn WHERE doc_id='FIN-26092'")
                    .fetch_one(&dst)
                    .await
                    .unwrap();
            assert_eq!(amount, "1250");

            // The preserved tables kept the TARGET's own (secret-bearing) rows —
            // the owner is NOT locked out and the channel secret survives.
            let hash: String = sqlx::query_scalar("SELECT password_hash FROM app_user WHERE id=1")
                .fetch_one(&dst)
                .await
                .unwrap();
            assert_eq!(hash, "KEEP-ME", "owner credentials must be preserved");
            let cfg: String = sqlx::query_scalar("SELECT config_json FROM notify_channel")
                .fetch_one(&dst)
                .await
                .unwrap();
            assert_eq!(cfg, "{\"key\":\"keep\"}");

            // seq counters were raised to the imported serials, not lowered.
            assert_eq!(seq_value(&dst, "FIN", "doc:y26").await, 92);
            assert_eq!(seq_value(&dst, "FIT", "type:S").await, 412);
            assert_eq!(seq_value(&dst, "LRN", "type:C").await, 11);
            assert_eq!(seq_value(&dst, "LRN", "type:N").await, 221);

            // Summary reflects the imported (non-skipped) tables only.
            assert_eq!(summary.tables.get("fin_txn"), Some(&1));
            assert!(!summary.tables.contains_key("app_user"));
            assert!(!summary.tables.contains_key("notify_channel"));
            assert_eq!(summary.total_rows, count_imported(&summary));
        }

        fn count_imported(summary: &ImportSummary) -> i64 {
            summary.tables.values().sum()
        }

        #[tokio::test]
        async fn import_tolerates_absent_secret_columns_and_self_round_trips() {
            // Import back into the SAME db it was exported from: a no-op-ish
            // round trip that must still succeed (secret columns were stripped
            // from the export, and the importer tolerates their absence).
            let (_dir, pool) = schema_pool().await;
            seed_domain(&pool).await;
            let export = export_all(&pool).await.expect("export");
            let before = count(&pool, "fin_txn").await;
            import_all(&pool, export).await.expect("self import");
            assert_eq!(count(&pool, "fin_txn").await, before);
            // Owner + channel secrets untouched.
            assert_eq!(count(&pool, "app_user").await, 1);
            assert_eq!(count(&pool, "notify_channel").await, 1);
        }

        #[tokio::test]
        async fn import_rejects_incompatible_major_version() {
            let (_dir, pool) = schema_pool().await;
            seed_domain(&pool).await;
            let mut export = export_all(&pool).await.expect("export");
            export.version = "9.9.9".to_string();
            let err = import_all(&pool, export).await.expect_err("must reject");
            let (code, payload) = ep_i18n::parse_err(&err).expect("i18n err");
            assert_eq!(code, "app.admin.err_import_version");
            assert_eq!(payload, Some("9.9.9"));
            // Nothing was written: the original data is intact.
            assert_eq!(count(&pool, "fin_txn").await, 1);
        }

        #[tokio::test]
        async fn import_rejects_unknown_table() {
            let (_dir, pool) = schema_pool().await;
            seed_domain(&pool).await;
            let mut export = export_all(&pool).await.expect("export");
            export
                .tables
                .insert("evil_table".to_string(), vec![serde_json::json!({"x": 1})]);
            let err = import_all(&pool, export).await.expect_err("must reject");
            let (code, payload) = ep_i18n::parse_err(&err).expect("i18n err");
            assert_eq!(code, "app.admin.err_import_unknown_table");
            assert_eq!(payload, Some("evil_table"));
            assert_eq!(count(&pool, "fin_txn").await, 1);
        }
    }
}
