pub mod notifications;
pub mod security;

use crate::admin::{admin_status, AdminStatus, ExportAllFn};
use ep_core::IconKind;
use ep_i18n::{server_fn_error_text, t, use_locale};
use ep_ui::{use_tweaks, Density, TweakState};
use ep_ui::{Card, Icon, PageHead, StatRow};
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
    const COUNT_USER_DATA_ROWS_SQL: &str = "
        SELECT
            (SELECT COUNT(*) FROM module_link) +
            (SELECT COUNT(*) FROM activity) +
            (SELECT COUNT(*) FROM notification) +
            (SELECT COUNT(*) FROM notify_channel) +
            (SELECT COUNT(*) FROM notify_delivery) +
            (SELECT COUNT(*) FROM pat) +
            (SELECT COUNT(*) FROM fin_account) +
            (SELECT COUNT(*) FROM fin_category) +
            (SELECT COUNT(*) FROM fin_txn) +
            (SELECT COUNT(*) FROM fin_budget) +
            (SELECT COUNT(*) FROM fit_workout) +
            (SELECT COUNT(*) FROM fit_set) +
            (SELECT COUNT(*) FROM lrn_course) +
            (SELECT COUNT(*) FROM lrn_book) +
            (SELECT COUNT(*) FROM lrn_note)
    ";
    sqlx::query_scalar(COUNT_USER_DATA_ROWS_SQL)
        .fetch_one(pool)
        .await
}

/// Build a `data:application/json` download URI from a [`DataExport`]. Pure
/// string math via `ep_core::url_encode_query_value` — wasm-safe, no Blob /
/// URL.createObjectURL. On the (unreachable for valid DTOs) serialization
/// failure, falls back to an empty JSON object so the anchor stays valid.
fn export_data_uri(data: &crate::admin::DataExport) -> String {
    let json = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    let encoded = ep_core::url_encode_query_value(&json);
    let mut uri =
        String::with_capacity("data:application/json;charset=utf-8,".len() + encoded.len());
    uri.push_str("data:application/json;charset=utf-8,");
    uri.push_str(&encoded);
    uri
}

