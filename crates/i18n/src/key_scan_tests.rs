use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{EN, ZH_CN};

#[test]
fn rust_i18n_keys_and_catalog_are_in_sync() {
    let root = workspace_root();
    let scan_roots = [
        root.join("app/src"),
        root.join("modules/finance/src"),
        root.join("modules/fitness/src"),
        root.join("modules/journal/src"),
        root.join("crates"),
    ];
    let mut refs = BTreeSet::new();
    for dir in scan_roots {
        collect_i18n_refs(&dir, &mut refs);
    }

    let missing: Vec<_> = refs
        .iter()
        .filter(|(_, key)| EN.get(key.as_str()).is_none() || ZH_CN.get(key.as_str()).is_none())
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "Rust source references missing i18n keys: {missing:#?}"
    );

    let referenced = refs
        .into_iter()
        .map(|(_, key)| key)
        .collect::<BTreeSet<_>>();
    let unused = EN
        .keys()
        .filter(|key| !referenced.contains(**key))
        .copied()
        .collect::<Vec<_>>();
    assert!(
        unused.is_empty(),
        "i18n catalog keys have no production Rust string-literal reference: {unused:#?}"
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
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs")
            && path
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|name| !name.ends_with("_tests.rs"))
        {
            let src = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let production = strip_cfg_test_items(&strip_rust_comments(&src));
            collect_i18n_refs_from_source(&path, &production, refs);
        }
    }
}

fn collect_i18n_refs_from_source(path: &Path, src: &str, refs: &mut BTreeSet<(String, String)>) {
    collect_string_literal_keys(path, src, refs);

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

/// Collect catalog-backed Rust string literals, including descriptor fields
/// and DTO label keys that are resolved indirectly at runtime. Call-specific
/// scanning below additionally catches missing direct-call keys and the
/// unquoted `t!(…, a.b.c)` macro form.
fn collect_string_literal_keys(path: &Path, src: &str, refs: &mut BTreeSet<(String, String)>) {
    let bytes = src.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos] != b'"' {
            pos += 1;
            continue;
        }

        pos += 1;
        let mut value = String::new();
        let mut escaped = false;
        while pos < bytes.len() {
            let byte = bytes[pos];
            pos += 1;
            if escaped {
                value.push(byte as char);
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                if EN.get(value.as_str()).is_some() || ZH_CN.get(value.as_str()).is_some() {
                    insert_key_ref(path, value, refs);
                }
                break;
            } else if byte.is_ascii() {
                value.push(byte as char);
            } else {
                // All i18n keys are ASCII; a non-ASCII literal cannot match.
                value.clear();
            }
        }
    }
}

fn strip_rust_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos] == b'"' {
            out.push('"');
            pos += 1;
            let mut escaped = false;
            while pos < bytes.len() {
                let byte = bytes[pos];
                out.push(byte as char);
                pos += 1;
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    break;
                }
            }
        } else if bytes[pos..].starts_with(b"//") {
            pos += 2;
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
        } else if bytes[pos..].starts_with(b"/*") {
            pos += 2;
            let mut depth = 1usize;
            while pos < bytes.len() && depth > 0 {
                if bytes[pos..].starts_with(b"/*") {
                    depth += 1;
                    pos += 2;
                } else if bytes[pos..].starts_with(b"*/") {
                    depth -= 1;
                    pos += 2;
                } else {
                    if bytes[pos] == b'\n' {
                        out.push('\n');
                    }
                    pos += 1;
                }
            }
        } else {
            out.push(bytes[pos] as char);
            pos += 1;
        }
    }
    out
}

fn strip_cfg_test_items(src: &str) -> String {
    let source = strip_cfg_items_with_attr(src, "#[cfg(all(test, feature = \"ssr\"))]");
    strip_cfg_items_with_attr(&source, "#[cfg(test)]")
}

fn strip_cfg_items_with_attr(src: &str, attr: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut cursor = 0;
    while let Some(relative) = src[cursor..].find(attr) {
        let start = cursor + relative;
        out.push_str(&src[cursor..start]);
        let item_start = start + attr.len();
        let rest = &src[item_start..];
        let brace = rest.find('{');
        let semicolon = rest.find(';');
        match (brace, semicolon) {
            (None, Some(end)) => {
                cursor = item_start + end + 1;
            }
            (Some(open), Some(end)) if end < open => {
                cursor = item_start + end + 1;
            }
            (Some(open), _) => {
                let open = item_start + open;
                cursor = matching_brace_end(src, open).unwrap_or(src.len());
            }
            (None, None) => {
                cursor = src.len();
            }
        }
    }
    out.push_str(&src[cursor..]);
    out
}

fn matching_brace_end(src: &str, open: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth = 0usize;
    let mut pos = open;
    let mut in_string = false;
    let mut escaped = false;
    while pos < bytes.len() {
        let byte = bytes[pos];
        pos += 1;
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos);
                }
            }
            _ => {}
        }
    }
    None
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
    ["app.", "core.", "finance.", "fitness.", "journal.", "ui."]
        .iter()
        .any(|prefix| key.starts_with(prefix))
}
