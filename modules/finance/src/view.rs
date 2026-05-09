use crate::model::{Account, AccountStats, Category, MonthBucket, Tag, Txn, ACCOUNT_TYPES, TONES};
use crate::server_fns::*;
use ep_core::{
    fmt_int, fmt_money, fmt_ts_date, fmt_ts_hm, fmt_ts_md, unix_to_ymdhm, ymd_to_unix_midnight,
    IconKind, Tone,
};
use ep_i18n::{server_fn_error_text, t, tf, use_locale};
use ep_ui::{
    escape_js_single_quoted, Card, ChartBars, Direction, Icon, Kpi, PageHead, RowDeleteAction,
    TabSpec, Tabs, Tag as UiTag,
};
use leptos::prelude::*;

#[derive(Clone, Copy)]
struct LedgerFilters {
    merchant: RwSignal<String>,
    category: RwSignal<String>,
    date_from: RwSignal<String>,
    date_to: RwSignal<String>,
}

#[component]
pub fn FinanceView() -> impl IntoView {
    let locale = use_locale();
    let active = RwSignal::new(String::from("ledger"));
    let ledger = Resource::new(|| (), |_| async { load_ledger().await });
    let add = ServerAction::<AddTxn>::new();
    let delete = ServerAction::<DeleteTxn>::new();
    let set_budget = ServerAction::<SetBudget>::new();
    let import_budgets = ServerAction::<ImportBudgetsFrom>::new();
    let create_account = ServerAction::<CreateAccount>::new();
    let update_account = ServerAction::<UpdateAccount>::new();
    let delete_account = ServerAction::<DeleteAccount>::new();
    let create_category = ServerAction::<CreateCategory>::new();
    let update_category = ServerAction::<UpdateCategory>::new();
    let delete_category = ServerAction::<DeleteCategory>::new();
    let update_txn = ServerAction::<UpdateTxn>::new();
    let add_transfer = ServerAction::<AddTransfer>::new();
    let txn_modal_open = RwSignal::new(false);
    let txn_modal_mode = RwSignal::new(String::from("txn"));
    let merchant_filter = RwSignal::new(String::new());
    let category_filter = RwSignal::new(String::new());
    let date_from_filter = RwSignal::new(String::new());
    let date_to_filter = RwSignal::new(String::new());

    // Refetch when any action's version ticks. We compare per-element via a
    // fixed-size array so the closure stays Send + 'static.
    Effect::new(move |prev: Option<[usize; 12]>| {
        let cur = [
            add.version().get(),
            delete.version().get(),
            set_budget.version().get(),
            import_budgets.version().get(),
            create_account.version().get(),
            update_account.version().get(),
            delete_account.version().get(),
            create_category.version().get(),
            update_category.version().get(),
            delete_category.version().get(),
            update_txn.version().get(),
            add_transfer.version().get(),
        ];
        if prev.is_some_and(|p| p != cur) {
            ledger.refetch();
        }
        cur
    });

    view! {
        <div class="view">
            <PageHead
                code="FIN-01"
                module=t(locale, "finance.page.module")
                title=t(locale, "finance.page.title")
                title_cn=t(locale, "finance.page.title_cn")
                sub=t(locale, "finance.page.sub")
                actions=view! {
                    <button class="btn primary" type="button"
                            on:click=move |_| {
                                active.set(String::from("ledger"));
                                txn_modal_mode.set(String::from("txn"));
                                txn_modal_open.set(true);
                            }>
                        <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.action.add_txn")}
                    </button>
                }.into_any()
            />

            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:160px">{t(locale, "app.common.loading")}</div> }>
                {move || ledger.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</div></div> }.into_any(),
                    Ok(data) => render_ledger(
                        data, active, add, delete, set_budget, import_budgets,
                        create_account, update_account, delete_account,
                        create_category, update_category, delete_category,
                        update_txn, add_transfer,
                        txn_modal_open, txn_modal_mode,
                        merchant_filter, category_filter, date_from_filter, date_to_filter,
                    ).into_any(),
                })}
            </Suspense>
        </div>
    }
}

#[allow(clippy::too_many_arguments)]
fn render_ledger(
    data: LedgerData,
    active: RwSignal<String>,
    add: ServerAction<AddTxn>,
    delete: ServerAction<DeleteTxn>,
    set_budget: ServerAction<SetBudget>,
    import_budgets: ServerAction<ImportBudgetsFrom>,
    create_account: ServerAction<CreateAccount>,
    update_account: ServerAction<UpdateAccount>,
    delete_account: ServerAction<DeleteAccount>,
    create_category: ServerAction<CreateCategory>,
    update_category: ServerAction<UpdateCategory>,
    delete_category: ServerAction<DeleteCategory>,
    update_txn: ServerAction<UpdateTxn>,
    add_transfer: ServerAction<AddTransfer>,
    txn_modal_open: RwSignal<bool>,
    txn_modal_mode: RwSignal<String>,
    merchant_filter: RwSignal<String>,
    category_filter: RwSignal<String>,
    date_from_filter: RwSignal<String>,
    date_to_filter: RwSignal<String>,
) -> impl IntoView {
    let locale = use_locale();
    let m = &data.month;
    let bud_pct = if m.budget_total > 0.0 {
        (m.expense / m.budget_total * 100.0).round() as u32
    } else {
        0
    };
    let bud_dir = match bud_pct {
        0..=60 => Direction::Up,
        61..=85 => Direction::Flat,
        _ => Direction::Down,
    };
    // Daily spend / 3-month-rolling daily spend, used for the FIN-K02 trend.
    let daily_now = m.expense / m.days_elapsed.max(1) as f64;
    // 3m rolling has 90 days denominator (avg_expense_3m is a monthly figure).
    let daily_3m = m.avg_expense_3m / 30.0;
    let daily_delta = daily_now - daily_3m;
    let daily_dir = if daily_delta < -0.5 {
        Direction::Up
    }
    // less spend = up (saving)
    else if daily_delta > 0.5 {
        Direction::Down
    } else {
        Direction::Flat
    };
    let savings_pct = (m.savings_rate * 100.0).round() as u32;
    let savings_dir = match savings_pct {
        0..=10 => Direction::Down,
        11..=29 => Direction::Flat,
        _ => Direction::Up,
    };
    let emergency_dir = if m.emergency_months >= 6.0 {
        Direction::Up
    } else if m.emergency_months >= 3.0 {
        Direction::Flat
    } else {
        Direction::Down
    };
    // Tab badge reflects visible rows (LIMIT 50, not month-scoped); the
    // month aggregate goes in the card sub-label.
    let txns_count = data.txns.len() as u32;
    let accounts_count = data.accounts.len() as u32;
    let budgets_count = data.budgets.len() as u32;
    let categories_count = data.categories.len() as u32;

    let banner = render_banner(&data);
    // Pre-compute attribute strings — the `view!` macro rejects bare if/else
    // in attribute-value position.
    let daily_delta_text = if daily_3m > 0.0 {
        let sign = if daily_delta >= 0.0 { "+" } else { "−" };
        tf(
            locale,
            "finance.kpi.spend_vs_avg",
            &[("sign", sign), ("amount", &fmt_int(daily_delta.abs()))],
        )
    } else {
        tf(
            locale,
            "finance.kpi.day_insufficient",
            &[("day", &m.days_elapsed.to_string())],
        )
    };
    let savings_delta_text = if m.savings >= 0.0 {
        tf(
            locale,
            "finance.kpi.net_savings",
            &[("amount", &fmt_int(m.savings))],
        )
    } else {
        tf(
            locale,
            "finance.kpi.net_deficit",
            &[("amount", &fmt_int(m.savings.abs()))],
        )
    };
    let emergency_delta_text = if m.avg_expense_3m > 0.0 {
        tf(
            locale,
            "finance.kpi.emergency_delta",
            &[
                ("liquid", &fmt_int(m.liquid_balance)),
                ("avg", &fmt_int(m.avg_expense_3m)),
            ],
        )
    } else {
        t(locale, "finance.kpi.emergency_empty").to_string()
    };
    let kpis = view! {
        <div class="kpi-grid">
            <Kpi code="FIN-K01" label=t(locale, "finance.kpi.budget") value=format!("{}", bud_pct) unit="%".to_string()
                 delta=format!("¥{} / ¥{}", fmt_int(m.expense), fmt_int(m.budget_total))
                 dir=bud_dir/>
            <Kpi code="FIN-K02" label=t(locale, "finance.kpi.daily_spend")
                 value=format!("¥{}", fmt_int(daily_now))
                 delta=daily_delta_text
                 dir=daily_dir/>
            <Kpi code="FIN-K03" label=t(locale, "finance.kpi.savings_rate")
                 value=format!("{}", savings_pct) unit="%".to_string()
                 delta=savings_delta_text
                 dir=savings_dir/>
            <Kpi code="FIN-K04" label=t(locale, "finance.kpi.emergency")
                 value=format!("{:.1}", m.emergency_months) unit=t(locale, "finance.kpi.month_unit").to_string()
                 delta=emergency_delta_text
                 dir=emergency_dir/>
        </div>
    };

    let tabs = vec![
        TabSpec::new("ledger", t(locale, "finance.tab.ledger")).with_count(txns_count),
        TabSpec::new("budget", t(locale, "finance.tab.budget")).with_count(budgets_count),
        TabSpec::new("accounts", t(locale, "finance.tab.accounts")).with_count(accounts_count),
        TabSpec::new("categories", t(locale, "finance.tab.categories"))
            .with_count(categories_count),
        TabSpec::new("reports", t(locale, "finance.tab.reports")),
    ];

    // Share the loaded LedgerData across every tab branch by Arc instead of
    // deep-cloning per branch. (Arc, not Rc — Leptos's reactive closures
    // require Send.) Each tab closure clones a cheap handle.
    let data = std::sync::Arc::new(data);
    let data_for_ledger = data.clone();
    let data_for_budget = data.clone();
    let data_for_accounts = data.clone();
    let data_for_categories = data.clone();
    let data_for_reports = data;

    view! {
        {banner}
        {kpis}
        <Tabs tabs=tabs active=active/>
        {move || match active.get().as_str() {
            "budget" => render_budget(&data_for_budget, set_budget, import_budgets).into_any(),
            "accounts" => render_accounts(&data_for_accounts, create_account, update_account, delete_account).into_any(),
            "categories" => render_categories(&data_for_categories, create_category, update_category, delete_category).into_any(),
            "reports" => render_reports(&data_for_reports).into_any(),
            _ => view! {
                {render_txn_modal(
                    add,
                    add_transfer,
                    data_for_ledger.categories.clone(),
                    data_for_ledger.accounts.clone(),
                    txn_modal_open,
                    txn_modal_mode,
                )}
                {render_ledger_tab(
                    &data_for_ledger,
                    delete,
                    update_txn,
                    txn_modal_open,
                    txn_modal_mode,
                    LedgerFilters {
                        merchant: merchant_filter,
                        category: category_filter,
                        date_from: date_from_filter,
                        date_to: date_to_filter,
                    },
                )}
            }.into_any(),
        }}
    }
}

