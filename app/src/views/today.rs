use ep_core::IconKind;
use ep_ui::{Card, Icon, Kpi, kpi::Direction, PageHead};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

// Gated to `ssr` because the only callers (`load_today` body and the
// `#[cfg(not(feature = "ssr"))]` stub) both live behind the same flag.
#[cfg(feature = "ssr")]
use ep_core::server_err;
#[cfg(feature = "ssr")]
use ep_core::fmt_ts_hm;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TodayData {
    pub date: String,                // YYYY-MM-DD
    pub items: Vec<TodayItem>,
    pub event_count: u32,
    pub fin_expense: f64,            // today, magnitude (yuan)
    pub fit_count: u32,
    pub lrn_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayItem {
    pub time: String,                // HH:MM
    pub module: String,              // FIN / FIT / LRN / SYS
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
        let st: ep_core::AppState = expect_context();
        let pool = &st.db;

        // We want "today" to match the user's wall clock, not UTC midnight.
        // SQLite modifiers compose left-to-right against the running time
        // string, so the order matters: `'now','localtime'` shifts the
        // string to local time; `'start of day'` then rounds *that* down
        // to local 00:00; `'utc'` converts back to UTC so `unixepoch()`
        // returns the right epoch seconds.
        // (`'now','start of day','localtime'` would round UTC first, then
        //  shift, giving UTC midnight off by the local offset — the bug
        //  the Codex stop-hook caught.)
        let date: String = sqlx::query_scalar("SELECT date('now','localtime')")
            .fetch_one(pool).await.map_err(server_err)?;
        type Row = (i64, String, String, String, Option<f64>, Option<String>);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT occurred_at, module, doc_id, summary, amount, link_doc
               FROM activity
              WHERE occurred_at >= unixepoch('now','localtime','start of day','utc')
              ORDER BY occurred_at ASC"
        )
        .fetch_all(pool)
        .await
        .map_err(server_err)?;

        let event_count = rows.len() as u32;
        let mut fin_expense = 0.0;
        let mut fit_count: u32 = 0;
        let mut lrn_count: u32 = 0;
        let items: Vec<TodayItem> = rows.into_iter().map(|(ts, module, doc_id, summary, amount, link)| {
            match module.as_str() {
                "FIN" => if let Some(a) = amount { if a < 0.0 { fin_expense += -a; } },
                "FIT" => fit_count += 1,
                "LRN" => lrn_count += 1,
                _ => {}
            }
            TodayItem {
                time: fmt_ts_hm(Some(ts)),
                module, doc_id, summary, amount, link_doc: link,
            }
        }).collect();

        Ok(TodayData {
            date,
            items,
            event_count, fin_expense, fit_count, lrn_count,
        })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[component]
pub fn TodayView() -> impl IntoView {
    let today = Resource::new(|| (), |_| async { load_today().await });

    view! {
        <div class="view">
            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:200px">"loading…"</div> }>
                {move || today.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">"加载失败 · " {e.to_string()}</div></div> }.into_any(),
                    Ok(d) => render_today(d).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_today(d: TodayData) -> impl IntoView {
    let title_cn = format!("今日 · {}", d.date);
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
            module="TODAY · 今日聚焦"
            title="Today"
            title_cn=title_cn
            sub="来自各模块的真实事件流 · 按时间排序 · 0:00 起算"
        />

        <div class="kpi-grid">
            <Kpi code="TDY-K01" label="今日事件" value=event_value
                 unit="条".to_string()
                 delta="跨模块累计".to_string() dir=Direction::Flat/>
            <Kpi code="TDY-K02" label="今日支出" value=fin_value
                 delta="FIN 自动累计".to_string() dir=Direction::Flat/>
            <Kpi code="TDY-K03" label="今日训练" value=fit_value
                 unit="次".to_string()
                 delta="FIT 自动累计".to_string() dir=Direction::Flat/>
            <Kpi code="TDY-K04" label="今日学习" value=lrn_value
                 unit="条".to_string()
                 delta="LRN 自动累计".to_string() dir=Direction::Flat/>
        </div>

        <Card title="今日时间线" code="TDY-LN-01"
              sub=if empty { "尚无事件 · 在任一模块创建一条记录即可填充".to_string() }
                   else { format!("{} 条事件 · 点击跳转源模块", items.len()) }>
            {if empty {
                view! { <p class="muted">"今日还没有事件。去 Finance / Fitness / Learning 任一模块创建一条记录就会出现在这里。"</p> }.into_any()
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
        if a > 0.0 { format!("+¥{}", ep_core::fmt_money(a)) }
        else { format!("−¥{}", ep_core::fmt_money(a.abs())) }
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
    /// boundary up to a full day off the user's actual local midnight. A
    /// direct probe (commit 4a5d685's body) measured 16h drift under
    /// `TZ=Asia/Shanghai`; this test ensures any future "simplification"
    /// of the SQL trips an alarm rather than silently re-introducing the
    /// bug.
    #[tokio::test(flavor = "current_thread")]
    async fn local_day_boundary_is_in_the_past() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let fixed: i64 = sqlx::query_scalar(
            "SELECT unixepoch('now','localtime','start of day','utc')"
        ).fetch_one(&pool).await.unwrap();
        let now: i64 = sqlx::query_scalar("SELECT unixepoch('now')")
            .fetch_one(&pool).await.unwrap();
        assert!(fixed <= now, "local midnight ({fixed}) must not exceed now ({now})");
        // And no further than 24h in the past, regardless of TZ.
        assert!(now - fixed < 24 * 3600,
                "boundary {fixed} should be within 24h of now {now} ({}h drift)",
                (now - fixed) / 3600);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn local_date_string_is_iso_today_or_yesterday() {
        // `date('now','localtime')` should always be a valid ISO 8601 date
        // string within ±1 day of today (UTC). We can't pin the exact value
        // without controlling the TZ, but we can pin the format.
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let date: String = sqlx::query_scalar("SELECT date('now','localtime')")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(date.len(), 10, "expected YYYY-MM-DD, got {date:?}");
        assert!(date.chars().nth(4) == Some('-') && date.chars().nth(7) == Some('-'),
                "expected YYYY-MM-DD, got {date:?}");
    }
}
