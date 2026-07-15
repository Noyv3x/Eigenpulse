use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IconKind {
    Dashboard,
    Finance,
    Fitness,
    Journal,
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
    Upload,
    Export,
    Logout,
    Close,
    Empty,
}
