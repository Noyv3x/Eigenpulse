use ep_core::IconKind;
use ep_ui::{Card, Icon, PageHead, SectionLabel};
use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct ModuleCard {
    pub code: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub status: ModStatus,
    pub ver: &'static str,
    pub glyph: &'static str,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModStatus { On, Off, Beta }
impl ModStatus {
    fn label(&self) -> &'static str { match self { Self::On => "已启用", Self::Beta => "BETA", Self::Off => "未启用" } }
    fn class(&self) -> &'static str { match self { Self::On => "on", Self::Beta => "beta", Self::Off => "off" } }
}

const CARDS: &[ModuleCard] = &[
    ModuleCard { code: "DSH", name: "Dashboard", desc: "全局指标与今日聚焦",            status: ModStatus::On,  ver: "0.1.0", glyph: "hub" },
    ModuleCard { code: "FIN", name: "财务管理",   desc: "账户、预算、收支、投资组合",     status: ModStatus::On,  ver: "0.1.0", glyph: "fin" },
    ModuleCard { code: "FIT", name: "健身管理",   desc: "训练计划、动作库、身体指标",     status: ModStatus::On,  ver: "0.1.0", glyph: "fit" },
    ModuleCard { code: "LRN", name: "学习管理",   desc: "课程、阅读、笔记、Anki 集成",    status: ModStatus::On,  ver: "0.1.0", glyph: "lrn" },
    ModuleCard { code: "NUT", name: "饮食管理",   desc: "热量、宏量元素、备餐计划",       status: ModStatus::Off, ver: "0.0.1", glyph: "fin" },
    ModuleCard { code: "SLP", name: "睡眠分析",   desc: "睡眠阶段、负荷、恢复建议",       status: ModStatus::Beta,ver: "0.0.1", glyph: "lrn" },
    ModuleCard { code: "TSK", name: "任务 / OKR", desc: "目标拆解、周期复盘",            status: ModStatus::Off, ver: "0.0.1", glyph: "mod" },
    ModuleCard { code: "JRN", name: "日记 / 情绪", desc: "每日心情、结构化反思",           status: ModStatus::Off, ver: "0.0.1", glyph: "mod" },
    ModuleCard { code: "HAB", name: "习惯追踪",   desc: "每日打卡、连续天数、热图",       status: ModStatus::Off, ver: "0.0.1", glyph: "fit" },
    ModuleCard { code: "INV", name: "个人资产盘点",desc: "物品、保修、折旧、定位",         status: ModStatus::Off, ver: "0.0.1", glyph: "fin" },
    ModuleCard { code: "TRV", name: "旅行 / 里程", desc: "行程、签证、里程账户",           status: ModStatus::Off, ver: "0.0.1", glyph: "mod" },
    ModuleCard { code: "REL", name: "人际 / CRM", desc: "联系人、关系维护提醒",           status: ModStatus::Off, ver: "0.0.1", glyph: "lrn" },
];

const MATRIX: &[(&str, [u8; 7])] = &[
    ("FIN", [1,0,1,1,1,0,1]),
    ("FIT", [1,1,0,0,1,1,1]),
    ("LRN", [1,1,0,0,0,0,1]),
    ("NUT", [0,1,1,0,0,0,0]),
    ("SLP", [1,0,1,0,0,0,0]),
    ("TSK", [1,1,1,1,0,0,0]),
];
const COLS: &[&str] = &["DSH","FIN","FIT","LRN","NUT","SLP","TSK"];

#[component]
pub fn MarketView() -> impl IntoView {
    let active = RwSignal::new(String::from("all"));
    let on_count = CARDS.iter().filter(|c| c.status == ModStatus::On).count();
    let off_count = CARDS.iter().filter(|c| c.status == ModStatus::Off).count();
    let beta_count = CARDS.iter().filter(|c| c.status == ModStatus::Beta).count();

    view! {
        <div class="view">
            <PageHead
                code="MOD-09".to_string()
                module="MODULES · 模块市场".to_string()
                title="Modules".to_string()
                title_cn="模块市场"
                sub="系统由模块构成。每个模块是独立的数据域与界面，可随时启用、停用或扩展。"
                actions=view! {
                    <>
                        <button class="btn"><Icon kind=IconKind::Cube size=14/>"开发文档"</button>
                        <button class="btn primary"><Icon kind=IconKind::Plus size=14/>"创建自定义模块"</button>
                    </>
                }.into_any()
            />

            <div class="module-banner">
                <div class="module-glyph mod mono">"MOD"</div>
                <div style="flex:1">
                    <div class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">"架构 / ARCHITECTURE"</div>
                    <div style="font-size:20px;font-weight:600;letter-spacing:-0.01em;margin-bottom:4px">
                        "你的生活操作系统 " <span class="serif muted" style="font-size:15px;font-weight:400">"· 可无限扩展"</span>
                    </div>
                    <div style="font-size:13px;color:var(--ink-3);max-width:560px;line-height:1.5">
                        "每个模块遵循统一的 " <span class="mono">"<KPI · Ledger · Reports · Settings>"</span> " 四段式结构。"
                        "数据通过单号（如 FIN-24091 ↔ FIT-P-002）在模块间相互关联。"
                    </div>
                </div>
                <div class="hstack" style="gap:20px">
                    <div style="text-align:center">
                        <div class="mono" style="font-size:22px;font-weight:600">{on_count.to_string()}</div>
                        <div class="mono" style="font-size:10px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">"已启用"</div>
                    </div>
                    <div style="text-align:center">
                        <div class="mono" style="font-size:22px;font-weight:600">{beta_count.to_string()}</div>
                        <div class="mono" style="font-size:10px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">"Beta"</div>
                    </div>
                    <div style="text-align:center">
                        <div class="mono" style="font-size:22px;font-weight:600">"∞"</div>
                        <div class="mono" style="font-size:10px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">"可扩展"</div>
                    </div>
                </div>
            </div>

            <div class="tabs">
                {[("all","全部",CARDS.len()),("on","已启用",on_count),("off","可添加",off_count),("beta","Beta",beta_count)]
                    .into_iter().map(|(id, label, count)| {
                        let id_s = id.to_string();
                        let id_for_class = id_s.clone();
                        let id_for_click = id_s.clone();
                        let class = move || if active.get() == id_for_class { "tab active" } else { "tab" };
                        view! {
                            <button class=class on:click=move |_| active.set(id_for_click.clone())>
                                {label}
                                <span class="count mono">{count.to_string()}</span>
                            </button>
                        }
                    }).collect_view()}
            </div>

            <div class="mkt-grid">
                {move || {
                    let f = active.get();
                    CARDS.iter().filter(move |c| {
                        match f.as_str() {
                            "all" => true,
                            "on"  => c.status == ModStatus::On,
                            "off" => c.status == ModStatus::Off,
                            "beta"=> c.status == ModStatus::Beta,
                            _ => true,
                        }
                    }).map(|c| {
                        let glyph_class = format!("mkt-glyph module-glyph {}", c.glyph);
                        let on_cells = if c.status == ModStatus::On { 7 } else if c.status == ModStatus::Beta { 4 } else { 3 };
                        view! {
                            <div class="mkt-card">
                                <span class=format!("mkt-status {}", c.status.class())>{c.status.label()}</span>
                                <div class="mkt-head">
                                    <div class=glyph_class>{c.code}</div>
                                    <div style="flex:1;min-width:0">
                                        <div class="mkt-title">{c.name}</div>
                                        <div class="mkt-code">{format!("{}-MODULE · v{}", c.code, c.ver)}</div>
                                    </div>
                                </div>
                                <div class="mkt-desc">{c.desc}</div>
                                <div class="module-dna" title="模块能力指示">
                                    {(0..8).map(|i| view! { <span class={if i < on_cells { "on" } else { "" }}></span> }).collect_view()}
                                </div>
                                <div class="mkt-footer">
                                    <div class="hstack" style="gap:6px">
                                        <Icon kind=IconKind::Link size=11/>
                                        <span class="mono">
                                            {if c.status == ModStatus::On { "已关联 3 模块" } else { "可关联 4 模块" }}
                                        </span>
                                    </div>
                                    {match c.status {
                                        ModStatus::On => view! { <button class="btn sm ghost">"管理"</button> }.into_any(),
                                        ModStatus::Beta => view! { <button class="btn sm">"加入 Beta"</button> }.into_any(),
                                        ModStatus::Off => view! { <button class="btn sm accent"><Icon kind=IconKind::Plus size=12/>"启用"</button> }.into_any(),
                                    }}
                                </div>
                            </div>
                        }
                    }).collect_view()
                }}
                <div class="mkt-card add">
                    <Icon kind=IconKind::Plus size=28/>
                    <div style="margin-top:8px">
                        <div style="font-weight:500;font-size:14px;color:var(--ink-2)">"创建自定义模块"</div>
                        <div style="font-size:12px;margin-top:4px">"使用模板或从零开始 · 自定义字段与关联"</div>
                    </div>
                </div>
            </div>

            <SectionLabel index="§ 02".to_string()>"模块关联 · Inter-Module Links"</SectionLabel>

            <Card title="数据关联矩阵" code="MOD-MTX-01" sub="行 → 列 · 表示有引用关系">
                <div style="overflow:auto">
                    <table class="tbl" style="min-width:640px">
                        <thead>
                            <tr>
                                <th style="width:80px">"源 → 目标"</th>
                                {COLS.iter().map(|c| view! { <th style="text-align:center;width:70px">{*c}</th> }).collect_view()}
                            </tr>
                        </thead>
                        <tbody>
                            {MATRIX.iter().map(|(row, vals)| view! {
                                <tr>
                                    <td class="mono" style="font-weight:500">{*row}</td>
                                    {vals.iter().map(|v| {
                                        let style = if *v == 1 {
                                            "display:inline-block;width:10px;height:10px;border-radius:2px;background:var(--primary)"
                                        } else {
                                            "display:inline-block;width:10px;height:10px;border-radius:2px;background:var(--bg-2);border:1px solid var(--border)"
                                        };
                                        view! { <td style="text-align:center"><span style=style></span></td> }
                                    }).collect_view()}
                                </tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
            </Card>
        </div>
    }
}
