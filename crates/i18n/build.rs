//! Compile-time i18n catalog builder.
//!
//! Scans `{crates,modules,app}/<x>/i18n/{en,zh-CN}.json`, merges them
//! into two `phf::Map` literals at `$OUT_DIR/generated.rs`. Panics on
//! missing locale file, missing-on-one-side keys, namespace-prefix
//! violation, or nested-JSON schema — the build is the right place to
//! enforce these invariants.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const LOCALES: &[&str] = &["zh-CN", "en"];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("ep-i18n must live two levels under the workspace root")
        .to_path_buf();

    // Watch the parent dirs so adding a new `<x>/i18n/` directory
    // re-runs discovery — without this, a fresh bundle wouldn't be
    // picked up until something else forced a build.rs re-run.
    for sub in ["crates", "modules", "app"] {
        let p = workspace_root.join(sub);
        if p.exists() {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    }

    let bundles = discover_bundles(&workspace_root);
    if bundles.is_empty() {
        write_generated(&BTreeMap::new(), &BTreeMap::new());
        return;
    }

    let mut catalogs: BTreeMap<&'static str, BTreeMap<String, String>> =
        LOCALES.iter().map(|l| (*l, BTreeMap::new())).collect();

    for bundle in &bundles {
        for &locale in LOCALES {
            let path = bundle.dir.join(format!("{locale}.json"));
            if !path.exists() {
                panic!(
                    "i18n: missing locale file {} for namespace `{}`. \
                     Every i18n/ dir must ship one JSON per locale in {LOCALES:?}.",
                    path.display(),
                    bundle.namespace_prefix
                );
            }
            let raw = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("i18n: read {}: {e}", path.display()));
            let parsed: BTreeMap<String, String> = serde_json::from_str(&raw).unwrap_or_else(|e| {
                panic!(
                    "i18n: parse {} as flat string map: {e}. \
                         All keys must be top-level (`finance.page.title`), no nested objects.",
                    path.display()
                )
            });
            for (key, value) in parsed {
                if !key.starts_with(&format!("{}.", bundle.namespace_prefix))
                    && key != bundle.namespace_prefix
                {
                    panic!(
                        "i18n: key `{}` in {} violates the namespace prefix `{}.`. \
                         Each i18n/ dir owns exactly one prefix; declare cross-cutting keys in app/i18n/ instead.",
                        key,
                        path.display(),
                        bundle.namespace_prefix,
                    );
                }
                let cat = catalogs.get_mut(locale).unwrap();
                if let Some(existing) = cat.insert(key.clone(), value) {
                    panic!(
                        "i18n: duplicate key `{}` (locale {}). \
                         Already defined as `{}`. Pick a unique prefix per namespace.",
                        key, locale, existing
                    );
                }
            }
            println!("cargo:rerun-if-changed={}", path.display());
        }
        println!("cargo:rerun-if-changed={}", bundle.dir.display());
    }

    // Cross-locale parity check.
    let zh_keys: std::collections::BTreeSet<&str> =
        catalogs["zh-CN"].keys().map(|k| k.as_str()).collect();
    let en_keys: std::collections::BTreeSet<&str> =
        catalogs["en"].keys().map(|k| k.as_str()).collect();
    let only_zh: Vec<&&str> = zh_keys.difference(&en_keys).collect();
    let only_en: Vec<&&str> = en_keys.difference(&zh_keys).collect();
    if !only_zh.is_empty() || !only_en.is_empty() {
        panic!(
            "i18n: locale key sets diverge.\n  only in zh-CN: {only_zh:?}\n  only in en: {only_en:?}"
        );
    }
    for key in zh_keys {
        let zh_placeholders = placeholders(catalogs["zh-CN"].get(key).expect("zh key exists"));
        let en_placeholders = placeholders(catalogs["en"].get(key).expect("en key exists"));
        if zh_placeholders != en_placeholders {
            panic!(
                "i18n: placeholder set diverges for key `{key}`.\n  zh-CN: {zh_placeholders:?}\n  en: {en_placeholders:?}"
            );
        }
    }

    write_generated(&catalogs["zh-CN"], &catalogs["en"]);
}

struct Bundle {
    dir: PathBuf,
    namespace_prefix: String,
}

fn discover_bundles(root: &Path) -> Vec<Bundle> {
    let mut out = Vec::new();
    for sub in ["crates", "modules", "app"] {
        let sub_path = root.join(sub);
        if !sub_path.exists() {
            continue;
        }
        if sub == "app" {
            let i18n_dir = sub_path.join("i18n");
            if i18n_dir.is_dir() {
                out.push(Bundle {
                    dir: i18n_dir,
                    namespace_prefix: "app".into(),
                });
            }
            continue;
        }
        for entry in WalkDir::new(&sub_path)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_dir())
        {
            let crate_dir = entry.path();
            let i18n_dir = crate_dir.join("i18n");
            if !i18n_dir.is_dir() {
                continue;
            }
            let crate_name = crate_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
            out.push(Bundle {
                dir: i18n_dir,
                namespace_prefix: namespace_for_crate(crate_name),
            });
        }
    }
    out
}

/// Special-case: `crates/i18n` owns cross-cutting `app.common.*` keys;
/// `modules/mod_marketplace` exposes itself under `marketplace.*`.
fn namespace_for_crate(name: &str) -> String {
    match name {
        "i18n" => "app.common".into(),
        "mod_marketplace" => "marketplace".into(),
        other => other.into(),
    }
}

fn placeholders(template: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end_rel) = template[i + 1..].find('}') {
                let name = &template[i + 1..i + 1 + end_rel];
                if is_placeholder_name(name) {
                    out.insert(name.to_string());
                }
                i += 1 + end_rel + 1;
                continue;
            }
        }
        let Some(ch) = template[i..].chars().next() else {
            break;
        };
        i += ch.len_utf8();
    }
    out
}

fn is_placeholder_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn write_generated(zh: &BTreeMap<String, String>, en: &BTreeMap<String, String>) {
    let out_dir = env::var("OUT_DIR").unwrap();
    let path = PathBuf::from(out_dir).join("generated.rs");
    let mut f =
        fs::File::create(&path).unwrap_or_else(|e| panic!("i18n: create {}: {e}", path.display()));

    writeln!(f, "// Generated by build.rs — do not edit.").unwrap();
    writeln!(f, "//").unwrap();
    writeln!(
        f,
        "// {} keys × {} locales = {} string entries.",
        zh.len(),
        LOCALES.len(),
        zh.len() * LOCALES.len()
    )
    .unwrap();

    write_phf_map(&mut f, "ZH_CN", zh);
    write_phf_map(&mut f, "EN", en);
}

fn write_phf_map(f: &mut fs::File, name: &str, entries: &BTreeMap<String, String>) {
    let mut builder = phf_codegen::Map::<&str>::new();
    for (k, v) in entries {
        builder.entry(k.as_str(), &format!("{:?}", v));
    }
    writeln!(
        f,
        "pub(crate) static {name}: phf::Map<&'static str, &'static str> = {};",
        builder.build()
    )
    .unwrap();
}
