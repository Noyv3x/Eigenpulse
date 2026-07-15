use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header, Method, StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::SignedCookieJar;
use ep_core::AppState;
use std::net::SocketAddr;

use crate::session::{
    lookup_session, session_cookie, should_refresh_session, AuthUser, COOKIE_NAME,
};

const PUBLIC_EXACT: &[&str] = &[
    "/login",
    "/logout",
    "/livez",
    "/readyz",
    "/sw.js",
    "/events/notifications", // SSE authenticates the cookie inside its handler
];

const PUBLIC_PREFIXES: &[&str] = &["/static", "/pkg"];

fn is_public(uri: &Uri) -> bool {
    let path = uri.path();
    PUBLIC_EXACT.contains(&path)
        || PUBLIC_PREFIXES
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
    let public = is_public(req.uri());
    if (!public || req.uri().path() == "/logout") && !unsafe_request_has_same_origin(&req) {
        return (
            StatusCode::FORBIDDEN,
            "cross-origin cookie-authenticated request rejected",
        )
            .into_response();
    }
    if public {
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

/// Cookie-authenticated mutations must come from the exact origin serving the
/// app. `SameSite=Lax` still sends cookies between same-site sibling origins
/// (for example, another service on the same NAS host but a different port),
/// so it is not a complete CSRF boundary by itself.
fn unsafe_request_has_same_origin(req: &Request) -> bool {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect| connect.0);
    unsafe_request_has_same_origin_with(req, crate::trusted_proxies(), peer)
}

fn unsafe_request_has_same_origin_with(
    req: &Request,
    proxies: &crate::TrustedProxies,
    peer: Option<SocketAddr>,
) -> bool {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }

    let Some(origin) = req.headers().get(header::ORIGIN) else {
        return false;
    };
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .or_else(|| req.uri().authority().map(|authority| authority.as_str()));
    let Some(host) = host else { return false };
    let Ok(origin) = origin.to_str() else {
        return false;
    };
    let Ok(origin) = origin.parse::<Uri>() else {
        return false;
    };
    let request_scheme = proxies
        .forwarded_proto(req.headers(), peer)
        .or(req.uri().scheme_str())
        .unwrap_or("http");
    if !matches!(request_scheme, "http" | "https") || origin.scheme_str() != Some(request_scheme) {
        return false;
    }

    origin
        .authority()
        .is_some_and(|authority| authority.as_str().eq_ignore_ascii_case(host))
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

/// SSR server-fn entry guard: authenticate the session user AND pull `AppState`
/// in one call — the two things every authenticated server fn does first. The
/// session check is enforced (its `AuthUser` is dropped) so the auth gate can
/// never be forgotten; callers that need the user should call
/// [`require_user_for_server_fn`] directly.
pub async fn authed_state() -> Result<ep_core::AppState, leptos::server_fn::ServerFnError> {
    require_user_for_server_fn().await?;
    ep_core::app_state_context()
}

#[cfg(test)]
mod tests {
    use super::{is_public, unsafe_request_has_same_origin_with};
    use crate::TrustedProxies;
    use axum::{body::Body, extract::Request, http::Uri};
    use std::net::SocketAddr;

    fn is_same_origin(req: &Request) -> bool {
        unsafe_request_has_same_origin_with(req, &TrustedProxies::default(), None)
    }

    #[test]
    fn public_allowlist_keeps_sse_scope_narrow() {
        assert!(is_public(&"/events/notifications".parse::<Uri>().unwrap()));
        assert!(!is_public(&"/events/debug".parse::<Uri>().unwrap()));
        assert!(!is_public(&"/events".parse::<Uri>().unwrap()));
    }

    #[test]
    fn health_endpoints_are_exact_public_paths() {
        for raw in ["/livez", "/readyz"] {
            assert!(is_public(&raw.parse::<Uri>().unwrap()), "raw={raw}");
        }
        for raw in ["/definitely-not-public", "/livez/debug", "/readyz/extra"] {
            assert!(!is_public(&raw.parse::<Uri>().unwrap()), "raw={raw}");
        }
    }

    #[test]
    fn public_allowlist_requires_exact_path_segment_boundary() {
        for raw in ["/static/styles.css", "/pkg/eigenpulse.wasm"] {
            assert!(is_public(&raw.parse::<Uri>().unwrap()), "raw={raw}");
        }

        for raw in ["/staticx/styles.css", "/pkgextra/eigenpulse.wasm"] {
            assert!(!is_public(&raw.parse::<Uri>().unwrap()), "raw={raw}");
        }
    }

    #[test]
    fn csrf_origin_guard_accepts_only_exact_origin_for_mutations() {
        let same_origin = Request::builder()
            .method("POST")
            .uri("/api/_internal/fin/add_txn")
            .header("host", "eigenpulse.home.arpa:3000")
            .header("origin", "http://eigenpulse.home.arpa:3000")
            .body(Body::empty())
            .unwrap();
        assert!(is_same_origin(&same_origin));

        let proxies = TrustedProxies::parse("10.0.0.0/8").unwrap();
        let trusted_peer: SocketAddr = "10.0.0.2:4321".parse().unwrap();
        let proxied_https = Request::builder()
            .method("POST")
            .uri("/api/_internal/fin/add_txn")
            .header("host", "eigenpulse.home.arpa:8443")
            .header("x-forwarded-proto", "https")
            .header("origin", "https://eigenpulse.home.arpa:8443")
            .body(Body::empty())
            .unwrap();
        assert!(unsafe_request_has_same_origin_with(
            &proxied_https,
            &proxies,
            Some(trusted_peer)
        ));

        // The same header from a direct/untrusted peer cannot change the
        // request scheme and therefore fails the exact-origin check.
        let direct_peer: SocketAddr = "203.0.113.9:4321".parse().unwrap();
        assert!(!unsafe_request_has_same_origin_with(
            &proxied_https,
            &proxies,
            Some(direct_peer)
        ));

        let http2_authority_fallback = Request::builder()
            .method("POST")
            .uri("https://eigenpulse.home.arpa/api/_internal/fin/add_txn")
            .header("origin", "https://eigenpulse.home.arpa")
            .body(Body::empty())
            .unwrap();
        assert!(is_same_origin(&http2_authority_fallback));

        for origin in [
            "https://eigenpulse.home.arpa:3000",
            "http://eigenpulse.home.arpa:4000",
            "http://other.home.arpa:3000",
            "null",
        ] {
            let req = Request::builder()
                .method("POST")
                .uri("/api/_internal/fin/add_txn")
                .header("host", "eigenpulse.home.arpa:3000")
                .header("origin", origin)
                .body(Body::empty())
                .unwrap();
            assert!(!is_same_origin(&req), "origin={origin}");
        }

        let missing_origin = Request::builder()
            .method("POST")
            .uri("/api/_internal/fin/add_txn")
            .header("host", "eigenpulse.home.arpa:3000")
            .body(Body::empty())
            .unwrap();
        assert!(!is_same_origin(&missing_origin));
    }

    #[test]
    fn csrf_origin_guard_does_not_block_safe_methods() {
        let req = Request::builder()
            .method("GET")
            .uri("/finance")
            .body(Body::empty())
            .unwrap();
        assert!(is_same_origin(&req));
    }
}
