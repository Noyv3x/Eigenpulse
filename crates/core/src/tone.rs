use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::str::FromStr;

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

    pub fn css_var(&self) -> &'static str {
        match self {
            Self::None => "var(--primary)",
            Self::Green => "var(--green)",
            Self::Amber => "var(--amber)",
            Self::Rose => "var(--rose)",
            Self::Blue => "var(--blue)",
            Self::Violet => "var(--violet)",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "green" => Self::Green,
            "amber" => Self::Amber,
            "rose" => Self::Rose,
            "blue" => Self::Blue,
            "violet" => Self::Violet,
            _ => Self::None,
        }
    }
}

impl FromStr for Tone {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_tones() {
        assert_eq!(Tone::parse("green"), Tone::Green);
        assert_eq!(Tone::parse("amber"), Tone::Amber);
        assert_eq!(Tone::parse("rose"), Tone::Rose);
        assert_eq!(Tone::parse("blue"), Tone::Blue);
        assert_eq!(Tone::parse("violet"), Tone::Violet);
    }

    #[test]
    fn parse_unknown_tone_as_none() {
        assert_eq!(Tone::parse(""), Tone::None);
        assert_eq!(Tone::parse("custom"), Tone::None);
    }

    #[test]
    fn css_var_only_returns_known_design_tokens() {
        assert_eq!(Tone::Green.css_var(), "var(--green)");
        assert_eq!(
            Tone::parse("red);color:transparent").css_var(),
            "var(--primary)"
        );
    }

    #[test]
    fn from_str_trait_is_infallible() {
        assert_eq!("green".parse::<Tone>().unwrap(), Tone::Green);
        assert_eq!("custom".parse::<Tone>().unwrap(), Tone::None);
    }
}
