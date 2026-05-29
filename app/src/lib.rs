//! Eigenpulse — Leptos `App` shell shared by the SSR binary and hydrate bundle.

pub mod admin;
mod app;
mod security;
mod views;

pub use app::App;
// `shell()` is SSR-only (renders the document; hydrate mounts `<App/>` instead).
#[cfg(feature = "ssr")]
pub use app::shell;

// Re-export the security-headers middleware so it is part of the crate's public
// surface. The binary (`main.rs`) layers it on the web router; the lib build
// would otherwise flag the SSR-only CSP machinery as dead code since the shared
// `app.rs` only consumes `security::theme_init_inline`.
#[cfg(feature = "ssr")]
pub use security::security_headers;

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
