use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Crit,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Crit => "crit",
        }
    }
    pub fn try_parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "info" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "crit" | "critical" | "error" => Some(Self::Crit),
            _ => None,
        }
    }
    pub fn parse(s: &str) -> Self {
        Self::try_parse(s).unwrap_or(Self::Info)
    }
    pub fn passes(&self, min: Self) -> bool {
        *self >= min
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_parse_known_severities() {
        assert_eq!(Severity::try_parse("info"), Some(Severity::Info));
        assert_eq!(Severity::try_parse(" WARN "), Some(Severity::Warn));
        assert_eq!(Severity::try_parse("critical"), Some(Severity::Crit));
    }

    #[test]
    fn try_parse_rejects_unknown_severity() {
        assert_eq!(Severity::try_parse("urgent"), None);
        assert_eq!(Severity::try_parse(""), None);
    }

    #[test]
    fn parse_keeps_legacy_info_fallback() {
        assert_eq!(Severity::parse("urgent"), Severity::Info);
    }
}
