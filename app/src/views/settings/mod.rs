#![cfg_attr(
    not(feature = "ssr"),
    allow(
        unused_variables,
        reason = "Leptos server-function parameters are serialized by client builds while their implementations are SSR-only"
    )
)]

pub mod notifications;
pub mod security;

use crate::admin::{admin_status, AdminStatus};
use ep_core::IconKind;
use ep_i18n::{server_fn_error_text, t, use_locale};
use ep_ui::{use_tweaks, Card, Density, Icon, PageHead, StatRow, TweakState};
use leptos::prelude::*;
use leptos_router::components::A;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use ep_core::server_err;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSummary {
    pub handle: String,
    pub name: String,
    pub role: String,
    pub database_location: String,
    pub timezone: String,
    pub timezone_offset: String,
    pub timezone_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimezonePreference {
    pub timezone: String,
    pub timezone_offset: String,
    pub timezone_mode: String,
}

/// Minimal result returned to the hydrated shell after browser timezone
/// detection. `changed` is the reload signal; manual mode always returns it
/// as false and never changes either persistence or the active snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserTimezoneSync {
    pub timezone: String,
    pub timezone_mode: String,
    pub changed: bool,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimezoneMode {
    Auto,
    Manual,
}

#[cfg(feature = "ssr")]
impl TimezoneMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }

    fn parse(value: &str) -> Result<Self, ServerFnError> {
        match value {
            "auto" => Ok(Self::Auto),
            "manual" => Ok(Self::Manual),
            _ => Err(server_err("stored timezone mode is invalid")),
        }
    }
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
        let user: (String, String, String, String, String) = sqlx::query_as(
            "SELECT handle, name, role, timezone, timezone_mode
               FROM app_user WHERE id = 1",
        )
        .fetch_one(&state.db)
        .await
        .map_err(server_err)?;
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://data/eigenpulse.db?mode=rwc".into());
        let timezone = ep_core::AppTimezone::parse(&user.3)
            .ok_or_else(|| server_err("stored timezone is invalid"))?;
        let timezone_mode = TimezoneMode::parse(&user.4)?;
        Ok(SettingsSummary {
            handle: user.0,
            name: user.1,
            role: user.2,
            database_location: database_location_label(&database_url),
            timezone: timezone.name().to_string(),
            timezone_offset: timezone.utc_offset_label(ep_core::unix_now()),
            timezone_mode: timezone_mode.as_str().to_string(),
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    SaveDisplayTimezone,
    "/api/_internal/cfg",
    "Url",
    "save_display_timezone"
)]
pub async fn save_display_timezone(timezone: String) -> Result<TimezonePreference, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let timezone = ep_core::AppTimezone::parse(&timezone)
            .ok_or_else(|| ep_i18n::err("app.settings.index.err_timezone_invalid"))?;
        persist_timezone(&state.db, &state.timezone, timezone, TimezoneMode::Manual).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    EnableAutomaticTimezone,
    "/api/_internal/cfg",
    "Url",
    "enable_automatic_timezone"
)]
pub async fn enable_automatic_timezone(
    timezone: String,
) -> Result<TimezonePreference, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let timezone = ep_core::AppTimezone::parse(&timezone)
            .ok_or_else(|| ep_i18n::err("app.settings.index.err_timezone_invalid"))?;
        persist_timezone(&state.db, &state.timezone, timezone, TimezoneMode::Auto).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    SyncBrowserTimezone,
    "/api/_internal/cfg",
    "Url",
    "sync_browser_timezone"
)]
pub async fn sync_browser_timezone(timezone: String) -> Result<BrowserTimezoneSync, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let timezone = ep_core::AppTimezone::parse(&timezone)
            .ok_or_else(|| ep_i18n::err("app.settings.index.err_timezone_invalid"))?;
        sync_browser_timezone_inner(&state.db, &state.timezone, timezone).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
