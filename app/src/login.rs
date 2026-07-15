use axum::{
    extract::{ConnectInfo, Extension, Form, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, SameSite, SignedCookieJar};
use ep_auth::{
    cookie_secure, expired_session_cookie, issue_csrf_token, login_create_session,
    logout_destroy_session, session_cookie, verify_csrf, LoginThrottle, COOKIE_NAME,
};
use ep_core::AppState;
use ep_i18n::{build_set_cookie, t, Locale, LOCALE_COOKIE};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::OnceLock;

/// Double-submit CSRF cookie for the login form. Not `HttpOnly` is unnecessary —
/// the value is also embedded in the form, so we keep it `HttpOnly` to deny JS
/// readers while the form field carries the matching copy.
const CSRF_COOKIE: &str = "ep_csrf";

/// Process-wide brute-force limiter for the login POST, keyed by client IP.
/// Lives in a `OnceLock` rather than `AppState` because `AppState` is defined in
/// `ep-core`, which cannot depend on `ep-auth` (that would be a dependency
/// cycle). Defaults: 5 failed attempts / 15-minute fixed window per IP.
fn login_throttle() -> &'static LoginThrottle {
    static THROTTLE: OnceLock<LoginThrottle> = OnceLock::new();
    THROTTLE.get_or_init(LoginThrottle::default)
}

