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

    /// Pick the supported language with the highest `q` weight.
    pub fn parse_accept_language(header: &str) -> Self {
        let mut best: Option<(Self, f32)> = None;

        for raw in header.split(',') {
            let mut parts = raw.split(';');
            let tag = parts.next().unwrap_or("").trim();
            if tag.is_empty() {
                continue;
            }

            let mut q = 1.0_f32;
            let mut invalid_q = false;
            for param in parts {
                let param = param.trim();
                if let Some(value) = param.strip_prefix("q=") {
                    match value.trim().parse::<f32>() {
                        Ok(parsed) if (0.0..=1.0).contains(&parsed) => q = parsed,
                        _ => invalid_q = true,
                    }
                }
            }
            if invalid_q || q <= 0.0 {
                continue;
            }

            let loc = Self::parse(tag).or_else(|| {
                let primary = tag.split('-').next().unwrap_or("");
                match primary.to_ascii_lowercase().as_str() {
                    "zh" => Some(Self::ZhCn),
                    "en" => Some(Self::En),
                    _ => None,
                }
            });
            if let Some(loc) = loc {
                if best.is_none_or(|(_, best_q)| q > best_q) {
                    best = Some((loc, q));
                }
            }
        }

        best.map(|(loc, _)| loc).unwrap_or(Self::DEFAULT)
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
    fn parse_accept_language_respects_quality_values() {
        assert_eq!(
            Locale::parse_accept_language("en;q=0.1,zh-CN;q=0.9"),
            Locale::ZhCn
        );
        assert_eq!(
            Locale::parse_accept_language("en;q=0,zh-CN;q=0.5"),
            Locale::ZhCn
        );
        assert_eq!(Locale::parse_accept_language("en;q=0"), Locale::DEFAULT);
        assert_eq!(
            Locale::parse_accept_language("en;q=0.8,zh-CN;q=0.8"),
            Locale::En
        );
        assert_eq!(
            Locale::parse_accept_language("zh-CN;q=bogus,en;q=0.4"),
            Locale::En
        );
    }

    #[test]
    fn toggle_round_trip() {
        assert_eq!(Locale::ZhCn.toggle().toggle(), Locale::ZhCn);
        assert_eq!(Locale::En.toggle().toggle(), Locale::En);
    }
}
