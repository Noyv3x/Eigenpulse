use axum::{
    extract::{Form, Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, SameSite, SignedCookieJar};
use ep_auth::{verify_password, login_create_session, COOKIE_NAME, logout_destroy_session};
use ep_core::AppState;
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
pub struct NextQuery { #[serde(default)] pub next: Option<String>, #[serde(default)] pub error: Option<String> }

pub async fn page(Query(q): Query<NextQuery>) -> Html<String> {
    let next = q.next.unwrap_or_else(|| "/".into());
    let err_block = if q.error.is_some() {
        r#"<div style="background:var(--rose-soft);color:var(--rose-ink);padding:8px 12px;border-radius:8px;font-size:13px;margin-bottom:14px">密码错误，请重试</div>"#
    } else { "" };
    let html = format!(r##"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width,initial-scale=1"/>
<title>登录 · Eigenpulse</title>
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
        <div class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">Personal ERP · 登录</div>
      </div>
    </div>
    {err_block}
    <input type="hidden" name="next" value="{next_html}"/>
    <label style="display:block;font-size:12px;color:var(--ink-3);margin-bottom:6px;font-family:var(--font-mono);text-transform:uppercase;letter-spacing:0.06em">密码 · PASSWORD</label>
    <input type="password" name="password" autofocus required
           style="width:100%;padding:10px 12px;border:1px solid var(--border);border-radius:8px;background:var(--bg-2);font-family:var(--font-mono);font-size:14px;margin-bottom:16px"/>
    <button class="btn primary" type="submit" style="width:100%;justify-content:center;padding:9px 14px;font-size:13.5px">登录 · LOGIN</button>
    <p class="muted" style="font-size:11.5px;margin-top:18px;line-height:1.5">单用户系统 · 密码由 EP_ADMIN_PASSWORD 环境变量在首次启动时设定。</p>
  </form>
</div>
</body></html>"##, err_block = err_block, next_html = ep_core::html_escape(&next));
    Html(html)
}

#[derive(Debug, Deserialize)]
pub struct LoginInput { pub password: String, #[serde(default)] pub next: Option<String> }

pub async fn submit(
    State(state): State<AppState>,
    jar: SignedCookieJar,
    Form(input): Form<LoginInput>,
) -> Response {
    let row: Option<(i64, String)> = match sqlx::query_as(
        "SELECT id, password_hash FROM app_user WHERE id = 1"
    ).fetch_optional(&state.db).await {
        Ok(r) => r,
        Err(e) => return error500(&e.to_string()),
    };
    let Some((user_id, hash)) = row else {
        return Redirect::temporary("/login?error=1").into_response();
    };
    let password = input.password.clone();
    let ok = tokio::task::spawn_blocking(move || verify_password(&password, &hash).unwrap_or(false))
        .await
        .unwrap_or(false);
    if !ok { return Redirect::temporary("/login?error=1").into_response(); }
    let sess = match login_create_session(&state.db, user_id).await {
        Ok(s) => s,
        Err(e) => return error500(&e.to_string()),
    };
    let next = input.next.filter(|s| s.starts_with('/')).unwrap_or_else(|| "/".into());
    let cookie = Cookie::build((COOKIE_NAME, sess.token))
        .path("/")
        .secure(cookie_secure())
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::days(30))
        .build();
    let jar = jar.add(cookie);
    (jar, Redirect::to(&next)).into_response()
}

pub async fn logout(
    State(state): State<AppState>,
    jar: SignedCookieJar,
) -> Response {
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
    ).into_response()
}

