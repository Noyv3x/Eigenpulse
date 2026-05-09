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
