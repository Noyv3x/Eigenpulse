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
