use sqlx::SqlitePool;

pub async fn bootstrap_admin(pool: &SqlitePool) -> anyhow::Result<()> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM app_user WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    if exists.is_some() {
        return Ok(());
    }
    let password = std::env::var("EP_ADMIN_PASSWORD").map_err(|_| {
        anyhow::anyhow!(
            "EP_ADMIN_PASSWORD env var is required for first boot to create the OWNER account"
        )
    })?;
    let password = normalize_bootstrap_password(&password)?;
    let hash = crate::hash_password(password)?;
    if insert_owner_if_absent(pool, &hash).await? {
        tracing::warn!(
            "OWNER account bootstrapped from EP_ADMIN_PASSWORD; consider rotating via /settings/security"
        );
    }
    Ok(())
}

/// The initial existence read avoids requiring the environment variable after
/// first boot, while this conflict-safe INSERT closes the two-process startup
/// race. A losing bootstrap must never rotate the winner's password.
async fn insert_owner_if_absent(pool: &SqlitePool, hash: &str) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "INSERT INTO app_user (id, handle, name, role, password_hash)
         VALUES (1, 'admin', 'Owner', 'OWNER', ?1)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(hash)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

fn normalize_bootstrap_password(password: &str) -> anyhow::Result<&str> {
    let password = password.trim();
    crate::validate_password(password)
}

#[cfg(test)]
mod tests {
    use super::{insert_owner_if_absent, normalize_bootstrap_password};
    use sqlx::SqlitePool;

    async fn user_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE app_user (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                handle TEXT NOT NULL,
                name TEXT NOT NULL,
                role TEXT NOT NULL,
                password_hash TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[test]
    fn normalize_bootstrap_password_trims_boundary_whitespace() {
        assert_eq!(
            normalize_bootstrap_password("  dev-password\n").unwrap(),
            "dev-password"
        );
    }

    #[test]
    fn normalize_bootstrap_password_rejects_blank_or_short_values() {
        assert!(normalize_bootstrap_password("      ").is_err());
        assert!(normalize_bootstrap_password(" 12345 ").is_err());
        assert!(normalize_bootstrap_password(&"x".repeat(crate::MAX_PASSWORD_BYTES + 1)).is_err());
    }

    #[tokio::test]
    async fn concurrent_bootstraps_never_overwrite_the_winner() {
        let pool = user_pool().await;
        let (left, right) = tokio::join!(
            insert_owner_if_absent(&pool, "hash-left"),
            insert_owner_if_absent(&pool, "hash-right")
        );
        let left = left.unwrap();
        let right = right.unwrap();
        assert_ne!(left, right, "exactly one bootstrap should insert");

        let stored: String = sqlx::query_scalar("SELECT password_hash FROM app_user WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, if left { "hash-left" } else { "hash-right" });

        assert!(!insert_owner_if_absent(&pool, "must-not-replace")
            .await
            .unwrap());
        let unchanged: String =
            sqlx::query_scalar("SELECT password_hash FROM app_user WHERE id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(unchanged, stored);
    }
}
