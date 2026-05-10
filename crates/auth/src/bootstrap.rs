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
    sqlx::query(
        "INSERT INTO app_user (id, handle, name, role, password_hash)
         VALUES (1, 'admin', 'Owner', 'OWNER', ?1)
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

fn normalize_bootstrap_password(password: &str) -> anyhow::Result<&str> {
    let password = password.trim();
    if password.chars().count() < 6 {
        anyhow::bail!("EP_ADMIN_PASSWORD must be at least 6 characters");
    }
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::normalize_bootstrap_password;

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
    }
}
