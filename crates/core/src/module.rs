use crate::AppState;

pub trait Module: Sync + 'static {
    fn code(&self) -> &'static str;

    /// `(name, sql)` pairs run idempotently via `_ep_module_migration` ledger.
    fn migrations(&self) -> &'static [(&'static str, &'static str)];

    /// Open-API sub-router; mounted under PAT middleware at `/api/v1/<code>`.
    fn open_api(&self, _state: AppState) -> axum::Router<AppState> {
        axum::Router::<AppState>::new()
    }
}

#[macro_export]
macro_rules! assert_module_migrations_registered {
    ($module:expr) => {{
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let migration_dir = manifest_dir.join("migrations");
        let files: std::collections::BTreeSet<String> = match std::fs::read_dir(&migration_dir) {
            Ok(entries) => entries
                .filter_map(|entry| {
                    let path = entry
                        .unwrap_or_else(|e| panic!("read {} entry: {e}", migration_dir.display()))
                        .path();
                    if path.extension().and_then(|ext| ext.to_str()) != Some("sql") {
                        return None;
                    }
                    Some(
                        path.file_stem()
                            .unwrap_or_else(|| {
                                panic!("migration file has no stem: {}", path.display())
                            })
                            .to_string_lossy()
                            .into_owned(),
                    )
                })
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::collections::BTreeSet::new(),
            Err(e) => panic!("read {}: {e}", migration_dir.display()),
        };
        let migrations = $module.migrations();
        let registered: std::collections::BTreeSet<String> = migrations
            .iter()
            .map(|(name, _)| (*name).to_string())
            .collect();

        assert_eq!(
            registered.len(),
            migrations.len(),
            "module migrations contain duplicate names"
        );
        assert_eq!(
            registered,
            files,
            "module migration registrations must match *.sql files under {}",
            migration_dir.display()
        );
    }};
}
