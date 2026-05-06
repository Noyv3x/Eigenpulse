use ep_core::IconKind;
use ep_i18n::{t, use_locale};
use ep_ui::{Card, Icon, PageHead, SectionLabel};
use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct ModuleCard {
    pub code: &'static str,
    pub name_key: &'static str,
    pub desc_key: &'static str,
    pub status: ModStatus,
    pub ver: &'static str,
    pub glyph: &'static str,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModStatus {
    On,
    Off,
    Beta,
}
impl ModStatus {
    fn label_key(&self) -> &'static str {
        match self {
            Self::On => "marketplace.status.on",
            Self::Beta => "marketplace.status.beta",
            Self::Off => "marketplace.status.off",
        }
    }
    fn class(&self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Beta => "beta",
            Self::Off => "off",
        }
    }
}

const CARDS: &[ModuleCard] = &[
    ModuleCard {
        code: "DSH",
        name_key: "marketplace.card.dsh.name",
        desc_key: "marketplace.card.dsh.desc",
        status: ModStatus::On,
        ver: "0.1.0",
        glyph: "hub",
    },
    ModuleCard {
        code: "FIN",
        name_key: "marketplace.card.fin.name",
        desc_key: "marketplace.card.fin.desc",
        status: ModStatus::On,
        ver: "0.1.0",
        glyph: "fin",
    },
    ModuleCard {
        code: "FIT",
        name_key: "marketplace.card.fit.name",
        desc_key: "marketplace.card.fit.desc",
        status: ModStatus::On,
        ver: "0.1.0",
        glyph: "fit",
    },
    ModuleCard {
        code: "LRN",
        name_key: "marketplace.card.lrn.name",
        desc_key: "marketplace.card.lrn.desc",
        status: ModStatus::On,
        ver: "0.1.0",
        glyph: "lrn",
    },
    ModuleCard {
        code: "NUT",
        name_key: "marketplace.card.nut.name",
        desc_key: "marketplace.card.nut.desc",
        status: ModStatus::Off,
        ver: "0.0.1",
        glyph: "fin",
    },
    ModuleCard {
        code: "SLP",
        name_key: "marketplace.card.slp.name",
        desc_key: "marketplace.card.slp.desc",
        status: ModStatus::Beta,
        ver: "0.0.1",
        glyph: "lrn",
    },
    ModuleCard {
        code: "TSK",
        name_key: "marketplace.card.tsk.name",
        desc_key: "marketplace.card.tsk.desc",
        status: ModStatus::Off,
        ver: "0.0.1",
        glyph: "mod",
    },
    ModuleCard {
        code: "JRN",
        name_key: "marketplace.card.jrn.name",
        desc_key: "marketplace.card.jrn.desc",
        status: ModStatus::Off,
        ver: "0.0.1",
        glyph: "mod",
    },
    ModuleCard {
        code: "HAB",
        name_key: "marketplace.card.hab.name",
        desc_key: "marketplace.card.hab.desc",
        status: ModStatus::Off,
        ver: "0.0.1",
        glyph: "fit",
    },
    ModuleCard {
        code: "INV",
        name_key: "marketplace.card.inv.name",
        desc_key: "marketplace.card.inv.desc",
        status: ModStatus::Off,
        ver: "0.0.1",
        glyph: "fin",
    },
    ModuleCard {
        code: "TRV",
        name_key: "marketplace.card.trv.name",
        desc_key: "marketplace.card.trv.desc",
        status: ModStatus::Off,
        ver: "0.0.1",
        glyph: "mod",
    },
    ModuleCard {
        code: "REL",
        name_key: "marketplace.card.rel.name",
        desc_key: "marketplace.card.rel.desc",
        status: ModStatus::Off,
        ver: "0.0.1",
        glyph: "lrn",
    },
];

const MATRIX: &[(&str, [u8; 7])] = &[
    ("FIN", [1, 0, 1, 1, 1, 0, 1]),
    ("FIT", [1, 1, 0, 0, 1, 1, 1]),
    ("LRN", [1, 1, 0, 0, 0, 0, 1]),
    ("NUT", [0, 1, 1, 0, 0, 0, 0]),
    ("SLP", [1, 0, 1, 0, 0, 0, 0]),
    ("TSK", [1, 1, 1, 1, 0, 0, 0]),
];
const COLS: &[&str] = &["DSH", "FIN", "FIT", "LRN", "NUT", "SLP", "TSK"];

