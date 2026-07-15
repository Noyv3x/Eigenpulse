//! Portable Eigenpulse backup archives.
//!
//! An `.epbackup` is a Zip64 archive containing a consistent `VACUUM INTO`
//! database snapshot, the Fitness media object tree, and a small JSON manifest
//! with a size and SHA-256 digest for every payload. Media files are already
//! compressed, so they are stored; the database and manifest use deflate.
//!
//! The caller must hold the Fitness media mutation lock for the whole
//! [`create_epbackup`] call. [`restore_epbackup_offline`] is deliberately an
//! offline helper: no process may have the destination database open while it
//! runs.

use crate::backup::{snapshot, unique_snapshot_path};
use anyhow::Context as _;
use ep_core::{detect_media_format, MEDIA_FORMAT_PROBE_BYTES};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub(crate) const EPBACKUP_FORMAT: &str = "eigenpulse.epbackup";
pub(crate) const EPBACKUP_FORMAT_VERSION: u32 = 1;
pub(crate) const EPBACKUP_SCHEMA_GENERATION: u32 = crate::CURRENT_SCHEMA_GENERATION;
pub(crate) const EPBACKUP_MANIFEST_PATH: &str = "manifest.json";
pub(crate) const EPBACKUP_DATABASE_PATH: &str = "database/eigenpulse.db";
pub(crate) const EPBACKUP_MEDIA_PREFIX: &str = "modules/fitness/media/objects/";

