//! Eigenpulse — Leptos `App` shell shared by the SSR binary and hydrate bundle.

pub mod admin;
mod app;
pub mod modules;
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
    #[cfg(debug_assertions)]
    console_error_panic_hook::set_once();

    leptos::mount::hydrate_body(App);

    // Chart hosts contain SSR fallback children that Tachys must claim before
    // the ECharts loader is allowed to replace the host contents. The root
    // marker handles a loader that executes after hydration; the event handles
    // the normal loader-first order without relying on timing or a timeout.
    if let Some(win) = web_sys::window() {
        if let Some(document) = win.document() {
            if let Some(root) = document.document_element() {
                let _ = root.set_attribute("data-ep-hydrated", "true");
            }
        }
        if let Ok(event) = web_sys::Event::new("eigenpulse:hydrated") {
            let _ = win.dispatch_event(&event);
        }
    }

    // Service workers are unavailable on plain LAN HTTP origins. Accessing
    // the container there can throw before `register()` even returns, which
    // used to surface as an uncaught hydrate error on the supported NAS setup.
    if let Some(win) = web_sys::window().filter(web_sys::Window::is_secure_context) {
        let options = web_sys::RegistrationOptions::new();
        options.set_scope("/");
        let _ = win
            .navigator()
            .service_worker()
            .register_with_options("/sw.js", &options);
    }
}
