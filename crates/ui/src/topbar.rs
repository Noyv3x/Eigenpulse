use crate::sidebar::nav_items;
use crate::tweaks::{use_tweaks, Theme, TweakState};
use crate::{notifications::use_unread_signal, Icon};
use ep_core::{IconKind, ModuleDescriptor};
use ep_i18n::{t, use_locale};
use leptos::prelude::*;
use leptos_router::hooks::use_location;

#[component]
pub fn Topbar(
    sidebar_collapsed: RwSignal<bool>,
    mobile_nav_open: RwSignal<bool>,
    modules: Vec<&'static ModuleDescriptor>,
) -> impl IntoView {
    let loc = use_location();
    let unread = use_unread_signal();
    let tweaks = use_tweaks();
    // SSR pulls the per-request locale from leptos context; hydrate falls
    // back to the pre-paint `<html lang>` written by theme-init/login shell.
    // Locale changes still happen via full reload, so this value is stable
    // for the lifetime of the hydrated app.
    let locale = use_locale();
    // Move focus into the modal mobile drawer after opening, and return it to
    // the trigger after Escape, scrim dismissal or route navigation closes it.
    // Deferring one animation frame lets the reactive `mobile-open`/`inert`
    // attributes settle before the browser performs the focus operation.
    let mobile_was_open = StoredValue::new(false);
    Effect::new(move |_| {
        let is_open = mobile_nav_open.get();
        let _was_open = mobile_was_open.get_value();
        #[cfg(feature = "hydrate")]
        if !_was_open && is_open {
            focus_after_render("#app-sidebar .nav-item.active", Some("sidebar-first-nav"));
        } else if _was_open && !is_open {
            focus_after_render("#mobile-nav-toggle", None);
        }
        mobile_was_open.set_value(is_open);
    });

    #[cfg(feature = "hydrate")]
    {
        let escape_listener =
            leptos::leptos_dom::helpers::window_event_listener(leptos::ev::keydown, move |event| {
                if event.key() == "Escape" && mobile_nav_open.get_untracked() {
                    mobile_nav_open.set(false);
                }
            });
        on_cleanup(move || escape_listener.remove());
    }

    let nav = StoredValue::new(nav_items(&modules));
    let crumb = move || crumb_for_path(&loc.pathname.get(), &nav.read_value());

    view! {
        <div
            class="topbar"
            inert=move || mobile_nav_open.get()
        >
            <button
                class="icon-btn desktop-sidebar-toggle"
                type="button"
                title=t!(locale, ui.topbar.collapse_title)
                aria-label=t!(locale, ui.topbar.collapse_title)
                aria-expanded=move || (!sidebar_collapsed.get()).to_string()
                aria-controls="app-sidebar"
                on:click=move |_| sidebar_collapsed.update(|v| *v = !*v)
            >
                <Icon kind=IconKind::Menu size=16/>
            </button>
            <button
                id="mobile-nav-toggle"
                class="icon-btn mobile-nav-toggle"
                type="button"
                title=move || if mobile_nav_open.get() {
                    t!(locale, ui.sidebar.close_nav)
                } else {
                    t!(locale, ui.topbar.open_nav)
                }
                aria-label=move || if mobile_nav_open.get() {
                    t!(locale, ui.sidebar.close_nav)
                } else {
                    t!(locale, ui.topbar.open_nav)
                }
                aria-expanded=move || mobile_nav_open.get().to_string()
                aria-controls="app-sidebar"
                on:click=move |_| mobile_nav_open.update(|v| *v = !*v)
            >
                <Icon kind=IconKind::Menu size=16/>
            </button>
            <div class="topbar-title">
                {move || {
                    let name_key = crumb();
                    view! {
                        <>
                            <span class="crumb">"EIGENPULSE"</span>
                            <span class="topbar-sep">"›"</span>
                            <span class="topbar-h1">{t(locale, name_key)}</span>
                        </>
                    }
                }}
            </div>
            <div class="topbar-spacer"></div>
            <button
                class="icon-btn lang-toggle mono"
                type="button"
                title=t!(locale, ui.topbar.lang_toggle.title)
                aria-label=t!(locale, ui.topbar.lang_toggle.title)
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
                type="button"
                title=move || if tweaks.get().theme == Theme::Dark {
                    t!(locale, ui.topbar.light_title)
                } else {
                    t!(locale, ui.topbar.dark_title)
                }
                aria-label=move || if tweaks.get().theme == Theme::Dark {
                    t!(locale, ui.topbar.light_title)
                } else {
                    t!(locale, ui.topbar.dark_title)
                }
                // Announces whether dark mode is the active state.
                aria-pressed=move || (tweaks.get().theme == Theme::Dark).to_string()
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
            <a
                class="icon-btn"
                href="/notifications"
                title=t!(locale, ui.topbar.notif_title)
                aria-label=t!(locale, ui.topbar.notif_title)
            >
                <Icon kind=IconKind::Bell size=16/>
                {move || (unread.get() > 0).then(|| view! { <span class="dot"></span> })}
            </a>
        </div>
    }
}

#[cfg(feature = "hydrate")]
fn focus_after_render(selector: &'static str, fallback_id: Option<&'static str>) {
    leptos::leptos_dom::helpers::request_animation_frame(move || {
        use wasm_bindgen::JsCast as _;

        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let element = document
            .query_selector(selector)
            .ok()
            .flatten()
            .or_else(|| fallback_id.and_then(|id| document.get_element_by_id(id)));
        if let Some(element) =
            element.and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = element.focus();
        }
    });
}

fn crumb_for_path(path: &str, nav: &[crate::sidebar::NavItem]) -> &'static str {
    if path == "/notifications" || path.starts_with("/notifications/") {
        return "ui.topbar.notifications_crumb";
    }
    nav.iter()
        .find(|n| n.matches(path))
        .copied()
        .map(|item| item.name_key)
        .unwrap_or("ui.sidebar.nav.dsh")
}

#[cfg(test)]
mod tests {
    use super::crumb_for_path;
    use crate::sidebar::nav_items;
    use ep_core::{IconKind, ModuleDescriptor};

    static FINANCE: ModuleDescriptor = ModuleDescriptor {
        slug: "finance",
        route: "/finance",
        name_key: "finance.name",
        description_key: "finance.description",
        icon: IconKind::Finance,
        read_scope: "finance:read",
        write_scope: "finance:write",
        read_scope_label_key: "finance.scope.read",
        write_scope_label_key: "finance.scope.write",
    };
    static MODULES: &[&ModuleDescriptor] = &[&FINANCE];

    #[test]
    fn breadcrumb_maps_notifications_without_sidebar_nav_entry() {
        let nav = nav_items(MODULES);
        assert_eq!(
            crumb_for_path("/notifications", &nav),
            "ui.topbar.notifications_crumb"
        );
        assert_eq!(
            crumb_for_path("/notifications/archive", &nav),
            "ui.topbar.notifications_crumb"
        );
    }

    #[test]
    fn breadcrumb_keeps_sidebar_routes_and_falls_back_to_dashboard() {
        let nav = nav_items(MODULES);
        assert_eq!(crumb_for_path("/finance/ledger", &nav), "finance.name");
        assert_eq!(crumb_for_path("/unknown", &nav), "ui.sidebar.nav.dsh");
    }
}
