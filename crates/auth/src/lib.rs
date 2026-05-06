#[cfg(feature = "ssr")]
pub mod argon;
#[cfg(feature = "ssr")]
pub mod bootstrap;
#[cfg(feature = "ssr")]
pub mod middleware;
#[cfg(feature = "ssr")]
pub mod pat;
#[cfg(feature = "ssr")]
pub mod session;

#[cfg(feature = "ssr")]
pub use argon::{hash_password, hash_password_async, verify_password, verify_password_async};
#[cfg(feature = "ssr")]
pub use bootstrap::bootstrap_admin;
#[cfg(feature = "ssr")]
pub use middleware::{require_session, require_user_for_server_fn};
#[cfg(feature = "ssr")]
pub use pat::{generate_pat, hash_token, list_pats, require_pat, revoke_pat, AuthPat};
#[cfg(feature = "ssr")]
pub use session::{
    login_create_session, logout_destroy_session, lookup_session, purge_all_sessions, AuthUser,
    Session, COOKIE_NAME,
};

#[cfg(feature = "ssr")]
pub fn unauthorized(message: &str) -> axum::response::Response {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;
    // serde_json escapes both `"` and `\` correctly; the prior hand-rolled
    // `.replace('"', "\\\"")` missed backslashes.
    let body = serde_json::json!({
        "error": { "code": "unauthorized", "message": message }
    })
    .to_string();
    (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response()
}
