//! Migration-immutability guard.
//!
//! Editing a migration `.sql` after a database has applied it is silent
//! corruption: `sqlx::migrate!()` records each global migration's byte
//! checksum in `_sqlx_migrations` and trips `VersionMismatch` on the next
//! boot. Module migrations also store a mandatory SHA-256 in
//! `_ep_module_migration`, and the registry rejects changed or unknown applied
//! entries. This file-level guard catches accidental edits before startup or
//! deployment. Never edit a committed migration; add a new
//! `NNN_<reason>.sql` migration instead. This test automates that rule.
//!
//! It recursively hashes every committed migration .sql in the canonical
//! platform and module migration trees and compares the result against the checked-in snapshot
//! `tests/migration_checksums.json`. If a committed migration's bytes change
//! — or a tracked file disappears — the test fails.
//!
//! ## Regenerating the snapshot (intentional change)
//!
//! Adding a brand-new migration file is expected and *should* update the
//! snapshot. Editing an existing one is the thing this guard exists to catch,
//! so regeneration is deliberately a manual, named step rather than something
//! the test does silently:
//!
//! ```sh
//! EP_UPDATE_MIGRATION_SNAPSHOT=1 \
//!   cargo test -p ep-db --test migration_immutability --locked
//! ```
//!
//! That rewrites `crates/db/tests/migration_checksums.json` from the current
//! tree and then passes. Review the diff: a single new `+ "…/00N_*.sql"` entry
//! is fine; a changed hash on a *pre-existing* line means you edited an applied
//! migration and should add a new one instead.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Workspace root, derived from this crate's manifest dir (`crates/db`).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize workspace root")
}

fn snapshot_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("migration_checksums.json")
}

/// Collect `(relative-path, sha256-hex)` for every committed migration .sql.
/// Keys are workspace-relative with `/` separators and sorted.
fn collect_checksums() -> BTreeMap<String, String> {
    let root = workspace_root();
    let mut dirs: Vec<PathBuf> = vec![root.join("migrations")];

    // Every business module owns one canonical `migrations/` tree.
    let modules_dir = root.join("modules");
    if modules_dir.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&modules_dir)
            .expect("read modules dir")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect();
        entries.sort();
        for module in entries {
            let migration_dir = module.join("migrations");
            if migration_dir.is_dir() {
                dirs.push(migration_dir);
            }
        }
    }

    let mut files = Vec::new();
    for dir in dirs {
        collect_sql_files(&dir, &mut files);
    }
    files.sort();

    let mut out = BTreeMap::new();
    for path in files {
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("read migration {}: {e}", path.display()));
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let hex = hex::encode(hasher.finalize());

        let rel = path
            .strip_prefix(&root)
            .expect("migration path under workspace root")
            .to_string_lossy()
            .replace('\\', "/");
        out.insert(rel, hex);
    }
    out
}

fn collect_sql_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries = read.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_sql_files(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("sql") {
            out.push(path);
        }
    }
}

fn load_snapshot() -> BTreeMap<String, String> {
    let raw = std::fs::read_to_string(snapshot_path()).unwrap_or_else(|e| {
        panic!(
            "missing migration checksum snapshot at {} ({e}); generate it with \
             `EP_UPDATE_MIGRATION_SNAPSHOT=1 cargo test -p ep-db --test migration_immutability`",
            snapshot_path().display()
        )
    });
    serde_json::from_str(&raw).expect("parse migration_checksums.json")
}

#[test]
fn committed_migrations_are_immutable() {
    let current = collect_checksums();
    assert!(
        !current.is_empty(),
        "found no migration .sql files — path resolution is wrong"
    );

    if std::env::var_os("EP_UPDATE_MIGRATION_SNAPSHOT").is_some() {
        // Pretty-print with sorted keys (BTreeMap already orders them) so the
        // checked-in JSON diffs cleanly.
        let json = serde_json::to_string_pretty(&current).expect("serialize snapshot");
        std::fs::write(snapshot_path(), format!("{json}\n")).expect("write snapshot");
        eprintln!(
            "migration checksum snapshot regenerated ({} files) — review the diff before committing",
            current.len()
        );
        return;
    }

    let snapshot = load_snapshot();

    // 1) Every snapshotted migration must still exist with identical bytes.
    //    A changed hash = an edited applied migration (the corruption this
    //    guard exists to catch). A missing file = a deleted applied migration.
    let mut problems: Vec<String> = Vec::new();
    for (path, expected) in &snapshot {
        match current.get(path) {
            None => problems.push(format!(
                "migration `{path}` is in the snapshot but missing from the tree \
                 (deleting an applied migration corrupts deployed DBs)"
            )),
            Some(actual) if actual != expected => problems.push(format!(
                "migration `{path}` changed bytes (snapshot {expected:.12}…, now {actual:.12}…) \
                 — never edit an applied migration; add a new NNN_<reason>.sql instead"
            )),
            Some(_) => {}
        }
    }

    // 2) New migration files are allowed, but they must be added to the
    //    snapshot in the same change (so the guard tracks them going forward).
    for path in current.keys() {
        if !snapshot.contains_key(path) {
            problems.push(format!(
                "migration `{path}` is new and not in the snapshot — if this is an intentional \
                 addition, regenerate the snapshot with \
                 `EP_UPDATE_MIGRATION_SNAPSHOT=1 cargo test -p ep-db --test migration_immutability`"
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "migration immutability check failed:\n  - {}",
        problems.join("\n  - ")
    );
}