#[derive(Debug, Deserialize)]
pub struct NextQuery {
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

pub async fn page(
    Extension(locale): Extension<Locale>,
    jar: SignedCookieJar,
    Query(q): Query<NextQuery>,
) -> Response {
    let next = sanitize_next(q.next.as_deref());
    // Reuse an existing verified token so concurrent login-page requests do
    // not invalidate a form that is already visible. Browsers can request an
    // undeclared `/favicon.ico` while rendering the page; if that request is
    // redirected back here, rotating the cookie would make every password look
    // wrong because the original form still carries the previous token.
    let csrf = login_csrf_token(&jar);
    let csp_nonce = issue_csrf_token();
    let err_block = if q.error.is_some() {
        format!(
            r#"<div class="login-error"><svg width="14" height="14" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="6.5"/><path d="M9 5v4M9 12.5v.5"/></svg>{}</div>"#,
            ep_core::html_escape(t(locale, "app.login.error_bad_password")),
        )
    } else {
        String::new()
    };
    let html = format!(
        r##"<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width,initial-scale=1"/>
<title>{title}</title>
<link rel="icon" href="/static/favicon.svg" type="image/svg+xml"/>
<link rel="stylesheet" href="/static/styles.css"/>
<script nonce="{csp_nonce}">{theme_init}</script>
</head>
<body>
<div class="login-shell">
  <form method="post" action="/login" class="login-card">
    <div class="login-brand">
      <div class="brand-mark mono">E</div>
      <div class="brand-text">
        <strong>Eigenpulse</strong>
        <small>{meta}</small>
      </div>
    </div>
    {err_block}
    <input type="hidden" name="next" value="{next_html}"/>
    <input type="hidden" name="csrf" value="{csrf_html}"/>
    <label class="login-field" for="login-password">{password_label}</label>
    <input id="login-password" class="login-input mono" type="password" name="password" autocomplete="current-password" autofocus required/>
    <button class="btn primary login-submit" type="submit">{submit}</button>
    <p class="login-hint">{system_hint}</p>
  </form>
</div>
</body></html>"##,
        lang = locale.as_html_lang(),
        title = ep_core::html_escape(t(locale, "app.login.title")),
        meta = ep_core::html_escape(t(locale, "app.login.meta")),
        // The response CSP allow-lists this exact script with the per-response
        // nonce rendered onto the element, keeping login FOUC-free.
        theme_init = crate::security::theme_init_inline(),
        csp_nonce = csp_nonce,
        err_block = err_block,
        next_html = ep_core::html_escape(&next),
        csrf_html = ep_core::html_escape(&csrf),
        password_label = ep_core::html_escape(t(locale, "app.login.password_label")),
        submit = ep_core::html_escape(t(locale, "app.login.submit")),
        system_hint = ep_core::html_escape(t(locale, "app.login.system_hint")),
    );
    let mut response = (jar.add(csrf_cookie(csrf)), Html(html)).into_response();
    let csp = login_csp_content(&csp_nonce);
    response.headers_mut().insert(
        axum::http::HeaderName::from_static("content-security-policy"),
        HeaderValue::from_str(&csp).expect("login CSP contains only a base62 nonce"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn login_csrf_token(jar: &SignedCookieJar) -> String {
    jar.get(CSRF_COOKIE)
        .map(|cookie| cookie.value().to_owned())
        .filter(|token| !token.is_empty())
        .unwrap_or_else(issue_csrf_token)
}

#[derive(Debug, Deserialize)]
pub struct LoginInput {
    pub password: String,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub csrf: Option<String>,
}

pub async fn submit(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: SignedCookieJar,
    Form(input): Form<LoginInput>,
) -> Response {
    // 1) CSRF double-submit. Reject before any DB / crypto work. A missing
    //    cookie or form field yields empty strings, which `verify_csrf` treats
    //    as a non-match. CSRF failures funnel into the *same* error redirect as
    //    a bad password, so they leak nothing and don't count toward the
    //    brute-force throttle below.
    let cookie_csrf = jar
        .get(CSRF_COOKIE)
        .map(|c| c.value().to_string())
        .unwrap_or_default();
    let form_csrf = input.csrf.clone().unwrap_or_default();
    if !verify_csrf(&form_csrf, &cookie_csrf) {
        return login_error_redirect(input.next.as_deref()).into_response();
    }

    // 2) Brute-force throttle, keyed by client IP. Only attempts that get past
    //    CSRF are counted; a successful login `reset`s the key. The peer socket
    //    IP is authoritative unless a trusted reverse proxy is configured
    //    (`EP_TRUSTED_PROXY_CIDRS`) — see `TrustedProxies::client_ip`.
    let client_ip = ep_auth::trusted_proxies()
        .client_ip(&headers, peer)
        .to_string();
    if let Err(retry) = login_throttle().check_and_record(&client_ip).await {
        return too_many_requests(retry.seconds, input.next.as_deref());
    }
    if ep_auth::validate_password(&input.password).is_err() {
        return login_error_redirect(input.next.as_deref()).into_response();
    }

    // 3) Read id + password_hash + persisted locale in one round-trip. The
    //    locale is used below to seed the `ep_locale` cookie when the browser
    //    hasn't picked one yet (cross-device sync of the user's preference).
    let row: Option<(i64, String, String)> =
        match sqlx::query_as("SELECT id, password_hash, locale FROM app_user WHERE id = 1")
            .fetch_optional(&state.db)
            .await
        {
            Ok(r) => r,
            Err(e) => return error500(e),
        };
    let Some((user_id, hash, stored_locale)) = row else {
        return login_error_redirect(input.next.as_deref()).into_response();
    };
    let ok = match ep_auth::verify_password_async(input.password.clone(), hash).await {
        Ok(ok) => ok,
        Err(e) => {
            // A genuine error here (corrupt/unparseable stored hash, blocking
            // pool join failure) is distinct from a wrong password. Log it so
            // an otherwise-unexplained "always wrong" lockout is diagnosable,
            // but still funnel the user into the same generic error redirect so
            // nothing leaks to the client.
            tracing::error!(error = %e, "password verification errored (possible corrupt stored hash)");
            false
        }
    };
    if !ok {
        return login_error_redirect(input.next.as_deref()).into_response();
    }
    // Successful auth: clear the throttle for this IP so earlier typos don't
    // penalize the now-authenticated user.
    login_throttle().reset(&client_ip).await;

    let sess = match login_create_session(&state.db, user_id).await {
        Ok(s) => s,
        Err(e) => return error500(e),
    };
    let next = sanitize_next(input.next.as_deref());
    // Rotate the session cookie in and clear the now-spent CSRF cookie.
    let jar = jar
        .add(session_cookie(sess.token))
        .add(expired_csrf_cookie());

    // Locale seed: if the browser has no `ep_locale` cookie yet AND the user
    // has previously persisted a preference (via `set_user_locale` server
    // fn), forward that preference to the cookie. Once present, the
    // `locale_layer` middleware uses cookie > Accept-Language > default —
    // so this seed is what makes the user's choice survive a fresh browser
    // / private window. Empty `stored_locale` (the migration default) means
    // "user never picked"; let Accept-Language win in that case.
    let cookie_present = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|h| {
            h.split(';')
                .any(|p| p.trim_start().starts_with(&format!("{LOCALE_COOKIE}=")))
        })
        .unwrap_or(false);
    let mut response = (jar, Redirect::to(&next)).into_response();
    if !cookie_present {
        if let Some(loc) = (!stored_locale.is_empty())
            .then(|| Locale::parse(&stored_locale))
            .flatten()
        {
            if let Ok(value) = HeaderValue::from_str(&build_set_cookie(loc)) {
                response.headers_mut().append(header::SET_COOKIE, value);
            }
        }
    }
    response
}

/// CSRF cookie: signed (via `SignedCookieJar`), `HttpOnly`, `SameSite=Lax`,
/// path `/`, secure mirroring `EP_COOKIE_SECURE`. Lives only as long as a login
/// page is open; it is cleared on a successful login.
fn csrf_cookie(token: impl Into<String>) -> Cookie<'static> {
    Cookie::build((CSRF_COOKIE, token.into()))
        .path("/")
        .secure(cookie_secure())
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
}

fn expired_csrf_cookie() -> Cookie<'static> {
    Cookie::build((CSRF_COOKIE, ""))
        .path("/")
        .secure(cookie_secure())
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(cookie::time::Duration::seconds(0))
        .build()
}

/// Header-delivered CSP for the hand-written login document. Login has no
/// hydration scripts, so the handler can place its nonce in both this header
/// and the sole inline theme-init script.
fn login_csp_content(nonce: &str) -> String {
    format!(
        "default-src 'self'; \
         base-uri 'self'; \
         object-src 'none'; \
         frame-ancestors 'none'; \
         img-src 'self' data:; \
         font-src 'self' data:; \
         style-src 'self'; \
         script-src 'self' 'nonce-{nonce}'; \
         connect-src 'self'; \
         form-action 'self'"
    )
}

fn sanitize_next(next: Option<&str>) -> String {
    next.and_then(ep_core::safe_in_app_path)
        .unwrap_or("/")
        .to_string()
}

fn login_error_redirect(next: Option<&str>) -> Redirect {
    Redirect::to(&login_error_location(next))
}

fn login_error_location(next: Option<&str>) -> String {
    let next = sanitize_next(next);
    if next == "/" {
        "/login?error=1".to_string()
    } else {
        format!(
            "/login?error=1&next={}",
            ep_core::url_encode_query_value(&next)
        )
    }
}

/// Throttled response: a 429 redirect back to the login error page with a
/// `Retry-After` header. We keep the redirect (rather than a bare 429 body) so
/// the browser lands on the styled login page; the header lets API-ish clients
/// back off.
fn too_many_requests(retry_after_secs: u64, next: Option<&str>) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::LOCATION, login_error_location(next))],
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

