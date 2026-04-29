use ep_core::Tone;
use serde::{Deserialize, Serialize};

/// Workout intensity label. Wire form keeps the column as TEXT (seed data
/// uses the single-letter codes), so this enum is purely Rust-side: one
/// source of truth for validation, color, label, and the load-weight
/// factor that compute_summary multiplies by `duration_m`. Mirrors the
/// `model::Tag` pattern in finance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Strain {
    L,
    M,
    H,
}

impl Strain {
    pub const fn as_str(&self) -> &'static str {
        match self { Self::L => "L", Self::M => "M", Self::H => "H" }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "L" => Some(Self::L),
            "M" => Some(Self::M),
            "H" => Some(Self::H),
            _ => None,
        }
    }
    pub const fn label_cn(&self) -> &'static str {
        match self { Self::L => "轻", Self::M => "中", Self::H => "高" }
    }
    pub const fn tone(&self) -> Tone {
        match self { Self::L => Tone::Green, Self::M => Tone::Amber, Self::H => Tone::Rose }
    }
    /// Per-strain weight applied to `duration_m` for the load metric. Kept
    /// in sync with the SQL `CASE strain WHEN … THEN …` in
    /// `compute_summary` so re-tuning happens in one place.
    pub const fn load_factor(&self) -> f64 {
        match self { Self::L => 0.6, Self::M => 1.0, Self::H => 1.4 }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkout {
    pub kind: String,
    pub program: Option<String>,
    pub duration_m: i64,
    pub load_text: Option<String>,
    pub strain: Option<String>,
    pub notes: Option<String>,
}
