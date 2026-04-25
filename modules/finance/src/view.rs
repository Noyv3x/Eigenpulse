use crate::server_fns::*;
use ep_core::{fmt_int, fmt_money, IconKind, Tone};
use ep_ui::{Card, ChartBars, Icon, Kpi, PageHead, Tabs, TabSpec, Tag};
use ep_ui::kpi::Direction;
use leptos::prelude::*;

#[component]
pub fn FinanceView() -> impl IntoView {
    let active = RwSignal::new(String::from("ledger"));
    let ledger = Resource::new(|| (), |_| async { load_ledger().await });

    view! {
        <div class="view">
            <PageHead
                code="FIN-01".to_string()
                module="FINANCE · 财务管理".to_string()
                title="Finance".to_string()
                title_cn="财务管理"
                sub="账户、预算、收支、投资。支持跨模块关联与自动分类。"
                actions=view! {
                    <>
                        <button class="btn"><Icon kind=IconKind::Filter size=14/>"筛选"</button>
                        <button class="btn"><Icon kind=IconKind::Export size=14/>"导出"</button>
                        <button class="btn primary"><Icon kind=IconKind::Plus size=14/>"记一笔"</button>
                    </>
                }.into_any()
            />

            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:160px">"loading…"</div> }>
                {move || ledger.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">"加载失败 · " {e.to_string()}</div></div> }.into_any(),
                    Ok(data) => render_ledger(data, active).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_ledger(data: LedgerData, active: RwSignal<String>) -> impl IntoView {
    let bud_pct = if data.month.budget_total > 0.0 {
        (data.month.budget_used / data.month.budget_total * 100.0).round() as u32
    } else { 0 };
    let txns_count = data.txns.len() as u32;
    let accounts_count = data.accounts.len() as u32;

    let banner = render_banner(&data);
    let kpis = view! {
        <div class="kpi-grid">
            <Kpi code="FIN-K01" label="本月预算" value=format!("{}", bud_pct) unit="%".to_string()
                 delta=format!("¥{} / ¥{}", fmt_int(data.month.budget_used), fmt_int(data.month.budget_total))
                 dir=Direction::Flat/>
            <Kpi code="FIN-K02" label="日均支出" value=format!("¥{}", fmt_int(data.month.expense / 22.0))
                 delta="+¥14 vs 上月".to_string() dir=Direction::Down/>
            <Kpi code="FIN-K03" label="投资收益" value="+¥1,284".to_string()
                 delta="+3.2% MTD".to_string() dir=Direction::Up/>
            <Kpi code="FIN-K04" label="应急金" value="6.2".to_string() unit="月".to_string()
                 delta="目标 6 月".to_string() dir=Direction::Up/>
        </div>
    };

    let tabs = vec![
        TabSpec::new("ledger", "总账 / Ledger").with_count(txns_count),
        TabSpec::new("budget", "预算 / Budget"),
        TabSpec::new("accounts", "账户 / Accounts").with_count(accounts_count),
        TabSpec::new("reports", "报表 / Reports"),
    ];

    let data_for_ledger = data.clone();
    let data_for_budget = data.clone();
    let data_for_accounts = data.clone();

    view! {
        {banner}
        {kpis}
        <Tabs tabs=tabs active=active/>
        {move || match active.get().as_str() {
            "budget" => render_budget(&data_for_budget).into_any(),
            "accounts" => render_accounts(&data_for_accounts).into_any(),
            "reports" => render_reports().into_any(),
            _ => render_ledger_tab(&data_for_ledger).into_any(),
        }}
    }
}

fn render_banner(d: &LedgerData) -> impl IntoView {
    let m = &d.month;
    view! {
        <div class="module-banner">
            <div class="module-glyph fin mono">"¥"</div>
            <div style="flex:1">
                <div class="hstack" style="margin-bottom:6px;gap:8px">
                    <span class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">"净资产 / NET WORTH"</span>
                    <Tag tone=Tone::Green dot=true>"健康"</Tag>
                </div>
                <div class="mono" style="font-size:32px;font-weight:600;letter-spacing:-0.02em;line-height:1.1">
                    "¥" {fmt_money(m.balance)}
                </div>
                <div class="hstack" style="gap:16px;margin-top:8px;font-size:12.5px;color:var(--ink-3)">
                    <span class="mono">"+¥" {fmt_int(m.balance_delta)} " 本周"</span>
                    <span class="mono">"储蓄率 47.8%"</span>
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
                    <div class="mono" style="font-size:18px;font-weight:600">"¥" {fmt_int(m.savings)}</div>
                </div>
            </div>
        </div>
    }
}

fn render_ledger_tab(d: &LedgerData) -> impl IntoView {
    let txns = d.txns.clone();
    let cat_summary = d.category_summary.clone();
    view! {
        <div class="grid-2">
            <Card title="交易明细"
                  code="FIN-LGR-01"
                  sub=format!("共 {} 笔 · 本月", txns.len())>
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
                            </tr>
                        </thead>
                        <tbody>
                            {txns.into_iter().map(|t| {
                                let dt = time::OffsetDateTime::from_unix_timestamp(t.occurred_at).ok();
                                let date = dt.map(|d| format!("{:02}-{:02}", d.month() as u8, d.day())).unwrap_or_default();
                                let time_ = dt.map(|d| format!("{:02}:{:02}", d.hour(), d.minute())).unwrap_or_default();
                                let cls_amt = if t.amount > 0.0 { "num amt-pos" } else { "num amt-neg" };
                                let txind = match t.tag.as_str() { "inc" => "txind inc", "tfr" => "txind tfr", _ => "txind exp" };
                                let amount_text = if t.amount > 0.0 {
                                    format!("+¥{}", fmt_money(t.amount))
                                } else {
                                    format!("−¥{}", fmt_money(t.amount.abs()))
                                };
                                let link = t.linked_doc_id.clone();
                                let cat_tone = match t.category_code.as_str() {
                                    "INC" => Tone::Green,
                                    "TFR" => Tone::Blue,
                                    _ => Tone::None,
                                };
                                let cat_label = t.category_code.clone();
                                view! {
                                    <tr>
                                        <td class="mono dim">{date}<div style="font-size:10px;color:var(--ink-4)">{time_}</div></td>
                                        <td class="doc">{t.doc_id.clone()}</td>
                                        <td>
                                            <span class=txind></span>
                                            {t.merchant.clone()}
                                        </td>
                                        <td><Tag tone=cat_tone>{cat_label}</Tag></td>
                                        <td class="mono dim">{t.account_code.clone()}</td>
                                        <td class=cls_amt>{amount_text}</td>
                                        <td class="mono dim">
                                            {match link {
                                                Some(l) => view! { <span><Icon kind=IconKind::Link size=10/> " " {l}</span> }.into_any(),
                                                None => view! { <span>"—"</span> }.into_any(),
                                            }}
                                        </td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
            </Card>

            <div class="vstack" style="gap:20px">
                <Card title="支出结构" code="FIN-R02" sub="本月 · 按类别">
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
                </Card>

                <Card title="智能建议" code="FIN-AI-01" sub="基于近 30 天数据">
                    <div class="vstack" style="gap:10px">
                        <div class="list-row">
                            <div class="icon-tile"><Icon kind=IconKind::Sparkle size=14/></div>
                            <div>
                                <div class="title">"餐饮超预算 8%"</div>
                                <div class="meta">"建议本周在家用餐 3 次，预计节省 ¥240"</div>
                            </div>
                        </div>
                        <div class="list-row">
                            <div class="icon-tile"><Icon kind=IconKind::Link size=14/></div>
                            <div>
                                <div class="title">"健身装备 · 可关联"</div>
                                <div class="meta">"FIN-24084 已链接到 FIT-G-007 装备库"</div>
                            </div>
                        </div>
                        <div class="list-row">
                            <div class="icon-tile"><Icon kind=IconKind::Coin size=14/></div>
                            <div>
                                <div class="title">"可定投 ¥3,000"</div>
                                <div class="meta">"余额宝 → ETF 组合，建议分批"</div>
                            </div>
                        </div>
                    </div>
                </Card>
            </div>
        </div>
    }
}

fn render_budget(d: &LedgerData) -> impl IntoView {
    let cats = d.category_summary.clone();
    view! {
        <div class="grid-2">
            <Card title="预算池" code="FIN-BDG-01" sub="本月 · 元">
                <div class="vstack" style="gap:14px">
                    {cats.into_iter().map(|c| {
                        let bud = match c.code.as_str() {
                            "F&B" => 3200.0, "TRN" => 1600.0, "HLT" => 1200.0,
                            "EDU" => 1500.0, "HSE" => 2000.0, _ => 1500.0,
                        };
                        let pct = if bud > 0.0 { (c.value / bud * 100.0).round() as u32 } else { 0 };
                        let bar_color = if pct > 95 { "var(--rose)".to_string() }
                                        else if c.tone.is_empty() { "var(--primary)".to_string() }
                                        else { format!("var(--{})", c.tone) };
                        let pct_class = if pct > 100 { "amt-neg" } else { "dim" };
                        view! {
                            <div>
                                <div style="display:flex;justify-content:space-between;align-items:baseline;margin-bottom:4px">
                                    <div style="font-size:13px">
                                        <span style="font-weight:500">{c.name.clone()}</span>
                                        <span class="mono dim" style="margin-left:6px;font-size:10.5px">{format!("FIN-B-{}", c.code)}</span>
                                    </div>
                                    <div class="mono" style="font-size:12px">
                                        {format!("¥{} / ¥{} · ", fmt_int(c.value), fmt_int(bud))}
                                        <span class=pct_class>{format!("{}%", pct)}</span>
                                    </div>
                                </div>
                                <div class="bar thick"><span style=format!("width:{}%;background:{}", pct.min(100), bar_color)></span></div>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </Card>
            <Card title="下月规划" code="FIN-BDG-02" sub="草稿 · 基于 3 月均值">
                <div class="placeholder-img" style="min-height:240px">"budget planner · canvas"</div>
            </Card>
        </div>
    }
}

fn render_accounts(d: &LedgerData) -> impl IntoView {
    let accounts = d.accounts.clone();
    view! {
        <div class="grid-3">
            {accounts.into_iter().map(|a| {
                let tone = Tone::from_str(&a.tone);
                view! {
                    <Card title=a.name.clone() code=a.code.clone() sub=a.r#type.clone()>
                        <div class="mono" style="font-size:24px;font-weight:600;letter-spacing:-0.02em">
                            "¥" {fmt_money(a.balance)}
                        </div>
                        <div class="hstack" style="margin-top:10px;gap:10px">
                            <Tag tone=tone>{a.r#type.clone()}</Tag>
                            <span class="mono dim" style="font-size:10.5px">"最近活动 04-22"</span>
                        </div>
                        <div style="margin-top:14px">
                            <ChartBars data=vec![3.0,5.0,4.0,6.0,4.0,7.0,5.0,8.0,6.0,9.0,7.0,10.0,8.0,9.0]/>
                        </div>
                    </Card>
                }
            }).collect_view()}
        </div>
    }
}

fn render_reports() -> impl IntoView {
    view! {
        <div class="grid-2">
            <Card title="月度趋势" code="FIN-RPT-01" sub="12 个月">
                <div class="placeholder-img" style="min-height:240px">"time series chart"</div>
            </Card>
            <Card title="分类分布" code="FIN-RPT-02" sub="12 个月">
                <div class="placeholder-img" style="min-height:240px">"sankey · category flow"</div>
            </Card>
        </div>
    }
}

