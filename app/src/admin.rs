//! Owner-only status and portable backup operations.
//!
//! A backup is a complete `.epbackup` archive containing the current SQLite
//! database and the Fitness media object store. Restore is intentionally not
//! exposed over HTTP: it is an offline command handled by `main --restore`.

use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminStatus {
    pub version: String,
    pub db_size_bytes: i64,
    pub integrity_ok: bool,
    pub session_count: i64,
    pub notification_count: i64,
    /// Filename only. Host filesystem paths are never sent to the browser.
    pub last_backup_path: Option<String>,
    pub last_backup_exists: bool,
    pub last_backup_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub path: String,
    pub bytes: i64,
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

/// Stream the newest portable archive. Cookie-session middleware protects this
/// route; the response is never cached and does not expose its host path.
#[cfg(feature = "ssr")]
pub async fn download_latest(
    axum::extract::State(state): axum::extract::State<ep_core::AppState>,
) -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{header, HeaderValue, StatusCode};
    use axum::response::IntoResponse as _;
    use tokio_util::io::ReaderStream;

    let latest = match ssr::latest_backup(&state.db).await {
        Ok(Some(backup)) => backup,
        Ok(None) => return (StatusCode::NOT_FOUND, "no backup available").into_response(),
        Err(error) => {
            tracing::warn!(%error, "failed to locate latest portable backup");
            return (StatusCode::INTERNAL_SERVER_ERROR, "backup unavailable").into_response();
        }
    };
    let file = match tokio::fs::File::open(&latest.path).await {
        Ok(file) => file,
        Err(error) => {
            tracing::warn!(%error, "failed to open latest portable backup");
            return (StatusCode::INTERNAL_SERVER_ERROR, "backup unavailable").into_response();
        }
    };

    let mut response = Body::from_stream(ReaderStream::new(file)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.eigenpulse.backup+zip"),
    );
    let disposition = format!("attachment; filename=\"{}\"", latest.filename);
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(feature = "ssr")]
mod ssr {
    use super::{AdminStatus, BackupInfo};
    use ep_core::server_err;
    use leptos::server_fn::ServerFnError;
    use sqlx::SqlitePool;
    use std::path::{Path, PathBuf};

    #[derive(Debug)]
    pub struct BackupFile {
        pub path: PathBuf,
        pub filename: String,
        pub bytes: i64,
    }

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
        let latest = latest_backup(pool).await.map_err(server_err)?;

        Ok(AdminStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            db_size_bytes,
            integrity_ok,
            session_count,
            notification_count,
            last_backup_path: latest.as_ref().map(|item| item.filename.clone()),
            last_backup_exists: latest.is_some(),
            last_backup_bytes: latest.as_ref().map(|item| item.bytes),
        })
    }

    pub async fn run_backup(pool: &SqlitePool) -> Result<BackupInfo, ServerFnError> {
        // The shared lock keeps file publication/deletion from racing the
        // archive's media traversal.
        let _media_guard = ep_core::module_data_lock().lock().await;
        ep_fitness::validate_media_store(pool)
            .await
            .map_err(server_err)?;
        let directory = backup_dir(pool).await.map_err(server_err)?;
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(server_err)?;
        let dest = ep_db::backup::unique_snapshot_path(&directory, "eigenpulse", "epbackup");
        let media = ep_core::module_data_root().join("fitness/media/objects");
        ep_db::backup::create_epbackup(pool, &media, &dest)
            .await
            .map_err(server_err)?;
        if let Err(error) =
            ep_db::backup::prune_snapshots(&directory, "eigenpulse", "epbackup", 10).await
        {
            // The new archive is already durably published. Retention cleanup
            // is best-effort and must not turn a successful backup into an
            // error response that encourages duplicate retries.
            tracing::warn!(%error, "portable backup succeeded but old backup pruning failed");
        }
        let bytes = tokio::fs::metadata(&dest).await.map_err(server_err)?.len() as i64;
        let filename = dest
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("eigenpulse.epbackup")
            .to_string();
        Ok(BackupInfo {
            path: filename,
            bytes,
        })
    }

    async fn backup_dir(pool: &SqlitePool) -> anyhow::Result<PathBuf> {
        let database: String =
            sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
                .fetch_one(pool)
                .await?;
        if database.is_empty() {
            anyhow::bail!("portable backup requires a file-backed SQLite database");
        }
        let database = PathBuf::from(database);
        Ok(database
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .join("backups"))
    }

    pub async fn latest_backup(pool: &SqlitePool) -> anyhow::Result<Option<BackupFile>> {
        let directory = backup_dir(pool).await?;
        let mut entries = match tokio::fs::read_dir(&directory).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut candidates = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let filename = entry.file_name().to_string_lossy().into_owned();
            if !filename.starts_with("eigenpulse-") || !filename.ends_with(".epbackup") {
                continue;
            }
            let metadata = tokio::fs::symlink_metadata(entry.path()).await?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                continue;
            }
            candidates.push(BackupFile {
                path: entry.path(),
                filename,
                bytes: metadata.len() as i64,
            });
        }
        candidates.sort_by(|left, right| left.filename.cmp(&right.filename));
        Ok(candidates.pop())
    }

    #[cfg(test)]
    mod tests {
        use super::latest_backup;

        #[tokio::test]
        async fn latest_backup_ignores_unmanaged_files() {
            let temp = tempfile::tempdir().unwrap();
            let db = temp.path().join("eigenpulse.db");
            let url = format!("sqlite://{}?mode=rwc", db.display());
            let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
            let backups = temp.path().join("backups");
            tokio::fs::create_dir(&backups).await.unwrap();
            tokio::fs::write(backups.join("notes.txt"), b"ignored")
                .await
                .unwrap();
            tokio::fs::write(backups.join("eigenpulse-0002.epbackup"), b"two")
                .await
                .unwrap();
            tokio::fs::write(backups.join("eigenpulse-0001.epbackup"), b"one")
                .await
                .unwrap();

            let latest = latest_backup(&pool).await.unwrap().unwrap();
            assert_eq!(latest.filename, "eigenpulse-0002.epbackup");
            assert_eq!(latest.bytes, 3);
        }
    }
}
