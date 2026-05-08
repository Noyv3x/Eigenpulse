pub mod model;
pub mod server_fns;
pub mod suggestions;
pub mod view;

#[cfg(feature = "ssr")]
pub mod api;

pub use model::*;
pub use server_fns::*;
pub use view::FinanceView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{AppState, IconKind, Module, ModuleLink, NavSection};

    pub struct FinanceModule;

    pub static MODULE: &dyn Module = &FinanceModule;

    impl Module for FinanceModule {
        fn code(&self) -> &'static str {
            "FIN"
        }
        fn name(&self) -> &'static str {
            "Finance"
        }
        fn name_cn(&self) -> &'static str {
            "\u{8d22}\u{52a1}\u{7ba1}\u{7406}"
        }
        fn nav_section(&self) -> NavSection {
            NavSection::Modules
        }
        fn nav_icon(&self) -> IconKind {
            IconKind::Finance
        }
        fn glyph(&self) -> &'static str {
            "fin"
        }
        fn description(&self) -> &'static str {
            "\u{8d26}\u{6237}\u{3001}\u{9884}\u{7b97}\u{3001}\u{6536}\u{652f}\u{3001}\u{6295}\u{8d44}\u{7ec4}\u{5408}"
        }
        fn version(&self) -> &'static str {
            "0.1.0"
        }

        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[
                ("001_finance", include_str!("../migrations/001_finance.sql")),
                (
                    "002_finance_crud",
                    include_str!("../migrations/002_finance_crud.sql"),
                ),
                (
                    "003_finance_remove_archive_usage",
                    include_str!("../migrations/003_finance_remove_archive_usage.sql"),
                ),
            ]
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

        // No `dashboard_widgets` override: the global Dashboard already
        // renders FIN-K01 / FIN-K02 directly via its `load_dashboard` server
        // fn (see `app/src/views/dashboard.rs`). A second widget pipeline
        // here would either be unused or double-render.
        fn links(&self) -> &'static [ModuleLink] {
            &[
                ModuleLink {
                    source: "FIN",
                    target: "DSH",
                    kind: "totals-feed",
                },
                // Transfer pairs are intra-FIN edges in module_link
                // (kind='tfr-pair'); document the kind here for module-graph
                // viz. Runtime is unaffected.
                ModuleLink {
                    source: "FIN",
                    target: "FIN",
                    kind: "tfr-pair",
                },
            ]
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{FinanceModule, MODULE};
