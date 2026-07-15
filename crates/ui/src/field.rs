use leptos::prelude::*;

/// A semantic label and optional supporting text for a form control.
#[component]
pub fn Field(
    #[prop(into, optional)] label: Option<String>,
    #[prop(into, optional)] hint: Option<String>,
    #[prop(into, optional)] error: Option<String>,
    #[prop(default = false)] wide: bool,
    children: Children,
) -> impl IntoView {
    let class = if wide {
        "ep-field form-grid-wide"
    } else {
        "ep-field"
    };
    view! {
        <label class=class>
            {label.map(|l| view! { <span class="ep-field-label">{l}</span> })}
            {children()}
            {hint.map(|h| view! { <span class="ep-field-hint">{h}</span> })}
            {error.map(|e| view! { <span class="ep-field-error">{e}</span> })}
        </label>
    }
}
