pub mod model;
pub mod server_fns;
pub mod view;

pub use model::*;
pub use server_fns::*;
pub use view::LearningView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{IconKind, Module, ModuleLink, NavSection, AppState};

    pub struct LearningModule;
    pub static MODULE: &dyn Module = &LearningModule;

    impl Module for LearningModule {
        fn code(&self) -> &'static str { "LRN" }
        fn name(&self) -> &'static str { "Learning" }
        fn name_cn(&self) -> &'static str { "学习管理" }
        fn nav_section(&self) -> NavSection { NavSection::Modules }
        fn nav_icon(&self) -> IconKind { IconKind::Learning }
        fn glyph(&self) -> &'static str { "lrn" }
        fn description(&self) -> &'static str { "课程、阅读、笔记、Anki 集成" }
        fn version(&self) -> &'static str { "0.1.0" }
        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[("001_learning", include_str!("../migrations/001_learning.sql"))]
        }
        fn routes(&self, _state: AppState) -> axum::Router<AppState> { axum::Router::new() }
        fn links(&self) -> &'static [ModuleLink] {
            &[
                ModuleLink { source: "LRN", target: "DSH", kind: "totals-feed" },
                ModuleLink { source: "LRN", target: "FIN", kind: "doc-ref" },
            ]
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{LearningModule, MODULE};
