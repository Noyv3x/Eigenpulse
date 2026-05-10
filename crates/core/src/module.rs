use crate::AppState;

pub trait Module: Sync + 'static {
    fn code(&self) -> &'static str;

    /// `(name, sql)` pairs run idempotently via `_ep_module_migration` ledger.
    fn migrations(&self) -> &'static [(&'static str, &'static str)];

    /// Open-API sub-router; mounted under PAT middleware at `/api/v1/<code>`.
    fn open_api(&self, _state: AppState) -> axum::Router<AppState> {
        axum::Router::new()
    }
}

#[macro_export]
macro_rules! assert_module_migrations_registered {
    ($module:expr) => {{
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let migration_dir = manifest_dir.join("migrations");
        let files: std::collections::BTreeSet<String> = std::fs::read_dir(&migration_dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", migration_dir.display()))
            .map(|entry| {
                entry
                    .expect("migration dir entry")
                    .path()
                    .file_stem()
                    .expect("migration file stem")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let registered: std::collections::BTreeSet<String> = $module
            .migrations()
            .iter()
            .map(|(name, _)| (*name).to_string())
            .collect();

        assert_eq!(registered, files);
    }};
}
