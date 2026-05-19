use axum::{
    extract::{Request, State},
    http::Uri,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::SignedCookieJar;
use ep_core::AppState;

use crate::session::{
    lookup_session, session_cookie, should_refresh_session, AuthUser, COOKIE_NAME,
};

const PUBLIC_PREFIXES: &[&str] = &[
    "/login",
    "/logout",
    "/healthz",
    "/static",
    "/pkg",
    "/favicon.svg",
    "/manifest.webmanifest",
    "/sw.js",
    "/theme-init.js",
    "/api/v1",               // protected separately by PAT middleware
    "/events/notifications", // SSE has its own auth via cookie checked inside handler
];

fn is_public(uri: &Uri) -> bool {
    let path = uri.path();
    PUBLIC_PREFIXES
        .iter()
        .any(|prefix| path_matches_prefix(path, prefix))
}

fn path_matches_prefix(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub async fn require_session(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    req: Request,
    next: Next,
) -> Response {
    if is_public(req.uri()) {
        return next.run(req).await;
    }
    let token = jar.get(COOKIE_NAME).map(|c| c.value().to_string());
    let token = match token {
        Some(t) => t,
        None => return redirect_login(req.uri()),
    };
    match lookup_session(&state.db, &token).await {
        Ok(Some((sess, user))) => {
            let refresh_cookie = should_refresh_session(sess.expires_at, ep_core::unix_now());
            let mut req = req;
            req.extensions_mut().insert(user);
            let response = next.run(req).await;
            if refresh_cookie {
                (jar.add(session_cookie(sess.token)), response).into_response()
            } else {
                response
            }
        }
        Ok(None) => redirect_login(req.uri()),
        Err(e) => {
            tracing::warn!(error = %e, "session lookup failed");
            redirect_login(req.uri())
        }
    }
}

fn redirect_login(uri: &Uri) -> Response {
    let next =
        ep_core::url_encode_query_value(uri.path_and_query().map(|p| p.as_str()).unwrap_or("/"));
    Redirect::temporary(&format!("/login?next={next}")).into_response()
}

/// Used inside Leptos `#[server]` functions to gate server-side mutations.
pub async fn require_user_for_server_fn() -> Result<AuthUser, leptos::server_fn::ServerFnError> {
    let parts: axum::http::request::Parts = leptos_axum::extract()
        .await
        .map_err(|e| ep_core::server_err(format!("server fn request context missing: {e}")))?;
    let user = parts.extensions.get::<AuthUser>().cloned();
    user.ok_or_else(|| leptos::server_fn::ServerFnError::Args("unauthorized".into()))
}

#[cfg(test)]
mod tests {
    use super::is_public;
    use axum::http::Uri;

    #[test]
    fn public_allowlist_keeps_sse_scope_narrow() {
        assert!(is_public(&"/events/notifications".parse::<Uri>().unwrap()));
        assert!(!is_public(&"/events/debug".parse::<Uri>().unwrap()));
        assert!(!is_public(&"/events".parse::<Uri>().unwrap()));
    }

    #[test]
    fn public_allowlist_requires_exact_path_segment_boundary() {
        for raw in ["/api/v1", "/api/v1/healthz", "/static/styles.css"] {
            assert!(is_public(&raw.parse::<Uri>().unwrap()), "raw={raw}");
        }

        for raw in ["/api/v10", "/api/v1extra", "/staticx/styles.css"] {
            assert!(!is_public(&raw.parse::<Uri>().unwrap()), "raw={raw}");
        }
    }
}
