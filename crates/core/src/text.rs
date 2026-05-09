/// Trim text input and return `None` when it is empty after trimming.
///
/// This keeps form/server-fn normalization consistent for optional text
/// columns such as notes, authors, programs, and ad-hoc descriptions.
pub fn trim_to_option(input: &str) -> Option<String> {
    let s = input.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// Return a trimmed in-app absolute path if it is safe to put in redirects or
/// same-origin links.
///
/// This deliberately accepts only local absolute paths (`/finance`, not
/// `https://...`, `//host`, `javascript:...`, or backslash variants) and rejects
/// raw or percent-encoded control characters.
pub fn safe_in_app_path(input: &str) -> Option<&str> {
    let path = input.trim();
    if path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && !contains_percent_encoded_control(path)
    {
        Some(path)
    } else {
        None
    }
}

/// Percent-encode a string for use as one query parameter value.
///
/// This is intentionally small and dependency-free because auth redirects use
/// it on both app and auth crate boundaries.
pub fn url_encode_query_value(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(b));
            }
            _ => {
                out.push('%');
                out.push(char::from(HEX[(b >> 4) as usize]));
                out.push(char::from(HEX[(b & 0x0F) as usize]));
            }
        }
    }
    out
}

/// Return a trimmed document id/reference if it has the expected Eigenpulse
/// identifier shape.
///
/// The check intentionally stays format-tolerant so module-specific generators
/// can evolve (`FIN-26092`, `FIT-S-0412`, `LRN-B-0001`, legacy fixtures like
/// `FIT-26001`) while still rejecting URLs, paths, controls, and free text.
pub fn safe_doc_id(input: &str) -> Option<&str> {
    let doc_id = input.trim();
    if !(4..=32).contains(&doc_id.len()) {
        return None;
    }
    let mut parts = doc_id.split('-');
    let prefix = parts.next()?;
    if !(2..=8).contains(&prefix.len())
        || !prefix.bytes().all(|b| b.is_ascii_uppercase())
        || parts.clone().next().is_none()
    {
        return None;
    }

    let mut last_was_hyphen = false;
    for b in doc_id.bytes() {
        match b {
            b'A'..=b'Z' | b'0'..=b'9' => last_was_hyphen = false,
            b'-' if !last_was_hyphen => last_was_hyphen = true,
            _ => return None,
        }
    }
    (!last_was_hyphen).then_some(doc_id)
}

fn contains_percent_encoded_control(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'%' {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                let decoded = (hi << 4) | lo;
                if decoded <= 0x1f || decoded == 0x7f {
                    return true;
                }
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    false
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_to_option_drops_empty_input() {
        assert_eq!(trim_to_option(""), None);
        assert_eq!(trim_to_option(" \t\n "), None);
    }

    #[test]
    fn trim_to_option_keeps_trimmed_text() {
        assert_eq!(trim_to_option("  note  ").as_deref(), Some("note"));
    }

    #[test]
    fn safe_in_app_path_accepts_local_absolute_paths() {
        assert_eq!(safe_in_app_path(" /finance "), Some("/finance"));
        assert_eq!(
            safe_in_app_path("/settings/security?tab=pat"),
            Some("/settings/security?tab=pat")
        );
    }

    #[test]
    fn safe_in_app_path_rejects_external_or_control_paths() {
        for raw in [
            "",
            "https://example.com",
            "//example.com",
            "javascript:alert(1)",
            "/finance\\evil",
            "/finance%0d%0aevil",
            "/finance%7F",
        ] {
            assert_eq!(safe_in_app_path(raw), None, "raw={raw}");
        }
    }

    #[test]
    fn url_encode_query_value_preserves_unreserved_ascii() {
        assert_eq!(
            url_encode_query_value("/finance/ABC-123_~"),
            "%2Ffinance%2FABC-123_~"
        );
    }

    #[test]
    fn url_encode_query_value_escapes_query_separators_and_utf8() {
        assert_eq!(
            url_encode_query_value("/finance?tab=预算&x=1#frag"),
            "%2Ffinance%3Ftab%3D%E9%A2%84%E7%AE%97%26x%3D1%23frag"
        );
    }

    #[test]
    fn safe_doc_id_accepts_known_shapes() {
        for raw in [" FIN-26092 ", "FIT-S-0412", "LRN-B-0001", "FIT-26001"] {
            assert_eq!(safe_doc_id(raw), Some(raw.trim()), "raw={raw}");
        }
    }

    #[test]
    fn safe_doc_id_rejects_free_text_urls_and_paths() {
        for raw in [
            "",
            "F-1",
            "FIN",
            "fin-26092",
            "FIN--26092",
            "FIN-",
            "FIN/26092",
            "https://example.com",
            "javascript:alert(1)",
            "FIN-26092\nX",
            "VERY-LONG-MODULE-CODE-26092-EXTRA",
        ] {
            assert_eq!(safe_doc_id(raw), None, "raw={raw}");
        }
    }
}
