#![cfg(feature = "hydrate")]

use crate::cookie::build_set_cookie;
use crate::locale::Locale;
use wasm_bindgen::JsCast;

/// Write the `ep_locale` cookie and reload — the next request hits SSR
/// with the new cookie, which renders the page in the chosen language.
pub fn switch_locale_via_reload(target: Locale) {
    let Some(win) = web_sys::window() else { return };
    if let Some(doc) = win.document() {
        if let Ok(html_doc) = doc.dyn_into::<web_sys::HtmlDocument>() {
            let _ = html_doc.set_cookie(&build_set_cookie(target));
        }
    }
    let _ = win.location().reload();
}
