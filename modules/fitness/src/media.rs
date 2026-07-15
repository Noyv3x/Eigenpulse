//! Authenticated web handlers for exercise demonstration media.
//!
//! `media_router()` intentionally does not install its own session middleware:
//! the application must merge it inside the same cookie-session-protected
//! router as the Leptos pages. Mutating handlers additionally require a
//! same-origin `Origin` header to prevent cookie-based CSRF.

#![allow(
    clippy::result_large_err,
    reason = "Axum handlers use Response as their rejection type"
)]

use crate::model::{ExerciseMedia, ExerciseMediaInput, MAX_EXERCISE_MEDIA};
use crate::server_fns::{add_media_metadata_batch_inner, delete_media_metadata_inner};
use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::header::{
    ACCEPT_RANGES, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG, HOST,
    IF_NONE_MATCH, ORIGIN, RANGE,
};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use ep_core::{detect_media_format, AppState, MEDIA_FORMAT_PROBE_BYTES};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::path::{Path as FsPath, PathBuf};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;

const DEFAULT_MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
const DEFAULT_QUOTA_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// Routes exposed by the module for mounting inside the protected app router.
///
/// - `POST /fitness/media/exercises/:exercise_id`
/// - `GET|HEAD|DELETE /fitness/media/:media_id`
pub fn media_router() -> Router<AppState> {
    let request_limit = max_file_bytes()
        .saturating_mul(MAX_EXERCISE_MEDIA as u64)
        .saturating_add(64 * 1024)
        .min(usize::MAX as u64) as usize;
    Router::<AppState>::new()
        .route("/fitness/media/exercises/:exercise_id", post(upload_media))
        .route(
            "/fitness/media/:media_id",
            get(serve_media).delete(delete_media),
        )
        .route("/fitness/media/:media_id/delete", post(delete_media_form))
        .layer(DefaultBodyLimit::max(request_limit))
}

/// Verify that every media metadata row has exactly one matching regular file
/// (size, type and SHA-256) and that the object store has no untracked files.
/// Portable-backup callers must hold [`ep_core::module_data_lock`] while this
/// runs and until archive creation completes.
pub async fn validate_media_store(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    let rows: Vec<(String, i64, String, String)> =
        sqlx::query_as("SELECT object_key, byte_size, sha256, media_type FROM fit_exercise_media")
            .fetch_all(pool)
            .await?;
    let expected = rows
        .into_iter()
        .map(|(key, size, sha256, media_type)| (key, (size, sha256, media_type)))
        .collect::<std::collections::HashMap<_, _>>();
    if let Some(key) = expected.keys().find(|key| !valid_object_key(key)) {
        anyhow::bail!("invalid fitness media object key in database: {key}");
    }
    let objects = fitness_media_root().join("objects");
    if !tokio::fs::try_exists(&objects).await? {
        if expected.is_empty() {
            return Ok(());
        }
        anyhow::bail!("fitness media object directory is missing");
    }
    let metadata = tokio::fs::symlink_metadata(&objects).await?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!("fitness media object path is not a real directory");
    }

    let mut found = std::collections::HashSet::new();
    let mut removed_orphan = false;
    let mut entries = tokio::fs::read_dir(&objects).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let metadata = tokio::fs::symlink_metadata(&path).await?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            anyhow::bail!("unsupported fitness media object: {}", path.display());
        }
        let key = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("fitness media object key is not UTF-8"))?;
        let Some((size, expected_sha, expected_type)) = expected.get(&key) else {
            // A metadata-first delete or a crash between object publication and
            // COMMIT can leave an unreferenced object. The caller holds the
            // module-data lock, so reclaiming it here is race-free and keeps a
            // recoverable orphan from blocking the user's next backup.
            tokio::fs::remove_file(&path).await?;
            removed_orphan = true;
            continue;
        };
        if metadata.len() == 0 || i64::try_from(metadata.len()).ok() != Some(*size) {
            anyhow::bail!("fitness media object size mismatch: {key}");
        }
        let mut file = tokio::fs::File::open(&path).await?;
        let mut prefix = Vec::with_capacity(MEDIA_FORMAT_PROBE_BYTES);
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            if prefix.len() < MEDIA_FORMAT_PROBE_BYTES {
                let take = (MEDIA_FORMAT_PROBE_BYTES - prefix.len()).min(read);
                prefix.extend_from_slice(&buffer[..take]);
            }
            hasher.update(&buffer[..read]);
        }
        let actual_type = detect_media_format(&prefix)
            .map(|format| format.media_type())
            .ok_or_else(|| anyhow::anyhow!("unsupported fitness media signature: {key}"))?;
        if actual_type != expected_type || hex::encode(hasher.finalize()) != *expected_sha {
            anyhow::bail!("fitness media object content mismatch: {key}");
        }
        found.insert(key);
    }
    if removed_orphan {
        sync_directory_io(&objects).await?;
    }
    if found.len() != expected.len() {
        let missing = expected
            .keys()
            .find(|key| !found.contains(*key))
            .map(String::as_str)
            .unwrap_or("unknown");
        anyhow::bail!("fitness media metadata references a missing object: {missing}");
    }
    Ok(())
}

