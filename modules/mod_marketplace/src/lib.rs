mod view;

pub use view::MarketView;

#[cfg(feature = "ssr")]
mod ssr_module {
    use ep_core::Module;

    pub struct MarketplaceModule;
    pub static MODULE: &dyn Module = &MarketplaceModule;

    impl Module for MarketplaceModule {
        fn code(&self) -> &'static str {
            "MOD"
        }
        fn migrations(&self) -> &'static [(&'static str, &'static str)] {
            &[]
        }
    }

    #[cfg(test)]
    mod tests {
        use super::MODULE;

        #[test]
        fn every_migration_file_is_registered() {
            ep_core::assert_module_migrations_registered!(MODULE);
        }
    }
}

#[cfg(feature = "ssr")]
pub use ssr_module::{MarketplaceModule, MODULE};
