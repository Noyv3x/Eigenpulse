//! Shared Leptos components for Eigenpulse — ports of the React design prototype.

pub mod icon;
pub mod tweaks;
pub mod kpi;
pub mod card;
pub mod tag;
pub mod tabs;
pub mod page_head;
pub mod section_label;
pub mod chart_bars;
pub mod donut;
pub mod ring;
pub mod heatmap;
pub mod sidebar;
pub mod topbar;
pub mod notifications;
pub mod stat;

pub use icon::Icon;
pub use kpi::Kpi;
pub use card::Card;
pub use tag::{Tag, Tone};
pub use tabs::{Tabs, TabSpec};
pub use page_head::PageHead;
pub use section_label::SectionLabel;
pub use chart_bars::ChartBars;
pub use donut::Donut;
pub use ring::Ring;
pub use heatmap::Heatmap;
pub use sidebar::Sidebar;
pub use topbar::Topbar;
pub use tweaks::{TweakState, Theme, Density, provide_tweak_state, use_tweaks};
pub use notifications::{NotificationsBellPopover, provide_unread_signal, use_unread_signal};
pub use stat::StatRow;
