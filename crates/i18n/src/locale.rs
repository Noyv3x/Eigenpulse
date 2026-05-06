use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Locale {
    #[default]
    ZhCn,
    En,
}

impl Locale {
    pub const DEFAULT: Self = Self::ZhCn;
    pub const ALL: &'static [Self] = &[Self::ZhCn, Self::En];

    /// BCP-47 code consumed by `<html lang>`, `ep_locale` cookie, and `app_user.locale`.
    pub fn as_code(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::En => "en",
        }
    }

    /// Same as [`as_code`] — kept as a separate accessor so `<html lang=…>` reads intentionally.
    pub fn as_html_lang(self) -> &'static str {
        self.as_code()
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "zh-CN" | "zh-cn" | "zh_CN" => Some(Self::ZhCn),
            "en" | "en-US" | "en-GB" | "en-us" | "en-gb" => Some(Self::En),
            _ => None,
        }
    }

    pub fn parse_or_default(s: &str) -> Self {
        Self::parse(s).unwrap_or(Self::DEFAULT)
    }

    /// Quality factors ignored — first prefix-match wins.
    pub fn parse_accept_language(header: &str) -> Self {
        for tag in header.split(',') {
            let tag = tag.split(';').next().unwrap_or("").trim();
            if tag.is_empty() {
                continue;
            }
            if let Some(loc) = Self::parse(tag) {
                return loc;
            }
            let primary = tag.split('-').next().unwrap_or("");
            match primary.to_ascii_lowercase().as_str() {
                "zh" => return Self::ZhCn,
                "en" => return Self::En,
                _ => {}
            }
        }
        Self::DEFAULT
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::ZhCn => Self::En,
            Self::En => Self::ZhCn,
        }
    }
}

pub const LOCALE_COOKIE: &str = "ep_locale";

/// 1 year, matching `ep_tweaks`.
pub const LOCALE_COOKIE_MAX_AGE_SECS: i64 = 31_536_000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exact() {
        assert_eq!(Locale::parse("zh-CN"), Some(Locale::ZhCn));
        assert_eq!(Locale::parse("en"), Some(Locale::En));
        assert_eq!(Locale::parse("en-US"), Some(Locale::En));
        assert_eq!(Locale::parse("fr"), None);
    }

    #[test]
    fn parse_accept_language_prefix() {
        assert_eq!(
            Locale::parse_accept_language("zh-Hans-CN,zh;q=0.9"),
            Locale::ZhCn
        );
        assert_eq!(Locale::parse_accept_language("en-GB,en;q=0.9"), Locale::En);
        assert_eq!(
            Locale::parse_accept_language("fr-FR,de;q=0.7"),
            Locale::DEFAULT
        );
        assert_eq!(Locale::parse_accept_language(""), Locale::DEFAULT);
    }

    #[test]
    fn toggle_round_trip() {
        assert_eq!(Locale::ZhCn.toggle().toggle(), Locale::ZhCn);
        assert_eq!(Locale::En.toggle().toggle(), Locale::En);
    }
}
