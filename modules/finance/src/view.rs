use crate::model::{
    Account, AccountStats, Category, Currency, MonthBucket, Tag, Txn, ACCOUNT_TYPES, TONES,
};
use crate::server_fns::*;
use ep_core::{
    amount_step, fmt_minor, fmt_minor_compact, fmt_minor_raw, fmt_ts_date, fmt_ts_hm, fmt_ts_md,
    fmt_ts_ymd, major_to_minor, parse_ymd, unix_to_ymdhm, ymd_to_unix_midnight, IconKind,
    MinorAmount, Tone,
};
use ep_i18n::{server_fn_error_text, t, tf, use_locale};
use ep_ui::{
    use_toast, Card, ChartBars, Direction, EmptyState, ErrorSlot, Icon, Kpi, PageHead,
    RowDeleteAction, SkeletonCard, SkeletonKpi, TabSpec, Tabs, Tag as UiTag,
};
use leptos::prelude::*;

#[derive(Clone, Copy)]
struct LedgerFilters {
    merchant: RwSignal<String>,
    category: RwSignal<String>,
    date_from: RwSignal<String>,
    date_to: RwSignal<String>,
}

#[derive(Clone, Copy)]
struct FinanceActions {
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
    create_currency: ServerAction<CreateCurrency>,
    update_currency: ServerAction<UpdateCurrency>,
    delete_currency: ServerAction<DeleteCurrency>,
    set_primary_currency: ServerAction<SetPrimaryCurrency>,
}

impl FinanceActions {
    fn versions(self) -> [usize; 16] {
        [
            self.add.version().get(),
            self.delete.version().get(),
            self.set_budget.version().get(),
            self.import_budgets.version().get(),
            self.create_account.version().get(),
            self.update_account.version().get(),
            self.delete_account.version().get(),
            self.create_category.version().get(),
            self.update_category.version().get(),
            self.delete_category.version().get(),
            self.update_txn.version().get(),
            self.add_transfer.version().get(),
            self.create_currency.version().get(),
            self.update_currency.version().get(),
            self.delete_currency.version().get(),
            self.set_primary_currency.version().get(),
        ]
    }
}

#[derive(Clone, Copy)]
struct FinanceUiState {
    active: RwSignal<String>,
    /// Currency code the whole module is scoped to ("每个货币独立分页"). Empty
    /// resolves to the primary currency server-side.
    selected_currency: RwSignal<String>,
    txn_modal_open: RwSignal<bool>,
    txn_modal_mode: RwSignal<String>,
    filters: LedgerFilters,
}

struct TxnRowContext<'a> {
    cat_lookup: &'a std::collections::HashMap<String, Category>,
    cat_options: std::sync::Arc<Vec<Category>>,
    acc_options: std::sync::Arc<Vec<Account>>,
    delete: ServerAction<DeleteTxn>,
    update_txn: ServerAction<UpdateTxn>,
    decimals: u8,
    symbol: String,
}

/// On the first time `action.value()` transitions to `Ok`, push a toast
/// and, when supplied, close `modal_open`. Subsequent ticks for the same
/// version (leptos resamples the value) are de-duplicated via a per-action
/// version compare against the action's internal `version()` signal.
fn wire_action_toast<S, F>(
    action: ServerAction<S>,
    toast: ep_ui::ToastStack,
    modal_open: Option<RwSignal<bool>>,
    msg: F,
) where
    S: leptos::server_fn::ServerFn + Clone + Send + Sync + 'static,
    <S as leptos::server_fn::ServerFn>::Output: Clone + Send + Sync + 'static,
    <S as leptos::server_fn::ServerFn>::Error: Clone + Send + Sync + 'static,
    F: Fn(&<S as leptos::server_fn::ServerFn>::Output) -> String + Send + Sync + 'static,
{
    let last_seen = RwSignal::new(0usize);
    Effect::new(move |_| {
        let v = action.version().get();
        if v == 0 || v == last_seen.get_untracked() {
            return;
        }
        if let Some(Ok(out)) = action.value().get() {
            toast.success(msg(&out));
            if let Some(open) = modal_open {
                open.set(false);
            }
            last_seen.set(v);
        }
    });
}

#[component]
pub fn FinanceView() -> impl IntoView {
    let locale = use_locale();
    let active = RwSignal::new(String::from("ledger"));
    let selected_currency = RwSignal::new(String::new());
    let ledger = Resource::new(
        move || selected_currency.get(),
        |code| async move { load_ledger(code).await },
    );
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
    let create_currency = ServerAction::<CreateCurrency>::new();
    let update_currency = ServerAction::<UpdateCurrency>::new();
    let delete_currency = ServerAction::<DeleteCurrency>::new();
    let set_primary_currency = ServerAction::<SetPrimaryCurrency>::new();
    let txn_modal_open = RwSignal::new(false);
    let txn_modal_mode = RwSignal::new(String::from("txn"));
    let merchant_filter = RwSignal::new(String::new());
    let category_filter = RwSignal::new(String::new());
    let date_from_filter = RwSignal::new(String::new());
    let date_to_filter = RwSignal::new(String::new());
    let actions = FinanceActions {
        add,
        delete,
        set_budget,
        import_budgets,
        create_account,
        update_account,
        delete_account,
        create_category,
        update_category,
        delete_category,
        update_txn,
        add_transfer,
        create_currency,
        update_currency,
        delete_currency,
        set_primary_currency,
    };
    let ui = FinanceUiState {
        active,
        selected_currency,
        txn_modal_open,
        txn_modal_mode,
        filters: LedgerFilters {
            merchant: merchant_filter,
            category: category_filter,
            date_from: date_from_filter,
            date_to: date_to_filter,
        },
    };

    // Refetch when any action's version ticks. We compare per-element via a
    // fixed-size array so the closure stays Send + 'static. (Switching the
    // selected currency refetches on its own — it is the resource's source.)
    Effect::new(move |prev: Option<[usize; 16]>| {
        let cur = actions.versions();
        if prev.is_some_and(|p| p != cur) {
            ledger.refetch();
        }
        cur
    });

    // Toast / modal feedback: when a server action transitions from pending
    // to Ok, push a toast and close any open modal so the user gets a clear
    // "done" signal without manually dismissing.
    let toast = use_toast();
    wire_action_toast(add, toast, Some(txn_modal_open), move |_| {
        t(locale, "finance.toast.txn_added").to_string()
    });
    wire_action_toast(update_txn, toast, Some(txn_modal_open), move |_| {
        t(locale, "finance.toast.txn_updated").to_string()
    });
    wire_action_toast(add_transfer, toast, Some(txn_modal_open), move |_| {
        t(locale, "finance.toast.transfer_added").to_string()
    });
    wire_action_toast(delete, toast, None, move |_| {
        t(locale, "finance.toast.txn_deleted").to_string()
    });
    wire_action_toast(create_account, toast, None, move |a: &Account| {
        ep_i18n::tf(locale, "finance.toast.account_added", &[("name", &a.name)])
    });
    wire_action_toast(update_account, toast, None, move |_| {
        t(locale, "finance.toast.account_updated").to_string()
    });
    wire_action_toast(delete_account, toast, None, move |_| {
        t(locale, "finance.toast.account_deleted").to_string()
    });
    wire_action_toast(create_category, toast, None, move |c: &Category| {
        ep_i18n::tf(locale, "finance.toast.category_added", &[("name", &c.name)])
    });
    wire_action_toast(update_category, toast, None, move |_| {
        t(locale, "finance.toast.category_updated").to_string()
    });
    wire_action_toast(delete_category, toast, None, move |_| {
        t(locale, "finance.toast.category_deleted").to_string()
    });
    wire_action_toast(set_budget, toast, None, move |_| {
        t(locale, "finance.toast.budget_saved").to_string()
    });
    wire_action_toast(import_budgets, toast, None, move |_| {
        t(locale, "finance.toast.budget_imported").to_string()
    });
    wire_action_toast(create_currency, toast, None, move |c: &Currency| {
        ep_i18n::tf(locale, "finance.toast.currency_added", &[("code", &c.code)])
    });
    wire_action_toast(update_currency, toast, None, move |_| {
        t(locale, "finance.toast.currency_updated").to_string()
    });
    wire_action_toast(delete_currency, toast, None, move |_| {
        t(locale, "finance.toast.currency_deleted").to_string()
    });
    wire_action_toast(set_primary_currency, toast, None, move |_| {
        t(locale, "finance.toast.primary_set").to_string()
    });

    let page_actions = view! {
        <button class="btn primary" type="button"
                on:click=move |_| {
                    active.set(String::from("ledger"));
                    txn_modal_mode.set(String::from("txn"));
                    txn_modal_open.set(true);
                }>
            <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.action.add_txn")}
        </button>
    }
    .into_any();

    view! {
        <div class="view">
            <PageHead
                code="FIN-01"
                module=t(locale, "finance.page.module")
                title=t(locale, "finance.page.title")
                title_cn=t(locale, "finance.page.title_cn")
                sub=t(locale, "finance.page.sub")
                actions=page_actions
            />

            <Suspense fallback=move || view! {
                <div style="margin-bottom:20px"><SkeletonCard rows=0/></div>
                <SkeletonKpi count=4/>
                <SkeletonCard rows=3/>
            }>
                {move || ledger.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</div></div> }.into_any(),
                    Ok(data) => render_ledger(data, ui, actions).into_any(),
                })}
            </Suspense>
        </div>
    }
}

