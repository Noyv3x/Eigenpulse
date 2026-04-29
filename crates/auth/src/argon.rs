use argon2::password_hash::{PasswordHasher, PasswordVerifier, SaltString, PasswordHash};
use argon2::{Argon2, Algorithm, Version, Params};
use rand::rngs::OsRng;

fn hasher() -> Argon2<'static> {
    let params = Params::new(19_456, 2, 1, None).expect("valid argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

pub fn hash_password(plain: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = hasher()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("argon2 hash failed: {e}"))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(plain: &str, encoded: &str) -> anyhow::Result<bool> {
    let parsed = PasswordHash::new(encoded)
        .map_err(|e| anyhow::anyhow!("argon2 parse failed: {e}"))?;
    Ok(hasher().verify_password(plain.as_bytes(), &parsed).is_ok())
}

/// Async wrapper for `hash_password` that bounces the ~150 ms Argon2id
/// computation onto the blocking pool. Use from server fns / axum handlers
/// so the leptos runtime / tower worker isn't parked.
pub async fn hash_password_async(plain: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&plain)).await?
}

/// Async wrapper for `verify_password`. Same rationale as `hash_password_async`.
pub async fn verify_password_async(plain: String, encoded: String) -> anyhow::Result<bool> {
    tokio::task::spawn_blocking(move || verify_password(&plain, &encoded)).await?
}
