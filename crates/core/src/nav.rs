use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavSection {
    Core,
    Modules,
    System,
}

impl NavSection {
    pub fn label_key(&self) -> &'static str {
        match self {
            Self::Core => "core.nav.section.core",
            Self::Modules => "core.nav.section.modules",
            Self::System => "core.nav.section.system",
        }
    }
    pub fn order(&self) -> u8 {
        match self {
            Self::Core => 0,
            Self::Modules => 1,
            Self::System => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IconKind {
    Dashboard,
    Today,
    Finance,
    Fitness,
    Learning,
    Modules,
    Reports,
    Settings,
    Help,
    Search,
    Bell,
    Plus,
    Arrow,
    ArrowUp,
    ArrowDown,
    Flat,
    Check,
    More,
    Menu,
    Chevron,
    Filter,
    Sun,
    Moon,
    Tag,
    Link,
    Sparkle,
    Upload,
    Flame,
    Book,
    Dumbbell,
    Heart,
    Coin,
    Grid,
    Export,
    Cube,
}

#[derive(Clone, Debug)]
pub struct NavEntry {
    pub code: &'static str,
    pub name: &'static str,
    pub name_cn: &'static str,
    pub icon: IconKind,
    pub section: NavSection,
    pub path: String,
}
