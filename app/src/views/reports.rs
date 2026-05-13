use ep_core::IconKind;
use ep_core::{fmt_int, fmt_money};
use ep_finance::{CategorySummary, MonthBucket};
use ep_i18n::{server_fn_error_text, t, tf, use_locale};
use ep_ui::{
    Card, ChartBars, Direction, EmptyState, Icon, Kpi, PageHead, Ring, SkeletonCard, SkeletonKpi,
    Tag,
};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use ep_core::server_err;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportsData {
    /// 12 most recent months, oldest → newest.
    pub months: Vec<MonthBucket>,
    /// Category breakdown over the last 30 days (descending by value).
    /// Reuses the finance `CategorySummary` shape — same fields, same
    /// pct=value/total*100 normalization, just a wider time window.
    pub category_30d: Vec<CategorySummary>,
    /// All accounts with current balance and their share of the total
    /// positive balance, rendered as the per-account Ring fill.
    pub accounts: Vec<ReportAccount>,
    pub year_income: f64,
    pub year_expense: f64,
    pub year_savings_rate: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportAccount {
    pub code: String,
    pub name: String,
    pub r#type: String,
    pub tone: String,
    pub balance: f64,
    pub pct: u32,
}

#[server(LoadReports, "/api/_internal/rpt", "Url", "load_reports")]
pub async fn load_reports() -> Result<ReportsData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;
        let pool = &st.db;

        // 30-day category share: only true expenses (tag='exp').
        type CatRow = (String, f64);
        let cat_30d_q = sqlx::query_as::<_, CatRow>(
            "SELECT category_code, SUM(-amount)
               FROM fin_txn
              WHERE tag = 'exp' AND occurred_at >= unixepoch('now','-30 days')
              GROUP BY category_code
              ORDER BY 2 DESC",
        )
        .fetch_all(pool);

        type CatMetaRow = (String, String, String);
        let cat_meta_q =
            sqlx::query_as::<_, CatMetaRow>("SELECT code, name, tone FROM fin_category")
                .fetch_all(pool);

        type AccRow = (String, String, String, String, f64);
        let accounts_q = sqlx::query_as::<_, AccRow>(
            "SELECT code, name, type, tone, balance
               FROM fin_account ORDER BY code",
        )
        .fetch_all(pool);

        let year_income_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(amount), 0.0) FROM fin_txn
              WHERE amount > 0 AND tag = 'inc' AND occurred_at >= unixepoch('now','localtime','start of year','utc')"
        ).fetch_one(pool);
        let year_expense_q = sqlx::query_scalar::<_, f64>(
            "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn
              WHERE tag = 'exp' AND occurred_at >= unixepoch('now','localtime','start of year','utc')"
        ).fetch_one(pool);

        let months_q = ep_finance::load_month_buckets_12(pool);

        let (month_rows, cat_rows, cat_meta, acc_rows, year_income, year_expense) =
            tokio::try_join!(
                months_q,
                cat_30d_q,
                cat_meta_q,
                accounts_q,
                year_income_q,
                year_expense_q
            )
            .map_err(server_err)?;

        let cat_total: f64 = cat_rows.iter().map(|(_, v)| *v).sum();
        let category_30d: Vec<CategorySummary> = cat_rows
            .into_iter()
            .map(|(code, value)| {
                let meta = cat_meta.iter().find(|(c, _, _)| *c == code);
                CategorySummary {
                    code: code.clone(),
                    name: meta.map(|m| m.1.clone()).unwrap_or_default(),
                    tone: meta.map(|m| m.2.clone()).unwrap_or_default(),
                    value,
                    pct: if cat_total > 0.0 {
                        (value / cat_total * 1000.0).round() / 10.0
                    } else {
                        0.0
                    },
                }
            })
            .collect();

        let acc_total: f64 = acc_rows.iter().map(|r| r.4.max(0.0)).sum();
        let accounts: Vec<ReportAccount> = acc_rows
            .into_iter()
            .map(|(code, name, r#type, tone, balance)| {
                let pct = if acc_total > 0.0 {
                    (balance.max(0.0) / acc_total * 100.0).round() as u32
                } else {
                    0
                };
                ReportAccount {
                    code,
                    name,
                    r#type,
                    tone,
                    balance,
                    pct,
                }
            })
            .collect();

        let year_savings_rate = if year_income > 0.0 {
            ((year_income - year_expense) / year_income).max(0.0) as f32
        } else {
            0.0
        };

        Ok(ReportsData {
            months: month_rows,
            category_30d,
            accounts,
            year_income,
            year_expense,
            year_savings_rate,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[component]
pub fn ReportsView() -> impl IntoView {
    let data = Resource::new(|| (), |_| async { load_reports().await });
    let locale = use_locale();
    view! {
        <div class="view">
            <PageHead
                code="RPT-01"
                module=t(locale, "app.reports.page.module")
                title=t(locale, "app.reports.page.title")
                title_cn=t(locale, "app.reports.page.title_cn")
                sub=t(locale, "app.reports.page.sub")
            />
            <Suspense fallback=move || view! {
                <SkeletonKpi count=4/>
                <div style="margin-bottom:20px"><SkeletonCard rows=3/></div>
                <div class="grid-2">
                    <SkeletonCard rows=2/>
                    <SkeletonCard rows=2/>
                </div>
            }>
                {move || data.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</div></div> }.into_any(),
                    Ok(d) => render_reports(d).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_reports(d: ReportsData) -> impl IntoView {
    let locale = use_locale();
    let months_count = d.months.len();
    let kpis = view! {
        <div class="kpi-grid">
            <Kpi code="RPT-K01" label=t(locale, "app.reports.kpi.income") value=format!("¥{}", fmt_int(d.year_income))
                 delta=t(locale, "app.reports.kpi.ytd") dir=Direction::Up/>
            <Kpi code="RPT-K02" label=t(locale, "app.reports.kpi.expense") value=format!("¥{}", fmt_int(d.year_expense))
                 delta=t(locale, "app.reports.kpi.ytd") dir=Direction::Down/>
            <Kpi code="RPT-K03" label=t(locale, "app.reports.kpi.savings_rate")
                 value=format!("{}", (d.year_savings_rate * 100.0).round() as u32)
                 unit="%".to_string()
                 delta=tf(locale, "app.reports.kpi.net_savings", &[("amount", &fmt_int((d.year_income - d.year_expense).max(0.0)))])
                 dir=Direction::Flat/>
            <Kpi code="RPT-K04" label=t(locale, "app.reports.kpi.accounts") value=format!("{}", d.accounts.len())
                 delta=tf(locale, "app.reports.kpi.cover_months", &[("count", &months_count.to_string())]) dir=Direction::Flat/>
        </div>
    };
    view! {
        {kpis}
        {render_month_trend(&d)}
        <div class="grid-2" style="margin-top:20px">
            {render_category_share(&d)}
            {render_account_health(&d)}
        </div>
    }
}

fn render_month_trend(d: &ReportsData) -> impl IntoView {
    let locale = use_locale();
    let labels: Vec<String> = d
        .months
        .iter()
        // Month label "Apr" / "May" — short form fits the 12-col grid.
        .map(|m| m.period.split('-').nth(1).unwrap_or("?").to_string())
        .collect();
    let income_data: Vec<f64> = d.months.iter().map(|m| m.income).collect();
    let expense_data: Vec<f64> = d.months.iter().map(|m| m.expense).collect();
    let (last_in, last_out, last_net) = d
        .months
        .last()
        .map(|m| (m.income, m.expense, m.net))
        .unwrap_or((0.0, 0.0, 0.0));
    let net_strip = ep_finance::render_net_strip(&d.months);
    view! {
        <Card title=t(locale, "app.reports.month.title") code="RPT-MTH-01"
              sub=tf(locale, "app.reports.month.sub", &[
                  ("count", &d.months.len().to_string()),
                  ("net", &fmt_int(last_net)),
                  ("income", &fmt_int(last_in)),
                  ("expense", &fmt_int(last_out)),
              ])>
            <div class="vstack" style="gap:14px">
                <div>
                    <div class="mono dim chart-row-label">{t(locale, "app.reports.month.income")}</div>
                    <ChartBars data=income_data labels=labels.clone()/>
                </div>
                <div>
                    <div class="mono dim chart-row-label">{t(locale, "app.reports.month.expense")}</div>
                    <ChartBars data=expense_data labels=labels/>
                </div>
                <div>
                    <div class="mono dim chart-row-label">{t(locale, "app.reports.month.net")}</div>
                    {net_strip}
                </div>
            </div>
        </Card>
    }
}

fn render_category_share(d: &ReportsData) -> impl IntoView {
    let locale = use_locale();
    let title_sub = tf(
        locale,
        "app.reports.category.sub",
        &[(
            "total",
            &fmt_int(d.category_30d.iter().map(|c| c.value).sum::<f64>()),
        )],
    );
    let cats = d.category_30d.clone();
    let empty = cats.is_empty();
    view! {
        <Card title=t(locale, "app.reports.category.title") code="RPT-CAT-01" sub=title_sub>
            {if empty {
                view! {
                    <EmptyState
                        icon=IconKind::Coin
                        title=t(locale, "app.reports.category.title")
                        desc=t(locale, "app.reports.category.empty")
                        code="RPT-CAT-EMPTY"
                        compact=true
                    />
                }.into_any()
            } else {
                view! {
                    <div class="vstack" style="gap:10px">
                        {cats.into_iter().map(|c| {
                            let bar_color = ep_core::Tone::parse(&c.tone).css_var();
                            // Cap visible width at the highest category share so the
                            // bars never look uniformly tiny (same rule the FIN
                            // finance expense-mix card uses, see modules/finance/src/view.rs).
                            let pct = (c.pct * 3.0).min(100.0);
                            let _ = c.code;
                            view! {
                                <div>
                                    <div class="cat-row-head">
                                        <div><span>{c.name.clone()}</span></div>
                                        <div class="mono cat-row-value">
                                            {format!("¥{}", fmt_int(c.value))}
                                            <span class="dim">{format!(" · {}%", c.pct)}</span>
                                        </div>
                                    </div>
                                    <div class="bar"><span style=format!("width:{:.1}%;background:{}", pct, bar_color)></span></div>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </Card>
    }
}

fn render_account_health(d: &ReportsData) -> impl IntoView {
    let locale = use_locale();
    let total = d.accounts.iter().map(|a| a.balance).sum::<f64>();
    let rows = d.accounts.clone();
    view! {
        <Card title=t(locale, "app.reports.account_health.title") code="RPT-ACC-01"
              sub=tf(locale, "app.reports.account_health.sub", &[
                  ("total", &fmt_int(total)),
                  ("count", &d.accounts.len().to_string()),
              ])>
            <div class="vstack" style="gap:14px">
                {rows.into_iter().map(|a| {
                    let tone = ep_core::Tone::parse(&a.tone);
                    let _ = a.code;
                    view! {
                        <div class="acc-row">
                            <div class="acc-row-meta">
                                <div class="acc-row-name">
                                    {a.name.clone()}
                                </div>
                                <div class="acc-row-tags">
                                    <Tag tone=tone>{a.r#type.clone()}</Tag>
                                    <span class="mono acc-row-pct">{tf(locale, "app.reports.account_health.pct", &[("pct", &a.pct.to_string())])}</span>
                                </div>
                            </div>
                            <div class="acc-row-rhs">
                                <div class="mono acc-row-balance">
                                    {format!("¥{}", fmt_money(a.balance))}
                                </div>
                                <Ring pct=a.pct size=56/>
                            </div>
                        </div>
                    }
                }).collect_view()}
            </div>
            <div class="acc-row-footer">
                <Icon kind=IconKind::Coin size=12/>
                {t(locale, "app.reports.account_health.footer")}
            </div>
        </Card>
    }
}
