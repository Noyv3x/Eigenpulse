use ep_core::Tone;
use serde::{Deserialize, Serialize};

/// Workout intensity label. Wire form keeps the column as TEXT (seed data
/// uses the single-letter codes), so this enum is purely Rust-side: one
/// source of truth for validation, storage, and UI tone mapping. Mirrors
/// the `model::Tag` pattern in finance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Strain {
    L,
    M,
    H,
}

impl Strain {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::L => "L",
            Self::M => "M",
            Self::H => "H",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "L" => Some(Self::L),
            "M" => Some(Self::M),
            "H" => Some(Self::H),
            _ => None,
        }
    }
    pub const fn tone(&self) -> Tone {
        match self {
            Self::L => Tone::Green,
            Self::M => Tone::Amber,
            Self::H => Tone::Rose,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workout {
    pub doc_id: String,
    pub occurred_at: i64,
    pub kind: String,
    pub program: Option<String>,
    pub duration_m: i64,
    pub load_text: Option<String>,
    pub strain: Option<String>,
    pub rpe: Option<i64>,
    pub notes: Option<String>,
}
