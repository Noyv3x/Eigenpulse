//! The single compile-time composition root for bundled business modules.

use ep_core::{ModuleDescriptor, ModuleSummary};
use leptos::server_fn::ServerFnError;
use std::future::Future;
use std::pin::Pin;

type SummaryFuture =
    Pin<Box<dyn Future<Output = Result<ModuleSummary, ServerFnError>> + Send + 'static>>;

/// Everything the shared application shell needs to compose one independent
/// business module. Adding a module requires one entry here plus its explicit
/// route; navigation, dashboard summaries, PAT scopes, and SSR registration
/// all derive from this catalog.
pub struct ModuleEntry {
    pub descriptor: &'static ModuleDescriptor,
    load_summary: fn() -> SummaryFuture,
    #[cfg(feature = "ssr")]
    module: &'static dyn ep_core::Module,
}

impl ModuleEntry {
    pub fn load_summary(&self) -> SummaryFuture {
        (self.load_summary)()
    }
}

fn finance_summary() -> SummaryFuture {
    Box::pin(ep_finance::load_home_summary())
}

fn fitness_summary() -> SummaryFuture {
    Box::pin(ep_fitness::load_home_summary())
}

fn journal_summary() -> SummaryFuture {
    Box::pin(ep_journal::load_home_summary())
}

pub static MODULES: &[ModuleEntry] = &[
    ModuleEntry {
        descriptor: &ep_finance::DESCRIPTOR,
        load_summary: finance_summary,
        #[cfg(feature = "ssr")]
        module: ep_finance::MODULE,
    },
    ModuleEntry {
        descriptor: &ep_fitness::DESCRIPTOR,
        load_summary: fitness_summary,
        #[cfg(feature = "ssr")]
        module: ep_fitness::MODULE,
    },
    ModuleEntry {
        descriptor: &ep_journal::DESCRIPTOR,
        load_summary: journal_summary,
        #[cfg(feature = "ssr")]
        module: ep_journal::MODULE,
    },
];

pub fn descriptors() -> Vec<&'static ModuleDescriptor> {
    MODULES.iter().map(|entry| entry.descriptor).collect()
}

#[cfg(feature = "ssr")]
pub fn registry() -> ep_core::ModuleRegistry {
    MODULES
        .iter()
        .fold(ep_core::ModuleRegistry::new(), |registry, entry| {
            registry.with(entry.module)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;

    #[test]
    fn composition_catalog_has_unique_module_identity() {
        let mut slugs = HashSet::new();
        let mut routes = HashSet::new();
        let mut scopes = HashSet::new();
        for entry in MODULES {
            assert!(slugs.insert(entry.descriptor.slug));
            assert!(routes.insert(entry.descriptor.route));
            assert!(scopes.insert(entry.descriptor.read_scope));
            assert!(scopes.insert(entry.descriptor.write_scope));
        }
    }

    #[test]
    fn composition_catalog_is_exactly_the_current_apps() {
        let actual = MODULES
            .iter()
            .map(|entry| {
                (
                    entry.descriptor.slug,
                    entry.descriptor.route,
                    entry.descriptor.read_scope,
                    entry.descriptor.write_scope,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                ("finance", "/finance", "finance:read", "finance:write"),
                ("fitness", "/fitness", "fitness:read", "fitness:write"),
                ("journal", "/journal", "journal:read", "journal:write"),
            ]
        );
        assert_eq!(
            descriptors(),
            MODULES
                .iter()
                .map(|entry| entry.descriptor)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn business_module_sources_observe_storage_isolation() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("app has a workspace parent");
        let modules = [
            ("finance", "fin_", "Finance"),
            ("fitness", "fit_", "Fitness"),
            ("journal", "jrn_", "Journal"),
        ];
        for (module, _, _) in modules {
            for (owner_slug, prefix, owner) in modules {
                if owner_slug != module {
                    assert_module_does_not_reference_prefix(
                        &root.join("modules").join(module),
                        prefix,
                        owner,
                    );
                }
            }
        }
    }

    fn assert_module_does_not_reference_prefix(module: &Path, prefix: &str, owner: &str) {
        let mut files = Vec::new();
        collect_production_files(&module.join("src"), &mut files);
        collect_production_files(&module.join("migrations"), &mut files);
        assert!(
            !files.is_empty(),
            "no production files found under {}",
            module.display()
        );

        for path in files {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            assert!(
                !source.contains(prefix),
                "{} must not reference {owner}-owned table prefix `{prefix}`",
                path.display()
            );
        }
    }

    fn collect_production_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let entries =
            fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
        for entry in entries {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                collect_production_files(&path, out);
            } else if matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("rs" | "sql")
            ) {
                out.push(path);
            }
        }
    }
}
