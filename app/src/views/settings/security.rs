use ep_core::IconKind;
use ep_ui::{Card, Icon, PageHead, RowDeleteAction, Tag};
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

#[server(ChangePassword, "/api/_internal/cfg", "Url", "change_password")]
pub async fn change_password(
    current: String,
    new: String,
    confirm: String,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        if new != confirm {
            return Err(ServerFnError::Args("两次输入的新密码不一致".into()));
        }
        if new.chars().count() < 6 {
            return Err(ServerFnError::Args("新密码至少 6 个字符".into()));
        }
        if new == current {
            return Err(ServerFnError::Args("新密码与当前密码相同".into()));
        }
        let st: ep_core::AppState = expect_context();
        let (current_hash,): (String,) = sqlx::query_as("SELECT password_hash FROM app_user WHERE id = 1")
            .fetch_one(&st.db).await.map_err(server_err)?;
        let ok = ep_auth::verify_password_async(current, current_hash)
            .await.map_err(server_err)?;
        if !ok {
            return Err(ServerFnError::Args("当前密码错误".into()));
        }
        let new_hash = ep_auth::hash_password_async(new).await.map_err(server_err)?;
        // UPDATE + session purge in one tx so a pre-rotation cookie can never
        // outlive the new credential. Includes the caller's own cookie — the
        // sub-text on the Card warns the user that re-login is required.
        let mut tx = st.db.begin().await.map_err(server_err)?;
        sqlx::query("UPDATE app_user SET password_hash = ?1 WHERE id = 1")
            .bind(&new_hash).execute(&mut *tx).await.map_err(server_err)?;
        ep_auth::purge_all_sessions(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(())
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
    let change = ServerAction::<ChangePassword>::new();

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
                module="SETTINGS · 安全"
                title="Security"
                title_cn="账户安全 · API"
                sub="修改登录密码，管理 /api/v1/* 的 Personal Access Tokens。"
            />

            <Card title="修改密码" code="CFG-SEC-PWD"
                  sub="提交后所有会话（含本设备）都会失效，需要重新登录。">
                <ActionForm action=change attr:class="vstack" attr:style="gap:10px">
                    <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"当前密码"</span>
                            <input name="current" type="password" required autocomplete="current-password"
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"新密码 (≥ 6)"</span>
                            <input name="new" type="password" required minlength="6" autocomplete="new-password"
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"确认新密码"</span>
                            <input name="confirm" type="password" required minlength="6" autocomplete="new-password"
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        </label>
                    </div>
                    <div class="hstack" style="gap:8px;align-items:center">
                        <button class="btn primary" type="submit">
                            <Icon kind=IconKind::Check size=14/>"修改密码"
                        </button>
                        <span class="error-slot">
                            {move || match change.value().get() {
                                Some(Ok(_)) => view! {
                                    <span class="tag green">
                                        "✓ 已修改 · 所有会话已失效，"
                                        <a href="/login" style="color:inherit;text-decoration:underline">"重新登录"</a>
                                    </span>
                                }.into_any(),
                                Some(Err(e)) => view! {
                                    <span class="tag rose">{e.to_string()}</span>
                                }.into_any(),
                                None => ().into_any(),
                            }}
                        </span>
                    </div>
                </ActionForm>
            </Card>

            <div style="margin-top:24px"></div>

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
                        // See docs/follow-ups.md #26 — sibling <ActionForm> rewrites
                        // the slot in hydrate, breaking text-node walking unless
                        // the conditional view is anchored in a stable wrapper.
                        <span class="error-slot">
                            {move || generate.value().get().and_then(|r| r.err()).map(|e| view! {
                                <span class="tag rose">{e.to_string()}</span>
                            })}
                        </span>
                    </div>
                </ActionForm>

                <div class="new-token-slot">
                    {move || new_token().map(|t| view! {
                        <div style="margin-top:14px;padding:14px;border:1px solid var(--primary);border-radius:10px;background:var(--primary-soft)">
                            <div class="mono" style="font-size:11px;color:var(--primary-ink);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">
                                "✓ Token · 仅展示一次，请妥善保存"
                            </div>
                            <code class="mono" style="font-size:13px;word-break:break-all;color:var(--ink)">{t}</code>
                        </div>
                    })}
                </div>
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
                                                        <RowDeleteAction action=revoke value=id.to_string()
                                                                         field="id" confirm="撤销该 token？" label="撤销"/>
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

