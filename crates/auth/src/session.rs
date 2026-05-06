use rand::RngCore;
use sqlx::SqlitePool;
use time::OffsetDateTime;

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
        let _ = sqlx::query("DELETE FROM session WHERE token = ?1")
            .bind(&token)
            .execute(pool)
            .await;
        return Ok(None);
    }
    // Sliding renewal.
    if last_seen < now - 3600 {
        let _ = sqlx::query("UPDATE session SET last_seen = ?1 WHERE token = ?2")
            .bind(now)
            .bind(&token)
            .execute(pool)
            .await;
    }
    if expires_at < now + 7 * 24 * 3600 {
        let _ = sqlx::query("UPDATE session SET expires_at = ?1 WHERE token = ?2")
            .bind(now + SESSION_LIFETIME_SECS)
            .bind(&token)
            .execute(pool)
            .await;
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
}
