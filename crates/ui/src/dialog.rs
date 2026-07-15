use leptos::{html, prelude::*};

/// Shared native confirmation-dialog primitive.
///
/// Callers keep ownership of `open`, which lets action forms remain mounted
/// until their submit event has fired. The native `<dialog>` supplies the top
/// layer, background inertness, focus containment and Escape/cancel event.
/// The element is normally rendered inside a stable conditional slot; when it
/// is removed, cleanup returns focus to the control that opened it.
#[component]
pub fn Dialog(
    open: RwSignal<bool>,
    #[prop(into)] label: String,
    #[prop(optional)] dismissible: Option<Signal<bool>>,
    children: Children,
) -> impl IntoView {
    let dialog_ref = NodeRef::<html::Dialog>::new();
    let dismissible = dismissible.unwrap_or_else(|| Signal::derive(|| true));

    #[cfg(feature = "hydrate")]
    let return_focus = StoredValue::new(Option::<web_sys::HtmlElement>::None);

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        let is_open = open.get();

        if let Some(dialog) = dialog_ref.get() {
            use wasm_bindgen::JsCast;

            if is_open && !dialog.open() {
                let active = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.active_element())
                    .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok());
                return_focus.set_value(active);

                // `show_modal` promotes the element to the browser top layer;
                // unlike an `open` attribute it also makes the page inert and
                // applies the native focus-management algorithm.
                let _ = dialog.show_modal();
            } else if !is_open && dialog.open() {
                dialog.close();
            }

            if !is_open {
                if let Some(target) = return_focus.get_value() {
                    let _ = target.focus();
                }
                return_focus.set_value(None);
            }
        }
    });

    #[cfg(feature = "hydrate")]
    on_cleanup(move || {
        if let Some(dialog) = dialog_ref.get_untracked() {
            if dialog.open() {
                dialog.close();
            }
        }
        if let Some(target) = return_focus.get_value() {
            let _ = target.focus();
        }
    });

    view! {
        <dialog
            node_ref=dialog_ref
            class="ep-dialog ep-dialog--confirm"
            role="alertdialog"
            aria-modal="true"
            aria-label=label
            on:cancel=move |event: leptos::web_sys::Event| {
                event.prevent_default();
                if dismissible.get_untracked() {
                    open.set(false);
                }
            }
            on:click=move |event| {
                // Native backdrop clicks target the `<dialog>`. Coordinates
                // distinguish the backdrop from unoccupied interior space.
                if dismissible.get_untracked()
                    && click_is_on_backdrop(&event, dialog_ref)
                {
                    open.set(false);
                }
            }
        >
            {children()}
        </dialog>
    }
}

fn click_is_on_backdrop(event: &leptos::ev::MouseEvent, dialog_ref: NodeRef<html::Dialog>) -> bool {
    #[cfg(feature = "hydrate")]
    {
        if event.target() != event.current_target() {
            return false;
        }
        let Some(dialog) = dialog_ref.get_untracked() else {
            return false;
        };
        let rect = dialog.get_bounding_client_rect();
        let x = f64::from(event.client_x());
        let y = f64::from(event.client_y());
        x < rect.left() || x > rect.right() || y < rect.top() || y > rect.bottom()
    }
    #[cfg(not(feature = "hydrate"))]
    {
        let _ = (event, dialog_ref);
        false
    }
}
