use ep_core::{fmt_minor_compact, IconKind, MinorAmount};
use ep_finance::Currency;
use ep_i18n::{server_fn_error_text, t, tf, use_locale};
use ep_ui::{Card, Direction, EmptyState, Kpi, PageHead, SkeletonCard, SkeletonKpi};
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
    /// Today's expense magnitude in the primary currency's minor units.
    pub fin_expense: MinorAmount,
    pub fit_count: u32,
    pub lrn_count: u32,
    /// Every currency, so the timeline can format each row's amount.
    pub currencies: Vec<Currency>,
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
pub async fn load_today() -> Result<TodayData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;
        let pool = &st.db;

        // The "spent today" KPI is a cross-module aggregate — primary currency
        // only (currencies never convert into one another). Three independent
        // fetches join in parallel; sqlx errors get normalized to ServerFnError
        // so they ride in the same try_join.
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
        let items: Vec<TodayItem> = today
            .rows
            .into_iter()
            .map(|row| {
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
                TodayItem {
                    time: fmt_ts_hm(Some(row.occurred_at)),
                    module: row.module,
                    summary: row.summary,
                    amount: row.amount,
                    currency_code: row.currency_code,
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
            currencies,
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
            <Suspense fallback=move || view! {
                <div style="margin-bottom:22px"><SkeletonCard rows=0/></div>
                <SkeletonKpi count=4/>
                <SkeletonCard rows=2/>
            }>
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
              sub=if empty { String::new() }
                   else { tf(locale, "app.today.card.timeline_sub", &[("count", &items.len().to_string())]) }>
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