/// The currency switcher strip — one button per currency, the active one (as
/// resolved server-side) highlighted. Clicking re-scopes the whole module.
fn render_currency_switcher(d: &LedgerData, selected: RwSignal<String>) -> impl IntoView {
    let active_code = d.currency.code.clone();
    let pills: Vec<_> = d
        .currencies
        .iter()
        .map(|c| {
            let code = c.code.clone();
            let is_active = code == active_code;
            let label = format!("{} {}", c.symbol, c.code);
            let title = currency_caption(c);
            let cls = if is_active {
                "btn primary sm"
            } else {
                "btn sm"
            };
            view! {
                <button class=cls type="button" title=title
                        on:click=move |_| {
                            if selected.get_untracked() != code {
                                selected.set(code.clone());
                            }
                        }>
                    {label}
                </button>
            }
        })
        .collect();
    view! {
        <div class="hstack" style="gap:6px;flex-wrap:wrap;margin-bottom:16px;align-items:center">
            {pills}
        </div>
    }
}

fn render_ledger(data: LedgerData, ui: FinanceUiState, actions: FinanceActions) -> impl IntoView {
    let locale = use_locale();
    let active = ui.active;
    let decimals = data.currency.decimals;
    let symbol = data.currency.symbol.clone();
    let m = &data.month;
    let bud_pct = if m.budget_total.is_positive() {
        (m.expense.to_f64() / m.budget_total.to_f64() * 100.0).round() as u32
    } else {
        0
    };
    let bud_dir = match bud_pct {
        0..=60 => Direction::Up,
        61..=85 => Direction::Flat,
        _ => Direction::Down,
    };
    // Daily spend / 3-month-rolling daily spend, used for the FIN-K02 trend.
    // All figures are minor units; the threshold scales with the currency.
    let daily_now = m.expense / i128::from(m.days_elapsed.max(1));
    let daily_3m = m.avg_expense_3m / 30;
    let daily_delta = daily_now - daily_3m;
    let dir_eps = MinorAmount::new(10_i128.pow(u32::from(decimals)) / 2);
    let daily_dir = if daily_delta < -dir_eps {
        Direction::Up
    }
    // less spend = up (saving)
    else if daily_delta > dir_eps {
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
    let currencies_count = data.currencies.len() as u32;

    let switcher = render_currency_switcher(&data, ui.selected_currency);
    let banner = render_banner(&data);
    // Pre-compute attribute strings — the `view!` macro rejects bare if/else
    // in attribute-value position.
    let daily_delta_text = if daily_3m.is_positive() {
        let sign = if daily_delta >= 0 { "+" } else { "−" };
        tf(
            locale,
            "finance.kpi.spend_vs_avg",
            &[
                ("sign", sign),
                ("symbol", &symbol),
                ("amount", &fmt_minor_compact(daily_delta.abs(), decimals)),
            ],
        )
    } else {
        tf(
            locale,
            "finance.kpi.day_insufficient",
            &[("day", &m.days_elapsed.to_string())],
        )
    };
    let savings_delta_text = if m.savings >= 0 {
        tf(
            locale,
            "finance.kpi.net_savings",
            &[
                ("symbol", &symbol),
                ("amount", &fmt_minor_compact(m.savings, decimals)),
            ],
        )
    } else {
        tf(
            locale,
            "finance.kpi.net_deficit",
            &[
                ("symbol", &symbol),
                ("amount", &fmt_minor_compact(m.savings.abs(), decimals)),
            ],
        )
    };
    let emergency_delta_text = if m.avg_expense_3m > 0 {
        tf(
            locale,
            "finance.kpi.emergency_delta",
            &[
                ("symbol", &symbol),
                ("liquid", &fmt_minor_compact(m.liquid_balance, decimals)),
                ("avg", &fmt_minor_compact(m.avg_expense_3m, decimals)),
            ],
        )
    } else {
        t(locale, "finance.kpi.emergency_empty").to_string()
    };
    let kpi_budget = format!(
        "{s}{} / {s}{}",
        fmt_minor_compact(m.expense, decimals),
        fmt_minor_compact(m.budget_total, decimals),
        s = symbol
    );
    let kpi_daily = format!("{}{}", symbol, fmt_minor_compact(daily_now, decimals));
    let kpis = view! {
        <div class="kpi-grid">
            <Kpi code="FIN-K01" label=t(locale, "finance.kpi.budget") value=format!("{}", bud_pct) unit="%".to_string()
                 delta=kpi_budget
                 dir=bud_dir/>
            <Kpi code="FIN-K02" label=t(locale, "finance.kpi.daily_spend")
                 value=kpi_daily
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

    // Order matches the real onboarding path: Ledger (daily use) →
    // Accounts → Categories (setup) → Budget (planning) → Reports (review),
    // with the occasional-use Currencies config last.
    let tabs = vec![
        TabSpec::new("ledger", t(locale, "finance.tab.ledger")).with_count(txns_count),
        TabSpec::new("accounts", t(locale, "finance.tab.accounts")).with_count(accounts_count),
        TabSpec::new("categories", t(locale, "finance.tab.categories"))
            .with_count(categories_count),
        TabSpec::new("budget", t(locale, "finance.tab.budget")).with_count(budgets_count),
        TabSpec::new("reports", t(locale, "finance.tab.reports")),
        TabSpec::new("currencies", t(locale, "finance.tab.currencies"))
            .with_count(currencies_count),
    ];

    // Share the loaded LedgerData across every tab branch by Arc instead of
    // deep-cloning per branch. (Arc, not Rc — Leptos's reactive closures
    // require Send.) Each tab closure clones a cheap handle.
    let data = std::sync::Arc::new(data);
    let data_for_ledger = data.clone();
    let data_for_budget = data.clone();
    let data_for_accounts = data.clone();
    let data_for_categories = data.clone();
    let data_for_currencies = data.clone();
    let data_for_reports = data;

    view! {
        {switcher}
        {banner}
        {kpis}
        <Tabs tabs=tabs active=active/>
        {move || match active.get().as_str() {
            "budget" => render_budget(&data_for_budget, actions.set_budget, actions.import_budgets).into_any(),
            "accounts" => render_accounts(&data_for_accounts, actions.create_account, actions.update_account, actions.delete_account).into_any(),
            "categories" => render_categories(&data_for_categories, actions.create_category, actions.update_category, actions.delete_category).into_any(),
            "reports" => render_reports(&data_for_reports).into_any(),
            "currencies" => render_currencies(&data_for_currencies, actions.create_currency, actions.update_currency, actions.delete_currency, actions.set_primary_currency).into_any(),
            _ => view! {
                {render_txn_modal(
                    actions.add,
                    actions.add_transfer,
                    data_for_ledger.clone(),
                    ui.txn_modal_open,
                    ui.txn_modal_mode,
                )}
                {render_ledger_tab(
                    &data_for_ledger,
                    actions.delete,
                    actions.update_txn,
                    ui.filters,
                )}
            }.into_any(),
        }}
    }
}

fn render_banner(d: &LedgerData) -> impl IntoView {
    let locale = use_locale();
    let m = &d.month;
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.clone();
    let week_sign = if m.balance_delta >= 0 { "+" } else { "−" };
    let savings_pct = (m.savings_rate * 100.0).round() as u32;
    // Net-worth tone tracks the most recent week: + → green/healthy,
    // 0 → flat/neutral, − → rose/watch.
    let (worth_tone, worth_label_key) = if m.balance_delta > 0 {
        (Tone::Green, "finance.banner.status.healthy")
    } else if m.balance_delta == 0 {
        (Tone::None, "finance.banner.status.flat")
    } else {
        (Tone::Rose, "finance.banner.status.watch")
    };
    let glyph = symbol.clone();
    let balance_text = format!("{}{}", symbol, fmt_minor(m.balance, decimals));
    let income_text = format!("+{}{}", symbol, fmt_minor_compact(m.income, decimals));
    let expense_text = format!("−{}{}", symbol, fmt_minor_compact(m.expense, decimals));
    let savings_text = format!(
        "{}{}{}",
        if m.savings >= 0 { "" } else { "−" },
        symbol,
        fmt_minor_compact(m.savings.abs(), decimals)
    );
    let currency_label = currency_caption(&d.currency);
    view! {
        <div class="module-banner">
            <div class="module-glyph fin mono">{glyph}</div>
            <div style="flex:1">
                <div class="hstack" style="margin-bottom:6px;gap:8px">
                    <span class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">{t(locale, "finance.banner.net_worth")}</span>
                    <UiTag tone=worth_tone dot=true>{t(locale, worth_label_key)}</UiTag>
                    <span class="mono" style="font-size:10.5px;color:var(--ink-4)">{currency_label}</span>
                </div>
                <div class="mono" style="font-size:32px;font-weight:600;letter-spacing:-0.02em;line-height:1.1">
                    {balance_text}
                </div>
                <div class="hstack" style="gap:16px;margin-top:8px;font-size:12.5px;color:var(--ink-3)">
                    <span class="mono">{tf(locale, "finance.banner.balance_delta", &[("sign", week_sign), ("symbol", &symbol), ("amount", &fmt_minor_compact(m.balance_delta.abs(), decimals))])}</span>
                    <span class="mono">{tf(locale, "finance.banner.savings_rate", &[("pct", &savings_pct.to_string())])}</span>
                    <span class="mono">{
                        let n = d.accounts.len();
                        if n == 1 { t(locale, "finance.banner.account_count_one").to_string() }
                        else { tf(locale, "finance.banner.account_count_other", &[("count", &n.to_string())]) }
                    }</span>
                </div>
            </div>
            <div class="hstack" style="gap:20px;padding-right:8px">
                <div>
                    <div class="mono" style="font-size:10.5px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:4px">{t(locale, "finance.banner.income")}</div>
                    <div class="mono" style="font-size:18px;font-weight:600;color:var(--primary-ink)">{income_text}</div>
                </div>
                <div class="sep-v"></div>
                <div>
                    <div class="mono" style="font-size:10.5px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:4px">{t(locale, "finance.banner.spend")}</div>
                    <div class="mono" style="font-size:18px;font-weight:600">{expense_text}</div>
                </div>
                <div class="sep-v"></div>
                <div>
                    <div class="mono" style="font-size:10.5px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:4px">{t(locale, "finance.banner.net_savings")}</div>
                    <div class="mono" style="font-size:18px;font-weight:600">{savings_text}</div>
                </div>
            </div>
        </div>
    }
}

const INPUT_STYLE: &str =
    "padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)";
const INPUT_STYLE_MONO: &str = "padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono)";
const FIELD_LABEL: &str = "font-size:11px;text-transform:uppercase;letter-spacing:0.06em";

fn currency_caption(c: &Currency) -> String {
    let remark = c.remark.trim();
    if remark.is_empty() {
        c.code.clone()
    } else {
        format!("{} · {}", c.code, remark)
    }
}

fn category_label(icon: &str, name: &str) -> String {
    let icon = icon.trim();
    if icon.is_empty() {
        name.to_string()
    } else {
        format!("{icon} {name}")
    }
}

fn category_display(c: &Category) -> String {
    category_label(&c.icon, &c.name)
}

fn category_summary_display(c: &crate::model::CategorySummary) -> String {
    category_label(&c.icon, &c.name)
}

fn render_txn_modal(
    add: ServerAction<AddTxn>,
    add_transfer: ServerAction<AddTransfer>,
    data: std::sync::Arc<LedgerData>,
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
                let body = if is_transfer {
                    render_transfer_form(add_transfer, &data).into_any()
                } else {
                    render_new_txn_form(add, &data).into_any()
                };
                view! {
                    <div class="fin-modal-backdrop">
                        <div class="fin-modal" role="dialog" aria-modal="true">
                            <div class="fin-modal-head">
                                <div>
                                    <div class="card-title">{title}</div>
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
                                {body}
                            </div>
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

fn render_new_txn_form(add: ServerAction<AddTxn>, data: &LedgerData) -> impl IntoView {
    let locale = use_locale();
    let categories = data.categories.clone();
    let accounts = data.accounts.clone();
    let currency_code = data.currency.code.clone();
    let decimals = data.currency.decimals;
    let amount_label = tf(
        locale,
        "finance.field.amount_in",
        &[("symbol", &data.currency.symbol)],
    );
    let step = amount_step(decimals);
    view! {
        <ActionForm action=add attr:class="vstack fin-op-form" attr:style="gap:12px">
            <input type="hidden" name="currency_code" value=currency_code/>
            // Row 1 — money: amount + expense/income toggle. This is the
            // single most consequential field, so it's prominent + first.
            <div style="display:grid;grid-template-columns:1fr 1fr;gap:10px">
                <label class="vstack" style="gap:4px">
                    <span class="mono dim" style=FIELD_LABEL>{amount_label}</span>
                    <input name="amount" type="number" step=step.clone() min=step required
                           autofocus
                           placeholder="42.00" style=INPUT_STYLE_MONO/>
                </label>
                <label class="vstack" style="gap:4px">
                    <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.tag")}</span>
                    <select name="tag" style=INPUT_STYLE>
                        <option value=Tag::Exp.as_str() selected="selected">{t(locale, "finance.tag.short_exp")}</option>
                        <option value=Tag::Inc.as_str()>{t(locale, "finance.tag.short_inc")}</option>
                    </select>
                </label>
            </div>
            // Row 2 — what / where.
            <label class="vstack" style="gap:4px">
                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.merchant_desc")}</span>
                <input id="fin-new-merchant" name="merchant" required
                       maxlength=MAX_TXN_MERCHANT_CHARS.to_string()
                       placeholder=t(locale, "finance.placeholder.merchant") style=INPUT_STYLE/>
            </label>
            // Row 3 — categorisation: category + account side-by-side.
            <div style="display:grid;grid-template-columns:1fr 1fr;gap:10px">
                <label class="vstack" style="gap:4px">
                    <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.category")}</span>
                    <select name="category_code" required style=INPUT_STYLE>
                        {categories.into_iter().enumerate().map(|(i, c)| {
                            let code = c.code.clone();
                            let label = category_display(&c);
                            view! { <option value=code selected={i == 0}>{label}</option> }
                        }).collect_view()}
                    </select>
                </label>
                <label class="vstack" style="gap:4px">
                    <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.account")}</span>
                    <select name="account_code" required style=INPUT_STYLE>
                        {accounts.into_iter().enumerate().map(|(i, a)| {
                            let code = a.code.clone();
                            view! { <option value=code selected={i == 0}>{a.name.clone()}</option> }
                        }).collect_view()}
                    </select>
                </label>
            </div>
            // Row 4 — note + date.
            <div style="display:grid;grid-template-columns:2fr 1fr;gap:10px">
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
            </div>
            // Row 5 — error + actions, right-aligned.
            <div class="hstack" style="gap:8px;align-items:center;justify-content:flex-end;flex-wrap:wrap">
                <ErrorSlot action=add style="flex:1"/>
                <button class="btn primary" type="submit">
                    <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.modal.submit_new")}
                </button>
            </div>
        </ActionForm>
    }
}

/// Transfer card; submits to `add_transfer` (writes the paired ±amount rows).
/// The account pickers span every currency — a transfer may cross currencies,
/// in which case the user enters the out-amount and in-amount separately
/// (there is no conversion). Option values are `"{currency}/{code}"`.
fn render_transfer_form(
    add_transfer: ServerAction<AddTransfer>,
    data: &LedgerData,
) -> impl IntoView {
    let locale = use_locale();
    // Map currency code → symbol so each account option can show its currency.
    let symbol_of = |code: &str| {
        data.currencies
            .iter()
            .find(|c| c.code == code)
            .map(|c| c.symbol.clone())
            .unwrap_or_default()
    };
    let opts: Vec<(String, String)> = data
        .transfer_accounts
        .iter()
        .map(|a| {
            let value = format!("{}/{}", a.currency_code, a.code);
            let label = format!(
                "{} · {} {}",
                a.name,
                symbol_of(&a.currency_code),
                a.currency_code
            );
            (value, label)
        })
        .collect();
    let from_opts = opts.clone();
    let to_opts = opts;
    view! {
            <ActionForm action=add_transfer attr:class="vstack fin-op-form" attr:style="gap:10px">
                <div style="display:grid;grid-template-columns:1fr 1fr;gap:10px">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.from_account")}</span>
                        <select name="from_account" style=INPUT_STYLE>
                            {from_opts.into_iter().enumerate().map(|(i, (value, label))| {
                                view! { <option value=value selected={i == 0}>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.to_account")}</span>
                        <select name="to_account" style=INPUT_STYLE>
                            {to_opts.into_iter().enumerate().map(|(i, (value, label))| {
                                view! { <option value=value selected={i == 1}>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                </div>
                <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:10px">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.transfer_out")}</span>
                        <input name="from_amount" type="number" step="any" min="0" required
                               placeholder="500.00" style=INPUT_STYLE_MONO/>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.transfer_in")}</span>
                        <input name="to_amount" type="number" step="any" min="0" required
                               placeholder="500.00" style=INPUT_STYLE_MONO/>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.date_optional")}</span>
                        <input name="occurred_at" type="date" style=INPUT_STYLE_MONO
                               title=t(locale, "finance.placeholder.date_now")/>
                    </label>
                </div>
                <p class="mono dim" style="font-size:10.5px;margin:-2px 0 2px">
                    {t(locale, "finance.card.transfer.hint")}
                </p>
                <div style="display:grid;grid-template-columns:3fr auto auto;gap:10px;align-items:end">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.note_optional")}</span>
                        <input name="note" maxlength=MAX_TXN_NOTE_CHARS.to_string()
                               placeholder=t(locale, "finance.placeholder.transfer_note") style=INPUT_STYLE/>
                    </label>
                    <ErrorSlot action=add_transfer style="align-self:center"/>
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
    filters: LedgerFilters,
) -> impl IntoView {
    let locale = use_locale();
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.clone();
    // The reactive tbody closure (a `<Card>` child) captures its own symbol
    // clone so the sibling category-share card can still use `symbol`.
    let row_symbol = symbol.clone();
    let txns = d.txns.clone();
    let cat_summary = d.category_summary.clone();
    // Share category/account vectors via `Arc` so each `render_txn_row`
    // does a single ref-count bump instead of cloning the entire vec for
    // every row in the table.
    let cat_options: std::sync::Arc<Vec<Category>> = std::sync::Arc::new(d.categories.clone());
    let acc_options: std::sync::Arc<Vec<Account>> = std::sync::Arc::new(d.accounts.clone());
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
    let export_href = csv_data_uri(&txns, decimals);
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
                <div class="ledger-toolbar">
                    <div class="hstack ledger-filters" style="gap:8px;flex-wrap:wrap">
                        <input type="text" placeholder=t(locale, "finance.filter.search")
                               prop:value=move || filters.merchant.get()
                               on:input=move |ev| filters.merchant.set(event_target_value(&ev))
                               style=format!("flex:1 1 200px;min-width:140px;{}", INPUT_STYLE)/>
                        <select prop:value=move || filters.category.get()
                                on:change=move |ev| filters.category.set(event_target_value(&ev))
                                style=format!("min-width:140px;{}", INPUT_STYLE)>
                            <option value="">{t(locale, "finance.filter.all_categories")}</option>
                            {cat_options.iter().map(|c| {
                                let code = c.code.clone();
                                let label = category_display(c);
                                view! { <option value=code>{label}</option> }
                            }).collect_view()}
                        </select>
                        <input type="date"
                               prop:value=move || filters.date_from.get()
                               on:input=move |ev| filters.date_from.set(event_target_value(&ev))
                               style=format!("width:130px;{}", INPUT_STYLE_MONO)
                               title=t(locale, "finance.filter.date_from")/>
                        <input type="date"
                               prop:value=move || filters.date_to.get()
                               on:input=move |ev| filters.date_to.set(event_target_value(&ev))
                               style=format!("width:130px;{}", INPUT_STYLE_MONO)
                               title=t(locale, "finance.filter.date_to")/>
                        <span style="flex:1"></span>
                        <a class="btn" download="finance-export.csv" href=export_href>
                            <Icon kind=IconKind::Export size=14/>{t(locale, "finance.action.export")}
                        </a>
                    </div>
                </div>
                <div class="scroll-x">
                    <table class="tbl">
                        <thead>
                            <tr>
                                <th style="width:84px">{t(locale, "finance.field.date")}</th>
                                <th>{t(locale, "finance.field.merchant_desc")}</th>
                                <th style="width:120px">{t(locale, "finance.field.category")}</th>
                                <th style="width:120px">{t(locale, "finance.field.account")}</th>
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
                                txns.iter()
                                    .filter(|t| {
                                        (mq.is_empty() || t.merchant.to_lowercase().contains(&mq))
                                        && (cq.is_empty() || t.category_code == cq)
                                        && from_ts.map(|f| t.occurred_at >= f).unwrap_or(true)
                                        && to_ts.map(|to| t.occurred_at <= to).unwrap_or(true)
                                    })
                                    .cloned()
                                    .map(|t| render_txn_row(t, TxnRowContext {
                                        cat_lookup,
                                        cat_options: cat_options.clone(),
                                        acc_options: acc_options.clone(),
                                        delete,
                                        update_txn,
                                        decimals,
                                        symbol: row_symbol.clone(),
                                    }))
                                    .collect_view()
                            }}
                        </tbody>
                    </table>
                </div>
            </Card>

            <div class="vstack" style="gap:20px">
                <Card title=t(locale, "finance.card.category_share.title") code="FIN-R02" sub=t(locale, "finance.card.category_share.sub")>
                    {if cat_summary.is_empty() {
                        view! {
                            <EmptyState
                                icon=IconKind::Coin
                                title=t(locale, "finance.card.category_share.title")
                                desc=t(locale, "finance.card.category_share.empty")
                                code="FIN-R02-EMPTY"
                                compact=true
                            />
                        }.into_any()
                    } else {
                        render_category_share_rows(cat_summary, &symbol, decimals).into_any()
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

/// Per-row budget delete: "delete" here means "set this period's budget
/// for the category to zero", so we route through the `set_budget`
/// action with four hidden fields. Shape mirrors `RowDeleteAction` but
/// can't reuse it because that helper ships exactly one hidden field.
fn render_budget_delete(
    action: ServerAction<SetBudget>,
    currency_code: String,
    period: String,
    category_code: String,
    confirm_text: String,
) -> impl IntoView {
    let locale = use_locale();
    let label = t(locale, "finance.action.delete").to_string();
    let confirm_label = t(locale, "finance.action.confirm_delete").to_string();
    let cancel_label = t(locale, "finance.confirm.cancel").to_string();
    let open = RwSignal::new(false);
    let currency_inner = currency_code.clone();
    let period_inner = period.clone();
    let category_inner = category_code.clone();
    let confirm_inner = confirm_text.clone();
    let confirm_label_inner = confirm_label.clone();
    let cancel_label_inner = cancel_label.clone();
    view! {
        <span class="row-actions-slot">
            <button class="btn sm danger" type="button"
                    on:click=move |_| open.set(true)>{label}</button>
            {move || {
                if !open.get() {
                    return view! { <span></span> }.into_any();
                }
                let currency = currency_inner.clone();
                let period = period_inner.clone();
                let category = category_inner.clone();
                let confirm_text = confirm_inner.clone();
                let confirm_label = confirm_label_inner.clone();
                let cancel_label = cancel_label_inner.clone();
                view! {
                    <div class="fin-modal-backdrop confirm-backdrop"
                         on:click=move |_| open.set(false)>
                        <div class="fin-modal confirm-modal" role="alertdialog" aria-modal="true"
                             on:click=move |e| e.stop_propagation()>
                            <div class="confirm-body">
                                <div class="confirm-icon danger">
                                    <Icon kind=IconKind::Close size=18/>
                                </div>
                                <div class="confirm-text">
                                    <div class="confirm-title">{confirm_text}</div>
                                </div>
                            </div>
                            <ActionForm action=action attr:class="confirm-foot">
                                <input type="hidden" name="currency_code" value=currency/>
                                <input type="hidden" name="period" value=period/>
                                <input type="hidden" name="category_code" value=category/>
                                <input type="hidden" name="amount" value="0"/>
                                <button class="btn ghost" type="button"
                                        on:click=move |_| open.set(false)>{cancel_label}</button>
                                <button class="btn primary danger-action" type="submit"
                                        on:click=move |_| open.set(false)>{confirm_label}</button>
                            </ActionForm>
                        </div>
                    </div>
                }.into_any()
            }}
        </span>
    }
}

fn render_suggestions(items: Vec<crate::suggestions::Suggestion>) -> impl IntoView {
    let locale = use_locale();
    if items.is_empty() {
        return view! {
            <EmptyState
                icon=IconKind::Sparkle
                title=t(locale, "finance.card.suggestions.title")
                desc=t(locale, "finance.card.suggestions.empty")
                code="FIN-RUL-EMPTY"
                compact=true
            />
        }
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

fn render_txn_row(t: Txn, ctx: TxnRowContext<'_>) -> impl IntoView {
    let locale = use_locale();
    let date = fmt_ts_md(Some(t.occurred_at));
    let time_ = fmt_ts_hm(Some(t.occurred_at));
    let cls_amt = if t.amount > 0 {
        "num amt-pos"
    } else {
        "num amt-neg"
    };
    let txind = match Tag::parse(&t.tag) {
        Some(Tag::Inc) => "txind inc",
        Some(Tag::Tfr) => "txind tfr",
        _ => "txind exp",
    };
    let amount_text = if t.amount > 0 {
        format!("+{}{}", ctx.symbol, fmt_minor(t.amount, ctx.decimals))
    } else {
        format!("−{}{}", ctx.symbol, fmt_minor(t.amount.abs(), ctx.decimals))
    };
    let is_tfr = matches!(Tag::parse(&t.tag), Some(Tag::Tfr));
    let cat_tone = ctx
        .cat_lookup
        .get(&t.category_code)
        .map(|c| Tone::parse(&c.tone))
        .unwrap_or(Tone::None);
    // Resolve the human-readable name from the lookup; fall back to the
    // raw code only when the category was deleted but transactions still
    // reference it (the UI never shows the bare code by design).
    let cat_label = ctx
        .cat_lookup
        .get(&t.category_code)
        .map(category_display)
        .unwrap_or_else(|| t.category_code.clone());
    let account_label = ctx
        .acc_options
        .iter()
        .find(|a| a.code == t.account_code)
        .map(|a| a.name.clone())
        .unwrap_or_else(|| t.account_code.clone());
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

    let action_cell = if is_tfr {
        view! {
            <span class="dim mono"
                  style="font-size:10.5px"
                  title=ep_i18n::t(locale, "finance.title.tfr_not_editable")>"——"</span>
            <RowDeleteAction action=ctx.delete value=doc_id.clone()
                             confirm=ep_i18n::t(locale, "finance.confirm.delete_transfer")/>
        }
        .into_any()
    } else {
        view! {
            <button class="btn sm" type="button"
                    on:click=move |_| editing.set(true)>
                {ep_i18n::t(locale, "finance.action.edit")}
            </button>
            <RowDeleteAction action=ctx.delete value=doc_id.clone()
                             confirm=ep_i18n::t(locale, "finance.confirm.delete_txn")/>
        }
        .into_any()
    };

    let edit_form = render_txn_edit_form(
        &t,
        editing,
        is_tfr,
        ctx.cat_options,
        ctx.acc_options,
        ctx.update_txn,
        ctx.decimals,
    );

    view! {
        <tr>
            <td class="mono dim">{date}<div style="font-size:10px;color:var(--ink-4)">{time_}</div></td>
            <td>
                <span class=txind></span>
                {t.merchant.clone()}
            </td>
            <td>{cat_cell}</td>
            <td class="dim">{account_label}</td>
            <td class=cls_amt>{amount_text}</td>
            <td class="num">
                <div class="hstack" style="gap:4px;justify-content:flex-end;align-items:center">
                    {action_cell}
                </div>
            </td>
        </tr>
        <tr class="edit-row" style:display=move || if editing.get() { "table-row" } else { "none" }>
            <td colspan="6" style="padding:0;border:0">
                {edit_form}
            </td>
        </tr>
    }
}

/// The per-row inline edit modal for a non-transfer transaction. Lifted out of
/// `render_txn_row` to keep that function scannable; it returns the exact same
/// `<div class="fin-modal-slot">` subtree that was built inline before, so the
/// DOM — and the hydrate text-node anchor — is unchanged. Transfer rows render
/// an empty placeholder: they are edited by delete + recreate, not in place.
fn render_txn_edit_form(
    t: &Txn,
    editing: RwSignal<bool>,
    is_tfr: bool,
    cat_options: std::sync::Arc<Vec<Category>>,
    acc_options: std::sync::Arc<Vec<Account>>,
    update_txn: ServerAction<UpdateTxn>,
    decimals: u8,
) -> impl IntoView {
    let locale = use_locale();
    if is_tfr {
        return view! { <span></span> }.into_any();
    }
    let edit_doc_id = t.doc_id.clone();
    let edit_merchant = t.merchant.clone();
    let edit_amount_str = fmt_minor_raw(t.amount.abs(), decimals);
    let edit_account = t.account_code.clone();
    let edit_category = t.category_code.clone();
    let edit_note = t.note.clone().unwrap_or_default();
    let edit_date = fmt_ts_ymd(Some(t.occurred_at));
    let edit_sub_merchant = t.merchant.clone();
    let step = amount_step(decimals);
    let cat_opts_active = cat_options;
    let acc_opts_active = acc_options;
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
                    let sub_merchant = edit_sub_merchant.clone();
                    let form_step = step.clone();
                    let form_cat_opts = cat_opts_active.clone();
                    let form_acc_opts = acc_opts_active.clone();
                    view! {
                        <div class="fin-modal-backdrop">
                            <div class="fin-modal" role="dialog" aria-modal="true">
                                <div class="fin-modal-head">
                                    <div>
                                        <div class="card-title">{ep_i18n::t(locale, "finance.action.edit")}</div>
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
                                                <input name="amount" type="number" step=form_step.clone() min=form_step
                                                       value=form_amount style=INPUT_STYLE_MONO/>
                                            </label>
                                        </div>
                                        <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:10px">
                                            <label class="vstack" style="gap:4px">
                                                <span class="mono dim" style=FIELD_LABEL>{ep_i18n::t(locale, "finance.field.category")}</span>
                                                <select name="category_code" style=INPUT_STYLE>
                                                    {form_cat_opts.iter().map(|c| {
                                                        let selected = c.code == form_category.as_str();
                                                        let code = c.code.clone();
                                                        let label = category_display(c);
                                                        view! { <option value=code selected=selected>{label}</option> }
                                                    }).collect_view()}
                                                </select>
                                            </label>
                                            <label class="vstack" style="gap:4px">
                                                <span class="mono dim" style=FIELD_LABEL>{ep_i18n::t(locale, "finance.field.account")}</span>
                                                <select name="account_code" style=INPUT_STYLE>
                                                    {form_acc_opts.iter().map(|a| {
                                                        let selected = a.code == form_account.as_str();
                                                        let code = a.code.clone();
                                                        view! { <option value=code selected=selected>{a.name.clone()}</option> }
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
                                            <ErrorSlot action=update_txn/>
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
    }
    .into_any()
}

/// One budget row in the FIN-BDG-01 pool card — category name, used/total bar,
/// inline amount-edit form, and the per-row delete affordance. Lifted out of
/// `render_budget`'s per-budget `map` closure to keep that function's `view!`
/// body readable; the emitted `<div>` subtree is unchanged.
fn render_budget_row(
    b: crate::model::BudgetEntry,
    currency_code: &str,
    period: &str,
    decimals: u8,
    symbol: &str,
    cat_lookup: &std::collections::HashMap<String, (String, String)>,
    set_budget: ServerAction<SetBudget>,
) -> impl IntoView {
    let locale = use_locale();
    let (name, tone) = cat_lookup
        .get(&b.category_code)
        .cloned()
        .unwrap_or_else(|| (b.category_code.clone(), String::new()));
    let pct_f = if b.amount.is_positive() {
        b.used.to_f64() / b.amount.to_f64() * 100.0
    } else {
        0.0
    };
    let pct = pct_f.round() as i32;
    let bar_color = if pct > 95 {
        "var(--rose)"
    } else if pct > 80 {
        "var(--amber)"
    } else {
        Tone::parse(&tone).css_var()
    };
    let pct_class = if pct > 100 { "amt-neg" } else { "dim" };
    let bar_width = (pct as i64).clamp(0, 100);
    let edit_currency = currency_code.to_string();
    let edit_period = period.to_string();
    let edit_category = b.category_code.clone();
    let delete_currency = currency_code.to_string();
    let delete_period = period.to_string();
    let delete_category = b.category_code.clone();
    let row_amount = fmt_minor_raw(b.amount, decimals);
    let used_total = format!(
        "{s}{} / {s}{} · ",
        fmt_minor_compact(b.used, decimals),
        fmt_minor_compact(b.amount, decimals),
        s = symbol
    );
    let step = amount_step(decimals);
    let row_action = set_budget;
    let delete_action = set_budget;
    let delete_confirm = tf(locale, "finance.budget.confirm_delete", &[("name", &name)]);
    view! {
        <div>
            <div style="display:flex;justify-content:space-between;align-items:baseline;margin-bottom:4px">
                <div style="font-size:13px">
                    <span style="font-weight:500">{name}</span>
                </div>
                <div class="mono" style="font-size:12px">
                    {used_total}
                    <span class=pct_class>{format!("{}%", pct)}</span>
                </div>
            </div>
            <div class="bar thick"><span style=format!("width:{}%;background:{}", bar_width, bar_color)></span></div>
            <div class="hstack" style="gap:8px;margin-top:8px;justify-content:flex-end;flex-wrap:wrap">
                <ActionForm action=row_action attr:class="hstack" attr:style="gap:6px;align-items:center">
                    <input type="hidden" name="currency_code" value=edit_currency/>
                    <input type="hidden" name="period" value=edit_period/>
                    <input type="hidden" name="category_code" value=edit_category/>
                    <input name="amount" type="number" step=step min="0"
                           value=row_amount
                           style=format!("width:110px;{}", INPUT_STYLE_MONO)/>
                    <button class="btn sm" type="submit">
                        <Icon kind=IconKind::Check size=12/>{t(locale, "finance.action.save")}
                    </button>
                </ActionForm>
                {render_budget_delete(
                    delete_action,
                    delete_currency,
                    delete_period,
                    delete_category,
                    delete_confirm,
                )}
            </div>
        </div>
    }
}

fn render_budget(
    d: &LedgerData,
    set_budget: ServerAction<SetBudget>,
    import_budgets: ServerAction<ImportBudgetsFrom>,
) -> impl IntoView {
    let locale = use_locale();
    let m = &d.month;
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.clone();
    let currency_code = d.currency.code.clone();
    let period = m.period.clone();
    let categories_for_form = d.categories.clone();
    // Owned-string lookup so the closures below don't capture a borrow into
    // a Vec the view! macro will move.
    let cat_lookup: std::collections::HashMap<String, (String, String)> = d
        .categories
        .iter()
        .map(|c| (c.code.clone(), (category_display(c), c.tone.clone())))
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
                ("symbol", &symbol),
                ("count", &budgets_count.to_string()),
                ("used", &fmt_minor_compact(m.expense, decimals)),
                ("total", &fmt_minor_compact(m.budget_total, decimals)),
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
    let editor_currency = currency_code.clone();
    let editor_period = period.clone();
    let import_currency = currency_code.clone();
    let row_currency = currency_code.clone();
    let unbudgeted_symbol = symbol.clone();
    let planner_symbol = symbol.clone();
    // Distinct clone for the budget-row map (a `<Card>` child); `symbol`
    // itself is still needed by the editor card's amount label below.
    let row_symbol = symbol.clone();
    let step = amount_step(decimals);

    view! {
        <div class="grid-2">
            <Card title=pool_title code="FIN-BDG-01" sub=pool_sub>
                {if budgets.is_empty() {
                    view! {
                        <div class="vstack" style="gap:10px">
                            <p class="muted">{empty_period_hint}</p>
                            <div class="hstack" style="gap:8px">
                                <ActionForm action=import_budgets attr:style="display:inline">
                                    <input type="hidden" name="currency_code" value=import_currency.clone()/>
                                    <input type="hidden" name="source_period" value=import_source.clone()/>
                                    <input type="hidden" name="target_period" value=import_target.clone()/>
                                    <button class="btn primary" type="submit">
                                        <Icon kind=IconKind::Upload size=14/>
                                        {import_button_label}
                                    </button>
                                </ActionForm>
                                <ErrorSlot action=import_budgets/>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="vstack" style="gap:14px">
                            {budgets.into_iter()
                                .map(|b| render_budget_row(b, &row_currency, &period, decimals, &row_symbol, &cat_lookup, set_budget))
                                .collect_view()}
                            {if unbudgeted.is_empty() {
                                view! { <span></span> }.into_any()
                            } else {
                                view! {
                                    <div style="margin-top:6px;padding-top:10px;border-top:1px dashed var(--border)">
                                        <div class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">{t(locale, "finance.budget.unbudgeted")}</div>
                                        {unbudgeted.into_iter().map(|c| {
                                            let amount = format!("{}{}", unbudgeted_symbol, fmt_minor_compact(c.value, decimals));
                                            view! {
                                                <div style="display:flex;justify-content:space-between;font-size:12.5px;padding:4px 0">
                                                    <span>{category_summary_display(&c)}</span>
                                                    <span class="mono">{amount}</span>
                                                </div>
                                            }
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
                        <input type="hidden" name="currency_code" value=editor_currency/>
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
                                        let label = category_display(&c);
                                        view! { <option value=code>{label}</option> }
                                    }).collect_view()}
                                </select>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{tf(locale, "finance.field.amount_in", &[("symbol", &symbol)])}</span>
                                <input name="amount" type="number" step=step min="0"
                                       placeholder="3200" style=INPUT_STYLE_MONO/>
                            </label>
                            <button class="btn primary" type="submit">
                                <Icon kind=IconKind::Check size=14/>{t(locale, "finance.action.save")}
                            </button>
                        </div>
                        <ErrorSlot action=set_budget/>
                    </ActionForm>
                </Card>

                <Card title=t(locale, "finance.budget.card.next_title") code="FIN-BDG-02" sub=next_month_sub>
                    {if next_month_planner.is_empty() {
                        view! { <p class="muted">{t(locale, "finance.budget.card.next_empty")}</p> }.into_any()
                    } else {
                        view! {
                            <div class="vstack" style="gap:10px">
                                {next_month_planner.into_iter().map(|(name, code, suggested)| {
                                    let amount = format!("{}{}", planner_symbol, fmt_minor_compact(suggested, decimals));
                                    view! {
                                        <div style="display:flex;justify-content:space-between;align-items:baseline;font-size:13px">
                                            <span>
                                                {name}
                                                <span class="mono dim" style="margin-left:6px;font-size:10.5px">{code}</span>
                                            </span>
                                            <span class="mono">{amount}</span>
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
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.clone();
    let currency_code = d.currency.code.clone();
    let pairs: Vec<(Account, AccountStats)> = d
        .accounts
        .iter()
        .cloned()
        .zip(d.account_stats.iter().cloned())
        .collect();
    view! {
        {render_account_manager(create_account, currency_code.clone(), decimals)}
        <div class="grid-3" style="margin-top:20px">
            {pairs.into_iter().map(|(a, s)| {
                render_account_card(a, s, update_account, delete_account, decimals, symbol.clone())
            }).collect_view()}
        </div>
    }
}

fn render_account_manager(
    create_account: ServerAction<CreateAccount>,
    currency_code: String,
    decimals: u8,
) -> impl IntoView {
    let locale = use_locale();
    let open = RwSignal::new(false);
    let toggle = move |_| open.update(|v| *v = !*v);
    let last_v = RwSignal::new(0usize);
    Effect::new(move |_| {
        let v = create_account.version().get();
        if v != 0
            && v != last_v.get_untracked()
            && matches!(create_account.value().get(), Some(Ok(_)))
        {
            open.set(false);
            last_v.set(v);
        }
    });
    let step = amount_step(decimals);
    view! {
        <Card title=t(locale, "finance.account.manager.title") code="FIN-ACC-MGR" sub=t(locale, "finance.account.manager.sub")>
            <div class="hstack" style="margin-bottom:10px">
                <button class="btn primary" type="button" on:click=toggle>
                    <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.account.new")}
                </button>
            </div>
            {move || if open.get() {
                // Per-render clones — the `<ActionForm>` children closure
                // moves whatever it captures, so the outer reactive closure
                // must hand it fresh values to stay `FnMut`.
                let currency_code = currency_code.clone();
                let step = step.clone();
                view! {
                    <ActionForm action=create_account attr:class="vstack" attr:style="gap:10px;padding:14px;background:var(--bg-2);border:1px solid var(--border);border-radius:8px">
                        <input type="hidden" name="currency_code" value=currency_code.clone()/>
                        <input type="hidden" name="code" value=""/>
                        <input type="hidden" name="tone" value=""/>
                        <div style="display:grid;grid-template-columns:2fr 1fr 1fr;gap:10px">
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.name")}</span>
                                <input name="name" required autofocus
                                       maxlength=MAX_ACCOUNT_NAME_CHARS.to_string()
                                       placeholder=t(locale, "finance.placeholder.account_name")
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
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.opening_balance")}</span>
                                <input name="opening_balance" type="number" step=step value="0"
                                       style=INPUT_STYLE_MONO/>
                            </label>
                        </div>
                        <div class="hstack" style="gap:10px;align-items:center;justify-content:flex-end;flex-wrap:wrap">
                            <ErrorSlot action=create_account style="flex:1"/>
                            <button class="btn ghost" type="button"
                                    on:click=move |_| open.set(false)>{t(locale, "finance.action.cancel")}</button>
                            <button class="btn primary" type="submit">
                                <Icon kind=IconKind::Check size=14/>{t(locale, "finance.action.create")}
                            </button>
                        </div>
                    </ActionForm>
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }}
        </Card>
    }
}

/// Single account card with its inline edit form + delete button.
fn render_account_card(
    a: Account,
    s: AccountStats,
    update_account: ServerAction<UpdateAccount>,
    delete_account: ServerAction<DeleteAccount>,
    decimals: u8,
    symbol: String,
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
    let currency_code = a.currency_code.clone();
    let code = a.code.clone();
    let name = a.name.clone();
    let type_str = a.r#type.clone();
    let tone_str = a.tone.clone();
    let card_title = a.name.clone();
    let card_sub = a.r#type.clone();
    let balance_text = format!("{}{}", symbol, fmt_minor(a.balance, decimals));
    let delete_ref = format!("{}/{}", a.currency_code, a.code);
    // ChartBars takes f64 heights; accounting amounts stay exact elsewhere.
    let history: Vec<f64> = s.history_14d.iter().map(|v| v.to_f64()).collect();
    let confirm_msg = tf(
        locale,
        "finance.account.confirm_delete",
        &[("name", &a.name)],
    );
    view! {
        <Card title=card_title sub=card_sub>
            <div class="mono" style="font-size:24px;font-weight:600;letter-spacing:-0.02em">
                {balance_text}
            </div>
            <div class="hstack" style="margin-top:10px;gap:10px">
                <UiTag tone=tone>{a.r#type.clone()}</UiTag>
                <span class="mono dim" style="font-size:10.5px">{last_seen}</span>
            </div>
            <div style="margin-top:14px">
                <ChartBars data=history/>
            </div>
            <div class="hstack" style="margin-top:12px;gap:6px;flex-wrap:wrap">
                <details style="flex:1;min-width:0">
                    <summary class="btn ghost" style="cursor:pointer;display:inline-flex;align-items:center;gap:4px">
                        <Icon kind=IconKind::Settings size=12/>{t(locale, "finance.action.edit")}
                    </summary>
                    <ActionForm action=update_account attr:class="vstack" attr:style="gap:8px;margin-top:8px">
                        <input type="hidden" name="currency_code" value=currency_code/>
                        <input type="hidden" name="code" value=code.clone()/>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.name")}</span>
                            <input name="name" required maxlength=MAX_ACCOUNT_NAME_CHARS.to_string() value=name
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
                            <ErrorSlot action=update_account/>
                        </div>
                    </ActionForm>
                </details>
                <RowDeleteAction action=delete_account value=delete_ref field="account_ref"
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
    let currency_code = d.currency.code.clone();
    // Sort by sort_order then code so the management table matches what the
    // dropdown shows. Cloning is fine — the categories vec is small (≤ ~20).
    let mut cats = d.categories.clone();
    cats.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.code.cmp(&b.code)));
    let usage = d.category_usage.clone();
    let next_sort = cats.iter().map(|c| c.sort_order).max().unwrap_or(0) + 1;
    let open = RwSignal::new(false);
    let toggle = move |_| open.update(|v| *v = !*v);
    let last_v = RwSignal::new(0usize);
    Effect::new(move |_| {
        let v = create_category.version().get();
        if v != 0
            && v != last_v.get_untracked()
            && matches!(create_category.value().get(), Some(Ok(_)))
        {
            open.set(false);
            last_v.set(v);
        }
    });
    view! {
        <Card title=t(locale, "finance.category.manager.title") code="FIN-CAT-MGR" sub=t(locale, "finance.category.manager.sub")>
            <div class="hstack" style="margin-bottom:10px">
                <button class="btn primary" type="button" on:click=toggle>
                    <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.category.new")}
                </button>
            </div>
            <div class="fin-modal-slot">
                {move || if open.get() {
                    // Per-render clone — the `<ActionForm>` children closure moves
                    // what it captures; the outer reactive closure stays `FnMut`.
                    let currency_code = currency_code.clone();
                    view! {
                        <div class="fin-modal-backdrop">
                            <div class="fin-modal" role="dialog" aria-modal="true">
                                <div class="fin-modal-head">
                                    <div>
                                        <div class="card-title">{t(locale, "finance.category.new")}</div>
                                        <p class="card-sub">{t(locale, "finance.category.manager.sub")}</p>
                                    </div>
                                    <button class="btn ghost sm" type="button"
                                            aria-label=t(locale, "finance.action.cancel")
                                            on:click=move |_| open.set(false)>{t(locale, "finance.action.cancel")}</button>
                                </div>
                                <div class="fin-modal-body">
                                    <ActionForm action=create_category attr:class="vstack" attr:style="gap:12px">
                                        <input type="hidden" name="currency_code" value=currency_code.clone()/>
                                        <input type="hidden" name="code" value=""/>
                                        <input type="hidden" name="tone" value=""/>
                                        <input type="hidden" name="sort_order" value=next_sort.to_string()/>
                                        <div style="display:grid;grid-template-columns:96px 1fr;gap:10px">
                                            <label class="vstack" style="gap:4px">
                                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.icon")}</span>
                                                <input name="icon" autofocus
                                                       maxlength=MAX_CATEGORY_ICON_CHARS.to_string()
                                                       placeholder=t(locale, "finance.placeholder.category_icon")
                                                       style=INPUT_STYLE/>
                                            </label>
                                            <label class="vstack" style="gap:4px">
                                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.name")}</span>
                                                <input name="name" required
                                                       maxlength=MAX_CATEGORY_NAME_CHARS.to_string()
                                                       placeholder=t(locale, "finance.placeholder.category_name")
                                                       style=INPUT_STYLE/>
                                            </label>
                                        </div>
                                        <div class="hstack" style="gap:10px;align-items:center;justify-content:flex-end;flex-wrap:wrap">
                                            <ErrorSlot action=create_category style="flex:1"/>
                                            <button class="btn ghost" type="button"
                                                    on:click=move |_| open.set(false)>{t(locale, "finance.action.cancel")}</button>
                                            <button class="btn primary" type="submit">
                                                <Icon kind=IconKind::Check size=14/>{t(locale, "finance.action.create")}
                                            </button>
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

            <div class="scroll-x" style="margin-top:14px">
                <table class="tbl">
                    <thead>
                        <tr>
                            <th style="width:56px">{t(locale, "finance.field.icon")}</th>
                            <th>{t(locale, "finance.field.name")}</th>
                            <th style="width:80px">{t(locale, "finance.field.tone")}</th>
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
        &[("name", &c.name)],
    );
    let currency_code = c.currency_code.clone();
    let code = c.code.clone();
    let name = c.name.clone();
    let icon = c.icon.clone();
    let tone_str = c.tone.clone();
    let sort_order_str = c.sort_order.to_string();
    let display_name = c.name.clone();
    let display_icon = if c.icon.trim().is_empty() {
        "—".to_string()
    } else {
        c.icon.clone()
    };
    let delete_ref = format!("{}/{}", c.currency_code, c.code);
    let display_tone_label = if c.tone.is_empty() {
        "—".to_string()
    } else {
        c.tone.clone()
    };
    view! {
        <tr>
            <td style="font-size:17px">{display_icon}</td>
            <td>{display_name}</td>
            <td>
                <UiTag tone=tone_enum>{display_tone_label}</UiTag>
            </td>
            <td class="num mono">{usage_count}</td>
            <td class="num">
                <div class="hstack" style="gap:6px;justify-content:flex-end;flex-wrap:wrap">
                    <details>
                        <summary class="btn ghost" style="cursor:pointer;display:inline-flex;align-items:center;gap:4px">
                            <Icon kind=IconKind::Settings size=12/>{t(locale, "finance.action.edit")}
                        </summary>
                        <ActionForm action=update_category attr:class="vstack" attr:style="gap:8px;margin-top:8px;min-width:240px">
                            <input type="hidden" name="currency_code" value=currency_code/>
                            <input type="hidden" name="code" value=code.clone()/>
                            <input type="hidden" name="sort_order" value=sort_order_str/>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.icon")}</span>
                                <input name="icon" maxlength=MAX_CATEGORY_ICON_CHARS.to_string() value=icon
                                       placeholder=t(locale, "finance.placeholder.category_icon")
                                       style=INPUT_STYLE/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.name")}</span>
                                <input name="name" required maxlength=MAX_CATEGORY_NAME_CHARS.to_string() value=name
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
                            <div class="hstack" style="gap:8px;align-items:center">
                                <button class="btn primary" type="submit">
                                    <Icon kind=IconKind::Check size=12/>{t(locale, "finance.action.save")}
                                </button>
                                <ErrorSlot action=update_category/>
                            </div>
                        </ActionForm>
                    </details>
                    <RowDeleteAction action=delete_category value=delete_ref field="category_ref"
                                     confirm=confirm_msg label=t(locale, "finance.action.delete")/>
                </div>
            </td>
        </tr>
    }
}

/// The currency management tab — CRUD over `fin_currency`. Each currency is an
/// isolated finance "page"; this tab is where they're created, renamed, given
/// a custom symbol / precision, and promoted to primary.
fn render_currencies(
    d: &LedgerData,
    create_currency: ServerAction<CreateCurrency>,
    update_currency: ServerAction<UpdateCurrency>,
    delete_currency: ServerAction<DeleteCurrency>,
    set_primary_currency: ServerAction<SetPrimaryCurrency>,
) -> impl IntoView {
    let locale = use_locale();
    let currencies = d.currencies.clone();
    let next_sort = currencies.iter().map(|c| c.sort_order).max().unwrap_or(0) + 1;
    let open = RwSignal::new(false);
    let toggle = move |_| open.update(|v| *v = !*v);
    let last_v = RwSignal::new(0usize);
    Effect::new(move |_| {
        let v = create_currency.version().get();
        if v != 0
            && v != last_v.get_untracked()
            && matches!(create_currency.value().get(), Some(Ok(_)))
        {
            open.set(false);
            last_v.set(v);
        }
    });
    view! {
        <Card title=t(locale, "finance.currency.manager.title") code="FIN-CUR-MGR"
              sub=t(locale, "finance.currency.manager.sub")>
            <div class="hstack" style="margin-bottom:10px">
                <button class="btn primary" type="button" on:click=toggle>
                    <Icon kind=IconKind::Plus size=14/>{t(locale, "finance.currency.new")}
                </button>
            </div>
            {move || if open.get() {
                view! {
                    <ActionForm action=create_currency attr:class="vstack" attr:style="gap:10px;padding:14px;background:var(--bg-2);border:1px solid var(--border);border-radius:8px">
                        <input type="hidden" name="sort_order" value=next_sort.to_string()/>
                        <div style="display:grid;grid-template-columns:1fr 1fr 2fr 1fr;gap:10px">
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.currency_code")}</span>
                                <input name="code" required autofocus maxlength="8"
                                       placeholder="USD" style=INPUT_STYLE_MONO/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.currency_symbol")}</span>
                                <input name="symbol" required maxlength="8"
                                       placeholder="$" style=INPUT_STYLE/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.currency_remark")}</span>
                                <input name="remark" maxlength="32"
                                       placeholder=t(locale, "finance.placeholder.currency_remark") style=INPUT_STYLE/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.currency_decimals")}</span>
                                <input name="decimals" type="number" min="0" max="18" value="2" required
                                       style=INPUT_STYLE_MONO/>
                            </label>
                        </div>
                        <ErrorSlot action=create_currency/>
                        <div class="hstack" style="gap:10px;align-items:center;justify-content:flex-end;flex-wrap:wrap">
                            <button class="btn ghost" type="button"
                                    on:click=move |_| open.set(false)>{t(locale, "finance.action.cancel")}</button>
                            <button class="btn primary" type="submit">
                                <Icon kind=IconKind::Check size=14/>{t(locale, "finance.action.create")}
                            </button>
                        </div>
                    </ActionForm>
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }}

            <div class="scroll-x" style="margin-top:14px">
                <table class="tbl">
                    <thead>
                        <tr>
                            <th>{t(locale, "finance.field.currency_code")}</th>
                            <th style="width:70px">{t(locale, "finance.field.currency_symbol")}</th>
                            <th>{t(locale, "finance.field.currency_remark")}</th>
                            <th class="num" style="width:80px">{t(locale, "finance.field.currency_decimals")}</th>
                            <th class="num" style="width:300px">{t(locale, "finance.field.ops")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {currencies.into_iter().map(|c| render_currency_row(c, update_currency, delete_currency, set_primary_currency)).collect_view()}
                    </tbody>
                </table>
            </div>
        </Card>
    }
}

/// Single row of the currency management table. `code` is immutable — the
/// edit form only touches symbol / remark / decimals / sort order.
fn render_currency_row(
    c: Currency,
    update_currency: ServerAction<UpdateCurrency>,
    delete_currency: ServerAction<DeleteCurrency>,
    set_primary_currency: ServerAction<SetPrimaryCurrency>,
) -> impl IntoView {
    let locale = use_locale();
    let is_primary = c.is_primary;
    let edit_code = c.code.clone();
    let primary_code = c.code.clone();
    let delete_code = c.code.clone();
    let symbol = c.symbol.clone();
    let remark = c.remark.clone();
    let sort_order_str = c.sort_order.to_string();
    let decimals_value = c.decimals.to_string();
    let display_code = c.code.clone();
    let display_symbol = c.symbol.clone();
    let display_remark = c.remark.clone();
    let display_decimals = c.decimals.to_string();
    let confirm_msg = tf(
        locale,
        "finance.currency.confirm_delete",
        &[("code", &c.code)],
    );
    let primary_cell = if is_primary {
        view! { <UiTag tone=Tone::Green>{t(locale, "finance.currency.primary")}</UiTag> }.into_any()
    } else {
        view! {
            <ActionForm action=set_primary_currency attr:style="display:inline">
                <input type="hidden" name="code" value=primary_code/>
                <button class="btn sm" type="submit">{t(locale, "finance.currency.set_primary")}</button>
            </ActionForm>
        }
        .into_any()
    };
    view! {
        <tr>
            <td class="mono">
                {display_code}
                {if is_primary {
                    view! { " " <UiTag tone=Tone::Green dot=true>{t(locale, "finance.currency.primary")}</UiTag> }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
            </td>
            <td class="mono">{display_symbol}</td>
            <td>{display_remark}</td>
            <td class="num mono">{display_decimals}</td>
            <td class="num">
                <div class="hstack" style="gap:6px;justify-content:flex-end;flex-wrap:wrap">
                    <details>
                        <summary class="btn ghost" style="cursor:pointer;display:inline-flex;align-items:center;gap:4px">
                            <Icon kind=IconKind::Settings size=12/>{t(locale, "finance.action.edit")}
                        </summary>
                        <ActionForm action=update_currency attr:class="vstack" attr:style="gap:8px;margin-top:8px;min-width:240px">
                            <input type="hidden" name="code" value=edit_code/>
                            <input type="hidden" name="sort_order" value=sort_order_str/>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.currency_symbol")}</span>
                                <input name="symbol" required maxlength="8" value=symbol style=INPUT_STYLE/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.currency_remark")}</span>
                                <input name="remark" maxlength="32" value=remark style=INPUT_STYLE/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>{t(locale, "finance.field.currency_decimals")}</span>
                                <input name="decimals" type="number" min="0" max="18" value=decimals_value style=INPUT_STYLE_MONO/>
                                <span class="mono dim" style="font-size:10px">{t(locale, "finance.currency.decimals_hint")}</span>
                            </label>
                            <div class="hstack" style="gap:8px;align-items:center">
                                <button class="btn primary" type="submit">
                                    <Icon kind=IconKind::Check size=12/>{t(locale, "finance.action.save")}
                                </button>
                                <ErrorSlot action=update_currency/>
                            </div>
                        </ActionForm>
                    </details>
                    {primary_cell}
                    <RowDeleteAction action=delete_currency value=delete_code field="code"
                                     confirm=confirm_msg label=t(locale, "finance.action.delete")/>
                </div>
            </td>
        </tr>
    }
}

fn render_reports(d: &LedgerData) -> impl IntoView {
    let locale = use_locale();
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.clone();
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
    let income_data: Vec<f64> = months.iter().map(|m| m.income.to_f64()).collect();
    let expense_data: Vec<f64> = months.iter().map(|m| m.expense.to_f64()).collect();
    let last = months.last().cloned().unwrap_or(MonthBucket {
        period: d.month.period.clone(),
        income: MinorAmount::ZERO,
        expense: MinorAmount::ZERO,
        net: MinorAmount::ZERO,
    });
    let net_strip = render_net_strip(&months, decimals);

    let category_share = render_category_share_card(d);

    view! {
        <div class="grid-2">
            <Card title=t(locale, "finance.reports.month_title") code="FIN-RPT-01"
                  sub=tf(locale, "finance.reports.month_sub", &[
                      ("symbol", &symbol),
                      ("count", &months.len().to_string()),
                      ("net", &fmt_minor_compact(last.net, decimals)),
                      ("income", &fmt_minor_compact(last.income, decimals)),
                      ("expense", &fmt_minor_compact(last.expense, decimals)),
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
/// max, so a deficit month would render the same as a +0 month.
pub fn render_net_strip(months: &[MonthBucket], decimals: u8) -> impl IntoView {
    let n = months.len();
    let cells: Vec<_> = months.iter().map(|m| {
        let mm = m.period.split('-').nth(1).unwrap_or("?").to_string();
        let (color_var, sign, val) = net_cell_parts(m.net, decimals);
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
/// net (minor units). Surplus uses `--primary-ink` (the design system has no
/// `--green-*` — sage green lives under `--primary-*`; see `.tag.green`).
fn net_cell_parts(net: MinorAmount, decimals: u8) -> (&'static str, &'static str, String) {
    if net.is_positive() {
        ("var(--primary-ink)", "+", fmt_minor_compact(net, decimals))
    } else if net.is_negative() {
        (
            "var(--rose-ink)",
            "−",
            fmt_minor_compact(net.abs(), decimals),
        )
    } else {
        ("var(--ink-4)", "", "0".to_string())
    }
}

/// The per-category bar list shared by both category-share cards: the ledger
/// tab's side card and the reports tab's FIN-RPT-02 card render the identical
/// row layout — only their surrounding `<Card>` chrome and empty state differ,
/// so just this inner list is shared.
fn render_category_share_rows(
    cats: Vec<crate::model::CategorySummary>,
    symbol: &str,
    decimals: u8,
) -> impl IntoView {
    let rows: Vec<_> = cats
        .into_iter()
        .map(|c| {
            let bar_color = Tone::parse(&c.tone).css_var();
            let pct = (c.pct * 3.0).min(100.0);
            let value = format!("{}{}", symbol, fmt_minor_compact(c.value, decimals));
            let label = category_summary_display(&c);
            view! {
                <div>
                    <div style="display:flex;justify-content:space-between;align-items:baseline;margin-bottom:4px">
                        <div style="font-size:12.5px">{label}</div>
                        <div class="mono" style="font-size:12px">{value} <span class="dim">{format!("· {}%", c.pct)}</span></div>
                    </div>
                    <div class="bar"><span style=format!("width:{:.1}%;background:{}", pct, bar_color)></span></div>
                </div>
            }
        })
        .collect();
    view! {
        <div class="vstack" style="gap:10px">
            {rows}
        </div>
    }
}

fn render_category_share_card(d: &LedgerData) -> impl IntoView {
    let locale = use_locale();
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.clone();
    let cats = d.category_summary.clone();
    let total: MinorAmount = cats.iter().map(|c| c.value).sum::<MinorAmount>().abs();
    view! {
        <Card title=t(locale, "finance.reports.category_title") code="FIN-RPT-02"
              sub=tf(locale, "finance.reports.category_sub", &[("symbol", &symbol), ("total", &fmt_minor_compact(total, decimals))])>
            {if cats.is_empty() {
                view! { <p class="muted">{t(locale, "finance.reports.category_empty")}</p> }.into_any()
            } else {
                render_category_share_rows(cats, &symbol, decimals).into_any()
            }}
        </Card>
    }
}

/// Suggested per-category budgets for next month, derived from the last
/// 3 calendar months of activity. Returns `(name, code, amount_minor)` for
/// every category that had any expense in the window, rounded to a tidy grid.
/// Empty when there's no 3-month history yet (fresh install).
fn next_month_plan(d: &LedgerData) -> Vec<(String, String, MinorAmount)> {
    use std::collections::HashMap;
    // Bucket months_12's last 3 months: months_12 is oldest → newest, so the
    // tail-3 is the recent quarter.
    let recent: Vec<&MonthBucket> = d.months_12.iter().rev().take(3).collect();
    if recent.is_empty() {
        return Vec::new();
    }
    // months_12 only carries totals, not per-category. For the per-category
    // average we approximate using the current month's category_summary
    // (signed, accurate, already aggregated). Scaling the current-month spend
    // by the elapsed-day ratio gives a forward-looking estimate without a
    // second SQL pass — "what would the rest of this month look like if
    // today's pace continued?", snapped to a tidy 50-major-unit grid.
    let elapsed = d.month.days_elapsed.max(1) as f64;
    let projected_factor = (31.0 / elapsed).min(2.5);
    // 50 major units, expressed in the currency's minor units.
    let grid = major_to_minor(50, d.currency.decimals);
    let grid_f = grid.to_f64();
    let mut by_code: HashMap<String, (String, MinorAmount)> = HashMap::new();
    for c in &d.category_summary {
        let projected = c.value.to_f64() * projected_factor;
        // Round to nearest grid step, with a floor of one step to avoid noise.
        let suggested =
            MinorAmount::new(((projected / grid_f).round() * grid_f).max(grid_f) as i128);
        by_code.insert(c.code.clone(), (category_summary_display(c), suggested));
    }
    let mut out: Vec<(String, String, MinorAmount)> = by_code
        .into_iter()
        .map(|(code, (name, suggested))| (name, code, suggested))
        .collect();
    // Largest suggested first — the user's attention is most valuable on
    // the categories that drive most of the spend.
    out.sort_by_key(|entry| std::cmp::Reverse(entry.2));
    out
}

/// Parse a `YYYY-MM-DD` string into a unix-second timestamp at the START of
/// that day in UTC. Empty / malformed input yields `None`. Pure math, safe
/// on wasm32.
fn parse_date_floor(s: &str) -> Option<i64> {
    parse_ymd(s).and_then(|(y, m, d)| date_to_unix(y, m, d, 0))
}

/// Same as `parse_date_floor` but at the END of the day (23:59:59 UTC) so
/// `t.occurred_at <= to_ts` is an inclusive day filter.
fn parse_date_ceiling(s: &str) -> Option<i64> {
    parse_ymd(s).and_then(|(y, m, d)| date_to_unix(y, m, d, 86_399))
}

fn date_to_unix(year: i32, month: u8, day: u8, offset_seconds: i64) -> Option<i64> {
    ymd_to_unix_midnight(year, month, day).map(|ts| ts + offset_seconds)
}

// CSV export — pure-Rust so the same code path runs on SSR (initial href is
// rendered as part of the page) and hydrate (refreshed reactively when the
// resource refetches). Amounts are written at the scoped currency's precision.
fn csv_data_uri(txns: &[Txn], decimals: u8) -> String {
    use std::fmt::Write as _;

    let mut csv = String::with_capacity(80 + txns.len() * 96);
    csv.push_str("doc_id,occurred_at,merchant,category,account,currency,amount,tag,note\n");
    for t in txns {
        let occurred = unix_to_ymdhm(t.occurred_at)
            .map(|(y, m, d, hh, mm)| format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:00Z"))
            .unwrap_or_default();
        let _ = writeln!(
            csv,
            "{},{},{},{},{},{},{},{},{}",
            t.doc_id,
            occurred,
            csv_escape(&t.merchant),
            t.category_code,
            t.account_code,
            t.currency_code,
            fmt_minor_raw(t.amount, decimals),
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
            currency_code: "CNY".into(),
            occurred_at: 0,
            merchant: "a,b".into(),
            category_code: "F&B".into(),
            account_code: "ACC-01".into(),
            amount: MinorAmount::from(-1_230),
            tag: "exp".into(),
            note: Some("x\"y".into()),
            linked_doc_id: None,
        }];

        let uri = csv_data_uri(&txns, 2);

        assert!(uri.starts_with("data:text/csv;charset=utf-8,doc_id%2Coccurred_at"));
        assert!(uri.contains("FIN-1%2C1970-01-01T00%3A00%3A00Z"));
        assert!(uri.contains("%22a%2Cb%22"));
        // -1230 minor units at 2 decimals → "-12.30"; `,exp,` follows.
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
    fn amount_step_scales_with_precision() {
        assert_eq!(amount_step(0), "1");
        assert_eq!(amount_step(1), "0.1");
        assert_eq!(amount_step(2), "0.01");
        assert_eq!(amount_step(8), "0.00000001");
        assert_eq!(amount_step(18), "0.000000000000000001");
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
        // 123_456 minor units at 2 decimals → 1,234.56 → compact "1,235".
        let (color, sign, val) = net_cell_parts(MinorAmount::from(123_456), 2);
        assert_eq!(color, "var(--primary-ink)");
        assert_eq!(sign, "+");
        assert_eq!(val, "1,235");
    }

    #[test]
    fn net_cell_deficit_uses_rose_ink_and_minus_sign() {
        let (color, sign, val) = net_cell_parts(MinorAmount::from(-123_456), 2);
        assert_eq!(color, "var(--rose-ink)");
        assert_eq!(sign, "−"); // U+2212 minus, not ASCII '-'
        assert_eq!(val, "1,235");
    }

    #[test]
    fn net_cell_zero_renders_dim_no_sign() {
        let (color, sign, val) = net_cell_parts(MinorAmount::ZERO, 2);
        assert_eq!(color, "var(--ink-4)");
        assert_eq!(sign, "");
        assert_eq!(val, "0");
    }
}
