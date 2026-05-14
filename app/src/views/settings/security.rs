use ep_core::IconKind;
use ep_i18n::{server_fn_error_text, t, use_locale};
use ep_ui::{Card, ErrorSlot, Icon, PageHead, RowDeleteAction, Tag};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use ep_core::server_err;

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

const ALL_SCOPES: &[(&str, &str)] = &[
    (
        ep_core::SCOPE_ACTIVITY_READ,
        "app.settings.security.scope.activity_read",
    ),
    (
        ep_core::SCOPE_FIN_READ,
        "app.settings.security.scope.fin_read",
    ),
    (
        ep_core::SCOPE_FIN_WRITE,
        "app.settings.security.scope.fin_write",
    ),
    (
        ep_core::SCOPE_FIT_READ,
        "app.settings.security.scope.fit_read",
    ),
    (
        ep_core::SCOPE_FIT_WRITE,
        "app.settings.security.scope.fit_write",
    ),
    (
        ep_core::SCOPE_LRN_READ,
        "app.settings.security.scope.lrn_read",
    ),
    (
        ep_core::SCOPE_LRN_WRITE,
        "app.settings.security.scope.lrn_write",
    ),
    (
        ep_core::SCOPE_NOTIFY_WRITE,
        "app.settings.security.scope.notify_write",
    ),
    (ep_core::SCOPE_ALL, "app.settings.security.scope.all"),
];

#[cfg(feature = "ssr")]
fn normalize_pat_input(
    name: String,
    scopes: String,
    expires_days: String,
) -> Result<(String, Vec<String>, Option<i64>), ServerFnError> {
    let name = ep_core::trim_to_option(&name)
        .ok_or_else(|| ep_i18n::err("app.settings.security.err_pat_name_required"))?;
    if name.chars().count() > ep_auth::MAX_PAT_NAME_CHARS {
        return Err(ep_i18n::err_with(
            "app.settings.security.err_pat_name_too_long",
            ep_auth::MAX_PAT_NAME_CHARS,
        ));
    }

    let mut scope_vec: Vec<String> = Vec::new();
    for scope in scopes.split_whitespace() {
        if !ALL_SCOPES.iter().any(|(allowed, _)| *allowed == scope) {
            return Err(ep_i18n::err_with(
                "app.settings.security.err_scope_unknown",
                scope,
            ));
        }
        if !scope_vec.iter().any(|existing| existing == scope) {
            scope_vec.push(scope.to_string());
        }
    }
    if scope_vec.is_empty() {
        return Err(ep_i18n::err("app.settings.security.err_scope_required"));
    }

    let trimmed = expires_days.trim();
    let expires_days = if trimmed.is_empty() {
        None
    } else {
        let days = trimmed
            .parse::<i64>()
            .map_err(|_| ep_i18n::err("app.settings.security.err_expires_integer"))?;
        if days <= 0 {
            return Err(ep_i18n::err("app.settings.security.err_expires_positive"));
        }
        Some(days)
    };

    Ok((name, scope_vec, expires_days))
}

#[cfg(feature = "ssr")]
fn normalize_pat_id(id: i64) -> Result<i64, ServerFnError> {
    if id > 0 {
        Ok(id)
    } else {
        Err(ep_i18n::err_with(
            "app.settings.security.err_pat_not_found",
            id.to_string(),
        ))
    }
}