async fn upload_media(
    State(state): State<AppState>,
    Path(exercise_id): Path<i64>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Redirect, Response> {
    ensure_same_origin(&headers)?;
    if exercise_id <= 0 {
        return Err(bad_request("invalid exercise id"));
    }
    let _guard = ep_core::module_data_lock().lock().await;
    let existing_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM fit_exercise_media WHERE exercise_id = ?1",
    )
    .bind(exercise_id)
    .fetch_one(&state.db)
    .await
    .map_err(internal_error)?;
    let exercise_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fit_exercise WHERE id = ?1)")
            .bind(exercise_id)
            .fetch_one(&state.db)
            .await
            .map_err(internal_error)?;
    if !exercise_exists {
        return Err(not_found());
    }
    let remaining = MAX_EXERCISE_MEDIA.saturating_sub(existing_count);
    if remaining == 0 {
        return Err(bad_request("this exercise already has 12 media items"));
    }

    let root = fitness_media_root();
    let objects = root.join("objects");
    let staging = root.join("staging");
    ensure_private_directory(&root).await?;
    ensure_private_directory(&objects).await?;
    ensure_private_directory(&staging).await?;
    cleanup_stale_parts(&staging).await?;
    reconcile_orphan_objects(&state.db, &objects).await?;

    let used = object_usage(&objects)
        .await?
        .checked_add(object_usage(&staging).await?)
        .ok_or_else(|| internal_error("fitness media quota size overflow"))?;
    let quota_remaining = quota_bytes().saturating_sub(used);
    let (staged, title) =
        stage_multipart(multipart, &staging, remaining as usize, quota_remaining).await?;
    let prepared = staged
        .into_iter()
        .map(|staged| {
            let object_key = format!("{}.{}", staged.random_key, staged.extension);
            let final_path = objects.join(&object_key);
            PreparedUpload {
                staged,
                object_key,
                final_path,
            }
        })
        .collect::<Vec<_>>();

    for upload in &prepared {
        if let Err(error) = tokio::fs::rename(&upload.staged.path, &upload.final_path).await {
            cleanup_prepared_uploads(&prepared, &objects, &staging).await;
            return Err(internal_error(error));
        }
    }
    if let Err(error) = sync_directory(&objects).await {
        cleanup_prepared_uploads(&prepared, &objects, &staging).await;
        return Err(error);
    }
    let metadata = prepared
        .iter()
        .map(|upload| ExerciseMediaInput {
            object_key: upload.object_key.clone(),
            title: title.clone(),
            media_type: upload.staged.media_type.to_owned(),
            byte_size: upload.staged.byte_size as i64,
            sha256: upload.staged.sha256.clone(),
        })
        .collect();
    if let Err(error) = add_media_metadata_batch_inner(&state.db, exercise_id, metadata).await {
        if error.cleanup_objects {
            cleanup_prepared_uploads(&prepared, &objects, &staging).await;
        } else {
            tracing::error!(
                exercise_id,
                "preserving fitness media objects because database commit outcome is uncertain"
            );
        }
        return Err(server_fn_error(error.error));
    }
    Ok(Redirect::to("/fitness?tab=exercises"))
}

struct PreparedUpload {
    staged: StagedUpload,
    object_key: String,
    final_path: PathBuf,
}

