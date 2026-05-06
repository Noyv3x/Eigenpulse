//! Shared Leptos components for Eigenpulse — ports of the React design prototype.

pub mod card;
pub mod chart_bars;
pub mod donut;
pub mod heatmap;
pub mod icon;
pub mod kpi;
pub mod notifications;
pub mod page_head;
pub mod ring;
pub mod row_action;
pub mod section_label;
pub mod sidebar;
pub mod stat;
pub mod tabs;
pub mod tag;
pub mod topbar;
pub mod tweaks;

pub use card::Card;
pub use chart_bars::ChartBars;
pub use donut::Donut;
pub use heatmap::Heatmap;
pub use icon::Icon;
pub use kpi::Kpi;
pub use notifications::{provide_unread_signal, use_unread_signal, NotificationsBellPopover};
pub use page_head::PageHead;
pub use ring::Ring;
pub use row_action::RowDeleteAction;
pub use section_label::SectionLabel;
pub use sidebar::Sidebar;
pub use stat::StatRow;
pub use tabs::{TabSpec, Tabs};
pub use tag::{Tag, Tone};
pub use topbar::Topbar;
pub use tweaks::{provide_tweak_state, use_tweaks, Density, Theme, TweakState};