async fn persist_timezone(
    pool: &sqlx::SqlitePool,
    store: &ep_core::TimezoneStore,
    timezone: ep_core::AppTimezone,
    mode: TimezoneMode,
) -> Result<TimezonePreference, ServerFnError> {
    // Keep the commit and publication in one serialized critical section so
    // two simultaneous browser submissions cannot leave memory older than DB.
    let _update_guard = store.begin_update().await;
    let mut tx = pool.begin().await.map_err(server_err)?;
    let result = sqlx::query("UPDATE app_user SET timezone = ?1, timezone_mode = ?2 WHERE id = 1")
        .bind(timezone.name())
        .bind(mode.as_str())
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    if result.rows_affected() != 1 {
        return Err(server_err("OWNER row missing while saving timezone"));
    }
    tx.commit().await.map_err(server_err)?;
    store.replace(timezone);

    Ok(TimezonePreference {
        timezone: timezone.name().to_string(),
        timezone_offset: timezone.utc_offset_label(ep_core::unix_now()),
        timezone_mode: mode.as_str().to_string(),
    })
}

#[cfg(feature = "ssr")]
async fn sync_browser_timezone_inner(
    pool: &sqlx::SqlitePool,
    store: &ep_core::TimezoneStore,
    browser_timezone: ep_core::AppTimezone,
) -> Result<BrowserTimezoneSync, ServerFnError> {
    // Use the same serialization boundary as explicit settings writes. This
    // makes the mode decision, optional DB update, and snapshot publication a
    // single ordered operation even when multiple devices hydrate together.
    let _update_guard = store.begin_update().await;
    let mut tx = pool.begin().await.map_err(server_err)?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT timezone, timezone_mode FROM app_user WHERE id = 1")
            .fetch_optional(&mut *tx)
            .await
            .map_err(server_err)?;
    let (stored_name, stored_mode) =
        row.ok_or_else(|| server_err("OWNER row missing while syncing timezone"))?;
    let stored_timezone = ep_core::AppTimezone::parse(&stored_name)
        .ok_or_else(|| server_err("stored timezone is invalid"))?;
    let mode = TimezoneMode::parse(&stored_mode)?;

    if mode == TimezoneMode::Manual {
        tx.commit().await.map_err(server_err)?;
        return Ok(BrowserTimezoneSync {
            timezone: stored_timezone.name().to_string(),
            timezone_mode: mode.as_str().to_string(),
            changed: false,
        });
    }

    let database_changed = stored_timezone.name() != browser_timezone.name();
    let snapshot_changed = store.snapshot() != browser_timezone;
    if database_changed {
        let result = sqlx::query(
            "UPDATE app_user SET timezone = ?1
              WHERE id = 1 AND timezone_mode = 'auto'",
        )
        .bind(browser_timezone.name())
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
        if result.rows_affected() != 1 {
            return Err(server_err("automatic timezone mode changed while syncing"));
        }
    }
    tx.commit().await.map_err(server_err)?;
    if database_changed || snapshot_changed {
        store.replace(browser_timezone);
    }

    Ok(BrowserTimezoneSync {
        timezone: browser_timezone.name().to_string(),
        timezone_mode: mode.as_str().to_string(),
        changed: database_changed || snapshot_changed,
    })
}

/// Minimal identity for the app shell. It intentionally contains no password,
/// token, or module data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidebarIdentity {
    pub handle: String,
    pub name: String,
    pub role: String,
}

