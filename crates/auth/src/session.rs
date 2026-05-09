use axum_extra::extract::cookie::{Cookie, SameSite};
use rand::RngCore;
use sqlx::SqlitePool;
use time::{Duration, OffsetDateTime};

pub const COOKIE_NAME: &str = "ep_sid";
pub const SESSION_LIFETIME_SECS: i64 = 30 * 24 * 60 * 60; // 30d

#[derive(Clone, Debug)]
pub struct Session {
    pub token: String,
    pub user_id: i64,
    pub expires_at: i64,
}

#[derive(Clone, Debug)]
pub struct AuthUser {
    pub id: i64,
    pub handle: String,
    pub name: String,
    pub role: String,
}

pub fn random_token() -> String {
    let mut buf = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

/// HTTPS-only cookie if `EP_COOKIE_SECURE=1` (recommended for production).
/// Default false so local HTTP / NAS-LAN deployments can persist sessions.
pub fn cookie_secure() -> bool {
    std::env::var("EP_COOKIE_SECURE")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

pub fn session_cookie(token: impl Into<String>) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, token.into()))
        .path("/")
        .secure(cookie_secure())
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::seconds(SESSION_LIFETIME_SECS))
        .build()
}

pub fn expired_session_cookie() -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, ""))
        .path("/")
        .secure(cookie_secure())
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::seconds(0))
        .build()
}

pub async fn login_create_session(pool: &SqlitePool, user_id: i64) -> anyhow::Result<Session> {
    let token = random_token();
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let expires_at = now + SESSION_LIFETIME_SECS;
    sqlx::query(
        "INSERT INTO session (token, user_id, issued_at, expires_at, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?3)",
    )
    .bind(&token)
    .bind(user_id)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(Session {
        token,
        user_id,
        expires_at,
    })
}

pub async fn logout_destroy_session(pool: &SqlitePool, token: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM session WHERE token = ?1")
        .bind(token)
        .execute(pool)
        .await?;
    Ok(())
}

/// Forcibly invalidate every cookie session in the DB (including the caller's
/// own). Used by password rotation paths — both the in-app
/// `/settings/security` change-password server fn and the
/// `crates/auth/examples/reset_password.rs` recovery CLI — to guarantee a
/// pre-rotation cookie can't outlive the new credential. Generic over
/// `SqliteExecutor` so callers in a transaction can pass `&mut *tx`.
pub async fn purge_all_sessions<'e, E>(executor: E) -> sqlx::Result<u64>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let result = sqlx::query("DELETE FROM session").execute(executor).await?;
    Ok(result.rows_affected())
}

pub async fn lookup_session(
    pool: &SqlitePool,
    token: &str,
) -> anyhow::Result<Option<(Session, AuthUser)>> {
    let row: Option<(String, i64, i64, i64, String, String, String)> = sqlx::query_as(
        "SELECT s.token, s.user_id, s.expires_at, s.last_seen, u.handle, u.name, u.role
           FROM session s
           JOIN app_user u ON u.id = s.user_id
          WHERE s.token = ?1",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    let Some((token, user_id, expires_at, last_seen, handle, name, role)) = row else {
        return Ok(None);
    };
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if expires_at <= now {
        if let Err(e) = sqlx::query("DELETE FROM session WHERE token = ?1")
            .bind(&token)
            .execute(pool)
            .await
        {
            tracing::warn!(error = %e, "failed to delete expired session");
        }
        return Ok(None);
    }
    // Sliding renewal.
    if last_seen < now - 3600 {
        if let Err(e) = sqlx::query("UPDATE session SET last_seen = ?1 WHERE token = ?2")
            .bind(now)
            .bind(&token)
            .execute(pool)
            .await
        {
            tracing::warn!(error = %e, "failed to update session last_seen");
        }
    }
    if should_refresh_session(expires_at, now) {
        if let Err(e) = sqlx::query("UPDATE session SET expires_at = ?1 WHERE token = ?2")
            .bind(now + SESSION_LIFETIME_SECS)
            .bind(&token)
            .execute(pool)
            .await
        {
            tracing::warn!(error = %e, "failed to extend session expiry");
        }
    }
    Ok(Some((
        Session {
            token,
            user_id,
            expires_at,
        },
        AuthUser {
            id: user_id,
            handle,
            name,
            role,
        },
    )))
}

pub fn should_refresh_session(expires_at: i64, now: i64) -> bool {
    expires_at < now + 7 * 24 * 3600
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a fresh in-memory pool with the minimal `session` schema. We
    /// don't run the full `0001_init.sql` because it has FK references
    /// (session.user_id → app_user.id) that aren't load-bearing for this
    /// helper's behavior — purge_all_sessions doesn't care who owns each row.
    async fn fixture_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE session (
                token TEXT PRIMARY KEY,
                user_id INTEGER NOT NULL,
                issued_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                last_seen INTEGER NOT NULL
             )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test(flavor = "current_thread")]
    async fn purge_all_sessions_clears_populated_table() {
        let pool = fixture_pool().await;
        for i in 0..3 {
            sqlx::query("INSERT INTO session VALUES (?, 1, 0, 0, 0)")
                .bind(format!("tok-{i}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        let purged = purge_all_sessions(&pool).await.unwrap();
        assert_eq!(purged, 3);
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn purge_all_sessions_on_empty_table_is_zero() {
        let pool = fixture_pool().await;
        let purged = purge_all_sessions(&pool).await.unwrap();
        assert_eq!(purged, 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn purge_all_sessions_works_inside_transaction() {
        let pool = fixture_pool().await;
        sqlx::query("INSERT INTO session VALUES ('a', 1, 0, 0, 0)")
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        // Generic-over-Executor: should accept `&mut *tx`, not just `&pool`.
        let purged = purge_all_sessions(&mut *tx).await.unwrap();
        assert_eq!(purged, 1);
        tx.commit().await.unwrap();
        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn should_refresh_session_inside_last_week_only() {
        let now = 1_700_000_000;
        assert!(should_refresh_session(now + 6 * 24 * 3600, now));
        assert!(!should_refresh_session(now + 8 * 24 * 3600, now));
    }

    #[test]
    fn session_cookie_uses_browser_session_attributes() {
        let cookie = session_cookie("token");
        assert_eq!(cookie.name(), COOKIE_NAME);
        assert_eq!(cookie.value(), "token");
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert_eq!(
            cookie.max_age().map(|d| d.whole_seconds()),
            Some(SESSION_LIFETIME_SECS)
        );
    }

    #[test]
    fn expired_session_cookie_clears_browser_cookie() {
        let cookie = expired_session_cookie();
        assert_eq!(cookie.name(), COOKIE_NAME);
        assert_eq!(cookie.value(), "");
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.same_site(), Some(SameSite::Lax));
        assert_eq!(cookie.max_age().map(|d| d.whole_seconds()), Some(0));
    }
}
