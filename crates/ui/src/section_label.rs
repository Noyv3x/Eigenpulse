use leptos::prelude::*;

#[component]
pub fn SectionLabel(
    #[prop(optional)] index: Option<String>,
    #[prop(optional)] right: Option<AnyView>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="section-label">
            {index.map(|i| view! { <span class="index">{i}</span> })}
            <span>{children()}</span>
            {right.map(|r| view! { <><span class="spacer" style="flex:1"></span>{r}</> })}
        </div>
    }
}
