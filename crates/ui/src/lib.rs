//! Shared Leptos components for Eigenpulse.

mod card;
mod chart;
mod dialog;
mod empty_state;
mod error_slot;
mod field;
mod icon;
mod kpi;
mod load_error;
mod notifications;
mod page_head;
mod row_action;
mod sidebar;
mod stat;
mod tabs;
mod tag;
mod topbar;
mod tweaks;

pub use card::Card;
pub use chart::{
    AxisChart, AxisSeries, AxisSeriesKind, CalendarHeatmapChart, Chart, ChartDatum, ChartHeight,
    ChartSpec, ChartTone, ChartValue, DonutChart, GaugeChart, HorizontalBarChart, SparklineChart,
};
pub use dialog::Dialog;
pub use empty_state::{EmptyState, SkeletonCard};
pub use error_slot::ErrorSlot;
pub use field::Field;
pub use icon::Icon;
pub use kpi::{Direction, Kpi};
pub use load_error::LoadError;
pub use notifications::{provide_unread_signal, use_unread_signal};
pub use page_head::PageHead;
pub use row_action::RowDeleteAction;
pub use sidebar::Sidebar;
pub use stat::StatRow;
pub use tabs::{TabSpec, Tabs};
pub use tag::{Tag, Tone};
pub use topbar::Topbar;
pub use tweaks::{provide_tweak_state, use_tweaks, Density, Theme, TweakState};
