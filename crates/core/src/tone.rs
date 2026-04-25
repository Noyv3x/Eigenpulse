use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Tone {
    #[default]
    None,
    Green,
    Amber,
    Rose,
    Blue,
    Violet,
}

impl Tone {
    pub fn class(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Green => "green",
            Self::Amber => "amber",
            Self::Rose => "rose",
            Self::Blue => "blue",
            Self::Violet => "violet",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "green" => Self::Green,
            "amber" => Self::Amber,
            "rose"  => Self::Rose,
            "blue"  => Self::Blue,
            "violet" => Self::Violet,
            _ => Self::None,
        }
    }
}
