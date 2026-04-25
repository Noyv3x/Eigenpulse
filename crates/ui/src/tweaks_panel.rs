use crate::tweaks::{Density, Theme, TweakState, use_tweaks};
use crate::Icon;
use ep_core::IconKind;
use leptos::prelude::*;

#[component]
pub fn TweaksPanel() -> impl IntoView {
    let s = use_tweaks();
    view! {
        <div class="tweaks-panel">
            <div class="tweaks-head">
                <span>"Tweaks"</span>
                <span class="mono">"v0.1"</span>
            </div>
            <div class="tweaks-body">
                <div class="tweak-row">
                    <label>"外观 · THEME"</label>
                    <div class="seg">
                        <button
                            class=move || if s.get().theme == Theme::Light { "on" } else { "" }
                            on:click=move |_| s.update(|v: &mut TweakState| v.theme = Theme::Light)
                        >
                            <Icon kind=IconKind::Sun size=14/> "Light"
                        </button>
                        <button
                            class=move || if s.get().theme == Theme::Dark { "on" } else { "" }
                            on:click=move |_| s.update(|v: &mut TweakState| v.theme = Theme::Dark)
                        >
                            <Icon kind=IconKind::Moon size=14/> "Dark"
                        </button>
                    </div>
                </div>
                <div class="tweak-row">
                    <label>"密度 · DENSITY"</label>
                    <div class="seg">
                        <button
                            class=move || if s.get().density == Density::Comfortable { "on" } else { "" }
                            on:click=move |_| s.update(|v: &mut TweakState| v.density = Density::Comfortable)
                        >"宽松"</button>
                        <button
                            class=move || if s.get().density == Density::Compact { "on" } else { "" }
                            on:click=move |_| s.update(|v: &mut TweakState| v.density = Density::Compact)
                        >"紧凑"</button>
                    </div>
                </div>
            </div>
        </div>
    }
}
