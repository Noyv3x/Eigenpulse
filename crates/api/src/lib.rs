#[cfg(feature = "ssr")]
pub mod errors;
#[cfg(feature = "ssr")]
pub mod whoami;
#[cfg(feature = "ssr")]
pub mod today;
#[cfg(feature = "ssr")]
pub mod notify;
#[cfg(feature = "ssr")]
pub mod healthz;

#[cfg(feature = "ssr")]
use axum::{routing::{get, post}, Router};
#[cfg(feature = "ssr")]
use ep_core::{AppState, ModuleRegistry};

#[cfg(feature = "ssr")]
pub fn router(state: AppState, registry: &ModuleRegistry) -> Router<AppState> {
    let mut r = Router::new()
        .route("/healthz", get(healthz::ok))
        .route("/whoami", get(whoami::handler))
        .route("/today", get(today::handler))
        .route("/notify", post(notify::handler))
        .merge(registry.open_api_router(state.clone()));
    // Layer here is intentional empty; the binary attaches PAT middleware to the whole `/api/v1/*` group.
    let _ = state;
    r = r.with_state(state.clone());
    r
}
