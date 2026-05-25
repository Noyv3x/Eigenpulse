use ep_core::IconKind;
use ep_core::{fmt_minor, fmt_minor_compact, MinorAmount};
use ep_finance::{CategorySummary, Currency, MonthBucket};
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
    /// 12 most recent months, oldest → newest. Amounts in `currency`'s minor units.
    pub months: Vec<MonthBucket>,
    /// Category breakdown over the last 30 days (descending by value).
    pub category_30d: Vec<CategorySummary>,
    /// All accounts with current balance and their share of the total
    /// positive balance, rendered as the per-account Ring fill.
    pub accounts: Vec<ReportAccount>,
    pub year_income: MinorAmount,
    pub year_expense: MinorAmount,
    pub year_savings_rate: f32,
    /// The primary currency this cross-module report is scoped to.
    pub currency: Currency,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportAccount {
    pub code: String,
    pub name: String,
    pub r#type: String,
    pub tone: String,
    /// Balance in the primary currency's minor units.
    pub balance: MinorAmount,
    pub pct: u32,
}

fn category_summary_label(c: &CategorySummary) -> String {
    let icon = c.icon.trim();
    if icon.is_empty() {
        c.name.clone()
    } else {
        format!("{icon} {}", c.name)
    }
}

#[server(LoadReports, "/api/_internal/rpt", "Url", "load_reports")]
pub async fn load_reports() -> Result<ReportsData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;
        let pool = &st.db;

        // Cross-module report: scoped to the primary currency only.
        let currency = ep_finance::resolve_currency(pool, "").await?;
        let cc = currency.code.clone();

        // 30-day category share: only true expenses (tag='exp').
        type CatRow = (String, MinorAmount);
        let cat_30d_q = sqlx::query_as::<_, CatRow>(
            "SELECT category_code, amount
               FROM fin_txn
              WHERE currency_code = ?1 AND tag = 'exp'
                AND occurred_at >= unixepoch('now','-30 days')",
        )
        .bind(&cc)
        .fetch_all(pool);

        type CatMetaRow = (String, String, String, String);
        let cat_meta_q = sqlx::query_as::<_, CatMetaRow>(
            "SELECT code, name, icon, tone FROM fin_category WHERE currency_code = ?1",
        )
        .bind(&cc)
        .fetch_all(pool);

        type AccRow = (String, String, String, String, MinorAmount);
        let accounts_q = sqlx::query_as::<_, AccRow>(
            "SELECT code, name, type, tone, balance
               FROM fin_account WHERE currency_code = ?1 ORDER BY code",
        )
        .bind(&cc)
        .fetch_all(pool);

        type YearTxnRow = (MinorAmount, String);
        let year_txns_q = sqlx::query_as::<_, YearTxnRow>(
            "SELECT amount, tag FROM fin_txn
              WHERE currency_code = ?1
                AND occurred_at >= unixepoch('now','localtime','start of year','utc')",
        )
        .bind(&cc)
        .fetch_all(pool);

        let months_q = ep_finance::load_month_buckets_12(pool, &cc);

        let (month_rows, cat_rows, cat_meta, acc_rows, year_txns) =
            tokio::try_join!(months_q, cat_30d_q, cat_meta_q, accounts_q, year_txns_q,)
                .map_err(server_err)?;

        let mut cat_totals: std::collections::HashMap<String, MinorAmount> =
            std::collections::HashMap::new();
        for (code, amount) in cat_rows {
            if amount.is_negative() {
                *cat_totals.entry(code).or_default() += amount.abs();
            }
        }
        let cat_total: MinorAmount = cat_totals.values().copied().sum();
        let mut category_30d: Vec<CategorySummary> = cat_totals
            .into_iter()
            .map(|(code, value)| {
                let meta = cat_meta.iter().find(|(c, _, _, _)| *c == code);
                CategorySummary {
                    code: code.clone(),
                    name: meta.map(|m| m.1.clone()).unwrap_or_default(),
                    icon: meta.map(|m| m.2.clone()).unwrap_or_default(),
                    tone: meta.map(|m| m.3.clone()).unwrap_or_default(),
                    value,
                    pct: if cat_total.is_positive() {
                        (value.to_f64() / cat_total.to_f64() * 1000.0).round() / 10.0
                    } else {
                        0.0
                    },
                }
            })
            .collect();
        category_30d.sort_by_key(|c| std::cmp::Reverse(c.value));

        let acc_total: MinorAmount = acc_rows.iter().map(|r| r.4.max(MinorAmount::ZERO)).sum();
        let accounts: Vec<ReportAccount> = acc_rows
            .into_iter()
            .map(|(code, name, r#type, tone, balance)| {
                let pct = if acc_total.is_positive() {
                    (balance.max(MinorAmount::ZERO).to_f64() / acc_total.to_f64() * 100.0).round()
                        as u32
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

        let year_income: MinorAmount = year_txns
            .iter()
            .filter(|(amount, tag)| tag == "inc" && amount.is_positive())
            .map(|(amount, _)| *amount)
            .sum();
        let year_expense: MinorAmount = year_txns
            .iter()
            .filter(|(amount, tag)| tag == "exp" && amount.is_negative())
            .map(|(amount, _)| amount.abs())
            .sum();
        let year_savings_rate = if year_income.is_positive() {
            ((year_income - year_expense).to_f64() / year_income.to_f64()).max(0.0) as f32
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
            currency,
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
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.clone();
    let months_count = d.months.len();
    let income_value = format!("{}{}", symbol, fmt_minor_compact(d.year_income, decimals));
    let expense_value = format!("{}{}", symbol, fmt_minor_compact(d.year_expense, decimals));
    let net_savings_delta = tf(
        locale,
        "app.reports.kpi.net_savings",
        &[
            ("symbol", &symbol),
            (
                "amount",
                &fmt_minor_compact(
                    (d.year_income - d.year_expense).max(MinorAmount::ZERO),
                    decimals,
                ),
            ),
        ],
    );
    let kpis = view! {
        <div class="kpi-grid">
            <Kpi code="RPT-K01" label=t(locale, "app.reports.kpi.income") value=income_value
                 delta=t(locale, "app.reports.kpi.ytd") dir=Direction::Up/>
            <Kpi code="RPT-K02" label=t(locale, "app.reports.kpi.expense") value=expense_value
                 delta=t(locale, "app.reports.kpi.ytd") dir=Direction::Down/>
            <Kpi code="RPT-K03" label=t(locale, "app.reports.kpi.savings_rate")
                 value=format!("{}", (d.year_savings_rate * 100.0).round() as u32)
                 unit="%".to_string()
                 delta=net_savings_delta
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
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.clone();
    let labels: Vec<String> = d
        .months
        .iter()
        // Month label "Apr" / "May" — short form fits the 12-col grid.
        .map(|m| m.period.split('-').nth(1).unwrap_or("?").to_string())
        .collect();
    // ChartBars takes f64 heights; accounting amounts stay exact elsewhere.
    let income_data: Vec<f64> = d.months.iter().map(|m| m.income.to_f64()).collect();
    let expense_data: Vec<f64> = d.months.iter().map(|m| m.expense.to_f64()).collect();
    let (last_in, last_out, last_net) = d
        .months
        .last()
        .map(|m| (m.income, m.expense, m.net))
        .unwrap_or((MinorAmount::ZERO, MinorAmount::ZERO, MinorAmount::ZERO));
    let net_strip = ep_finance::render_net_strip(&d.months, decimals);
    view! {
        <Card title=t(locale, "app.reports.month.title") code="RPT-MTH-01"
              sub=tf(locale, "app.reports.month.sub", &[
                  ("symbol", &symbol),
                  ("count", &d.months.len().to_string()),
                  ("net", &fmt_minor_compact(last_net, decimals)),
                  ("income", &fmt_minor_compact(last_in, decimals)),
                  ("expense", &fmt_minor_compact(last_out, decimals)),
              ])>
            <div class="vstack" style="gap:14px">
                <div>
                    <div class="mono dim chart-row-label">{t(locale, "app.reports.month.income")}</div>
                    <ChartBars data=income_data labels=labels.clone()/>
                </div>
                <div>
                    <div class="mono dim chart-row-label">{t(locale, "app.reports.month.expense")}</div>
                    <ChartBars data=expense_data labels=labels class="expense"/>
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
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.clone();
    let title_sub = tf(
        locale,
        "app.reports.category.sub",
        &[
            ("symbol", &symbol),
            (
                "total",
                &fmt_minor_compact(
                    d.category_30d.iter().map(|c| c.value).sum::<MinorAmount>(),
                    decimals,
                ),
            ),
        ],
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
                            let value = format!("{}{}", symbol, fmt_minor_compact(c.value, decimals));
                            let label = category_summary_label(&c);
                            let _ = c.code;
                            view! {
                                <div>
                                    <div class="cat-row-head">
                                        <div><span>{label}</span></div>
                                        <div class="mono cat-row-value">
                                            {value}
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
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.clone();
    let total = d.accounts.iter().map(|a| a.balance).sum::<MinorAmount>();
    let rows = d.accounts.clone();
    view! {
        <Card title=t(locale, "app.reports.account_health.title") code="RPT-ACC-01"
              sub=tf(locale, "app.reports.account_health.sub", &[
                  ("symbol", &symbol),
                  ("total", &fmt_minor_compact(total, decimals)),
                  ("count", &d.accounts.len().to_string()),
              ])>
            <div class="vstack" style="gap:14px">
                {rows.into_iter().map(|a| {
                    let tone = ep_core::Tone::parse(&a.tone);
                    let balance = format!("{}{}", symbol, fmt_minor(a.balance, decimals));
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
                                    {balance}
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