#[server(
    LoadSidebarIdentity,
    "/api/_internal/cfg",
    "Url",
    "load_sidebar_identity"
)]
pub async fn load_sidebar_identity() -> Result<SidebarIdentity, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let user: (String, String, String) =
            sqlx::query_as("SELECT handle, name, role FROM app_user WHERE id = 1")
                .fetch_one(&state.db)
                .await
                .map_err(server_err)?;
        Ok(SidebarIdentity {
            handle: user.0,
            name: user.1,
            role: user.2,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
fn database_location_label(database_url: &str) -> String {
    if database_url
        .split_once('?')
        .map_or(database_url, |(path, _)| path)
        .starts_with("sqlite:")
    {
        "SQLite · local file".to_string()
    } else {
        "configured database".to_string()
    }
}

fn render_backup_row(status: Resource<Result<AdminStatus, ServerFnError>>) -> impl IntoView {
    let locale = use_locale();
    view! {
        <Suspense fallback=move || view! {
            <StatRow label=t(locale, "app.settings.index.data_card.backup") value=t(locale, "app.common.loading").to_string()/>
        }>
            {move || {
                let value = match status.get() {
                    Some(Ok(item)) if item.last_backup_exists => item
                        .last_backup_bytes
                        .map(crate::views::status::fmt_bytes)
                        .unwrap_or_else(|| t(locale, "app.settings.index.unconfigured").to_string()),
                    _ => t(locale, "app.settings.index.unconfigured").to_string(),
                };
                view! { <StatRow label=t(locale, "app.settings.index.data_card.backup") value=value/> }
            }}
        </Suspense>
    }
}

#[component]
pub fn SettingsIndex() -> impl IntoView {
    let tweaks = use_tweaks();
    let locale = use_locale();
    let save_timezone = ServerAction::<SaveDisplayTimezone>::new();
    let enable_automatic_timezone = ServerAction::<EnableAutomaticTimezone>::new();
    let browser_timezone = RwSignal::new(None::<String>);
    #[cfg(feature = "hydrate")]
    Effect::new(move |_| browser_timezone.set(crate::app::browser_timezone_name()));
    let summary = Resource::new(
        move || {
            (
                save_timezone.version().get(),
                enable_automatic_timezone.version().get(),
            )
        },
        |_| async { load_settings_summary().await },
    );
    let status = Resource::new(|| (), |_| async { admin_status().await });

    view! {
        <div class="view">
            <PageHead
                module=t(locale, "app.settings.index.page.module")
                title=t(locale, "app.settings.index.page.title")
                title_cn=t(locale, "app.settings.index.page.title_cn")
            />
            <div class="grid-2">
                <Card title=t(locale, "app.settings.index.account_card.title")>
                    <Suspense fallback=move || view! {
                        <div class="vstack" style="gap:8px;padding:6px 0">
                            <span class="skeleton-line" style="height:14px;width:60%;display:block"></span>
                            <span class="skeleton-line" style="height:14px;width:45%;display:block"></span>
                        </div>
                    }>
                        {move || summary.get().map(|result| match result {
                            Err(error) => view! {
                                <p>{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&error)}</p>
                            }.into_any(),
                            Ok(item) => view! {
                                <div class="vstack" style="gap:0">
                                    <StatRow label=t(locale, "app.settings.index.user") value=format!("{} · @{}", item.name, item.handle)/>
                                    <StatRow label=t(locale, "app.settings.index.role") value=item.role/>
                                </div>
                            }.into_any(),
                        })}
                    </Suspense>
                </Card>

                <Card
                    title=t(locale, "app.settings.index.regional_card.title")
                    sub=t(locale, "app.settings.index.regional_card.sub")
                >
                    <Suspense fallback=move || view! {
                        <div class="vstack" style="gap:10px">
                            <span class="skeleton-line" style="height:14px;width:55%;display:block"></span>
                            <span class="skeleton-line" style="height:38px;width:100%;display:block"></span>
                        </div>
                    }>
                        {move || summary.get().map(|result| match result {
                            Err(error) => view! {
                                <p>{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&error)}</p>
                            }.into_any(),
                            Ok(item) => {
                                let effective = format!("{} · {}", item.timezone, item.timezone_offset);
                                let mode_label = if item.timezone_mode == "auto" {
                                    t(locale, "app.settings.index.timezone_mode_auto")
                                } else {
                                    t(locale, "app.settings.index.timezone_mode_manual")
                                };
                                view! {
                                    <div class="vstack" style="gap:14px">
                                        <StatRow
                                            label=t(locale, "app.settings.index.timezone_current")
                                            value=effective
                                        />
                                        <div
                                            data-testid="timezone-mode"
                                            data-timezone-mode=item.timezone_mode
                                        >
                                            <StatRow
                                                label=t(locale, "app.settings.index.timezone_mode")
                                                value=mode_label
                                            />
                                        </div>
                                        <div
                                            class="vstack"
                                            style="gap:10px;padding:12px;border:1px solid var(--border);border-radius:10px"
                                        >
                                            <div class="vstack" style="gap:4px">
                                                <strong>{t(locale, "app.settings.index.timezone_auto_title")}</strong>
                                                <span class="muted" style="font-size:12px">
                                                    {t(locale, "app.settings.index.timezone_auto_hint")}
                                                </span>
                                            </div>
                                            <div class="hstack" style="gap:8px;align-items:center;flex-wrap:wrap">
                                                <span class="muted">{t(locale, "app.settings.index.timezone_browser")}</span>
                                                <span
                                                    class="tag blue mono"
                                                    data-testid="browser-timezone"
                                                >
                                                    {move || browser_timezone.get().unwrap_or_else(|| {
                                                        t(locale, "app.settings.index.timezone_detecting").to_string()
                                                    })}
                                                </span>
                                            </div>
                                            <ActionForm
                                                action=enable_automatic_timezone
                                                attr:class="hstack"
                                                attr:style="gap:8px;align-items:center;flex-wrap:wrap"
                                                attr:data-testid="timezone-auto-form"
                                            >
                                                <input
                                                    type="hidden"
                                                    name="timezone"
                                                    prop:value=move || browser_timezone.get().unwrap_or_default()
                                                />
                                                <button
                                                    class="btn"
                                                    type="submit"
                                                    data-testid="timezone-auto-button"
                                                    disabled=move || {
                                                        browser_timezone.get().is_none()
                                                            || enable_automatic_timezone.pending().get()
                                                    }
                                                    aria-busy=move || enable_automatic_timezone.pending().get().to_string()
                                                >
                                                    <Icon kind=IconKind::Arrow size=14/>
                                                    {t(locale, "app.settings.index.timezone_auto_enable")}
                                                </button>
                                                <span class="error-slot" aria-live="polite">
                                                    {move || match enable_automatic_timezone.value().get() {
                                                        Some(Ok(_)) => view! {
                                                            <span class="tag green" role="status">
                                                                {t(locale, "app.settings.index.timezone_auto_saved")}
                                                            </span>
                                                        }.into_any(),
                                                        Some(Err(error)) => view! {
                                                            <span class="tag rose" role="alert">
                                                                {server_fn_error_text(&error)}
                                                            </span>
                                                        }.into_any(),
                                                        None => ().into_any(),
                                                    }}
                                                </span>
                                            </ActionForm>
                                        </div>
                                        <ActionForm
                                            action=save_timezone
                                            attr:class="vstack"
                                            attr:style="gap:10px"
                                            attr:data-testid="timezone-form"
                                        >
                                            <label class="vstack" style="gap:4px">
                                                <span class="ep-field-label">{t(locale, "app.settings.index.timezone_label")}</span>
                                                <input
                                                    name="timezone"
                                                    list="ep-timezone-options"
                                                    required
                                                    maxlength="64"
                                                    autocomplete="off"
                                                    class="ep-input mono"
                                                    data-testid="timezone-input"
                                                    placeholder=t(locale, "app.settings.index.timezone_placeholder")
                                                    value=item.timezone
                                                />
                                                <datalist id="ep-timezone-options">
                                                    {COMMON_TIMEZONES.into_iter().map(|name| view! { <option value=name/> }).collect_view()}
                                                </datalist>
                                                <span class="muted" style="font-size:12px">
                                                    {t(locale, "app.settings.index.timezone_hint")}
                                                </span>
                                            </label>
                                            <div class="hstack" style="gap:8px;align-items:center;flex-wrap:wrap">
                                                <button
                                                    class="btn primary"
                                                    type="submit"
                                                    disabled=move || save_timezone.pending().get()
                                                    aria-busy=move || save_timezone.pending().get().to_string()
                                                >
                                                    <Icon kind=IconKind::Check size=14/>
                                                    {t(locale, "app.settings.index.timezone_save")}
                                                </button>
                                                <span class="error-slot" aria-live="polite">
                                                    {move || match save_timezone.value().get() {
                                                        Some(Ok(_)) => view! {
                                                            <span class="tag green" role="status">
                                                                {t(locale, "app.settings.index.timezone_saved")}
                                                            </span>
                                                        }.into_any(),
                                                        Some(Err(error)) => view! {
                                                            <span class="tag rose" role="alert">
                                                                {server_fn_error_text(&error)}
                                                            </span>
                                                        }.into_any(),
                                                        None => ().into_any(),
                                                    }}
                                                </span>
                                            </div>
                                        </ActionForm>
                                    </div>
                                }.into_any()
                            },
                        })}
                    </Suspense>
                </Card>

                <Card title=t(locale, "app.settings.index.data_card.title")>
                    <Suspense fallback=move || view! {
                        <div class="vstack" style="gap:8px;padding:6px 0">
                            <span class="skeleton-line" style="height:14px;width:55%;display:block"></span>
                            <span class="skeleton-line" style="height:14px;width:40%;display:block"></span>
                        </div>
                    }>
                        {move || summary.get().map(|result| match result {
                            Err(error) => view! {
                                <p>{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&error)}</p>
                            }.into_any(),
                            Ok(item) => view! {
                                <div class="vstack" style="gap:0">
                                    <StatRow label=t(locale, "app.settings.index.data_card.storage") value=item.database_location/>
                                    {render_backup_row(status)}
                                    <StatRow label=t(locale, "app.settings.index.data_card.sync") value=t(locale, "app.settings.index.data_card.local").to_string()/>
                                </div>
                            }.into_any(),
                        })}
                    </Suspense>
                    <div class="hstack" style="gap:10px;align-items:center;flex-wrap:wrap;margin-top:14px">
                        <A href="/status" attr:class="btn">
                            <Icon kind=IconKind::Check size=14/>
                            {t(locale, "app.settings.index.data_card.open_status")}
                        </A>
                        <span class="new-token-slot">
                            {move || status.get().and_then(Result::ok).filter(|item| item.last_backup_exists).map(|_| view! {
                                <a class="btn" href="/api/_internal/admin/backup/latest" download="eigenpulse.epbackup">
                                    <Icon kind=IconKind::Export size=14/>
                                    {t(locale, "app.settings.index.data_card.download")}
                                </a>
                            })}
                        </span>
                    </div>
                </Card>

                <Card title=t(locale, "app.settings.index.notify_card.title") sub="SMTP / Bark / Telegram / Discord">
                    <p class="muted">
                        {t(locale, "app.settings.index.notify_card.hint_a")} " "
                        <A href="/settings/notifications">{t(locale, "app.settings.index.notify_card.link")}</A>
                        " " {t(locale, "app.settings.index.notify_card.hint_b")}
                    </p>
                </Card>

                <Card title=t(locale, "app.settings.index.api_card.title") sub=t(locale, "app.settings.index.api_card.sub")>
                    <p class="muted">
                        {t(locale, "app.settings.index.api_card.hint_a")} " "
                        <A href="/settings/security">{t(locale, "app.settings.index.api_card.link")}</A>
                        " " {t(locale, "app.settings.index.api_card.hint_b")}
                    </p>
                </Card>

                <Card title=t(locale, "app.settings.index.ui_card.title") sub=t(locale, "app.settings.index.ui_card.sub")>
                    <div class="tweak-row">
                        <label>{t(locale, "app.settings.index.density_label")}</label>
                        <div class="seg">
                            <button
                                type="button"
                                class=move || if tweaks.get().density == Density::Comfortable { "on" } else { "" }
                                on:click=move |_| tweaks.update(|value: &mut TweakState| value.density = Density::Comfortable)
                            >{t(locale, "app.settings.index.density_comfortable")}</button>
                            <button
                                type="button"
                                class=move || if tweaks.get().density == Density::Compact { "on" } else { "" }
                                on:click=move |_| tweaks.update(|value: &mut TweakState| value.density = Density::Compact)
                            >{t(locale, "app.settings.index.density_compact")}</button>
                        </div>
                    </div>
                </Card>
            </div>
        </div>
    }
}

