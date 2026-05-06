pub mod model;
pub mod server_fns;
pub mod view;

pub use model::*;
pub use server_fns::*;
pub use view::LearningView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{AppState, IconKind, Module, ModuleLink, NavSection};

    pub struct LearningModule;
    pub static MODULE: &dyn Module = &LearningModule;

    impl Module for LearningModule {
        fn code(&self) -> &'static str {
            "LRN"
        }
        fn name(&self) -> &'static str {
            "Learning"
        }
        fn name_cn(&self) -> &'static str {
            "\u{5b66}\u{4e60}\u{7ba1}\u{7406}"
        }
        fn nav_section(&self) -> NavSection {
            NavSection::Modules
        }
        fn nav_icon(&self) -> IconKind {
            IconKind::Learning
        }
        fn glyph(&self) -> &'static str {
            "lrn"
        }
        fn description(&self) -> &'static str {
            "\u{8bfe}\u{7a0b}\u{3001}\u{9605}\u{8bfb}\u{3001}\u{7b14}\u{8bb0}\u{3001}Anki \u{96c6}\u{6210}"
        }
        fn version(&self) -> &'static str {
            "0.1.0"
        }
        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[(
                "001_learning",
                include_str!("../migrations/001_learning.sql"),
            )]
        }
        fn routes(&self, _state: AppState) -> axum::Router<AppState> {
            axum::Router::new()
        }
        fn links(&self) -> &'static [ModuleLink] {
            &[
                ModuleLink {
                    source: "LRN",
                    target: "DSH",
                    kind: "totals-feed",
                },
                ModuleLink {
                    source: "LRN",
                    target: "FIN",
                    kind: "doc-ref",
                },
            ]
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{LearningModule, MODULE};
