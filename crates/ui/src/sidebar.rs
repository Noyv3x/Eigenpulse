use crate::Icon;
use ep_core::{IconKind, NavSection};
use ep_i18n::{t, use_locale};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

#[derive(Clone, Copy)]
pub struct NavItem {
    pub code: &'static str,
    pub name_key: &'static str,
    pub icon: IconKind,
    pub section: NavSection,
    pub path: &'static str,
}

pub const NAV: &[NavItem] = &[
    NavItem {
        code: "DSH",
        name_key: "ui.sidebar.nav.dsh",
        icon: IconKind::Dashboard,
        section: NavSection::Core,
        path: "/",
    },
    NavItem {
        code: "TDY",
        name_key: "ui.sidebar.nav.tdy",
        icon: IconKind::Today,
        section: NavSection::Core,
        path: "/today",
    },
    NavItem {
        code: "FIN",
        name_key: "ui.sidebar.nav.fin",
        icon: IconKind::Finance,
        section: NavSection::Modules,
        path: "/finance",
    },
    NavItem {
        code: "FIT",
        name_key: "ui.sidebar.nav.fit",
        icon: IconKind::Fitness,
        section: NavSection::Modules,
        path: "/fitness",
    },
    NavItem {
        code: "LRN",
        name_key: "ui.sidebar.nav.lrn",
        icon: IconKind::Learning,
        section: NavSection::Modules,
        path: "/learning",
    },
    NavItem {
        code: "MOD",
        name_key: "ui.sidebar.nav.mod",
        icon: IconKind::Modules,
        section: NavSection::System,
        path: "/modules",
    },
    NavItem {
        code: "RPT",
        name_key: "ui.sidebar.nav.rpt",
        icon: IconKind::Reports,
        section: NavSection::System,
        path: "/reports",
    },
    NavItem {
        code: "CFG",
        name_key: "ui.sidebar.nav.cfg",
        icon: IconKind::Settings,
        section: NavSection::System,
        path: "/settings",
    },
];

#[component]
pub fn Sidebar() -> impl IntoView {
    let loc = use_location();
    let pathname = move || loc.pathname.get();
    let locale = use_locale();
    view! {
        <aside class="sidebar">
            <div class="brand">
                <div class="brand-mark mono">"E"</div>
                <div class="brand-text">
                    <div class="brand-name">"Eigenpulse"</div>
                    <div class="brand-sub mono">"Personal ERP · v0.1"</div>
                </div>
            </div>
            <div class="sidebar-scroll">
                {[NavSection::Core, NavSection::Modules, NavSection::System].into_iter().map(|sec| {
                    let items = NAV.iter().filter(|n| n.section == sec).collect::<Vec<_>>();
                    let title = t(locale, sec.label_key());
                    view! {
                        <div>
                            <div class="nav-section"><span class="nav-section-text">{title}</span></div>
                            <ul class="nav-list">
                                {items.into_iter().map(|n| {
                                    let path = n.path;
                                    let active = move || {
                                        let p = pathname();
                                        if path == "/" { p == "/" } else { p == path || p.starts_with(&format!("{path}/")) }
                                    };
                                    let class = move || if active() { "nav-item active" } else { "nav-item" };
                                    view! {
                                        <li>
                                            <A href=path attr:class=class>
                                                <Icon kind=n.icon size=16/>
                                                <span class="nav-label">
                                                    {t(locale, n.name_key)}
                                                </span>
                                                <span class="code mono">{n.code}</span>
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
                <div class="avatar">"L"</div>
                <div class="avatar-meta">
                    <div style="font-weight:500">"Leo Chen"</div>
                    <small>"OWNER · UID-0001"</small>
                </div>
                <a class="foot-btn" href="/logout" title=t!(locale, ui.sidebar.logout_title)>
                    <Icon kind=IconKind::Menu size=14/>
                </a>
            </div>
        </aside>
    }
}