const MANIFEST_MAX_BYTES: u64 = 1024 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupEntry {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupManifest {
    pub format: String,
    pub format_version: u32,
    pub schema_generation: u32,
    pub created_at_unix: u64,
    pub database: BackupEntry,
    pub media: Vec<BackupEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestoreLimits {
    pub max_entries: usize,
    pub max_total_bytes: u64,
    pub max_database_bytes: u64,
    pub max_media_bytes: u64,
    pub max_media_file_bytes: u64,
}

impl Default for RestoreLimits {
    fn default() -> Self {
        let media_quota = env_bytes("EP_FITNESS_MEDIA_QUOTA_BYTES", 20 * 1024 * 1024 * 1024);
        let max_database_bytes = 16_u64 * 1024 * 1024 * 1024;
        Self {
            max_entries: 100_000,
            max_total_bytes: max_database_bytes.saturating_add(media_quota),
            max_database_bytes,
            max_media_bytes: media_quota,
            max_media_file_bytes: env_bytes("EP_FITNESS_MEDIA_MAX_FILE_BYTES", 128 * 1024 * 1024)
                .min(media_quota),
        }
    }
}

fn env_bytes(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[derive(Debug, Clone)]
struct SourceFile {
    source: PathBuf,
    entry: BackupEntry,
}

const RESTORE_JOURNAL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestoreJournal {
    version: u32,
    database_dest: PathBuf,
    database_stage: PathBuf,
    media_dest: PathBuf,
    media_stage: PathBuf,
    old_media: PathBuf,
    had_media: bool,
    database_moves: Vec<(PathBuf, PathBuf)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestoreRecovery {
    None,
    RolledBack,
    FinishedPublication,
}

/// Create and atomically publish a portable Zip64 backup.
///
/// `media_root` may be absent, in which case the archive contains an empty
/// media list. If it exists, it must be a real directory containing only real
/// files and directories; symlinks are rejected rather than followed.
/// `dest` is never replaced.
pub async fn create_epbackup(
    pool: &sqlx::SqlitePool,
    media_root: &Path,
    dest: &Path,
) -> anyhow::Result<BackupManifest> {
    if tokio::fs::try_exists(dest).await? {
        anyhow::bail!("backup destination already exists: {}", dest.display());
    }
    let parent = dest
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    ensure_directory(parent).await?;

    let work_dir = unique_snapshot_path(parent, ".epbackup-stage", "work");
    tokio::fs::create_dir(&work_dir).await?;
    set_private_mode(&work_dir, 0o700).await?;
    let database_snapshot = work_dir.join("eigenpulse.db");
    let temp_archive = unique_snapshot_path(parent, ".epbackup", "tmp");

    let result = async {
        snapshot(pool, &database_snapshot).await?;

        let database_for_hash = database_snapshot.clone();
        let database_entry = tokio::task::spawn_blocking(move || {
            let (size_bytes, sha256) = hash_file(&database_for_hash)?;
            Ok::<_, anyhow::Error>(BackupEntry {
                path: EPBACKUP_DATABASE_PATH.to_owned(),
                size_bytes,
                sha256,
            })
        })
        .await??;

        let media_root = media_root.to_owned();
        let media_sources =
            tokio::task::spawn_blocking(move || collect_media(&media_root)).await??;
        let manifest = BackupManifest {
            format: EPBACKUP_FORMAT.to_owned(),
            format_version: EPBACKUP_FORMAT_VERSION,
            schema_generation: EPBACKUP_SCHEMA_GENERATION,
            created_at_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            database: database_entry,
            media: media_sources
                .iter()
                .map(|source| source.entry.clone())
                .collect(),
        };
        validate_manifest(&manifest, RestoreLimits::default())?;

        let archive_path = temp_archive.clone();
        let snapshot_path = database_snapshot.clone();
        let archive_manifest = manifest.clone();
        tokio::task::spawn_blocking(move || {
            write_archive(
                &archive_path,
                &snapshot_path,
                &media_sources,
                &archive_manifest,
            )
        })
        .await??;
        tokio::fs::File::open(&temp_archive)
            .await?
            .sync_all()
            .await?;
        set_private_mode(&temp_archive, 0o600).await?;

        crate::backup::publish_temp_noreplace(&temp_archive, dest)
            .await
            .with_context(|| format!("publish backup without replacing {}", dest.display()))?;
        Ok(manifest)
    }
    .await;

    let _ = tokio::fs::remove_file(&temp_archive).await;
    let _ = tokio::fs::remove_dir_all(&work_dir).await;
    result
}

/// Read and validate an archive's manifest and entry layout without extracting
/// payloads. Payload SHA-256 values are verified by restore, not inspection.
#[cfg(test)]
async fn inspect_epbackup(archive: &Path, limits: RestoreLimits) -> anyhow::Result<BackupManifest> {
    let archive = archive.to_owned();
    tokio::task::spawn_blocking(move || inspect_archive(&archive, limits)).await?
}

/// Restore an `.epbackup` after fully validating it in staging.
///
/// This helper must only run while Eigenpulse is stopped. It acquires the same
/// advisory data lock as the server, validates the complete replacement in
/// staging, then swaps the database (including any existing WAL/SHM files) and
/// media through a rollback-safe publication lifecycle.
pub async fn restore_epbackup_offline(
    archive: &Path,
    database_dest: &Path,
    media_dest: &Path,
    limits: RestoreLimits,
) -> anyhow::Result<BackupManifest> {
    let _database_lock = crate::lock::acquire_database_lock_for_restore(database_dest, media_dest)?;
    prepare_restore_destination(database_dest, false).await?;
    inspect_existing_database_files(database_dest).await?;
    prepare_restore_destination(media_dest, true).await?;

    let database_stage = unique_restore_sibling(database_dest, "new");
    let media_stage = unique_restore_sibling(media_dest, "new");
    tokio::fs::create_dir(&media_stage).await?;
    set_private_mode(&media_stage, 0o700).await?;

    let archive_owned = archive.to_owned();
    let db_stage_owned = database_stage.clone();
    let media_stage_owned = media_stage.clone();
    let extraction = tokio::task::spawn_blocking(move || {
        extract_archive(&archive_owned, &db_stage_owned, &media_stage_owned, limits)
    })
    .await;
    let manifest = match extraction {
        Ok(Ok(manifest)) => manifest,
        Ok(Err(error)) => {
            cleanup_restore_staging(&database_stage, &media_stage).await;
            return Err(error);
        }
        Err(error) => {
            cleanup_restore_staging(&database_stage, &media_stage).await;
            return Err(error.into());
        }
    };

    if let Err(error) = validate_restored_database(&database_stage).await {
        cleanup_restore_staging(&database_stage, &media_stage).await;
        return Err(error);
    }
    if let Err(error) =
        validate_restored_media_index(&database_stage, &media_stage, &manifest).await
    {
        cleanup_restore_staging(&database_stage, &media_stage).await;
        return Err(error);
    }
    if let Err(error) = invalidate_restored_sessions(&database_stage).await {
        cleanup_restore_staging(&database_stage, &media_stage).await;
        return Err(error);
    }
    // Session invalidation deliberately opens the staged database read-write.
    // Re-run both validations afterwards so a hostile trigger attached to the
    // session table cannot mutate the module media index between validation
    // and publication.
    if let Err(error) = validate_restored_database(&database_stage).await {
        cleanup_restore_staging(&database_stage, &media_stage).await;
        return Err(error.context("restored database changed during session invalidation"));
    }
    if let Err(error) =
        validate_restored_media_index(&database_stage, &media_stage, &manifest).await
    {
        cleanup_restore_staging(&database_stage, &media_stage).await;
        return Err(error.context("restored media index changed during session invalidation"));
    }
    set_private_mode(&database_stage, 0o600).await?;
    tokio::fs::File::open(&database_stage)
        .await?
        .sync_all()
        .await?;
    sync_directory_tree(&media_stage).await?;

    publish_restore_staging(&database_stage, &media_stage, database_dest, media_dest).await?;
    Ok(manifest)
}

/// Publish a fully validated and synced restore staging set.
///
/// Existing SQLite main/WAL/SHM files are moved to private sibling names and
/// kept until the replacement database, media tree, and parent directories
/// have all been synced. This is important because a committed transaction may
/// live only in the WAL. An fsynced journal lets the next process either roll
/// back an interrupted publication or finish cleanup after the durable
/// publication marker was written.
async fn publish_restore_staging(
    database_stage: &Path,
    media_stage: &Path,
    database_dest: &Path,
    media_dest: &Path,
) -> anyhow::Result<()> {
    let database_parent = parent_or_dot(database_dest);
    let media_parent = parent_or_dot(media_dest);
    let existing_database = match inspect_existing_database_files(database_dest).await {
        Ok(files) => files,
        Err(error) => {
            cleanup_restore_staging(database_stage, media_stage).await;
            return Err(error);
        }
    };
    let old_database = unique_restore_sibling(database_dest, "old");
    let old_media = unique_restore_sibling(media_dest, "old");
    let had_media = match tokio::fs::try_exists(media_dest).await {
        Ok(exists) => exists,
        Err(error) => {
            cleanup_restore_staging(database_stage, media_stage).await;
            return Err(error.into());
        }
    };

    let database_moves = existing_database
        .into_iter()
        .map(|source| {
            let backup = if source == database_dest {
                old_database.clone()
            } else if source == database_sidecar_path(database_dest, "-wal") {
                database_sidecar_path(&old_database, "-wal")
            } else {
                database_sidecar_path(&old_database, "-shm")
            };
            (source, backup)
        })
        .collect::<Vec<_>>();
    let journal = RestoreJournal {
        version: RESTORE_JOURNAL_VERSION,
        database_dest: database_dest.to_owned(),
        database_stage: database_stage.to_owned(),
        media_dest: media_dest.to_owned(),
        media_stage: media_stage.to_owned(),
        old_media,
        had_media,
        database_moves,
    };
    if let Err(error) = write_restore_journal(&journal).await {
        cleanup_restore_staging(database_stage, media_stage).await;
        return Err(error).context("create durable restore journal");
    }

    let publish = async {
        for (source, backup) in &journal.database_moves {
            tokio::fs::rename(source, backup).await?;
        }
        if journal.had_media {
            tokio::fs::rename(media_dest, &journal.old_media).await?;
        }

        tokio::fs::rename(database_stage, database_dest).await?;
        tokio::fs::rename(media_stage, media_dest).await?;

        // Sync after the renames. The files were already synced in staging;
        // syncing again plus both parent directories makes the publication
        // durable before any old main/WAL/SHM file is removed.
        tokio::fs::File::open(database_dest)
            .await?
            .sync_all()
            .await?;
        sync_directory_tree(media_dest).await?;
        sync_directory(database_parent).await?;
        if media_parent != database_parent {
            sync_directory(media_parent).await?;
        }
        mark_restore_published(database_dest).await?;
        Ok::<(), anyhow::Error>(())
    }
    .await;

    if let Err(error) = publish {
        let database = database_dest.to_owned();
        let media = media_dest.to_owned();
        match tokio::task::spawn_blocking(move || recover_interrupted_restore(&database, &media))
            .await
        {
            Ok(Ok(RestoreRecovery::RolledBack)) => {
                cleanup_restore_staging(database_stage, media_stage).await;
                return Err(error)
                    .context("restore publication failed; previous data was rolled back");
            }
            Ok(Ok(other)) => {
                anyhow::bail!(
                    "restore publication failed ({error:#}); unexpected recovery state: {other:?}"
                );
            }
            Ok(Err(rollback_error)) => {
                anyhow::bail!(
                    "restore publication failed ({error:#}); rollback is incomplete and all recovery artifacts were preserved: {rollback_error:#}"
                );
            }
            Err(join_error) => {
                anyhow::bail!(
                    "restore publication failed ({error:#}); rollback task failed and all recovery artifacts were preserved: {join_error}"
                );
            }
        }
    }

    let database = database_dest.to_owned();
    let media = media_dest.to_owned();
    let recovery =
        tokio::task::spawn_blocking(move || recover_interrupted_restore(&database, &media))
            .await
            .context("finish restore cleanup task")??;
    if recovery != RestoreRecovery::FinishedPublication {
        anyhow::bail!("unexpected durable restore cleanup state: {recovery:?}");
    }
    Ok(())
}

fn write_archive(
    archive_path: &Path,
    database_snapshot: &Path,
    media_sources: &[SourceFile],
    manifest: &BackupManifest,
) -> anyhow::Result<()> {
    let file = create_private_file(archive_path)?;
    let mut writer = ZipWriter::new(file);
    let compressed = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true)
        .unix_permissions(0o600);
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(true)
        .unix_permissions(0o600);

    writer.start_file(EPBACKUP_DATABASE_PATH, compressed)?;
    copy_file_into(database_snapshot, &manifest.database, &mut writer)?;
    for source in media_sources {
        writer.start_file(&source.entry.path, stored)?;
        copy_file_into(&source.source, &source.entry, &mut writer)?;
    }
    writer.start_file(EPBACKUP_MANIFEST_PATH, compressed)?;
    serde_json::to_writer_pretty(&mut writer, manifest)?;
    writer.write_all(b"\n")?;

    let file = writer.finish()?;
    file.sync_all()?;
    Ok(())
}

fn copy_file_into(
    path: &Path,
    expected: &BackupEntry,
    writer: &mut ZipWriter<File>,
) -> anyhow::Result<()> {
    let mut source = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| anyhow::anyhow!("file size overflow: {}", path.display()))?;
        hasher.update(&buffer[..read]);
        writer.write_all(&buffer[..read])?;
    }
    if size != expected.size_bytes || hex_lower(&hasher.finalize()) != expected.sha256 {
        anyhow::bail!(
            "backup source changed while the archive was being created: {}",
            path.display()
        );
    }
    Ok(())
}

fn collect_media(root: &Path) -> anyhow::Result<Vec<SourceFile>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let metadata = std::fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!(
            "fitness media root must be a non-symlink directory: {}",
            root.display()
        );
    }

    let mut paths = Vec::new();
    collect_regular_files(root, &mut paths)?;
    paths.sort();
    let mut sources = Vec::with_capacity(paths.len());
    for source in paths {
        let relative = source.strip_prefix(root).expect("collected below root");
        let relative = portable_relative_path(relative)?;
        let (size_bytes, sha256) = hash_file(&source)?;
        let probe = read_media_probe(File::open(&source)?)?;
        if detect_media_format(&probe).is_none() {
            anyhow::bail!(
                "fitness media has an unsupported signature: {}",
                source.display()
            );
        }
        sources.push(SourceFile {
            source,
            entry: BackupEntry {
                path: format!("{EPBACKUP_MEDIA_PREFIX}{relative}"),
                size_bytes,
                sha256,
            },
        });
    }
    Ok(sources)
}

fn collect_regular_files(dir: &Path, out: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let mut entries = std::fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!("symlink is not allowed in media backup: {}", path.display());
        }
        if metadata.is_dir() {
            collect_regular_files(&path, out)?;
        } else if metadata.is_file() {
            out.push(path);
        } else {
            anyhow::bail!("unsupported media filesystem entry: {}", path.display());
        }
    }
    Ok(())
}

fn portable_relative_path(path: &Path) -> anyhow::Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(
                value
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("media filename is not valid UTF-8"))?,
            ),
            _ => anyhow::bail!("media path is not a safe relative path: {}", path.display()),
        }
    }
    if parts.is_empty() {
        anyhow::bail!("media path cannot be empty");
    }
    Ok(parts.join("/"))
}

