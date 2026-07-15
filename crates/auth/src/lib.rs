#[cfg(feature = "ssr")]
mod argon;
#[cfg(feature = "ssr")]
mod bootstrap;
#[cfg(feature = "ssr")]
mod login_guard;
#[cfg(feature = "ssr")]
mod middleware;
#[cfg(feature = "ssr")]
mod password;
#[cfg(feature = "ssr")]
mod pat;
#[cfg(feature = "ssr")]
mod proxy;
#[cfg(feature = "ssr")]
mod session;

#[cfg(feature = "ssr")]
pub use argon::{hash_password, hash_password_async, verify_password_async};
#[cfg(feature = "ssr")]
pub use bootstrap::bootstrap_admin;
#[cfg(feature = "ssr")]
pub use login_guard::{issue_csrf_token, verify_csrf, LoginThrottle, RetryAfter};
#[cfg(feature = "ssr")]
pub use middleware::{authed_state, require_session, require_user_for_server_fn};
#[cfg(feature = "ssr")]
pub use password::{validate_password, MAX_PASSWORD_BYTES, MIN_PASSWORD_CHARS};
#[cfg(feature = "ssr")]
pub use pat::{
    generate_pat, list_pats, require_pat, require_scope, revoke_pat, AuthPat, MAX_PAT_NAME_CHARS,
};
#[cfg(feature = "ssr")]
pub use proxy::{init_trusted_proxies_from_env, trusted_proxies, TrustedProxies};
#[cfg(feature = "ssr")]
pub use session::{
    cookie_secure, delete_expired_sessions, expired_session_cookie, login_create_session,
    logout_destroy_session, lookup_session, purge_all_sessions, session_cookie, session_is_valid,
    should_refresh_session, AuthUser, Session, COOKIE_NAME,
};

#[cfg(feature = "ssr")]
pub(crate) fn json_error(
    status: axum::http::StatusCode,
    code: &str,
    message: &str,
) -> axum::response::Response {
    ep_core::api_error_response(status, code, message)
}

#[cfg(feature = "ssr")]
pub fn unauthorized(message: &str) -> axum::response::Response {
    json_error(
        axum::http::StatusCode::UNAUTHORIZED,
        "unauthorized",
        message,
    )
}
