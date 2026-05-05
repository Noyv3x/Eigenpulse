/// HTML-escape `&`, `<`, `>`, `"` for safe inclusion in HTML text or attribute values.
pub fn html_escape(s: &str) -> String {
    let Some(first) = s.find(['&', '<', '>', '"']) else {
        return s.to_string();
    };

    let mut out = String::with_capacity(s.len() + 8);
    out.push_str(&s[..first]);
    for ch in s[first..].chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_plain_text_unchanged() {
        assert_eq!(html_escape("Eigenpulse"), "Eigenpulse");
    }

    #[test]
    fn escapes_html_text_and_attributes() {
        assert_eq!(
            html_escape(r#"<a href="/?next=a&b">x</a>"#),
            "&lt;a href=&quot;/?next=a&amp;b&quot;&gt;x&lt;/a&gt;"
        );
    }

    #[test]
    fn preserves_non_ascii_text() {
        assert_eq!(html_escape("工资 · <入账>"), "工资 · &lt;入账&gt;");
    }
}