async fn stage_multipart(
    mut multipart: Multipart,
    staging: &FsPath,
    max_items: usize,
    mut quota_remaining: u64,
) -> Result<(Vec<StagedUpload>, Option<String>), Response> {
    let mut staged = Vec::new();
    let mut title = None;
    let result = async {
        while let Some(mut field) = multipart.next_field().await.map_err(bad_multipart)? {
            match field.name() {
                Some("title") => {
                    let mut value = Vec::new();
                    while let Some(chunk) = field.chunk().await.map_err(bad_multipart)? {
                        if value.len().saturating_add(chunk.len()) > 512 {
                            return Err(bad_request("media title is too long"));
                        }
                        value.extend_from_slice(&chunk);
                    }
                    let value = String::from_utf8(value)
                        .map_err(|_| bad_request("media title must be UTF-8"))?;
                    let value = value.trim();
                    if value.chars().count() > 120 {
                        return Err(bad_request("media title is too long"));
                    }
                    title = (!value.is_empty()).then(|| value.to_owned());
                }
                Some("media") => {
                    if staged.len() >= max_items {
                        return Err(bad_request("upload would exceed 12 media items"));
                    }
                    let upload = stage_upload(staging, &mut field, quota_remaining).await?;
                    quota_remaining = quota_remaining.saturating_sub(upload.byte_size);
                    staged.push(upload);
                }
                _ => while field.chunk().await.map_err(bad_multipart)?.is_some() {},
            }
        }
        if staged.is_empty() {
            return Err(bad_request("multipart form did not contain a media file"));
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok((staged, title)),
        Err(error) => {
            cleanup_staged_uploads(&staged, staging).await;
            Err(error)
        }
    }
}

async fn cleanup_staged_uploads(uploads: &[StagedUpload], staging: &FsPath) {
    let mut removed = false;
    for upload in uploads {
        match tokio::fs::remove_file(&upload.path).await {
            Ok(()) => removed = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(%error, path = %upload.path.display(), "failed to clean staged fitness media")
            }
        }
    }
    if removed {
        if let Err(error) = sync_directory(staging).await {
            tracing::warn!("failed to sync fitness media staging cleanup: {error:?}");
        }
    }
}

async fn cleanup_prepared_uploads(uploads: &[PreparedUpload], objects: &FsPath, staging: &FsPath) {
    let mut removed_object = false;
    let mut removed_staged = false;
    for upload in uploads {
        match tokio::fs::remove_file(&upload.final_path).await {
            Ok(()) => removed_object = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(%error, path = %upload.final_path.display(), "failed to compensate published fitness media")
            }
        }
        match tokio::fs::remove_file(&upload.staged.path).await {
            Ok(()) => removed_staged = true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(%error, path = %upload.staged.path.display(), "failed to clean staged fitness media")
            }
        }
    }
    if removed_object {
        let _ = sync_directory(objects).await;
    }
    if removed_staged {
        let _ = sync_directory(staging).await;
    }
}

struct StagedUpload {
    path: PathBuf,
    random_key: String,
    media_type: &'static str,
    extension: &'static str,
    byte_size: u64,
    sha256: String,
}

async fn stage_upload(
    staging: &FsPath,
    field: &mut axum::extract::multipart::Field<'_>,
    quota_remaining: u64,
) -> Result<StagedUpload, Response> {
    let mut random = [0_u8; 24];
    rand::rngs::OsRng.fill_bytes(&mut random);
    let random_key = hex::encode(random);
    let path = staging.join(format!("{random_key}.part"));
    let mut file = create_private_file(&path).await.map_err(internal_error)?;
    let max_file = max_file_bytes();
    let allowed = max_file.min(quota_remaining);
    if allowed == 0 {
        let _ = tokio::fs::remove_file(&path).await;
        return Err(payload_too_large("fitness media quota is full"));
    }
    let mut total = 0_u64;
    let mut prefix = Vec::with_capacity(MEDIA_FORMAT_PROBE_BYTES);
    let mut hasher = Sha256::new();
    loop {
        let chunk = match field.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                drop(file);
                let _ = tokio::fs::remove_file(&path).await;
                return Err(bad_multipart(error));
            }
        };
        total = total.saturating_add(chunk.len() as u64);
        if total > allowed {
            drop(file);
            let _ = tokio::fs::remove_file(&path).await;
            let message = if total > max_file {
                "media file exceeds EP_FITNESS_MEDIA_MAX_FILE_BYTES"
            } else {
                "fitness media quota would be exceeded"
            };
            return Err(payload_too_large(message));
        }
        if prefix.len() < MEDIA_FORMAT_PROBE_BYTES {
            let take = (MEDIA_FORMAT_PROBE_BYTES - prefix.len()).min(chunk.len());
            prefix.extend_from_slice(&chunk[..take]);
        }
        hasher.update(&chunk);
        if let Err(error) = file.write_all(&chunk).await {
            drop(file);
            let _ = tokio::fs::remove_file(&path).await;
            return Err(internal_error(error));
        }
    }
    if total == 0 {
        drop(file);
        let _ = tokio::fs::remove_file(&path).await;
        return Err(bad_request("media file is empty"));
    }
    let format = match detect_media_format(&prefix) {
        Some(format) => format,
        None => {
            drop(file);
            let _ = tokio::fs::remove_file(&path).await;
            return Err(bad_request(
                "only genuine GIF, MP4, and WebM files are accepted",
            ));
        }
    };
    if let Err(error) = file.sync_all().await {
        drop(file);
        let _ = tokio::fs::remove_file(&path).await;
        return Err(internal_error(error));
    }
    drop(file);
    Ok(StagedUpload {
        path,
        random_key,
        media_type: format.media_type(),
        extension: format.extension(),
        byte_size: total,
        sha256: hex::encode(hasher.finalize()),
    })
}

