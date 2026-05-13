use ep_core::IconKind;
use ep_i18n::{server_fn_error_text, t, use_locale};
#[cfg(feature = "hydrate")]
use ep_ui::use_unread_signal;
use ep_ui::{Card, EmptyState, PageHead, Tag as UiTag};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRow {
    pub id: i64,
    pub created_at: i64,
    pub severity: String,
    pub module: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub link: Option<String>,
    pub doc_ref: Option<String>,
    pub read: bool,
}

#[cfg(feature = "ssr")]
type NotificationQueryRow = (
    i64,
    i64,
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
);

#[server(ListNotifications, "/api/_internal/cfg", "Url", "list_notifications")]
pub async fn list_notifications() -> Result<Vec<NotificationRow>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let rows: Vec<NotificationQueryRow> = sqlx::query_as(
            "SELECT id, created_at, severity, module, title, body, link, doc_ref, read_at
                   FROM notification ORDER BY created_at DESC LIMIT 100",
        )
        .fetch_all(&state.db)
        .await
        .map_err(ep_core::server_err)?;
        Ok(rows
            .into_iter()
            .map(|r| NotificationRow {
                id: r.0,
                created_at: r.1,
                severity: r.2,
                module: r.3,
                title: r.4,
                body: r.5,
                link: r.6,
                doc_ref: r.7,
                read: r.8.is_some(),
            })
            .collect())
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    UnreadNotificationCount,
    "/api/_internal/cfg",
    "Url",
    "unread_notification_count"
)]
pub async fn unread_notification_count() -> Result<u32, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        unread_notification_count_inner(&state.db)
            .await
            .map_err(ep_core::server_err)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
async fn unread_notification_count_inner(pool: &sqlx::SqlitePool) -> sqlx::Result<u32> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification WHERE read_at IS NULL")
        .fetch_one(pool)
        .await?;
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

#[server(
    MarkNotificationsRead,
    "/api/_internal/cfg",
    "Url",
    "mark_notifications_read"
)]
pub async fn mark_notifications_read() -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        sqlx::query("UPDATE notification SET read_at = unixepoch() WHERE read_at IS NULL")
            .execute(&state.db)
            .await
            .map_err(ep_core::server_err)?;
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[component]
pub fn NotificationsView() -> impl IntoView {
    let r = Resource::new(|| (), |_| async { list_notifications().await });
    let locale = use_locale();
    #[cfg(feature = "hydrate")]
    {
        let unread = use_unread_signal();
        Effect::new(move |prev: Option<()>| {
            if prev.is_none() {
                leptos::task::spawn_local(async move {
                    if mark_notifications_read().await.is_ok() {
                        unread.set(0);
                    }
                });
            }
        });
    }
    view! {
        <div class="view">
            <PageHead
                code="NOT-01".to_string()
                module=t(locale, "app.notifications.page.module")
                title=t(locale, "app.notifications.page.title")
                title_cn=t(locale, "app.notifications.page.title_cn")
            />
            <Card>
                <Suspense fallback=move || view! {
                    <span class="skeleton-line" style="height:18px;width:35%;margin-bottom:10px;display:block"></span>
                    <span class="skeleton-line" style="height:14px;margin-bottom:8px;display:block"></span>
                    <span class="skeleton-line" style="height:14px;margin-bottom:8px;display:block"></span>
                    <span class="skeleton-line" style="height:14px;display:block"></span>
                }>
                    {move || r.get().map(|res| match res {
                        Err(e) => view! { <p>{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</p> }.into_any(),
                        Ok(rows) if rows.is_empty() => view! {
                            <EmptyState
                                icon=IconKind::Bell
                                title=t(locale, "app.notifications.empty")
                                desc=t(locale, "app.notifications.empty_hint")
                                code="NOT-EMPTY"
                            />
                        }.into_any(),
                        Ok(rows) => view! {
                            <div class="vstack" style="gap:0">
                                {rows.into_iter().map(render_notification_row).collect_view()}
                            </div>
                        }.into_any(),
                    })}
                </Suspense>
            </Card>
        </div>
    }
}

fn render_notification_row(n: NotificationRow) -> AnyView {
    let tone = match n.severity.as_str() {
        "warn" => ep_ui::Tone::Amber,
        "crit" => ep_ui::Tone::Rose,
        _ => ep_ui::Tone::Green,
    };
    let severity = n.severity.to_uppercase();
    let when = ep_core::fmt_ts_minute(Some(n.created_at));
    let body = n.body.unwrap_or_default();
    let module = n.module;
    // `doc_ref` is internal correlation metadata; we kept it on the DTO so
    // notification triage tools / API consumers still see it, but the
    // notification UI is for the human reader and never surfaces it.
    let _ = n.doc_ref;
    let href = n
        .link
        .as_deref()
        .and_then(safe_notification_link)
        .map(str::to_string);
    let content = view! {
        <UiTag tone=tone>{severity}</UiTag>
        <div>
            <div class="title">{n.title}</div>
            <div class="meta">
                {body}
                <span class="mono dim" style="margin-left:8px">"· " {when}</span>
                {module.map(|m| view! { <span class="mono dim" style="margin-left:8px">"· " {m}</span> })}
            </div>
        </div>
        <div></div>
    };
    match href {
        Some(href) => {
            view! { <a class="list-row list-row-link" href=href>{content}</a> }.into_any()
        }
        None => view! { <div class="list-row">{content}</div> }.into_any(),
    }
}

fn safe_notification_link(raw: &str) -> Option<&str> {
    ep_core::safe_in_app_path(raw)
}

#[cfg(test)]
mod tests {
    use super::safe_notification_link;

    #[test]
    fn safe_notification_link_accepts_local_paths() {
        assert_eq!(safe_notification_link(" /finance "), Some("/finance"));
        assert_eq!(
            safe_notification_link("/settings/security?tab=pat"),
            Some("/settings/security?tab=pat")
        );
    }

    #[test]
    fn safe_notification_link_rejects_external_or_control_paths() {
        for raw in [
            "",
            "https://example.com",
            "//example.com",
            "javascript:alert(1)",
            "/finance\\evil",
            "/finance%0d%0aevil",
            "/finance%7F",
        ] {
            assert_eq!(safe_notification_link(raw), None, "raw={raw}");
        }
    }

    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn unread_notification_count_inner_counts_only_unread_rows() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::query(
            "CREATE TABLE notification (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                read_at INTEGER
            )",
        )
        .execute(&pool)
        .await
        .expect("schema");
        sqlx::query(
            "INSERT INTO notification (title, read_at) VALUES
             ('unread-a', NULL),
             ('read', 123),
             ('unread-b', NULL)",
        )
        .execute(&pool)
        .await
        .expect("seed");

        assert_eq!(
            super::unread_notification_count_inner(&pool)
                .await
                .expect("count"),
            2
        );
    }
}
