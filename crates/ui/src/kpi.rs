use crate::Icon;
use ep_core::IconKind;
use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Direction {
    Up,
    Down,
    #[default]
    Flat,
}
impl Direction {
    pub fn class(&self) -> &'static str {
        match self { Self::Up => "up", Self::Down => "down", Self::Flat => "flat" }
    }
    pub fn icon(&self) -> IconKind {
        match self { Self::Up => IconKind::ArrowUp, Self::Down => IconKind::ArrowDown, Self::Flat => IconKind::Flat }
    }
}

#[component]
pub fn Kpi(
    #[prop(into)] code: String,
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    #[prop(into, optional)] unit: Option<String>,
    #[prop(into, optional)] delta: Option<String>,
    #[prop(default = Direction::Flat)] dir: Direction,
) -> impl IntoView {
    view! {
        <div class="kpi">
            <div class="kpi-label">
                <span>{label}</span>
                <span class="spacer"></span>
                <span class="code">{code}</span>
            </div>
            <div class="kpi-value mono">
                {value}
                {unit.map(|u| view! { <small>{u}</small> })}
            </div>
            <div class=move || format!("kpi-delta {}", dir.class())>
                <Icon kind=dir.icon() size=12/>
                <span>{delta.unwrap_or_default()}</span>
            </div>
        </div>
    }
}
