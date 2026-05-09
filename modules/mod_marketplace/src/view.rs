use ep_core::IconKind;
use ep_i18n::{t, use_locale};
use ep_ui::{Card, Icon, PageHead};
use leptos::prelude::*;
use leptos_router::components::A;

#[derive(Clone, Copy)]
pub struct ModuleCard {
    pub code: &'static str,
    pub name_key: &'static str,
    pub desc_key: &'static str,
    pub ver: &'static str,
    pub glyph: &'static str,
    pub path: &'static str,
}

const CARDS: &[ModuleCard] = &[
    ModuleCard {
        code: "FIN",
        name_key: "marketplace.card.fin.name",
        desc_key: "marketplace.card.fin.desc",
        ver: "0.1.0",
        glyph: "fin",
        path: "/finance",
    },
    ModuleCard {
        code: "FIT",
        name_key: "marketplace.card.fit.name",
        desc_key: "marketplace.card.fit.desc",
        ver: "0.1.0",
        glyph: "fit",
        path: "/fitness",
    },
    ModuleCard {
        code: "LRN",
        name_key: "marketplace.card.lrn.name",
        desc_key: "marketplace.card.lrn.desc",
        ver: "0.1.0",
        glyph: "lrn",
        path: "/learning",
    },
    ModuleCard {
        code: "MOD",
        name_key: "marketplace.card.mod.name",
        desc_key: "marketplace.card.mod.desc",
        ver: "0.1.0",
        glyph: "mod",
        path: "/modules",
    },
];

#[component]
pub fn MarketView() -> impl IntoView {
    let locale = use_locale();
    let installed_count = CARDS.len();

    view! {
        <div class="view">
            <PageHead
                code="MOD-09".to_string()
                module=t(locale, "marketplace.page.module")
                title=t(locale, "marketplace.page.title")
                title_cn=t(locale, "marketplace.page.title_cn")
                sub=t(locale, "marketplace.page.sub")
            />

            <div class="module-banner">
                <div class="module-glyph mod mono">"MOD"</div>
                <div style="flex:1">
                    <div class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em;margin-bottom:6px">{t(locale, "marketplace.arch.caption")}</div>
                    <div style="font-size:20px;font-weight:600;letter-spacing:-0.01em;margin-bottom:4px">
                        {t(locale, "marketplace.arch.title")} " " <span class="serif muted" style="font-size:15px;font-weight:400">{t(locale, "marketplace.arch.extensible")}</span>
                    </div>
                    <div style="font-size:13px;color:var(--ink-3);max-width:560px;line-height:1.5">
                        {t(locale, "marketplace.arch.body_a")} " " <span class="mono">"<Module · Routes · API>"</span> " " {t(locale, "marketplace.arch.body_b")}
                    </div>
                </div>
                <div class="hstack" style="gap:20px">
                    <div style="text-align:center">
                        <div class="mono" style="font-size:22px;font-weight:600">{installed_count.to_string()}</div>
                        <div class="mono" style="font-size:10px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">{t(locale, "marketplace.metric.installed")}</div>
                    </div>
                </div>
            </div>

            <Card title=t(locale, "marketplace.installed.title") code="MOD-INST-01" sub=t(locale, "marketplace.installed.sub")>
                <div class="mkt-grid">
                    {CARDS.iter().map(|c| {
                        let glyph_class = format!("mkt-glyph module-glyph {}", c.glyph);
                        view! {
                            <div class="mkt-card">
                                <span class="mkt-status on">{t(locale, "marketplace.status.installed")}</span>
                                <div class="mkt-head">
                                    <div class=glyph_class>{c.code}</div>
                                    <div style="flex:1;min-width:0">
                                        <div class="mkt-title">{t(locale, c.name_key)}</div>
                                        <div class="mkt-code">{format!("{}-MODULE · v{}", c.code, c.ver)}</div>
                                    </div>
                                </div>
                                <div class="mkt-desc">{t(locale, c.desc_key)}</div>
                                <div class="mkt-footer">
                                    <div class="hstack" style="gap:6px">
                                        <Icon kind=IconKind::Link size=11/>
                                        <span class="mono">{t(locale, "marketplace.links.registered")}</span>
                                    </div>
                                    <A href=c.path attr:class="btn sm ghost">
                                        {t(locale, "marketplace.btn.open")}
                                    </A>
                                </div>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </Card>
        </div>
    }
}
