use axum::{
    extract::{Extension, Form, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, SameSite, SignedCookieJar};
use ep_auth::{login_create_session, logout_destroy_session, COOKIE_NAME};
use ep_core::AppState;
use ep_i18n::{build_set_cookie, t, Locale, LOCALE_COOKIE};
use serde::Deserialize;
use time::Duration;

/// HTTPS-only cookie if `EP_COOKIE_SECURE=1` (recommended for production).
/// Default false so local HTTP / NAS-LAN deployments can persist sessions.
fn cookie_secure() -> bool {
    std::env::var("EP_COOKIE_SECURE")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
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
    Query(q): Query<NextQuery>,
) -> Html<String> {
    let next = q.next.unwrap_or_else(|| "/".into());
    let err_block = if q.error.is_some() {
        format!(
            r#"<div style="background:var(--rose-soft);color:var(--rose-ink);padding:8px 12px;border-radius:8px;font-size:13px;margin-bottom:14px">{}</div>"#,
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
<link rel="stylesheet" href="/static/styles.css"/>
<script src="/static/theme-init.js"></script>
</head>
<body>
<div style="min-height:100vh;display:grid;place-items:center;background:var(--bg)">
  <form method="post" action="/login" style="background:var(--surface);border:1px solid var(--border);border-radius:14px;padding:32px 28px;width:380px;box-shadow:var(--shadow-md)">
    <div style="display:flex;align-items:center;gap:12px;margin-bottom:24px">
      <div class="brand-mark mono" style="width:34px;height:34px;background:var(--ink);color:var(--bg);border-radius:9px;display:grid;place-items:center;font-weight:700">E</div>
      <div>
        <div style="font-weight:600;font-size:16px">Eigenpulse</div>
        <div class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">{meta}</div>
      </div>
    </div>
    {err_block}
    <input type="hidden" name="next" value="{next_html}"/>
    <label style="display:block;font-size:12px;color:var(--ink-3);margin-bottom:6px;font-family:var(--font-mono);text-transform:uppercase;letter-spacing:0.06em">{password_label}</label>
    <input type="password" name="password" autofocus required
           style="width:100%;padding:10px 12px;border:1px solid var(--border);border-radius:8px;background:var(--bg-2);font-family:var(--font-mono);font-size:14px;margin-bottom:16px"/>
    <button class="btn primary" type="submit" style="width:100%;justify-content:center;padding:9px 14px;font-size:13.5px">{submit}</button>
    <p class="muted" style="font-size:11.5px;margin-top:18px;line-height:1.5">{system_hint}</p>
  </form>
</div>
</body></html>"##,
        lang = locale.as_html_lang(),
        title = ep_core::html_escape(t(locale, "app.login.title")),
        meta = ep_core::html_escape(t(locale, "app.login.meta")),
        err_block = err_block,
        next_html = ep_core::html_escape(&next),
        password_label = ep_core::html_escape(t(locale, "app.login.password_label")),
        submit = ep_core::html_escape(t(locale, "app.login.submit")),
        system_hint = ep_core::html_escape(t(locale, "app.login.system_hint")),
    );
    Html(html)
}

#[derive(Debug, Deserialize)]
pub struct LoginInput {
    pub password: String,
    #[serde(default)]
    pub next: Option<String>,
}

pub async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: SignedCookieJar,
    Form(input): Form<LoginInput>,
) -> Response {
    // Read id + password_hash + persisted locale in one round-trip. The
    // locale is used below to seed the `ep_locale` cookie when the browser
    // hasn't picked one yet (cross-device sync of the user's preference).
    let row: Option<(i64, String, String)> =
        match sqlx::query_as("SELECT id, password_hash, locale FROM app_user WHERE id = 1")
            .fetch_optional(&state.db)
            .await
        {
            Ok(r) => r,
            Err(e) => return error500(&e.to_string()),
        };
    let Some((user_id, hash, stored_locale)) = row else {
        return Redirect::to("/login?error=1").into_response();
    };
    let ok = ep_auth::verify_password_async(input.password.clone(), hash)
        .await
        .unwrap_or(false);
    if !ok {
        return Redirect::to("/login?error=1").into_response();
    }
    let sess = match login_create_session(&state.db, user_id).await {
        Ok(s) => s,
        Err(e) => return error500(&e.to_string()),
    };
    let next = input
        .next
        .filter(|s| s.starts_with('/'))
        .unwrap_or_else(|| "/".into());
    let cookie = Cookie::build((COOKIE_NAME, sess.token))
        .path("/")
        .secure(cookie_secure())
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::days(30))
        .build();
    let jar = jar.add(cookie);

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

pub async fn logout(State(state): State<AppState>, jar: SignedCookieJar) -> Response {
    if let Some(c) = jar.get(COOKIE_NAME) {
        let _ = logout_destroy_session(&state.db, c.value()).await;
    }
    let removed = Cookie::build((COOKIE_NAME, ""))
        .path("/")
        .secure(cookie_secure())
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::seconds(0))
        .build();
    let jar = jar.add(removed);
    (jar, Redirect::to("/login")).into_response()
}

fn error500(msg: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!("internal: {msg}"),
    )
        .into_response()
}
