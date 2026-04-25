//! WASM hydration entry point. Imported by the browser via the `pkg/` script
//! emitted by `cargo-leptos`. Re-exports the shared `App` from `eigenpulse`.

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use leptos::prelude::*;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(eigenpulse::App);

    if let Some(win) = web_sys::window() {
        let _ = win.navigator().service_worker().register("/static/sw.js");
    }
}
