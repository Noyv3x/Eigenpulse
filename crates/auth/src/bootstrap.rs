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
    if password.len() < 6 {
        anyhow::bail!("EP_ADMIN_PASSWORD must be at least 6 characters");
    }
    let hash = crate::hash_password(&password)?;
    sqlx::query(
        "INSERT INTO app_user (id, password_hash) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET password_hash = excluded.password_hash",
    )
    .bind(&hash)
    .execute(pool)
    .await?;
    tracing::warn!(
        "OWNER account bootstrapped from EP_ADMIN_PASSWORD; consider rotating via /settings/security"
    );
    Ok(())
}
