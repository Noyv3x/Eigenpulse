use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::rngs::OsRng;

fn hasher() -> anyhow::Result<Argon2<'static>> {
    let params = Params::new(19_456, 2, 1, None)
        .map_err(|e| anyhow::anyhow!("invalid argon2 params: {e}"))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

pub fn hash_password(plain: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = hasher()?
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("argon2 hash failed: {e}"))?
        .to_string();
    Ok(hash)
}

pub fn verify_password(plain: &str, encoded: &str) -> anyhow::Result<bool> {
    let parsed =
        PasswordHash::new(encoded).map_err(|e| anyhow::anyhow!("argon2 parse failed: {e}"))?;
    Ok(hasher()?.verify_password(plain.as_bytes(), &parsed).is_ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_verify_sync_roundtrip() {
        let h = hash_password("hunter2_test").unwrap();
        assert!(verify_password("hunter2_test", &h).unwrap());
        assert!(!verify_password("wrong", &h).unwrap());
    }

    #[test]
    fn hash_produces_distinct_outputs_for_same_input() {
        // SaltString::generate(OsRng) means two hashes of the same plaintext
        // must differ (otherwise we have a salt collision or a bug).
        let a = hash_password("same-plain").unwrap();
        let b = hash_password("same-plain").unwrap();
        assert_ne!(
            a, b,
            "same plaintext must produce different hashes (random salt)"
        );
        assert!(verify_password("same-plain", &a).unwrap());
        assert!(verify_password("same-plain", &b).unwrap());
    }

    #[test]
    fn verify_rejects_garbage_hash() {
        assert!(verify_password("anything", "not-a-real-hash").is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn hash_verify_async_roundtrip() {
        let h = hash_password_async("test1234".into()).await.unwrap();
        assert!(verify_password_async("test1234".into(), h.clone())
            .await
            .unwrap());
        assert!(!verify_password_async("nope".into(), h).await.unwrap());
    }
}
