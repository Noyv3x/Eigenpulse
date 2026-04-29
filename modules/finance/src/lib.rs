pub mod model;
pub mod server_fns;
pub mod suggestions;
pub mod view;

#[cfg(feature = "ssr")]
pub mod widgets;
#[cfg(feature = "ssr")]
pub mod api;

pub use model::*;
pub use server_fns::*;
pub use view::FinanceView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{IconKind, Module, ModuleLink, NavSection, AppState};

    pub struct FinanceModule;

    pub static MODULE: &dyn Module = &FinanceModule;

    impl Module for FinanceModule {
        fn code(&self) -> &'static str { "FIN" }
        fn name(&self) -> &'static str { "Finance" }
        fn name_cn(&self) -> &'static str { "财务管理" }
        fn nav_section(&self) -> NavSection { NavSection::Modules }
        fn nav_icon(&self) -> IconKind { IconKind::Finance }
        fn glyph(&self) -> &'static str { "fin" }
        fn description(&self) -> &'static str { "账户、预算、收支、投资组合" }
        fn version(&self) -> &'static str { "0.1.0" }

        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[("001_finance", include_str!("../migrations/001_finance.sql"))]
        }

        fn routes(&self, _state: AppState) -> axum::Router<AppState> {
            // Server-fn URLs are mounted globally by leptos_axum; module-specific
            // non-leptos routes (admin endpoints) would go here.
            axum::Router::new()
        }

        fn open_api(&self, state: AppState) -> axum::Router<AppState> {
            super::api::open_api(state)
        }

        fn open_api_scopes(&self) -> &'static [&'static str] {
            &["fin:read", "fin:write"]
        }

        fn dashboard_widgets(&self) -> &'static [ep_core::DashboardWidget] {
            super::widgets::WIDGETS
        }

        fn links(&self) -> &'static [ModuleLink] {
            &[
                ModuleLink { source: "FIN", target: "DSH", kind: "totals-feed" },
                ModuleLink { source: "FIN", target: "FIT", kind: "doc-ref" },
                ModuleLink { source: "FIN", target: "LRN", kind: "doc-ref" },
            ]
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{FinanceModule, MODULE};
