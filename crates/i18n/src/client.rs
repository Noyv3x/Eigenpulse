#![cfg(feature = "hydrate")]

use crate::cookie::build_set_cookie;
use crate::locale::Locale;
use wasm_bindgen::JsCast;

/// Persist the preference, write the `ep_locale` cookie, then reload — the
/// next request hits SSR with the new cookie and renders in the chosen language.
pub fn switch_locale_via_reload(target: Locale) {
    leptos::task::spawn_local(async move {
        let _ = crate::server_fns::set_user_locale(target.as_code().to_string()).await;
        write_locale_cookie_and_reload(target);
    });
}

fn write_locale_cookie_and_reload(target: Locale) {
    let Some(win) = web_sys::window() else { return };
    if let Some(doc) = win.document() {
        if let Ok(html_doc) = doc.dyn_into::<web_sys::HtmlDocument>() {
            let _ = html_doc.set_cookie(&build_set_cookie(target));
        }
    }
    let _ = win.location().reload();
}
