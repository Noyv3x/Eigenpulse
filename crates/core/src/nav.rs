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
    Bell,
    Plus,
    Arrow,
    ArrowUp,
    ArrowDown,
    Flat,
    Check,
    Menu,
    Sun,
    Moon,
    Link,
    Sparkle,
    Upload,
    Flame,
    Coin,
    Export,
}
