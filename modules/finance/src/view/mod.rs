use crate::model::*;
use crate::server_fns::*;
use ep_i18n::{server_fn_error_text, t, use_locale};
use ep_ui::{
    AxisChart, AxisSeries, Card, Chart, ChartDatum, ChartHeight, ChartSpec, ChartTone, ChartValue,
    Direction, DonutChart, Field, HorizontalBarChart, Kpi, LoadError, PageHead,
};
use leptos::prelude::*;

#[derive(Clone)]
struct MoneyFormatter {
    symbol: String,
    decimals: u8,
}

impl MoneyFormatter {
    fn format(&self, amount: MinorAmount) -> String {
        crate::charts::format_money(&self.symbol, self.decimals, amount)
    }
}

#[component]
pub fn FinanceView() -> impl IntoView {
    let locale = use_locale();
    let selected_currency = RwSignal::new(0_i64);
    let create_account = ServerAction::<CreateFinanceAccount>::new();
    let create_category = ServerAction::<CreateFinanceCategory>::new();
    let add_txn = ServerAction::<AddFinanceTxn>::new();
    let delete_txn = ServerAction::<DeleteFinanceTxn>::new();
    let add_transfer = ServerAction::<AddFinanceTransfer>::new();
    let set_budget = ServerAction::<SetFinanceBudget>::new();

    let data = Resource::new(
        move || {
            (
                selected_currency.get(),
                create_account.version().get(),
                create_category.version().get(),
                add_txn.version().get(),
                delete_txn.version().get(),
                add_transfer.version().get(),
                set_budget.version().get(),
            )
        },
        |(currency_id, ..)| async move { load_finance_data(currency_id).await },
    );

    view! {
        <PageHead
            module=t(locale, "finance.page.module").to_string()
            title=t(locale, "finance.page.title").to_string()
            title_cn=t(locale, "finance.page.title_cn").to_string()
            sub=t(locale, "finance.page.sub").to_string()
        />
        <Suspense fallback=move || view! { <div class="card"><div class="card-body">{t(locale, "app.common.loading")}</div></div> }>
            {move || match data.get() {
                Some(Ok(payload)) => render_finance(
                    payload,
                    selected_currency,
                    create_account,
                    create_category,
                    add_txn,
                    delete_txn,
                    add_transfer,
                    set_budget,
                ).into_any(),
                Some(Err(error)) => view! {
                    <LoadError detail=server_fn_error_text(&error)/>
                }.into_any(),
                None => ().into_any(),
            }}
        </Suspense>
    }
}