pub async fn logout(State(state): State<AppState>, jar: SignedCookieJar) -> Response {
    if let Some(c) = jar.get(COOKIE_NAME) {
        if let Err(e) = logout_destroy_session(&state.db, c.value()).await {
            tracing::warn!(error = %e, "failed to destroy logout session");
        }
    }
    let jar = jar.add(expired_session_cookie());
    (jar, Redirect::to("/login")).into_response()
}

fn error500(error: impl std::fmt::Display) -> Response {
    tracing::error!(error = %error, "login handler failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "internal server error",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{
        csrf_cookie, login_csp_content, login_csrf_token, login_error_location, sanitize_next,
    };
    use axum_extra::extract::cookie::{Key, SignedCookieJar};

    #[test]
    fn concurrent_login_pages_reuse_the_verified_csrf_cookie() {
        let jar = SignedCookieJar::new(Key::generate());
        let first = login_csrf_token(&jar);
        let jar = jar.add(csrf_cookie(first.clone()));

        assert_eq!(login_csrf_token(&jar), first);
    }

    #[test]
    fn sanitize_next_accepts_local_paths() {
        assert_eq!(sanitize_next(Some("/")), "/");
        assert_eq!(
            sanitize_next(Some("/finance?tab=budget")),
            "/finance?tab=budget"
        );
    }

    #[test]
    fn sanitize_next_rejects_external_and_backslash_paths() {
        assert_eq!(sanitize_next(Some("//example.com")), "/");
        assert_eq!(sanitize_next(Some("https://example.com")), "/");
        assert_eq!(sanitize_next(Some(r"/\example.com")), "/");
        assert_eq!(sanitize_next(Some(r"/finance\evil")), "/");
        assert_eq!(sanitize_next(Some("/finance%0d%0aevil")), "/");
        assert_eq!(sanitize_next(Some("/finance%1Fevil")), "/");
        assert_eq!(sanitize_next(Some("/finance%7Fevil")), "/");
        assert_eq!(sanitize_next(Some("/finance\r\nevil")), "/");
        assert_eq!(sanitize_next(None), "/");
    }

    #[test]
    fn login_error_location_preserves_safe_next_only() {
        assert_eq!(
            login_error_location(Some("/finance?tab=budget")),
            "/login?error=1&next=%2Ffinance%3Ftab%3Dbudget"
        );
        assert_eq!(
            login_error_location(Some("https://example.com")),
            "/login?error=1"
        );
        assert_eq!(login_error_location(None), "/login?error=1");
    }

    #[test]
    fn login_csp_allows_only_the_handler_nonce() {
        let csp = login_csp_content("abc123");
        assert!(csp.contains("script-src 'self' 'nonce-abc123'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(!csp.contains("'unsafe-inline'"));
        assert!(!csp.contains("wasm-unsafe-eval"));
    }
}
