use crate::sidebar::NAV;
use crate::tweaks::{Theme, TweakState, use_tweaks};
use crate::{notifications::use_unread_signal, Icon};
use ep_core::IconKind;
use leptos::prelude::*;
use leptos_router::hooks::use_location;

#[component]
pub fn Topbar() -> impl IntoView {
    let loc = use_location();
    let unread = use_unread_signal();
    let tweaks = use_tweaks();

    let crumb = move || {
        let p = loc.pathname.get();
        let item = NAV.iter().find(|n| {
            if n.path == "/" { p == "/" } else { p == n.path || p.starts_with(&format!("{}/", n.path)) }
        }).copied().unwrap_or(NAV[0]);
        (item.code, item.name, item.name_cn)
    };

    view! {
        <div class="topbar">
            <button class="icon-btn" title="折叠侧栏">
                <Icon kind=IconKind::Menu size=16/>
            </button>
            <div class="topbar-title">
                {move || {
                    let (code, name, cn) = crumb();
                    view! {
                        <>
                            <span class="crumb">"EIGENPULSE / " {code}</span>
                            <span class="topbar-sep">"›"</span>
                            <span class="topbar-h1">{name} <span class="dim" style="font-weight:400">"· "{cn}</span></span>
                        </>
                    }
                }}
            </div>
            <div class="topbar-spacer"></div>
            <div class="search">
                <Icon kind=IconKind::Search size=14/>
                <span>"搜索模块、单号或记录…"</span>
                <kbd>"⌘K"</kbd>
            </div>
            <button
                class="icon-btn"
                title=move || if tweaks.get().theme == Theme::Dark { "切到浅色" } else { "切到深色" }
                on:click=move |_| tweaks.update(|v: &mut TweakState| {
                    v.theme = if v.theme == Theme::Dark { Theme::Light } else { Theme::Dark };
                })
            >
                {move || if tweaks.get().theme == Theme::Dark {
                    view! { <Icon kind=IconKind::Sun size=16/> }
                } else {
                    view! { <Icon kind=IconKind::Moon size=16/> }
                }}
            </button>
            <a class="icon-btn" href="/notifications" title="通知">
                <Icon kind=IconKind::Bell size=16/>
                {move || (unread.get() > 0).then(|| view! { <span class="dot"></span> })}
            </a>
            <button class="icon-btn" title="帮助">
                <Icon kind=IconKind::Help size=16/>
            </button>
        </div>
    }
}
