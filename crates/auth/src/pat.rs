use axum::{
    extract::{OriginalUri, Request, State},
    http::{header, HeaderMap},
    middleware::Next,
    response::Response,
};
use ep_core::AppState;
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

#[derive(Clone, Debug)]
pub struct AuthPat {
    pub id: i64,
    pub name: String,
    pub scopes: Vec<String>,
}

/// `pat` table row minus the secret `hash` column. Columns line up so
/// `sqlx::FromRow` decodes `list_pats` rows directly.
#[derive(Debug, Clone, sqlx::FromRow)]
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

/// Auth-path projection: just the columns `require_pat` needs to validate a
/// bearer token. Stays a tuple — it is destructured immediately, never stored.
type PatAuthRow = (i64, String, String, Option<i64>, Option<i64>, Option<i64>);

pub const MAX_PAT_NAME_CHARS: usize = 64;

/// Minimum interval between `pat.last_used_at` writes. Read-only API calls hit
/// `require_pat` on every request; without this throttle each one would issue a
/// `UPDATE`, making GETs write-bound. 3600s matches the `session.last_seen`
/// sliding-renewal window in `session.rs`.
const LAST_USED_THROTTLE_SECS: i64 = 3600;

fn hash_token(token: &str) -> String {
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

pub(crate) fn base62_encode(bytes: &[u8]) -> String {
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
    let name = normalize_pat_name(name)?;
    let scopes_s = normalize_pat_scopes(scopes)?;
    if expires_at.is_some_and(|timestamp| !ep_core::is_valid_app_timestamp(timestamp)) {
        anyhow::bail!("PAT expiration is outside the supported application calendar");
    }
    let token = random_token();
    let prefix = token.chars().take(12).collect::<String>();
    let hash = hash_token(&token);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO pat (name, prefix, hash, scopes, expires_at)
         VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
    )
    .bind(&name)
    .bind(&prefix)
    .bind(&hash)
    .bind(&scopes_s)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;
    let row = PatRow {
        id,
        name,
        prefix,
        scopes: scopes_s,
        created_at: ep_core::unix_now(),
        expires_at,
        last_used_at: None,
        revoked_at: None,
    };
    Ok((token, row))
}

fn normalize_pat_name(name: &str) -> anyhow::Result<String> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("PAT name is required");
    }
    if name.chars().count() > MAX_PAT_NAME_CHARS {
        anyhow::bail!("PAT name must be at most {MAX_PAT_NAME_CHARS} characters");
    }
    Ok(name.to_string())
}

fn normalize_pat_scopes(scopes: &[&str]) -> anyhow::Result<String> {
    let mut normalized: Vec<String> = Vec::new();
    for raw in scopes {
        for scope in raw.split_whitespace() {
            if !normalized.iter().any(|existing| existing == scope) {
                normalized.push(scope.to_string());
            }
        }
    }
    if normalized.is_empty() {
        anyhow::bail!("PAT scopes are required");
    }
    Ok(normalized.join(" "))
}

pub async fn list_pats(pool: &SqlitePool) -> anyhow::Result<Vec<PatRow>> {
    let rows = sqlx::query_as::<_, PatRow>(
        "SELECT id, name, prefix, scopes, created_at, expires_at, last_used_at, revoked_at
               FROM pat ORDER BY revoked_at IS NOT NULL, created_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn revoke_pat(pool: &SqlitePool, id: i64) -> anyhow::Result<bool> {
    let now = ep_core::unix_now();
    let res = sqlx::query("UPDATE pat SET revoked_at = ?1 WHERE id = ?2 AND revoked_at IS NULL")
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected() > 0)
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
    if is_public_open_api_health_request(&req) {
        return next.run(req).await;
    }
    let token = match extract_bearer(req.headers()) {
        Some(t) if t.starts_with("ep_pat_") => t.to_string(),
        _ => return unauthorized(),
    };
    let h = hash_token(&token);
    let row: Option<PatAuthRow> = match sqlx::query_as(
        "SELECT id, name, scopes, expires_at, revoked_at, last_used_at
           FROM pat WHERE hash = ?1",
    )
    .bind(&h)
    .fetch_optional(&state.db)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::warn!(error = %e, "PAT lookup failed");
            return unauthorized();
        }
    };
    let Some((id, name, scopes, expires_at, revoked_at, last_used_at)) = row else {
        return unauthorized();
    };
    if revoked_at.is_some() {
        return unauthorized();
    }
    let now = ep_core::unix_now();
    if expires_at.map(|e| e <= now).unwrap_or(false) {
        return unauthorized();
    }
    let scopes_v: Vec<String> = scopes.split_whitespace().map(|s| s.to_string()).collect();
    // Throttle the `last_used_at` write so read-only API traffic isn't
    // write-bound: only touch the row when it has never been used or the
    // recorded timestamp is older than `LAST_USED_THROTTLE_SECS`. The auth
    // projection includes that timestamp so a fresh GET issues no UPDATE at
    // all; the WHERE still closes a race between concurrent stale requests.
    if should_touch_last_used(last_used_at, now) {
        if let Err(e) = sqlx::query(
            "UPDATE pat SET last_used_at = ?1
             WHERE id = ?2 AND (last_used_at IS NULL OR last_used_at < ?3)",
        )
        .bind(now)
        .bind(id)
        .bind(now - LAST_USED_THROTTLE_SECS)
        .execute(&state.db)
        .await
        {
            tracing::warn!(pat_id = id, error = %e, "failed to update PAT last_used_at");
        }
    }
    let mut req = req;
    req.extensions_mut().insert(AuthPat {
        id,
        name,
        scopes: scopes_v,
    });
    next.run(req).await
}

