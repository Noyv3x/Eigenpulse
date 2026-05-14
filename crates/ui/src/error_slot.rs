use leptos::prelude::*;
use leptos::server_fn::error::NoCustomError;
use leptos::server_fn::ServerFn;

/// Inline server-action error display for the form on a module page.
///
/// Renders the stable `<span class="error-slot">` wrapper that every
/// `<ActionForm>` submit row needs next to its button — the wrapper is
/// load-bearing for tachys hydration (see AGENTS.md: a moving placeholder
/// neighbour panics the text-node walker). When the bound action last
/// resolved to an `Err`, the localized message renders as a rose `tag`
/// inside it; otherwise the slot stays empty.
///
/// `style` threads optional inline layout (`flex:1`, `align-self:center`, …)
/// onto the wrapper for the call sites that need it; omitted otherwise.
///
/// `S::Error = NoCustomError` matches every `#[server]` fn in this workspace —
/// they all return the plain `ServerFnError`, which is what
/// `ep_i18n::server_fn_error_text` renders.
#[component]
pub fn ErrorSlot<S>(
    action: ServerAction<S>,
    #[prop(into, optional)] style: Option<String>,
) -> impl IntoView
where
    S: ServerFn<Error = NoCustomError> + Clone + Send + Sync + 'static,
    <S as ServerFn>::Output: Clone + Send + Sync + 'static,
{
    view! {
        <span class="error-slot" style=style>
            {move || {
                action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| view! { <span class="tag rose">{ep_i18n::server_fn_error_text(&e)}</span> })
            }}
        </span>
    }
}