async fn serve_media(
    State(state): State<AppState>,
    Path(media_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    let media = media_row(&state.db, media_id).await?;
    let path = safe_object_path(&fitness_media_root().join("objects"), &media.object_key)?;
    let object_metadata = tokio::fs::symlink_metadata(&path).await.map_err(|error| {
        tracing::warn!(error = %error, media_id, "fitness media object missing");
        not_found()
    })?;
    if !object_metadata.file_type().is_file() || object_metadata.file_type().is_symlink() {
        tracing::error!(media_id, path = %path.display(), "refusing non-regular fitness media object");
        return Err(not_found());
    }
    let mut file = tokio::fs::File::open(&path).await.map_err(|error| {
        tracing::warn!(error = %error, media_id, "fitness media object missing");
        not_found()
    })?;
    let size = file.metadata().await.map_err(internal_error)?.len();
    if size == 0 || i64::try_from(size).ok() != Some(media.byte_size) {
        tracing::error!(
            media_id,
            expected = media.byte_size,
            actual = size,
            "fitness media object size mismatch"
        );
        return Err(not_found());
    }
    let etag = format!("\"{}\"", media.sha256);
    if headers
        .get(IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == etag)
    {
        return Ok(Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(ETAG, etag)
            .body(Body::empty())
            .expect("valid 304 response"));
    }
    let range = match headers.get(RANGE).and_then(|value| value.to_str().ok()) {
        Some(value) => match parse_range(value, size) {
            Some(range) => Some(range),
            None => {
                return Ok(Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(CONTENT_RANGE, format!("bytes */{size}"))
                    .body(Body::empty())
                    .expect("valid range response"));
            }
        },
        None => None,
    };
    let (status, start, end) = range
        .map(|(start, end)| (StatusCode::PARTIAL_CONTENT, start, end))
        .unwrap_or((StatusCode::OK, 0, size - 1));
    file.seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(internal_error)?;
    let length = end - start + 1;
    let stream = ReaderStream::new(file.take(length));
    let mut builder = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, content_type(&media.media_type))
        .header(CONTENT_LENGTH, length)
        .header(ACCEPT_RANGES, "bytes")
        .header(ETAG, etag)
        .header(CACHE_CONTROL, "private, no-cache, no-transform")
        .header("x-content-type-options", "nosniff");
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
    }
    Ok(builder
        .body(Body::from_stream(stream))
        .expect("valid media response"))
}