fn hash_file(path: &Path) -> anyhow::Result<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| anyhow::anyhow!("file size overflow: {}", path.display()))?;
        hasher.update(&buffer[..read]);
    }
    Ok((size, hex_lower(&hasher.finalize())))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn inspect_archive(path: &Path, limits: RestoreLimits) -> anyhow::Result<BackupManifest> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).context("invalid .epbackup ZIP archive")?;
    if archive.len() > limits.max_entries {
        anyhow::bail!(
            "backup contains too many entries: {} > {}",
            archive.len(),
            limits.max_entries
        );
    }

    let manifest_index = scan_archive_entries(&mut archive, limits)?;
    let manifest_file = archive.by_index(manifest_index)?;
    if manifest_file.size() > MANIFEST_MAX_BYTES {
        anyhow::bail!("backup manifest exceeds {MANIFEST_MAX_BYTES} bytes");
    }
    let mut bytes = Vec::with_capacity(manifest_file.size() as usize);
    manifest_file
        .take(MANIFEST_MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MANIFEST_MAX_BYTES {
        anyhow::bail!("backup manifest exceeds {MANIFEST_MAX_BYTES} bytes");
    }
    let manifest: BackupManifest =
        serde_json::from_slice(&bytes).context("invalid backup manifest JSON")?;
    validate_manifest(&manifest, limits)?;
    validate_archive_layout(&mut archive, &manifest, limits)?;
    Ok(manifest)
}

fn scan_archive_entries<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    limits: RestoreLimits,
) -> anyhow::Result<usize> {
    let mut names = BTreeSet::new();
    let mut manifest_index = None;
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_owned();
        if !names.insert(name.clone()) {
            anyhow::bail!("backup contains duplicate entry: {name}");
        }
        if entry.is_dir() {
            anyhow::bail!("backup contains unexpected directory entry: {name}");
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            anyhow::bail!("backup entry uses unsupported compression: {name}");
        }
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| anyhow::anyhow!("backup uncompressed size overflow"))?;
        if total > limits.max_total_bytes.saturating_add(MANIFEST_MAX_BYTES) {
            anyhow::bail!("backup exceeds restore size limit");
        }
        if name == EPBACKUP_MANIFEST_PATH {
            manifest_index = Some(index);
        }
    }
    manifest_index.ok_or_else(|| anyhow::anyhow!("backup is missing {EPBACKUP_MANIFEST_PATH}"))
}

fn validate_manifest(manifest: &BackupManifest, limits: RestoreLimits) -> anyhow::Result<()> {
    if manifest.format != EPBACKUP_FORMAT {
        anyhow::bail!("unsupported backup format: {}", manifest.format);
    }
    if manifest.format_version != EPBACKUP_FORMAT_VERSION {
        anyhow::bail!(
            "unsupported backup format version: {}",
            manifest.format_version
        );
    }
    if manifest.schema_generation != EPBACKUP_SCHEMA_GENERATION {
        anyhow::bail!(
            "unsupported backup schema generation: {}",
            manifest.schema_generation
        );
    }
    if manifest.database.path != EPBACKUP_DATABASE_PATH {
        anyhow::bail!("backup manifest has an invalid database path");
    }
    validate_digest(&manifest.database.sha256)?;
    if manifest.database.size_bytes == 0 || manifest.database.size_bytes > limits.max_database_bytes
    {
        anyhow::bail!("backup database size is outside restore limits");
    }

    let mut paths = BTreeSet::new();
    paths.insert(manifest.database.path.as_str());
    let mut total = manifest.database.size_bytes;
    let mut media_total = 0_u64;
    for entry in &manifest.media {
        validate_digest(&entry.sha256)?;
        if entry.size_bytes == 0 || entry.size_bytes > limits.max_media_file_bytes {
            anyhow::bail!("media entry size is outside restore limits: {}", entry.path);
        }
        let relative = entry
            .path
            .strip_prefix(EPBACKUP_MEDIA_PREFIX)
            .ok_or_else(|| anyhow::anyhow!("invalid media archive path: {}", entry.path))?;
        validate_archive_relative_path(relative)?;
        validate_media_object_key(relative)?;
        if !paths.insert(entry.path.as_str()) {
            anyhow::bail!("duplicate path in backup manifest: {}", entry.path);
        }
        total = total
            .checked_add(entry.size_bytes)
            .ok_or_else(|| anyhow::anyhow!("backup manifest size overflow"))?;
        media_total = media_total
            .checked_add(entry.size_bytes)
            .ok_or_else(|| anyhow::anyhow!("backup media size overflow"))?;
        if media_total > limits.max_media_bytes {
            anyhow::bail!("backup media exceeds configured Fitness media quota");
        }
    }
    if manifest.media.len().saturating_add(2) > limits.max_entries {
        anyhow::bail!("backup manifest contains too many entries");
    }
    if total > limits.max_total_bytes {
        anyhow::bail!("backup manifest exceeds restore size limit");
    }
    Ok(())
}

fn validate_digest(value: &str) -> anyhow::Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        anyhow::bail!("invalid SHA-256 digest in backup manifest");
    }
    Ok(())
}

fn validate_archive_relative_path(path: &str) -> anyhow::Result<()> {
    if path.is_empty() || path.contains('\\') {
        anyhow::bail!("unsafe backup entry path: {path}");
    }
    let parsed = Path::new(path);
    if parsed
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        Ok(())
    } else {
        anyhow::bail!("unsafe backup entry path: {path}")
    }
}

fn validate_media_object_key(key: &str) -> anyhow::Result<()> {
    if !(1..=128).contains(&key.len())
        || key == "."
        || key == ".."
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        anyhow::bail!("invalid opaque Fitness media object key: {key}");
    }
    Ok(())
}

fn validate_archive_layout<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    manifest: &BackupManifest,
    limits: RestoreLimits,
) -> anyhow::Result<BTreeMap<String, usize>> {
    let mut expected = BTreeMap::new();
    expected.insert(manifest.database.path.clone(), manifest.database.size_bytes);
    for entry in &manifest.media {
        expected.insert(entry.path.clone(), entry.size_bytes);
    }

    let mut indexes = BTreeMap::new();
    let mut payload_total = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.name() == EPBACKUP_MANIFEST_PATH {
            continue;
        }
        let Some(expected_size) = expected.get(entry.name()) else {
            anyhow::bail!("backup contains unlisted payload: {}", entry.name());
        };
        if entry.size() != *expected_size {
            anyhow::bail!(
                "backup entry size disagrees with manifest: {}",
                entry.name()
            );
        }
        payload_total = payload_total
            .checked_add(entry.size())
            .ok_or_else(|| anyhow::anyhow!("backup payload size overflow"))?;
        if payload_total > limits.max_total_bytes {
            anyhow::bail!("backup exceeds restore size limit");
        }
        indexes.insert(entry.name().to_owned(), index);
    }
    if indexes.len() != expected.len() {
        let missing = expected
            .keys()
            .find(|path| !indexes.contains_key(*path))
            .map(String::as_str)
            .unwrap_or("unknown");
        anyhow::bail!("backup is missing manifest payload: {missing}");
    }
    Ok(indexes)
}

fn extract_archive(
    archive_path: &Path,
    database_stage: &Path,
    media_stage: &Path,
    limits: RestoreLimits,
) -> anyhow::Result<BackupManifest> {
    let manifest = inspect_archive(archive_path, limits)?;
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    scan_archive_entries(&mut archive, limits)?;
    let indexes = validate_archive_layout(&mut archive, &manifest, limits)?;

    let database_index = *indexes
        .get(&manifest.database.path)
        .expect("layout validated database entry");
    extract_payload(
        &mut archive,
        database_index,
        database_stage,
        &manifest.database,
        false,
    )?;
    for media in &manifest.media {
        let relative = media
            .path
            .strip_prefix(EPBACKUP_MEDIA_PREFIX)
            .expect("manifest validated media path");
        let destination = media_stage.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
            set_private_mode_blocking(parent, 0o700)?;
        }
        extract_payload(
            &mut archive,
            *indexes.get(&media.path).expect("layout validated media"),
            &destination,
            media,
            true,
        )?;
    }
    Ok(manifest)
}

fn extract_payload<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    index: usize,
    destination: &Path,
    expected: &BackupEntry,
    validate_media: bool,
) -> anyhow::Result<()> {
    let mut input = archive.by_index(index)?;
    let mut output = create_private_file(destination)?;
    let mut hasher = Sha256::new();
    let mut copied = 0_u64;
    let mut probe = Vec::with_capacity(MEDIA_FORMAT_PROBE_BYTES);
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    let result = (|| {
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            copied = copied
                .checked_add(read as u64)
                .ok_or_else(|| anyhow::anyhow!("restored file size overflow"))?;
            if copied > expected.size_bytes {
                anyhow::bail!("restored payload exceeds manifest size: {}", expected.path);
            }
            if probe.len() < MEDIA_FORMAT_PROBE_BYTES {
                let take = (MEDIA_FORMAT_PROBE_BYTES - probe.len()).min(read);
                probe.extend_from_slice(&buffer[..take]);
            }
            hasher.update(&buffer[..read]);
            output.write_all(&buffer[..read])?;
        }
        output.sync_all()?;
        if copied != expected.size_bytes {
            anyhow::bail!("restored payload size mismatch: {}", expected.path);
        }
        let digest = hex_lower(&hasher.finalize());
        if digest != expected.sha256 {
            anyhow::bail!("restored payload checksum mismatch: {}", expected.path);
        }
        if validate_media && detect_media_format(&probe).is_none() {
            anyhow::bail!(
                "restored media has an unsupported signature: {}",
                expected.path
            );
        }
        if !validate_media && !probe.starts_with(b"SQLite format 3\0") {
            anyhow::bail!("restored database has an invalid SQLite header");
        }
        Ok::<(), anyhow::Error>(())
    })();
    if result.is_err() {
        drop(output);
        let _ = std::fs::remove_file(destination);
    }
    result
}

