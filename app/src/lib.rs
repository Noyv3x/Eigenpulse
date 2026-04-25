//! Eigenpulse — Leptos `App` shell shared by SSR (server binary) and Hydrate (`app-client`).

pub mod app;
pub mod views;

pub use app::{App, shell};

#[cfg(feature = "hydrate")]
#[cfg_attr(feature = "hydrate", wasm_bindgen::prelude::wasm_bindgen)]
pub fn hydrate() {
    use leptos::prelude::*;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);

    if let Some(win) = web_sys::window() {
        let _ = win.navigator().service_worker().register("/static/sw.js");
    }
}
