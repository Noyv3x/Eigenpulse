use leptos::prelude::*;
use leptos::server_fn::{client::Client, codec::PostUrl, request::ClientReq, ServerFn};

/// Per-row "delete" / "revoke" affordance for ledger-style tables.
///
/// Renders a small destructive-styled button. When the user clicks, an
/// in-app confirmation dialog appears (instead of the browser's native
/// `confirm()` prompt); only on confirmation do we actually submit the
/// `ActionForm` that drives the underlying server action. The dialog chrome
/// is kept inline here rather than shared: submitting `<ActionForm>` children
/// through a generic slot collides with Leptos's build-multiple-times
/// semantics on owned-`String` attributes.
///
/// `value` ships as the hidden input's value. Default `field="doc_id"` fits
/// most modules; override with `field="id"` for PAT revoke / notify channel
/// where the server fn takes an `i64` named `id` (the wire is still string-
/// form-data, so a stringified `i64` deserialises fine).
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
    let cancel_label = ep_i18n::t(locale, "ui.row_action.cancel_label").to_string();
    let confirm_label = ep_i18n::t(locale, "ui.row_action.confirm_label").to_string();
    let open = RwSignal::new(false);
    // Close the dialog once the action has actually fired — NOT from the
    // submit button's own `on:click`. Flipping `open` in the submit handler
    // tears the `<ActionForm>` out of the DOM during the click event, before
    // the browser dispatches the submit, so the POST is silently dropped
    // ("Form submission canceled because the form is not connected") and the
    // just-disposed reactive closure panics. Watching the version closes the
    // dialog on completion instead. Mirrors the create-form pattern in
    // finance's `render_account_manager`.
    let last_version = RwSignal::new(0usize);
    Effect::new(move |_| {
        let v = action.version().get();
        if v != 0 && v != last_version.get_untracked() {
            open.set(false);
            last_version.set(v);
        }
    });
    let value_for_form = value.clone();
    let field_for_form = field.clone();
    let confirm_for_dialog = confirm_msg.clone();
    let confirm_label_for_btn = confirm_label.clone();
    let cancel_label_for_btn = cancel_label.clone();
    view! {
        <span class="row-actions-slot">
            <button class="btn sm danger" type="button"
                    on:click=move |_| open.set(true)>{label}</button>
            {move || {
                if !open.get() {
                    return view! { <span></span> }.into_any();
                }
                let value_inner = value_for_form.clone();
                let field_inner = field_for_form.clone();
                let confirm_text = confirm_for_dialog.clone();
                let confirm_label = confirm_label_for_btn.clone();
                let cancel_label = cancel_label_for_btn.clone();
                view! {
                    <div class="fin-modal-backdrop confirm-backdrop"
                         on:click=move |_| open.set(false)>
                        <div class="fin-modal confirm-modal" role="alertdialog" aria-modal="true"
                             aria-labelledby="row-action-confirm-title"
                             on:click=move |e| e.stop_propagation()>
                            <div class="confirm-body">
                                <div class="confirm-icon danger">
                                    <crate::Icon kind=ep_core::IconKind::Close size=18/>
                                </div>
                                <div class="confirm-text">
                                    <div class="confirm-title" id="row-action-confirm-title">{confirm_text}</div>
                                </div>
                            </div>
                            <ActionForm action=action attr:class="confirm-foot">
                                <input type="hidden" name=field_inner value=value_inner/>
                                <button class="btn ghost" type="button"
                                        on:click=move |_| open.set(false)>{cancel_label}</button>
                                <button class="btn primary danger-action" type="submit">{confirm_label}</button>
                            </ActionForm>
                        </div>
                    </div>
                }.into_any()
            }}
        </span>
    }
}
