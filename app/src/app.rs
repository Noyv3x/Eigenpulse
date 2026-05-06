use ep_i18n::{t, Locale};
use ep_ui::notifications::provide_unread_signal;
use ep_ui::{provide_tweak_state, Sidebar, Topbar, TweakState};
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::{Route, Router, Routes, A};
use leptos_router::path;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    let _tweaks = provide_tweak_state(TweakState::default());
    let _unread = provide_unread_signal(0);

    view! {
        <Stylesheet id="ep" href="/static/styles.css"/>
        <Title text="Eigenpulse · Personal Life ERP"/>
        <Link rel="icon" type_="image/svg+xml" href="/static/favicon.svg"/>
        <Link rel="manifest" href="/static/manifest.webmanifest"/>
        <Link rel="apple-touch-icon" sizes="180x180" href="/static/icons/apple-touch-180.png"/>
        <Meta name="theme-color" content="#fbf9f5"/>
        <Meta name="apple-mobile-web-app-capable" content="yes"/>
        <Meta name="apple-mobile-web-app-title" content="Eigenpulse"/>
        <Meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover"/>
        <Script src="/static/theme-init.js"/>

        <Router>
            <div class="app">
                <Sidebar/>
                <Topbar/>
                <main class="main">
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
                    </Routes>
                </main>
            </div>
        </Router>
    }
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
pub fn shell(options: leptos::config::LeptosOptions) -> impl IntoView {
    use leptos_meta::MetaTags;
    let lang = use_context::<Locale>().unwrap_or_default().as_html_lang();
    view! {
        <!DOCTYPE html>
        <html lang=lang>
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover"/>
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
