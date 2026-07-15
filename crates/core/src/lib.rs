//! Eigenpulse core types: shared between SSR & hydrate.
//!
//! - `IconKind`, `Tone` — visual primitives, both ends.
//! - `Module` trait & `ModuleRegistry` — SSR-only (depend on axum/sqlx).
//! - `AppState` — SSR-only.

mod descriptor;
mod escape;
mod fmt;
mod nav;
mod severity;
mod text;
mod tone;

#[cfg(feature = "ssr")]
mod api_error;
#[cfg(feature = "ssr")]
mod clock;
#[cfg(feature = "ssr")]
mod media;
#[cfg(feature = "ssr")]
mod module;
#[cfg(feature = "ssr")]
mod notify_msg;
#[cfg(feature = "ssr")]
mod registry;
#[cfg(feature = "ssr")]
mod state;

pub use descriptor::{
    normalize_summary_trend, ModuleDescriptor, ModuleSummary, ModuleSummaryState, SummaryMetric,
    SummaryTrend, SummaryTrendPoint,
};
pub use escape::html_escape;
pub use fmt::{fmt_int, is_valid_app_timestamp, parse_ymd};
#[cfg(feature = "ssr")]
pub use fmt::{AppTimezone, CalendarDate, CalendarRange, TimezoneStore};
pub use nav::IconKind;
pub use severity::Severity;
pub use text::{safe_in_app_path, trim_to_option, url_encode_query_value};
pub use tone::Tone;

/// Canonical response for resources that use module-local integer identities.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct EntityId {
    pub id: i64,
}

impl EntityId {
    pub const fn new(id: i64) -> Self {
        Self { id }
    }
}

pub const SCOPE_NOTIFICATIONS_WRITE: &str = "notifications:write";
pub const SCOPE_ALL: &str = "*";

/// Shared credential bounds. These live in the hydrate-safe core crate so
/// browser constraints and every server-side credential entry point cannot
/// drift apart.
pub const MIN_PASSWORD_CHARS: usize = 6;
pub const MAX_PASSWORD_BYTES: usize = 1024;

/// Map any `Display` (sqlx::Error, anyhow::Error, &str, …) into an internal
/// `ServerFnError`. SSR logs the detailed message server-side, but the value
/// sent over the server-fn wire is generic so SQL paths, URLs, and third-party
/// client details do not reach the browser.
pub fn server_err<E: std::fmt::Display>(e: E) -> leptos::server_fn::ServerFnError {
    let message = e.to_string();
    #[cfg(feature = "ssr")]
    {
        tracing::warn!(error = %message, "server function internal error");
        leptos::server_fn::ServerFnError::ServerError("internal server error".into())
    }
    #[cfg(not(feature = "ssr"))]
    {
        leptos::server_fn::ServerFnError::ServerError(message)
    }
}

/// Deserialize PATCH fields where three states matter:
/// field omitted = keep existing, `null` = clear existing, value = replace.
pub fn deserialize_nullable_patch<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(Some)
}

#[cfg(feature = "ssr")]
pub fn app_state_context() -> Result<AppState, leptos::server_fn::ServerFnError> {
    leptos::prelude::use_context::<AppState>()
        .ok_or_else(|| server_err("AppState context missing in server function"))
}

#[cfg(feature = "ssr")]
pub use api_error::{api_error_response, ApiJson, ApiQuery};
#[cfg(feature = "ssr")]
pub use clock::unix_now;
#[cfg(feature = "ssr")]
pub use media::{
    detect_media_format, module_data_lock, module_data_root, MediaFormat, MEDIA_FORMAT_PROBE_BYTES,
};
#[cfg(feature = "ssr")]
pub use module::Module;
#[cfg(feature = "ssr")]
pub use notify_msg::{NotifyBusHandle, NotifyBusTrait, NotifyEvent, NotifyMessage};
#[cfg(feature = "ssr")]
pub use registry::{run_module_migrations, ModuleRegistry};
#[cfg(feature = "ssr")]
pub use state::AppState;

#[cfg(test)]
mod tests {
    use super::server_err;
    use leptos::server_fn::ServerFnError;

    #[test]
    fn server_err_hides_internal_detail_on_ssr() {
        let err = server_err("sqlite://secret/path failed");
        match err {
            ServerFnError::ServerError(message) => {
                if cfg!(feature = "ssr") {
                    assert_eq!(message, "internal server error");
                } else {
                    assert_eq!(message, "sqlite://secret/path failed");
                }
            }
            other => panic!("expected ServerError, got {other:?}"),
        }
    }
}
