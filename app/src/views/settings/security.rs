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
pub struct PatDto {
    pub id: i64,
    pub name: String,
    pub prefix: String,
    pub scopes: String,
    pub created_local: String,
    pub expires_local: Option<String>,
    pub last_used_local: String,
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

#[derive(Clone, Copy)]
struct ScopeOption {
    scope: &'static str,
    label_key: &'static str,
}

fn scope_options() -> Vec<ScopeOption> {
    let mut options = Vec::with_capacity(crate::modules::MODULES.len() * 2 + 2);
    for module in crate::modules::MODULES {
        options.push(ScopeOption {
            scope: module.descriptor.read_scope,
            label_key: module.descriptor.read_scope_label_key,
        });
        options.push(ScopeOption {
            scope: module.descriptor.write_scope,
            label_key: module.descriptor.write_scope_label_key,
        });
    }
    options.push(ScopeOption {
        scope: ep_core::SCOPE_NOTIFICATIONS_WRITE,
        label_key: "app.settings.security.scope.notify_write",
    });
    options.push(ScopeOption {
        scope: ep_core::SCOPE_ALL,
        label_key: "app.settings.security.scope.all",
    });
    options
}

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

    let allowed_scopes = scope_options();
    let mut scope_vec: Vec<String> = Vec::new();
    for scope in scopes.split_whitespace() {
        if !allowed_scopes.iter().any(|option| option.scope == scope) {
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
        let timezone = st.timezone();
        let rows = ep_auth::list_pats(&st.db).await.map_err(server_err)?;
        let now = ep_core::unix_now();
        Ok(rows
            .into_iter()
            .map(|r| {
                let is_revoked = r.revoked_at.is_some();
                let is_expired = r.expires_at.map(|e| e <= now).unwrap_or(false);
                let created_local = timezone.fmt_date(Some(r.created_at));
                let expires_local = r.expires_at.map(|ts| timezone.fmt_date(Some(ts)));
                let last_used_local = timezone.fmt_date(r.last_used_at);
                PatDto {
                    id: r.id,
                    name: r.name,
                    prefix: r.prefix,
                    scopes: r.scopes,
                    created_local,
                    expires_local,
                    last_used_local,
                    is_expired,
                    is_revoked,
                }
            })
            .collect())
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
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
        let timezone = st.timezone();
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
        if expires_at.is_some_and(|timestamp| !ep_core::is_valid_app_timestamp(timestamp)) {
            return Err(ep_i18n::err("app.settings.security.err_expires_too_large"));
        }
        let (token, row) = ep_auth::generate_pat(&st.db, &name, &scope_refs, expires_at)
            .await
            .map_err(server_err)?;
        let is_expired = row.expires_at.map(|e| e <= now).unwrap_or(false);
        let created_local = timezone.fmt_date(Some(row.created_at));
        let expires_local = row.expires_at.map(|ts| timezone.fmt_date(Some(ts)));
        let last_used_local = timezone.fmt_date(row.last_used_at);
        Ok(GeneratedPat {
            token,
            row: PatDto {
                id: row.id,
                name: row.name,
                prefix: row.prefix,
                scopes: row.scopes,
                created_local,
                expires_local,
                last_used_local,
                is_expired,
                is_revoked: false,
            },
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
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
        Err(ep_core::server_err("ssr-only"))
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
        if ep_auth::validate_password(&current).is_err() {
            return Err(ep_i18n::err("app.settings.security.err_current_password"));
        }
        if ep_auth::validate_password(&new).is_err()
            || ep_auth::validate_password(&confirm).is_err()
        {
            return Err(ep_i18n::err("app.settings.security.err_password_length"));
        }
        if new != confirm {
            return Err(ep_i18n::err("app.settings.security.err_password_confirm"));
        }
        if new == current {
            return Err(ep_i18n::err("app.settings.security.err_password_same"));
        }
        let st = ep_core::app_state_context()?;
        change_password_inner(&st.db, current, new).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
async fn change_password_inner(
    pool: &sqlx::SqlitePool,
    current: String,
    new: String,
) -> Result<(), ServerFnError> {
    let current_hash: String =
        sqlx::query_scalar("SELECT password_hash FROM app_user WHERE id = 1")
            .fetch_one(pool)
            .await
            .map_err(server_err)?;
    let ok = ep_auth::verify_password_async(current, current_hash.clone())
        .await
        .map_err(server_err)?;
    if !ok {
        return Err(ep_i18n::err("app.settings.security.err_current_password"));
    }
    let new_hash = ep_auth::hash_password_async(new)
        .await
        .map_err(server_err)?;
    commit_password_change(pool, &current_hash, &new_hash).await
}

/// Atomically publish a newly-computed password hash and revoke every cookie
/// session. The old hash is part of the UPDATE predicate: if another password
/// change won while Argon2 was running, this request fails instead of silently
/// overwriting that newer credential.
#[cfg(feature = "ssr")]
async fn commit_password_change(
    pool: &sqlx::SqlitePool,
    expected_hash: &str,
    new_hash: &str,
) -> Result<(), ServerFnError> {
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let updated =
        sqlx::query("UPDATE app_user SET password_hash = ?1 WHERE id = 1 AND password_hash = ?2")
            .bind(new_hash)
            .bind(expected_hash)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
    if updated.rows_affected() != 1 {
        return Err(ep_i18n::err("app.settings.security.err_current_password"));
    }
    ep_auth::purge_all_sessions(&mut *tx)
        .await
        .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(())
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    async fn password_test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("password test pool");
        sqlx::query(
            "CREATE TABLE app_user (
                id INTEGER PRIMARY KEY,
                password_hash TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("app_user schema");
        sqlx::query(
            "CREATE TABLE session (
                token TEXT PRIMARY KEY,
                user_id INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("session schema");
        sqlx::query("INSERT INTO app_user (id, password_hash) VALUES (1, 'old-hash')")
            .execute(&pool)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO session (token, user_id) VALUES ('session-1', 1)")
            .execute(&pool)
            .await
            .expect("seed session");
        pool
    }

    #[tokio::test]
    async fn concurrent_password_cas_allows_exactly_one_winner() {
        let pool = password_test_pool().await;

        let (first, second) = tokio::join!(
            commit_password_change(&pool, "old-hash", "new-hash-a"),
            commit_password_change(&pool, "old-hash", "new-hash-b"),
        );

        assert_ne!(first.is_ok(), second.is_ok(), "exactly one CAS must win");
        let stored: String = sqlx::query_scalar("SELECT password_hash FROM app_user WHERE id = 1")
            .fetch_one(&pool)
            .await
            .expect("stored hash");
        assert!(matches!(stored.as_str(), "new-hash-a" | "new-hash-b"));
        let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session")
            .fetch_one(&pool)
            .await
            .expect("session count");
        assert_eq!(sessions, 0);
    }

    #[tokio::test]
    async fn failed_session_purge_rolls_back_password_cas() {
        let pool = password_test_pool().await;
        sqlx::query(
            "CREATE TRIGGER reject_session_purge
             BEFORE DELETE ON session
             BEGIN
               SELECT RAISE(ABORT, 'session purge rejected');
             END",
        )
        .execute(&pool)
        .await
        .expect("reject trigger");

        commit_password_change(&pool, "old-hash", "new-hash")
            .await
            .expect_err("trigger must abort the transaction");

        let stored: String = sqlx::query_scalar("SELECT password_hash FROM app_user WHERE id = 1")
            .fetch_one(&pool)
            .await
            .expect("stored hash");
        assert_eq!(stored, "old-hash");
        let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session")
            .fetch_one(&pool)
            .await
            .expect("session count");
        assert_eq!(sessions, 1);
    }

    #[test]
    fn normalize_pat_input_trims_and_dedupes_scopes() {
        let (name, scopes, expires_days) = normalize_pat_input(
            "  iOS Shortcuts  ".into(),
            format!(
                "{} {} {} {} {}",
                ep_finance::DESCRIPTOR.read_scope,
                ep_finance::DESCRIPTOR.read_scope,
                ep_core::SCOPE_NOTIFICATIONS_WRITE,
                ep_fitness::DESCRIPTOR.read_scope,
                ep_journal::DESCRIPTOR.read_scope
            ),
            " 30 ".into(),
        )
        .expect("valid PAT input");

        assert_eq!(name, "iOS Shortcuts");
        assert_eq!(
            scopes,
            vec![
                ep_finance::DESCRIPTOR.read_scope,
                ep_core::SCOPE_NOTIFICATIONS_WRITE,
                ep_fitness::DESCRIPTOR.read_scope,
                ep_journal::DESCRIPTOR.read_scope
            ]
        );
        assert_eq!(expires_days, Some(30));
    }

    #[test]
    fn all_scope_options_derive_from_module_catalog() {
        let options: Vec<&str> = scope_options()
            .into_iter()
            .map(|option| option.scope)
            .collect();
        assert_eq!(
            options.len(),
            crate::modules::MODULES.len().saturating_mul(2) + 2
        );
        for module in crate::modules::MODULES {
            assert!(options.contains(&module.descriptor.read_scope));
            assert!(options.contains(&module.descriptor.write_scope));
        }
        assert!(options.contains(&ep_core::SCOPE_NOTIFICATIONS_WRITE));
        assert!(options.contains(&ep_core::SCOPE_ALL));
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
            ep_finance::DESCRIPTOR.read_scope.into(),
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
            scopes: ep_finance::DESCRIPTOR.read_scope.into(),
            created_local: "1970-01-01".into(),
            expires_local: None,
            last_used_local: "—".into(),
            is_expired: false,
            is_revoked: false,
        };

        let value = serde_json::to_value(dto).expect("serialize PatDto");

        assert!(value.get("prefix").is_some());
        assert!(value.get("hash").is_none());
        assert!(value.get("token").is_none());
        assert!(value.get("password_hash").is_none());
        assert!(value.get("created_at").is_none());
        assert!(value.get("expires_at").is_none());
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

    // A token is only recoverable from this action result. Hide the previous
    // plaintext as soon as a new generation starts, and keep the submit button
    // disabled until that request finishes so concurrent results cannot
    // overwrite one another and leave an undisclosed valid token behind.
    Effect::new(move |was_pending: Option<bool>| {
        let pending = generate.pending().get();
        if pending && was_pending == Some(false) {
            generate.clear();
        }
        pending
    });

    let new_token = move || generate.value().get().and_then(|r| r.ok()).map(|g| g.token);
    let locale = use_locale();
    let scope_options = scope_options();
    let default_scopes = scope_options
        .iter()
        .filter(|option| option.scope != ep_core::SCOPE_ALL)
        .map(|option| option.scope)
        .collect::<Vec<_>>()
        .join(" ");

    view! {
        <div class="view">
            <PageHead
                module=t(locale, "app.settings.security.page.module")
                title=t(locale, "app.settings.security.page.title")
                title_cn=t(locale, "app.settings.security.page.title_cn")
                sub=t(locale, "app.settings.security.page.sub")
            />

            <Card title=t(locale, "app.settings.security.form.password_title")
                  sub=t(locale, "app.settings.security.form.password_sub")>
                <ActionForm action=change attr:class="vstack" attr:style="gap:10px">
                    <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="ep-field-label">{t(locale, "app.settings.security.field.current_password")}</span>
                            <input name="current" type="password" required
                                   maxlength=ep_core::MAX_PASSWORD_BYTES.to_string() autocomplete="current-password"
                                   class="ep-input"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="ep-field-label">{t(locale, "app.settings.security.field.new_password")}</span>
                            <input name="new" type="password" required
                                   minlength=ep_core::MIN_PASSWORD_CHARS.to_string()
                                   maxlength=ep_core::MAX_PASSWORD_BYTES.to_string() autocomplete="new-password"
                                   class="ep-input"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="ep-field-label">{t(locale, "app.settings.security.field.confirm_new")}</span>
                            <input name="confirm" type="password" required
                                   minlength=ep_core::MIN_PASSWORD_CHARS.to_string()
                                   maxlength=ep_core::MAX_PASSWORD_BYTES.to_string() autocomplete="new-password"
                                   class="ep-input"/>
                        </label>
                    </div>
                    <div class="hstack" style="gap:8px;align-items:center">
                        <button class="btn primary" type="submit"
                                disabled=move || change.pending().get()
                                aria-busy=move || change.pending().get().to_string()>
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

            <Card title=t(locale, "app.settings.security.form.pat_title")>
                <ActionForm action=generate attr:class="vstack" attr:style="gap:10px">
                    <div style="display:grid;grid-template-columns:2fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="ep-field-label">{t(locale, "app.settings.security.field.name")}</span>
                            <input name="name" required placeholder=t(locale, "app.settings.security.placeholder.name")
                                   class="ep-input"/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="ep-field-label">{t(locale, "app.settings.security.field.expires_days")}</span>
                            <input name="expires_days" type="number" min="1" placeholder="365"
                                   class="ep-input mono"/>
                        </label>
                    </div>
                    <fieldset style="border:1px solid var(--border);border-radius:8px;padding:8px 12px">
                        <legend class="ep-field-label" style="padding:0 4px">{t(locale, "app.settings.security.field.scopes")}</legend>
                        <input name="scopes" required placeholder=default_scopes.clone() value=default_scopes
                               class="ep-input mono" style="width:100%"/>
                        <div class="hstack" style="gap:8px;margin-top:8px;flex-wrap:wrap;font-size:11px;color:var(--ink-3)">
                            {scope_options.into_iter().map(|option| view! {
                                <span class="mono">{format!("{} · {}", option.scope, t(locale, option.label_key))}</span>
                            }).collect_view()}
                        </div>
                    </fieldset>
                    <div class="hstack" style="gap:8px">
                        <button class="btn primary" type="submit"
                                disabled=move || generate.pending().get()
                                aria-busy=move || generate.pending().get().to_string()>
                            <Icon kind=IconKind::Plus size=14/>{t(locale, "app.settings.security.btn.generate")}
                        </button>
                        // ErrorSlot keeps a stable DOM anchor next to the
                        // ActionForm in both SSR and hydrate trees.
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

            <Card title=t(locale, "app.settings.security.list.title")>
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
                                        let created = p.created_local;
                                        let last_used = p.last_used_local;
                                        let expires = p.expires_local.unwrap_or_else(|| t(locale, "app.settings.security.never").into());
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
