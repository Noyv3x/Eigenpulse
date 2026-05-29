//! Eigenpulse core types: shared between SSR & hydrate.
//!
//! - `IconKind`, `NavSection`, `Tone` — visual primitives, both ends.
//! - `Module` trait & `ModuleRegistry` — SSR-only (depend on axum/sqlx).
//! - `AppState` — SSR-only.

mod escape;
mod fmt;
mod money;
mod nav;
mod severity;
mod text;
mod tone;

#[cfg(feature = "ssr")]
mod activity;
#[cfg(feature = "ssr")]
mod api_error;
#[cfg(feature = "ssr")]
mod clock;
#[cfg(feature = "ssr")]
mod ids;
#[cfg(feature = "ssr")]
mod module;
#[cfg(feature = "ssr")]
mod notify_msg;
#[cfg(feature = "ssr")]
mod refs;
#[cfg(feature = "ssr")]
mod registry;
#[cfg(feature = "ssr")]
mod state;

pub use escape::html_escape;
pub use fmt::{
    amount_step, fmt_int, fmt_minor, fmt_minor_compact, fmt_minor_raw, fmt_ts_date, fmt_ts_hm,
    fmt_ts_md, fmt_ts_minute, fmt_ts_ymd, major_to_minor, parse_minor, parse_ymd, thousands_sep,
    unix_to_ymdhm, ymd_to_unix_midnight,
};
pub use money::{MinorAmount, ParseMinorAmountError};
pub use nav::{IconKind, NavSection};
pub use severity::Severity;
pub use text::{
    normalize_doc_id_input, safe_doc_id, safe_in_app_path, trim_to_option, url_encode_query_value,
    DocIdInputError,
};
pub use tone::Tone;

pub const SCOPE_ACTIVITY_READ: &str = "activity:read";
pub const SCOPE_FIN_READ: &str = "fin:read";
pub const SCOPE_FIN_WRITE: &str = "fin:write";
pub const SCOPE_FIT_READ: &str = "fit:read";
pub const SCOPE_FIT_WRITE: &str = "fit:write";
pub const SCOPE_LRN_READ: &str = "lrn:read";
pub const SCOPE_LRN_WRITE: &str = "lrn:write";
pub const SCOPE_NOTIFY_WRITE: &str = "notify:write";
pub const SCOPE_ALL: &str = "*";
pub const PAT_SCOPES: &[&str] = &[
    SCOPE_ACTIVITY_READ,
    SCOPE_FIN_READ,
    SCOPE_FIN_WRITE,
    SCOPE_FIT_READ,
    SCOPE_FIT_WRITE,
    SCOPE_LRN_READ,
    SCOPE_LRN_WRITE,
    SCOPE_NOTIFY_WRITE,
    SCOPE_ALL,
];

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

/// Apply a nullable PATCH field to its current optional value.
///
/// `None` means the field was omitted and keeps `current`; `Some(None)` means
/// explicit JSON `null` and clears the value; `Some(Some(value))` replaces it.
pub fn apply_nullable_patch<T>(input: Option<Option<T>>, current: Option<T>) -> Option<T> {
    match input {
        Some(value) => value,
        None => current,
    }
}

/// Apply a nullable PATCH field, returning `T::default()` when the result is
/// absent. This is useful for existing server-fn inputs that use blank strings
/// as "no value".
pub fn apply_nullable_patch_or_default<T: Default>(
    input: Option<Option<T>>,
    current: Option<T>,
) -> T {
    apply_nullable_patch(input, current).unwrap_or_default()
}

#[cfg(feature = "ssr")]
pub fn app_state_context() -> Result<AppState, leptos::server_fn::ServerFnError> {
    leptos::prelude::use_context::<AppState>()
        .ok_or_else(|| server_err("AppState context missing in server function"))
}

#[cfg(feature = "ssr")]
pub use activity::{
    load_today_activity, load_today_activity_paged, TodayActivity, TodayActivityOrder,
    TodayActivityRow,
};
#[cfg(feature = "ssr")]
pub use api_error::{api_error_response, ApiErrorBody, ApiErrorInner, ApiJson, ApiQuery};
#[cfg(feature = "ssr")]
pub use clock::unix_now;
#[cfg(feature = "ssr")]
pub use ids::{next_doc_id, DocIdShape};
#[cfg(feature = "ssr")]
pub use module::Module;
#[cfg(feature = "ssr")]
pub use notify_msg::{NotifyBusHandle, NotifyBusTrait, NotifyMessage};
#[cfg(feature = "ssr")]
pub use refs::{clear_doc_references, delete_doc_activity_and_references};
#[cfg(feature = "ssr")]
pub use registry::{run_module_migrations, ModuleRegistry};
#[cfg(feature = "ssr")]
pub use state::AppState;

#[cfg(test)]
mod tests {
    use super::{apply_nullable_patch, apply_nullable_patch_or_default, server_err};
    use leptos::server_fn::ServerFnError;

    #[test]
    fn apply_nullable_patch_preserves_clears_or_replaces() {
        assert_eq!(
            apply_nullable_patch(None, Some("old".to_string())),
            Some("old".to_string())
        );
        assert_eq!(
            apply_nullable_patch(Some(None), Some("old".to_string())),
            None
        );
        assert_eq!(
            apply_nullable_patch(Some(Some("new".to_string())), Some("old".to_string())),
            Some("new".to_string())
        );
    }

    #[test]
    fn apply_nullable_patch_or_default_converts_absent_to_default() {
        assert_eq!(
            apply_nullable_patch_or_default(None, Some("old".to_string())),
            "old"
        );
        assert_eq!(
            apply_nullable_patch_or_default(Some(None), Some("old".to_string())),
            ""
        );
        assert_eq!(
            apply_nullable_patch_or_default(Some(Some("new".to_string())), None),
            "new"
        );
    }

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
