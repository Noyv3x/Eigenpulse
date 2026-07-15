use leptos::prelude::*;

#[derive(Clone, Debug)]
pub struct TabSpec {
    pub id: String,
    pub label: String,
    pub count: Option<u32>,
}
impl TabSpec {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            count: None,
        }
    }
    pub fn with_count(mut self, c: u32) -> Self {
        self.count = Some(c);
        self
    }
}

#[component]
pub fn Tabs(
    tabs: Vec<TabSpec>,
    active: RwSignal<String>,
    #[prop(into)] panel_id: String,
) -> impl IntoView {
    let keyboard_tabs = StoredValue::new(tabs.clone());
    let panel_id = StoredValue::new(panel_id);
    view! {
        <div class="tabs" role="tablist">
            <For
                each=move || tabs.clone()
                key=|t: &TabSpec| t.id.clone()
                children=move |t: TabSpec| {
                    let id = t.id.clone();
                    let id_for_class = id.clone();
                    let id_for_click = id.clone();
                    let id_for_selected = id.clone();
                    let button_id = format!("ep-tab-{id}");
                    let class = move || {
                        if active.get() == id_for_class { "tab active" } else { "tab" }
                    };
                    view! {
                        <button
                            type="button"
                            class=class
                            id=button_id
                            role="tab"
                            aria-selected=move || (active.get() == id_for_selected).to_string()
                            aria-controls=panel_id.get_value()
                            tabindex=move || if active.get() == id { "0" } else { "-1" }
                            on:click=move |_| active.set(id_for_click.clone())
                            on:keydown=move |event: leptos::ev::KeyboardEvent| {
                                let key = event.key();
                                let tabs = keyboard_tabs.get_value();
                                if let Some(next) = next_tab_id(&tabs, &active.get_untracked(), &key) {
                                    event.prevent_default();
                                    active.set(next.clone());
                                    focus_tab(&next);
                                }
                            }
                        >
                            {t.label}
                            {t.count.map(|c| view! { <span class="count mono">{c.to_string()}</span> })}
                        </button>
                    }
                }
            />
        </div>
    }
}

fn next_tab_id(tabs: &[TabSpec], active: &str, key: &str) -> Option<String> {
    if tabs.is_empty() {
        return None;
    }
    let current = tabs.iter().position(|tab| tab.id == active).unwrap_or(0);
    let next = match key {
        "ArrowRight" | "ArrowDown" => (current + 1) % tabs.len(),
        "ArrowLeft" | "ArrowUp" => (current + tabs.len() - 1) % tabs.len(),
        "Home" => 0,
        "End" => tabs.len() - 1,
        _ => return None,
    };
    Some(tabs[next].id.clone())
}

fn focus_tab(id: &str) {
    #[cfg(feature = "hydrate")]
    {
        use wasm_bindgen::JsCast as _;
        if let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&format!("ep-tab-{id}")))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = element.focus();
        }
    }
    #[cfg(not(feature = "hydrate"))]
    let _ = id;
}

#[cfg(test)]
mod tests {
    use super::{next_tab_id, TabSpec};

    #[test]
    fn keyboard_navigation_wraps_and_supports_home_end() {
        let tabs = vec![TabSpec::new("a", "A"), TabSpec::new("b", "B")];
        assert_eq!(next_tab_id(&tabs, "a", "ArrowLeft").as_deref(), Some("b"));
        assert_eq!(next_tab_id(&tabs, "b", "ArrowRight").as_deref(), Some("a"));
        assert_eq!(next_tab_id(&tabs, "b", "Home").as_deref(), Some("a"));
        assert_eq!(next_tab_id(&tabs, "a", "End").as_deref(), Some("b"));
        assert_eq!(next_tab_id(&tabs, "a", "Enter"), None);
    }
}
