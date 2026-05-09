pub mod notifications;
pub mod security;

use ep_i18n::{server_fn_error_text, t, use_locale};
use ep_ui::{use_tweaks, Density, TweakState};
use ep_ui::{Card, PageHead, StatRow};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use leptos_router::components::A;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use ep_core::server_err;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSummary {
    pub handle: String,
    pub name: String,
    pub role: String,
    pub created_at: i64,
    pub data_rows: i64,
    pub database_location: String,
}

#[server(
    LoadSettingsSummary,
    "/api/_internal/cfg",
    "Url",
    "load_settings_summary"
)]
pub async fn load_settings_summary() -> Result<SettingsSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let user: (String, String, String, i64) =
            sqlx::query_as("SELECT handle, name, role, created_at FROM app_user WHERE id = 1")
                .fetch_one(&state.db)
                .await
                .map_err(server_err)?;
        let data_rows = count_user_data_rows(&state.db).await.map_err(server_err)?;
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://data/eigenpulse.db?mode=rwc".into());

        Ok(SettingsSummary {
            handle: user.0,
            name: user.1,
            role: user.2,
            created_at: user.3,
            data_rows,
            database_location: database_location_label(&database_url),
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
fn database_location_label(database_url: &str) -> String {
    let without_query = database_url
        .split_once('?')
        .map_or(database_url, |(path, _)| path);
    if without_query.starts_with("sqlite://") {
        "SQLite · local file".to_string()
    } else {
        "configured database".to_string()
    }
}

#[cfg(feature = "ssr")]
async fn count_user_data_rows(pool: &sqlx::SqlitePool) -> sqlx::Result<i64> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name
           FROM sqlite_schema
          WHERE type = 'table'
            AND name NOT LIKE 'sqlite_%'
            AND name NOT IN ('_sqlx_migrations', '_ep_module_migration', 'seq', 'session', 'app_user')",
    )
    .fetch_all(pool)
    .await?;

    let mut total = 0;
    for table in tables {
        // Table names come from sqlite_schema, not user input. The quoted
        // identifier keeps this robust if future module tables contain odd
        // characters.
        let quoted = table.replace('"', "\"\"");
        let sql = format!("SELECT COUNT(*) FROM \"{quoted}\"");
        let rows: i64 = sqlx::query_scalar(&sql).fetch_one(pool).await?;
        total += rows;
    }
    Ok(total)
}

#[component]
pub fn SettingsIndex() -> impl IntoView {
    let tweaks = use_tweaks();
    let locale = use_locale();
    let summary = Resource::new(|| (), |_| async { load_settings_summary().await });
    view! {
        <div class="view">
            <PageHead
                code="CFG-01"
                module=t(locale, "app.settings.index.page.module")
                title=t(locale, "app.settings.index.page.title")
                title_cn=t(locale, "app.settings.index.page.title_cn")
            />
            <div class="grid-2">
                <Card title=t(locale, "app.settings.index.account_card.title") code="CFG-ACC">
                    <Suspense fallback=move || view! {
                        <div class="placeholder-img" style="min-height:96px">{t(locale, "app.common.loading")}</div>
                    }>
                        {move || summary.get().map(|res| match res {
                            Err(e) => view! { <p>{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</p> }.into_any(),
                            Ok(s) => {
                                let user = format!("{} · @{}", s.name, s.handle);
                                view! {
                                    <div class="vstack" style="gap:0">
                                        <StatRow label=t(locale, "app.settings.index.user") value=user/>
                                        <StatRow label=t(locale, "app.settings.index.role") value=s.role/>
                                        <StatRow label=t(locale, "app.settings.index.data_count") value=ep_core::fmt_int(s.data_rows as f64)/>
                                    </div>
                                }.into_any()
                            }
                        })}
                    </Suspense>
                </Card>
                <Card title=t(locale, "app.settings.index.data_card.title") code="CFG-DATA">
                    <Suspense fallback=move || view! {
                        <div class="placeholder-img" style="min-height:96px">{t(locale, "app.common.loading")}</div>
                    }>
                        {move || summary.get().map(|res| match res {
                            Err(e) => view! { <p>{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</p> }.into_any(),
                            Ok(s) => view! {
                                <div class="vstack" style="gap:0">
                                    <StatRow label=t(locale, "app.settings.index.data_card.storage") value=s.database_location/>
                                    <StatRow label=t(locale, "app.settings.index.data_card.backup") value=t(locale, "app.settings.index.unconfigured").to_string()/>
                                    <StatRow label=t(locale, "app.settings.index.data_card.sync") value=t(locale, "app.settings.index.data_card.local").to_string()/>
                                </div>
                            }.into_any()
                        })}
                    </Suspense>
                </Card>
                <Card title=t(locale, "app.settings.index.notify_card.title") code="CFG-NOT" sub="SMTP / Bark / Telegram / Discord">
                    <p class="muted">{t(locale, "app.settings.index.notify_card.hint_a")} " " <A href="/settings/notifications">{t(locale, "app.settings.index.notify_card.link")}</A> " " {t(locale, "app.settings.index.notify_card.hint_b")}</p>
                </Card>
                <Card title=t(locale, "app.settings.index.api_card.title") code="CFG-SEC" sub=t(locale, "app.settings.index.api_card.sub")>
                    <p class="muted">{t(locale, "app.settings.index.api_card.hint_a")} " " <A href="/settings/security">{t(locale, "app.settings.index.api_card.link")}</A> " " {t(locale, "app.settings.index.api_card.hint_b")}</p>
                </Card>
                <Card title=t(locale, "app.settings.index.ui_card.title") code="CFG-UI" sub=t(locale, "app.settings.index.ui_card.sub")>
                    <div class="tweak-row">
                        <label>{t(locale, "app.settings.index.density_label")}</label>
                        <div class="seg">
                            <button
                                class=move || if tweaks.get().density == Density::Comfortable { "on" } else { "" }
                                on:click=move |_| tweaks.update(|v: &mut TweakState| v.density = Density::Comfortable)
                            >{t(locale, "app.settings.index.density_comfortable")}</button>
                            <button
                                class=move || if tweaks.get().density == Density::Compact { "on" } else { "" }
                                on:click=move |_| tweaks.update(|v: &mut TweakState| v.density = Density::Compact)
                            >{t(locale, "app.settings.index.density_compact")}</button>
                        </div>
                    </div>
                </Card>
            </div>
        </div>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    #[test]
    fn database_location_label_hides_filesystem_paths() {
        assert_eq!(
            super::database_location_label("sqlite://data/eigenpulse.db?mode=rwc"),
            "SQLite · local file"
        );
        assert_eq!(
            super::database_location_label("sqlite:///data/eigenpulse.db?mode=rwc"),
            "SQLite · local file"
        );
        assert_eq!(
            super::database_location_label("postgres://user:pass@example/db"),
            "configured database"
        );
    }
}
