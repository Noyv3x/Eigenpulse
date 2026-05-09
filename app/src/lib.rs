//! Eigenpulse — Leptos `App` shell shared by the SSR binary and hydrate bundle.

mod app;
mod views;

pub use app::{shell, App};

#[cfg(feature = "hydrate")]
#[cfg_attr(feature = "hydrate", wasm_bindgen::prelude::wasm_bindgen)]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);

    if let Some(win) = web_sys::window() {
        let options = web_sys::RegistrationOptions::new();
        options.set_scope("/");
        let _ = win
            .navigator()
            .service_worker()
            .register_with_options("/static/sw.js", &options);
    }
}
