use crate::sidebar::NAV;
use crate::tweaks::{use_tweaks, Theme, TweakState};
use crate::{notifications::use_unread_signal, Icon};
use ep_core::IconKind;
use ep_i18n::{t, use_locale};
use leptos::prelude::*;
use leptos_router::hooks::use_location;

#[component]
pub fn Topbar(sidebar_collapsed: RwSignal<bool>, mobile_nav_open: RwSignal<bool>) -> impl IntoView {
    let loc = use_location();
    let unread = use_unread_signal();
    let tweaks = use_tweaks();
    // SSR pulls the per-request locale from leptos context; hydrate falls
    // back to the pre-paint `<html lang>` written by theme-init/login shell.
    // Locale changes still happen via full reload, so this value is stable
    // for the lifetime of the hydrated app.
    let locale = use_locale();

    let crumb = move || crumb_for_path(&loc.pathname.get());

    view! {
        <div class="topbar">
            <button
                class="icon-btn"
                title=t!(locale, ui.topbar.collapse_title)
                on:click=move |_| {
                    sidebar_collapsed.update(|v| *v = !*v);
                    mobile_nav_open.update(|v| *v = !*v);
                }
            >
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
        </div>
    }
}

fn crumb_for_path(path: &str) -> (&'static str, &'static str) {
    if path == "/notifications" || path.starts_with("/notifications/") {
        return ("NOT", "ui.topbar.notifications_crumb");
    }
    let item = NAV
        .iter()
        .find(|n| {
            if n.path == "/" {
                path == "/"
            } else {
                path == n.path || path.starts_with(&format!("{}/", n.path))
            }
        })
        .copied()
        .unwrap_or(NAV[0]);
    (item.code, item.name_key)
}

#[cfg(test)]
mod tests {
    use super::crumb_for_path;

    #[test]
    fn breadcrumb_maps_notifications_without_sidebar_nav_entry() {
        assert_eq!(
            crumb_for_path("/notifications"),
            ("NOT", "ui.topbar.notifications_crumb")
        );
        assert_eq!(
            crumb_for_path("/notifications/archive"),
            ("NOT", "ui.topbar.notifications_crumb")
        );
    }

    #[test]
    fn breadcrumb_keeps_sidebar_routes_and_falls_back_to_dashboard() {
        assert_eq!(
            crumb_for_path("/finance/ledger"),
            ("FIN", "ui.sidebar.nav.fin")
        );
        assert_eq!(crumb_for_path("/unknown"), ("DSH", "ui.sidebar.nav.dsh"));
    }
}
