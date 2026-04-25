use crate::Icon;
use ep_core::{IconKind, NavSection};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

#[derive(Clone, Copy)]
pub struct NavItem {
    pub code: &'static str,
    pub name: &'static str,
    pub name_cn: &'static str,
    pub icon: IconKind,
    pub section: NavSection,
    pub path: &'static str,
}

pub const NAV: &[NavItem] = &[
    NavItem { code: "DSH", name: "Dashboard", name_cn: "全局仪表", icon: IconKind::Dashboard, section: NavSection::Core,    path: "/" },
    NavItem { code: "TDY", name: "Today",     name_cn: "今日聚焦", icon: IconKind::Today,     section: NavSection::Core,    path: "/today" },
    NavItem { code: "FIN", name: "Finance",   name_cn: "财务管理", icon: IconKind::Finance,   section: NavSection::Modules, path: "/finance" },
    NavItem { code: "FIT", name: "Fitness",   name_cn: "健身管理", icon: IconKind::Fitness,   section: NavSection::Modules, path: "/fitness" },
    NavItem { code: "LRN", name: "Learning",  name_cn: "学习管理", icon: IconKind::Learning,  section: NavSection::Modules, path: "/learning" },
    NavItem { code: "MOD", name: "Modules",   name_cn: "模块市场", icon: IconKind::Modules,   section: NavSection::System,  path: "/modules" },
    NavItem { code: "RPT", name: "Reports",   name_cn: "报表中心", icon: IconKind::Reports,   section: NavSection::System,  path: "/reports" },
    NavItem { code: "CFG", name: "Settings",  name_cn: "系统设置", icon: IconKind::Settings,  section: NavSection::System,  path: "/settings" },
];

#[component]
pub fn Sidebar() -> impl IntoView {
    let loc = use_location();
    let pathname = move || loc.pathname.get();
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
                    let title = sec.label();
                    view! {
                        <div>
                            <div class="nav-section"><span class="nav-section-text">{title}</span></div>
                            <ul class="nav-list">
                                {items.into_iter().map(|n| {
                                    let path = n.path;
                                    let pathname = pathname.clone();
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
                                                    {n.name} <span class="dim" style="font-size:11px;margin-left:2px">"· "{n.name_cn}</span>
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
                <a class="foot-btn" href="/logout" title="退出登录">
                    <Icon kind=IconKind::Menu size=14/>
                </a>
            </div>
        </aside>
    }
}
