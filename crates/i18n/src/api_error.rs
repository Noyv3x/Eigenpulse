//! Shared Open-API error mapping for feature-module routers.
//!
//! Every `modules/*/src/api.rs` used to carry a byte-identical copy of these
//! helpers. They live here because the mapping needs both the i18n catalog
//! (`parse_err` / `t` / `tf`) and `ep_core::api_error_response`.

use axum::http::StatusCode;
use axum::response::Response;
use leptos::server_fn::ServerFnError;
use std::fmt::Display;

/// Map a `ServerFnError` returned by a module helper into an Open-API JSON
/// error response.
///
/// Domain errors created via [`crate::err`] / [`crate::err_with`] keep their
/// i18n `code` and render an English message; a plain `Args` error keeps its
/// message as a `bad_request`; anything else is logged under `log_label` and
/// hidden behind a generic 500 so SQL text / client internals never leak.
pub fn i18n_error_response(e: ServerFnError, log_label: &str) -> Response {
    if let ServerFnError::Args(msg) = &e {
        if let Some((code, payload)) = crate::parse_err(&e) {
            return ep_core::api_error_response(
                status_for_i18n_error(code),
                code,
                i18n_error_message(code, payload),
            );
        }
        return ep_core::api_error_response(StatusCode::BAD_REQUEST, "bad_request", msg.as_str());
    }
    tracing::warn!(error = %e, context = log_label, "open api helper error");
    internal_error_response()
}

/// 500 wrapper for a raw DB / infrastructure error: logs `e` under `log_label`
/// and returns a generic message.
pub fn db_error_response<E: Display>(e: E, log_label: &str) -> Response {
    tracing::warn!(error = %e, context = log_label, "open api db error");
    internal_error_response()
}

fn internal_error_response() -> Response {
    ep_core::api_error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "database error",
    )
}

/// `*_not_found` → 404, `*_taken` → 409, everything else → 400. The `_taken`
/// branch is only reached by modules with uniqueness conflicts (finance
/// account/category codes); others simply never hit it.
fn status_for_i18n_error(code: &str) -> StatusCode {
    if code.ends_with("_not_found") {
        StatusCode::NOT_FOUND
    } else if code.ends_with("_taken") {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_REQUEST
    }
}

fn i18n_error_message(code: &str, payload: Option<&str>) -> String {
    match payload {
        Some(payload) => crate::tf(crate::Locale::En, code, &[("payload", payload)]),
        None => crate::t(crate::Locale::En, code).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_keys_off_the_i18n_code_suffix() {
        assert_eq!(
            status_for_i18n_error("finance.err.txn_not_found"),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status_for_i18n_error("finance.err.account_code_taken"),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status_for_i18n_error("finance.err.amount_must_be_nonzero"),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn message_interpolates_payload_in_english() {
        assert_eq!(
            i18n_error_message("finance.err.txn_not_found", Some("FIN-1")),
            "Transaction 'FIN-1' not found"
        );
    }

    #[test]
    fn i18n_error_response_maps_domain_errors_and_hides_internals() {
        // Domain `Args` error keeps its mapped status.
        let domain = i18n_error_response(
            crate::err_with("finance.err.txn_not_found", "FIN-1"),
            "test",
        );
        assert_eq!(domain.status(), StatusCode::NOT_FOUND);

        // A non-Args error is hidden behind a generic 500.
        let internal =
            i18n_error_response(ServerFnError::ServerError("sqlite boom".into()), "test");
        assert_eq!(internal.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