fn read_media_probe(file: File) -> std::io::Result<Vec<u8>> {
    let mut probe = Vec::with_capacity(MEDIA_FORMAT_PROBE_BYTES);
    file.take(MEDIA_FORMAT_PROBE_BYTES as u64)
        .read_to_end(&mut probe)?;
    Ok(probe)
}

async fn validate_restored_database(path: &Path) -> anyhow::Result<()> {
    let url = format!("sqlite://{}", path.display());
    let options = SqliteConnectOptions::from_str(&url)?
        .read_only(true)
        .immutable(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(0)
        .connect_with(options)
        .await?;
    let validation = async {
        let quick_check: String = sqlx::query_scalar("PRAGMA quick_check")
            .fetch_one(&pool)
            .await?;
        if !quick_check.eq_ignore_ascii_case("ok") {
            anyhow::bail!("restored database quick_check failed: {quick_check}");
        }
        let generation: Option<String> =
            sqlx::query_scalar("SELECT value FROM ep_meta WHERE key = 'schema_generation'")
                .fetch_optional(&pool)
                .await?;
        let expected = crate::CURRENT_SCHEMA_GENERATION.to_string();
        if generation.as_deref() != Some(expected.as_str()) {
            anyhow::bail!(
                "restored database is not schema generation {}",
                crate::CURRENT_SCHEMA_GENERATION
            );
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    pool.close().await;
    validation
}

async fn validate_restored_media_index(
    database: &Path,
    media_root: &Path,
    manifest: &BackupManifest,
) -> anyhow::Result<()> {
    let url = format!("sqlite://{}", database.display());
    let options = SqliteConnectOptions::from_str(&url)?
        .read_only(true)
        .immutable(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(0)
        .connect_with(options)
        .await?;
    let rows = sqlx::query_as::<_, (String, String, i64, String)>(
        "SELECT object_key, media_type, byte_size, sha256 FROM fit_exercise_media",
    )
    .fetch_all(&pool)
    .await
    .context("restored database is missing the Fitness media index")?;
    pool.close().await;

    let archived = manifest
        .media
        .iter()
        .map(|entry| {
            let key = entry
                .path
                .strip_prefix(EPBACKUP_MEDIA_PREFIX)
                .expect("manifest media path validated");
            (key, entry)
        })
        .collect::<BTreeMap<_, _>>();
    if rows.len() != archived.len() {
        anyhow::bail!(
            "Fitness media metadata/archive count mismatch: {} rows, {} files",
            rows.len(),
            archived.len()
        );
    }
    for (key, media_type, byte_size, sha256) in rows {
        validate_media_object_key(&key)?;
        let entry = archived.get(key.as_str()).ok_or_else(|| {
            anyhow::anyhow!("Fitness media object is missing from archive: {key}")
        })?;
        if byte_size <= 0
            || u64::try_from(byte_size).ok() != Some(entry.size_bytes)
            || sha256 != entry.sha256
        {
            anyhow::bail!("Fitness media metadata mismatch for object: {key}");
        }
        let path = media_root.join(&key);
        let probe = read_media_probe(File::open(&path)?)?;
        if detect_media_format(&probe).map(|format| format.media_type())
            != Some(media_type.as_str())
        {
            anyhow::bail!("Fitness media type/signature mismatch for object: {key}");
        }
    }
    Ok(())
}

/// A backup retains the owner's password hash and PATs, but browser sessions
/// must never survive a restore. Vacuuming the staging copy also removes the
/// deleted cookie tokens from free SQLite pages before publication.
async fn invalidate_restored_sessions(path: &Path) -> anyhow::Result<()> {
    let url = format!("sqlite://{}", path.display());
    let options = SqliteConnectOptions::from_str(&url)?
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Delete)
        .foreign_keys(true)
        .create_if_missing(false);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .min_connections(1)
        .connect_with(options)
        .await?;
    let result = async {
        sqlx::query("DELETE FROM session").execute(&pool).await?;
        sqlx::query("VACUUM").execute(&pool).await?;
        let quick_check: String = sqlx::query_scalar("PRAGMA quick_check")
            .fetch_one(&pool)
            .await?;
        if !quick_check.eq_ignore_ascii_case("ok") {
            anyhow::bail!(
                "restored database quick_check failed after session purge: {quick_check}"
            );
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    pool.close().await;
    result
}

fn database_sidecar_path(database: &Path, suffix: &str) -> PathBuf {
    let mut path = database.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

/// Return the existing SQLite file set without modifying it. WAL and SHM are
/// part of the database state during restore publication, not disposable temp
/// files: a committed transaction may still exist only in the WAL.
async fn inspect_existing_database_files(database: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let main_exists = tokio::fs::try_exists(database).await?;
    let mut files = Vec::with_capacity(3);
    if main_exists {
        validate_regular_restore_file(database, "database").await?;
        files.push(database.to_owned());
    }
    for suffix in ["-wal", "-shm"] {
        let sidecar = database_sidecar_path(database, suffix);
        if tokio::fs::try_exists(&sidecar).await? {
            if !main_exists {
                anyhow::bail!(
                    "SQLite sidecar exists without its main database: {}",
                    sidecar.display()
                );
            }
            validate_regular_restore_file(&sidecar, "SQLite sidecar").await?;
            files.push(sidecar);
        }
    }
    Ok(files)
}

async fn validate_regular_restore_file(path: &Path, kind: &str) -> anyhow::Result<()> {
    let metadata = tokio::fs::symlink_metadata(path).await?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!("unsafe {kind} path: {}", path.display());
    }
    Ok(())
}

fn restore_journal_paths(database: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
    let parent = parent_or_dot(database);
    let name = database
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("database path must have a UTF-8 filename"))?;
    let journal = parent.join(format!(".{name}.restore-journal.json"));
    let published = parent.join(format!(".{name}.restore-journal.published"));
    Ok((journal, published))
}

async fn write_restore_journal(journal: &RestoreJournal) -> anyhow::Result<()> {
    let journal = journal.clone();
    tokio::task::spawn_blocking(move || write_restore_journal_blocking(&journal)).await??;
    Ok(())
}

fn write_restore_journal_blocking(journal: &RestoreJournal) -> anyhow::Result<()> {
    validate_restore_journal(journal, &journal.database_dest, &journal.media_dest)?;
    let (path, published) = restore_journal_paths(&journal.database_dest)?;
    if path.try_exists()? {
        anyhow::bail!("stale restore journal exists: {}", path.display());
    }
    if published.try_exists()? {
        anyhow::bail!(
            "stale restore publication marker exists: {}",
            published.display()
        );
    }
    let parent = parent_or_dot(&path);
    let temporary = unique_snapshot_path(parent, ".restore-journal-stage", "tmp");
    let result = (|| {
        let mut file = create_private_file(&temporary)?;
        serde_json::to_writer_pretty(&mut file, journal)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        std::fs::rename(&temporary, &path)?;
        sync_directory_blocking(parent)?;
        Ok::<(), anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
        let _ = std::fs::remove_file(&path);
        let _ = sync_directory_blocking(parent);
    }
    result
}

async fn mark_restore_published(database: &Path) -> anyhow::Result<()> {
    let database = database.to_owned();
    tokio::task::spawn_blocking(move || mark_restore_published_blocking(&database)).await??;
    Ok(())
}

fn mark_restore_published_blocking(database: &Path) -> anyhow::Result<()> {
    let (journal, published) = restore_journal_paths(database)?;
    validate_existing_regular_file(&journal, "restore journal")?;
    if published.try_exists()? {
        anyhow::bail!(
            "restore publication marker already exists: {}",
            published.display()
        );
    }
    let parent = parent_or_dot(&published);
    let temporary = unique_snapshot_path(parent, ".restore-published-stage", "tmp");
    let result = (|| {
        let mut file = create_private_file(&temporary)?;
        file.write_all(b"published\n")?;
        file.sync_all()?;
        std::fs::rename(&temporary, &published)?;
        sync_directory_blocking(parent)?;
        Ok::<(), anyhow::Error>(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
        let _ = std::fs::remove_file(&published);
        let _ = sync_directory_blocking(parent);
    }
    result
}

/// Recover a process death in the middle of offline restore publication.
/// Called while holding the database ownership lock, before the server opens
/// SQLite or a new restore begins.
pub(crate) fn recover_interrupted_restore(
    database: &Path,
    expected_media: &Path,
) -> anyhow::Result<RestoreRecovery> {
    let (journal_path, published_path) = restore_journal_paths(database)?;
    let journal_exists = journal_path.try_exists()?;
    let published_exists = published_path.try_exists()?;
    if !journal_exists {
        if published_exists {
            validate_existing_regular_file(&published_path, "restore publication marker")?;
            std::fs::remove_file(&published_path)?;
            sync_directory_blocking(parent_or_dot(&published_path))?;
            return Ok(RestoreRecovery::FinishedPublication);
        }
        return Ok(RestoreRecovery::None);
    }

    validate_existing_regular_file(&journal_path, "restore journal")?;
    let bytes = std::fs::read(&journal_path)?;
    if bytes.len() > 64 * 1024 {
        anyhow::bail!(
            "restore journal is unexpectedly large: {}",
            journal_path.display()
        );
    }
    let journal: RestoreJournal =
        serde_json::from_slice(&bytes).context("invalid restore journal")?;
    validate_restore_journal(&journal, database, expected_media)?;

    if published_exists {
        validate_existing_regular_file(&published_path, "restore publication marker")?;
        finish_published_restore(&journal, &journal_path, &published_path)?;
        Ok(RestoreRecovery::FinishedPublication)
    } else {
        rollback_interrupted_restore(&journal, &journal_path)?;
        Ok(RestoreRecovery::RolledBack)
    }
}

fn validate_restore_journal(
    journal: &RestoreJournal,
    database: &Path,
    expected_media: &Path,
) -> anyhow::Result<()> {
    if journal.version != RESTORE_JOURNAL_VERSION
        || journal.database_dest != database
        || journal.media_dest != expected_media
    {
        anyhow::bail!("restore journal does not match the requested database/media destinations");
    }
    let database_parent = parent_or_dot(database);
    if parent_or_dot(&journal.database_stage) != database_parent
        || !is_expected_restore_sibling(&journal.database_stage, database, "new")
    {
        anyhow::bail!("restore journal has an unsafe database staging path");
    }
    let media_parent = parent_or_dot(&journal.media_dest);
    if parent_or_dot(&journal.media_stage) != media_parent
        || parent_or_dot(&journal.old_media) != media_parent
        || !is_expected_restore_sibling(&journal.media_stage, &journal.media_dest, "new")
        || !is_expected_restore_sibling(&journal.old_media, &journal.media_dest, "old")
    {
        anyhow::bail!("restore journal has unsafe media paths");
    }

    let allowed_sources = [
        journal.database_dest.clone(),
        database_sidecar_path(&journal.database_dest, "-wal"),
        database_sidecar_path(&journal.database_dest, "-shm"),
    ];
    let mut sources = BTreeSet::new();
    let mut backups = BTreeSet::new();
    let old_database = journal
        .database_moves
        .iter()
        .find(|(source, _)| source == database)
        .map(|(_, backup)| backup);
    if journal.database_moves.is_empty() != old_database.is_none()
        || old_database.is_some_and(|path| !is_expected_restore_sibling(path, database, "old"))
    {
        anyhow::bail!("restore journal has an unsafe old database path");
    }
    for (source, backup) in &journal.database_moves {
        let Some(old_database) = old_database else {
            anyhow::bail!("restore journal sidecars require an old main database");
        };
        let expected_backup = if source == database {
            old_database.clone()
        } else if source == &database_sidecar_path(database, "-wal") {
            database_sidecar_path(old_database, "-wal")
        } else {
            database_sidecar_path(old_database, "-shm")
        };
        if !allowed_sources.contains(source)
            || !sources.insert(source.clone())
            || parent_or_dot(backup) != database_parent
            || !backups.insert(backup.clone())
            || backup != &expected_backup
        {
            anyhow::bail!("restore journal has unsafe SQLite move paths");
        }
    }
    Ok(())
}

/// Restore journals are local recovery metadata, not a general filesystem
/// operation format. Every mutable auxiliary path must be a sibling generated
/// by `unique_restore_sibling` for its exact destination. The media destination
/// itself comes from the application's restore configuration and is written
/// only after that destination has passed its type checks.
fn is_expected_restore_sibling(candidate: &Path, destination: &Path, kind: &str) -> bool {
    if parent_or_dot(candidate) != parent_or_dot(destination) {
        return false;
    }
    let Some(destination_name) = destination.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(candidate_name) = candidate.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let prefix = format!(".{destination_name}.restore-{kind}-");
    let Some(id) = candidate_name
        .strip_prefix(&prefix)
        .and_then(|name| name.strip_suffix(".tmp"))
    else {
        return false;
    };
    id.len() == 20 && id.bytes().all(|byte| byte.is_ascii_digit())
}

fn rollback_interrupted_restore(
    journal: &RestoreJournal,
    journal_path: &Path,
) -> anyhow::Result<()> {
    // If the original media directory was moved, first preserve a published
    // replacement by moving it back to its exact staging path, then restore
    // the original directory. Any ambiguity aborts without deleting anything.
    if journal.old_media.try_exists()? {
        validate_existing_directory(&journal.old_media, "old media recovery directory")?;
        if journal.media_dest.try_exists()? {
            validate_existing_directory(&journal.media_dest, "published media directory")?;
            if journal.media_stage.try_exists()? {
                anyhow::bail!(
                    "both published and staged media exist; recovery artifacts preserved"
                );
            }
            std::fs::rename(&journal.media_dest, &journal.media_stage)?;
        }
        std::fs::rename(&journal.old_media, &journal.media_dest)?;
    } else if !journal.had_media
        && !journal.media_stage.try_exists()?
        && journal.media_dest.try_exists()?
    {
        validate_existing_directory(&journal.media_dest, "published media directory")?;
        std::fs::rename(&journal.media_dest, &journal.media_stage)?;
    }

    let original_main_existed = journal
        .database_moves
        .iter()
        .any(|(source, _)| source == &journal.database_dest);
    let main_backup_exists = journal
        .database_moves
        .iter()
        .find(|(source, _)| source == &journal.database_dest)
        .map(|(_, backup)| backup.try_exists())
        .transpose()?
        .unwrap_or(false);
    if main_backup_exists {
        if journal.database_dest.try_exists()? {
            validate_existing_regular_file(&journal.database_dest, "published database")?;
            if journal.database_stage.try_exists()? {
                anyhow::bail!(
                    "both published and staged databases exist; recovery artifacts preserved"
                );
            }
            std::fs::rename(&journal.database_dest, &journal.database_stage)?;
        }
    } else if !original_main_existed
        && !journal.database_stage.try_exists()?
        && journal.database_dest.try_exists()?
    {
        validate_existing_regular_file(&journal.database_dest, "published database")?;
        std::fs::rename(&journal.database_dest, &journal.database_stage)?;
    }

    for (source, backup) in journal.database_moves.iter().rev() {
        if backup.try_exists()? {
            validate_existing_regular_file(backup, "old SQLite recovery file")?;
            if source.try_exists()? {
                anyhow::bail!(
                    "both current and recovery SQLite files exist at {}; recovery artifacts preserved",
                    source.display()
                );
            }
            std::fs::rename(backup, source)?;
        }
    }

    sync_directory_blocking(parent_or_dot(&journal.database_dest))?;
    if parent_or_dot(&journal.media_dest) != parent_or_dot(&journal.database_dest) {
        sync_directory_blocking(parent_or_dot(&journal.media_dest))?;
    }

    cleanup_restore_staging_blocking(&journal.database_stage, &journal.media_stage)?;
    std::fs::remove_file(journal_path)?;
    sync_directory_blocking(parent_or_dot(journal_path))?;
    Ok(())
}

fn finish_published_restore(
    journal: &RestoreJournal,
    journal_path: &Path,
    published_path: &Path,
) -> anyhow::Result<()> {
    validate_existing_regular_file(&journal.database_dest, "published database")?;
    validate_existing_directory(&journal.media_dest, "published media directory")?;
    if journal.database_stage.try_exists()? || journal.media_stage.try_exists()? {
        anyhow::bail!("published restore still has staging paths; recovery artifacts preserved");
    }

    // The durable marker proves the replacements and parent renames were
    // synced. Only now may old main/WAL/SHM and media be deleted.
    for (_, backup) in &journal.database_moves {
        if backup.try_exists()? {
            validate_existing_regular_file(backup, "old SQLite recovery file")?;
            std::fs::remove_file(backup)?;
        }
    }
    if journal.old_media.try_exists()? {
        validate_existing_directory(&journal.old_media, "old media recovery directory")?;
        std::fs::remove_dir_all(&journal.old_media)?;
    }
    sync_directory_blocking(parent_or_dot(&journal.database_dest))?;
    if parent_or_dot(&journal.media_dest) != parent_or_dot(&journal.database_dest) {
        sync_directory_blocking(parent_or_dot(&journal.media_dest))?;
    }

    // Remove the journal first. A marker without a journal means cleanup was
    // already committed and is safe to discard on the next startup.
    std::fs::remove_file(journal_path)?;
    sync_directory_blocking(parent_or_dot(journal_path))?;
    std::fs::remove_file(published_path)?;
    sync_directory_blocking(parent_or_dot(published_path))?;
    Ok(())
}

fn cleanup_restore_staging_blocking(database: &Path, media: &Path) -> anyhow::Result<()> {
    if database.try_exists()? {
        validate_existing_regular_file(database, "restore staging database")?;
        std::fs::remove_file(database)?;
    }
    for suffix in ["-wal", "-shm"] {
        let sidecar = database_sidecar_path(database, suffix);
        if sidecar.try_exists()? {
            validate_existing_regular_file(&sidecar, "restore staging SQLite sidecar")?;
            std::fs::remove_file(sidecar)?;
        }
    }
    if media.try_exists()? {
        validate_existing_directory(media, "restore staging media directory")?;
        std::fs::remove_dir_all(media)?;
    }
    Ok(())
}

fn validate_existing_regular_file(path: &Path, kind: &str) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!("unsafe {kind} path: {}", path.display());
    }
    Ok(())
}

fn validate_existing_directory(path: &Path, kind: &str) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!("unsafe {kind} path: {}", path.display());
    }
    Ok(())
}

async fn prepare_restore_destination(path: &Path, expect_directory: bool) -> anyhow::Result<()> {
    let parent = parent_or_dot(path);
    ensure_directory(parent).await?;
    if tokio::fs::try_exists(path).await? {
        let metadata = tokio::fs::symlink_metadata(path).await?;
        let valid = if expect_directory {
            metadata.is_dir()
        } else {
            metadata.is_file()
        };
        if metadata.file_type().is_symlink() || !valid {
            anyhow::bail!(
                "restore destination has an unsafe filesystem type: {}",
                path.display()
            );
        }
    }
    Ok(())
}

async fn ensure_directory(path: &Path) -> anyhow::Result<()> {
    let existed = tokio::fs::try_exists(path).await?;
    tokio::fs::create_dir_all(path).await?;
    if !existed || is_managed_private_dir(path) {
        set_private_mode(path, 0o700).await?;
    }
    Ok(())
}

fn is_managed_private_dir(path: &Path) -> bool {
    path == Path::new("/data")
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "data" | "backups" | "objects"))
}

fn unique_restore_sibling(path: &Path, kind: &str) -> PathBuf {
    let parent = parent_or_dot(path);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("eigenpulse");
    unique_snapshot_path(parent, &format!(".{name}.restore-{kind}"), "tmp")
}

async fn cleanup_restore_staging(database: &Path, media: &Path) {
    let _ = tokio::fs::remove_file(database).await;
    for suffix in ["-wal", "-shm"] {
        let _ = tokio::fs::remove_file(database_sidecar_path(database, suffix)).await;
    }
    let _ = tokio::fs::remove_dir_all(media).await;
}

fn parent_or_dot(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> anyhow::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?)
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> anyhow::Result<File> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

#[cfg(unix)]
async fn set_private_mode(path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).await?;
    Ok(())
}

