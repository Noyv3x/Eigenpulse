use ep_i18n::{t, use_locale};
// `Locale` (the type) is only referenced by the SSR-only `shell()`.
#[cfg(feature = "ssr")]
use ep_i18n::Locale;
use ep_ui::{provide_toast_stack, provide_unread_signal, ToastViewport};
use ep_ui::{provide_tweak_state, Sidebar, Theme, Topbar, TweakState};
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::path;

/// Browser-chrome / status-bar colour for each theme. These mirror the `--bg`
/// design token per theme in `assets/styles.css` (light `oklch(0.985 0.004 85)`,
/// dark `oklch(0.18 0.012 250)`) as the sRGB hex the `theme-color` meta needs.
const THEME_COLOR_LIGHT: &str = "#fbf9f5";
const THEME_COLOR_DARK: &str = "#0e1217";

#[cfg(feature = "hydrate")]
type NotificationEventHandler = wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MessageEvent)>;
#[cfg(feature = "hydrate")]
type NotificationEventState = Option<(web_sys::EventSource, NotificationEventHandler)>;

#[cfg(feature = "hydrate")]
thread_local! {
    static NOTIFICATION_EVENTS: std::cell::RefCell<NotificationEventState> =
        const { std::cell::RefCell::new(None) };
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let _tweaks = provide_tweak_state(TweakState::default());
    let _unread = provide_unread_signal(0);
    let _toast = provide_toast_stack();
    let locale = use_locale();
    let sidebar_user_name = RwSignal::new(t(locale, "app.sidebar.account_fallback").to_string());
    let sidebar_user_meta = RwSignal::new("OWNER".to_string());
    let sidebar_avatar = RwSignal::new("A".to_string());
    let sidebar_collapsed = RwSignal::new(false);
    let mobile_nav_open = RwSignal::new(false);
    #[cfg(feature = "hydrate")]
    subscribe_notification_events(_unread);
    let unread_count = Resource::new(
        || (),
        |_| async { crate::views::notifications::unread_notification_count().await },
    );
    Effect::new(move |_| {
        if let Some(Ok(count)) = unread_count.get() {
            if should_apply_initial_unread_count() {
                _unread.update(|n| *n = (*n).max(count));
            }
        }
    });
    let account_summary = Resource::new(
        || (),
        |_| async { crate::views::settings::load_settings_summary().await },
    );
    Effect::new(move |_| {
        if let Some(Ok(summary)) = account_summary.get() {
            let initial = summary
                .name
                .chars()
                .next()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "A".to_string());
            sidebar_avatar.set(initial);
            sidebar_user_meta.set(format!("{} · @{}", summary.role, summary.handle));
            sidebar_user_name.set(summary.name);
        }
    });

    view! {
        <Stylesheet id="ep" href="/static/styles.css"/>
        <Title text="Eigenpulse · Personal Life ERP"/>
        <Link rel="icon" type_="image/svg+xml" href="/static/favicon.svg"/>
        <Link rel="manifest" href="/static/manifest.webmanifest"/>
        // Reactive status-bar / browser-chrome colour. leptos_meta's `<Meta>`
        // exposes no `media` prop (0.7.8) and a raw `<meta media=…>` placed in
        // `<App/>` would land in `<body>`, not `<head>` — only the leptos_meta
        // components hoist via `<MetaTags/>`. So instead of two static
        // media-scoped tags we drive a single hoisted `<Meta>` off the live
        // theme signal: dark-theme users no longer get a light status-bar
        // flash once the theme-init script + hydrate settle the theme. The
        // hex values mirror the `--bg` token for each theme in `styles.css`.
        <Meta
            name="theme-color"
            content=move || if _tweaks.get().theme == Theme::Dark {
                THEME_COLOR_DARK
            } else {
                THEME_COLOR_LIGHT
            }
        />
        <Meta name="mobile-web-app-capable" content="yes"/>
        // Kept alongside the standard `mobile-web-app-capable` above for older
        // iOS Safari, which still only honours the `apple-` prefixed form.
        <Meta name="apple-mobile-web-app-capable" content="yes"/>
        <Meta name="apple-mobile-web-app-title" content="Eigenpulse"/>
        <Meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover"/>
        // The anti-FOUC theme-init IIFE is inlined into the SSR `<head>` by
        // `shell()` (see `crate::security::theme_init_inline`), so it runs
        // before first paint with no render-blocking network fetch. No
        // `<Script src>` here — a duplicate would re-run the same idempotent
        // IIFE and require a redundant request.

        <Router>
            <div class=move || {
                match (sidebar_collapsed.get(), mobile_nav_open.get()) {
                    (true, true) => "app collapsed mobile-open",
                    (true, false) => "app collapsed",
                    (false, true) => "app mobile-open",
                    (false, false) => "app",
                }
            }>
                <Sidebar
                    user_name=sidebar_user_name
                    user_meta=sidebar_user_meta
                    avatar_text=sidebar_avatar
                />
                <Topbar sidebar_collapsed mobile_nav_open/>
                <main class="main">
                    <ToastViewport/>
                    <Routes fallback=NotFound>
                        <Route path=path!("")             view=crate::views::dashboard::DashboardView/>
                        <Route path=path!("today")        view=crate::views::today::TodayView/>
                        <Route path=path!("finance")      view=ep_finance::FinanceView/>
                        <Route path=path!("fitness")      view=ep_fitness::FitnessView/>
                        <Route path=path!("learning")     view=ep_learning::LearningView/>
                        <Route path=path!("modules")      view=ep_marketplace::MarketView/>
                        <Route path=path!("reports")      view=crate::views::reports::ReportsView/>
                        <Route path=path!("notifications") view=crate::views::notifications::NotificationsView/>
                        <Route path=path!("settings")     view=crate::views::settings::SettingsIndex/>
                        <Route path=path!("settings/notifications") view=crate::views::settings::notifications::NotificationChannelsView/>
                        <Route path=path!("settings/security")      view=crate::views::settings::security::PatView/>
                        <Route path=path!("status")                 view=crate::views::status::StatusView/>
                    </Routes>
                </main>
            </div>
        </Router>
    }
}

