pub use ep_core::{MAX_PASSWORD_BYTES, MIN_PASSWORD_CHARS};

pub fn validate_password(password: &str) -> anyhow::Result<&str> {
    let chars = password.chars().count();
    if chars < MIN_PASSWORD_CHARS {
        anyhow::bail!("password must be at least {MIN_PASSWORD_CHARS} characters (got {chars})");
    }
    if password.len() > MAX_PASSWORD_BYTES {
        anyhow::bail!(
            "password must be at most {MAX_PASSWORD_BYTES} UTF-8 bytes (got {})",
            password.len()
        );
    }
    Ok(password)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_bounds_are_shared_and_utf8_aware() {
        assert!(validate_password("12345").is_err());
        assert_eq!(validate_password("123456").unwrap(), "123456");
        assert!(validate_password(&"x".repeat(MAX_PASSWORD_BYTES)).is_ok());
        assert!(validate_password(&"x".repeat(MAX_PASSWORD_BYTES + 1)).is_err());
        assert!(validate_password(&"密".repeat(MAX_PASSWORD_BYTES / 3 + 1)).is_err());
    }
}
