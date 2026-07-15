use leptos::prelude::*;

/// Standard "load failed" card for a `<Suspense>`/`Resource` `Err(_)` arm.
///
/// Every module view duplicated
/// `<div class="card"><div class="card-body">{load_failed} " · " {detail}</div></div>`
/// in its resource error branch. This generalizes that card. Pass the
/// already-rendered error text (e.g. `ep_i18n::server_fn_error_text(&e)`) as
/// `detail`; the localized "加载失败 / Load failed" prefix is supplied here.
///
/// Wasm-safe: no timestamps, no fs/process; the locale is read from context
/// via `ep_i18n::use_locale`, same as every other component in this crate.
#[component]
pub fn LoadError(
    /// Optional error detail appended after a " · " separator. Omit for a
    /// bare "load failed" card.
    #[prop(into, optional)]
    detail: Option<String>,
) -> impl IntoView {
    let locale = ep_i18n::use_locale();
    let prefix = ep_i18n::t(locale, "app.common.load_failed");
    view! {
        <div class="card">
            <div class="card-body">
                {prefix}
                {detail.map(|d| view! { " · " {d} })}
            </div>
        </div>
    }
}
