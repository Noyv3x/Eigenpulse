//! Process-wide ownership lock for one Eigenpulse data set.

use fs2::FileExt as _;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

/// Held for the full server lifetime (or full offline restore) so two
/// Eigenpulse processes cannot mutate or replace the same database/media set.
pub struct DatabaseLock {
    file: File,
    path: PathBuf,
}

impl std::fmt::Debug for DatabaseLock {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DatabaseLock")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Drop for DatabaseLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

pub fn acquire_database_lock(database: &Path) -> anyhow::Result<DatabaseLock> {
    let media = std::env::var_os("EP_MODULE_DATA_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data/modules"))
        .join("fitness/media/objects");
    acquire_database_lock_inner(database, &media)
}

pub(crate) fn acquire_database_lock_for_restore(
    database: &Path,
    media: &Path,
) -> anyhow::Result<DatabaseLock> {
    acquire_database_lock_inner(database, media)
}

fn acquire_database_lock_inner(database: &Path, media: &Path) -> anyhow::Result<DatabaseLock> {
    let parent = database
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    create_private_parent_dirs(parent)?;
    let name = database
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("database path must have a UTF-8 filename"))?;
    let path = parent.join(format!(".{name}.lock"));
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let file = options.open(&path)?;
    file.try_lock_exclusive().map_err(|error| {
        anyhow::anyhow!(
            "Eigenpulse data directory is already in use (lock {}): {error}",
            path.display()
        )
    })?;
    crate::archive::recover_interrupted_restore(database, media).map_err(|error| {
        anyhow::anyhow!(
            "an interrupted offline restore requires recovery before Eigenpulse can start: {error:#}"
        )
    })?;
    Ok(DatabaseLock { file, path })
}

#[cfg(unix)]
fn create_private_parent_dirs(parent: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};

    let mut missing = Vec::new();
    let mut cursor = parent;
    while !cursor.as_os_str().is_empty() {
        match std::fs::symlink_metadata(cursor) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    anyhow::bail!("database parent is not a directory: {}", cursor.display());
                }
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(cursor.to_owned());
                let Some(next) = cursor.parent() else {
                    break;
                };
                cursor = next;
            }
            Err(error) => return Err(error.into()),
        }
    }

    for directory in missing.into_iter().rev() {
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        match builder.create(&directory) {
            Ok(()) => {
                // Apply the exact mode even under an unusually restrictive or
                // permissive umask. Existing directories are never retuned.
                std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if !std::fs::symlink_metadata(&directory)?.is_dir() {
                    anyhow::bail!(
                        "database parent is not a directory: {}",
                        directory.display()
                    );
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn create_private_parent_dirs(parent: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(parent)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::acquire_database_lock;

    #[test]
    fn a_second_process_lock_is_rejected_until_the_guard_drops() {
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("eigenpulse.db");
        let first = acquire_database_lock(&database).unwrap();
        assert!(acquire_database_lock(&database).is_err());
        drop(first);
        assert!(acquire_database_lock(&database).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn newly_created_database_parent_directories_are_private() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("private");
        let second = first.join("nested");
        let database = second.join("eigenpulse.db");

        let _lock = acquire_database_lock(&database).unwrap();

        for path in [&first, &second] {
            let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "{} must be private", path.display());
        }
    }
}