#[server(ListPats, "/api/_internal/cfg", "Url", "list_pats")]
pub async fn list_pats() -> Result<Vec<PatDto>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;
        let rows = ep_auth::list_pats(&st.db).await.map_err(server_err)?;
        let now = ep_core::unix_now();
        Ok(rows
            .into_iter()
            .map(|r| {
                let is_revoked = r.revoked_at.is_some();
                let is_expired = r.expires_at.map(|e| e <= now).unwrap_or(false);
                PatDto {
                    id: r.id,
                    name: r.name,
                    prefix: r.prefix,
                    scopes: r.scopes,
                    created_at: r.created_at,
                    expires_at: r.expires_at,
                    last_used_at: r.last_used_at,
                    revoked_at: r.revoked_at,
                    is_expired,
                    is_revoked,
                }
            })
            .collect())
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
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
        let (name, scope_vec, expires_days) = normalize_pat_input(name, scopes, expires_days)?;
        let st = ep_core::app_state_context()?;
        let scope_refs: Vec<&str> = scope_vec.iter().map(String::as_str).collect();
        let now = ep_core::unix_now();
        let expires_at =
            match expires_days {
                Some(d) => Some(
                    now.checked_add(d.checked_mul(86_400).ok_or_else(|| {
                        ep_i18n::err("app.settings.security.err_expires_too_large")
                    })?)
                    .ok_or_else(|| ep_i18n::err("app.settings.security.err_expires_too_large"))?,
                ),
                None => None,
            };
        let (token, row) = ep_auth::generate_pat(&st.db, &name, &scope_refs, expires_at)
            .await
            .map_err(server_err)?;
        let is_expired = row.expires_at.map(|e| e <= now).unwrap_or(false);
        Ok(GeneratedPat {
            token,
            row: PatDto {
                id: row.id,
                name: row.name,
                prefix: row.prefix,
                scopes: row.scopes,
                created_at: row.created_at,
                expires_at: row.expires_at,
                last_used_at: row.last_used_at,
                revoked_at: row.revoked_at,
                is_expired,
                is_revoked: false,
            },
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[server(RevokePat, "/api/_internal/cfg", "Url", "revoke_pat")]
pub async fn revoke_pat(id: i64) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let id = normalize_pat_id(id)?;
        let st = ep_core::app_state_context()?;
        let revoked = ep_auth::revoke_pat(&st.db, id).await.map_err(server_err)?;
        if revoked {
            Ok(())
        } else {
            Err(ep_i18n::err_with(
                "app.settings.security.err_pat_not_found",
                id.to_string(),
            ))
        }
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
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
            return Err(ep_i18n::err("app.settings.security.err_password_confirm"));
        }
        if new.chars().count() < 6 {
            return Err(ep_i18n::err("app.settings.security.err_password_length"));
        }
        if new == current {
            return Err(ep_i18n::err("app.settings.security.err_password_same"));
        }
        let st = ep_core::app_state_context()?;
        let (current_hash,): (String,) =
            sqlx::query_as("SELECT password_hash FROM app_user WHERE id = 1")
                .fetch_one(&st.db)
                .await
                .map_err(server_err)?;
        let ok = ep_auth::verify_password_async(current, current_hash)
            .await
            .map_err(server_err)?;
        if !ok {
            return Err(ep_i18n::err("app.settings.security.err_current_password"));
        }
        let new_hash = ep_auth::hash_password_async(new)
            .await
            .map_err(server_err)?;
        // UPDATE + session purge in one tx so a pre-rotation cookie can never
        // outlive the new credential. Includes the caller's own cookie — the
        // sub-text on the Card warns the user that re-login is required.
        let mut tx = st.db.begin().await.map_err(server_err)?;
        sqlx::query("UPDATE app_user SET password_hash = ?1 WHERE id = 1")
            .bind(&new_hash)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
        ep_auth::purge_all_sessions(&mut *tx)
            .await
            .map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    #[test]
    fn normalize_pat_input_trims_and_dedupes_scopes() {
        let (name, scopes, expires_days) = normalize_pat_input(
            "  iOS Shortcuts  ".into(),
            format!(
                "{} {} {} {}",
                ep_core::SCOPE_ACTIVITY_READ,
                ep_core::SCOPE_FIN_READ,
                ep_core::SCOPE_FIN_READ,
                ep_core::SCOPE_NOTIFY_WRITE
            ),
            " 30 ".into(),
        )
        .expect("valid PAT input");

        assert_eq!(name, "iOS Shortcuts");
        assert_eq!(
            scopes,
            vec![
                ep_core::SCOPE_ACTIVITY_READ,
                ep_core::SCOPE_FIN_READ,
                ep_core::SCOPE_NOTIFY_WRITE
            ]
        );
        assert_eq!(expires_days, Some(30));
    }

    #[test]
    fn all_scope_options_match_core_scope_set() {
        let options: Vec<&str> = ALL_SCOPES.iter().map(|(scope, _)| *scope).collect();

        assert_eq!(options, ep_core::PAT_SCOPES);
    }

    #[test]
    fn normalize_pat_input_rejects_unknown_scope() {
        let err = normalize_pat_input("api".into(), "unknown:write".into(), "".into())
            .expect_err("stale scope should fail");

        assert_eq!(
            ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("app.settings.security.err_scope_unknown", "unknown:write"))
        );
    }

    #[test]
    fn normalize_pat_input_requires_scope() {
        let err = normalize_pat_input("api".into(), "   ".into(), "".into())
            .expect_err("empty scopes fail");

        assert_eq!(
            ep_i18n::parse_err(&err).map(|(code, _)| code),
            Some("app.settings.security.err_scope_required")
        );
    }

    #[test]
    fn normalize_pat_input_rejects_overlong_name() {
        let err = normalize_pat_input(
            "x".repeat(ep_auth::MAX_PAT_NAME_CHARS + 1),
            ep_core::SCOPE_FIN_READ.into(),
            "".into(),
        )
        .expect_err("overlong name should fail");

        assert_eq!(
            ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("app.settings.security.err_pat_name_too_long", "64"))
        );
    }

    #[test]
    fn normalize_pat_id_rejects_non_positive_ids() {
        assert_eq!(normalize_pat_id(7).unwrap(), 7);

        let err = normalize_pat_id(0).expect_err("invalid id");
        assert_eq!(
            ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("app.settings.security.err_pat_not_found", "0"))
        );
    }

    #[test]
    fn pat_list_dto_never_serializes_secret_material() {
        let dto = PatDto {
            id: 1,
            name: "iOS Shortcuts".into(),
            prefix: "ep_pat_ABCDE".into(),
            scopes: ep_core::SCOPE_FIN_READ.into(),
            created_at: 1,
            expires_at: None,
            last_used_at: None,
            revoked_at: None,
            is_expired: false,
            is_revoked: false,
        };

        let value = serde_json::to_value(dto).expect("serialize PatDto");

        assert!(value.get("prefix").is_some());
        assert!(value.get("hash").is_none());
        assert!(value.get("token").is_none());
        assert!(value.get("password_hash").is_none());
    }
}

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
    let locale = use_locale();

    view! {
        <div class="view">
            <PageHead
                code="CFG-SEC-01"
                module=t(locale, "app.settings.security.page.module")
                title=t(locale, "app.settings.security.page.title")
                title_cn=t(locale, "app.settings.security.page.title_cn")
                sub=t(locale, "app.settings.security.page.sub")
            />

            <Card title=t(locale, "app.settings.security.form.password_title") code="CFG-SEC-PWD"
                  sub=t(locale, "app.settings.security.form.password_sub")>
                <ActionForm action=change attr:class="vstack" attr:style="gap:10px">
                    <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">{t(locale, "app.settings.security.field.current_password")}</span>
                            <input name="current" type="password" required autocomplete="current-password"
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">{t(locale, "app.settings.security.field.new_password")}</span>
                            <input name="new" type="password" required minlength="6" autocomplete="new-password"
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">{t(locale, "app.settings.security.field.confirm_new")}</span>
                            <input name="confirm" type="password" required minlength="6" autocomplete="new-password"
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        </label>
                    </div>
                    <div class="hstack" style="gap:8px;align-items:center">
                        <button class="btn primary" type="submit">
                            <Icon kind=IconKind::Check size=14/>{t(locale, "app.settings.security.btn.change_password")}
                        </button>
                        <span class="error-slot">
                            {move || match change.value().get() {
                                Some(Ok(_)) => view! {
                                    <span class="tag green">
                                        {t(locale, "app.settings.security.changed_prefix")}
                                        <a href="/login" style="color:inherit;text-decoration:underline">{t(locale, "app.settings.security.relogin")}</a>
                                    </span>
                                }.into_any(),
                                Some(Err(e)) => view! {
                                    <span class="tag rose">{server_fn_error_text(&e)}</span>
                                }.into_any(),
                                None => ().into_any(),
                            }}
                        </span>
                    </div>
                </ActionForm>
            </Card>

            <div style="margin-top:24px"></div>

            <Card title=t(locale, "app.settings.security.form.pat_title") code="CFG-SEC-NEW">
                <ActionForm action=generate attr:class="vstack" attr:style="gap:10px">
                    <div style="display:grid;grid-template-columns:2fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">{t(locale, "app.settings.security.field.name")}</span>
                            <input name="name" required placeholder=t(locale, "app.settings.security.placeholder.name")
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">{t(locale, "app.settings.security.field.expires_days")}</span>
                            <input name="expires_days" type="number" min="1" placeholder="365"
                                   style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono)"/>
                        </label>
                    </div>
                    <fieldset style="border:1px solid var(--border);border-radius:8px;padding:8px 12px">
                        <legend class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em;padding:0 4px">{t(locale, "app.settings.security.field.scopes")}</legend>
                        <input name="scopes" required placeholder="activity:read fin:read fin:write fit:read fit:write lrn:read lrn:write notify:write" value="activity:read fin:read fin:write fit:read fit:write lrn:read lrn:write notify:write"
                               style="width:100%;padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono);font-size:12px"/>
                        <div class="hstack" style="gap:8px;margin-top:8px;flex-wrap:wrap;font-size:11px;color:var(--ink-3)">
                            {ALL_SCOPES.iter().map(|(scope, label_key)| view! {
                                <span class="mono">{format!("{} · {}", scope, t(locale, label_key))}</span>
                            }).collect_view()}
                        </div>
                    </fieldset>
                    <div class="hstack" style="gap:8px">
                        <button class="btn primary" type="submit">
                            <Icon kind=IconKind::Plus size=14/>{t(locale, "app.settings.security.btn.generate")}
                        </button>
                        // ErrorSlot supplies the stable `error-slot` wrapper that
                        // anchors text-node hydrate walking next to the sibling
                        // <ActionForm>. See docs/follow-ups.md #26.
                        <ErrorSlot action=generate/>
                    </div>
                </ActionForm>

                <div class="new-token-slot">
                    {move || new_token().map(|token| view! {
                        <div style="margin-top:14px;padding:14px;border:1px solid var(--primary);border-radius:10px;background:var(--primary-soft)">
                            <div class="mono" style="font-size:11px;color:var(--primary-ink);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">
                                {t(locale, "app.settings.security.token_once")}
                            </div>
                            <code class="mono" style="font-size:13px;word-break:break-all;color:var(--ink)">{token}</code>
                        </div>
                    })}
                </div>
            </Card>

            <div style="margin-top:24px"></div>

            <Card title=t(locale, "app.settings.security.list.title") code="CFG-SEC-LST">
                <Suspense fallback=move || view! {
                    <div style="display:flex;flex-direction:column;gap:10px;padding:6px 0">
                        <span class="skeleton-line" style="height:14px;width:50%;display:block"></span>
                        <span class="skeleton-line" style="height:14px;display:block"></span>
                        <span class="skeleton-line" style="height:14px;display:block"></span>
                    </div>
                }>
                    {move || pats.get().map(|res| match res {
                        Err(e) => view! { <p>{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</p> }.into_any(),
                        Ok(rows) if rows.is_empty() => view! {
                            <ep_ui::EmptyState
                                icon=ep_core::IconKind::Settings
                                title=t(locale, "app.settings.security.list.empty_title")
                                desc=t(locale, "app.settings.security.list.empty")
                                code="CFG-SEC-EMPTY"
                                compact=true
                            />
                        }.into_any(),
                        Ok(rows) => view! {
                            <table class="tbl">
                                <thead>
                                    <tr>
                                        <th>{t(locale, "app.settings.security.field.name")}</th>
                                        <th>{t(locale, "app.settings.security.field.prefix")}</th>
                                        <th>{t(locale, "app.settings.security.field.scopes_short")}</th>
                                        <th>{t(locale, "app.settings.security.field.created")}</th>
                                        <th>{t(locale, "app.settings.security.field.last_used")}</th>
                                        <th>{t(locale, "app.settings.security.field.expires")}</th>
                                        <th>{t(locale, "app.settings.security.field.status")}</th>
                                        <th class="num">{t(locale, "app.settings.security.field.ops")}</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {rows.into_iter().map(|p| {
                                        let id = p.id;
                                        let revoked = p.is_revoked;
                                        let expired = p.is_expired;
                                        let (status_tone, status_label) = if revoked {
                                            (ep_core::Tone::Rose, t(locale, "app.settings.security.status.revoked"))
                                        } else if expired {
                                            (ep_core::Tone::Amber, t(locale, "app.settings.security.status.expired"))
                                        } else {
                                            (ep_core::Tone::Green, t(locale, "app.settings.security.status.active"))
                                        };
                                        let created = ep_core::fmt_ts_date(Some(p.created_at));
                                        let last_used = ep_core::fmt_ts_date(p.last_used_at);
                                        let expires = p.expires_at.map(|e| ep_core::fmt_ts_date(Some(e))).unwrap_or_else(|| t(locale, "app.settings.security.never").into());
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
                                                                         field="id" confirm=t(locale, "app.settings.security.confirm_revoke") label=t(locale, "app.settings.security.revoke")/>
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
