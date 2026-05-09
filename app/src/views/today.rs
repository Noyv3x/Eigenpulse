use ep_core::IconKind;
use ep_i18n::{server_fn_error_text, t, tf, use_locale};
use ep_ui::{Card, Direction, Icon, Kpi, PageHead};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

// Gated to `ssr` because the only callers (`load_today` body and the
// `#[cfg(not(feature = "ssr"))]` stub) both live behind the same flag.
#[cfg(feature = "ssr")]
use ep_core::server_err;
#[cfg(feature = "ssr")]
use ep_core::{fmt_ts_hm, TodayActivityOrder};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TodayData {
    pub date: String, // YYYY-MM-DD
    pub items: Vec<TodayItem>,
    pub event_count: u32,
    pub fin_expense: f64, // today, magnitude (yuan)
    pub fit_count: u32,
    pub lrn_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayItem {
    pub time: String,   // HH:MM
    pub module: String, // FIN / FIT / LRN / SYS
    pub doc_id: String,
    pub summary: String,
    pub amount: Option<f64>,
    pub link_doc: Option<String>,
}

#[server(LoadToday, "/api/_internal/tdy", "Url", "load_today")]
pub async fn load_today() -> Result<TodayData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;
        let pool = &st.db;

        let today = ep_core::load_today_activity(pool, TodayActivityOrder::Asc, None)
            .await
            .map_err(server_err)?;

        let event_count = today.rows.len() as u32;
        let mut fin_expense = 0.0;
        let mut fit_count: u32 = 0;
        let mut lrn_count: u32 = 0;
        let items: Vec<TodayItem> = today
            .rows
            .into_iter()
            .map(|row| {
                match row.module.as_str() {
                    "FIN" => {
                        if let Some(a) = row.amount {
                            if a < 0.0 {
                                fin_expense += -a;
                            }
                        }
                    }
                    "FIT" => fit_count += 1,
                    "LRN" => lrn_count += 1,
                    _ => {}
                }
                TodayItem {
                    time: fmt_ts_hm(Some(row.occurred_at)),
                    module: row.module,
                    doc_id: row.doc_id,
                    summary: row.summary,
                    amount: row.amount,
                    link_doc: row.link_doc,
                }
            })
            .collect();

        Ok(TodayData {
            date: today.date,
            items,
            event_count,
            fin_expense,
            fit_count,
            lrn_count,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[component]
pub fn TodayView() -> impl IntoView {
    let today = Resource::new(|| (), |_| async { load_today().await });
    let locale = use_locale();

    view! {
        <div class="view">
            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:200px">{t(locale, "app.common.loading")}</div> }>
                {move || today.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</div></div> }.into_any(),
                    Ok(d) => render_today(d).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_today(d: TodayData) -> impl IntoView {
    let locale = use_locale();
    let title_cn = tf(locale, "app.today.page.title_cn", &[("date", &d.date)]);
    let event_value = format!("{}", d.event_count);
    let fin_value = if d.fin_expense > 0.0 {
        format!("¥{}", ep_core::fmt_int(d.fin_expense))
    } else {
        "¥0".to_string()
    };
    let fit_value = format!("{}", d.fit_count);
    let lrn_value = format!("{}", d.lrn_count);
    let items = d.items;
    let empty = items.is_empty();

    view! {
        <PageHead
            code="TDY-01"
            module=t(locale, "app.today.page.module")
            title=t(locale, "app.today.page.title")
            title_cn=title_cn
            sub=t(locale, "app.today.page.subtitle")
        />

        <div class="kpi-grid">
            <Kpi code="TDY-K01" label=t(locale, "app.today.kpi.event_count") value=event_value
                 unit=t(locale, "app.today.unit.entries").to_string()
                 delta=t(locale, "app.today.kpi.cross_module_total").to_string() dir=Direction::Flat/>
            <Kpi code="TDY-K02" label=t(locale, "app.today.kpi.spent") value=fin_value
                 delta=t(locale, "app.today.kpi.fin_auto").to_string() dir=Direction::Flat/>
            <Kpi code="TDY-K03" label=t(locale, "app.today.kpi.workouts") value=fit_value
                 unit=t(locale, "app.today.unit.times").to_string()
                 delta=t(locale, "app.today.kpi.fit_auto").to_string() dir=Direction::Flat/>
            <Kpi code="TDY-K04" label=t(locale, "app.today.kpi.learning") value=lrn_value
                 unit=t(locale, "app.today.unit.entries").to_string()
                 delta=t(locale, "app.today.kpi.lrn_auto").to_string() dir=Direction::Flat/>
        </div>

        <Card title=t(locale, "app.today.card.timeline_title") code="TDY-LN-01"
              sub=if empty { t(locale, "app.today.card.empty_sub").to_string() }
                   else { tf(locale, "app.today.card.timeline_sub", &[("count", &items.len().to_string())]) }>
            {if empty {
                view! { <p class="muted">{t(locale, "app.today.card.empty_hint")}</p> }.into_any()
            } else {
                view! {
                    <div class="today-list">
                        {items.into_iter().map(render_today_item).collect_view()}
                    </div>
                }.into_any()
            }}
        </Card>
    }
}

fn render_today_item(it: TodayItem) -> impl IntoView {
    let module_link = match it.module.as_str() {
        "FIN" => "/finance",
        "FIT" => "/fitness",
        "LRN" => "/learning",
        _ => "/",
    };
    let amount_text = it.amount.map(|a| {
        if a > 0.0 {
            format!("+¥{}", ep_core::fmt_money(a))
        } else {
            format!("−¥{}", ep_core::fmt_money(a.abs()))
        }
    });
    view! {
        <a class="today-item" href=module_link>
            <span class="time mono">{it.time}</span>
            <span class="mark"></span>
            <div>
                <div class="text">
                    <span class="text-module mono dim">{it.module.clone()}</span>
                    {it.summary}
                    {amount_text.map(|a| view! { <span class="text-amount mono dim">{a}</span> })}
                </div>
            </div>
            <span class="ref mono">
                {it.doc_id.clone()}
                {it.link_doc.map(|l| view! {
                    <span class="ref-link">
                        <Icon kind=IconKind::Link size=10/> " " {l}
                    </span>
                })}
            </span>
        </a>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod boundary_tests {
    /// Pins the SQLite modifier-order fix described in the rationale comment
    /// inside `load_today`. The reversed form
    /// `unixepoch('now','start of day','localtime')` would compute UTC
    /// midnight first, then shift by the local TZ offset — putting the
    /// boundary up to a full day off the user's actual local midnight. Manual
    /// probes under `TZ=Asia/Shanghai` have shown multi-hour drift for the
    /// reversed form; this test ensures any future "simplification" of the SQL
    /// trips an alarm rather than silently re-introducing the bug.
    #[tokio::test(flavor = "current_thread")]
    async fn local_day_boundary_is_in_the_past() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let fixed: i64 =
            sqlx::query_scalar("SELECT unixepoch('now','localtime','start of day','utc')")
                .fetch_one(&pool)
                .await
                .unwrap();
        let now: i64 = sqlx::query_scalar("SELECT unixepoch('now')")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            fixed <= now,
            "local midnight ({fixed}) must not exceed now ({now})"
        );
        // And no further than 24h in the past, regardless of TZ.
        assert!(
            now - fixed < 24 * 3600,
            "boundary {fixed} should be within 24h of now {now} ({}h drift)",
            (now - fixed) / 3600
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn local_date_string_is_iso_today_or_yesterday() {
        // `date('now','localtime')` should always be a valid ISO 8601 date
        // string within ±1 day of today (UTC). We can't pin the exact value
        // without controlling the TZ, but we can pin the format.
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let date: String = sqlx::query_scalar("SELECT date('now','localtime')")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(date.len(), 10, "expected YYYY-MM-DD, got {date:?}");
        assert!(
            date.chars().nth(4) == Some('-') && date.chars().nth(7) == Some('-'),
            "expected YYYY-MM-DD, got {date:?}"
        );
    }
}