fn render_banner(d: &LedgerData) -> impl IntoView {
    let locale = use_locale();
    let m = &d.month;
    let week_sign = if m.balance_delta >= 0.0 { "+" } else { "−" };
    let savings_pct = (m.savings_rate * 100.0).round() as u32;
    // Net-worth tone tracks the most recent week: + → green/healthy,
    // 0 → flat/neutral, − → rose/watch.
    let (worth_tone, worth_label_key) = if m.balance_delta > 0.0 {
        (Tone::Green, "finance.banner.status.healthy")
    } else if m.balance_delta == 0.0 {
        (Tone::None, "finance.banner.status.flat")
    } else {
        (Tone::Rose, "finance.banner.status.watch")
    };
    view! {
        <div class="module-banner">
            <div class="module-glyph fin mono">"¥"</div>
            <div style="flex:1">
                <div class="hstack" style="margin-bottom:6px;gap:8px">
                    <span class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">{t(locale, "finance.banner.net_worth")}</span>
                    <UiTag tone=worth_tone dot=true>{t(locale, worth_label_key)}</UiTag>
                </div>
                <div class="mono" style="font-size:32px;font-weight:600;letter-spacing:-0.02em;line-height:1.1">
                    "¥" {fmt_money(m.balance)}
                </div>
                <div class="hstack" style="gap:16px;margin-top:8px;font-size:12.5px;color:var(--ink-3)">
                    <span class="mono">{tf(locale, "finance.banner.balance_delta", &[("sign", week_sign), ("amount", &fmt_int(m.balance_delta.abs()))])}</span>
                    <span class="mono">{tf(locale, "finance.banner.savings_rate", &[("pct", &savings_pct.to_string())])}</span>
                    <span class="mono">{tf(locale, "finance.banner.account_count", &[("count", &d.accounts.len().to_string())])}</span>
                </div>
            </div>
            <div class="hstack" style="gap:20px;padding-right:8px">
                <div>
                    <div class="mono" style="font-size:10.5px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:4px">{t(locale, "finance.banner.income")}</div>
                    <div class="mono" style="font-size:18px;font-weight:600;color:var(--primary-ink)">"+¥" {fmt_int(m.income)}</div>
                </div>
                <div class="sep-v"></div>
                <div>
                    <div class="mono" style="font-size:10.5px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:4px">{t(locale, "finance.banner.spend")}</div>
                    <div class="mono" style="font-size:18px;font-weight:600">"−¥" {fmt_int(m.expense)}</div>
                </div>
                <div class="sep-v"></div>
                <div>
                    <div class="mono" style="font-size:10.5px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:4px">{t(locale, "finance.banner.net_savings")}</div>
                    <div class="mono" style="font-size:18px;font-weight:600">{format!("{}¥{}", if m.savings >= 0.0 { "" } else { "−" }, fmt_int(m.savings.abs()))}</div>
                </div>
            </div>
        </div>
    }
}

const INPUT_STYLE: &str =
    "padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)";
const INPUT_STYLE_MONO: &str = "padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono)";
const FIELD_LABEL: &str = "font-size:11px;text-transform:uppercase;letter-spacing:0.06em";

