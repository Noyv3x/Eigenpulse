use crate::Icon;
use ep_core::IconKind;
use leptos::prelude::*;

/// In-app confirmation dialog. Open/close is controlled by `open`; the caller
/// provides `on_confirm` (called when the user accepts) and optional copy.
///
/// The dialog renders nothing while `open` is false — there is no DOM cost
/// for callers that never trigger it.
///
/// For dialogs whose confirm action needs to drive a server `<ActionForm>`
/// (rather than a JS callback), see `RowDeleteAction` — submitting form
/// children inside a generic `ChildrenFn` slot collides with Leptos's
/// build-multiple-times semantics on owned-String attributes, so the
/// row-level affordance keeps its own copy of the dialog chrome.
#[component]
pub fn ConfirmDialog(
    open: RwSignal<bool>,
    #[prop(into)] title: String,
    #[prop(into, optional)] desc: Option<String>,
    #[prop(into, optional)] confirm_label: Option<String>,
    #[prop(into, optional)] cancel_label: Option<String>,
    #[prop(default = true)] danger: bool,
    on_confirm: Callback<()>,
) -> impl IntoView {
    let confirm_label = confirm_label.unwrap_or_else(|| "Confirm".to_string());
    let cancel_label = cancel_label.unwrap_or_else(|| "Cancel".to_string());
    let confirm_class = if danger {
        "btn primary danger-action"
    } else {
        "btn primary"
    };
    let title_clone = title.clone();
    let desc_clone = desc.clone();
    let confirm_label_clone = confirm_label.clone();
    let cancel_label_clone = cancel_label.clone();
    view! {
        <div class="confirm-slot">
            {move || {
                if !open.get() { return view! { <span></span> }.into_any(); }
                let title = title_clone.clone();
                let desc = desc_clone.clone();
                let confirm_label = confirm_label_clone.clone();
                let cancel_label = cancel_label_clone.clone();
                view! {
                    <div class="fin-modal-backdrop confirm-backdrop"
                         on:click=move |_| open.set(false)>
                        <div class="fin-modal confirm-modal" role="alertdialog" aria-modal="true"
                             on:click=move |e| e.stop_propagation()>
                            <div class="confirm-body">
                                <div class="confirm-icon" class:danger=danger>
                                    <Icon kind=IconKind::Close size=18/>
                                </div>
                                <div class="confirm-text">
                                    <div class="confirm-title">{title}</div>
                                    {desc.map(|d| view! { <p class="confirm-desc">{d}</p> })}
                                </div>
                            </div>
                            <div class="confirm-foot">
                                <button class="btn ghost" type="button"
                                        on:click=move |_| open.set(false)>{cancel_label}</button>
                                <button class=confirm_class type="button"
                                        on:click=move |_| {
                                            open.set(false);
                                            on_confirm.run(());
                                        }>{confirm_label}</button>
                            </div>
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}