#[cfg(not(unix))]
async fn set_private_mode(_path: &Path, _mode: u32) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_mode_blocking(path: &Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_mode_blocking(_path: &Path, _mode: u32) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
async fn sync_directory(path: &Path) -> anyhow::Result<()> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || File::open(path)?.sync_all()).await??;
    Ok(())
}

#[cfg(unix)]
async fn sync_directory_tree(path: &Path) -> anyhow::Result<()> {
    fn sync_tree(path: &Path) -> anyhow::Result<()> {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!(
                "refusing to sync symlink in restored tree: {}",
                path.display()
            );
        }
        if metadata.is_file() {
            File::open(path)?.sync_all()?;
            return Ok(());
        }
        if !metadata.is_dir() {
            anyhow::bail!("unsupported restored filesystem entry: {}", path.display());
        }
        for entry in std::fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
        File::open(path)?.sync_all()?;
        Ok(())
    }
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || sync_tree(&path)).await??;
    Ok(())
}

#[cfg(not(unix))]
async fn sync_directory(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory_blocking(path: &Path) -> anyhow::Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory_blocking(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
async fn sync_directory_tree(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn baseline_pool(path: &Path) -> sqlx::SqlitePool {
        let url = format!("sqlite://{}", path.display());
        let options = SqliteConnectOptions::from_str(&url)
            .expect("options")
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.expect("pool");
        crate::CORE_MIGRATOR
            .run(&pool)
            .await
            .expect("baseline migrations");
        sqlx::query(
            "CREATE TABLE fit_exercise_media (
                object_key TEXT PRIMARY KEY,
                media_type TEXT NOT NULL,
                byte_size INTEGER NOT NULL,
                sha256 TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("fitness media index");
        pool
    }

    #[tokio::test]
    async fn portable_backup_round_trips_database_and_media() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source_db = temp.path().join("source.db");
        let pool = baseline_pool(&source_db).await;
        sqlx::query("CREATE TABLE fixture (value TEXT NOT NULL)")
            .execute(&pool)
            .await
            .expect("fixture table");
        sqlx::query("INSERT INTO fixture VALUES ('preserved')")
            .execute(&pool)
            .await
            .expect("fixture row");
        sqlx::query("INSERT INTO app_user (id, password_hash) VALUES (1, 'argon2-preserved')")
            .execute(&pool)
            .await
            .expect("owner");
        sqlx::query(
            "INSERT INTO session (token, user_id, issued_at, expires_at, last_seen)
             VALUES ('must-be-invalidated', 1, 1, 9999999999, 1)",
        )
        .execute(&pool)
        .await
        .expect("session");
        sqlx::query(
            "INSERT INTO pat (name, prefix, hash, scopes)
             VALUES ('kept', 'ep_pat_test', 'hash-kept', 'finance:read')",
        )
        .execute(&pool)
        .await
        .expect("pat");

        let media_root = temp.path().join("source-media");
        tokio::fs::create_dir_all(&media_root)
            .await
            .expect("media dir");
        let gif = b"GIF89a-test-payload";
        tokio::fs::write(media_root.join("demo.gif"), gif)
            .await
            .expect("media");
        sqlx::query(
            "INSERT INTO fit_exercise_media(object_key,media_type,byte_size,sha256)
             VALUES ('demo.gif','gif',?1,?2)",
        )
        .bind(gif.len() as i64)
        .bind(hex_lower(&Sha256::digest(gif)))
        .execute(&pool)
        .await
        .expect("media metadata");

        let archive = temp.path().join("backup.epbackup");
        let created = create_epbackup(&pool, &media_root, &archive)
            .await
            .expect("create backup");
        let archive_bytes = tokio::fs::read(&archive).await.expect("archive bytes");
        assert!(
            first_local_entry_has_zip64_extra(&archive_bytes),
            "payload entries must be explicitly Zip64-capable"
        );
        assert_eq!(created.schema_generation, crate::CURRENT_SCHEMA_GENERATION);
        assert_eq!(created.media.len(), 1);
        assert_eq!(
            created.media[0].path,
            format!("{EPBACKUP_MEDIA_PREFIX}demo.gif")
        );

        let inspected = inspect_epbackup(&archive, RestoreLimits::default())
            .await
            .expect("inspect");
        assert_eq!(inspected, created);

        let restored_db = temp.path().join("restored/data/eigenpulse.db");
        let restored_media = temp
            .path()
            .join("restored/data/modules/fitness/media/objects");
        tokio::fs::create_dir_all(&restored_media)
            .await
            .expect("old restore media");
        tokio::fs::write(&restored_db, b"old-main")
            .await
            .expect("old restore database");
        tokio::fs::write(
            database_sidecar_path(&restored_db, "-wal"),
            b"old-committed-wal",
        )
        .await
        .expect("old restore wal");
        tokio::fs::write(database_sidecar_path(&restored_db, "-shm"), b"old-shm")
            .await
            .expect("old restore shm");
        tokio::fs::write(restored_media.join("old.gif"), b"GIF89a-old")
            .await
            .expect("old restore media file");
        let restored = restore_epbackup_offline(
            &archive,
            &restored_db,
            &restored_media,
            RestoreLimits::default(),
        )
        .await
        .expect("restore");
        assert_eq!(restored, created);
        assert!(
            !tokio::fs::try_exists(database_sidecar_path(&restored_db, "-wal"))
                .await
                .expect("wal existence"),
            "old WAL must only be removed after durable publication"
        );
        assert!(
            !tokio::fs::try_exists(database_sidecar_path(&restored_db, "-shm"))
                .await
                .expect("shm existence"),
            "old SHM must only be removed after durable publication"
        );
        assert_eq!(
            tokio::fs::read(restored_media.join("demo.gif"))
                .await
                .expect("restored media"),
            gif
        );

        let restored_pool =
            sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=ro", restored_db.display()))
                .await
                .expect("restored pool");
        let value: String = sqlx::query_scalar("SELECT value FROM fixture")
            .fetch_one(&restored_pool)
            .await
            .expect("fixture value");
        assert_eq!(value, "preserved");
        let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session")
            .fetch_one(&restored_pool)
            .await
            .expect("sessions");
        assert_eq!(sessions, 0, "restore must invalidate browser sessions");
        let password_hash: String =
            sqlx::query_scalar("SELECT password_hash FROM app_user WHERE id = 1")
                .fetch_one(&restored_pool)
                .await
                .expect("password hash");
        assert_eq!(password_hash, "argon2-preserved");
        let pats: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pat")
            .fetch_one(&restored_pool)
            .await
            .expect("pats");
        assert_eq!(pats, 1, "restore must preserve PATs");
    }

    #[tokio::test]
    async fn backup_never_replaces_existing_destination() {
        let temp = tempfile::tempdir().expect("tempdir");
        let pool = baseline_pool(&temp.path().join("source.db")).await;
        let archive = temp.path().join("existing.epbackup");
        tokio::fs::write(&archive, b"known-good")
            .await
            .expect("seed archive");

        create_epbackup(&pool, &temp.path().join("missing-media"), &archive)
            .await
            .expect_err("existing archive must be rejected");
        assert_eq!(
            tokio::fs::read(&archive).await.expect("archive"),
            b"known-good"
        );
    }

    #[tokio::test]
    async fn restore_rejects_checksum_mismatch_before_replacing_data() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source_db = temp.path().join("source.db");
        let pool = baseline_pool(&source_db).await;
        let snapshot_path = temp.path().join("snapshot.db");
        snapshot(&pool, &snapshot_path).await.expect("snapshot");
        let size = std::fs::metadata(&snapshot_path).expect("metadata").len();
        let manifest = BackupManifest {
            format: EPBACKUP_FORMAT.to_owned(),
            format_version: EPBACKUP_FORMAT_VERSION,
            schema_generation: EPBACKUP_SCHEMA_GENERATION,
            created_at_unix: 1,
            database: BackupEntry {
                path: EPBACKUP_DATABASE_PATH.to_owned(),
                size_bytes: size,
                sha256: "0".repeat(64),
            },
            media: Vec::new(),
        };
        let archive = temp.path().join("bad.epbackup");
        let file = create_private_file(&archive).expect("archive file");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(true);
        writer
            .start_file(EPBACKUP_DATABASE_PATH, options)
            .expect("database entry");
        std::io::copy(
            &mut File::open(&snapshot_path).expect("snapshot file"),
            &mut writer,
        )
        .expect("database bytes");
        writer
            .start_file(EPBACKUP_MANIFEST_PATH, options)
            .expect("manifest entry");
        serde_json::to_writer(&mut writer, &manifest).expect("manifest");
        writer.finish().expect("finish archive");

        let database_dest = temp.path().join("restore/eigenpulse.db");
        let media_dest = temp.path().join("restore/media");
        tokio::fs::create_dir_all(&media_dest)
            .await
            .expect("media dest");
        tokio::fs::write(&database_dest, b"old-database")
            .await
            .expect("old db");
        let wal_dest = database_sidecar_path(&database_dest, "-wal");
        let shm_dest = database_sidecar_path(&database_dest, "-shm");
        tokio::fs::write(&wal_dest, b"committed-wal-data")
            .await
            .expect("old wal");
        tokio::fs::write(&shm_dest, b"old-shm-data")
            .await
            .expect("old shm");
        tokio::fs::write(media_dest.join("old"), b"old-media")
            .await
            .expect("old media");

        let error = restore_epbackup_offline(
            &archive,
            &database_dest,
            &media_dest,
            RestoreLimits::default(),
        )
        .await
        .expect_err("bad checksum must fail");
        assert!(format!("{error:#}").contains("checksum mismatch"));
        assert_eq!(
            tokio::fs::read(&database_dest).await.expect("old db"),
            b"old-database"
        );
        assert_eq!(
            tokio::fs::read(media_dest.join("old"))
                .await
                .expect("old media"),
            b"old-media"
        );
        assert_eq!(
            tokio::fs::read(&wal_dest).await.expect("old wal"),
            b"committed-wal-data",
            "archive validation failure must not discard WAL-only commits"
        );
        assert_eq!(
            tokio::fs::read(&shm_dest).await.expect("old shm"),
            b"old-shm-data"
        );
    }

    #[tokio::test]
    async fn publication_failure_rolls_back_database_sidecars_and_media() {
        let temp = tempfile::tempdir().expect("tempdir");
        let database_dest = temp.path().join("eigenpulse.db");
        let wal_dest = database_sidecar_path(&database_dest, "-wal");
        let shm_dest = database_sidecar_path(&database_dest, "-shm");
        let media_dest = temp.path().join("objects");
        let missing_database_stage = unique_restore_sibling(&database_dest, "new");
        let media_stage = unique_restore_sibling(&media_dest, "new");

        tokio::fs::write(&database_dest, b"old-main")
            .await
            .expect("main");
        tokio::fs::write(&wal_dest, b"old-wal").await.expect("wal");
        tokio::fs::write(&shm_dest, b"old-shm").await.expect("shm");
        tokio::fs::create_dir(&media_dest).await.expect("old media");
        tokio::fs::write(media_dest.join("old.gif"), b"GIF89a-old")
            .await
            .expect("old media file");
        tokio::fs::create_dir(&media_stage)
            .await
            .expect("media stage");

        let error = publish_restore_staging(
            &missing_database_stage,
            &media_stage,
            &database_dest,
            &media_dest,
        )
        .await
        .expect_err("missing staged database must fail publication");
        assert!(format!("{error:#}").contains("previous data was rolled back"));

        assert_eq!(tokio::fs::read(&database_dest).await.unwrap(), b"old-main");
        assert_eq!(tokio::fs::read(&wal_dest).await.unwrap(), b"old-wal");
        assert_eq!(tokio::fs::read(&shm_dest).await.unwrap(), b"old-shm");
        assert_eq!(
            tokio::fs::read(media_dest.join("old.gif")).await.unwrap(),
            b"GIF89a-old"
        );
        assert!(
            !media_stage.exists(),
            "completed rollback may clean replacement staging data"
        );
    }

    fn seed_interrupted_publication(root: &Path) -> RestoreJournal {
        let database_dest = root.join("eigenpulse.db");
        let media_dest = root.join("objects");
        let database_stage = unique_restore_sibling(&database_dest, "new");
        let old_database = unique_restore_sibling(&database_dest, "old");
        let media_stage = unique_restore_sibling(&media_dest, "new");
        let old_media = unique_restore_sibling(&media_dest, "old");

        std::fs::write(&database_dest, b"old-main").unwrap();
        std::fs::write(database_sidecar_path(&database_dest, "-wal"), b"old-wal").unwrap();
        std::fs::write(database_sidecar_path(&database_dest, "-shm"), b"old-shm").unwrap();
        std::fs::write(&database_stage, b"new-main").unwrap();
        std::fs::create_dir(&media_dest).unwrap();
        std::fs::write(media_dest.join("old.gif"), b"GIF89a-old").unwrap();
        std::fs::create_dir(&media_stage).unwrap();
        std::fs::write(media_stage.join("new.gif"), b"GIF89a-new").unwrap();

        RestoreJournal {
            version: RESTORE_JOURNAL_VERSION,
            database_dest: database_dest.clone(),
            database_stage,
            media_dest,
            media_stage,
            old_media,
            had_media: true,
            database_moves: vec![
                (database_dest.clone(), old_database.clone()),
                (
                    database_sidecar_path(&database_dest, "-wal"),
                    database_sidecar_path(&old_database, "-wal"),
                ),
                (
                    database_sidecar_path(&database_dest, "-shm"),
                    database_sidecar_path(&old_database, "-shm"),
                ),
            ],
        }
    }

    fn simulate_all_publication_renames(journal: &RestoreJournal) {
        write_restore_journal_blocking(journal).unwrap();
        for (source, backup) in &journal.database_moves {
            std::fs::rename(source, backup).unwrap();
        }
        std::fs::rename(&journal.media_dest, &journal.old_media).unwrap();
        std::fs::rename(&journal.database_stage, &journal.database_dest).unwrap();
        std::fs::rename(&journal.media_stage, &journal.media_dest).unwrap();
    }

    #[test]
    fn startup_recovery_rolls_back_process_death_before_durable_marker() {
        let temp = tempfile::tempdir().unwrap();
        let journal = seed_interrupted_publication(temp.path());
        simulate_all_publication_renames(&journal);

        assert_eq!(
            recover_interrupted_restore(&journal.database_dest, &journal.media_dest).unwrap(),
            RestoreRecovery::RolledBack
        );
        assert_eq!(std::fs::read(&journal.database_dest).unwrap(), b"old-main");
        assert_eq!(
            std::fs::read(database_sidecar_path(&journal.database_dest, "-wal")).unwrap(),
            b"old-wal"
        );
        assert_eq!(
            std::fs::read(database_sidecar_path(&journal.database_dest, "-shm")).unwrap(),
            b"old-shm"
        );
        assert_eq!(
            std::fs::read(journal.media_dest.join("old.gif")).unwrap(),
            b"GIF89a-old"
        );
        assert!(!journal.database_stage.exists());
        assert!(!journal.media_stage.exists());
    }

    #[test]
    fn startup_recovery_rejects_a_journal_for_another_media_root() {
        let temp = tempfile::tempdir().unwrap();
        let journal = seed_interrupted_publication(temp.path());
        simulate_all_publication_renames(&journal);
        let unrelated_media = temp.path().join("unrelated/objects");

        let error = recover_interrupted_restore(&journal.database_dest, &unrelated_media)
            .expect_err("journal media destination must be anchored by application config");
        assert!(format!("{error:#}").contains("does not match"));
        let (journal_path, _) = restore_journal_paths(&journal.database_dest).unwrap();
        assert!(
            journal_path.exists(),
            "mismatched recovery must preserve journal"
        );
        assert_eq!(std::fs::read(&journal.database_dest).unwrap(), b"new-main");

        assert_eq!(
            recover_interrupted_restore(&journal.database_dest, &journal.media_dest).unwrap(),
            RestoreRecovery::RolledBack
        );
    }

    #[test]
    fn startup_recovery_finishes_cleanup_after_durable_marker() {
        let temp = tempfile::tempdir().unwrap();
        let journal = seed_interrupted_publication(temp.path());
        simulate_all_publication_renames(&journal);
        sync_directory_blocking(temp.path()).unwrap();
        mark_restore_published_blocking(&journal.database_dest).unwrap();

        assert_eq!(
            recover_interrupted_restore(&journal.database_dest, &journal.media_dest).unwrap(),
            RestoreRecovery::FinishedPublication
        );
        assert_eq!(std::fs::read(&journal.database_dest).unwrap(), b"new-main");
        assert_eq!(
            std::fs::read(journal.media_dest.join("new.gif")).unwrap(),
            b"GIF89a-new"
        );
        for (_, backup) in &journal.database_moves {
            assert!(!backup.exists(), "old SQLite file must be cleaned");
        }
        assert!(!journal.old_media.exists());
        let (journal_path, published_path) = restore_journal_paths(&journal.database_dest).unwrap();
        assert!(!journal_path.exists());
        assert!(!published_path.exists());
    }

    #[test]
    fn manifest_rejects_media_path_traversal() {
        let manifest = BackupManifest {
            format: EPBACKUP_FORMAT.to_owned(),
            format_version: EPBACKUP_FORMAT_VERSION,
            schema_generation: EPBACKUP_SCHEMA_GENERATION,
            created_at_unix: 1,
            database: BackupEntry {
                path: EPBACKUP_DATABASE_PATH.to_owned(),
                size_bytes: 1,
                sha256: "0".repeat(64),
            },
            media: vec![BackupEntry {
                path: format!("{EPBACKUP_MEDIA_PREFIX}../outside"),
                size_bytes: 1,
                sha256: "0".repeat(64),
            }],
        };
        let error = validate_manifest(&manifest, RestoreLimits::default())
            .expect_err("traversal must fail");
        assert!(format!("{error:#}").contains("unsafe backup entry path"));
    }

    #[test]
    fn manifest_rejects_another_schema_generation() {
        let other = crate::CURRENT_SCHEMA_GENERATION + 1;
        let manifest = BackupManifest {
            format: EPBACKUP_FORMAT.to_owned(),
            format_version: EPBACKUP_FORMAT_VERSION,
            schema_generation: other,
            created_at_unix: 1,
            database: BackupEntry {
                path: EPBACKUP_DATABASE_PATH.to_owned(),
                size_bytes: 1,
                sha256: "0".repeat(64),
            },
            media: Vec::new(),
        };
        let error = validate_manifest(&manifest, RestoreLimits::default())
            .expect_err("another schema generation must fail");
        assert!(
            format!("{error:#}")
                .contains(&format!("unsupported backup schema generation: {other}")),
            "{error:#}"
        );
    }

    #[test]
    fn manifest_enforces_media_quota_separately_from_total_limit() {
        let manifest = BackupManifest {
            format: EPBACKUP_FORMAT.to_owned(),
            format_version: EPBACKUP_FORMAT_VERSION,
            schema_generation: EPBACKUP_SCHEMA_GENERATION,
            created_at_unix: 1,
            database: BackupEntry {
                path: EPBACKUP_DATABASE_PATH.to_owned(),
                size_bytes: 1,
                sha256: "0".repeat(64),
            },
            media: ["one.gif", "two.gif"]
                .into_iter()
                .map(|key| BackupEntry {
                    path: format!("{EPBACKUP_MEDIA_PREFIX}{key}"),
                    size_bytes: 15,
                    sha256: "0".repeat(64),
                })
                .collect(),
        };
        let limits = RestoreLimits {
            max_entries: 10,
            max_total_bytes: 100,
            max_database_bytes: 80,
            max_media_bytes: 20,
            max_media_file_bytes: 20,
        };

        let error = validate_manifest(&manifest, limits)
            .expect_err("media quota must not borrow unused database allowance");

        assert!(format!("{error:#}").contains("media quota"));
    }

    #[test]
    fn media_object_keys_match_runtime_safety_rules() {
        let max_length = "x".repeat(128);
        for valid in ["a", "demo.gif", "A-Z_09.webm", max_length.as_str()] {
            validate_media_object_key(valid).expect("valid opaque key");
        }
        let too_long = "x".repeat(129);
        for invalid in [
            "",
            ".",
            "..",
            "nested/demo.gif",
            "back\\slash",
            "has space",
            "媒体.gif",
            too_long.as_str(),
        ] {
            validate_media_object_key(invalid).expect_err("invalid opaque key");
        }
    }

    #[test]
    fn media_collection_uses_the_strict_shared_detector() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("media");
        std::fs::create_dir(&root).expect("media root");

        let mp4 = b"\0\0\0\x18ftypisom\0\0\0\0mp42iso6";
        let webm = [
            0x1a, 0x45, 0xdf, 0xa3, 0x87, 0x42, 0x82, 0x84, b'w', b'e', b'b', b'm',
        ];
        std::fs::write(root.join("video.mp4"), mp4).expect("MP4 fixture");
        std::fs::write(root.join("video.webm"), webm).expect("WebM fixture");
        assert_eq!(collect_media(&root).expect("supported media").len(), 2);

        let avif = b"\0\0\0\x14ftypavif\0\0\0\0mif1";
        std::fs::write(root.join("still-image.avif"), avif).expect("AVIF fixture");
        let error = collect_media(&root).expect_err("AVIF must not pass as MP4 video");
        assert!(format!("{error:#}").contains("unsupported signature"));
        std::fs::remove_file(root.join("still-image.avif")).expect("remove AVIF fixture");

        let matroska = [
            0x1a, 0x45, 0xdf, 0xa3, 0x8b, 0x42, 0x82, 0x88, b'm', b'a', b't', b'r', b'o', b's',
            b'k', b'a',
        ];
        std::fs::write(root.join("generic.mkv"), matroska).expect("Matroska fixture");
        let error = collect_media(&root).expect_err("Matroska must not pass as WebM");
        assert!(format!("{error:#}").contains("unsupported signature"));
    }

    #[cfg(unix)]
    #[test]
    fn media_collection_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("media");
        std::fs::create_dir(&root).expect("media root");
        let outside = temp.path().join("outside.gif");
        std::fs::write(&outside, b"GIF89a-outside").expect("outside");
        symlink(&outside, root.join("linked")).expect("symlink");

        let error = collect_media(&root).expect_err("symlink must fail");
        assert!(format!("{error:#}").contains("symlink is not allowed"));
    }

    fn first_local_entry_has_zip64_extra(bytes: &[u8]) -> bool {
        if bytes.get(..4) != Some(b"PK\x03\x04") || bytes.len() < 30 {
            return false;
        }
        let name_len = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[28], bytes[29]]) as usize;
        let extra_start = 30_usize.saturating_add(name_len);
        let Some(extra) = bytes.get(extra_start..extra_start.saturating_add(extra_len)) else {
            return false;
        };
        let mut cursor = 0;
        while cursor + 4 <= extra.len() {
            let id = u16::from_le_bytes([extra[cursor], extra[cursor + 1]]);
            let size = u16::from_le_bytes([extra[cursor + 2], extra[cursor + 3]]) as usize;
            cursor += 4;
            if id == 0x0001 {
                return true;
            }
            cursor = cursor.saturating_add(size);
        }
        false
    }
}
