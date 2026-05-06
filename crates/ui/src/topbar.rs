use crate::sidebar::NAV;
use crate::tweaks::{use_tweaks, Theme, TweakState};
use crate::{notifications::use_unread_signal, Icon};
use ep_core::IconKind;
use ep_i18n::{t, use_locale};
use leptos::prelude::*;
use leptos_router::hooks::use_location;

#[component]
pub fn Topbar() -> impl IntoView {
    let loc = use_location();
    let unread = use_unread_signal();
    let tweaks = use_tweaks();
    // SSR pulls the per-request locale from leptos context (provided by
    // `app/src/main.rs::provide_state`); hydrate falls back to DEFAULT
    // because the SSR-rendered text is already correct in the DOM —
    // closures here only re-fire on theme/unread updates, never on
    // locale (which only changes via full reload).
    let locale = use_locale();

    let crumb = move || {
        let p = loc.pathname.get();
        let item = NAV
            .iter()
            .find(|n| {
                if n.path == "/" {
                    p == "/"
                } else {
                    p == n.path || p.starts_with(&format!("{}/", n.path))
                }
            })
            .copied()
            .unwrap_or(NAV[0]);
        (item.code, item.name_key)
    };

    view! {
        <div class="topbar">
            <button class="icon-btn" title=t!(locale, ui.topbar.collapse_title)>
                <Icon kind=IconKind::Menu size=16/>
            </button>
            <div class="topbar-title">
                {move || {
                    let (code, name_key) = crumb();
                    view! {
                        <>
                            <span class="crumb">"EIGENPULSE / " {code}</span>
                            <span class="topbar-sep">"›"</span>
                            <span class="topbar-h1">{t(locale, name_key)}</span>
                        </>
                    }
                }}
            </div>
            <div class="topbar-spacer"></div>
            <div class="search">
                <Icon kind=IconKind::Search size=14/>
                <span>{t!(locale, ui.topbar.search_placeholder)}</span>
                <kbd>"⌘K"</kbd>
            </div>
            <button
                class="icon-btn lang-toggle mono"
                title=t!(locale, ui.topbar.lang_toggle.title)
                on:click=move |_| {
                    // Reload-mode toggle. Browser-only path; SSR ignores
                    // `on:click` entirely so no cfg branch needed.
                    #[cfg(feature = "hydrate")]
                    ep_i18n::switch_locale_via_reload(locale.toggle());
                }
            >
                {t!(locale, ui.topbar.lang_toggle.label)}
            </button>
            <button
                class="icon-btn"
                title=move || if tweaks.get().theme == Theme::Dark {
                    t!(locale, ui.topbar.light_title)
                } else {
                    t!(locale, ui.topbar.dark_title)
                }
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
            <a class="icon-btn" href="/notifications" title=t!(locale, ui.topbar.notif_title)>
                <Icon kind=IconKind::Bell size=16/>
                {move || (unread.get() > 0).then(|| view! { <span class="dot"></span> })}
            </a>
            <button class="icon-btn" title=t!(locale, ui.topbar.help_title)>
                <Icon kind=IconKind::Help size=16/>
            </button>
        </div>
    }
}
