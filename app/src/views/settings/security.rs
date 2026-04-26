use ep_core::IconKind;
use ep_ui::{Card, Icon, PageHead, Tag};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

use super::server_err;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatDto {
    pub id: i64,
    pub name: String,
    pub prefix: String,
    pub scopes: String,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
    /// Pre-computed server-side. The view runs on the wasm32 hydrate target,
    /// where `time::OffsetDateTime::now_utc()` panics without the `wasm-bindgen`
    /// feature; pushing the "now" comparison server-side keeps the client pure.
    pub is_expired: bool,
    pub is_revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedPat {
    pub token: String,
    pub row: PatDto,
}

#[server(ListPats, "/api/_internal/cfg", "Url", "list_pats")]
pub async fn list_pats() -> Result<Vec<PatDto>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        let rows = ep_auth::list_pats(&st.db).await.map_err(server_err)?;
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        Ok(rows.into_iter().map(|r| {
            let is_revoked = r.revoked_at.is_some();
            let is_expired = r.expires_at.map(|e| e <= now).unwrap_or(false);
            PatDto {
                id: r.id, name: r.name, prefix: r.prefix, scopes: r.scopes,
                created_at: r.created_at, expires_at: r.expires_at,
                last_used_at: r.last_used_at, revoked_at: r.revoked_at,
                is_expired, is_revoked,
            }
        }).collect())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(GeneratePat, "/api/_internal/cfg", "Url", "generate_pat")]
pub async fn generate_pat(
    name: String,
    scopes: String,
    // HTML forms always submit empty inputs as "" — `Option<i64>` would fail
    // serde deserialization on a blank string. Accept `String` and parse here:
    // empty / whitespace = perpetual token (no expiry).
    expires_days: String,
) -> Result<GeneratedPat, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        if name.trim().is_empty() {
            return Err(ServerFnError::Args("name required".into()));
        }
        let trimmed = expires_days.trim();
        let expires_days: Option<i64> = if trimmed.is_empty() {
            None
        } else {
            match trimmed.parse::<i64>() {
                Ok(n) => Some(n),
                Err(_) => return Err(ServerFnError::Args(
                    "expires_days must be a positive integer or blank".into(),
                )),
            }
        };
        if let Some(d) = expires_days {
            if d <= 0 {
                return Err(ServerFnError::Args("expires_days must be positive".into()));
            }
        }
        let st: ep_core::AppState = expect_context();
        let scope_vec: Vec<&str> = scopes.split_whitespace().collect();
        let expires_at = expires_days.map(|d| time::OffsetDateTime::now_utc().unix_timestamp() + d * 86400);
        let (token, row) = ep_auth::generate_pat(&st.db, &name, &scope_vec, expires_at)
            .await.map_err(server_err)?;
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let is_expired = row.expires_at.map(|e| e <= now).unwrap_or(false);
        Ok(GeneratedPat {
            token,
            row: PatDto {
                id: row.id, name: row.name, prefix: row.prefix, scopes: row.scopes,
                created_at: row.created_at, expires_at: row.expires_at,
                last_used_at: row.last_used_at, revoked_at: row.revoked_at,
                is_expired, is_revoked: false,
            },
        })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(RevokePat, "/api/_internal/cfg", "Url", "revoke_pat")]
pub async fn revoke_pat(id: i64) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        ep_auth::revoke_pat(&st.db, id).await.map_err(server_err)
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

const ALL_SCOPES: &[(&str, &str)] = &[
    ("fin:read",     "FIN · 只读"),
    ("fin:write",    "FIN · 写入"),
    ("fit:write",    "FIT · 写入"),
    ("notify:write", "NOTIFY · 推送"),
    ("*",            "全部 · OWNER"),
];

