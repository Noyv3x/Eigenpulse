use crate::Icon;
use ep_core::{IconKind, ModuleDescriptor};
use ep_i18n::{t, use_locale};
use leptos::prelude::*;
use leptos_router::components::A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavSection {
    Core,
    Modules,
    System,
}

impl NavSection {
    const fn label_key(self) -> &'static str {
        match self {
            Self::Core => "core.nav.section.core",
            Self::Modules => "core.nav.section.modules",
            Self::System => "core.nav.section.system",
        }
    }
}
use leptos_router::hooks::use_location;

#[derive(Clone, Copy)]
pub(crate) struct NavItem {
    pub(crate) name_key: &'static str,
    pub(crate) icon: IconKind,
    section: NavSection,
    pub(crate) path: &'static str,
}

impl NavItem {
    pub(crate) fn matches(&self, path: &str) -> bool {
        if self.path == "/" {
            path == "/"
        } else {
            path == self.path || path.starts_with(&format!("{}/", self.path))
        }
    }
}

const HOME: NavItem = NavItem {
    name_key: "ui.sidebar.nav.dsh",
    icon: IconKind::Dashboard,
    section: NavSection::Core,
    path: "/",
};

const SETTINGS: NavItem = NavItem {
    name_key: "ui.sidebar.nav.cfg",
    icon: IconKind::Settings,
    section: NavSection::System,
    path: "/settings",
};

pub(crate) fn nav_items(modules: &[&'static ModuleDescriptor]) -> Vec<NavItem> {
    let mut items = Vec::with_capacity(modules.len() + 2);
    items.push(HOME);
    items.extend(modules.iter().map(|module| NavItem {
        name_key: module.name_key,
        icon: module.icon,
        section: NavSection::Modules,
        path: module.route,
    }));
    items.push(SETTINGS);
    items
}

#[component]
pub fn Sidebar(
    user_name: RwSignal<String>,
    user_meta: RwSignal<String>,
    avatar_text: RwSignal<String>,
    mobile_nav_open: RwSignal<bool>,
    modules: Vec<&'static ModuleDescriptor>,
) -> impl IntoView {
    let loc = use_location();
    let pathname_signal = loc.pathname;
    let pathname = move || pathname_signal.get();
    let locale = use_locale();
    let nav = StoredValue::new(nav_items(&modules));

    Effect::new(move |_| {
        pathname_signal.track();
        mobile_nav_open.set(false);
    });

    view! {
        <aside
            class="sidebar"
            id="app-sidebar"
            aria-label=t(locale, "ui.sidebar.navigation_label")
            on:keydown=move |event| trap_mobile_nav_focus(&event, mobile_nav_open.get_untracked())
        >
            <div class="brand">
                <div class="brand-mark mono">"E"</div>
                <div class="brand-text">
                    <div class="brand-name">"Eigenpulse"</div>
                    <div class="brand-sub">"Self-hosted Personal Hub"</div>
                </div>
            </div>
            <div class="sidebar-scroll">
                {[NavSection::Core, NavSection::Modules, NavSection::System].into_iter().map(|sec| {
                    let items = nav.read_value().iter().copied().filter(|item| item.section == sec).collect::<Vec<_>>();
                    let title = t(locale, sec.label_key());
                    view! {
                        <div>
                            <div class="nav-section"><span class="nav-section-text">{title}</span></div>
                            <ul class="nav-list">
                                {items.into_iter().map(|item| {
                                    let path = item.path;
                                    let active = move || item.matches(&pathname());
                                    let class = move || if active() { "nav-item active" } else { "nav-item" };
                                    let first_id = (path == "/").then_some("sidebar-first-nav");
                                    view! {
                                        <li>
                                            <A href=path attr:id=first_id attr:class=class on:click=move |_| mobile_nav_open.set(false)>
                                                <Icon kind=item.icon size=16/>
                                                <span class="nav-label">{t(locale, item.name_key)}</span>
                                            </A>
                                        </li>
                                    }
                                }).collect_view()}
                            </ul>
                        </div>
                    }
                }).collect_view()}
            </div>
            <div class="sidebar-foot">
                <div class="avatar">{move || avatar_text.get()}</div>
                <div class="avatar-meta">
                    <div style="font-weight:500">{move || user_name.get()}</div>
                    <small>{move || user_meta.get()}</small>
                </div>
                <form method="post" action="/logout">
                    <button
                        id="sidebar-logout"
                        class="foot-btn"
                        type="submit"
                        title=t!(locale, ui.sidebar.logout_title)
                        aria-label=t!(locale, ui.sidebar.logout_title)
                    >
                        <Icon kind=IconKind::Logout size=14/>
                    </button>
                </form>
            </div>
        </aside>
    }
}

fn trap_mobile_nav_focus(event: &leptos::ev::KeyboardEvent, is_open: bool) {
    #[cfg(feature = "hydrate")]
    {
        if !is_open || event.key() != "Tab" {
            return;
        }
        use wasm_bindgen::JsCast as _;
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let active_id = document.active_element().map(|element| element.id());
        let target = match (event.shift_key(), active_id.as_deref()) {
            (true, Some("sidebar-first-nav")) => Some("sidebar-logout"),
            (false, Some("sidebar-logout")) => Some("sidebar-first-nav"),
            _ => None,
        };
        if let Some(target) = target {
            event.prevent_default();
            if let Some(element) = document
                .get_element_by_id(target)
                .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
            {
                let _ = element.focus();
            }
        }
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = (event, is_open);
}

#[cfg(test)]
mod tests {
    use super::{nav_items, NavSection};
    use ep_core::{IconKind, ModuleDescriptor};
    use std::collections::HashSet;

    static TEST_MODULE: ModuleDescriptor = ModuleDescriptor {
        slug: "test",
        route: "/test",
        name_key: "test.name",
        description_key: "test.description",
        icon: IconKind::Fitness,
        read_scope: "test:read",
        write_scope: "test:write",
        read_scope_label_key: "test.scope.read",
        write_scope_label_key: "test.scope.write",
    };
    static MODULES: &[&ModuleDescriptor] = &[&TEST_MODULE];

    #[test]
    fn navigation_is_catalog_driven_and_unique() {
        let nav = nav_items(MODULES);
        let mut paths = HashSet::new();
        let mut keys = HashSet::new();
        for item in &nav {
            assert!(paths.insert(item.path));
            assert!(keys.insert(item.name_key));
            assert!(ep_core::safe_in_app_path(item.path).is_some());
        }
        let module = nav.iter().find(|item| item.path == "/test").unwrap();
        assert_eq!(module.section, NavSection::Modules);
    }
}