async fn delete_media(
    State(state): State<AppState>,
    Path(media_id): Path<i64>,
    headers: HeaderMap,
) -> Result<StatusCode, Response> {
    ensure_same_origin(&headers)?;
    let _guard = ep_core::module_data_lock().lock().await;
    let media = media_row(&state.db, media_id).await?;
    let objects = fitness_media_root().join("objects");
    let path = safe_object_path(&objects, &media.object_key)?;
    // Commit metadata removal first. A crash afterwards can only leave an
    // unreferenced object (reclaimed on the next mutation), never a live DB row
    // pointing at a file that was moved away before COMMIT.
    delete_media_metadata_inner(&state.db, media.exercise_id, media_id)
        .await
        .map_err(server_fn_error)?;
    match tokio::fs::remove_file(&path).await {
        Ok(()) => sync_directory(&objects).await?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(%error, media_id, path = %path.display(), "media metadata deleted but object cleanup was deferred");
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Plain HTML forms cannot issue DELETE. This same-origin POST alias keeps
/// media management usable without adding a browser-side fetch implementation.
async fn delete_media_form(
    State(state): State<AppState>,
    Path(media_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Redirect, Response> {
    delete_media(State(state), Path(media_id), headers).await?;
    Ok(Redirect::to("/fitness?tab=exercises"))
}

async fn media_row(pool: &sqlx::SqlitePool, id: i64) -> Result<ExerciseMedia, Response> {
    sqlx::query_as(
        "SELECT id, exercise_id, object_key, title, media_type, byte_size,
                sha256, sort_order, created_at
           FROM fit_exercise_media WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(not_found)
}

fn parse_range(value: &str, size: u64) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;
    if value.contains(',') {
        return None;
    }
    let (start, end) = value.split_once('-')?;
    if start.is_empty() {
        let suffix: u64 = end.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        let length = suffix.min(size);
        return Some((size - length, size - 1));
    }
    let start: u64 = start.parse().ok()?;
    if start >= size {
        return None;
    }
    let end = if end.is_empty() {
        size - 1
    } else {
        end.parse::<u64>().ok()?.min(size - 1)
    };
    (end >= start).then_some((start, end))
}

fn ensure_same_origin(headers: &HeaderMap) -> Result<(), Response> {
    let origin = headers
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| forbidden("missing Origin header"))?;
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| forbidden("missing Host header"))?;
    let origin_host = origin
        .split_once("://")
        .map(|(_, rest)| rest)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default();
    if !origin_host.eq_ignore_ascii_case(host) {
        return Err(forbidden("cross-origin media mutation rejected"));
    }
    Ok(())
}

fn safe_object_path(root: &FsPath, object_key: &str) -> Result<PathBuf, Response> {
    if !valid_object_key(object_key) {
        return Err(internal_error("invalid stored media object key"));
    }
    Ok(root.join(object_key))
}

fn valid_object_key(object_key: &str) -> bool {
    !object_key.is_empty()
        && object_key.len() <= 128
        && object_key != "."
        && object_key != ".."
        && object_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
}

fn fitness_media_root() -> PathBuf {
    ep_core::module_data_root().join("fitness").join("media")
}

async fn object_usage(objects: &FsPath) -> Result<u64, Response> {
    let mut entries = tokio::fs::read_dir(objects).await.map_err(internal_error)?;
    let mut total = 0_u64;
    while let Some(entry) = entries.next_entry().await.map_err(internal_error)? {
        let metadata = tokio::fs::symlink_metadata(entry.path())
            .await
            .map_err(internal_error)?;
        if metadata.file_type().is_symlink() {
            tracing::error!(path = %entry.path().display(), "symlink found in fitness media object store");
            return Err(internal_error(
                "fitness media object store contains a symlink",
            ));
        }
        if metadata.file_type().is_file() {
            total = total
                .checked_add(metadata.len())
                .ok_or_else(|| internal_error("fitness media quota size overflow"))?;
        }
    }
    Ok(total)
}

async fn ensure_private_directory(path: &FsPath) -> Result<(), Response> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(internal_error)?;
    let metadata = tokio::fs::symlink_metadata(path)
        .await
        .map_err(internal_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(internal_error(format!(
            "fitness media path is not a real directory: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(internal_error)?;
    }
    Ok(())
}

async fn create_private_file(path: &FsPath) -> std::io::Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    options.open(path).await
}

async fn cleanup_stale_parts(staging: &FsPath) -> Result<(), Response> {
    const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
    const MAX_SCAN_ENTRIES: usize = 100_000;
    let now = std::time::SystemTime::now();
    let mut entries = tokio::fs::read_dir(staging).await.map_err(internal_error)?;
    let mut scanned = 0usize;
    let mut removed = false;
    while let Some(entry) = entries.next_entry().await.map_err(internal_error)? {
        scanned += 1;
        if scanned > MAX_SCAN_ENTRIES {
            return Err(internal_error(
                "fitness media staging contains too many files",
            ));
        }
        let path = entry.path();
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(internal_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(internal_error(format!(
                "unsupported fitness media staging entry: {}",
                path.display()
            )));
        }
        let is_part = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".part"));
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= STALE_AFTER);
        if is_part && stale {
            tokio::fs::remove_file(&path)
                .await
                .map_err(internal_error)?;
            removed = true;
        }
    }
    if removed {
        sync_directory(staging).await?;
    }
    Ok(())
}

