#![cfg_attr(
    not(feature = "ssr"),
    allow(
        unused_variables,
        reason = "Leptos server-function parameters are serialized by client builds while their implementations are SSR-only"
    )
)]

use ep_core::IconKind;
use ep_i18n::{server_fn_error_text, t, use_locale};
use ep_ui::{Card, ErrorSlot, Icon, PageHead, RowDeleteAction, Tag};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use ep_core::server_err;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Public-facing channel summary. **Never** carries `config_json` —
/// SMTP passwords / Telegram bot tokens / webhook secrets must stay server-side.
/// Re-editing a channel from the UI requires the user to re-enter the secret;
/// that's the explicit trade-off vs. round-tripping plaintext credentials.
pub struct ChannelDto {
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    pub min_severity: String,
    pub created_at: i64,
}

#[cfg(feature = "ssr")]
fn normalize_channel_input(
    kind: String,
    name: String,
    config_json: String,
    min_severity: String,
) -> Result<(String, String, String, String), ServerFnError> {
    let kind = kind.trim().to_ascii_lowercase();
    if !["inapp", "smtp", "bark", "telegram", "discord"].contains(&kind.as_str()) {
        return Err(ep_i18n::err_with(
            "app.settings.notifications.err_kind_unknown",
            &kind,
        ));
    }

    let name = ep_core::trim_to_option(&name)
        .ok_or_else(|| ep_i18n::err("app.settings.notifications.err_name_required"))?;
    if name.chars().count() > ep_notify::MAX_CHANNEL_NAME_CHARS {
        return Err(ep_i18n::err_with(
            "app.settings.notifications.err_name_too_long",
            ep_notify::MAX_CHANNEL_NAME_CHARS,
        ));
    }

    let raw_min_severity = min_severity.trim();
    let min_severity = ep_core::Severity::try_parse(raw_min_severity)
        .ok_or_else(|| {
            ep_i18n::err_with(
                "app.settings.notifications.err_min_severity_unknown",
                raw_min_severity,
            )
        })?
        .as_str()
        .to_string();

    let config_json = config_json.trim().to_string();
    if serde_json::from_str::<serde_json::Value>(&config_json).is_err() {
        return Err(ep_i18n::err(
            "app.settings.notifications.err_config_json_invalid",
        ));
    }
    if let Err(e) = ep_notify::validate_notifier_config(&kind, &config_json) {
        tracing::warn!(kind = %kind, error = %e, "invalid notify channel config");
        return Err(ep_i18n::err_with(
            "app.settings.notifications.err_config_json_kind",
            &kind,
        ));
    }

    Ok((kind, name, config_json, min_severity))
}

#[cfg(feature = "ssr")]
fn normalize_channel_id(id: i64) -> Result<i64, ServerFnError> {
    if id > 0 {
        Ok(id)
    } else {
        Err(ep_i18n::err_with(
            "app.settings.notifications.err_channel_not_found",
            id.to_string(),
        ))
    }
}