/// The "Last Backup" stat row, fed by the `admin_status` resource. Rendered as
/// a nested `<Suspense>` inside the already-loaded summary so a backup-status
/// fetch failure degrades to "not configured" rather than blanking the card.
fn render_backup_row(status: Resource<Result<AdminStatus, ServerFnError>>) -> impl IntoView {
    let locale = use_locale();
    view! {
        <Suspense fallback=move || view! {
            <StatRow label=t(locale, "app.settings.index.data_card.backup") value=t(locale, "app.common.loading").to_string()/>
        }>
            {move || {
                let value = match status.get() {
                    Some(Ok(s)) if s.last_backup_exists => match s.last_backup_bytes {
                        Some(bytes) => crate::views::status::fmt_bytes(bytes),
                        None => t(locale, "app.settings.index.unconfigured").to_string(),
                    },
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
    let summary = Resource::new(|| (), |_| async { load_settings_summary().await });
    // The Data card surfaces the live backup snapshot (size only — paths are
    // for the dedicated /status page). Separate resource so a backup-status
    // failure never blanks the account card.
    let status = Resource::new(|| (), |_| async { admin_status().await });
    // Full-data export. On success the JSON is materialized into a `data:` URI
    // on a download anchor (same wasm-safe pattern as the finance CSV export) —
    // no Blob/URL.createObjectURL, no fs.
    let export = ServerAction::<ExportAllFn>::new();
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
                        <div style="display:flex;flex-direction:column;gap:8px;padding:6px 0">
                            <span class="skeleton-line" style="height:14px;width:60%;display:block"></span>
                            <span class="skeleton-line" style="height:14px;width:55%;display:block"></span>
                            <span class="skeleton-line" style="height:14px;width:40%;display:block"></span>
                        </div>
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
                        <div style="display:flex;flex-direction:column;gap:8px;padding:6px 0">
                            <span class="skeleton-line" style="height:14px;width:60%;display:block"></span>
                            <span class="skeleton-line" style="height:14px;width:55%;display:block"></span>
                            <span class="skeleton-line" style="height:14px;width:40%;display:block"></span>
                        </div>
                    }>
                        {move || summary.get().map(|res| match res {
                            Err(e) => view! { <p>{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</p> }.into_any(),
                            Ok(s) => view! {
                                <div class="vstack" style="gap:0">
                                    <StatRow label=t(locale, "app.settings.index.data_card.storage") value=s.database_location/>
                                    {render_backup_row(status)}
                                    <StatRow label=t(locale, "app.settings.index.data_card.sync") value=t(locale, "app.settings.index.data_card.local").to_string()/>
                                </div>
                            }.into_any()
                        })}
                    </Suspense>
                    <div class="hstack" style="gap:10px;align-items:center;flex-wrap:wrap;margin-top:14px">
                        <A href="/status" attr:class="btn">
                            <Icon kind=IconKind::Check size=14/>{t(locale, "app.settings.index.data_card.open_status")}
                        </A>
                        <ActionForm action=export attr:style="display:inline-flex;gap:10px;align-items:center;flex-wrap:wrap">
                            <button class="btn" type="submit">
                                <Icon kind=IconKind::Export size=14/>{t(locale, "app.settings.index.data_card.export")}
                            </button>
                            // Stable wrapper element so the tachys text-node
                            // walker keeps its anchor next to the <ActionForm>
                            // button (AGENTS.md footgun). Holds either the ready
                            // download link or the error tag.
                            <span class="new-token-slot">
                                {move || match export.value().get() {
                                    Some(Ok(data)) => {
                                        let href = export_data_uri(&data);
                                        view! {
                                            <a class="btn primary" download="eigenpulse-export.json" href=href>
                                                <Icon kind=IconKind::Upload size=14/>{t(locale, "app.settings.index.data_card.download")}
                                            </a>
                                        }.into_any()
                                    }
                                    Some(Err(e)) => view! {
                                        <span class="tag rose">{server_fn_error_text(&e)}</span>
                                    }.into_any(),
                                    None => ().into_any(),
                                }}
                            </span>
                        </ActionForm>
                    </div>
                    // Import is a documented server-side stub (no DB writes); the
                    // control is intentionally disabled until a safe transactional
                    // importer lands. Surfaced so the absence is explicit, not a
                    // missing feature.
                    <p class="muted" style="margin-top:10px;font-size:12px">
                        {t(locale, "app.settings.index.data_card.import_disabled")}
                    </p>
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

    #[tokio::test]
    async fn count_user_data_rows_counts_only_explicit_user_tables() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        for sql in [
            "CREATE TABLE app_user (id INTEGER)",
            "CREATE TABLE session (id INTEGER)",
            "CREATE TABLE seq (id INTEGER)",
            "CREATE TABLE _ep_module_migration (id INTEGER)",
            "CREATE TABLE module_link (id INTEGER)",
            "CREATE TABLE activity (id INTEGER)",
            "CREATE TABLE notification (id INTEGER)",
            "CREATE TABLE notify_channel (id INTEGER)",
            "CREATE TABLE notify_delivery (id INTEGER)",
            "CREATE TABLE pat (id INTEGER)",
            "CREATE TABLE fin_account (id INTEGER)",
            "CREATE TABLE fin_category (id INTEGER)",
            "CREATE TABLE fin_txn (id INTEGER)",
            "CREATE TABLE fin_budget (id INTEGER)",
            "CREATE TABLE fit_workout (id INTEGER)",
            "CREATE TABLE fit_set (id INTEGER)",
            "CREATE TABLE lrn_course (id INTEGER)",
            "CREATE TABLE lrn_book (id INTEGER)",
            "CREATE TABLE lrn_note (id INTEGER)",
        ] {
            sqlx::query(sql).execute(&pool).await.expect("create table");
        }

        for sql in [
            "INSERT INTO app_user (id) VALUES (1), (2)",
            "INSERT INTO session (id) VALUES (1), (2)",
            "INSERT INTO seq (id) VALUES (1), (2)",
            "INSERT INTO _ep_module_migration (id) VALUES (1), (2)",
            "INSERT INTO module_link (id) VALUES (1), (2)",
            "INSERT INTO activity (id) VALUES (1), (2)",
            "INSERT INTO notification (id) VALUES (1), (2)",
            "INSERT INTO notify_channel (id) VALUES (1), (2)",
            "INSERT INTO notify_delivery (id) VALUES (1), (2)",
            "INSERT INTO pat (id) VALUES (1), (2)",
            "INSERT INTO fin_account (id) VALUES (1), (2)",
            "INSERT INTO fin_category (id) VALUES (1), (2)",
            "INSERT INTO fin_txn (id) VALUES (1), (2)",
            "INSERT INTO fin_budget (id) VALUES (1), (2)",
            "INSERT INTO fit_workout (id) VALUES (1), (2)",
            "INSERT INTO fit_set (id) VALUES (1), (2)",
            "INSERT INTO lrn_course (id) VALUES (1), (2)",
            "INSERT INTO lrn_book (id) VALUES (1), (2)",
            "INSERT INTO lrn_note (id) VALUES (1), (2)",
        ] {
            sqlx::query(sql).execute(&pool).await.expect("insert rows");
        }

        let total = super::count_user_data_rows(&pool)
            .await
            .expect("count rows");

        assert_eq!(total, 30);
    }
}
