use ep_core::{fmt_minor_compact, IconKind, MinorAmount};
// `fmt_ts_hm` is consumed only inside the `#[cfg(feature = "ssr")]` body of
// `load_dashboard`; importing it at module scope would warn on the wasm32
// hydrate target where the body is replaced by an `ssr-only` stub.
#[cfg(feature = "ssr")]
use ep_core::{fmt_ts_hm, server_err};
use ep_finance::Currency;
use ep_i18n::{server_fn_error_text, t, use_locale};
use ep_ui::{
    Card, Direction, EmptyState, Kpi, LoadError, PageHead, SectionLabel, SkeletonCard, SkeletonKpi,
    Tag,
};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardData {
    /// Finance figures are scoped to the primary currency, in its minor units.
    pub savings: MinorAmount,
    pub budget_pct: u32,
    pub budget_remain: MinorAmount,
    pub today_events: u32,
    pub weekly_workouts: u32,
    pub weekly_learning: u32,
    pub recent: Vec<ActivityRow>,
    /// Every currency, so the activity feed can format each row's amount with
    /// the right symbol and precision.
    pub currencies: Vec<Currency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityRow {
    pub time: String,
    pub module: String,
    pub summary: String,
    /// Signed minor-unit amount (finance rows only).
    pub amount: Option<MinorAmount>,
    /// Currency of the amount; `None` for non-finance rows.
    pub currency_code: Option<String>,
}

#[cfg(feature = "ssr")]
type ActivityQueryRow = (i64, String, String, Option<MinorAmount>, Option<String>);

#[server(LoadDashboard, "/api/_internal/dsh", "Url", "load_dashboard")]
pub async fn load_dashboard() -> Result<DashboardData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state = ep_core::app_state_context()?;
        let pool = &state.db;
        // Cross-module aggregate view: finance KPIs show the primary currency
        // only — there is no conversion, so summing across currencies would
        // be meaningless.
        let primary = ep_finance::resolve_currency(pool, "").await?;
        // Independent read-only queries — fan out via tokio::try_join!
        // so the request pays one slowest-query latency instead of eight.
        type AmountTagRow = (MinorAmount, String);
        let month_txns_q = sqlx::query_as::<_, AmountTagRow>(
            "SELECT amount, tag FROM fin_txn
              WHERE currency_code = ?1
                AND occurred_at >= unixepoch('now','localtime','start of month','utc')",
        )
        .bind(&primary.code)
        .fetch_all(pool);
        let budget_rows_q = sqlx::query_scalar::<_, MinorAmount>(
            "SELECT amount FROM fin_budget
              WHERE currency_code = ?1 AND period = strftime('%Y-%m','now','localtime')",
        )
        .bind(&primary.code)
        .fetch_all(pool);
        let today_events_q = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM activity
              WHERE occurred_at >= unixepoch('now','localtime','start of day','utc')",
        )
        .fetch_one(pool);
        // Cross-module weekly KPIs use the *same* Monday-anchored local week
        // as each module's own banner so the dashboard never disagrees with the
        // module page (the rolling-7-day form drifted on Mon–Sat). The modifier
        // order is documented in `ep_fitness`'s `WEEK_START_LOCAL_MODIFIERS`:
        // '-6 days' steps back, 'weekday 1' lands on the week's Monday,
        // 'start of day' anchors local midnight, 'utc' converts to epoch.
        //
        // Both reads are off the unified `activity` feed (module='FIT'/'LRN')
        // rather than mixing a direct `fit_workout` count with an `activity`
        // count — every workout/learning entry writes exactly one activity row,
        // so the values match their module banners while sharing one source.
        const WEEK_START: &str = "'-6 days','weekday 1','start of day','utc'";
        // Bound to `let` so the formatted SQL outlives the `&str` borrow the
        // query builder holds until `try_join!` polls each future.
        let weekly_workouts_sql = format!(
            "SELECT COUNT(*) FROM activity
              WHERE module = 'FIT'
                AND occurred_at >= unixepoch('now','localtime',{WEEK_START})"
        );
        let weekly_learning_sql = format!(
            "SELECT COUNT(*) FROM activity
              WHERE module = 'LRN'
                AND occurred_at >= unixepoch('now','localtime',{WEEK_START})"
        );
        let weekly_workouts_q = sqlx::query_scalar::<_, i64>(&weekly_workouts_sql).fetch_one(pool);
        let weekly_learning_q = sqlx::query_scalar::<_, i64>(&weekly_learning_sql).fetch_one(pool);
        let rows_q = sqlx::query_as::<_, ActivityQueryRow>(
            "SELECT occurred_at, module, summary, amount, currency_code
               FROM activity ORDER BY occurred_at DESC LIMIT 12",
        )
        .fetch_all(pool);
        let currencies_q = ep_finance::list_currencies_inner(pool);
        let (
            month_txns,
            budget_rows,
            today_events,
            weekly_workouts,
            weekly_learning,
            rows,
            currencies,
        ) = tokio::try_join!(
            month_txns_q,
            budget_rows_q,
            today_events_q,
            weekly_workouts_q,
            weekly_learning_q,
            rows_q,
            currencies_q,
        )
        .map_err(server_err)?;
        let income: MinorAmount = month_txns
            .iter()
            .filter(|(amount, tag)| tag == "inc" && amount.is_positive())
            .map(|(amount, _)| *amount)
            .sum();
        let expense: MinorAmount = month_txns
            .iter()
            .filter(|(amount, tag)| tag == "exp" && amount.is_negative())
            .map(|(amount, _)| amount.abs())
            .sum();
        let budget_total: MinorAmount = budget_rows.into_iter().sum();
        let savings = income - expense;
        let budget_pct = if budget_total.is_positive() {
            (expense.to_f64() / budget_total.to_f64() * 100.0).round() as u32
        } else {
            0
        };
        let budget_remain = (budget_total - expense).max(MinorAmount::ZERO);
        let recent = rows
            .into_iter()
            .map(|r| ActivityRow {
                time: fmt_ts_hm(Some(r.0)),
                module: r.1,
                summary: r.2,
                amount: r.3,
                currency_code: r.4,
            })
            .collect();

        Ok(DashboardData {
            savings,
            budget_pct,
            budget_remain,
            today_events: today_events.max(0) as u32,
            weekly_workouts: weekly_workouts.max(0) as u32,
            weekly_learning: weekly_learning.max(0) as u32,
            recent,
            currencies,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[component]
pub fn DashboardView() -> impl IntoView {
    let r = Resource::new(|| (), |_| async { load_dashboard().await });
    let locale = use_locale();
    view! {
        <div class="view">
            <PageHead
                code="DSH-01"
                module=t(locale, "app.dashboard.page.module")
                title=t(locale, "app.dashboard.page.title")
                title_cn=t(locale, "app.dashboard.page.title_cn")
                sub=t(locale, "app.dashboard.page.subtitle")
            />

            <Suspense fallback=move || view! {
                <SkeletonKpi count=5/>
                <SectionLabel index="§ 02".to_string()>{t(locale, "app.dashboard.activity.title")}</SectionLabel>
                <SkeletonCard rows=2/>
            }>
                {move || r.get().map(|res| match res {
                    Err(e) => view! { <LoadError detail=server_fn_error_text(&e)/> }.into_any(),
                    Ok(d) => render_body(d).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_body(d: DashboardData) -> impl IntoView {
    let locale = use_locale();
    // Primary-currency symbol/precision for the finance KPIs.
    let (primary_symbol, primary_decimals) = d
        .currencies
        .iter()
        .find(|c| c.is_primary)
        .map(|c| (c.symbol.clone(), c.decimals))
        .unwrap_or_else(|| (String::new(), 2));
    // code → (symbol, decimals) for the mixed-currency activity feed.
    let cur_map: std::collections::HashMap<String, (String, u8)> = d
        .currencies
        .iter()
        .map(|c| (c.code.clone(), (c.symbol.clone(), c.decimals)))
        .collect();
    let recent = d.recent.clone();
    let savings_value = format!(
        "{}{}",
        primary_symbol,
        fmt_minor_compact(d.savings, primary_decimals)
    );
    let budget_remain_text = format!(
        "{}{} {}",
        primary_symbol,
        fmt_minor_compact(d.budget_remain, primary_decimals),
        t(locale, "app.dashboard.kpi.budget_remain_suffix")
    );
    view! {
        <div class="kpi-grid">
            <Kpi code="FIN-K01" label=t(locale, "app.dashboard.kpi.monthly_savings") value=savings_value
                 delta=t(locale, "app.dashboard.kpi.current_month") dir=Direction::Flat/>
            <Kpi code="FIN-K02" label=t(locale, "app.dashboard.kpi.budget_usage") value=format!("{}", d.budget_pct) unit="%"
                 delta=budget_remain_text dir=Direction::Flat/>
            <Kpi code="TDY-K01" label=t(locale, "app.dashboard.kpi.today_events") value=format!("{}", d.today_events)
                 unit=t(locale, "app.dashboard.unit.entries").to_string()
                 delta=t(locale, "app.dashboard.kpi.since_midnight") dir=Direction::Flat/>
            <Kpi code="FIT-K01" label=t(locale, "app.dashboard.kpi.weekly_workouts") value=format!("{}", d.weekly_workouts)
                 unit=t(locale, "app.dashboard.unit.times").to_string()
                 delta=t(locale, "app.dashboard.kpi.this_week") dir=Direction::Flat/>
            <Kpi code="LRN-K01" label=t(locale, "app.dashboard.kpi.weekly_learning") value=format!("{}", d.weekly_learning)
                 unit=t(locale, "app.dashboard.unit.entries").to_string()
                 delta=t(locale, "app.dashboard.kpi.this_week") dir=Direction::Flat/>
        </div>

        <SectionLabel index="§ 02".to_string()>{t(locale, "app.dashboard.activity.title")}</SectionLabel>

        <Card>
            {if recent.is_empty() {
                view! {
                    <EmptyState
                        icon=IconKind::Empty
                        title=t(locale, "app.dashboard.activity.title")
                        desc=t(locale, "app.dashboard.activity.empty")
                        code="DSH-EMPTY"
                    />
                }.into_any()
            } else {
                view! {
                    <div class="scroll-x">
                        <table class="tbl">
                            <thead>
                                <tr>
                                    <th style="width:120px">{t(locale, "app.dashboard.table.time")}</th>
                                    <th style="width:90px">{t(locale, "app.dashboard.table.module")}</th>
                                    <th>{t(locale, "app.dashboard.table.description")}</th>
                                    <th class="num" style="width:140px">{t(locale, "app.dashboard.table.amount_status")}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {recent.into_iter().map(|r| {
                                    let ActivityRow {
                                        time,
                                        module,
                                        summary,
                                        amount,
                                        currency_code,
                                    } = r;
                                    let tone = match module.as_str() {
                                        "FIN" => ep_core::Tone::Amber,
                                        "FIT" => ep_core::Tone::Green,
                                        "LRN" => ep_core::Tone::Blue,
                                        _ => ep_core::Tone::None,
                                    };
                                    let (amt_cls, amt_text) = match amount {
                                        Some(v) => {
                                            let (sym, dec) = currency_code.as_deref()
                                                .and_then(|c| cur_map.get(c))
                                                .cloned()
                                                .unwrap_or_else(|| (String::new(), 2));
                                            let (cls, sign) = if v >= 0 { ("num amt-pos", "+") } else { ("num amt-neg", "−") };
                                            (cls, format!("{sign}{sym}{}", fmt_minor_compact(v.abs(), dec)))
                                        }
                                        None => ("num dim", "—".to_string()),
                                    };
                                    view! {
                                        <tr>
                                            <td class="mono dim">{time}</td>
                                            <td><Tag tone=tone>{module}</Tag></td>
                                            <td>{summary}</td>
                                            <td class=amt_cls>{amt_text}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }.into_any()
            }}
        </Card>
    }
}