#[component]
pub fn MarketView() -> impl IntoView {
    let locale = use_locale();
    let active = RwSignal::new(String::from("all"));
    let on_count = CARDS.iter().filter(|c| c.status == ModStatus::On).count();
    let off_count = CARDS.iter().filter(|c| c.status == ModStatus::Off).count();
    let beta_count = CARDS.iter().filter(|c| c.status == ModStatus::Beta).count();

    view! {
        <div class="view">
            <PageHead
                code="MOD-09".to_string()
                module=t(locale, "marketplace.page.module")
                title="Modules".to_string()
                title_cn=t(locale, "marketplace.page.title_cn")
                sub=t(locale, "marketplace.page.sub")
                actions=view! {
                    <>
                        <button class="btn"><Icon kind=IconKind::Cube size=14/>{t(locale, "marketplace.btn.docs")}</button>
                        <button class="btn primary"><Icon kind=IconKind::Plus size=14/>{t(locale, "marketplace.btn.custom")}</button>
                    </>
                }.into_any()
            />

            <div class="module-banner">
                <div class="module-glyph mod mono">"MOD"</div>
                <div style="flex:1">
                    <div class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">{t(locale, "marketplace.arch.caption")}</div>
                    <div style="font-size:20px;font-weight:600;letter-spacing:-0.01em;margin-bottom:4px">
                        {t(locale, "marketplace.arch.title")} " " <span class="serif muted" style="font-size:15px;font-weight:400">{t(locale, "marketplace.arch.extensible")}</span>
                    </div>
                    <div style="font-size:13px;color:var(--ink-3);max-width:560px;line-height:1.5">
                        {t(locale, "marketplace.arch.body_a")} " " <span class="mono">"<KPI · Ledger · Reports · Settings>"</span> " " {t(locale, "marketplace.arch.body_b")}
                        " " {t(locale, "marketplace.arch.body_c")}
                    </div>
                </div>
                <div class="hstack" style="gap:20px">
                    <div style="text-align:center">
                        <div class="mono" style="font-size:22px;font-weight:600">{on_count.to_string()}</div>
                        <div class="mono" style="font-size:10px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">{t(locale, "marketplace.metric.enabled")}</div>
                    </div>
                    <div style="text-align:center">
                        <div class="mono" style="font-size:22px;font-weight:600">{beta_count.to_string()}</div>
                        <div class="mono" style="font-size:10px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">{t(locale, "marketplace.metric.beta")}</div>
                    </div>
                    <div style="text-align:center">
                        <div class="mono" style="font-size:22px;font-weight:600">"∞"</div>
                        <div class="mono" style="font-size:10px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">{t(locale, "marketplace.metric.extensible")}</div>
                    </div>
                </div>
            </div>

            <div class="tabs">
                {[("all","marketplace.tab.all",CARDS.len()),("on","marketplace.tab.on",on_count),("off","marketplace.tab.off",off_count),("beta","marketplace.tab.beta",beta_count)]
                    .into_iter().map(|(id, label, count)| {
                        let id_s = id.to_string();
                        let id_for_class = id_s.clone();
                        let id_for_click = id_s.clone();
                        let class = move || if active.get() == id_for_class { "tab active" } else { "tab" };
                        view! {
                            <button class=class on:click=move |_| active.set(id_for_click.clone())>
                                {t(locale, label)}
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
                                <span class=format!("mkt-status {}", c.status.class())>{t(locale, c.status.label_key())}</span>
                                <div class="mkt-head">
                                    <div class=glyph_class>{c.code}</div>
                                    <div style="flex:1;min-width:0">
                                        <div class="mkt-title">{t(locale, c.name_key)}</div>
                                        <div class="mkt-code">{format!("{}-MODULE · v{}", c.code, c.ver)}</div>
                                    </div>
                                </div>
                                <div class="mkt-desc">{t(locale, c.desc_key)}</div>
                                <div class="module-dna" title=t(locale, "marketplace.dna.title")>
                                    {(0..8).map(|i| view! { <span class={if i < on_cells { "on" } else { "" }}></span> }).collect_view()}
                                </div>
                                <div class="mkt-footer">
                                    <div class="hstack" style="gap:6px">
                                        <Icon kind=IconKind::Link size=11/>
                                        <span class="mono">
                                            {t(locale, if c.status == ModStatus::On { "marketplace.links.on" } else { "marketplace.links.off" })}
                                        </span>
                                    </div>
                                    {match c.status {
                                        ModStatus::On => view! { <button class="btn sm ghost">{t(locale, "marketplace.btn.manage")}</button> }.into_any(),
                                        ModStatus::Beta => view! { <button class="btn sm">{t(locale, "marketplace.btn.join_beta")}</button> }.into_any(),
                                        ModStatus::Off => view! { <button class="btn sm accent"><Icon kind=IconKind::Plus size=12/>{t(locale, "marketplace.btn.enable")}</button> }.into_any(),
                                    }}
                                </div>
                            </div>
                        }
                    }).collect_view()
                }}
                <div class="mkt-card add">
                    <Icon kind=IconKind::Plus size=28/>
                    <div style="margin-top:8px">
                        <div style="font-weight:500;font-size:14px;color:var(--ink-2)">{t(locale, "marketplace.btn.custom")}</div>
                        <div style="font-size:12px;margin-top:4px">{t(locale, "marketplace.custom.desc")}</div>
                    </div>
                </div>
            </div>

            <SectionLabel index="§ 02".to_string()>{t(locale, "marketplace.links.title")}</SectionLabel>

            <Card title=t(locale, "marketplace.matrix.title") code="MOD-MTX-01" sub=t(locale, "marketplace.matrix.sub")>
                <div style="overflow:auto">
                    <table class="tbl" style="min-width:640px">
                        <thead>
                            <tr>
                                <th style="width:80px">{t(locale, "marketplace.matrix.source_target")}</th>
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
