use leptos::prelude::*;

#[derive(Clone, Debug)]
pub struct TabSpec {
    pub id: String,
    pub label: String,
    pub count: Option<u32>,
}
impl TabSpec {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self { id: id.into(), label: label.into(), count: None }
    }
    pub fn with_count(mut self, c: u32) -> Self { self.count = Some(c); self }
}

#[component]
pub fn Tabs(
    tabs: Vec<TabSpec>,
    active: RwSignal<String>,
) -> impl IntoView {
    view! {
        <div class="tabs">
            <For
                each=move || tabs.clone()
                key=|t: &TabSpec| t.id.clone()
                children=move |t: TabSpec| {
                    let id = t.id.clone();
                    let id_for_class = id.clone();
                    let id_for_click = id.clone();
                    let class = move || {
                        if active.get() == id_for_class { "tab active" } else { "tab" }
                    };
                    view! {
                        <button
                            class=class
                            on:click=move |_| active.set(id_for_click.clone())
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