#[server(ListChannels, "/api/_internal/cfg", "Url", "list_channels")]
pub async fn list_channels() -> Result<Vec<ChannelDto>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;
        let rows = ep_notify::list_channels(&st.db).await.map_err(server_err)?;
        Ok(rows
            .into_iter()
            .map(|r| ChannelDto {
                id: r.id,
                kind: r.kind,
                name: r.name,
                enabled: r.enabled,
                min_severity: r.min_severity,
                created_at: r.created_at,
            })
            .collect())
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(CreateChannel, "/api/_internal/cfg", "Url", "create_channel")]
pub async fn create_channel(
    kind: String,
    name: String,
    config_json: String,
    min_severity: String,
) -> Result<i64, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let (kind, name, config_json, min_severity) =
            normalize_channel_input(kind, name, config_json, min_severity)?;
        let st = ep_core::app_state_context()?;
        ep_notify::create_channel(&st.db, &kind, &name, &config_json, &min_severity)
            .await
            .map_err(server_err)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    #[test]
    fn normalize_channel_input_trims_and_canonicalizes() {
        let (kind, name, config_json, min_severity) = normalize_channel_input(
            " INAPP ".into(),
            "  Local inbox  ".into(),
            "  {}  ".into(),
            " WARN ".into(),
        )
        .expect("valid input");

        assert_eq!(kind, "inapp");
        assert_eq!(name, "Local inbox");
        assert_eq!(config_json, "{}");
        assert_eq!(min_severity, "warn");
    }

    #[test]
    fn normalize_channel_input_accepts_severity_aliases() {
        let (_, _, _, min_severity) = normalize_channel_input(
            "inapp".into(),
            "Local inbox".into(),
            "{}".into(),
            " CRITICAL ".into(),
        )
        .expect("valid critical alias");

        assert_eq!(min_severity, "crit");
    }

    #[test]
    fn normalize_channel_input_rejects_blank_name() {
        let err = normalize_channel_input("inapp".into(), "   ".into(), "{}".into(), "info".into())
            .expect_err("blank name should fail");

        assert_eq!(
            ep_i18n::parse_err(&err).map(|(code, _)| code),
            Some("app.settings.notifications.err_name_required")
        );
    }

    #[test]
    fn normalize_channel_input_rejects_overlong_name() {
        let err = normalize_channel_input(
            "inapp".into(),
            "x".repeat(ep_notify::MAX_CHANNEL_NAME_CHARS + 1),
            "{}".into(),
            "info".into(),
        )
        .expect_err("overlong name should fail");

        assert_eq!(
            ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("app.settings.notifications.err_name_too_long", "64"))
        );
    }

    #[test]
    fn normalize_channel_input_rejects_mismatched_config() {
        let err =
            normalize_channel_input("telegram".into(), "Ops".into(), "{}".into(), "info".into())
                .expect_err("missing telegram fields should fail");

        assert_eq!(
            ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some((
                "app.settings.notifications.err_config_json_kind",
                "telegram"
            ))
        );
    }

    #[test]
    fn normalize_channel_id_rejects_non_positive_ids() {
        assert_eq!(normalize_channel_id(42).unwrap(), 42);

        let err = normalize_channel_id(0).expect_err("invalid id");
        assert_eq!(
            ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("app.settings.notifications.err_channel_not_found", "0"))
        );
    }

    #[test]
    fn channel_dto_never_serializes_config_json_or_provider_secrets() {
        let dto = ChannelDto {
            id: 1,
            kind: "telegram".into(),
            name: "Ops".into(),
            enabled: true,
            min_severity: "info".into(),
            created_at: 1,
        };

        let value = serde_json::to_value(dto).expect("serialize ChannelDto");

        assert!(value.get("kind").is_some());
        assert!(value.get("config_json").is_none());
        assert!(value.get("password").is_none());
        assert!(value.get("bot_token").is_none());
        assert!(value.get("device_key").is_none());
        assert!(value.get("webhook_url").is_none());
    }
}

