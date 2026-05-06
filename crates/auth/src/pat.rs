use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use ep_core::AppState;
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use time::OffsetDateTime;

#[derive(Clone, Debug)]
pub struct AuthPat {
    pub id: i64,
    pub name: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PatRow {
    pub id: i64,
    pub name: String,
    pub prefix: String,
    pub scopes: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

pub fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}

fn random_token() -> String {
    let mut buf = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut buf);
    let mut s = String::from("ep_pat_");
    s.push_str(&base62_encode(&buf));
    s
}

fn base62_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut out = String::new();
    for chunk in bytes.chunks(2) {
        let mut v: u32 = 0;
        for b in chunk {
            v = (v << 8) | (*b as u32);
        }
        for _ in 0..3 {
            out.push(ALPHABET[(v % 62) as usize] as char);
            v /= 62;
        }
    }
    out
}

pub async fn generate_pat(
    pool: &SqlitePool,
    name: &str,
    scopes: &[&str],
    expires_at: Option<i64>,
) -> anyhow::Result<(String, PatRow)> {
    let token = random_token();
    let prefix = token.chars().take(12).collect::<String>();
    let hash = hash_token(&token);
    let scopes_s = scopes.join(" ");
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO pat (name, prefix, hash, scopes, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
    )
    .bind(name)
    .bind(&prefix)
    .bind(&hash)
    .bind(&scopes_s)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;
    let row = PatRow {
        id,
        name: name.into(),
        prefix,
        scopes: scopes_s,
        created_at: OffsetDateTime::now_utc().unix_timestamp(),
        expires_at,
        last_used_at: None,
        revoked_at: None,
    };
    Ok((token, row))
}

pub async fn list_pats(pool: &SqlitePool) -> anyhow::Result<Vec<PatRow>> {
    let rows: Vec<(
        i64,
        String,
        String,
        String,
        i64,
        Option<i64>,
        Option<i64>,
        Option<i64>,
    )> = sqlx::query_as(
        "SELECT id, name, prefix, scopes, created_at, expires_at, last_used_at, revoked_at
               FROM pat ORDER BY revoked_at IS NOT NULL, created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| PatRow {
            id: r.0,
            name: r.1,
            prefix: r.2,
            scopes: r.3,
            created_at: r.4,
            expires_at: r.5,
            last_used_at: r.6,
            revoked_at: r.7,
        })
        .collect())
}

pub async fn revoke_pat(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    let now = OffsetDateTime::now_utc().unix_timestamp();
    sqlx::query("UPDATE pat SET revoked_at = ?1 WHERE id = ?2 AND revoked_at IS NULL")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.strip_prefix("Bearer ")
                .or_else(|| s.strip_prefix("bearer "))
        })
}

pub async fn require_pat(State(state): State<AppState>, req: Request, next: Next) -> Response {
    // Allow unauthenticated /api/v1/healthz
    if req.uri().path().ends_with("/healthz") {
        return next.run(req).await;
    }
    let token = match extract_bearer(req.headers()) {
        Some(t) if t.starts_with("ep_pat_") => t.to_string(),
        _ => return unauthorized(),
    };
    let h = hash_token(&token);
    let row: Option<(i64, String, String, Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT id, name, scopes, expires_at, revoked_at FROM pat WHERE hash = ?1")
            .bind(&h)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();
    let Some((id, name, scopes, expires_at, revoked_at)) = row else {
        return unauthorized();
    };
    if revoked_at.is_some() {
        return unauthorized();
    }
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if expires_at.map(|e| e <= now).unwrap_or(false) {
        return unauthorized();
    }
    let scopes_v: Vec<String> = scopes.split_whitespace().map(|s| s.to_string()).collect();
    let _ = sqlx::query("UPDATE pat SET last_used_at = ?1 WHERE id = ?2")
        .bind(now)
        .bind(id)
        .execute(&state.db)
        .await;
    let mut req = req;
    req.extensions_mut().insert(AuthPat {
        id,
        name,
        scopes: scopes_v,
    });
    next.run(req).await
}

fn unauthorized() -> Response {
    crate::unauthorized("missing or invalid PAT")
}

/// Helper for handlers to verify the current request bears a required scope.
pub fn require_scope(pat: &AuthPat, scope: &str) -> Result<(), Response> {
    if pat.scopes.iter().any(|s| s == scope || s == "*") {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            format!(r#"{{"error":{{"code":"forbidden","message":"requires scope: {scope}"}}}}"#),
        )
            .into_response())
    }
}
