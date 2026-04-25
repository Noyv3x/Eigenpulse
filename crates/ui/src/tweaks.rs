use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}
impl Theme {
    pub fn as_str(&self) -> &'static str { match self { Self::Light => "light", Self::Dark => "dark" } }
    pub fn parse(s: &str) -> Self { if s == "dark" { Self::Dark } else { Self::Light } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}
impl Density {
    pub fn as_str(&self) -> &'static str { match self { Self::Comfortable => "comfortable", Self::Compact => "compact" } }
    pub fn parse(s: &str) -> Self { if s == "compact" { Self::Compact } else { Self::Comfortable } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TweakState {
    pub theme: Theme,
    pub density: Density,
}

impl TweakState {
    pub fn serialize_short(&self) -> String { format!("{}:{}", self.theme.as_str(), self.density.as_str()) }
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
        Effect::new(move |prev: Option<TweakState>| -> TweakState {
            let v = s.get();
            if prev == Some(v) { return v; }
            if let Some(win) = web_sys::window() {
                let serialized = v.serialize_short();
                if let Ok(Some(storage)) = win.local_storage() {
                    let _ = storage.set_item("ep.tweaks", &serialized);
                }
                if let Some(doc) = win.document() {
                    let cookie = format!("ep_tweaks={serialized}; path=/; max-age=31536000; SameSite=Strict");
                    let _ = doc.set_cookie(&cookie);
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
    use_context::<RwSignal<TweakState>>()
        .expect("provide_tweak_state must be called in <App/>")
}
