pub mod notifications;
pub mod security;

// `server_err` lives in `ep_core` now — single source for both `app` views and
// module crates. Re-export so existing `super::server_err` paths still resolve.
pub use ep_core::server_err;

use ep_i18n::{t, use_locale};
use ep_ui::tweaks::{use_tweaks, Density, TweakState};
use ep_ui::{Card, PageHead, StatRow};
use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn SettingsIndex() -> impl IntoView {
    let tweaks = use_tweaks();
    let locale = use_locale();
    view! {
        <div class="view">
            <PageHead
                code="CFG-01"
                module=t(locale, "app.settings.index.page.module")
                title="Settings"
                title_cn=t(locale, "app.settings.index.page.title_cn")
            />
            <div class="grid-2">
                <Card title=t(locale, "app.settings.index.account_card.title") code="CFG-ACC">
                    <div class="vstack" style="gap:0">
                        <StatRow label=t(locale, "app.settings.index.user") value="Leo Chen · UID-0001".to_string()/>
                        <StatRow label=t(locale, "app.settings.index.role") value="OWNER".to_string()/>
                        <StatRow label=t(locale, "app.settings.index.data_count") value="—".to_string()/>
                    </div>
                </Card>
                <Card title=t(locale, "app.settings.index.data_card.title") code="CFG-DATA">
                    <div class="vstack" style="gap:0">
                        <StatRow label=t(locale, "app.settings.index.data_card.storage") value="data/eigenpulse.db".to_string()/>
                        <StatRow label=t(locale, "app.settings.index.data_card.backup") value=t(locale, "app.settings.index.unconfigured").to_string()/>
                        <StatRow label=t(locale, "app.settings.index.data_card.sync") value=t(locale, "app.settings.index.data_card.local").to_string()/>
                    </div>
                </Card>
                <Card title=t(locale, "app.settings.index.notify_card.title") code="CFG-NOT" sub="SMTP / Bark / Telegram / Discord">
                    <p class="muted">{t(locale, "app.settings.index.notify_card.hint_a")} " " <A href="/settings/notifications">{t(locale, "app.settings.index.notify_card.link")}</A> " " {t(locale, "app.settings.index.notify_card.hint_b")}</p>
                </Card>
                <Card title=t(locale, "app.settings.index.api_card.title") code="CFG-SEC" sub=t(locale, "app.settings.index.api_card.sub")>
                    <p class="muted">{t(locale, "app.settings.index.api_card.hint_a")} " " <A href="/settings/security">{t(locale, "app.settings.index.api_card.link")}</A> " " {t(locale, "app.settings.index.api_card.hint_b")}</p>
                </Card>
                <Card title=t(locale, "app.settings.index.ui_card.title") code="CFG-UI" sub=t(locale, "app.settings.index.ui_card.sub")>
                    <div class="tweak-row">
                        <label>{t(locale, "app.settings.index.density_label")}</label>
                        <div class="seg">
                            <button
                                class=move || if tweaks.get().density == Density::Comfortable { "on" } else { "" }
                                on:click=move |_| tweaks.update(|v: &mut TweakState| v.density = Density::Comfortable)
                            >{t(locale, "app.settings.index.density_comfortable")}</button>
                            <button
                                class=move || if tweaks.get().density == Density::Compact { "on" } else { "" }
                                on:click=move |_| tweaks.update(|v: &mut TweakState| v.density = Density::Compact)
                            >{t(locale, "app.settings.index.density_compact")}</button>
                        </div>
                    </div>
                </Card>
            </div>
        </div>
    }
}
