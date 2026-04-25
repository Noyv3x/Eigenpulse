use axum::{
    extract::{Request, State},
    http::Uri,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::SignedCookieJar;
use ep_core::AppState;

use crate::session::{lookup_session, AuthUser, COOKIE_NAME};

const PUBLIC_PREFIXES: &[&str] = &[
    "/login", "/logout", "/healthz", "/static", "/pkg",
    "/favicon.svg", "/manifest.webmanifest", "/sw.js", "/theme-init.js",
    "/api/v1",   // protected separately by PAT middleware
    "/events",   // SSE has its own auth via cookie checked inside handler
];

fn is_public(uri: &Uri) -> bool {
    let path = uri.path();
    PUBLIC_PREFIXES.iter().any(|p| path == *p || path.starts_with(&format!("{p}/")))
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
        Ok(Some((_sess, user))) => {
            let mut req = req;
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        _ => redirect_login(req.uri()),
    }
}

fn redirect_login(uri: &Uri) -> Response {
    let next = urlencoded(uri.path_and_query().map(|p| p.as_str()).unwrap_or("/"));
    Redirect::temporary(&format!("/login?next={next}")).into_response()
}

fn urlencoded(s: &str) -> String {
    s.replace('%', "%25")
        .replace('?', "%3F")
        .replace('=', "%3D")
        .replace('&', "%26")
}

/// Used inside Leptos `#[server]` functions to gate server-side mutations.
pub async fn require_user_for_server_fn() -> Result<AuthUser, leptos::server_fn::ServerFnError> {
    fn err(msg: &str) -> leptos::server_fn::ServerFnError {
        leptos::server_fn::ServerFnError::ServerError(msg.to_string())
    }
    let parts: axum::http::request::Parts = leptos_axum::extract().await.map_err(|_| err("no request context"))?;
    let user = parts.extensions.get::<AuthUser>().cloned();
    user.ok_or_else(|| err("unauthorized"))
}

