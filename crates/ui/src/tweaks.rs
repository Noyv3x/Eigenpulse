use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}
impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    pub fn parse(s: &str) -> Self {
        if s == "dark" {
            Self::Dark
        } else {
            Self::Light
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}
impl Density {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Compact => "compact",
        }
    }
    pub fn parse(s: &str) -> Self {
        if s == "compact" {
            Self::Compact
        } else {
            Self::Comfortable
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TweakState {
    pub theme: Theme,
    pub density: Density,
}

impl TweakState {
    pub fn serialize_short(&self) -> String {
        format!("{}:{}", self.theme.as_str(), self.density.as_str())
    }
    pub fn parse_short(s: &str) -> Self {
        let mut parts = s.split(':');
        let theme = Theme::parse(parts.next().unwrap_or("light"));
        let density = Density::parse(parts.next().unwrap_or("comfortable"));
        Self { theme, density }
    }
}

pub fn provide_tweak_state(initial: TweakState) -> RwSignal<TweakState> {
    let s = RwSignal::new(initial);

    #[cfg(feature = "hydrate")]
    {
        // SSR rendered with `initial` (typically Default). theme-init.js
        // already restored the real value to <html> / localStorage before
        // first paint. Mirror that into the signal *post-mount* so reactive
        // closures (button class, icon switch, button title) re-evaluate.
        // A plain `s.set(p)` at this point is too early — views haven't
        // mounted yet, so it just changes the initial value and the SSR
        // DOM stays out of sync. Wrapping in `Effect::new` defers it until
        // after hydrate completes; using `get_untracked` keeps this effect
        // running exactly once.
        Effect::new(move |_: Option<()>| {
            if let Some(persisted) = read_persisted_tweaks() {
                if persisted != s.get_untracked() {
                    s.set(persisted);
                }
            }
        });
        use wasm_bindgen::JsCast;
        Effect::new(move |prev: Option<TweakState>| -> TweakState {
            let v = s.get();
            if prev == Some(v) {
                return v;
            }
            if let Some(win) = web_sys::window() {
                let serialized = v.serialize_short();
                if let Ok(Some(storage)) = win.local_storage() {
                    let _ = storage.set_item("ep.tweaks", &serialized);
                }
                if let Some(doc) = win.document() {
                    let cookie = format!(
                        "ep_tweaks={serialized}; path=/; max-age=31536000; SameSite=Strict"
                    );
                    if let Ok(html_doc) = doc.clone().dyn_into::<web_sys::HtmlDocument>() {
                        let _ = html_doc.set_cookie(&cookie);
                    }
                    if let Some(el) = doc.document_element() {
                        let _ = el.set_attribute("data-theme", v.theme.as_str());
                        let _ = el.set_attribute("data-density", v.density.as_str());
                    }
                }
            }
            v
        });
    }

    provide_context(s);
    s
}

pub fn use_tweaks() -> RwSignal<TweakState> {
    use_context::<RwSignal<TweakState>>().unwrap_or_else(|| RwSignal::new(TweakState::default()))
}

#[cfg(feature = "hydrate")]
fn read_persisted_tweaks() -> Option<TweakState> {
    let win = web_sys::window()?;
    if let Ok(Some(storage)) = win.local_storage() {
        if let Ok(Some(raw)) = storage.get_item("ep.tweaks") {
            if !raw.is_empty() {
                return Some(TweakState::parse_short(&raw));
            }
        }
    }
    // Fallback: theme-init.js parsed cookie + wrote attributes on <html>.
    // Reading them here keeps SSR's pre-paint state and hydrate's signal
    // value in lock-step, so the first Effect run is a no-op rather than a
    // visible flash.
    let doc = win.document()?;
    let el = doc.document_element()?;
    // `Theme::parse` / `Density::parse` already fall back to the variant's
    // Default on unknown input, so passing the missing-attr empty string
    // through them is the same as hard-coding "light"/"comfortable" here —
    // and keeps the default in one place.
    Some(TweakState {
        theme: Theme::parse(el.get_attribute("data-theme").as_deref().unwrap_or("")),
        density: Density::parse(el.get_attribute("data-density").as_deref().unwrap_or("")),
    })
}