async fn reconcile_orphan_objects(
    pool: &sqlx::SqlitePool,
    objects: &FsPath,
) -> Result<(), Response> {
    const MAX_SCAN_ENTRIES: usize = 100_000;
    let referenced = sqlx::query_scalar::<_, String>("SELECT object_key FROM fit_exercise_media")
        .fetch_all(pool)
        .await
        .map_err(internal_error)?
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let mut present = std::collections::HashSet::new();
    let mut entries = tokio::fs::read_dir(objects).await.map_err(internal_error)?;
    let mut scanned = 0usize;
    let mut removed = false;
    while let Some(entry) = entries.next_entry().await.map_err(internal_error)? {
        scanned += 1;
        if scanned > MAX_SCAN_ENTRIES {
            return Err(internal_error(
                "fitness media object store contains too many files",
            ));
        }
        let path = entry.path();
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(internal_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(internal_error(format!(
                "unsupported fitness media object entry: {}",
                path.display()
            )));
        }
        let key = entry
            .file_name()
            .into_string()
            .map_err(|_| internal_error("fitness media object key is not valid UTF-8"))?;
        if referenced.contains(&key) {
            present.insert(key);
        } else {
            tokio::fs::remove_file(&path)
                .await
                .map_err(internal_error)?;
            removed = true;
        }
    }
    if let Some(missing) = referenced.difference(&present).next() {
        return Err(internal_error(format!(
            "fitness media metadata references a missing object: {missing}"
        )));
    }
    if removed {
        sync_directory(objects).await?;
    }
    Ok(())
}

#[cfg(unix)]
async fn sync_directory(path: &FsPath) -> Result<(), Response> {
    sync_directory_io(path).await.map_err(internal_error)
}

#[cfg(unix)]
async fn sync_directory_io(path: &FsPath) -> std::io::Result<()> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || std::fs::File::open(path)?.sync_all())
        .await
        .map_err(std::io::Error::other)?
}

#[cfg(not(unix))]
async fn sync_directory(_path: &FsPath) -> Result<(), Response> {
    Ok(())
}

#[cfg(not(unix))]
async fn sync_directory_io(_path: &FsPath) -> std::io::Result<()> {
    Ok(())
}

fn max_file_bytes() -> u64 {
    env_u64("EP_FITNESS_MEDIA_MAX_FILE_BYTES", DEFAULT_MAX_FILE_BYTES)
}

fn quota_bytes() -> u64 {
    env_u64("EP_FITNESS_MEDIA_QUOTA_BYTES", DEFAULT_QUOTA_BYTES)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn content_type(media_type: &str) -> &'static str {
    match media_type {
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

fn bad_multipart(error: axum::extract::multipart::MultipartError) -> Response {
    tracing::warn!(error = %error, "invalid fitness media multipart request");
    bad_request("invalid multipart body")
}

fn server_fn_error(error: leptos::server_fn::ServerFnError) -> Response {
    tracing::warn!(error = %error, "fitness media metadata operation failed");
    bad_request(error.to_string())
}

fn bad_request(message: impl Into<String>) -> Response {
    ep_core::api_error_response(StatusCode::BAD_REQUEST, "invalid_media", message.into())
}

fn payload_too_large(message: impl Into<String>) -> Response {
    ep_core::api_error_response(
        StatusCode::PAYLOAD_TOO_LARGE,
        "media_too_large",
        message.into(),
    )
}

fn forbidden(message: impl Into<String>) -> Response {
    ep_core::api_error_response(StatusCode::FORBIDDEN, "forbidden", message.into())
}

fn not_found() -> Response {
    ep_core::api_error_response(StatusCode::NOT_FOUND, "media_not_found", "media not found")
}

fn internal_error(error: impl std::fmt::Display) -> Response {
    tracing::error!(error = %error, "fitness media internal error");
    ep_core::api_error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "internal server error",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_media_detector_is_used_for_storage_metadata() {
        assert_eq!(
            detect_media_format(b"GIF89a.....")
                .map(|format| { (format.media_type(), format.extension()) }),
            Some(("gif", "gif"))
        );
        assert_eq!(detect_media_format(b"not a supported media file"), None);
    }

    #[test]
    fn range_parser_supports_normal_open_and_suffix_ranges() {
        assert_eq!(parse_range("bytes=0-9", 100), Some((0, 9)));
        assert_eq!(parse_range("bytes=90-", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=100-101", 100), None);
        assert_eq!(parse_range("bytes=0-1,4-5", 100), None);
    }
}