#[allow(clippy::too_many_arguments)]
fn render_finance(
    data: FinanceData,
    selected_currency: RwSignal<i64>,
    create_account: ServerAction<CreateFinanceAccount>,
    create_category: ServerAction<CreateFinanceCategory>,
    add_txn: ServerAction<AddFinanceTxn>,
    delete_txn: ServerAction<DeleteFinanceTxn>,
    add_transfer: ServerAction<AddFinanceTransfer>,
    set_budget: ServerAction<SetFinanceBudget>,
) -> impl IntoView {
    let locale = use_locale();
    let currency = data.currency.clone();
    let currency_id = currency.id;
    let money = MoneyFormatter {
        symbol: currency.symbol.clone(),
        decimals: currency.decimals,
    };

    let currency_options = data.currencies.clone();
    let accounts = data.accounts.clone();
    let active_accounts: Vec<Account> = accounts
        .iter()
        .filter(|account| !account.archived)
        .cloned()
        .collect();
    let transfer_accounts: Vec<TransferAccountRef> = data
        .transfer_accounts
        .iter()
        .filter(|account| !account.archived)
        .cloned()
        .collect();
    let categories = data.categories.clone();
    let active_categories: Vec<Category> = categories
        .iter()
        .filter(|category| !category.archived)
        .cloned()
        .collect();
    let transactions = data.transactions.clone();
    let budgets = data.budgets.clone();
    let months = data.months_12.clone();
    let category_summary = data.category_summary.clone();
    let budget_rows = budgets.clone();
    let month_rows = months.clone();
    let category_rows = category_summary.clone();
    let csv_uri = format!("/finance/export.csv?currency_id={currency_id}");

    let account_options_for_txn = active_accounts.clone();
    let category_options_for_txn = active_categories.clone();
    let category_options_for_budget = active_categories.clone();
    let from_accounts = transfer_accounts.clone();
    let to_accounts = transfer_accounts.clone();
    let can_add_txn = !active_accounts.is_empty() && !active_categories.is_empty();
    let has_categories = !active_categories.is_empty();
    let txn_money = money.clone();
    let account_money = money.clone();
    let budget_money = money.clone();
    let month_money = money.clone();
    let category_money = money.clone();
    let trend_window = RwSignal::new(12_usize);
    let trend_months = months.clone();
    let trend_symbol = currency.symbol.clone();
    let trend_decimals = currency.decimals;
    let trend_income_label = t(locale, "finance.chart.income").to_string();
    let trend_expense_label = t(locale, "finance.chart.expense").to_string();
    let trend_net_label = t(locale, "finance.chart.net").to_string();
    let trend_axis_label = format!(
        "{} ({})",
        t(locale, "finance.chart.amount_axis"),
        currency.code
    );
    let trend_spec = Signal::derive(move || {
        cashflow_spec(
            &trend_months,
            trend_window.get(),
            &trend_symbol,
            trend_decimals,
            &trend_income_label,
            &trend_expense_label,
            &trend_net_label,
            &trend_axis_label,
        )
    });
    let category_spec = spending_mix_spec(
        &category_summary,
        t(locale, "finance.chart.category.other"),
        &currency.symbol,
        currency.decimals,
        t(locale, "finance.chart.category.center"),
        money.format(data.month.expense),
    );
    let budget_spec = budget_utilization_spec(&budgets, &currency.symbol, currency.decimals);
    let range_options = [
        (3_usize, t(locale, "finance.chart.range.3")),
        (6_usize, t(locale, "finance.chart.range.6")),
        (12_usize, t(locale, "finance.chart.range.12")),
    ];

    view! {
        <div class="finance-toolbar card">
            <div class="card-body hstack" style="justify-content:space-between;gap:12px;flex-wrap:wrap">
                <Field label=t(locale, "finance.field.currency").to_string()>
                    <select
                        class="ep-select"
                        prop:value=currency_id.to_string()
                        on:change=move |event| {
                            if let Ok(id) = event_target_value(&event).parse::<i64>() {
                                selected_currency.set(id);
                            }
                        }
                    >
                        {currency_options.into_iter().map(|item| view! {
                            <option value=item.id.to_string()>{format!("{} · {}", item.code, item.remark)}</option>
                        }).collect_view()}
                    </select>
                </Field>
                <a class="btn" href=csv_uri download=format!("finance-{}.csv", currency.code)>
                    {t(locale, "finance.action.export")}
                </a>
            </div>
        </div>

        <div class="kpi-grid">
            <Kpi
                label=t(locale, "finance.summary.income").to_string()
                value=money.format(data.month.income)
                delta=data.month.period.clone()
                dir=Direction::Up
            />
            <Kpi
                label=t(locale, "finance.summary.expense").to_string()
                value=money.format(data.month.expense)
                delta=data.month.period.clone()
                dir=Direction::Down
            />
            <Kpi
                label=t(locale, "finance.summary.savings").to_string()
                value=money.format(data.month.savings)
                delta=format!("{} {}", data.month.transaction_count, t(locale, "finance.summary.transactions"))
                dir=if data.month.savings.is_negative() { Direction::Down } else { Direction::Up }
            />
            <Kpi
                label=t(locale, "finance.summary.balance").to_string()
                value=money.format(data.month.balance)
                delta=currency.code.clone()
            />
        </div>

        <div class="grid-2">
            <Card
                title=t(locale, "finance.card.new.title").to_string()
                sub=t(locale, "finance.card.new.sub").to_string()
            >
                {if !can_add_txn {
                    view! { <p class="muted">{t(locale, "finance.empty.setup_first")}</p> }.into_any()
                } else {
                    view! {
                        <ActionForm action=add_txn>
                            <input type="hidden" name="currency_id" value=currency_id/>
                            <div class="form-grid">
                                <Field label=t(locale, "finance.field.merchant_desc").to_string()>
                                    <input class="ep-input" name="merchant" required maxlength="128"/>
                                </Field>
                                <Field label=t(locale, "finance.field.amount").to_string()>
                                    <input class="ep-input" name="amount" required inputmode="decimal" placeholder="0.00"/>
                                </Field>
                                <Field label=t(locale, "finance.field.tag").to_string()>
                                    <select class="ep-select" name="tag">
                                        <option value="exp">{t(locale, "finance.tag.short_exp")}</option>
                                        <option value="inc">{t(locale, "finance.tag.short_inc")}</option>
                                    </select>
                                </Field>
                                <Field label=t(locale, "finance.field.account").to_string()>
                                    <select class="ep-select" name="account_id" required>
                                        {account_options_for_txn.into_iter().map(|account| view! {
                                            <option value=account.id>{account.name}</option>
                                        }).collect_view()}
                                    </select>
                                </Field>
                                <Field label=t(locale, "finance.field.category").to_string()>
                                    <select class="ep-select" name="category_id" required>
                                        {category_options_for_txn.into_iter().map(|category| view! {
                                            <option value=category.id>{format!("{} {}", category.icon, category.name)}</option>
                                        }).collect_view()}
                                    </select>
                                </Field>
                                <Field label=t(locale, "finance.field.date_default").to_string()>
                                    <input class="ep-input" type="date" name="occurred_at"/>
                                </Field>
                                <Field label=t(locale, "finance.field.note_optional").to_string() wide=true>
                                    <input class="ep-input" name="note" maxlength="2000"/>
                                </Field>
                            </div>
                            <button class="btn primary" type="submit" disabled=move || add_txn.pending().get()>
                                {t(locale, "finance.action.add_txn")}
                            </button>
                        </ActionForm>
                        <span class="error-slot">{move || add_txn.value().get().and_then(Result::err).map(|e| server_fn_error_text(&e))}</span>
                    }.into_any()
                }}
            </Card>

            <Card
                title=t(locale, "finance.card.transfer.title").to_string()
                sub=t(locale, "finance.card.transfer.sub").to_string()
            >
                {if transfer_accounts.len() < 2 {
                    view! { <p class="muted">{t(locale, "finance.card.transfer.needs_two_accounts")}</p> }.into_any()
                } else {
                    view! {
                        <ActionForm action=add_transfer>
                            <div class="form-grid">
                                <Field label=t(locale, "finance.field.from_account").to_string()>
                                    <select class="ep-select" name="from_account_id" required>
                                        {from_accounts.into_iter().map(|account| view! {
                                            <option value=account.id>{format!("{} · {}", account.currency_code, account.name)}</option>
                                        }).collect_view()}
                                    </select>
                                </Field>
                                <Field label=t(locale, "finance.field.to_account").to_string()>
                                    <select class="ep-select" name="to_account_id" required>
                                        {to_accounts.into_iter().map(|account| view! {
                                            <option value=account.id>{format!("{} · {}", account.currency_code, account.name)}</option>
                                        }).collect_view()}
                                    </select>
                                </Field>
                                <Field label=t(locale, "finance.field.transfer_out").to_string()>
                                    <input class="ep-input" name="from_amount" required inputmode="decimal"/>
                                </Field>
                                <Field label=t(locale, "finance.field.transfer_in").to_string()>
                                    <input class="ep-input" name="to_amount" required inputmode="decimal"/>
                                </Field>
                                <Field label=t(locale, "finance.field.date_default").to_string()>
                                    <input class="ep-input" type="date" name="occurred_at"/>
                                </Field>
                                <Field label=t(locale, "finance.field.note_optional").to_string()>
                                    <input class="ep-input" name="note" maxlength="2000"/>
                                </Field>
                            </div>
                            <button class="btn" type="submit" disabled=move || add_transfer.pending().get()>
                                {t(locale, "finance.action.transfer")}
                            </button>
                        </ActionForm>
                        <span class="error-slot">{move || add_transfer.value().get().and_then(Result::err).map(|e| server_fn_error_text(&e))}</span>
                    }.into_any()
                }}
            </Card>
        </div>

        <Card
            title=t(locale, "finance.card.ledger.title").to_string()
            sub=format!("{} · {}", data.month.period, currency.code)
        >
            {if transactions.is_empty() {
                view! { <div class="empty-state">{t(locale, "finance.empty.transactions")}</div> }.into_any()
            } else {
                view! {
                    <div class="scroll-x">
                        <table class="tbl">
                            <thead><tr>
                                <th>{t(locale, "finance.field.date")}</th>
                                <th>{t(locale, "finance.field.merchant")}</th>
                                <th>{t(locale, "finance.field.category")}</th>
                                <th>{t(locale, "finance.field.account")}</th>
                                <th class="num">{t(locale, "finance.field.amount")}</th>
                                <th>{t(locale, "finance.field.ops")}</th>
                            </tr></thead>
                            <tbody>
                                {transactions.into_iter().map(|txn| {
                                    let id = txn.id;
                                    let amount_class = if txn.amount.is_negative() { "num amt-neg" } else { "num amt-pos" };
                                    view! {
                                        <tr>
                                            <td class="mono">{txn.occurred_date}</td>
                                            <td>{txn.merchant}</td>
                                            <td>{txn.category_name.unwrap_or_else(|| t(locale, "finance.transfer.label").to_string())}</td>
                                            <td>{txn.account_name}</td>
                                            <td class=amount_class>{txn_money.format(txn.amount)}</td>
                                            <td>
                                                <ActionForm action=delete_txn>
                                                    <input type="hidden" name="id" value=id/>
                                                    <button class="btn sm" type="submit">{t(locale, "finance.action.delete")}</button>
                                                </ActionForm>
                                            </td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                    <span class="error-slot">{move || delete_txn.value().get().and_then(Result::err).map(|e| server_fn_error_text(&e))}</span>
                }.into_any()
            }}
        </Card>

        <div class="grid-2">
            <Card
                title=t(locale, "finance.tab.accounts").to_string()
                sub=t(locale, "finance.setup.accounts_sub").to_string()
            >
                <ActionForm action=create_account>
                    <input type="hidden" name="currency_id" value=currency_id/>
                    <div class="form-grid">
                        <Field label=t(locale, "finance.field.name").to_string()>
                            <input class="ep-input" name="name" required maxlength="80"/>
                        </Field>
                        <Field label=t(locale, "finance.field.type").to_string()>
                            <select class="ep-select" name="type">
                                {ACCOUNT_TYPES.iter().map(|kind| view! { <option value=*kind>{*kind}</option> }).collect_view()}
                            </select>
                        </Field>
                        <Field label=t(locale, "finance.field.opening_balance").to_string()>
                            <input class="ep-input" name="opening_balance" value="0" inputmode="decimal"/>
                        </Field>
                        <Field label=t(locale, "finance.field.tone").to_string()>
                            <select class="ep-select" name="tone"><option value=""></option>{TONES.iter().map(|tone| view! { <option value=*tone>{*tone}</option> }).collect_view()}</select>
                        </Field>
                    </div>
                    <button class="btn" type="submit">{t(locale, "finance.action.create")}</button>
                </ActionForm>
                <span class="error-slot">{move || create_account.value().get().and_then(Result::err).map(|e| server_fn_error_text(&e))}</span>
                {accounts.into_iter().map(|account| view! {
                    <div class="list-row">
                        <span>{account.name}</span>
                        <span class="mono">{account_money.format(account.balance)}</span>
                    </div>
                }).collect_view()}
            </Card>

            <Card
                title=t(locale, "finance.tab.categories").to_string()
                sub=t(locale, "finance.setup.categories_sub").to_string()
            >
                <ActionForm action=create_category>
                    <input type="hidden" name="currency_id" value=currency_id/>
                    <div class="form-grid">
                        <Field label=t(locale, "finance.field.name").to_string()>
                            <input class="ep-input" name="name" required maxlength="80"/>
                        </Field>
                        <Field label=t(locale, "finance.field.icon").to_string()>
                            <input class="ep-input" name="icon" maxlength="16"/>
                        </Field>
                        <Field label=t(locale, "finance.field.tone").to_string() wide=true>
                            <select class="ep-select" name="tone"><option value=""></option>{TONES.iter().map(|tone| view! { <option value=*tone>{*tone}</option> }).collect_view()}</select>
                        </Field>
                    </div>
                    <button class="btn" type="submit">{t(locale, "finance.action.create")}</button>
                </ActionForm>
                <span class="error-slot">{move || create_category.value().get().and_then(Result::err).map(|e| server_fn_error_text(&e))}</span>
                {categories.into_iter().map(|category| view! {
                    <div class="list-row"><span>{format!("{} {}", category.icon, category.name)}</span></div>
                }).collect_view()}
            </Card>
        </div>

        <Card
            title=t(locale, "finance.chart.trend.title").to_string()
            sub=t(locale, "finance.reports.module_local").to_string()
        >
            <div class="hstack" style="justify-content:flex-end;gap:6px;margin-bottom:12px">
                {range_options.into_iter().map(|(window, label)| view! {
                    <button
                        data-testid=format!("finance-trend-range-{window}")
                        class=move || if trend_window.get() == window { "btn sm primary" } else { "btn sm" }
                        type="button"
                        aria-pressed=move || (trend_window.get() == window).to_string()
                        on:click=move |_| trend_window.set(window)
                    >
                        {label}
                    </button>
                }).collect_view()}
            </div>
            <Chart
                label=t(locale, "finance.chart.trend.title")
                description=t(locale, "finance.chart.trend.description")
                spec=trend_spec
                height=ChartHeight::Tall
            />
        </Card>

        <div class="grid-2">
            <Card title=t(locale, "finance.chart.category.title").to_string()>
                {if category_summary.is_empty() {
                    view! { <p class="muted">{t(locale, "finance.empty.transactions")}</p> }.into_any()
                } else {
                    view! {
                        <Chart
                            label=t(locale, "finance.chart.category.title")
                            description=t(locale, "finance.chart.category.description")
                            spec=category_spec
                        />
                    }.into_any()
                }}
            </Card>

            <Card
                title=t(locale, "finance.chart.budget.title").to_string()
                sub=format!("{} · {}", t(locale, "finance.tab.budget"), data.month.period)
            >
                {if !has_categories {
                    view! { <p class="muted">{t(locale, "finance.empty.categories")}</p> }.into_any()
                } else {
                    view! {
                        <ActionForm action=set_budget>
                            <input type="hidden" name="currency_id" value=currency_id/>
                            <input type="hidden" name="period" value=data.month.period.clone()/>
                            <div class="form-grid">
                                <Field label=t(locale, "finance.field.category").to_string()>
                                    <select class="ep-select" name="category_id">
                                        {category_options_for_budget.into_iter().map(|category| view! { <option value=category.id>{category.name}</option> }).collect_view()}
                                    </select>
                                </Field>
                                <Field label=t(locale, "finance.field.amount").to_string()>
                                    <input class="ep-input" name="amount" inputmode="decimal" required/>
                                </Field>
                            </div>
                            <button class="btn" type="submit">{t(locale, "finance.action.save")}</button>
                        </ActionForm>
                        <span class="error-slot">{move || set_budget.value().get().and_then(Result::err).map(|e| server_fn_error_text(&e))}</span>
                        {if budgets.is_empty() {
                            view! { <p class="muted">{t(locale, "finance.chart.budget.empty")}</p> }.into_any()
                        } else {
                            view! {
                                <Chart
                                    label=t(locale, "finance.chart.budget.title")
                                    description=t(locale, "finance.chart.budget.description")
                                    spec=budget_spec
                                />
                            }.into_any()
                        }}
                        {budget_rows.into_iter().map(|budget| view! {
                            <div class="list-row">
                                <span>{budget.category_name}</span>
                                <span class="mono">{format!("{} / {}", budget_money.format(budget.used), budget_money.format(budget.amount))}</span>
                            </div>
                        }).collect_view()}
                    }.into_any()
                }}
            </Card>
        </div>

        <Card
            title=t(locale, "finance.tab.reports").to_string()
            sub=t(locale, "finance.reports.module_local").to_string()
        >
            {month_rows.into_iter().map(|month| view! {
                <div class="list-row">
                    <span class="mono">{month.period}</span>
                    <span class="mono">{month_money.format(month.net)}</span>
                </div>
            }).collect_view()}
            {category_rows.into_iter().map(|category| view! {
                <div class="list-row">
                    <span>{format!("{} {}", category.icon, category.name)}</span>
                    <span class="mono">{category_money.format(category.value)}</span>
                </div>
            }).collect_view()}
        </Card>
    }
}

#[allow(clippy::too_many_arguments)]
fn cashflow_spec(
    months: &[MonthBucket],
    window: usize,
    symbol: &str,
    decimals: u8,
    income_label: &str,
    expense_label: &str,
    net_label: &str,
    axis_label: &str,
) -> ChartSpec {
    let points = crate::charts::trend_points(months, window, symbol, decimals);
    ChartSpec::Axis(AxisChart {
        categories: points.iter().map(|point| point.label.clone()).collect(),
        series: vec![
            AxisSeries::bar(
                income_label,
                points
                    .iter()
                    .map(|point| {
                        Some(ChartValue::new(
                            point.income_geometry,
                            point.income_display.clone(),
                        ))
                    })
                    .collect(),
            )
            .with_tone(ChartTone::Positive),
            AxisSeries::bar(
                expense_label,
                points
                    .iter()
                    .map(|point| {
                        Some(ChartValue::new(
                            point.expense_geometry,
                            point.expense_display.clone(),
                        ))
                    })
                    .collect(),
            )
            .with_tone(ChartTone::Negative),
            AxisSeries::line(
                net_label,
                points
                    .iter()
                    .map(|point| {
                        Some(ChartValue::new(
                            point.net_geometry,
                            point.net_display.clone(),
                        ))
                    })
                    .collect(),
            )
            .with_tone(ChartTone::Primary)
            .smooth(true),
        ],
        y_label: Some(axis_label.to_string()),
        stacked: false,
    })
}

fn spending_mix_spec(
    categories: &[CategorySummary],
    other_label: &str,
    symbol: &str,
    decimals: u8,
    center_label: &str,
    center_value: String,
) -> ChartSpec {
    let segments = crate::charts::category_slices(categories, other_label, symbol, decimals)
        .unwrap_or_default()
        .into_iter()
        .map(|slice| {
            ChartDatum::new(slice.label, ChartValue::new(slice.geometry, slice.display))
                .with_tone(category_chart_tone(&slice.tone))
        })
        .collect();
    ChartSpec::Donut(DonutChart {
        segments,
        center_label: Some(center_label.to_string()),
        center_value: Some(center_value),
    })
}

fn budget_utilization_spec(budgets: &[Budget], symbol: &str, decimals: u8) -> ChartSpec {
    let items = crate::charts::budget_bars(budgets, symbol, decimals)
        .into_iter()
        .map(|bar| {
            ChartDatum::new(bar.label, ChartValue::new(bar.fill, bar.display)).with_tone(
                if bar.over_budget {
                    ChartTone::Negative
                } else {
                    ChartTone::Positive
                },
            )
        })
        .collect();
    ChartSpec::HorizontalBar(HorizontalBarChart {
        items,
        max: Some(1.0),
    })
}

fn category_chart_tone(tone: &str) -> ChartTone {
    match tone {
        "green" => ChartTone::Positive,
        "rose" => ChartTone::Negative,
        "amber" => ChartTone::Warning,
        "blue" | "violet" => ChartTone::Primary,
        _ => ChartTone::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cashflow_chart_keeps_exact_tooltips_for_all_three_series() {
        let months = [MonthBucket {
            period: "2026-07".into(),
            income: MinorAmount::from(123_456),
            expense: MinorAmount::from(23_456),
            net: MinorAmount::from(100_000),
        }];
        let ChartSpec::Axis(chart) = cashflow_spec(
            &months,
            12,
            "$",
            2,
            "Income",
            "Expense",
            "Net",
            "Amount (USD)",
        ) else {
            panic!("expected axis chart");
        };
        assert_eq!(chart.categories, ["2026-07"]);
        assert_eq!(chart.series.len(), 3);
        assert_eq!(
            chart.series[0].values[0]
                .as_ref()
                .map(|value| value.display.as_str()),
            Some("$1,234.56")
        );
        assert_eq!(
            chart.series[1].values[0]
                .as_ref()
                .map(|value| value.display.as_str()),
            Some("$234.56")
        );
        assert_eq!(
            chart.series[2].values[0]
                .as_ref()
                .map(|value| value.display.as_str()),
            Some("$1,000.00")
        );
    }

    #[test]
    fn over_budget_chart_items_use_negative_tone() {
        let budget = Budget {
            id: 1,
            currency_id: 1,
            currency_code: "USD".into(),
            period: "2026-07".into(),
            category_id: 1,
            category_name: "Food".into(),
            amount: MinorAmount::from(10_000),
            used: MinorAmount::from(11_000),
            created_at: 0,
            updated_at: 0,
        };
        let ChartSpec::HorizontalBar(chart) = budget_utilization_spec(&[budget], "$", 2) else {
            panic!("expected horizontal bar chart");
        };
        assert_eq!(chart.max, Some(1.0));
        assert_eq!(chart.items[0].tone, ChartTone::Negative);
        assert_eq!(chart.items[0].value.display, "$110.00 / $100.00");
    }
}