fn should_touch_last_used(last_used_at: Option<i64>, now: i64) -> bool {
    last_used_at.is_none_or(|last| last < now - LAST_USED_THROTTLE_SECS)
}

fn unauthorized() -> Response {
    crate::unauthorized("missing or invalid PAT")
}

fn is_public_open_api_healthz(path: &str) -> bool {
    path == "/api/v1/healthz"
}

fn is_public_open_api_health_request(req: &Request) -> bool {
    // `Router::nest` strips the mount prefix from `req.uri()`, while axum keeps
    // the canonical request path in `OriginalUri`. Always apply the public
    // exception to that canonical path so a nested `/healthz` cannot inherit it.
    let request_path = req
        .extensions()
        .get::<OriginalUri>()
        .map(|uri| uri.0.path())
        .unwrap_or_else(|| req.uri().path());
    is_public_open_api_healthz(request_path)
}

/// Helper for handlers to verify the current request bears a required scope.
#[allow(
    clippy::result_large_err,
    reason = "axum handlers consume Response directly"
)]
pub fn require_scope(pat: &AuthPat, scope: &str) -> Result<(), Response> {
    if pat
        .scopes
        .iter()
        .any(|s| s == scope || s == ep_core::SCOPE_ALL)
    {
        Ok(())
    } else {
        Err(crate::json_error(
            axum::http::StatusCode::FORBIDDEN,
            "forbidden",
            &format!("requires scope: {scope}"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        generate_pat, is_public_open_api_health_request, is_public_open_api_healthz,
        normalize_pat_name, normalize_pat_scopes, random_token, require_scope, revoke_pat, AuthPat,
        MAX_PAT_NAME_CHARS,
    };
    use axum::{body, extract::OriginalUri};
    use sqlx::SqlitePool;

    const TEST_READ_SCOPE: &str = "example:read";
    const TEST_WRITE_SCOPE: &str = "example:write";

    #[test]
    fn only_top_level_open_api_healthz_is_public() {
        assert!(is_public_open_api_healthz("/api/v1/healthz"));
        assert!(!is_public_open_api_healthz("/definitely-not-public"));
        assert!(!is_public_open_api_healthz("/api/v1/fin/healthz"));
        assert!(!is_public_open_api_healthz("/api/v1/healthz/extra"));
    }

    #[test]
    fn nested_health_probe_uses_the_canonical_original_uri() {
        let nested = axum::extract::Request::builder()
            .uri("/healthz")
            .extension(OriginalUri("/api/v1/healthz".parse().unwrap()))
            .body(body::Body::empty())
            .unwrap();
        assert!(is_public_open_api_health_request(&nested));

        let stripped_only = axum::extract::Request::builder()
            .uri("/healthz")
            .body(body::Body::empty())
            .unwrap();
        assert!(!is_public_open_api_health_request(&stripped_only));
    }

    #[test]
    fn last_used_read_path_skips_fresh_timestamps() {
        let now = 1_700_000_000;
        assert!(super::should_touch_last_used(None, now));
        assert!(!super::should_touch_last_used(Some(now), now));
        assert!(!super::should_touch_last_used(
            Some(now - super::LAST_USED_THROTTLE_SECS),
            now
        ));
        assert!(super::should_touch_last_used(
            Some(now - super::LAST_USED_THROTTLE_SECS - 1),
            now
        ));
    }

    #[test]
    fn require_scope_accepts_exact_scope_or_wildcard() {
        let exact = AuthPat {
            id: 1,
            name: "exact".into(),
            scopes: vec![TEST_READ_SCOPE.into()],
        };
        assert!(require_scope(&exact, TEST_READ_SCOPE).is_ok());
        assert!(require_scope(&exact, TEST_WRITE_SCOPE).is_err());

        let wildcard = AuthPat {
            id: 2,
            name: "all".into(),
            scopes: vec![ep_core::SCOPE_ALL.into()],
        };
        assert!(require_scope(&wildcard, TEST_WRITE_SCOPE).is_ok());
        assert!(require_scope(&wildcard, ep_core::SCOPE_NOTIFICATIONS_WRITE).is_ok());
    }

    #[test]
    fn random_token_uses_expected_prefix_length_and_charset() {
        let token = random_token();

        assert!(token.starts_with("ep_pat_"));
        assert_eq!(token.len(), 43);
        assert!(token["ep_pat_".len()..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric()));
    }

    #[tokio::test]
    async fn require_scope_forbidden_response_is_valid_json() {
        let pat = AuthPat {
            id: 1,
            name: "limited".into(),
            scopes: vec![TEST_READ_SCOPE.into()],
        };

        let response = require_scope(&pat, r#"bad"scope\"#).expect_err("scope should be denied");
        let body = body::to_bytes(response.into_body(), 16 * 1024)
            .await
            .expect("body");
        let json: serde_json::Value = serde_json::from_slice(&body).expect("valid json");

        assert_eq!(json["error"]["code"], "forbidden");
        assert_eq!(json["error"]["message"], r#"requires scope: bad"scope\"#);
    }

    async fn pat_test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.expect("pool");
        sqlx::query(
            "CREATE TABLE pat (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                name         TEXT NOT NULL,
                prefix       TEXT NOT NULL,
                hash         TEXT NOT NULL,
                scopes       TEXT NOT NULL,
                created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
                expires_at   INTEGER,
                last_used_at INTEGER,
                revoked_at   INTEGER
            )",
        )
        .execute(&pool)
        .await
        .expect("pat table");
        pool
    }

    #[tokio::test]
    async fn revoke_pat_reports_whether_active_token_was_updated() {
        let pool = pat_test_pool().await;
        let (_token, row) = generate_pat(&pool, "test", &[TEST_READ_SCOPE], None)
            .await
            .expect("pat");

        assert!(revoke_pat(&pool, row.id).await.expect("first revoke"));
        assert!(!revoke_pat(&pool, row.id)
            .await
            .expect("second revoke is no-op"));
        assert!(!revoke_pat(&pool, row.id + 100)
            .await
            .expect("missing revoke is no-op"));
    }

    #[tokio::test]
    async fn generate_pat_trims_and_dedupes_boundary_fields() {
        let pool = pat_test_pool().await;
        let joined_scopes = format!("{} {}", TEST_READ_SCOPE, ep_core::SCOPE_NOTIFICATIONS_WRITE);
        let (_token, row) = generate_pat(
            &pool,
            "  iOS Shortcuts  ",
            &[&joined_scopes, TEST_READ_SCOPE],
            None,
        )
        .await
        .expect("pat");

        assert_eq!(row.name, "iOS Shortcuts");
        assert_eq!(
            row.scopes,
            format!("{} {}", TEST_READ_SCOPE, ep_core::SCOPE_NOTIFICATIONS_WRITE)
        );
    }

    #[tokio::test]
    async fn generate_pat_rejects_expiration_outside_the_shared_calendar() {
        let pool = pat_test_pool().await;
        let err = generate_pat(&pool, "far future", &[TEST_READ_SCOPE], Some(i64::MAX))
            .await
            .expect_err("unrenderable expiration must be rejected");
        assert!(err.to_string().contains("supported application calendar"));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pat")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn last_used_at_write_is_throttled_by_window() {
        let pool = pat_test_pool().await;
        let (_token, row) = generate_pat(&pool, "throttle", &[TEST_READ_SCOPE], None)
            .await
            .expect("pat");

        // Same UPDATE shape `require_pat` issues. Helper returns whether the
        // row was actually touched (`rows_affected() > 0`).
        async fn touch(pool: &SqlitePool, id: i64, now: i64) -> u64 {
            sqlx::query(
                "UPDATE pat SET last_used_at = ?1
                 WHERE id = ?2 AND (last_used_at IS NULL OR last_used_at < ?3)",
            )
            .bind(now)
            .bind(id)
            .bind(now - super::LAST_USED_THROTTLE_SECS)
            .execute(pool)
            .await
            .expect("update")
            .rows_affected()
        }

        let t0 = 1_700_000_000;
        // First call: last_used_at is NULL → writes.
        assert_eq!(touch(&pool, row.id, t0).await, 1);
        // Within the window → skipped.
        assert_eq!(touch(&pool, row.id, t0 + 10).await, 0);
        assert_eq!(
            touch(&pool, row.id, t0 + super::LAST_USED_THROTTLE_SECS).await,
            0
        );
        // Past the window → writes again.
        assert_eq!(
            touch(&pool, row.id, t0 + super::LAST_USED_THROTTLE_SECS + 1).await,
            1
        );
    }

    #[test]
    fn normalize_pat_name_rejects_blank_and_overlong_values() {
        assert!(normalize_pat_name("   ").is_err());
        assert!(normalize_pat_name(&"x".repeat(MAX_PAT_NAME_CHARS + 1)).is_err());
    }

    #[test]
    fn normalize_pat_scopes_rejects_empty_and_splits_whitespace() {
        assert!(normalize_pat_scopes(&[]).is_err());
        assert!(normalize_pat_scopes(&["   "]).is_err());
        assert_eq!(
            normalize_pat_scopes(&[
                &format!(
                    " {}  {} ",
                    TEST_READ_SCOPE,
                    ep_core::SCOPE_NOTIFICATIONS_WRITE
                ),
                TEST_READ_SCOPE
            ])
            .expect("scopes"),
            format!("{} {}", TEST_READ_SCOPE, ep_core::SCOPE_NOTIFICATIONS_WRITE)
        );
    }
}
