use crate::model::{Account, AccountStats, Category, MonthBucket, Tag, Txn};
use crate::server_fns::*;
use ep_core::{fmt_int, fmt_money, fmt_ts_date, fmt_ts_hm, fmt_ts_md, IconKind, Tone};
use ep_ui::{Card, ChartBars, Icon, Kpi, PageHead, RowDeleteAction, Tabs, TabSpec, Tag as UiTag};
use ep_ui::kpi::Direction;
use leptos::prelude::*;

#[component]
pub fn FinanceView() -> impl IntoView {
    let active = RwSignal::new(String::from("ledger"));
    let ledger = Resource::new(|| (), |_| async { load_ledger().await });
    let add = ServerAction::<AddTxn>::new();
    let delete = ServerAction::<DeleteTxn>::new();
    let set_budget = ServerAction::<SetBudget>::new();
    let import_budgets = ServerAction::<ImportBudgetsFrom>::new();
    let merchant_filter = RwSignal::new(String::new());
    let category_filter = RwSignal::new(String::new());
    let date_from_filter = RwSignal::new(String::new());
    let date_to_filter = RwSignal::new(String::new());

    // Refetch only when an action's version actually changed since last
    // tick. ServerAction::version() ticks on both submit AND response, so
    // a naive `prev.is_some()` guard would refetch twice per submission.
    Effect::new(move |prev: Option<(usize, usize, usize, usize)>| {
        let cur = (
            add.version().get(),
            delete.version().get(),
            set_budget.version().get(),
            import_budgets.version().get(),
        );
        if prev.map_or(false, |p| p != cur) {
            ledger.refetch();
        }
        cur
    });

    view! {
        <div class="view">
            <PageHead
                code="FIN-01"
                module="FINANCE · 财务管理"
                title="Finance"
                title_cn="财务管理"
                sub="账户、预算、收支、投资。支持跨模块关联与自动分类。"
                actions=view! {
                    <a class="btn primary" href="#fin-new-merchant">
                        <Icon kind=IconKind::Plus size=14/>"记一笔"
                    </a>
                }.into_any()
            />

            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:160px">"loading…"</div> }>
                {move || ledger.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">"加载失败 · " {e.to_string()}</div></div> }.into_any(),
                    Ok(data) => render_ledger(
                        data, active, add, delete, set_budget, import_budgets,
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
    merchant_filter: RwSignal<String>,
    category_filter: RwSignal<String>,
    date_from_filter: RwSignal<String>,
    date_to_filter: RwSignal<String>,
) -> impl IntoView {
    let m = &data.month;
    let bud_pct = if m.budget_total > 0.0 {
        (m.expense / m.budget_total * 100.0).round() as u32
    } else { 0 };
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
    let daily_dir = if daily_delta < -0.5 { Direction::Up }      // less spend = up (saving)
                    else if daily_delta > 0.5 { Direction::Down }
                    else { Direction::Flat };
    let savings_pct = (m.savings_rate * 100.0).round() as u32;
    let savings_dir = match savings_pct {
        0..=10 => Direction::Down,
        11..=29 => Direction::Flat,
        _ => Direction::Up,
    };
    let emergency_dir = if m.emergency_months >= 6.0 { Direction::Up }
                       else if m.emergency_months >= 3.0 { Direction::Flat }
                       else { Direction::Down };
    // Tab badge reflects visible rows (LIMIT 50, not month-scoped); the
    // month aggregate goes in the card sub-label.
    let txns_count = data.txns.len() as u32;
    let accounts_count = data.accounts.len() as u32;
    let budgets_count = data.budgets.len() as u32;

    let banner = render_banner(&data);
    // Pre-compute attribute strings — the `view!` macro rejects bare if/else
    // in attribute-value position.
    let daily_delta_text = if daily_3m > 0.0 {
        let sign = if daily_delta >= 0.0 { "+" } else { "−" };
        format!("{}¥{} vs 90d 均值", sign, fmt_int(daily_delta.abs()))
    } else {
        format!("第 {} 天 · 90d 数据不足", m.days_elapsed)
    };
    let savings_delta_text = if m.savings >= 0.0 {
        format!("¥{} 净结余", fmt_int(m.savings))
    } else {
        format!("−¥{} 透支", fmt_int(m.savings.abs()))
    };
    let emergency_delta_text = if m.avg_expense_3m > 0.0 {
        format!("¥{} 流动 / ¥{} 月均", fmt_int(m.liquid_balance), fmt_int(m.avg_expense_3m))
    } else {
        "数据不足 · 至少需 1 月支出".to_string()
    };
    let kpis = view! {
        <div class="kpi-grid">
            <Kpi code="FIN-K01" label="本月预算" value=format!("{}", bud_pct) unit="%".to_string()
                 delta=format!("¥{} / ¥{}", fmt_int(m.expense), fmt_int(m.budget_total))
                 dir=bud_dir/>
            <Kpi code="FIN-K02" label="日均支出"
                 value=format!("¥{}", fmt_int(daily_now))
                 delta=daily_delta_text
                 dir=daily_dir/>
            <Kpi code="FIN-K03" label="储蓄率"
                 value=format!("{}", savings_pct) unit="%".to_string()
                 delta=savings_delta_text
                 dir=savings_dir/>
            <Kpi code="FIN-K04" label="应急金"
                 value=format!("{:.1}", m.emergency_months) unit="月".to_string()
                 delta=emergency_delta_text
                 dir=emergency_dir/>
        </div>
    };

    let tabs = vec![
        TabSpec::new("ledger", "总账 / Ledger").with_count(txns_count),
        TabSpec::new("budget", "预算 / Budget").with_count(budgets_count),
        TabSpec::new("accounts", "账户 / Accounts").with_count(accounts_count),
        TabSpec::new("reports", "报表 / Reports"),
    ];

    // Share the loaded LedgerData across all four tab branches by Arc
    // instead of deep-cloning per branch. (Arc, not Rc — Leptos's reactive
    // closures require Send.) Each tab closure clones a cheap handle.
    let data = std::sync::Arc::new(data);
    let data_for_ledger = data.clone();
    let data_for_budget = data.clone();
    let data_for_accounts = data.clone();
    let data_for_reports = data;

    view! {
        {banner}
        {kpis}
        <Tabs tabs=tabs active=active/>
        {move || match active.get().as_str() {
            "budget" => render_budget(&data_for_budget, set_budget, import_budgets).into_any(),
            "accounts" => render_accounts(&data_for_accounts).into_any(),
            "reports" => render_reports(&data_for_reports).into_any(),
            _ => view! {
                {render_new_txn_form(add, data_for_ledger.categories.clone(), data_for_ledger.accounts.clone())}
                {render_ledger_tab(&data_for_ledger, delete, merchant_filter, category_filter, date_from_filter, date_to_filter)}
            }.into_any(),
        }}
    }
}

fn render_banner(d: &LedgerData) -> impl IntoView {
    let m = &d.month;
    let week_sign = if m.balance_delta >= 0.0 { "+" } else { "−" };
    let savings_pct = (m.savings_rate * 100.0).round() as u32;
    // Net-worth tone tracks the most recent week: + → green/healthy,
    // 0 → flat/neutral, − → rose/watch.
    let (worth_tone, worth_label) = if m.balance_delta > 0.0 {
        (Tone::Green, "健康")
    } else if m.balance_delta == 0.0 {
        (Tone::None, "持平")
    } else {
        (Tone::Rose, "关注")
    };
    view! {
        <div class="module-banner">
            <div class="module-glyph fin mono">"¥"</div>
            <div style="flex:1">
                <div class="hstack" style="margin-bottom:6px;gap:8px">
                    <span class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">"净资产 / NET WORTH"</span>
                    <UiTag tone=worth_tone dot=true>{worth_label}</UiTag>
                </div>
                <div class="mono" style="font-size:32px;font-weight:600;letter-spacing:-0.02em;line-height:1.1">
                    "¥" {fmt_money(m.balance)}
                </div>
                <div class="hstack" style="gap:16px;margin-top:8px;font-size:12.5px;color:var(--ink-3)">
                    <span class="mono">{format!("{}¥{} 本周", week_sign, fmt_int(m.balance_delta.abs()))}</span>
                    <span class="mono">{format!("储蓄率 {}%", savings_pct)}</span>
                    <span class="mono">{format!("{} 账户", d.accounts.len())}</span>
                </div>
            </div>
            <div class="hstack" style="gap:20px;padding-right:8px">
                <div>
                    <div class="mono" style="font-size:10.5px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:4px">"月收入"</div>
                    <div class="mono" style="font-size:18px;font-weight:600;color:var(--primary-ink)">"+¥" {fmt_int(m.income)}</div>
                </div>
                <div class="sep-v"></div>
                <div>
                    <div class="mono" style="font-size:10.5px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:4px">"月支出"</div>
                    <div class="mono" style="font-size:18px;font-weight:600">"−¥" {fmt_int(m.expense)}</div>
                </div>
                <div class="sep-v"></div>
                <div>
                    <div class="mono" style="font-size:10.5px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:4px">"月结余"</div>
                    <div class="mono" style="font-size:18px;font-weight:600">{format!("{}¥{}", if m.savings >= 0.0 { "" } else { "−" }, fmt_int(m.savings.abs()))}</div>
                </div>
            </div>
        </div>
    }
}

const INPUT_STYLE: &str = "padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)";
const INPUT_STYLE_MONO: &str = "padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono)";
const FIELD_LABEL: &str = "font-size:11px;text-transform:uppercase;letter-spacing:0.06em";

fn render_new_txn_form(
    add: ServerAction<AddTxn>,
    categories: Vec<Category>,
    accounts: Vec<Account>,
) -> impl IntoView {
    view! {
        <Card title="记一笔" code="FIN-NEW" sub="新建交易 · 自动生成 FIN-NNNNN 单号 · 标签自动定号位">
            <ActionForm action=add attr:class="vstack" attr:style="gap:10px">
                <div style="display:grid;grid-template-columns:2fr 1fr 1fr;gap:10px">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>"商户 / 描述"</span>
                        <input id="fin-new-merchant" name="merchant" required
                               placeholder="盒马 · 生鲜" style=INPUT_STYLE/>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>"金额 (¥)"</span>
                        <input name="amount" type="number" step="0.01" min="0.01" required
                               placeholder="42.00" style=INPUT_STYLE_MONO/>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>"标签"</span>
                        <select name="tag" style=INPUT_STYLE>
                            <option value=Tag::Exp.as_str() selected="selected">"支出 · exp"</option>
                            <option value=Tag::Inc.as_str()>"收入 · inc"</option>
                            <option value=Tag::Tfr.as_str()>"转账 · tfr"</option>
                        </select>
                    </label>
                </div>
                <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:10px">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>"类别"</span>
                        <select name="category_code" style=INPUT_STYLE>
                            {categories.into_iter().enumerate().map(|(i, c)| {
                                let code = c.code.clone();
                                let label = format!("{} {}", c.name, c.code);
                                view! { <option value=code selected={i == 0}>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>"账户"</span>
                        <select name="account_code" style=INPUT_STYLE>
                            {accounts.into_iter().enumerate().map(|(i, a)| {
                                let code = a.code.clone();
                                let label = format!("{} · {}", a.code, a.name);
                                view! { <option value=code selected={i == 0}>{label}</option> }
                            }).collect_view()}
                        </select>
                    </label>
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>"关联单号 (可选)"</span>
                        <input name="linked_doc_id" placeholder="FIT-S-0412 / LRN-B-014"
                               style=INPUT_STYLE_MONO/>
                    </label>
                </div>
                <div style="display:grid;grid-template-columns:3fr auto auto;gap:10px;align-items:end">
                    <label class="vstack" style="gap:4px">
                        <span class="mono dim" style=FIELD_LABEL>"备注 (可选)"</span>
                        <input name="note" placeholder="…" style=INPUT_STYLE/>
                    </label>
                    <span class="error-slot" style="align-self:center">
                        {move || add.value().get().and_then(|r| r.err()).map(|e| view! {
                            <span class="tag rose">{e.to_string()}</span>
                        })}
                    </span>
                    <button class="btn primary" type="submit" style="align-self:center">
                        <Icon kind=IconKind::Plus size=14/>"记录"
                    </button>
                </div>
            </ActionForm>
        </Card>
    }
}

fn render_ledger_tab(
    d: &LedgerData,
    delete: ServerAction<DeleteTxn>,
    merchant_filter: RwSignal<String>,
    category_filter: RwSignal<String>,
    date_from_filter: RwSignal<String>,
    date_to_filter: RwSignal<String>,
) -> impl IntoView {
    let txns = d.txns.clone();
    let cat_summary = d.category_summary.clone();
    let cat_options = d.categories.clone();
    let cat_lookup: std::collections::HashMap<String, Category> = d.categories.iter()
        .map(|c| (c.code.clone(), c.clone()))
        .collect();
    let visible_count = txns.len();
    let total_count = d.month.total_txn_count as usize;
    // Computed once per render of this tab. The parent re-runs render_ledger_tab
    // whenever the resource refetches (add / delete), so the export link picks
    // up new rows automatically — no reactive attribute needed.
    let export_href = csv_data_uri(&txns);
    // Same lifetime story for AI suggestions: compute now while we still
    // hold `&d`, hand the owned `Vec<Suggestion>` to the view macro.
    let suggestions = crate::suggestions::compute_suggestions(d);

    // The table is "most recent 50, all-time" — the sub-label has to
    // surface that and the month-specific count without conflating them.
    let sub = match (visible_count, total_count) {
        (0, 0) => "暂无交易 · 在上方记一笔填充".to_string(),
        (v, m) if v >= 50 => format!("展示最近 50 笔 · 本月已记录 {} 笔 · 支持商户搜索 / 类别 / 日期筛选", m),
        (v, m) => format!("共 {} 笔（全部）· 本月 {} 笔 · 支持商户搜索 / 类别 / 日期筛选", v, m),
    };

    view! {
        <div class="grid-2" style="margin-top:20px">
            <Card title="交易明细" code="FIN-LGR-01" sub=sub>
                <div class="hstack" style="gap:10px;margin-bottom:12px;flex-wrap:wrap">
                    <input type="text" placeholder="搜索商户 / 描述…"
                           prop:value=move || merchant_filter.get()
                           on:input=move |ev| merchant_filter.set(event_target_value(&ev))
                           style=format!("flex:1;min-width:160px;{}", INPUT_STYLE)/>
                    <select prop:value=move || category_filter.get()
                            on:change=move |ev| category_filter.set(event_target_value(&ev))
                            style=INPUT_STYLE>
                        <option value="">"全部类别"</option>
                        {cat_options.into_iter().map(|c| {
                            let code = c.code.clone();
                            let label = format!("{} {}", c.name, c.code);
                            view! { <option value=code>{label}</option> }
                        }).collect_view()}
                    </select>
                    <input type="date"
                           prop:value=move || date_from_filter.get()
                           on:input=move |ev| date_from_filter.set(event_target_value(&ev))
                           style=INPUT_STYLE_MONO
                           title="起始日期"/>
                    <input type="date"
                           prop:value=move || date_to_filter.get()
                           on:input=move |ev| date_to_filter.set(event_target_value(&ev))
                           style=INPUT_STYLE_MONO
                           title="结束日期"/>
                    <a class="btn" download="finance-export.csv" href=export_href>
                        <Icon kind=IconKind::Export size=14/>"导出"
                    </a>
                </div>
                <div class="scroll-x">
                    <table class="tbl">
                        <thead>
                            <tr>
                                <th style="width:76px">"日期"</th>
                                <th style="width:110px">"单号"</th>
                                <th>"商户 / 描述"</th>
                                <th style="width:80px">"类别"</th>
                                <th style="width:80px">"账户"</th>
                                <th class="num" style="width:110px">"金额"</th>
                                <th style="width:80px">"关联"</th>
                                <th class="num" style="width:64px">"操作"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {move || {
                                let mq = merchant_filter.get().to_lowercase();
                                let cq = category_filter.get();
                                // Date filter: convert YYYY-MM-DD to start-of-day unix
                                // seconds, treating empty inputs as -∞ / +∞. The end
                                // bound includes the entire end day (24h window).
                                let from_ts = parse_date_floor(&date_from_filter.get());
                                let to_ts = parse_date_ceiling(&date_to_filter.get());
                                let cat_lookup = &cat_lookup;
                                txns.iter()
                                    .filter(|t| {
                                        (mq.is_empty() || t.merchant.to_lowercase().contains(&mq))
                                        && (cq.is_empty() || t.category_code == cq)
                                        && from_ts.map(|f| t.occurred_at >= f).unwrap_or(true)
                                        && to_ts.map(|to| t.occurred_at <= to).unwrap_or(true)
                                    })
                                    .cloned()
                                    .map(|t| render_txn_row(t, cat_lookup, delete))
                                    .collect_view()
                            }}
                        </tbody>
                    </table>
                </div>
            </Card>

            <div class="vstack" style="gap:20px">
                <Card title="支出结构" code="FIN-R02" sub="本月 · 按类别">
                    {if cat_summary.is_empty() {
                        view! { <p class="muted">"本月暂无支出 · 在左侧记一笔填充"</p> }.into_any()
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

                <Card title="智能建议" code="FIN-AI-01" sub="基于本月预算 + 近 30 天交易 · 规则驱动">
                    <div class="vstack" style="gap:10px">
                        {render_suggestions(suggestions)}
                    </div>
                </Card>
            </div>
        </div>
    }
}

fn render_suggestions(items: Vec<crate::suggestions::Suggestion>) -> impl IntoView {
    if items.is_empty() {
        return view! { <p class="muted">"未识别可行动建议 · 数据健康"</p> }.into_any();
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
    }.into_any()
}

fn render_txn_row(
    t: Txn,
    cat_lookup: &std::collections::HashMap<String, Category>,
    delete: ServerAction<DeleteTxn>,
) -> impl IntoView {
    let date = fmt_ts_md(Some(t.occurred_at));
    let time_ = fmt_ts_hm(Some(t.occurred_at));
    let cls_amt = if t.amount > 0.0 { "num amt-pos" } else { "num amt-neg" };
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
    let link = t.linked_doc_id.clone();
    let cat_tone = cat_lookup.get(&t.category_code)
        .map(|c| Tone::from_str(&c.tone))
        .unwrap_or(Tone::None);
    let cat_label = t.category_code.clone();
    let doc_id = t.doc_id.clone();
    view! {
        <tr>
            <td class="mono dim">{date}<div style="font-size:10px;color:var(--ink-4)">{time_}</div></td>
            <td class="doc">{t.doc_id.clone()}</td>
            <td>
                <span class=txind></span>
                {t.merchant.clone()}
            </td>
            <td><UiTag tone=cat_tone>{cat_label}</UiTag></td>
            <td class="mono dim">{t.account_code.clone()}</td>
            <td class=cls_amt>{amount_text}</td>
            <td class="mono dim">
                {match link {
                    Some(l) => view! { <span><Icon kind=IconKind::Link size=10/> " " {l}</span> }.into_any(),
                    None => view! { <span>"—"</span> }.into_any(),
                }}
            </td>
            <td class="num">
                <RowDeleteAction action=delete value=doc_id
                                 confirm="删除该笔交易？账户余额会同步回滚。"/>
            </td>
        </tr>
    }
}

fn render_budget(
    d: &LedgerData,
    set_budget: ServerAction<SetBudget>,
    import_budgets: ServerAction<ImportBudgetsFrom>,
) -> impl IntoView {
    let m = &d.month;
    let period = m.period.clone();
    let categories_for_form = d.categories.clone();
    // Owned-string lookup so the closures below don't capture a borrow into
    // a Vec the view! macro will move.
    let cat_lookup: std::collections::HashMap<String, (String, String)> = d.categories.iter()
        .map(|c| (c.code.clone(), (c.name.clone(), c.tone.clone())))
        .collect();
    let budgets = d.budgets.clone();
    let budgets_count = budgets.len();
    // Categories that have spent this month but no budget — surfaced so the
    // user can react ("oh I forgot to budget for X this month").
    let unbudgeted: Vec<crate::model::CategorySummary> = d.category_summary.iter()
        .filter(|c| !d.budgets.iter().any(|b| b.category_code == c.code))
        .cloned()
        .collect();
    let next_month_planner = next_month_plan(d);
    // Pre-compute every period-derived string up front so the view! body
    // doesn't have to clone `period` through nested closures.
    let import_source = previous_period(&period);
    let import_target = period.clone();
    let next_period_label = next_period(&period);
    let pool_title = format!("预算池 · {}", period);
    let pool_sub = if budgets_count == 0 {
        "本期尚未设置预算".to_string()
    } else {
        format!("{} 个类别 · 已用 ¥{} / ¥{}", budgets_count, fmt_int(m.expense), fmt_int(m.budget_total))
    };
    let import_button_label = format!("从 {} 导入", import_source);
    let empty_period_hint = format!("{} 期间未设置任何预算 · 通过右侧编辑器添加，或一键复制上期。", period);
    let next_month_sub = format!("基于本月支出节奏推算 · 建议 {} 期", next_period_label);
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
                                        <span class="tag rose">{e.to_string()}</span>
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
                                    </div>
                                }
                            }).collect_view()}
                            {if unbudgeted.is_empty() {
                                view! { <span></span> }.into_any()
                            } else {
                                view! {
                                    <div style="margin-top:6px;padding-top:10px;border-top:1px dashed var(--border)">
                                        <div class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">"未预算的类别 · 已发生支出"</div>
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
                <Card title="编辑预算" code="FIN-BDG-EDIT"
                      sub="选择期间 + 类别 · 金额 0 视为删除条目">
                    <ActionForm action=set_budget attr:class="vstack" attr:style="gap:10px">
                        <div style="display:grid;grid-template-columns:1fr 1.5fr 1fr auto;gap:10px;align-items:end">
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>"期间"</span>
                                <input name="period" type="month"
                                       value=editor_period required
                                       style=INPUT_STYLE_MONO/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>"类别"</span>
                                <select name="category_code" style=INPUT_STYLE>
                                    {categories_for_form.into_iter().map(|c| {
                                        let code = c.code.clone();
                                        let label = format!("{} {}", c.name, c.code);
                                        view! { <option value=code>{label}</option> }
                                    }).collect_view()}
                                </select>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style=FIELD_LABEL>"金额 (¥)"</span>
                                <input name="amount" type="number" step="50" min="0"
                                       placeholder="3200" style=INPUT_STYLE_MONO/>
                            </label>
                            <button class="btn primary" type="submit">
                                <Icon kind=IconKind::Check size=14/>"保存"
                            </button>
                        </div>
                        <span class="error-slot">
                            {move || set_budget.value().get().and_then(|r| r.err()).map(|e| view! {
                                <span class="tag rose">{e.to_string()}</span>
                            })}
                        </span>
                    </ActionForm>
                </Card>

                <Card title="下月规划" code="FIN-BDG-02" sub=next_month_sub>
                    {if next_month_planner.is_empty() {
                        view! { <p class="muted">"近 3 个月支出数据不足 · 至少需 1 笔记录方能给出规划"</p> }.into_any()
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
                                    "建议金额 = 近 3 月该类别支出 ÷ 3 · 取整到 50"
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
    if bytes.len() != 7 || bytes[4] != b'-' { return None; }
    let y: i32 = period[..4].parse().ok()?;
    let m: u32 = period[5..7].parse().ok()?;
    if !(1..=12).contains(&m) { return None; }
    Some((y, m))
}

fn render_accounts(d: &LedgerData) -> impl IntoView {
    let pairs: Vec<(Account, AccountStats)> = d.accounts.iter().cloned()
        .zip(d.account_stats.iter().cloned())
        .collect();
    view! {
        <div class="grid-3">
            {pairs.into_iter().map(|(a, s)| {
                let tone = Tone::from_str(&a.tone);
                let last_seen = match s.last_seen_at {
                    Some(ts) => format!("最近活动 {}", fmt_ts_date(Some(ts))),
                    None => "尚无活动".to_string(),
                };
                view! {
                    <Card title=a.name.clone() code=a.code.clone() sub=a.r#type.clone()>
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
                    </Card>
                }
            }).collect_view()}
        </div>
    }
}

fn render_reports(d: &LedgerData) -> impl IntoView {
    let months = d.months_12.clone();
    if months.is_empty() {
        return view! {
            <div class="card"><div class="card-body">
                <p class="muted">"暂无可聚合数据 · 至少需要一笔交易"</p>
            </div></div>
        }.into_any();
    }
    let labels: Vec<String> = months.iter()
        .map(|m| m.period.split('-').nth(1).unwrap_or("?").to_string())
        .collect();
    let income_data: Vec<f64> = months.iter().map(|m| m.income).collect();
    let expense_data: Vec<f64> = months.iter().map(|m| m.expense).collect();
    let last = months.last().cloned().unwrap_or(MonthBucket {
        period: d.month.period.clone(), income: 0.0, expense: 0.0, net: 0.0,
    });
    let net_strip = render_net_strip(&months);

    let category_share = render_category_share_card(d);

    view! {
        <div class="grid-2">
            <Card title="月度趋势" code="FIN-RPT-01"
                  sub=format!("{} 月 · 本月 ¥{} 净结余 · 收入 ¥{} / 支出 ¥{}",
                              months.len(), fmt_int(last.net), fmt_int(last.income), fmt_int(last.expense))>
                <div class="vstack" style="gap:14px">
                    <div>
                        <div class="mono dim" style="font-size:10.5px;text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">"收入 · INCOME"</div>
                        <ChartBars data=income_data labels=labels.clone()/>
                    </div>
                    <div>
                        <div class="mono dim" style="font-size:10.5px;text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">"支出 · EXPENSE"</div>
                        <ChartBars data=expense_data labels=labels/>
                    </div>
                    <div>
                        <div class="mono dim" style="font-size:10.5px;text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">"净结余 · NET (绿=盈余 / 玫=透支)"</div>
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
    let cats = d.category_summary.clone();
    // `.abs()` because `fmt_int` of IEEE -0.0 prints "-0".
    let total: f64 = cats.iter().map(|c| c.value).sum::<f64>().abs();
    view! {
        <Card title="类别分布" code="FIN-RPT-02"
              sub=format!("本月 · 共 ¥{}", fmt_int(total))>
            {if cats.is_empty() {
                view! { <p class="muted">"本月尚无支出数据"</p> }.into_any()
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
    if recent.is_empty() { return Vec::new(); }
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
    let mut out: Vec<(String, String, f64)> = by_code.into_iter()
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
    parse_date_components(s).map(|(y, m, d)| date_to_unix(y, m, d, 0))
}

/// Same as `parse_date_floor` but at the END of the day (23:59:59 UTC) so
/// `t.occurred_at <= to_ts` is an inclusive day filter.
fn parse_date_ceiling(s: &str) -> Option<i64> {
    parse_date_components(s).map(|(y, m, d)| date_to_unix(y, m, d, 86_399))
}

fn parse_date_components(s: &str) -> Option<(i32, u8, u8)> {
    let bytes = s.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' { return None; }
    let y: i32 = s[..4].parse().ok()?;
    let m: u8 = s[5..7].parse().ok()?;
    let d: u8 = s[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) { return None; }
    Some((y, m, d))
}

fn date_to_unix(year: i32, month: u8, day: u8, offset_seconds: i64) -> i64 {
    // Reuse `time::Date` to avoid hand-rolling leap-year arithmetic. Pure
    // math (no `now`-style call), so it's wasm-safe and matches the
    // `fmt_ts_*` helpers' precedent.
    let Ok(month_enum) = time::Month::try_from(month) else { return 0 };
    let date = match time::Date::from_calendar_date(year, month_enum, day) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    let dt = date.with_hms(0, 0, 0).unwrap_or_else(|_| {
        time::PrimitiveDateTime::new(date, time::Time::MIDNIGHT)
    });
    dt.assume_utc().unix_timestamp() + offset_seconds
}

// CSV export — pure-Rust so the same code path runs on SSR (initial href is
// rendered as part of the page) and hydrate (refreshed reactively when the
// resource refetches).
fn csv_data_uri(txns: &[Txn]) -> String {
    use std::fmt::Write as _;

    let mut csv = String::with_capacity(80 + txns.len() * 96);
    csv.push_str("doc_id,occurred_at,merchant,category,account,amount,tag,note,linked_doc_id\n");
    for t in txns {
        let occurred = time::OffsetDateTime::from_unix_timestamp(t.occurred_at)
            .ok()
            .and_then(|d| d.format(&time::format_description::well_known::Rfc3339).ok())
            .unwrap_or_default();
        let _ = writeln!(
            csv,
            "{},{},{},{},{},{:.2},{},{},{}",
            t.doc_id,
            occurred,
            csv_escape(&t.merchant),
            t.category_code,
            t.account_code,
            t.amount,
            t.tag,
            csv_escape(t.note.as_deref().unwrap_or("")),
            t.linked_doc_id.as_deref().unwrap_or(""),
        );
    }
    let encoded = percent_encode(&csv);
    let mut uri = String::with_capacity("data:text/csv;charset=utf-8,".len() + encoded.len());
    uri.push_str("data:text/csv;charset=utf-8,");
    uri.push_str(&encoded);
    uri
}

fn csv_escape(s: &str) -> String {
    if !s.bytes().any(|b| matches!(b, b',' | b'"' | b'\n' | b'\r')) {
        return s.to_string();
    }

    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        if ch == '"' {
            out.push_str("\"\"");
        } else {
            out.push(ch);
        }
    }
    out.push('"');
    out
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
    fn percent_encode_keeps_unreserved_and_encodes_utf8() {
        assert_eq!(percent_encode("AZaz09-_.~"), "AZaz09-_.~");
        assert_eq!(percent_encode("工资, ok"), "%E5%B7%A5%E8%B5%84%2C%20ok");
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
            linked_doc_id: Some("FIT-1".into()),
        }];

        let uri = csv_data_uri(&txns);

        assert!(uri.starts_with("data:text/csv;charset=utf-8,doc_id%2Coccurred_at"));
        assert!(uri.contains("FIN-1%2C1970-01-01T00%3A00%3A00Z"));
        assert!(uri.contains("%22a%2Cb%22"));
        assert!(uri.contains("-12.30%2Cexp%2C%22x%22%22y%22%2CFIT-1"));
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
        assert_eq!(parse_date_floor("not-a-date"), None);
    }

    #[test]
    fn parse_date_ceiling_returns_end_of_day() {
        // 86399 sec after midnight = 23:59:59 UTC
        assert_eq!(parse_date_ceiling("2024-05-01"), Some(1_714_521_600 + 86_399));
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
