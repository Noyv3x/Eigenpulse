use ep_core::IconKind;
use ep_i18n::{t, use_locale};
use ep_ui::{Card, Icon, NavItem, PageHead, NAV};
use leptos::prelude::*;
use leptos_router::components::A;

/// Marketplace-only card metadata, keyed off a NAV-derived module identity.
///
/// The set of *installed* modules is derived from `ep_ui::NAV` (which already
/// enumerates the registered modules' navigation entries) so the marketplace
/// cannot drift out of sync with what is actually navigable. NAV supplies the
/// identity (`code` + `path`); this table only supplies presentation metadata
/// (description, glyph, version) that NAV does not carry. A card is rendered
/// only when both a NAV entry *and* a metadata row exist for the same `code`,
/// so the marketplace can never list a module that isn't navigable.
#[derive(Clone, Copy)]
struct CardMeta {
    /// Matches `NavItem::code` — the NAV-derived identity this metadata keys off.
    code: &'static str,
    /// Marketplace's own display name key (kept distinct from the NAV label so
    /// the rendered card name is unchanged from the previous hardcoded list).
    name_key: &'static str,
    desc_key: &'static str,
    ver: &'static str,
    glyph: &'static str,
}

/// Presentation metadata for the NAV-derived installed modules.
///
/// Order here does not matter — cards are emitted in NAV order. Keeping a code
/// out of this table simply hides its card (e.g. core views like DSH/TDY and
/// system views like RPT/CFG are navigable but are not "installed modules").
const INSTALLED_META: &[CardMeta] = &[
    CardMeta {
        code: "FIN",
        name_key: "marketplace.card.fin.name",
        desc_key: "marketplace.card.fin.desc",
        ver: "0.1.0",
        glyph: "fin",
    },
    CardMeta {
        code: "FIT",
        name_key: "marketplace.card.fit.name",
        desc_key: "marketplace.card.fit.desc",
        ver: "0.1.0",
        glyph: "fit",
    },
    CardMeta {
        code: "LRN",
        name_key: "marketplace.card.lrn.name",
        desc_key: "marketplace.card.lrn.desc",
        ver: "0.1.0",
        glyph: "lrn",
    },
    CardMeta {
        code: "MOD",
        name_key: "marketplace.card.mod.name",
        desc_key: "marketplace.card.mod.desc",
        ver: "0.1.0",
        glyph: "mod",
    },
];

/// A NAV-derived installed-module card: NAV identity (`code`/`path`) joined with
/// the marketplace's presentation metadata.
#[derive(Clone, Copy)]
struct InstalledCard {
    code: &'static str,
    path: &'static str,
    name_key: &'static str,
    desc_key: &'static str,
    ver: &'static str,
    glyph: &'static str,
}

/// Walk NAV (the single source of truth for what is navigable) and join each
/// entry to its marketplace metadata. Entries without metadata are skipped, so
/// the result is the intersection of "navigable" and "has marketplace card" —
/// never a card for a non-navigable module.
fn installed_cards() -> Vec<InstalledCard> {
    NAV.iter()
        .filter_map(|nav: &NavItem| {
            INSTALLED_META
                .iter()
                .find(|m| m.code == nav.code)
                .map(|m| InstalledCard {
                    code: nav.code,
                    path: nav.path,
                    name_key: m.name_key,
                    desc_key: m.desc_key,
                    ver: m.ver,
                    glyph: m.glyph,
                })
        })
        .collect()
}

#[component]
pub fn MarketView() -> impl IntoView {
    let locale = use_locale();
    let cards = installed_cards();
    let installed_count = cards.len();

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
                    {cards.into_iter().map(|c| {
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

#[cfg(test)]
mod tests {
    use super::{installed_cards, INSTALLED_META};
    use ep_ui::NAV;

    /// Every marketplace card metadata row must correspond to a real NAV entry,
    /// otherwise the marketplace could advertise a module that isn't navigable.
    #[test]
    fn every_card_meta_maps_to_a_nav_entry() {
        for m in INSTALLED_META {
            assert!(
                NAV.iter().any(|n| n.code == m.code),
                "marketplace card meta {} has no matching NAV entry",
                m.code
            );
        }
    }

    /// The derived list is exactly the NAV-order intersection of NAV and the
    /// metadata table — preserving the previous {FIN, FIT, LRN, MOD} ordering.
    #[test]
    fn installed_cards_are_nav_order_and_match_registered_modules() {
        let codes: Vec<&str> = installed_cards().iter().map(|c| c.code).collect();
        assert_eq!(codes, vec!["FIN", "FIT", "LRN", "MOD"]);
    }
}
