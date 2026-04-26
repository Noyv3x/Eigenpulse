use ep_core::IconKind;
use ep_ui::{Card, Icon, PageHead, Tag};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

use super::server_err;

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

#[server(ListChannels, "/api/_internal/cfg", "Url", "list_channels")]
pub async fn list_channels() -> Result<Vec<ChannelDto>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        let rows = ep_notify::list_channels(&st.db).await.map_err(server_err)?;
        Ok(rows.into_iter().map(|r| ChannelDto {
            id: r.id, kind: r.kind, name: r.name, enabled: r.enabled,
            min_severity: r.min_severity, created_at: r.created_at,
        }).collect())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
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
        if !["inapp", "smtp", "bark", "telegram", "discord"].contains(&kind.as_str()) {
            return Err(ServerFnError::Args(format!("unknown kind: {kind}")));
        }
        if serde_json::from_str::<serde_json::Value>(&config_json).is_err() {
            return Err(ServerFnError::Args("config_json must be valid JSON".into()));
        }
        let st: ep_core::AppState = expect_context();
        ep_notify::create_channel(&st.db, &kind, &name, &config_json, &min_severity)
            .await.map_err(server_err)
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(DeleteChannel, "/api/_internal/cfg", "Url", "delete_channel")]
pub async fn delete_channel(id: i64) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        ep_notify::delete_channel(&st.db, id).await.map_err(server_err)
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(ToggleChannel, "/api/_internal/cfg", "Url", "toggle_channel")]
pub async fn toggle_channel(id: i64) -> Result<bool, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        let new_enabled: i64 = sqlx::query_scalar(
            "UPDATE notify_channel SET enabled = NOT enabled WHERE id = ?1 RETURNING enabled"
        ).bind(id).fetch_one(&st.db).await.map_err(server_err)?;
        Ok(new_enabled != 0)
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(TestChannel, "/api/_internal/cfg", "Url", "test_channel")]
pub async fn test_channel(id: i64) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        let row: (String, String) = sqlx::query_as(
            "SELECT kind, config_json FROM notify_channel WHERE id = ?1"
        ).bind(id).fetch_one(&st.db).await.map_err(server_err)?;
        // The notifier `Err` from lettre/reqwest can include the SMTP connection
        // string with password, the Bark device-key URL, the Telegram bot URL
        // (`api.telegram.org/bot<TOKEN>/sendMessage`), or the Discord webhook URL —
        // i.e. the very secrets stored in `config_json`. Forwarding it via
        // `server_err` would surface those secrets in the browser. Log the full
        // detail server-side and return a generic, channel-typed message.
        ep_notify::test_channel(&row.0, &row.1).await.map_err(|e| {
            tracing::warn!(channel_id = id, kind = %row.0, error = %e, "notify channel test failed");
            server_err(format!("{} 通道测试失败 · 详细错误已记录到服务器日志", row.0))
        })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
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

    view! {
        <div class="view">
            <PageHead
                code="CFG-NOT-01"
                module="SETTINGS · 通知通道"
                title="Notifications"
                title_cn="通知通道"
                sub="SMTP / Bark / Telegram / Discord · in-app SSE 始终启用"
            />

            <Card title="新增通道" code="CFG-NOT-NEW" sub="config_json 字段按通道类型填充；inapp 用 {}">
                <ActionForm action=create attr:class="vstack" attr:style="gap:10px">
                    <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"名称"</span>
                            <input name="name" required placeholder="家庭邮箱"
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"类型"</span>
                            <select name="kind" required style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)">
                                <option value="smtp">"SMTP · 邮件"</option>
                                <option value="bark">"Bark · iOS 推送"</option>
                                <option value="telegram">"Telegram Bot"</option>
                                <option value="discord">"Discord Webhook"</option>
                                <option value="inapp">"inapp · 站内"</option>
                            </select>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"最低严重度"</span>
                            <select name="min_severity" style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)">
                                <option value="info" selected="selected">"info · 全部"</option>
                                <option value="warn">"warn · 警告及以上"</option>
                                <option value="crit">"crit · 仅严重"</option>
                            </select>
                        </label>
                    </div>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"config_json"</span>
                        <textarea name="config_json" rows="4" required
                                  placeholder=r#"{"host":"smtp.example.com","port":587,"username":"...","password":"...","from":"a@b","to":"a@b","starttls":true}"#
                                  style="font-family:var(--font-mono);font-size:12px;padding:8px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"></textarea>
                    </label>
                    <div class="hstack" style="gap:8px">
                        <button class="btn primary" type="submit">
                            <Icon kind=IconKind::Plus size=14/>"添加"
                        </button>
                        {move || create.value().get().and_then(|r| r.err()).map(|e| view! {
                            <span class="tag rose">{e.to_string()}</span>
                        })}
                    </div>
                </ActionForm>
            </Card>

            {move || test_msg.get().map(|r| match r {
                Ok(_) => view! { <p style="margin:12px 0;color:var(--primary-ink)" class="mono">"✓ 测试通道发送成功"</p> }.into_any(),
                Err(e) => view! { <p style="margin:12px 0;color:var(--rose-ink)" class="mono">"✕ 测试失败 · " {e.to_string()}</p> }.into_any(),
            })}

            <div style="margin-top:24px"></div>

            <Card title="已配置通道" code="CFG-NOT-LST">
                <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:120px">"loading…"</div> }>
                    {move || channels.get().map(|res| match res {
                        Err(e) => view! { <p>"加载失败 · " {e.to_string()}</p> }.into_any(),
                        Ok(rows) if rows.is_empty() => view! { <p class="muted">"暂无通道。添加上方一项即可启用 SMTP/Bark/Telegram/Discord 推送。"</p> }.into_any(),
                        Ok(rows) => view! {
                            <table class="tbl">
                                <thead>
                                    <tr>
                                        <th>"名称"</th>
                                        <th>"类型"</th>
                                        <th>"最低严重度"</th>
                                        <th>"启用"</th>
                                        <th class="num">"操作"</th>
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
                                                        <button class="btn sm" type="submit">
                                                            <Tag tone=enabled_tone>{enabled_label}</Tag>
                                                        </button>
                                                    </ActionForm>
                                                </td>
                                                <td class="num">
                                                    <ActionForm action=test attr:style="display:inline;margin-right:6px">
                                                        <input type="hidden" name="id" value=id/>
                                                        <button class="btn sm" type="submit">"测试"</button>
                                                    </ActionForm>
                                                    <ActionForm action=delete attr:style="display:inline">
                                                        <input type="hidden" name="id" value=id/>
                                                        <button class="btn sm" type="submit"
                                                                style="color:var(--rose-ink)"
                                                                onclick="return confirm('删除该通道？')">"删除"</button>
                                                    </ActionForm>
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