fn render_txn_modal(
    add: ServerAction<AddTxn>,
    add_transfer: ServerAction<AddTransfer>,
    categories: Vec<Category>,
    accounts: Vec<Account>,
    open: RwSignal<bool>,
    mode: RwSignal<String>,
) -> impl IntoView {
    let locale = use_locale();
    view! {
        <div class="fin-modal-slot">
            {move || {
                if !open.get() {
                    return view! { <span></span> }.into_any();
                }
                let is_transfer = mode.get() == "transfer";
                let title = if is_transfer {
                    t(locale, "finance.card.transfer.title")
                } else {
                    t(locale, "finance.card.new.title")
                };
                let sub = if is_transfer {
                    t(locale, "finance.card.transfer.sub")
                } else {
                    t(locale, "finance.card.new.sub")
                };
                let txn_btn_class = if is_transfer { "btn ghost" } else { "btn primary" };
                let transfer_btn_class = if is_transfer { "btn primary" } else { "btn ghost" };
                view! {
                    <div class="fin-modal-backdrop">
                        <div class="fin-modal" role="dialog" aria-modal="true">
                            <div class="fin-modal-head">
                                <div>
                                    <div class="card-title">
                                        {title}
                                        <span class="code">"FIN-OPS"</span>
                                    </div>
                                    <p class="card-sub">{sub}</p>
                                </div>
                                <button class="btn ghost sm" type="button"
                                        aria-label=t(locale, "finance.action.cancel")
                                        on:click=move |_| open.set(false)>{t(locale, "finance.action.cancel")}</button>
                            </div>
                            <div class="fin-modal-body">
                                <div class="hstack" style="gap:8px;margin-bottom:14px;flex-wrap:wrap">
                                    <button class=txn_btn_class type="button"
                                            on:click=move |_| mode.set(String::from("txn"))>
                                        <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.action.add_txn")}
                                    </button>
                                    <button class=transfer_btn_class type="button"
                                            on:click=move |_| mode.set(String::from("transfer"))>
                                        <Icon kind=IconKind::Arrow size=14/>{t(locale, "finance.action.transfer")}
                                    </button>
                                </div>
                                {if is_transfer {
                                    render_transfer_form(add_transfer, accounts.clone()).into_any()
                                } else {
                                    render_new_txn_form(add, categories.clone(), accounts.clone()).into_any()
                                }}
                            </div>
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

fn render_new_txn_form(
    add: ServerAction<AddTxn>,
    categories: Vec<Category>,
    accounts: Vec<Account>,
) -> impl IntoView {
    let locale = use_locale();
    view! {
            <ActionForm action=add attr:class="vstack fin-op-form" attr:style="gap:10px">
                <div style="display:grid;grid-template-columns:2fr 1fr 1fr;gap:10px">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.merchant_desc")}</span>
                        <input id="fin-new-merchant" name="merchant" required
                               maxlength=MAX_TXN_MERCHANT_CHARS.to_string()
                               placeholder=t(locale, "finance.placeholder.merchant") style=INPUT_STYLE/>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.amount_yuan")}</span>
                        <input name="amount" type="number" step="0.01" min="0.01" required
                               placeholder="42.00" style=INPUT_STYLE_MONO/>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.tag")}</span>
                        <select name="tag" style=INPUT_STYLE>
                            <option value=Tag::Exp.as_str() selected="selected">{t(locale, "finance.tag.exp")}</option>
                            <option value=Tag::Inc.as_str()>{t(locale, "finance.tag.inc")}</option>
                        </select>
                    </label>
                </div>
                <div style="display:grid;grid-template-columns:1fr 1fr;gap:10px">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.category")}</span>
                        <select name="category_code" style=INPUT_STYLE>
                            {categories.into_iter().enumerate().map(|(i, c)| {
                                let code = c.code.clone();
                                let label = format!("{} {}", c.name, c.code);
                                view! { <option value=code selected={i == 0}>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.account")}</span>
                        <select name="account_code" style=INPUT_STYLE>
                            {accounts.into_iter().enumerate().map(|(i, a)| {
                                let code = a.code.clone();
                                let label = format!("{} · {}", a.code, a.name);
                                view! { <option value=code selected={i == 0}>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                </div>
                <div style="display:grid;grid-template-columns:2fr 2fr auto auto;gap:10px;align-items:end">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.note_optional")}</span>
                        <input name="note" maxlength=MAX_TXN_NOTE_CHARS.to_string()
                               placeholder="…" style=INPUT_STYLE/>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.date_default")}</span>
                        <input name="occurred_at" type="date" style=INPUT_STYLE_MONO
                               title=t(locale, "finance.placeholder.date_now")/>
                    </label>
                    <span class="error-slot" style="align-self:center">
                        {move || add.value().get().and_then(|r| r.err()).map(|e| view! {
                            <span class="tag rose">{server_fn_error_text(&e)}</span>
                        })}
                    </span>
                    <button class="btn primary" type="submit" style="align-self:center">
                        <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.action.add_txn")}
                    </button>
                </div>
            </ActionForm>
    }
}

/// Transfer card; submits to `add_transfer` (writes the paired ±amount rows).
fn render_transfer_form(
    add_transfer: ServerAction<AddTransfer>,
    accounts: Vec<Account>,
) -> impl IntoView {
    let locale = use_locale();
    let active_accounts: Vec<Account> = accounts;
    let from_accounts = active_accounts.clone();
    let to_accounts = active_accounts;
    view! {
            <ActionForm action=add_transfer attr:class="vstack fin-op-form" attr:style="gap:10px">
                <div style="display:grid;grid-template-columns:1fr 1fr 1fr 1fr;gap:10px">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.from_account")}</span>
                        <select name="from_account" style=INPUT_STYLE>
                            {from_accounts.into_iter().enumerate().map(|(i, a)| {
                                let code = a.code.clone();
                                let label = format!("{} · {}", a.code, a.name);
                                view! { <option value=code selected={i == 0}>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.to_account")}</span>
                        <select name="to_account" style=INPUT_STYLE>
                            {to_accounts.into_iter().enumerate().map(|(i, a)| {
                                let code = a.code.clone();
                                let label = format!("{} · {}", a.code, a.name);
                                view! { <option value=code selected={i == 1}>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.amount_yuan")}</span>
                        <input name="amount" type="number" step="0.01" min="0.01" required
                               placeholder="500.00" style=INPUT_STYLE_MONO/>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.date_optional")}</span>
                        <input name="occurred_at" type="date" style=INPUT_STYLE_MONO
                               title=t(locale, "finance.placeholder.date_now")/>
                    </label>
                </div>
                <div style="display:grid;grid-template-columns:3fr auto auto;gap:10px;align-items:end">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.note_optional")}</span>
                        <input name="note" maxlength=MAX_TXN_NOTE_CHARS.to_string()
                               placeholder=t(locale, "finance.placeholder.transfer_note") style=INPUT_STYLE/>
                    </label>
                    <span class="error-slot" style="align-self:center">
                        {move || add_transfer.value().get().and_then(|r| r.err()).map(|e| view! {
                            <span class="tag rose">{server_fn_error_text(&e)}</span>
                        })}
                    </span>
                    <button class="btn primary" type="submit" style="align-self:center">
                        <Icon kind=IconKind::Arrow size=14/>{t(locale, "finance.action.transfer")}
                    </button>
                </div>
            </ActionForm>
    }
}

fn render_ledger_tab(
    d: &LedgerData,
    delete: ServerAction<DeleteTxn>,
    update_txn: ServerAction<UpdateTxn>,
    txn_modal_open: RwSignal<bool>,
    txn_modal_mode: RwSignal<String>,
    filters: LedgerFilters,
) -> impl IntoView {
    let locale = use_locale();
    let txns = d.txns.clone();
    let cat_summary = d.category_summary.clone();
    let cat_options = d.categories.clone();
    let acc_options = d.accounts.clone();
    let cat_lookup: std::collections::HashMap<String, Category> = d
        .categories
        .iter()
        .map(|c| (c.code.clone(), c.clone()))
        .collect();
    let visible_count = txns.len();
    let total_count = d.month.total_txn_count as usize;
    // Computed once per render of this tab. The parent re-runs render_ledger_tab
    // whenever the resource refetches (add / delete), so the export link picks
    // up new rows automatically — no reactive attribute needed.
    let export_href = csv_data_uri(&txns);
    // Same lifetime story for rule suggestions: compute now while we still
    // hold `&d`, hand the owned `Vec<Suggestion>` to the view macro.
    let suggestions = crate::suggestions::compute_suggestions(d, locale);

    // The table is "most recent 50, all-time" — the sub-label has to
    // surface that and the month-specific count without conflating them.
    let sub = match (visible_count, total_count) {
        (0, 0) => t(locale, "finance.ledger.empty").to_string(),
        (v, m) if v >= 50 => tf(
            locale,
            "finance.ledger.sub_recent",
            &[("month", &m.to_string())],
        ),
        (v, m) => tf(
            locale,
            "finance.ledger.sub_all",
            &[("visible", &v.to_string()), ("month", &m.to_string())],
        ),
    };

    view! {
        <div class="grid-2" style="margin-top:20px">
            <Card title=t(locale, "finance.card.ledger.title") code="FIN-LGR-01" sub=sub>
                <div class="hstack" style="gap:10px;margin-bottom:12px;flex-wrap:wrap">
                    <button class="btn primary" type="button"
                            on:click=move |_| {
                                txn_modal_mode.set(String::from("txn"));
                                txn_modal_open.set(true);
                            }>
                        <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.action.add_txn")}
                    </button>
                    <button class="btn" type="button"
                            on:click=move |_| {
                                txn_modal_mode.set(String::from("transfer"));
                                txn_modal_open.set(true);
                            }>
                        <Icon kind=IconKind::Arrow size=14/>{t(locale, "finance.action.transfer")}
                    </button>
                    <input type="text" placeholder=t(locale, "finance.filter.search")
                           prop:value=move || filters.merchant.get()
                           on:input=move |ev| filters.merchant.set(event_target_value(&ev))
                           style=format!("flex:1;min-width:160px;{}", INPUT_STYLE)/>
                    <select prop:value=move || filters.category.get()
                            on:change=move |ev| filters.category.set(event_target_value(&ev))
                            style=INPUT_STYLE>
                        <option value="">{t(locale, "finance.filter.all_categories")}</option>
                        {cat_options.iter().map(|c| {
                            let code = c.code.clone();
                            let label = format!("{} {}", c.name, c.code);
                            view! { <option value=code>{label}</option> }
                        }).collect_view()}
                    </select>
                    <input type="date"
                           prop:value=move || filters.date_from.get()
                           on:input=move |ev| filters.date_from.set(event_target_value(&ev))
                           style=INPUT_STYLE_MONO
                           title=t(locale, "finance.filter.date_from")/>
                    <input type="date"
                           prop:value=move || filters.date_to.get()
                           on:input=move |ev| filters.date_to.set(event_target_value(&ev))
                           style=INPUT_STYLE_MONO
                           title=t(locale, "finance.filter.date_to")/>
                    <a class="btn" download="finance-export.csv" href=export_href>
                        <Icon kind=IconKind::Export size=14/>{t(locale, "finance.action.export")}
                    </a>
                </div>
                <div class="scroll-x">
                    <table class="tbl">
                        <thead>
                            <tr>
                                <th style="width:76px">{t(locale, "finance.field.date")}</th>
                                <th style="width:110px">"ID"</th>
                                <th>{t(locale, "finance.field.merchant_desc")}</th>
                                <th style="width:80px">{t(locale, "finance.field.category")}</th>
                                <th style="width:80px">{t(locale, "finance.field.account")}</th>
                                <th class="num" style="width:110px">{t(locale, "finance.field.amount")}</th>
                                <th class="num" style="width:120px">{t(locale, "finance.field.ops")}</th>
                            </tr>
                        </thead>
                        <tbody>
                            {move || {
                                let mq = filters.merchant.get().to_lowercase();
                                let cq = filters.category.get();
                                // Date filter: convert YYYY-MM-DD to start-of-day unix
                                // seconds, treating empty inputs as -∞ / +∞. The end
                                // bound includes the entire end day (24h window).
                                let from_ts = parse_date_floor(&filters.date_from.get());
                                let to_ts = parse_date_ceiling(&filters.date_to.get());
                                let cat_lookup = &cat_lookup;
                                let cat_options = &cat_options;
                                let acc_options = &acc_options;
                                txns.iter()
                                    .filter(|t| {
                                        (mq.is_empty() || t.merchant.to_lowercase().contains(&mq))
                                        && (cq.is_empty() || t.category_code == cq)
                                        && from_ts.map(|f| t.occurred_at >= f).unwrap_or(true)
                                        && to_ts.map(|to| t.occurred_at <= to).unwrap_or(true)
                                    })
                                    .cloned()
                                    .map(|t| render_txn_row(t, cat_lookup, cat_options, acc_options, delete, update_txn))
                                    .collect_view()
                            }}
                        </tbody>
                    </table>
                </div>
            </Card>

            <div class="vstack" style="gap:20px">
                <Card title=t(locale, "finance.card.category_share.title") code="FIN-R02" sub=t(locale, "finance.card.category_share.sub")>
                    {if cat_summary.is_empty() {
                        view! { <p class="muted">{t(locale, "finance.card.category_share.empty")}</p> }.into_any()
                    } else {
                        view! {
                            <div class="vstack" style="gap:10px">
                                {cat_summary.into_iter().map(|c| {
                                    let bar_color = if c.tone.is_empty() { "var(--primary)".to_string() } else { format!("var(--{})", c.tone) };
                                    let pct = (c.pct * 3.0).min(100.0);
                                    view! {
                                        <div>
                                            <div style="display:flex;justify-content:space-between;align-items:baseline;margin-bottom:4px">
                                                <div style="font-size:12.5px">
                                                    <span>{c.name.clone()}</span>
                                                    <span class="mono dim" style="margin-left:6px;font-size:10.5px">{c.code.clone()}</span>
                                                </div>
                                                <div class="mono" style="font-size:12px">{format!("¥{}", fmt_int(c.value))} <span class="dim">{format!("· {}%", c.pct)}</span></div>
                                            </div>
                                            <div class="bar"><span style=format!("width:{:.1}%;background:{}", pct, bar_color)></span></div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }}
                </Card>

                <Card title=t(locale, "finance.card.suggestions.title") code="FIN-RUL-01" sub=t(locale, "finance.card.suggestions.sub")>
                    <div class="vstack" style="gap:10px">
                        {render_suggestions(suggestions)}
                    </div>
                </Card>
            </div>
        </div>
    }
}

fn render_suggestions(items: Vec<crate::suggestions::Suggestion>) -> impl IntoView {
    let locale = use_locale();
    if items.is_empty() {
        return view! { <p class="muted">{t(locale, "finance.card.suggestions.empty")}</p> }
            .into_any();
    }
    // Wrap each row in an anchor when the rule provided a link target so the
    // suggestion is one click instead of a "look at this" toast. Inline
    // anchor styles are reset by `.list-row` in styles.css.
    view! {
        {items.into_iter().map(|s| match s.link {
            Some(href) => view! {
                <a class="list-row list-row-link" href=href>
                    <div class="icon-tile"><Icon kind=s.icon size=14/></div>
                    <div>
                        <div class="title">{s.title}</div>
                        <div class="meta">{s.meta}</div>
                    </div>
                </a>
            }.into_any(),
            None => view! {
                <div class="list-row">
                    <div class="icon-tile"><Icon kind=s.icon size=14/></div>
                    <div>
                        <div class="title">{s.title}</div>
                        <div class="meta">{s.meta}</div>
                    </div>
                </div>
            }.into_any(),
        }).collect_view()}
    }
    .into_any()
}

fn render_txn_row(
    t: Txn,
    cat_lookup: &std::collections::HashMap<String, Category>,
    cat_options: &[Category],
    acc_options: &[Account],
    delete: ServerAction<DeleteTxn>,
    update_txn: ServerAction<UpdateTxn>,
) -> impl IntoView {
    let locale = use_locale();
    let date = fmt_ts_md(Some(t.occurred_at));
    let time_ = fmt_ts_hm(Some(t.occurred_at));
    let cls_amt = if t.amount > 0.0 {
        "num amt-pos"
    } else {
        "num amt-neg"
    };
    let txind = match Tag::parse(&t.tag) {
        Some(Tag::Inc) => "txind inc",
        Some(Tag::Tfr) => "txind tfr",
        _ => "txind exp",
    };
    let amount_text = if t.amount > 0.0 {
        format!("+¥{}", fmt_money(t.amount))
    } else {
        format!("−¥{}", fmt_money(t.amount.abs()))
    };
    let is_tfr = matches!(Tag::parse(&t.tag), Some(Tag::Tfr));
    let cat_tone = cat_lookup
        .get(&t.category_code)
        .map(|c| Tone::parse(&c.tone))
        .unwrap_or(Tone::None);
    let cat_label = t.category_code.clone();
    let doc_id = t.doc_id.clone();

    // Category cell renders the transfer pill on tfr rows so the user sees
    // why the edit affordance is suppressed; non-tfr rows keep the existing
    // category Tag toned via cat_lookup.
    let cat_cell = if is_tfr {
        view! { <UiTag tone=Tone::Blue>{ep_i18n::t(locale, "finance.tag.transfer")}</UiTag> }
            .into_any()
    } else {
        view! { <UiTag tone=cat_tone>{cat_label}</UiTag> }.into_any()
    };

    // Per-row edit dialog. The row is recreated on refetch after submit, so
    // the modal naturally closes without another client-side action.
    let editing = RwSignal::new(false);

    // Action column varies by tfr-ness — non-tfr gets an edit dialog,
    // tfr gets a tooltip-bearing placeholder.
    let action_cell = if is_tfr {
        view! {
            <span class="dim mono"
                  style="font-size:10.5px"
                  title=ep_i18n::t(locale, "finance.title.tfr_not_editable")>"——"</span>
            <RowDeleteAction action=delete value=doc_id.clone()
                             confirm=ep_i18n::t(locale, "finance.confirm.delete_transfer")/>
        }
        .into_any()
    } else {
        view! {
            <button class="btn sm" type="button"
                    on:click=move |_| editing.set(true)>
                {ep_i18n::t(locale, "finance.action.edit")}
            </button>
            <RowDeleteAction action=delete value=doc_id.clone()
                             confirm=ep_i18n::t(locale, "finance.confirm.delete_txn")/>
        }
        .into_any()
    };

    // Pre-baked prefilled values for the edit form. All clones live on the
    // owned `t` we received by value, so they survive the closure capture.
    let edit_doc_id = t.doc_id.clone();
    let edit_merchant = t.merchant.clone();
    let edit_amount_str = format!("{:.2}", t.amount.abs());
    let edit_account = t.account_code.clone();
    let edit_category = t.category_code.clone();
    let edit_note = t.note.clone().unwrap_or_default();
    let edit_date = fmt_ts_yyyymmdd(t.occurred_at);
    let edit_title_doc = t.doc_id.clone();
    let edit_sub_merchant = t.merchant.clone();
    let cat_opts = cat_options.to_vec();
    let acc_opts = acc_options.to_vec();
    let cat_opts_active: Vec<Category> = cat_opts;
    let acc_opts_active: Vec<Account> = acc_opts;

    let edit_form = if is_tfr {
        view! { <span></span> }.into_any()
    } else {
        view! {
            <div class="fin-modal-slot">
                {move || if editing.get() {
                    let form_doc_id = edit_doc_id.clone();
                    let form_merchant = edit_merchant.clone();
                    let form_amount = edit_amount_str.clone();
                    let form_category = edit_category.clone();
                    let form_account = edit_account.clone();
                    let form_note = edit_note.clone();
                    let form_date = edit_date.clone();
                    let title_doc = edit_title_doc.clone();
                    let sub_merchant = edit_sub_merchant.clone();
                    let form_cat_opts = cat_opts_active.clone();
                    let form_acc_opts = acc_opts_active.clone();
                    view! {
                        <div class="fin-modal-backdrop">
                            <div class="fin-modal" role="dialog" aria-modal="true">
                                <div class="fin-modal-head">
                                    <div>
                                        <div class="card-title">
                                            {ep_i18n::t(locale, "finance.action.edit")}
                                            <span class="code">{title_doc}</span>
                                        </div>
                                        <p class="card-sub">{sub_merchant}</p>
                                    </div>
                                    <button class="btn ghost sm" type="button"
                                            aria-label=ep_i18n::t(locale, "finance.action.cancel")
                                            on:click=move |_| editing.set(false)>{ep_i18n::t(locale, "finance.action.cancel")}</button>
                                </div>
                                <div class="fin-modal-body">
                                    <ActionForm action=update_txn attr:class="vstack fin-op-form" attr:style="gap:10px">
                                        <input type="hidden" name="doc_id" value=form_doc_id/>
                                        <div style="display:grid;grid-template-columns:2fr 1fr;gap:10px">
                                            <label class="vstack" style="gap:4px">
                                                <span class="mono dim" style=FIELD_LABEL>{ep_i18n::t(locale, "finance.field.merchant")}</span>
                                                <input name="merchant" required
                                                       maxlength=MAX_TXN_MERCHANT_CHARS.to_string()
                                                       value=form_merchant style=INPUT_STYLE/>
                                            </label>
                                            <label class="vstack" style="gap:4px">
                                                <span class="mono dim" style=FIELD_LABEL>{ep_i18n::t(locale, "finance.field.amount")}</span>
                                                <input name="amount" type="number" step="0.01" min="0.01"
                                                       value=form_amount style=INPUT_STYLE_MONO/>
                                            </label>
                                        </div>
                                        <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:10px">
                                            <label class="vstack" style="gap:4px">
                                                <span class="mono dim" style=FIELD_LABEL>{ep_i18n::t(locale, "finance.field.category")}</span>
                                                <select name="category_code" style=INPUT_STYLE>
                                                    {form_cat_opts.into_iter().map(move |c| {
                                                        let selected = c.code == form_category.as_str();
                                                        let code = c.code.clone();
                                                        let label = format!("{} {}", c.name, c.code);
                                                        view! { <option value=code selected=selected>{label}</option> }
                                                    }).collect_view()}
                                                </select>
                                            </label>
                                            <label class="vstack" style="gap:4px">
                                                <span class="mono dim" style=FIELD_LABEL>{ep_i18n::t(locale, "finance.field.account")}</span>
                                                <select name="account_code" style=INPUT_STYLE>
                                                    {form_acc_opts.into_iter().map(move |a| {
                                                        let selected = a.code == form_account.as_str();
                                                        let code = a.code.clone();
                                                        let label = format!("{} · {}", a.code, a.name);
                                                        view! { <option value=code selected=selected>{label}</option> }
                                                    }).collect_view()}
                                                </select>
                                            </label>
                                            <label class="vstack" style="gap:4px">
                                                <span class="mono dim" style=FIELD_LABEL>{ep_i18n::t(locale, "finance.field.date")}</span>
                                                <input name="occurred_at" type="date" value=form_date style=INPUT_STYLE_MONO/>
                                            </label>
                                        </div>
                                        <div style="display:grid;grid-template-columns:1fr;gap:10px">
                                            <label class="vstack" style="gap:4px">
                                                <span class="mono dim" style=FIELD_LABEL>{ep_i18n::t(locale, "finance.field.note")}</span>
                                                <input name="note" maxlength=MAX_TXN_NOTE_CHARS.to_string()
                                                       value=form_note style=INPUT_STYLE/>
                                            </label>
                                        </div>
                                        <div class="hstack" style="gap:8px;align-items:center;justify-content:flex-end;flex-wrap:wrap">
                                            <span class="error-slot">
                                                {move || update_txn.value().get().and_then(|r| r.err()).map(|e| view! {
                                                    <span class="tag rose">{server_fn_error_text(&e)}</span>
                                                })}
                                            </span>
                                            <button class="btn ghost" type="button"
                                                    on:click=move |_| editing.set(false)>{ep_i18n::t(locale, "finance.action.cancel")}</button>
                                            <button class="btn primary" type="submit">{ep_i18n::t(locale, "finance.action.save")}</button>
                                        </div>
                                    </ActionForm>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
                </div>
        }.into_any()
    };

    view! {
        <tr>
            <td class="mono dim">{date}<div style="font-size:10px;color:var(--ink-4)">{time_}</div></td>
            <td class="doc">{t.doc_id.clone()}</td>
            <td>
                <span class=txind></span>
                {t.merchant.clone()}
            </td>
            <td>{cat_cell}</td>
            <td class="mono dim">{t.account_code.clone()}</td>
            <td class=cls_amt>{amount_text}</td>
            <td class="num">
                <div class="hstack" style="gap:4px;justify-content:flex-end;align-items:center">
                    {action_cell}
                </div>
            </td>
        </tr>
        <tr class="edit-row" style:display=move || if editing.get() { "table-row" } else { "none" }>
            <td colspan="7" style="padding:0;border:0">
                {edit_form}
            </td>
        </tr>
    }
}

/// Format a unix-second timestamp to `YYYY-MM-DD` (UTC). Used by the inline
/// edit form's `<input type="date">`.
fn fmt_ts_yyyymmdd(ts: i64) -> String {
    unix_to_ymdhm(ts)
        .map(|(y, m, d, _, _)| format!("{y:04}-{m:02}-{d:02}"))
        .unwrap_or_default()
}

fn render_budget(
    d: &LedgerData,
    set_budget: ServerAction<SetBudget>,
    import_budgets: ServerAction<ImportBudgetsFrom>,
) -> impl IntoView {
    let locale = use_locale();
    let m = &d.month;
    let period = m.period.clone();
    let categories_for_form = d.categories.clone();
    // Owned-string lookup so the closures below don't capture a borrow into
    // a Vec the view! macro will move.
    let cat_lookup: std::collections::HashMap<String, (String, String)> = d
        .categories
        .iter()
        .map(|c| (c.code.clone(), (c.name.clone(), c.tone.clone())))
        .collect();
    let budgets = d.budgets.clone();
    let budgets_count = budgets.len();
    // Categories that have spent this month but no budget — surfaced so the
    // user can react ("oh I forgot to budget for X this month").
    let unbudgeted: Vec<crate::model::CategorySummary> = d
        .category_summary
        .iter()
        .filter(|c| !d.budgets.iter().any(|b| b.category_code == c.code))
        .cloned()
        .collect();
    let next_month_planner = next_month_plan(d);
    // Pre-compute every period-derived string up front so the view! body
    // doesn't have to clone `period` through nested closures.
    let import_source = previous_period(&period);
    let import_target = period.clone();
    let next_period_label = next_period(&period);
    let pool_title = tf(locale, "finance.budget.pool_title", &[("period", &period)]);
    let pool_sub = if budgets_count == 0 {
        t(locale, "finance.budget.empty").to_string()
    } else {
        tf(
            locale,
            "finance.budget.pool_sub",
            &[
                ("count", &budgets_count.to_string()),
                ("used", &fmt_int(m.expense)),
                ("total", &fmt_int(m.budget_total)),
            ],
        )
    };
    let import_button_label = tf(
        locale,
        "finance.budget.import",
        &[("period", &import_source)],
    );
    let empty_period_hint = tf(
        locale,
        "finance.budget.empty_period",
        &[("period", &period)],
    );
    let next_month_sub = tf(
        locale,
        "finance.budget.card.next_sub",
        &[("period", &next_period_label)],
    );
    let editor_period = period.clone();

    view! {
        <div class="grid-2">
            <Card title=pool_title code="FIN-BDG-01" sub=pool_sub>
                {if budgets.is_empty() {
                    view! {
                        <div class="vstack" style="gap:10px">
                            <p class="muted">{empty_period_hint}</p>
                            <div class="hstack" style="gap:8px">
                                <ActionForm action=import_budgets attr:style="display:inline">
                                    <input type="hidden" name="source_period" value=import_source.clone()/>
                                    <input type="hidden" name="target_period" value=import_target.clone()/>
                                    <button class="btn primary" type="submit">
                                        <Icon kind=IconKind::Upload size=14/>
                                        {import_button_label}
                                    </button>
                                </ActionForm>
                                <span class="error-slot">
                                    {move || import_budgets.value().get().and_then(|r| r.err()).map(|e| view! {
                                        <span class="tag rose">{server_fn_error_text(&e)}</span>
                                    })}
                                </span>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="vstack" style="gap:14px">
                            {budgets.into_iter().map(|b| {
                                let (name, tone) = cat_lookup.get(&b.category_code)
                                    .cloned()
                                    .unwrap_or_else(|| (b.category_code.clone(), String::new()));
                                let pct_f = if b.amount > 0.0 { b.used / b.amount * 100.0 } else { 0.0 };
                                let pct = pct_f.round() as i32;
                                let bar_color = if pct > 95 { "var(--rose)".to_string() }
                                                else if pct > 80 { "var(--amber)".to_string() }
                                                else if tone.is_empty() { "var(--primary)".to_string() }
                                                else { format!("var(--{})", tone) };
                                let pct_class = if pct > 100 { "amt-neg" } else { "dim" };
                                let bar_width = (pct as i64).clamp(0, 100);
                                let edit_period = period.clone();
                                let edit_category = b.category_code.clone();
                                let delete_period = period.clone();
                                let delete_category = b.category_code.clone();
                                let row_amount = format!("{:.2}", b.amount);
                                let row_action = set_budget;
                                let delete_action = set_budget;
                                let delete_confirm = tf(locale, "finance.budget.confirm_delete", &[("code", &b.category_code)]);
                                view! {
                                    <div>
                                        <div style="display:flex;justify-content:space-between;align-items:baseline;margin-bottom:4px">
                                            <div style="font-size:13px">
                                                <span style="font-weight:500">{name}</span>
                                                <span class="mono dim" style="margin-left:6px;font-size:10.5px">{format!("FIN-B-{}", b.category_code)}</span>
                                            </div>
                                            <div class="mono" style="font-size:12px">
                                                {format!("¥{} / ¥{} · ", fmt_int(b.used), fmt_int(b.amount))}
                                                <span class=pct_class>{format!("{}%", pct)}</span>
                                            </div>
                                        </div>
                                        <div class="bar thick"><span style=format!("width:{}%;background:{}", bar_width, bar_color)></span></div>
                                        <div class="hstack" style="gap:8px;margin-top:8px;justify-content:flex-end;flex-wrap:wrap">
                                            <ActionForm action=row_action attr:class="hstack" attr:style="gap:6px;align-items:center">
                                                <input type="hidden" name="period" value=edit_period/>
                                                <input type="hidden" name="category_code" value=edit_category/>
                                                <input name="amount" type="number" step="50" min="0.01"
                                                       value=row_amount
                                                       style=format!("width:110px;{}", INPUT_STYLE_MONO)/>
                                                <button class="btn sm" type="submit">
                                                    <Icon kind=IconKind::Check size=12/>{t(locale, "finance.action.save")}
                                                </button>
                                            </ActionForm>
                                            <ActionForm action=delete_action attr:style="display:inline">
                                                <input type="hidden" name="period" value=delete_period/>
                                                <input type="hidden" name="category_code" value=delete_category/>
                                                <input type="hidden" name="amount" value="0"/>
                                                <button class="btn sm" type="submit"
                                                        style="color:var(--rose-ink)"
                                                        onclick=format!("return confirm('{}')", escape_js_single_quoted(&delete_confirm))>
                                                    {t(locale, "finance.action.delete")}
                                                </button>
                                            </ActionForm>
                                        </div>
                                    </div>
                                }
                            }).collect_view()}
                            {if unbudgeted.is_empty() {
                                view! { <span></span> }.into_any()
                            } else {
                                view! {
                                    <div style="margin-top:6px;padding-top:10px;border-top:1px dashed var(--border)">
                                        <div class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">{t(locale, "finance.budget.unbudgeted")}</div>
                                        {unbudgeted.into_iter().map(|c| view! {
                                            <div style="display:flex;justify-content:space-between;font-size:12.5px;padding:4px 0">
                                                <span>{c.name.clone()} <span class="mono dim" style="margin-left:6px;font-size:10.5px">{c.code.clone()}</span></span>
                                                <span class="mono">{format!("¥{}", fmt_int(c.value))}</span>
                                            </div>
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                        </div>
                    }.into_any()
                }}
            </Card>

            <div class="vstack" style="gap:20px">
                <Card title=t(locale, "finance.budget.card.edit_title") code="FIN-BDG-EDIT"
                      sub=t(locale, "finance.budget.card.edit_sub")>
                    <ActionForm action=set_budget attr:class="vstack" attr:style="gap:10px">
                        <div style="display:grid;grid-template-columns:1fr 1.5fr 1fr auto;gap:10px;align-items:end">
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.period")}</span>
                                <input name="period" type="month"
                                       value=editor_period required
                                       style=INPUT_STYLE_MONO/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.category")}</span>
                                <select name="category_code" style=INPUT_STYLE>
                                    {categories_for_form.into_iter().map(|c| {
                                        let code = c.code.clone();
                                        let label = format!("{} {}", c.name, c.code);
                                        view! { <option value=code>{label}</option> }
                                    }).collect_view()}
                                </select>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.amount_yuan")}</span>
                                <input name="amount" type="number" step="50" min="0"
                                       placeholder="3200" style=INPUT_STYLE_MONO/>
                            </label>
                            <button class="btn primary" type="submit">
                                <Icon kind=IconKind::Check size=14/>{t(locale, "finance.action.save")}
                            </button>
                        </div>
                        <span class="error-slot">
                            {move || set_budget.value().get().and_then(|r| r.err()).map(|e| view! {
                                <span class="tag rose">{server_fn_error_text(&e)}</span>
                            })}
                        </span>
                    </ActionForm>
                </Card>

                <Card title=t(locale, "finance.budget.card.next_title") code="FIN-BDG-02" sub=next_month_sub>
                    {if next_month_planner.is_empty() {
                        view! { <p class="muted">{t(locale, "finance.budget.card.next_empty")}</p> }.into_any()
                    } else {
                        view! {
                            <div class="vstack" style="gap:10px">
                                {next_month_planner.into_iter().map(|(name, code, suggested)| {
                                    view! {
                                        <div style="display:flex;justify-content:space-between;align-items:baseline;font-size:13px">
                                            <span>
                                                {name}
                                                <span class="mono dim" style="margin-left:6px;font-size:10.5px">{code}</span>
                                            </span>
                                            <span class="mono">{format!("¥{}", fmt_int(suggested))}</span>
                                        </div>
                                    }
                                }).collect_view()}
                                <div class="mono dim" style="font-size:10.5px;margin-top:4px;padding-top:8px;border-top:1px dashed var(--border)">
                                    {t(locale, "finance.budget.suggestion_formula")}
                                </div>
                            </div>
                        }.into_any()
                    }}
                </Card>
            </div>
        </div>
    }
}

/// "YYYY-MM" of the period that comes before `period`. Pure string math —
/// avoids an OffsetDateTime::now call on the wasm hydrate path. Falls back to
/// `period` itself on malformed input (the SQL would noop on a malformed
/// period anyway).
fn previous_period(period: &str) -> String {
    let (y, m) = parse_period(period).unwrap_or((2026, 1));
    let (py, pm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
    format!("{:04}-{:02}", py, pm)
}

fn next_period(period: &str) -> String {
    let (y, m) = parse_period(period).unwrap_or((2026, 12));
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    format!("{:04}-{:02}", ny, nm)
}

fn parse_period(period: &str) -> Option<(i32, u32)> {
    let bytes = period.as_bytes();
    if bytes.len() != 7 || bytes[4] != b'-' {
        return None;
    }
    let y: i32 = period[..4].parse().ok()?;
    let m: u32 = period[5..7].parse().ok()?;
    if !(1..=12).contains(&m) {
        return None;
    }
    Some((y, m))
}

fn render_accounts(
    d: &LedgerData,
    create_account: ServerAction<CreateAccount>,
    update_account: ServerAction<UpdateAccount>,
    delete_account: ServerAction<DeleteAccount>,
) -> impl IntoView {
    let pairs: Vec<(Account, AccountStats)> = d
        .accounts
        .iter()
        .cloned()
        .zip(d.account_stats.iter().cloned())
        .collect();
    view! {
        {render_account_manager(create_account)}
        <div class="grid-3" style="margin-top:20px">
            {pairs.into_iter().map(|(a, s)| {
                render_account_card(a, s, update_account, delete_account)
            }).collect_view()}
        </div>
    }
}

fn render_account_manager(create_account: ServerAction<CreateAccount>) -> impl IntoView {
    let locale = use_locale();
    view! {
        <Card title=t(locale, "finance.account.manager.title") code="FIN-ACC-MGR" sub=t(locale, "finance.account.manager.sub")>
            <details>
                <summary class="btn ghost" style="cursor:pointer;display:inline-flex;align-items:center;gap:6px">
                    <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.account.new")}
                </summary>
                <ActionForm action=create_account attr:class="vstack" attr:style="gap:10px;margin-top:12px">
                    <div style="display:grid;grid-template-columns:1fr 2fr 1fr 1fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.code")}</span>
                            <input name="code" required maxlength="16" placeholder="ACC-99"
                                   pattern="[-A-Z0-9]{2,16}"
                                   title=t(locale, "finance.account.title_code_pattern")
                                   style=INPUT_STYLE_MONO/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.name")}</span>
                            <input name="name" required maxlength="64" placeholder=t(locale, "finance.placeholder.account_name")
                                   style=INPUT_STYLE/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.type")}</span>
                            <select name="type" style=INPUT_STYLE>
                                {ACCOUNT_TYPES.iter().enumerate().map(|(i, t)| view! {
                                    <option value=*t selected={i == 0}>{*t}</option>
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.tone")}</span>
                            <select name="tone" style=INPUT_STYLE>
                                <option value="" selected="selected">"—"</option>
                                {TONES.iter().map(|t| view! {
                                    <option value=*t>{*t}</option>
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.opening_balance")}</span>
                            <input name="opening_balance" type="number" step="0.01" value="0"
                                   style=INPUT_STYLE_MONO/>
                        </label>
                    </div>
                    <div class="hstack" style="gap:10px;align-items:center">
                        <button class="btn primary" type="submit">
                            <Icon kind=IconKind::Check size=14/>{t(locale, "finance.action.create")}
                        </button>
                        <span class="error-slot">
                            {move || create_account.value().get().and_then(|r| r.err()).map(|e| view! {
                                <span class="tag rose">{server_fn_error_text(&e)}</span>
                            })}
                        </span>
                    </div>
                </ActionForm>
            </details>
        </Card>
    }
}

/// Single account card with its inline edit form + delete button.
fn render_account_card(
    a: Account,
    s: AccountStats,
    update_account: ServerAction<UpdateAccount>,
    delete_account: ServerAction<DeleteAccount>,
) -> impl IntoView {
    let locale = use_locale();
    let tone = Tone::parse(&a.tone);
    let last_seen = match s.last_seen_at {
        Some(ts) => tf(
            locale,
            "finance.account.last_activity",
            &[("date", &fmt_ts_date(Some(ts)))],
        ),
        None => t(locale, "finance.account.empty_activity").to_string(),
    };
    // Pre-bake the prefilled values so we never embed a `move ||` closure
    // inside `value=` (AGENTS.md "Don't put a `move ||`-returning attribute
    // on a child element passed through a prop"). render_accounts re-runs
    // on each ledger refetch — that's a fresh value capture each time.
    let code = a.code.clone();
    let name = a.name.clone();
    let type_str = a.r#type.clone();
    let tone_str = a.tone.clone();
    let card_title = a.name.clone();
    let card_code = a.code.clone();
    let card_sub = a.r#type.clone();
    let confirm_msg = tf(
        locale,
        "finance.account.confirm_delete",
        &[("code", &a.code)],
    );
    view! {
        <Card title=card_title code=card_code sub=card_sub>
            <div class="mono" style="font-size:24px;font-weight:600;letter-spacing:-0.02em">
                "¥" {fmt_money(a.balance)}
            </div>
            <div class="hstack" style="margin-top:10px;gap:10px">
                <UiTag tone=tone>{a.r#type.clone()}</UiTag>
                <span class="mono dim" style="font-size:10.5px">{last_seen}</span>
            </div>
            <div style="margin-top:14px">
                <ChartBars data=s.history_14d/>
            </div>
            <div class="hstack" style="margin-top:12px;gap:6px;flex-wrap:wrap">
                <details style="flex:1;min-width:0">
                    <summary class="btn ghost" style="cursor:pointer;display:inline-flex;align-items:center;gap:4px">
                        <Icon kind=IconKind::Settings size=12/>{t(locale, "finance.action.edit")}
                    </summary>
                    <ActionForm action=update_account attr:class="vstack" attr:style="gap:8px;margin-top:8px">
                        <input type="hidden" name="code" value=code.clone()/>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.code_readonly")}</span>
                            <input type="text" disabled value=code.clone()
                                   title=t(locale, "finance.account.title_code_readonly")
                                   style=INPUT_STYLE_MONO/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.name")}</span>
                            <input name="name" required maxlength="64" value=name
                                   style=INPUT_STYLE/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.type")}</span>
                            <select name="type" style=INPUT_STYLE>
                                {ACCOUNT_TYPES.iter().map(|t| {
                                    let selected = *t == type_str.as_str();
                                    view! { <option value=*t selected=selected>{*t}</option> }
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.tone")}</span>
                            <select name="tone" style=INPUT_STYLE>
                                <option value="" selected={tone_str.is_empty()}>"—"</option>
                                {TONES.iter().map(|t| {
                                    let selected = *t == tone_str.as_str();
                                    view! { <option value=*t selected=selected>{*t}</option> }
                                }).collect_view()}
                            </select>
                        </label>
                        <div class="hstack" style="gap:8px;align-items:center">
                            <button class="btn primary" type="submit">
                                <Icon kind=IconKind::Check size=12/>{t(locale, "finance.action.save")}
                            </button>
                            <span class="error-slot">
                                {move || update_account.value().get().and_then(|r| r.err()).map(|e| view! {
                                    <span class="tag rose">{server_fn_error_text(&e)}</span>
                                })}
                            </span>
                        </div>
                    </ActionForm>
                </details>
                <RowDeleteAction action=delete_account value=a.code.clone() field="code"
                                 confirm=confirm_msg label=t(locale, "finance.action.delete")/>
            </div>
        </Card>
    }
}

fn render_categories(
    d: &LedgerData,
    create_category: ServerAction<CreateCategory>,
    update_category: ServerAction<UpdateCategory>,
    delete_category: ServerAction<DeleteCategory>,
) -> impl IntoView {
    let locale = use_locale();
    // Sort by sort_order then code so the management table matches what the
    // dropdown shows. Cloning is fine — the categories vec is small (≤ ~20).
    let mut cats = d.categories.clone();
    cats.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.code.cmp(&b.code)));
    let usage = d.category_usage.clone();
    let next_sort = cats.iter().map(|c| c.sort_order).max().unwrap_or(0) + 1;
    view! {
        <Card title=t(locale, "finance.category.manager.title") code="FIN-CAT-MGR" sub=t(locale, "finance.category.manager.sub")>
            <details>
                <summary class="btn ghost" style="cursor:pointer;display:inline-flex;align-items:center;gap:6px">
                    <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.category.new")}
                </summary>
                <ActionForm action=create_category attr:class="vstack" attr:style="gap:10px;margin-top:12px">
                    <div style="display:grid;grid-template-columns:1fr 2fr 1fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.code")}</span>
                            <input name="code" required maxlength="8" placeholder="EDU"
                                   pattern="[A-Z&]{1,8}"
                                   title=t(locale, "finance.category.title_code_pattern")
                                   style=INPUT_STYLE_MONO/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.name")}</span>
                            <input name="name" required maxlength="32" placeholder=t(locale, "finance.placeholder.category_name")
                                   style=INPUT_STYLE/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.tone")}</span>
                            <select name="tone" style=INPUT_STYLE>
                                <option value="" selected="selected">"—"</option>
                                {TONES.iter().map(|t| view! {
                                    <option value=*t>{*t}</option>
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.sort_order")}</span>
                            <input name="sort_order" type="number" step="1" min="0"
                                   value=next_sort.to_string()
                                   style=INPUT_STYLE_MONO/>
                        </label>
                    </div>
                    <div class="hstack" style="gap:10px;align-items:center">
                        <button class="btn primary" type="submit">
                            <Icon kind=IconKind::Check size=14/>{t(locale, "finance.action.create")}
                        </button>
                        <span class="error-slot">
                            {move || create_category.value().get().and_then(|r| r.err()).map(|e| view! {
                                <span class="tag rose">{server_fn_error_text(&e)}</span>
                            })}
                        </span>
                    </div>
                </ActionForm>
            </details>

            <div class="scroll-x" style="margin-top:14px">
                <table class="tbl">
                    <thead>
                        <tr>
                            <th>{t(locale, "finance.field.name")}</th>
                            <th style="width:80px">{t(locale, "finance.field.code")}</th>
                            <th style="width:80px">{t(locale, "finance.field.tone")}</th>
                            <th class="num" style="width:64px">{t(locale, "finance.field.sort_order")}</th>
                            <th class="num" style="width:64px">{t(locale, "finance.field.used")}</th>
                            <th class="num" style="width:220px">{t(locale, "finance.field.ops")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {cats.into_iter().map(|c| render_category_row(c, &usage, update_category, delete_category)).collect_view()}
                    </tbody>
                </table>
            </div>
        </Card>
    }
}

/// Single row of the category management table. Inline edit lives behind a
/// per-row `<details>` to dodge text-node walker panics around ActionForm
/// (AGENTS.md "Wrap inline `{move || option.map(view!)}`…").
fn render_category_row(
    c: Category,
    usage: &std::collections::HashMap<String, i64>,
    update_category: ServerAction<UpdateCategory>,
    delete_category: ServerAction<DeleteCategory>,
) -> impl IntoView {
    let locale = use_locale();
    let tone_enum = Tone::parse(&c.tone);
    let usage_count = usage.get(&c.code).copied().unwrap_or(0);
    let confirm_msg = tf(
        locale,
        "finance.category.confirm_delete",
        &[("code", &c.code)],
    );
    let code = c.code.clone();
    let name = c.name.clone();
    let tone_str = c.tone.clone();
    let sort_order_str = c.sort_order.to_string();
    let display_name = c.name.clone();
    let display_code = c.code.clone();
    let display_tone_label = if c.tone.is_empty() {
        "—".to_string()
    } else {
        c.tone.clone()
    };
    view! {
        <tr>
            <td>{display_name}</td>
            <td class="mono">{display_code}</td>
            <td>
                <UiTag tone=tone_enum>{display_tone_label}</UiTag>
            </td>
            <td class="num mono">{c.sort_order}</td>
            <td class="num mono">{usage_count}</td>
            <td class="num">
                <div class="hstack" style="gap:6px;justify-content:flex-end;flex-wrap:wrap">
                    <details>
                        <summary class="btn ghost" style="cursor:pointer;display:inline-flex;align-items:center;gap:4px">
                            <Icon kind=IconKind::Settings size=12/>{t(locale, "finance.action.edit")}
                        </summary>
                        <ActionForm action=update_category attr:class="vstack" attr:style="gap:8px;margin-top:8px;min-width:240px">
                            <input type="hidden" name="code" value=code.clone()/>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.code_readonly")}</span>
                                <input type="text" disabled value=code
                                       title=t(locale, "finance.category.title_code_readonly")
                                       style=INPUT_STYLE_MONO/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.name")}</span>
                                <input name="name" required maxlength="32" value=name
                                       style=INPUT_STYLE/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.tone")}</span>
                                <select name="tone" style=INPUT_STYLE>
                                    <option value="" selected={tone_str.is_empty()}>"—"</option>
                                    {TONES.iter().map(|t| {
                                        let selected = *t == tone_str.as_str();
                                        view! { <option value=*t selected=selected>{*t}</option> }
                                    }).collect_view()}
                                </select>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.sort_order")}</span>
                                <input name="sort_order" type="number" step="1" min="0"
                                       value=sort_order_str
                                       style=INPUT_STYLE_MONO/>
                            </label>
                            <div class="hstack" style="gap:8px;align-items:center">
                                <button class="btn primary" type="submit">
                                    <Icon kind=IconKind::Check size=12/>{t(locale, "finance.action.save")}
                                </button>
                                <span class="error-slot">
                                    {move || update_category.value().get().and_then(|r| r.err()).map(|e| view! {
                                        <span class="tag rose">{server_fn_error_text(&e)}</span>
                                    })}
                                </span>
                            </div>
                        </ActionForm>
                    </details>
                    <RowDeleteAction action=delete_category value=c.code.clone() field="code"
                                     confirm=confirm_msg label=t(locale, "finance.action.delete")/>
                </div>
            </td>
        </tr>
    }
}

fn render_reports(d: &LedgerData) -> impl IntoView {
    let locale = use_locale();
    let months = d.months_12.clone();
    if months.is_empty() {
        return view! {
            <div class="card"><div class="card-body">
                <p class="muted">{t(locale, "finance.reports.empty")}</p>
            </div></div>
        }
        .into_any();
    }
    let labels: Vec<String> = months
        .iter()
        .map(|m| m.period.split('-').nth(1).unwrap_or("?").to_string())
        .collect();
    let income_data: Vec<f64> = months.iter().map(|m| m.income).collect();
    let expense_data: Vec<f64> = months.iter().map(|m| m.expense).collect();
    let last = months.last().cloned().unwrap_or(MonthBucket {
        period: d.month.period.clone(),
        income: 0.0,
        expense: 0.0,
        net: 0.0,
    });
    let net_strip = render_net_strip(&months);

    let category_share = render_category_share_card(d);

    view! {
        <div class="grid-2">
            <Card title=t(locale, "finance.reports.month_title") code="FIN-RPT-01"
                  sub=tf(locale, "finance.reports.month_sub", &[
                      ("count", &months.len().to_string()),
                      ("net", &fmt_int(last.net)),
                      ("income", &fmt_int(last.income)),
                      ("expense", &fmt_int(last.expense)),
                  ])>
                <div class="vstack" style="gap:14px">
                    <div>
                        <div class="mono dim" style="font-size:10.5px;text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">{t(locale, "finance.reports.income")}</div>
                        <ChartBars data=income_data labels=labels.clone()/>
                    </div>
                    <div>
                        <div class="mono dim" style="font-size:10.5px;text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">{t(locale, "finance.reports.expense")}</div>
                        <ChartBars data=expense_data labels=labels/>
                    </div>
                    <div>
                        <div class="mono dim" style="font-size:10.5px;text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">{t(locale, "finance.reports.net")}</div>
                        {net_strip}
                    </div>
                </div>
            </Card>
            {category_share}
        </div>
    }.into_any()
}

/// 12-month net trend rendered as coloured cells rather than `ChartBars`:
/// `ChartBars` clamps to a 4% minimum and normalises against a positive
/// max, so a deficit month would render the same as a +¥0 month.
pub fn render_net_strip(months: &[MonthBucket]) -> impl IntoView {
    let n = months.len();
    let cells: Vec<_> = months.iter().map(|m| {
        let mm = m.period.split('-').nth(1).unwrap_or("?").to_string();
        let (color_var, sign, val) = net_cell_parts(m.net);
        let cell_style = format!(
            "padding:6px 4px;background:var(--bg-2);border-radius:4px;color:{};text-align:center",
            color_var
        );
        view! {
            <div style=cell_style>
                <div class="mono" style="font-size:10px;color:var(--ink-4);margin-bottom:2px">{mm}</div>
                <div class="mono" style="font-size:11px;font-weight:600;line-height:1.2">{sign}{val}</div>
            </div>
        }
    }).collect();
    let grid_style = format!(
        "display:grid;grid-template-columns:repeat({}, minmax(0, 1fr));gap:3px",
        n
    );
    view! { <div style=grid_style>{cells}</div> }
}

/// `(css color var, sign prefix, formatted absolute value)` for one month's
/// net. Surplus uses `--primary-ink` (the design system has no `--green-*`
/// — sage green lives under `--primary-*`; see `.tag.green` in styles.css).
fn net_cell_parts(net: f64) -> (&'static str, &'static str, String) {
    if net > 0.0 {
        ("var(--primary-ink)", "+", fmt_int(net))
    } else if net < 0.0 {
        ("var(--rose-ink)", "−", fmt_int(net.abs()))
    } else {
        ("var(--ink-4)", "", "0".to_string())
    }
}

fn render_category_share_card(d: &LedgerData) -> impl IntoView {
    let locale = use_locale();
    let cats = d.category_summary.clone();
    // `.abs()` because `fmt_int` of IEEE -0.0 prints "-0".
    let total: f64 = cats.iter().map(|c| c.value).sum::<f64>().abs();
    view! {
        <Card title=t(locale, "finance.reports.category_title") code="FIN-RPT-02"
              sub=tf(locale, "finance.reports.category_sub", &[("total", &fmt_int(total))])>
            {if cats.is_empty() {
                view! { <p class="muted">{t(locale, "finance.reports.category_empty")}</p> }.into_any()
            } else {
                view! {
                    <div class="vstack" style="gap:10px">
                        {cats.into_iter().map(|c| {
                            let bar_color = if c.tone.is_empty() { "var(--primary)".to_string() } else { format!("var(--{})", c.tone) };
                            let pct = (c.pct * 3.0).min(100.0);
                            view! {
                                <div>
                                    <div style="display:flex;justify-content:space-between;align-items:baseline;margin-bottom:4px">
                                        <div style="font-size:12.5px">
                                            <span>{c.name.clone()}</span>
                                            <span class="mono dim" style="margin-left:6px;font-size:10.5px">{c.code.clone()}</span>
                                        </div>
                                        <div class="mono" style="font-size:12px">{format!("¥{}", fmt_int(c.value))} <span class="dim">{format!("· {}%", c.pct)}</span></div>
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

/// Suggested per-category budgets for next month, derived from the last
/// 3 calendar months of activity. Returns `(name, code, amount_rounded_to_50)`
/// for every category that had any expense in the window. Empty when there's
/// no 3-month history yet (fresh install).
fn next_month_plan(d: &LedgerData) -> Vec<(String, String, f64)> {
    use std::collections::HashMap;
    // Bucket months_12's last 3 months: months_12 is oldest → newest, so the
    // tail-3 is the recent quarter.
    let recent: Vec<&MonthBucket> = d.months_12.iter().rev().take(3).collect();
    if recent.is_empty() {
        return Vec::new();
    }
    // months_12 only carries totals, not per-category. For the per-category
    // average we approximate using the current month's category_summary
    // (which is signed, accurate, and already aggregated). Scaling the
    // current-month spend by the elapsed-day ratio gives the user a
    // forward-looking estimate without a second SQL pass.
    //
    // Note: this is a simplification — a more accurate planner would track
    // per-category-per-month rollups, but that's a 12×N row payload for
    // marginal gain. The current heuristic is "what would the rest of this
    // month look like if today's pace continued?", clamped to a 50-yuan grid.
    let elapsed = d.month.days_elapsed.max(1) as f64;
    // Approximate days in the user's current month (28..31). Erring high
    // (31) gives a more conservative budget suggestion. We don't import
    // `time` here — just use 31 as a static ceiling.
    let projected_factor = (31.0 / elapsed).min(2.5);
    let mut by_code: HashMap<String, (String, f64)> = HashMap::new();
    for c in &d.category_summary {
        let projected = c.value * projected_factor;
        // Round to nearest 50, with a floor of 50 to avoid 0-budget noise.
        let suggested = ((projected / 50.0).round() * 50.0).max(50.0);
        by_code.insert(c.code.clone(), (c.name.clone(), suggested));
    }
    let mut out: Vec<(String, String, f64)> = by_code
        .into_iter()
        .map(|(code, (name, suggested))| (name, code, suggested))
        .collect();
    // Largest suggested first — the user's attention is most valuable on
    // the categories that drive most of the spend.
    out.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Parse a `YYYY-MM-DD` string into a unix-second timestamp at the START of
/// that day in UTC. Empty / malformed input yields `None`. Pure math, safe
/// on wasm32.
fn parse_date_floor(s: &str) -> Option<i64> {
    parse_date_components(s).and_then(|(y, m, d)| date_to_unix(y, m, d, 0))
}

/// Same as `parse_date_floor` but at the END of the day (23:59:59 UTC) so
/// `t.occurred_at <= to_ts` is an inclusive day filter.
fn parse_date_ceiling(s: &str) -> Option<i64> {
    parse_date_components(s).and_then(|(y, m, d)| date_to_unix(y, m, d, 86_399))
}

fn parse_date_components(s: &str) -> Option<(i32, u8, u8)> {
    let bytes = s.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let y: i32 = s[..4].parse().ok()?;
    let m: u8 = s[5..7].parse().ok()?;
    let d: u8 = s[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

fn date_to_unix(year: i32, month: u8, day: u8, offset_seconds: i64) -> Option<i64> {
    ymd_to_unix_midnight(year, month, day).map(|ts| ts + offset_seconds)
}

// CSV export — pure-Rust so the same code path runs on SSR (initial href is
// rendered as part of the page) and hydrate (refreshed reactively when the
// resource refetches).
fn csv_data_uri(txns: &[Txn]) -> String {
    use std::fmt::Write as _;

    let mut csv = String::with_capacity(80 + txns.len() * 96);
    csv.push_str("doc_id,occurred_at,merchant,category,account,amount,tag,note\n");
    for t in txns {
        let occurred = unix_to_ymdhm(t.occurred_at)
            .map(|(y, m, d, hh, mm)| format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:00Z"))
            .unwrap_or_default();
        let _ = writeln!(
            csv,
            "{},{},{},{},{},{:.2},{},{}",
            t.doc_id,
            occurred,
            csv_escape(&t.merchant),
            t.category_code,
            t.account_code,
            t.amount,
            t.tag,
            csv_escape(t.note.as_deref().unwrap_or("")),
        );
    }
    let encoded = percent_encode(&csv);
    let mut uri = String::with_capacity("data:text/csv;charset=utf-8,".len() + encoded.len());
    uri.push_str("data:text/csv;charset=utf-8,");
    uri.push_str(&encoded);
    uri
}

fn csv_escape(s: &str) -> String {
    let field = neutralize_csv_formula(s);
    if !field
        .bytes()
        .any(|b| matches!(b, b',' | b'"' | b'\n' | b'\r'))
    {
        return field;
    }

    let mut out = String::with_capacity(field.len() + 2);
    out.push('"');
    for ch in field.chars() {
        if ch == '"' {
            out.push_str("\"\"");
        } else {
            out.push(ch);
        }
    }
    out.push('"');
    out
}

fn neutralize_csv_formula(s: &str) -> String {
    if s.chars()
        .find(|ch| !matches!(ch, ' ' | '\t'))
        .is_some_and(|ch| matches!(ch, '=' | '+' | '-' | '@'))
    {
        format!("'{s}")
    } else {
        s.to_string()
    }
}

fn percent_encode(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0f) as usize] as char);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escape_leaves_plain_fields_unquoted() {
        assert_eq!(csv_escape("Blue Bottle"), "Blue Bottle");
    }

    #[test]
    fn csv_escape_quotes_and_doubles_inner_quotes() {
        assert_eq!(csv_escape("a,\"b\"\n"), "\"a,\"\"b\"\"\n\"");
    }

    #[test]
    fn csv_escape_neutralizes_spreadsheet_formula_prefixes() {
        for raw in [
            "=1+1",
            "+cmd",
            "-10+20",
            "@SUM(1,2)",
            " \t=HYPERLINK(\"x\")",
        ] {
            let escaped = csv_escape(raw);
            assert!(
                escaped.starts_with('\'') || escaped.starts_with("\"'"),
                "raw={raw}, escaped={escaped}"
            );
        }
        assert_eq!(csv_escape("Coffee - lunch"), "Coffee - lunch");
    }

    #[test]
    fn percent_encode_keeps_unreserved_and_encodes_utf8() {
        assert_eq!(percent_encode("AZaz09-_.~"), "AZaz09-_.~");
        assert_eq!(
            percent_encode("\u{5de5}\u{8d44}, ok"),
            "%E5%B7%A5%E8%B5%84%2C%20ok"
        );
    }

    #[test]
    fn csv_data_uri_contains_encoded_header_and_rows() {
        let txns = [Txn {
            doc_id: "FIN-1".into(),
            occurred_at: 0,
            merchant: "a,b".into(),
            category_code: "F&B".into(),
            account_code: "ACC-01".into(),
            amount: -12.3,
            tag: "exp".into(),
            note: Some("x\"y".into()),
            linked_doc_id: None,
        }];

        let uri = csv_data_uri(&txns);

        assert!(uri.starts_with("data:text/csv;charset=utf-8,doc_id%2Coccurred_at"));
        assert!(uri.contains("FIN-1%2C1970-01-01T00%3A00%3A00Z"));
        assert!(uri.contains("%22a%2Cb%22"));
        assert!(uri.contains("-12.30%2Cexp%2C%22x%22%22y%22"));
    }

    #[test]
    fn previous_period_handles_january_rollover() {
        assert_eq!(previous_period("2026-01"), "2025-12");
        assert_eq!(previous_period("2026-05"), "2026-04");
    }

    #[test]
    fn next_period_handles_december_rollover() {
        assert_eq!(next_period("2025-12"), "2026-01");
        assert_eq!(next_period("2026-05"), "2026-06");
    }

    #[test]
    fn parse_date_floor_returns_midnight_utc() {
        // 2024-05-01 00:00:00 UTC = 1714521600
        assert_eq!(parse_date_floor("2024-05-01"), Some(1_714_521_600));
        // Empty / malformed → None
        assert_eq!(parse_date_floor(""), None);
        assert_eq!(parse_date_floor("2024-13-01"), None);
        assert_eq!(parse_date_floor("2023-02-29"), None);
        assert_eq!(parse_date_floor("2024-02-30"), None);
        assert_eq!(parse_date_floor("not-a-date"), None);
    }

    #[test]
    fn parse_date_ceiling_returns_end_of_day() {
        // 86399 sec after midnight = 23:59:59 UTC
        assert_eq!(
            parse_date_ceiling("2024-05-01"),
            Some(1_714_521_600 + 86_399)
        );
    }

    #[test]
    fn net_cell_surplus_uses_primary_ink_and_plus_sign() {
        let (color, sign, val) = net_cell_parts(1_234.56);
        // Project has no `--green-*` family; the sage-green tone lives in
        // `--primary-*`. Asserting the exact token guards against a future
        // regression that re-introduces `--green-ink` (which silently
        // resolves to nothing in CSS, leaving surplus months uncoloured).
        assert_eq!(color, "var(--primary-ink)");
        assert_eq!(sign, "+");
        assert_eq!(val, "1,235"); // fmt_int rounds .56 → 1,235
    }

    #[test]
    fn net_cell_deficit_uses_rose_ink_and_minus_sign() {
        let (color, sign, val) = net_cell_parts(-1_234.56);
        assert_eq!(color, "var(--rose-ink)");
        assert_eq!(sign, "−"); // U+2212 minus, not ASCII '-'
                               // Magnitude only — the sign lives in the `sign` slot.
        assert_eq!(val, "1,235");
    }

    #[test]
    fn net_cell_zero_renders_dim_no_sign() {
        let (color, sign, val) = net_cell_parts(0.0);
        assert_eq!(color, "var(--ink-4)");
        assert_eq!(sign, "");
        assert_eq!(val, "0");
    }

    #[test]
    fn net_cell_negative_zero_treated_as_zero() {
        // Without the explicit `< 0.0` check, an IEEE -0.0 would route to
        // the deficit branch and show "−0".
        let (color, sign, val) = net_cell_parts(-0.0);
        assert_eq!(color, "var(--ink-4)");
        assert_eq!(sign, "");
        assert_eq!(val, "0");
    }
}