#[server(DeleteChannel, "/api/_internal/cfg", "Url", "delete_channel")]
pub async fn delete_channel(id: i64) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let id = normalize_channel_id(id)?;
        let st = ep_core::app_state_context()?;
        let deleted = ep_notify::delete_channel(&st.db, id)
            .await
            .map_err(server_err)?;
        if deleted {
            Ok(())
        } else {
            Err(ep_i18n::err_with(
                "app.settings.notifications.err_channel_not_found",
                id.to_string(),
            ))
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(ToggleChannel, "/api/_internal/cfg", "Url", "toggle_channel")]
pub async fn toggle_channel(id: i64) -> Result<bool, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let id = normalize_channel_id(id)?;
        let st = ep_core::app_state_context()?;
        let new_enabled: Option<i64> = sqlx::query_scalar(
            "UPDATE notify_channel SET enabled = NOT enabled WHERE id = ?1 RETURNING enabled",
        )
        .bind(id)
        .fetch_optional(&st.db)
        .await
        .map_err(server_err)?;
        match new_enabled {
            Some(new_enabled) => Ok(new_enabled != 0),
            None => Err(ep_i18n::err_with(
                "app.settings.notifications.err_channel_not_found",
                id.to_string(),
            )),
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(TestChannel, "/api/_internal/cfg", "Url", "test_channel")]
pub async fn test_channel(id: i64) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let id = normalize_channel_id(id)?;
        let st = ep_core::app_state_context()?;
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT kind, config_json FROM notify_channel WHERE id = ?1")
                .bind(id)
                .fetch_optional(&st.db)
                .await
                .map_err(server_err)?;
        let Some(row) = row else {
            return Err(ep_i18n::err_with(
                "app.settings.notifications.err_channel_not_found",
                id.to_string(),
            ));
        };
        // The notifier `Err` from lettre/reqwest can include the SMTP connection
        // string with password, the Bark device-key URL, the Telegram bot URL
        // (`api.telegram.org/bot<TOKEN>/sendMessage`), or the Discord webhook URL.
        // Log that detail server-side and return only a localized, channel-typed
        // message over the server-fn wire.
        ep_notify::test_channel(&row.0, &row.1).await.map_err(|e| {
            tracing::warn!(channel_id = id, kind = %row.0, error = %e, "notify channel test failed");
            ep_i18n::err_with("app.settings.notifications.err_test_channel", &row.0)
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[component]
pub fn NotificationChannelsView() -> impl IntoView {
    let channels = Resource::new(|| (), |_| async { list_channels().await });
    let create = ServerAction::<CreateChannel>::new();
    let delete = ServerAction::<DeleteChannel>::new();
    let toggle = ServerAction::<ToggleChannel>::new();
    let test = ServerAction::<TestChannel>::new();

    // Refetch on action completion. Skip the very first run (`prev == None`):
    // the Resource already fetches on mount, so refetching there is a wasted query.
    Effect::new(move |prev: Option<()>| {
        create.version().get();
        delete.version().get();
        toggle.version().get();
        if prev.is_some() {
            channels.refetch();
        }
    });

    let test_msg = test.value();
    let locale = use_locale();

    view! {
        <div class="view">
            <PageHead
                module=t(locale, "app.settings.notifications.page.module")
                title=t(locale, "app.settings.notifications.page.title")
                title_cn=t(locale, "app.settings.notifications.page.title_cn")
                sub=t(locale, "app.settings.notifications.page.sub")
            />

            <Card title=t(locale, "app.settings.notifications.card.new_title") sub=t(locale, "app.settings.notifications.card.new_sub")>
                <ActionForm action=create attr:class="vstack" attr:style="gap:10px">
                    <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="ep-field-label">{t(locale, "app.settings.notifications.field.name")}</span>
                            <input name="name" required placeholder=t(locale, "app.settings.notifications.placeholder.name")
                                   class="ep-input"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="ep-field-label">{t(locale, "app.settings.notifications.field.kind")}</span>
                            <select name="kind" required class="ep-select">
                                <option value="smtp">{t(locale, "app.settings.notifications.option.smtp")}</option>
                                <option value="bark">{t(locale, "app.settings.notifications.option.bark")}</option>
                                <option value="telegram">"Telegram Bot"</option>
                                <option value="discord">"Discord Webhook"</option>
                                <option value="inapp">{t(locale, "app.settings.notifications.option.inapp")}</option>
                            </select>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="ep-field-label">{t(locale, "app.settings.notifications.field.min_severity")}</span>
                            <select name="min_severity" class="ep-select">
                                <option value="info" selected="selected">{t(locale, "app.settings.notifications.option.info")}</option>
                                <option value="warn">{t(locale, "app.settings.notifications.option.warn")}</option>
                                <option value="crit">{t(locale, "app.settings.notifications.option.crit")}</option>
                            </select>
                        </label>
                    </div>
                    <label class="vstack" style="gap:4px">
                        <span class="ep-field-label">{t(locale, "app.settings.notifications.field.config")}</span>
                        <textarea name="config_json" rows="4" required
                                  placeholder=r#"{"host":"smtp.example.com","port":587,"username":"...","password":"...","from":"a@b","to":"a@b","starttls":true}"#
                                  class="ep-textarea mono"></textarea>
                    </label>
                    <div class="hstack" style="gap:8px">
                        <button class="btn primary" type="submit"
                                disabled=move || create.pending().get()
                                aria-busy=move || create.pending().get().to_string()>
                            <Icon kind=IconKind::Plus size=14/>{t(locale, "app.settings.notifications.add")}
                        </button>
                        // ErrorSlot keeps a stable DOM anchor next to the
                        // ActionForm in both SSR and hydrate trees.
                        <ErrorSlot action=create/>
                    </div>
                </ActionForm>
            </Card>

            <div class="test-notice-slot">
                {move || test_msg.get().map(|r| match r {
                    Ok(_) => view! { <p style="margin:12px 0;color:var(--primary-ink)" class="mono">{t(locale, "app.settings.notifications.test_ok")}</p> }.into_any(),
                    Err(e) => view! { <p style="margin:12px 0;color:var(--rose-ink)" class="mono">{t(locale, "app.settings.notifications.test_failed")} {server_fn_error_text(&e)}</p> }.into_any(),
                })}
            </div>

            <div style="margin-top:24px"></div>

            <Card title=t(locale, "app.settings.notifications.card.list")>
                <Suspense fallback=move || view! {
                    <div style="display:flex;flex-direction:column;gap:10px;padding:6px 0">
                        <span class="skeleton-line" style="height:14px;width:55%;display:block"></span>
                        <span class="skeleton-line" style="height:14px;display:block"></span>
                        <span class="skeleton-line" style="height:14px;display:block"></span>
                    </div>
                }>
                    {move || channels.get().map(|res| match res {
                        Err(e) => view! { <p>{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</p> }.into_any(),
                        Ok(rows) if rows.is_empty() => view! {
                            <ep_ui::EmptyState
                                icon=ep_core::IconKind::Bell
                                title=t(locale, "app.settings.notifications.empty")
                                desc=t(locale, "app.settings.notifications.empty_hint")
                                compact=true
                            />
                        }.into_any(),
                        Ok(rows) => view! {
                            <table class="tbl">
                                <thead>
                                    <tr>
                                        <th>{t(locale, "app.settings.notifications.field.name")}</th>
                                        <th>{t(locale, "app.settings.notifications.field.kind")}</th>
                                        <th>{t(locale, "app.settings.notifications.field.min_severity")}</th>
                                        <th>{t(locale, "app.settings.notifications.field.enabled")}</th>
                                        <th class="num">{t(locale, "app.settings.notifications.field.ops")}</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {rows.into_iter().map(|c| {
                                        let id = c.id;
                                        let kind_tone = match c.kind.as_str() {
                                            "smtp" => ep_core::Tone::Amber,
                                            "bark" => ep_core::Tone::Blue,
                                            "telegram" => ep_core::Tone::Violet,
                                            "discord" => ep_core::Tone::Rose,
                                            _ => ep_core::Tone::Green,
                                        };
                                        let enabled_tone = if c.enabled { ep_core::Tone::Green } else { ep_core::Tone::None };
                                        let enabled_label = if c.enabled { "ON" } else { "OFF" };
                                        view! {
                                            <tr>
                                                <td>{c.name}</td>
                                                <td><Tag tone=kind_tone>{c.kind.to_uppercase()}</Tag></td>
                                                <td class="mono dim">{c.min_severity}</td>
                                                <td>
                                                    <ActionForm action=toggle attr:style="display:inline">
                                                        <input type="hidden" name="id" value=id/>
                                                        <button class="btn sm" type="submit"
                                                                disabled=move || toggle.pending().get()
                                                                aria-busy=move || toggle.pending().get().to_string()>
                                                            <Tag tone=enabled_tone>{enabled_label}</Tag>
                                                        </button>
                                                    </ActionForm>
                                                </td>
                                                <td class="num">
                                                    <span class="row-actions-slot">
                                                        <ActionForm action=test attr:style="display:inline;margin-right:6px">
                                                            <input type="hidden" name="id" value=id/>
                                                            <button class="btn sm" type="submit"
                                                                    disabled=move || test.pending().get()
                                                                    aria-busy=move || test.pending().get().to_string()>
                                                                {t(locale, "app.settings.notifications.test")}
                                                            </button>
                                                        </ActionForm>
                                                        <RowDeleteAction action=delete value=id.to_string()
                                                                         field="id" confirm=t(locale, "app.settings.notifications.confirm_delete")/>
                                                    </span>
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        }.into_any(),
                    })}
                </Suspense>
            </Card>
        </div>
    }
}
