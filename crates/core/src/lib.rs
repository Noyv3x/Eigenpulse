//! Eigenpulse core types: shared between SSR & hydrate.
//!
//! - `IconKind`, `NavSection`, `Tone` — visual primitives, both ends.
//! - `Module` trait & `ModuleRegistry` — SSR-only (depend on axum/sqlx).
//! - `AppState` — SSR-only.

pub mod nav;
pub mod tone;
pub mod severity;
pub mod fmt;
pub mod escape;

#[cfg(feature = "ssr")]
pub mod ids;
#[cfg(feature = "ssr")]
pub mod module;
#[cfg(feature = "ssr")]
pub mod registry;
#[cfg(feature = "ssr")]
pub mod state;
#[cfg(feature = "ssr")]
pub mod notify_msg;
#[cfg(feature = "ssr")]
pub mod errors;

pub use nav::{NavSection, NavEntry, IconKind};
pub use tone::Tone;
pub use severity::Severity;
pub use fmt::{fmt_int, fmt_money, fmt_ts_date, fmt_ts_hm, fmt_ts_md, fmt_ts_minute, thousands_sep};
pub use escape::html_escape;

/// Map any `Display` (sqlx::Error, anyhow::Error, &str, …) into a
/// `ServerFnError`. Exposed from `ep_core` so module crates and the binary
/// can share one definition; previously each `server_fns.rs` redefined it.
/// Compiled on both SSR and hydrate so the
/// `#[cfg(not(feature = "ssr"))] { Err(server_err("ssr-only")) }` stub
/// branches link.
pub fn server_err<E: std::fmt::Display>(e: E) -> leptos::server_fn::ServerFnError {
    leptos::server_fn::ServerFnError::ServerError(e.to_string())
}

#[cfg(feature = "ssr")]
pub use module::{Module, ModuleLink, DashboardWidget, WidgetKind, TodayItem};
#[cfg(feature = "ssr")]
pub use registry::ModuleRegistry;
#[cfg(feature = "ssr")]
pub use state::AppState;
#[cfg(feature = "ssr")]
pub use ids::{next_doc_id, DocIdShape};
#[cfg(feature = "ssr")]
pub use notify_msg::{NotifyMessage, NotifyBusHandle, NotifyBusTrait};
#[cfg(feature = "ssr")]
pub use errors::{Error, Result};
