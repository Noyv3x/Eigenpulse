use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{EN, ZH_CN};

#[test]
fn rust_i18n_key_references_exist_in_catalog() {
    let root = workspace_root();
    let scan_roots = [
        root.join("app/src"),
        root.join("modules"),
        root.join("crates/ui/src"),
    ];
    let mut refs = BTreeSet::new();
    for dir in scan_roots {
        collect_i18n_refs(&dir, &mut refs);
    }

    let missing: Vec<_> = refs
        .into_iter()
        .filter(|(_, key)| EN.get(key.as_str()).is_none() || ZH_CN.get(key.as_str()).is_none())
        .collect();
    assert!(
        missing.is_empty(),
        "Rust source references missing i18n keys: {missing:#?}"
    );
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/i18n has a workspace root ancestor")
        .to_path_buf()
}

fn collect_i18n_refs(dir: &Path, refs: &mut BTreeSet<(String, String)>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_i18n_refs(&path, refs);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let src = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            collect_i18n_refs_from_source(&path, &src, refs);
        }
    }
}

fn collect_i18n_refs_from_source(path: &Path, src: &str, refs: &mut BTreeSet<(String, String)>) {
    for call in ["t(", "tf(", "ep_i18n::t(", "ep_i18n::tf("] {
        collect_second_string_arg(path, src, call, refs);
    }
    for call in ["err(", "err_with(", "ep_i18n::err(", "ep_i18n::err_with("] {
        collect_first_string_arg(path, src, call, refs);
    }
    for call in ["t!(", "tf!("] {
        collect_macro_key(path, src, call, refs);
    }
}

fn collect_first_string_arg(
    path: &Path,
    src: &str,
    call: &str,
    refs: &mut BTreeSet<(String, String)>,
) {
    let mut pos = 0;
    while let Some(found) = src[pos..].find(call) {
        let start = pos + found;
        pos = start + call.len();
        if !is_call_boundary(src, start) {
            continue;
        }
        if let Some(key) = string_literal_at_or_after(src, pos) {
            insert_key_ref(path, key, refs);
        }
    }
}

fn collect_second_string_arg(
    path: &Path,
    src: &str,
    call: &str,
    refs: &mut BTreeSet<(String, String)>,
) {
    let mut pos = 0;
    while let Some(found) = src[pos..].find(call) {
        let start = pos + found;
        pos = start + call.len();
        if !is_call_boundary(src, start) {
            continue;
        }
        let Some(comma) = src[pos..].find(',') else {
            continue;
        };
        if let Some(key) = string_literal_at_or_after(src, pos + comma + 1) {
            insert_key_ref(path, key, refs);
        }
    }
}

fn collect_macro_key(path: &Path, src: &str, call: &str, refs: &mut BTreeSet<(String, String)>) {
    let mut pos = 0;
    while let Some(found) = src[pos..].find(call) {
        let start = pos + found;
        pos = start + call.len();
        if !is_call_boundary(src, start) {
            continue;
        }
        let Some(comma) = src[pos..].find(',') else {
            continue;
        };
        let mut key = String::new();
        for ch in src[pos + comma + 1..].trim_start().chars() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
                key.push(ch);
            } else {
                break;
            }
        }
        if !key.is_empty() {
            insert_key_ref(path, key, refs);
        }
    }
}

fn is_call_boundary(src: &str, start: usize) -> bool {
    src[..start]
        .chars()
        .next_back()
        .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
}

fn string_literal_at_or_after(src: &str, start: usize) -> Option<String> {
    let quote = src[start..].find('"')? + start;
    let mut out = String::new();
    let mut escaped = false;
    for ch in src[quote + 1..].chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}

fn insert_key_ref(path: &Path, key: String, refs: &mut BTreeSet<(String, String)>) {
    if is_i18n_key(&key) {
        refs.insert((path.display().to_string(), key));
    }
}

fn is_i18n_key(key: &str) -> bool {
    [
        "app.",
        "finance.",
        "fitness.",
        "learning.",
        "marketplace.",
        "ui.",
    ]
    .iter()
    .any(|prefix| key.starts_with(prefix))
}
