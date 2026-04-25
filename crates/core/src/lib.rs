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
pub use fmt::{fmt_int, fmt_money, thousands_sep};
pub use escape::html_escape;

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