#[component]
pub fn PatView() -> impl IntoView {
    let pats = Resource::new(|| (), |_| async { list_pats().await });
    let generate = ServerAction::<GeneratePat>::new();
    let revoke = ServerAction::<RevokePat>::new();

    Effect::new(move |prev: Option<()>| {
        generate.version().get();
        revoke.version().get();
        if prev.is_some() {
            pats.refetch();
        }
    });

    let new_token = move || generate.value().get().and_then(|r| r.ok()).map(|g| g.token);

    view! {
        <div class="view">
            <PageHead
                code="CFG-SEC-01"
                module="SETTINGS · API 安全"
                title="Personal Access Tokens"
                title_cn="开放 API 鉴权"
                sub="生成 ep_pat_… token 用于 /api/v1/* 调用。token 仅创建时展示一次。"
            />

            <Card title="生成新 Token" code="CFG-SEC-NEW">
                <ActionForm action=generate attr:class="vstack" attr:style="gap:10px">
                    <div style="display:grid;grid-template-columns:2fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"名称"</span>
                            <input name="name" required placeholder="iOS Shortcuts · 记账"
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"过期 · 天 (留空=永久)"</span>
                            <input name="expires_days" type="number" min="1" placeholder="365"
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono)"/>
                        </label>
                    </div>
                    <fieldset style="border:1px solid var(--border);border-radius:8px;padding:8px 12px">
                        <legend class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em;padding:0 4px">"scopes (空格分隔)"</legend>
                        <input name="scopes" required placeholder="fin:read fin:write notify:write" value="fin:read"
                               style="width:100%;padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono);font-size:12px"/>
                        <div class="hstack" style="gap:8px;margin-top:8px;flex-wrap:wrap;font-size:11px;color:var(--ink-3)">
                            {ALL_SCOPES.iter().map(|(scope, label)| view! {
                                <span class="mono">{format!("{} · {}", scope, label)}</span>
                            }).collect_view()}
                        </div>
                    </fieldset>
                    <div class="hstack" style="gap:8px">
                        <button class="btn primary" type="submit">
                            <Icon kind=IconKind::Plus size=14/>"生成"
                        </button>
                        {move || generate.value().get().and_then(|r| r.err()).map(|e| view! {
                            <span class="tag rose">{e.to_string()}</span>
                        })}
                    </div>
                </ActionForm>

                {move || new_token().map(|t| view! {
                    <div style="margin-top:14px;padding:14px;border:1px solid var(--primary);border-radius:10px;background:var(--primary-soft)">
                        <div class="mono" style="font-size:11px;color:var(--primary-ink);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">
                            "✓ Token · 仅展示一次，请妥善保存"
                        </div>
                        <code class="mono" style="font-size:13px;word-break:break-all;color:var(--ink)">{t}</code>
                    </div>
                })}
            </Card>

            <div style="margin-top:24px"></div>

            <Card title="已生成 Tokens" code="CFG-SEC-LST">
                <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:120px">"loading…"</div> }>
                    {move || pats.get().map(|res| match res {
                        Err(e) => view! { <p>"加载失败 · " {e.to_string()}</p> }.into_any(),
                        Ok(rows) if rows.is_empty() => view! { <p class="muted">"还没生成 token。在上方表单创建一个。"</p> }.into_any(),
                        Ok(rows) => view! {
                            <table class="tbl">
                                <thead>
                                    <tr>
                                        <th>"名称"</th>
                                        <th>"前缀"</th>
                                        <th>"Scopes"</th>
                                        <th>"创建"</th>
                                        <th>"最近使用"</th>
                                        <th>"过期"</th>
                                        <th>"状态"</th>
                                        <th class="num">"操作"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {rows.into_iter().map(|p| {
                                        let id = p.id;
                                        let revoked = p.is_revoked;
                                        let expired = p.is_expired;
                                        let (status_tone, status_label) = if revoked {
                                            (ep_core::Tone::Rose, "已撤销")
                                        } else if expired {
                                            (ep_core::Tone::Amber, "已过期")
                                        } else {
                                            (ep_core::Tone::Green, "有效")
                                        };
                                        let created = ep_core::fmt_ts_date(Some(p.created_at));
                                        let last_used = ep_core::fmt_ts_date(p.last_used_at);
                                        let expires = p.expires_at.map(|e| ep_core::fmt_ts_date(Some(e))).unwrap_or_else(|| "永久".into());
                                        view! {
                                            <tr>
                                                <td>{p.name}</td>
                                                <td class="doc">{p.prefix}</td>
                                                <td class="mono" style="font-size:11px">{p.scopes}</td>
                                                <td class="mono dim" style="font-size:11px">{created}</td>
                                                <td class="mono dim" style="font-size:11px">{last_used}</td>
                                                <td class="mono dim" style="font-size:11px">{expires}</td>
                                                <td><Tag tone=status_tone>{status_label}</Tag></td>
                                                <td class="num">
                                                    {(!revoked).then(|| view! {
                                                        <ActionForm action=revoke attr:style="display:inline">
                                                            <input type="hidden" name="id" value=id/>
                                                            <button class="btn sm" type="submit"
                                                                    style="color:var(--rose-ink)"
                                                                    onclick="return confirm('撤销该 token？')">"撤销"</button>
                                                        </ActionForm>
                                                    })}
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

