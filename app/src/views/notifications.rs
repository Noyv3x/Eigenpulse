use ep_i18n::{t, use_locale};
use ep_ui::{Card, PageHead};
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
        fn err(msg: String) -> ServerFnError {
            ServerFnError::ServerError(msg)
        }
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        let rows: Vec<NotificationQueryRow> = sqlx::query_as(
            "SELECT id, created_at, severity, module, title, body, link, doc_ref, read_at
                   FROM notification ORDER BY created_at DESC LIMIT 100",
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| err(e.to_string()))?;
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
        Err(ServerFnError::ServerError("ssr-only".into()))
    }
}

#[component]
pub fn NotificationsView() -> impl IntoView {
    let r = Resource::new(|| (), |_| async { list_notifications().await });
    let locale = use_locale();
    view! {
        <div class="view">
            <PageHead
                code="NOT-01".to_string()
                module=t(locale, "app.notifications.page.module")
                title="Notifications".to_string()
                title_cn=t(locale, "app.notifications.page.title_cn")
            />
            <Card>
                <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:160px">{t(locale, "app.common.loading")}</div> }>
                    {move || r.get().map(|res| match res {
                        Err(e) => view! { <p>{t(locale, "app.common.load_failed")} " · " {e.to_string()}</p> }.into_any(),
                        Ok(rows) if rows.is_empty() => view! { <p class="muted">{t(locale, "app.notifications.empty")}</p> }.into_any(),
                        Ok(rows) => view! {
                            <div class="vstack" style="gap:0">
                                {rows.into_iter().map(|n| {
                                    let cls_dot = match n.severity.as_str() {
                                        "warn" => "tag amber",
                                        "crit" => "tag rose",
                                        _ => "tag green",
                                    };
                                    let when = ep_core::fmt_ts_minute(Some(n.created_at));
                                    view! {
                                        <div class="list-row">
                                            <span class=cls_dot>{n.severity.to_uppercase()}</span>
                                            <div>
                                                <div class="title">{n.title}</div>
                                                <div class="meta">
                                                    {n.body.clone().unwrap_or_default()}
                                                    <span class="mono dim" style="margin-left:8px">"· " {when}</span>
                                                    {n.module.map(|m| view! { <span class="mono dim" style="margin-left:8px">"· " {m}</span> })}
                                                </div>
                                            </div>
                                            <div></div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any(),
                    })}
                </Suspense>
            </Card>
        </div>
    }
}
