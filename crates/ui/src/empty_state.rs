use crate::Icon;
use ep_core::IconKind;
use leptos::prelude::*;

#[component]
pub fn EmptyState(
    #[prop(into)] title: String,
    #[prop(into, optional)] desc: Option<String>,
    #[prop(into, optional)] code: Option<String>,
    #[prop(default = IconKind::Empty)] icon: IconKind,
    #[prop(default = false)] compact: bool,
    #[prop(optional)] cta: Option<AnyView>,
) -> impl IntoView {
    let class = if compact {
        "empty-state compact"
    } else {
        "empty-state"
    };
    view! {
        <div class=class>
            <div class="empty-glyph"><Icon kind=icon size=20/></div>
            <div class="empty-title">{title}</div>
            {desc.map(|d| view! { <p class="empty-desc">{d}</p> })}
            {code.map(|c| view! { <div class="empty-code mono">{c}</div> })}
            {cta.map(|c| view! { <div class="empty-cta">{c}</div> })}
        </div>
    }
}

#[component]
pub fn SkeletonKpi(#[prop(default = 4)] count: u8) -> impl IntoView {
    let tiles: Vec<_> = (0..count)
        .map(|_| {
            view! {
                <div class="skeleton-kpi">
                    <span class="skeleton-line lbl"></span>
                    <span class="skeleton-line val"></span>
                    <span class="skeleton-line dlt"></span>
                </div>
            }
        })
        .collect();
    view! {
        <div class="skeleton-kpi-grid">{tiles}</div>
    }
}

#[component]
pub fn SkeletonCard(#[prop(default = 3)] rows: u8) -> impl IntoView {
    let rows: Vec<_> = (0..rows)
        .map(|_| view! { <span class="skeleton-line"></span> })
        .collect();
    view! {
        <div class="skeleton-card">
            <span class="skeleton-line lg" style="width:32%"></span>
            <span class="skeleton-line sm"></span>
            {rows}
        </div>
    }
}
