pub mod view;

pub use view::MarketView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{AppState, IconKind, Module, ModuleLink, NavSection};

    pub struct MarketplaceModule;
    pub static MODULE: &dyn Module = &MarketplaceModule;

    impl Module for MarketplaceModule {
        fn code(&self) -> &'static str {
            "MOD"
        }
        fn name(&self) -> &'static str {
            "Modules"
        }
        fn name_cn(&self) -> &'static str {
            "\u{6a21}\u{5757}\u{5e02}\u{573a}"
        }
        fn nav_section(&self) -> NavSection {
            NavSection::System
        }
        fn nav_icon(&self) -> IconKind {
            IconKind::Modules
        }
        fn glyph(&self) -> &'static str {
            "mod"
        }
        fn description(&self) -> &'static str {
            "\u{5df2}\u{6ce8}\u{518c}\u{6a21}\u{5757}\u{7684}\u{6982}\u{89c8}\u{4e0e}\u{6269}\u{5c55}\u{5165}\u{53e3}"
        }
        fn version(&self) -> &'static str {
            "0.1.0"
        }
        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[]
        }
        fn routes(&self, _state: AppState) -> axum::Router<AppState> {
            axum::Router::new()
        }
        fn links(&self) -> &'static [ModuleLink] {
            &[]
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{MarketplaceModule, MODULE};
