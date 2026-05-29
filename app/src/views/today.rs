use ep_core::{fmt_minor_compact, IconKind, MinorAmount};
use ep_finance::Currency;
use ep_i18n::{server_fn_error_text, t, tf, use_locale};
use ep_ui::{Card, Direction, EmptyState, Kpi, LoadError, PageHead, SkeletonCard, SkeletonKpi};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

// Gated to `ssr` because the only callers (`load_today` body and the
// `#[cfg(not(feature = "ssr"))]` stub) both live behind the same flag.
#[cfg(feature = "ssr")]
use ep_core::server_err;
#[cfg(feature = "ssr")]
use ep_core::{fmt_ts_hm, TodayActivityOrder};

/// How many timeline rows one page of the today view shows. The KPI
/// aggregates always cover the full day regardless of the page window. Only
/// read in the SSR `load_today` body (the page size is echoed to the view on
/// `TodayData::limit`), so it is gated to keep the hydrate target warning-free.
#[cfg(feature = "ssr")]
const TODAY_PAGE_SIZE: u32 = 50;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TodayData {
    pub date: String, // YYYY-MM-DD
    pub items: Vec<TodayItem>,
    pub event_count: u32,
    /// Today's expense magnitude in the primary currency's minor units.
    pub fin_expense: MinorAmount,
    pub fit_count: u32,
    pub lrn_count: u32,
    /// Every currency, so the timeline can format each row's amount.
    pub currencies: Vec<Currency>,
    /// Zero-based row offset of the returned `items` window into today's full
    /// event list (for the prev/next pager). `event_count` is the page-agnostic
    /// total, so the view derives "showing N–M of T" without a second fetch.
    pub offset: u32,
    /// The page size the server applied (echoed so the view can clamp paging).
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodayItem {
    pub time: String,   // HH:MM
    pub module: String, // FIN / FIT / LRN / SYS
    pub summary: String,
    /// Signed minor-unit amount (finance rows only).
    pub amount: Option<MinorAmount>,
    /// Currency of the amount; `None` for non-finance rows.
    pub currency_code: Option<String>,
}

#[server(LoadToday, "/api/_internal/tdy", "Url", "load_today")]
pub async fn load_today(offset: u32) -> Result<TodayData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;
        let pool = &st.db;

        // The "spent today" KPI is a cross-module aggregate — primary currency
        // only (currencies never convert into one another). Three independent
        // fetches join in parallel; sqlx errors get normalized to ServerFnError
        // so they ride in the same try_join.
        //
        // We fetch the full day (the today set is naturally bounded) so the KPI
        // aggregates always cover everything; the prev/next pager then slices a
        // `TODAY_PAGE_SIZE` window out of the same in-memory list — no second
        // round-trip and no risk of the totals disagreeing with the page.
        let primary_fut = ep_finance::resolve_currency(pool, "");
        let today_fut = async {
            ep_core::load_today_activity(pool, TodayActivityOrder::Asc, None)
                .await
                .map_err(server_err)
        };
        let currencies_fut = async {
            ep_finance::list_currencies_inner(pool)
                .await
                .map_err(server_err)
        };
        let (primary, today, currencies) =
            tokio::try_join!(primary_fut, today_fut, currencies_fut)?;

        let event_count = today.rows.len() as u32;
        let mut fin_expense = MinorAmount::ZERO;
        let mut fit_count: u32 = 0;
        let mut lrn_count: u32 = 0;
        // Clamp the requested offset to the last real page start so a stale
        // "next" click past the end never returns an empty page mid-day. The
        // last page begins at the largest multiple of TODAY_PAGE_SIZE that is
        // still < event_count (0 when the day is empty or fits one page).
        let last_page_start = if event_count == 0 {
            0
        } else {
            ((event_count - 1) / TODAY_PAGE_SIZE) * TODAY_PAGE_SIZE
        };
        let offset = offset.min(last_page_start);
        let items: Vec<TodayItem> = today
            .rows
            .into_iter()
            .enumerate()
            .filter_map(|(idx, row)| {
                match row.module.as_str() {
                    // Only the primary currency feeds the cross-module KPI.
                    "FIN" if row.currency_code.as_deref() == Some(primary.code.as_str()) => {
                        if let Some(a) = row.amount.filter(|a| a.is_negative()) {
                            fin_expense += -a;
                        }
                    }
                    "FIT" => fit_count += 1,
                    "LRN" => lrn_count += 1,
                    _ => {}
                }
                // Aggregates above run over every row; only the current page
                // window is materialized into `TodayItem`s for the timeline.
                let idx = idx as u32;
                if idx < offset || idx >= offset + TODAY_PAGE_SIZE {
                    return None;
                }
                Some(TodayItem {
                    time: fmt_ts_hm(Some(row.occurred_at)),
                    module: row.module,
                    summary: row.summary,
                    amount: row.amount,
                    currency_code: row.currency_code,
                })
            })
            .collect();

        Ok(TodayData {
            date: today.date,
            items,
            event_count,
            fin_expense,
            fit_count,
            lrn_count,
            currencies,
            offset,
            limit: TODAY_PAGE_SIZE,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        let _ = offset;
        Err(server_err("ssr-only"))
    }
}

#[component]
pub fn TodayView() -> impl IntoView {
    // Row offset of the visible timeline page. The resource re-runs whenever it
    // changes; the KPI aggregates are page-agnostic so they stay stable across
    // pages. Starts at 0 (the start of the day).
    let offset = RwSignal::new(0u32);
    let today = Resource::new(
        move || offset.get(),
        |off| async move { load_today(off).await },
    );
    let locale = use_locale();

    view! {
        <div class="view">
            // PageHead lives *outside* the Suspense boundary so the page header
            // always renders — including on a load error — matching the
            // dashboard / reports views. The date-bearing Chinese title segment
            // depends on server-provided `d.date`, so it is rendered inside the
            // Suspense body (the timeline card sub) rather than the head; the
            // view runs on wasm32 where computing the local date is unsafe.
            <PageHead
                code="TDY-01"
                module=t(locale, "app.today.page.module")
                title=t(locale, "app.today.page.title")
                sub=t(locale, "app.today.page.subtitle")
            />
            <Suspense fallback=move || view! {
                <SkeletonKpi count=4/>
                <SkeletonCard rows=2/>
            }>
                {move || today.get().map(|res| match res {
                    Err(e) => view! { <LoadError detail=server_fn_error_text(&e)/> }.into_any(),
                    Ok(d) => render_today(d, offset).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_today(d: TodayData, offset: RwSignal<u32>) -> impl IntoView {
    let locale = use_locale();
    let date_label = tf(locale, "app.today.page.title_cn", &[("date", &d.date)]);
    let event_value = format!("{}", d.event_count);
    let (primary_symbol, primary_decimals) = d
        .currencies
        .iter()
        .find(|c| c.is_primary)
        .map(|c| (c.symbol.clone(), c.decimals))
        .unwrap_or_else(|| (String::new(), 2));
    let fin_value = format!(
        "{}{}",
        primary_symbol,
        fmt_minor_compact(d.fin_expense.max(MinorAmount::ZERO), primary_decimals)
    );
    let fit_value = format!("{}", d.fit_count);
    let lrn_value = format!("{}", d.lrn_count);
    // code → (symbol, decimals) for the mixed-currency timeline.
    let cur_map: std::collections::HashMap<String, (String, u8)> = d
        .currencies
        .iter()
        .map(|c| (c.code.clone(), (c.symbol.clone(), c.decimals)))
        .collect();
    let items = d.items;
    let empty = items.is_empty();

    let timeline_sub = if empty {
        date_label
    } else {
        format!(
            "{} · {}",
            date_label,
            tf(
                locale,
                "app.today.card.timeline_sub",
                &[("count", &items.len().to_string())]
            )
        )
    };

    // Prev/next pager state. The pager only shows when the day has more events
    // than one page. `from`/`to` are 1-based for the human-readable range.
    let total = d.event_count;
    let page_size = d.limit.max(1);
    let cur_offset = d.offset;
    let show_pager = total > page_size;
    let from = cur_offset + 1;
    let to = (cur_offset + items.len() as u32).max(from);
    let has_prev = cur_offset > 0;
    let has_next = cur_offset + page_size < total;
    let range_label = tf(
        locale,
        "app.today.pager.range",
        &[
            ("from", &from.to_string()),
            ("to", &to.to_string()),
            ("total", &total.to_string()),
        ],
    );

    view! {
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
              sub=timeline_sub>
            {if empty {
                view! {
                    <EmptyState
                        icon=IconKind::Today
                        title=t(locale, "app.today.card.empty_sub")
                        desc=t(locale, "app.today.card.empty_hint")
                        code="TDY-LN-EMPTY"
                    />
                }.into_any()
            } else {
                view! {
                    <div class="today-list">
                        {items.into_iter().map(|it| render_today_item(it, &cur_map)).collect_view()}
                    </div>
                }.into_any()
            }}
            {show_pager.then(|| view! {
                <div class="hstack" style="gap:10px;align-items:center;justify-content:flex-end;margin-top:12px">
                    <span class="mono dim" style="font-size:12px">{range_label}</span>
                    <button class="btn" type="button" disabled=!has_prev
                            on:click=move |_| offset.update(|o| *o = o.saturating_sub(page_size))>
                        {t(locale, "app.today.pager.prev")}
                    </button>
                    <button class="btn" type="button" disabled=!has_next
                            on:click=move |_| offset.update(|o| *o += page_size)>
                        {t(locale, "app.today.pager.next")}
                    </button>
                </div>
            })}
        </Card>
    }
}

fn render_today_item(
    it: TodayItem,
    cur_map: &std::collections::HashMap<String, (String, u8)>,
) -> impl IntoView {
    let TodayItem {
        time,
        module,
        summary,
        amount,
        currency_code,
    } = it;
    let module_link = match module.as_str() {
        "FIN" => "/finance",
        "FIT" => "/fitness",
        "LRN" => "/learning",
        _ => "/",
    };
    // Per-module marker colour on the timeline rail (mirrors the dashboard
    // module tags + finance txn-indicator accents): FIN amber, FIT green,
    // LRN blue. CSS reads the `mod-*` class via the `--mark-color` token.
    let item_class = match module.as_str() {
        "FIN" => "today-item mod-fin",
        "FIT" => "today-item mod-fit",
        "LRN" => "today-item mod-lrn",
        _ => "today-item",
    };
    let amount_text = amount.map(|a| {
        let (sym, dec) = currency_code
            .as_deref()
            .and_then(|c| cur_map.get(c))
            .cloned()
            .unwrap_or_else(|| (String::new(), 2));
        let sign = if a >= 0 { "+" } else { "−" };
        format!("{sign}{sym}{}", fmt_minor_compact(a.abs(), dec))
    });
    view! {
        <a class=item_class href=module_link>
            <span class="time mono">{time}</span>
            <span class="mark"></span>
            <div>
                <div class="text">
                    <span class="text-module mono dim">{module}</span>
                    {summary}
                    {amount_text.map(|a| view! { <span class="text-amount mono dim">{a}</span> })}
                </div>
            </div>
            <span></span>
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
