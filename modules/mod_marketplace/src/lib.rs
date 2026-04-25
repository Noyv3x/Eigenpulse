pub mod view;

pub use view::MarketView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{IconKind, Module, ModuleLink, NavSection, AppState};

    pub struct MarketplaceModule;
    pub static MODULE: &dyn Module = &MarketplaceModule;

    impl Module for MarketplaceModule {
        fn code(&self) -> &'static str { "MOD" }
        fn name(&self) -> &'static str { "Modules" }
        fn name_cn(&self) -> &'static str { "模块市场" }
        fn nav_section(&self) -> NavSection { NavSection::System }
        fn nav_icon(&self) -> IconKind { IconKind::Modules }
        fn glyph(&self) -> &'static str { "mod" }
        fn description(&self) -> &'static str { "已注册模块的概览与扩展入口" }
        fn version(&self) -> &'static str { "0.1.0" }
        fn migrations(&self) -> &'static [(&'static str, &'static str)] { &[] }
        fn routes(&self, _state: AppState) -> axum::Router<AppState> { axum::Router::new() }
        fn links(&self) -> &'static [ModuleLink] { &[] }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{MarketplaceModule, MODULE};
