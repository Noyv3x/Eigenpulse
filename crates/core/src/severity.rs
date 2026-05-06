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
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "warn" | "warning" => Self::Warn,
            "crit" | "critical" | "error" => Self::Crit,
            _ => Self::Info,
        }
    }
    pub fn passes(&self, min: Self) -> bool {
        *self >= min
    }
}