const COMMON_TIMEZONES: [&str; 23] = [
    "UTC",
    "Africa/Cairo",
    "Africa/Johannesburg",
    "America/Chicago",
    "America/Denver",
    "America/Los_Angeles",
    "America/New_York",
    "America/Sao_Paulo",
    "America/Toronto",
    "America/Vancouver",
    "Asia/Dubai",
    "Asia/Kolkata",
    "Asia/Shanghai",
    "Asia/Singapore",
    "Asia/Tokyo",
    "Australia/Perth",
    "Australia/Sydney",
    "Europe/Berlin",
    "Europe/London",
    "Europe/Moscow",
    "Europe/Paris",
    "Pacific/Auckland",
    "Pacific/Honolulu",
];

#[cfg(all(test, feature = "ssr"))]
mod tests {
    #[cfg(feature = "ssr")]
    async fn timezone_pool(with_owner: bool) -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::query(
            "CREATE TABLE app_user (
                id INTEGER PRIMARY KEY,
                timezone TEXT NOT NULL,
                timezone_mode TEXT NOT NULL DEFAULT 'auto'
                    CHECK (timezone_mode IN ('auto', 'manual'))
            )",
        )
        .execute(&pool)
        .await
        .expect("table");
        if with_owner {
            sqlx::query("INSERT INTO app_user (id, timezone) VALUES (1, 'UTC')")
                .execute(&pool)
                .await
                .expect("owner");
        }
        pool
    }

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

    #[tokio::test]
    async fn manual_save_persists_mode_then_publishes_snapshot() {
        let pool = timezone_pool(true).await;
        let store = ep_core::TimezoneStore::default();
        let shanghai = ep_core::AppTimezone::parse("Asia/Shanghai").expect("timezone");

        let saved = super::persist_timezone(&pool, &store, shanghai, super::TimezoneMode::Manual)
            .await
            .expect("save");

        let stored: (String, String) =
            sqlx::query_as("SELECT timezone, timezone_mode FROM app_user WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("stored timezone");
        assert_eq!(stored, ("Asia/Shanghai".into(), "manual".into()));
        assert_eq!(store.snapshot().name(), "Asia/Shanghai");
        assert_eq!(saved.timezone, "Asia/Shanghai");
        assert_eq!(saved.timezone_mode, "manual");
    }

    #[tokio::test]
    async fn automatic_sync_updates_auto_mode_and_is_idempotent() {
        let pool = timezone_pool(true).await;
        let store = ep_core::TimezoneStore::default();
        let tokyo = ep_core::AppTimezone::parse("Asia/Tokyo").expect("timezone");

        let changed = super::sync_browser_timezone_inner(&pool, &store, tokyo)
            .await
            .expect("sync");
        assert!(changed.changed);
        assert_eq!(changed.timezone, "Asia/Tokyo");
        assert_eq!(changed.timezone_mode, "auto");
        assert_eq!(store.snapshot().name(), "Asia/Tokyo");

        let unchanged = super::sync_browser_timezone_inner(&pool, &store, tokyo)
            .await
            .expect("second sync");
        assert!(!unchanged.changed);
        let stored: (String, String) =
            sqlx::query_as("SELECT timezone, timezone_mode FROM app_user WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("stored timezone");
        assert_eq!(stored, ("Asia/Tokyo".into(), "auto".into()));
    }

    #[tokio::test]
    async fn browser_sync_never_overrides_manual_mode() {
        let pool = timezone_pool(true).await;
        let store = ep_core::TimezoneStore::default();
        let shanghai = ep_core::AppTimezone::parse("Asia/Shanghai").expect("timezone");
        let new_york = ep_core::AppTimezone::parse("America/New_York").expect("timezone");
        super::persist_timezone(&pool, &store, shanghai, super::TimezoneMode::Manual)
            .await
            .expect("manual save");

        let synced = super::sync_browser_timezone_inner(&pool, &store, new_york)
            .await
            .expect("manual sync");
        assert!(!synced.changed);
        assert_eq!(synced.timezone, "Asia/Shanghai");
        assert_eq!(synced.timezone_mode, "manual");
        assert_eq!(store.snapshot().name(), "Asia/Shanghai");
        let stored: (String, String) =
            sqlx::query_as("SELECT timezone, timezone_mode FROM app_user WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("stored timezone");
        assert_eq!(stored, ("Asia/Shanghai".into(), "manual".into()));
    }

    #[tokio::test]
    async fn explicit_auto_enable_updates_timezone_and_mode_together() {
        let pool = timezone_pool(true).await;
        let store = ep_core::TimezoneStore::default();
        let shanghai = ep_core::AppTimezone::parse("Asia/Shanghai").expect("timezone");
        let auckland = ep_core::AppTimezone::parse("Pacific/Auckland").expect("timezone");
        super::persist_timezone(&pool, &store, shanghai, super::TimezoneMode::Manual)
            .await
            .expect("manual save");

        let enabled = super::persist_timezone(&pool, &store, auckland, super::TimezoneMode::Auto)
            .await
            .expect("enable auto");
        assert_eq!(enabled.timezone, "Pacific/Auckland");
        assert_eq!(enabled.timezone_mode, "auto");
        assert_eq!(store.snapshot().name(), "Pacific/Auckland");
        let stored: (String, String) =
            sqlx::query_as("SELECT timezone, timezone_mode FROM app_user WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("stored timezone");
        assert_eq!(stored, ("Pacific/Auckland".into(), "auto".into()));
    }

    #[tokio::test]
    async fn failed_timezone_save_keeps_the_active_snapshot() {
        let pool = timezone_pool(false).await;
        let store = ep_core::TimezoneStore::default();
        let shanghai = ep_core::AppTimezone::parse("Asia/Shanghai").expect("timezone");

        assert!(
            super::persist_timezone(&pool, &store, shanghai, super::TimezoneMode::Manual,)
                .await
                .is_err()
        );
        assert_eq!(store.snapshot().name(), "UTC");
    }
}
