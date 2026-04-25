pub mod view;

pub use view::FitnessView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{IconKind, Module, ModuleLink, NavSection, AppState};

    pub struct FitnessModule;
    pub static MODULE: &dyn Module = &FitnessModule;

    impl Module for FitnessModule {
        fn code(&self) -> &'static str { "FIT" }
        fn name(&self) -> &'static str { "Fitness" }
        fn name_cn(&self) -> &'static str { "健身管理" }
        fn nav_section(&self) -> NavSection { NavSection::Modules }
        fn nav_icon(&self) -> IconKind { IconKind::Fitness }
        fn glyph(&self) -> &'static str { "fit" }
        fn description(&self) -> &'static str { "训练计划、动作库、身体指标" }
        fn version(&self) -> &'static str { "0.1.0" }
        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[("001_fitness", include_str!("../migrations/001_fitness.sql"))]
        }
        fn routes(&self, _state: AppState) -> axum::Router<AppState> { axum::Router::new() }
        fn links(&self) -> &'static [ModuleLink] {
            &[
                ModuleLink { source: "FIT", target: "DSH", kind: "totals-feed" },
                ModuleLink { source: "FIT", target: "FIN", kind: "doc-ref" },
            ]
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{FitnessModule, MODULE};
