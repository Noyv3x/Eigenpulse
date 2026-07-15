use crate::locale::{Locale, LOCALE_COOKIE, LOCALE_COOKIE_MAX_AGE_SECS};

/// Mirrors the `ep_tweaks` cookie style (SameSite=Lax, no `Secure` —
/// the NAS/LAN HTTP deployment can't persist a Secure cookie).
pub fn build_set_cookie(locale: Locale) -> String {
    format!(
        "{LOCALE_COOKIE}={value}; Path=/; Max-Age={max}; SameSite=Lax",
        value = locale.as_code(),
        max = LOCALE_COOKIE_MAX_AGE_SECS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_zh() {
        let s = build_set_cookie(Locale::ZhCn);
        assert!(s.starts_with("ep_locale=zh-CN; "));
        assert!(s.contains("Max-Age=31536000"));
        assert!(s.contains("SameSite=Lax"));
        assert!(!s.contains("Secure"));
    }
}
