pub mod model;
pub mod server_fns;
pub mod view;

pub use model::*;
pub use server_fns::*;
pub use view::FitnessView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::{AppState, IconKind, Module, ModuleLink, NavSection};

    pub struct FitnessModule;
    pub static MODULE: &dyn Module = &FitnessModule;

    impl Module for FitnessModule {
        fn code(&self) -> &'static str {
            "FIT"
        }
        fn name(&self) -> &'static str {
            "Fitness"
        }
        fn name_cn(&self) -> &'static str {
            "\u{5065}\u{8eab}\u{7ba1}\u{7406}"
        }
        fn nav_section(&self) -> NavSection {
            NavSection::Modules
        }
        fn nav_icon(&self) -> IconKind {
            IconKind::Fitness
        }
        fn glyph(&self) -> &'static str {
            "fit"
        }
        fn description(&self) -> &'static str {
            "\u{8bad}\u{7ec3}\u{8ba1}\u{5212}\u{3001}\u{52a8}\u{4f5c}\u{5e93}\u{3001}\u{8eab}\u{4f53}\u{6307}\u{6807}"
        }
        fn version(&self) -> &'static str {
            "0.1.0"
        }
        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[("001_fitness", include_str!("../migrations/001_fitness.sql"))]
        }
        fn routes(&self, _state: AppState) -> axum::Router<AppState> {
            axum::Router::new()
        }
        fn links(&self) -> &'static [ModuleLink] {
            &[
                ModuleLink {
                    source: "FIT",
                    target: "DSH",
                    kind: "totals-feed",
                },
                ModuleLink {
                    source: "FIT",
                    target: "FIN",
                    kind: "doc-ref",
                },
            ]
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{FitnessModule, MODULE};
