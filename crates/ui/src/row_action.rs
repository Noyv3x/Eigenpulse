use leptos::prelude::*;
use leptos::server_fn::{client::Client, codec::PostUrl, request::ClientReq, ServerFn};

/// Per-row "delete" / "revoke" affordance for ledger-style tables.
///
/// Renders an inline `<ActionForm>` carrying a single hidden field plus a
/// red submit button with a `confirm()` JS guard, wrapped in
/// `<span class="row-actions-slot">` so tachys' text-node walker keeps a
/// stable anchor when the surrounding reactive content shifts. The wrapper
/// is re-emitted by this component, so callers don't need their own outer
/// span.
///
/// `value` ships as the hidden input's value. Default `field="doc_id"` fits
/// most modules; override with `field="id"` for PAT revoke / notify channel
/// where the server fn takes an `i64` named `id` (the wire is still string-
/// form-data, so a stringified `i64` deserialises fine).
///
/// `confirm` is escaped for a single-quoted JavaScript string before being
/// placed in the inline `onclick` handler.
#[component]
pub fn RowDeleteAction<S>(
    action: ServerAction<S>,
    #[prop(into)] value: String,
    #[prop(into, optional)] field: Option<String>,
    #[prop(into, optional)] confirm: Option<String>,
    #[prop(into, optional)] label: Option<String>,
) -> impl IntoView
where
    // Mirrors leptos's own `ActionForm` signature: form-encoded POST input
    // + FormData-from-web_sys ClientReq + DeserializeOwned for the server
    // fn struct.
    S: ServerFn<InputEncoding = PostUrl>
        + Clone
        + Send
        + Sync
        + 'static
        + serde::de::DeserializeOwned,
    <S as ServerFn>::Output: Send + Sync,
    <S as ServerFn>::Error: Send + Sync,
    <<<S as ServerFn>::Client as Client<<S as ServerFn>::Error>>::Request as ClientReq<
        <S as ServerFn>::Error,
    >>::FormData: From<leptos::web_sys::FormData>,
{
    let field = field.unwrap_or_else(|| "doc_id".into());
    let locale = ep_i18n::use_locale();
    let confirm_msg =
        confirm.unwrap_or_else(|| ep_i18n::t(locale, "ui.row_action.default_confirm").into());
    let label = label.unwrap_or_else(|| ep_i18n::t(locale, "ui.row_action.default_label").into());
    let onclick = format!(
        "return confirm('{}')",
        escape_js_single_quoted(&confirm_msg)
    );
    view! {
        <span class="row-actions-slot">
            <ActionForm action=action attr:style="display:inline">
                <input type="hidden" name=field value=value/>
                <button class="btn sm" type="submit"
                        style="color:var(--rose-ink)"
                        onclick=onclick>{label}</button>
            </ActionForm>
        </span>
    }
}

pub fn escape_js_single_quoted(s: &str) -> String {
    let Some(first) = s.find([
        '\\', '\'', '\n', '\r', '<', '>', '&', '\u{2028}', '\u{2029}',
    ]) else {
        return s.to_string();
    };

    let mut out = String::with_capacity(s.len() + 8);
    out.push_str(&s[..first]);
    for ch in s[first..].chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '\'' => out.push_str(r"\'"),
            '\n' => out.push_str(r"\n"),
            '\r' => out.push_str(r"\r"),
            '<' => out.push_str(r"\x3C"),
            '>' => out.push_str(r"\x3E"),
            '&' => out.push_str(r"\x26"),
            '\u{2028}' => out.push_str(r"\u2028"),
            '\u{2029}' => out.push_str(r"\u2029"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::escape_js_single_quoted;

    #[test]
    fn js_escape_leaves_plain_confirm_text_alone() {
        assert_eq!(
            escape_js_single_quoted("Delete this item?"),
            "Delete this item?"
        );
    }

    #[test]
    fn js_escape_handles_quotes_slashes_and_html_breakouts() {
        assert_eq!(
            escape_js_single_quoted("Bob's \\ item </script>\n&"),
            r"Bob\'s \\ item \x3C/script\x3E\n\x26"
        );
    }
}
