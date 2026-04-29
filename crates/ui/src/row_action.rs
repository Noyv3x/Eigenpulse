use leptos::prelude::*;
use leptos::server_fn::{
    client::Client,
    codec::PostUrl,
    request::ClientReq,
    ServerFn,
};

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
/// `confirm` and `label` are interpolated raw — pass **hard-coded string
/// literals only**. If a future caller wants to forward user data into the
/// confirm message, switch to a `data-confirm` attribute + a once-per-page
/// JS hook; the in-attribute escape used here is not a general HTML/JS
/// escape and would break on `\` / `<` / `</script>`.
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
    <<<S as ServerFn>::Client as Client<<S as ServerFn>::Error>>::Request
        as ClientReq<<S as ServerFn>::Error>>::FormData: From<leptos::web_sys::FormData>,
{
    let field = field.unwrap_or_else(|| "doc_id".into());
    let confirm_msg = confirm.unwrap_or_else(|| "确认删除？".into());
    let label = label.unwrap_or_else(|| "删除".into());
    let onclick = format!("return confirm('{confirm_msg}')");
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