#[cfg(any(feature = "hydrate", test))]
fn is_notifications_path(path: &str) -> bool {
    path == "/notifications" || path.starts_with("/notifications/")
}

fn should_apply_initial_unread_count() -> bool {
    #[cfg(feature = "hydrate")]
    {
        web_sys::window()
            .and_then(|w| w.location().pathname().ok())
            .is_none_or(|path| !is_notifications_path(&path))
    }
    #[cfg(not(feature = "hydrate"))]
    {
        true
    }
}

#[cfg(feature = "hydrate")]
fn subscribe_notification_events(unread: RwSignal<u32>) {
    use wasm_bindgen::{closure::Closure, JsCast};

    let Ok(events) = web_sys::EventSource::new("/events/notifications") else {
        return;
    };
    let on_message =
        Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            if event.data().as_string().is_some() {
                unread.update(|n| *n = n.saturating_add(1));
            }
        });
    events.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    NOTIFICATION_EVENTS.with(|slot| {
        let mut slot = slot.borrow_mut();
        if let Some((events, _)) = slot.take() {
            events.close();
        }
        *slot = Some((events, on_message));
    });
}

#[component]
fn NotFound() -> impl IntoView {
    let locale = ep_i18n::use_locale();
    view! {
        <div class="view">
            <div class="card"><div class="card-body">
                <h2>"404"</h2>
                <p class="muted">{t(locale, "app.not_found.message")} " · "<A href="/">{t(locale, "app.not_found.back_home")}</A></p>
            </div></div>
        </div>
    }
}

/// SSR document shell. Renders `<html><head/><body><App/></body></html>`.
///
/// `<html lang>` is set from the `Locale` provided in leptos context (by
/// the per-request `provide_state` callback in `main.rs`); falls back to
/// the default locale when no context is present (e.g. during hydration's
/// initial type-check pass).
///
/// SSR-only: only `main.rs` renders the document shell; the hydrate entry
/// (`lib.rs::hydrate`) mounts `<App/>` into the existing DOM and never calls
/// this. Gating it keeps the nonce/CSP machinery (and `leptos/nonce` →
/// `getrandom`) out of the wasm bundle.
#[cfg(feature = "ssr")]
pub fn shell(options: leptos::config::LeptosOptions) -> impl IntoView {
    use leptos_meta::MetaTags;
    let lang = use_context::<Locale>().unwrap_or_default().as_html_lang();
    // The per-request nonce `leptos_axum` provided (`provide_nonce()`), shared
    // by both the CSP `<meta>` and every inline script below so they always
    // match. See `crate::security` for why the CSP is a meta tag, not a header.
    let nonce = leptos::nonce::use_nonce();
    view! {
        <!DOCTYPE html>
        <html lang=lang>
            <head>
                <meta charset="utf-8"/>
                // CSP first, so it governs every inline script that follows.
                <meta
                    http-equiv="Content-Security-Policy"
                    content=crate::security::csp_content(nonce.clone())
                />
                <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover"/>
                // Anti-FOUC: inline the theme-init IIFE so `data-theme` /
                // `data-density` are set on `<html>` before first paint with no
                // render-blocking network fetch. Carries the per-request nonce
                // so it passes the CSP above (the `<HydrationScripts>` bootstrap
                // carries the same nonce, generated once by leptos_axum).
                <script nonce=nonce inner_html=crate::security::theme_init_inline()></script>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn detects_notifications_paths() {
        assert!(super::is_notifications_path("/notifications"));
        assert!(super::is_notifications_path("/notifications/archive"));
        assert!(!super::is_notifications_path("/notification-settings"));
        assert!(!super::is_notifications_path("/"));
    }
}
