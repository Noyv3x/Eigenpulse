use ep_core::IconKind;
use ep_core::{fmt_int, fmt_money};
use ep_finance::model::{Account, CategorySummary, MonthBucket};
use ep_i18n::{t, tf, use_locale};
use ep_ui::{kpi::Direction, Card, ChartBars, Icon, Kpi, PageHead, Ring, Tag};
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
    /// All accounts with current balance, paired with their
    /// share of the total positive balance (rendered as the per-account
    /// Ring fill). Vec stays parallel to `accounts` by index.
    pub accounts: Vec<Account>,
    pub account_pcts: Vec<u32>,
    pub year_income: f64,
    pub year_expense: f64,
    pub year_savings_rate: f32,
}

#[server(LoadReports, "/api/_internal/rpt", "Url", "load_reports")]
pub async fn load_reports() -> Result<ReportsData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        let pool = &st.db;

        // 12-month bucket: income (positive, tag='inc') + expense (magnitude
        // of negative amounts) per YYYY-MM. Use 'localtime' for the lower
        // bound + the period grouping so the buckets line up with the user's
        // wall clock — same rationale as today.rs.
        type MonthRow = (String, f64, f64);
        // Every "expense" aggregation filters `tag = 'exp'` so transfer
        // rows (`tag='tfr'`, from-leg amount<0) don't pollute the totals.
        let months_q = sqlx::query_as::<_, MonthRow>(
            "SELECT strftime('%Y-%m', occurred_at, 'unixepoch', 'localtime') AS period,
                    COALESCE(SUM(CASE WHEN tag='inc' AND amount > 0 THEN amount ELSE 0.0 END), 0.0) AS income,
                    COALESCE(SUM(CASE WHEN tag='exp' AND amount < 0 THEN -amount ELSE 0.0 END), 0.0) AS expense
               FROM fin_txn
              WHERE occurred_at >= unixepoch('now','localtime','start of month','-11 months','utc')
              GROUP BY period
              ORDER BY period ASC"
        ).fetch_all(pool);

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

        // 12 month period labels (oldest → newest), local-tz aware. Built
        // server-side so the client always sees a 12-bar chart even when
        // the user has only one month of activity (e.g. fresh install).
        let frame: Vec<String> = sqlx::query_scalar(
            "WITH RECURSIVE months(p, n) AS (
                SELECT strftime('%Y-%m','now','localtime','start of month','-11 months'), 0
                UNION ALL
                SELECT strftime('%Y-%m','now','localtime','start of month',
                                printf('-%d months', 11 - n - 1)), n + 1
                  FROM months
                 WHERE n + 1 < 12
             )
             SELECT p FROM months ORDER BY p ASC",
        )
        .fetch_all(pool)
        .await
        .map_err(server_err)?;

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

        // Left-join the dense 12-month frame with the sparse aggregates so
        // missing months render as zero-height bars rather than vanishing.
        let mut by_period: std::collections::HashMap<String, (f64, f64)> =
            std::collections::HashMap::new();
        for (p, income, expense) in month_rows {
            by_period.insert(p, (income, expense));
        }
        let months: Vec<MonthBucket> = frame
            .into_iter()
            .map(|period| {
                let (income, expense) = by_period.get(&period).copied().unwrap_or((0.0, 0.0));
                MonthBucket {
                    period,
                    income,
                    expense,
                    net: income - expense,
                }
            })
            .collect();

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
        let accounts: Vec<Account> = acc_rows
            .into_iter()
            .map(|(code, name, r#type, tone, balance)| {
                // `created_at` is not rendered here; safe defaults.
                Account {
                    code,
                    name,
                    r#type,
                    tone,
                    balance,
                    archived: false,
                    created_at: 0,
                }
            })
            .collect();
        let account_pcts: Vec<u32> = accounts
            .iter()
            .map(|a| {
                if acc_total > 0.0 {
                    (a.balance.max(0.0) / acc_total * 100.0).round() as u32
                } else {
                    0
                }
            })
            .collect();

        let year_savings_rate = if year_income > 0.0 {
            ((year_income - year_expense) / year_income).max(0.0) as f32
        } else {
            0.0
        };

        Ok(ReportsData {
            months,
            category_30d,
            accounts,
            account_pcts,
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
                title="Reports"
                title_cn=t(locale, "app.reports.page.title_cn")
                sub=t(locale, "app.reports.page.sub")
            />
            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:200px">{t(locale, "app.common.loading")}</div> }>
                {move || data.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">{t(locale, "app.common.load_failed")} " · " {e.to_string()}</div></div> }.into_any(),
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
    let net_strip = ep_finance::view::render_net_strip(&d.months);
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
                view! { <p class="muted">{t(locale, "app.reports.category.empty")}</p> }.into_any()
            } else {
                view! {
                    <div class="vstack" style="gap:10px">
                        {cats.into_iter().map(|c| {
                            let bar_color = if c.tone.is_empty() {
                                "var(--primary)".to_string()
                            } else {
                                format!("var(--{})", c.tone)
                            };
                            // Cap visible width at the highest category share so the
                            // bars never look uniformly tiny (same rule the FIN
                            // finance expense-mix card uses, see modules/finance/src/view.rs).
                            let pct = (c.pct * 3.0).min(100.0);
                            view! {
                                <div>
                                    <div class="cat-row-head">
                                        <div>
                                            <span>{c.name.clone()}</span>
                                            <span class="mono dim cat-row-code">{c.code.clone()}</span>
                                        </div>
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
    // Pair each account with its precomputed percentage so the iterator over
    // owned data doesn't need to thread through the parallel `account_pcts`
    // by index.
    let rows: Vec<(Account, u32)> = d
        .accounts
        .iter()
        .cloned()
        .zip(d.account_pcts.iter().copied())
        .collect();
    view! {
        <Card title=t(locale, "app.reports.account_health.title") code="RPT-ACC-01"
              sub=tf(locale, "app.reports.account_health.sub", &[
                  ("total", &fmt_int(total)),
                  ("count", &d.accounts.len().to_string()),
              ])>
            <div class="vstack" style="gap:14px">
                {rows.into_iter().map(|(a, pct)| {
                    let tone = ep_core::Tone::from_str(&a.tone);
                    view! {
                        <div class="acc-row">
                            <div class="acc-row-meta">
                                <div class="acc-row-name">
                                    {a.name.clone()}
                                    <span class="mono dim acc-row-code">{a.code.clone()}</span>
                                </div>
                                <div class="acc-row-tags">
                                    <Tag tone=tone>{a.r#type.clone()}</Tag>
                                    <span class="mono acc-row-pct">{tf(locale, "app.reports.account_health.pct", &[("pct", &pct.to_string())])}</span>
                                </div>
                            </div>
                            <div class="acc-row-rhs">
                                <div class="mono acc-row-balance">
                                    {format!("¥{}", fmt_money(a.balance))}
                                </div>
                                <Ring pct=pct size=56/>
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
