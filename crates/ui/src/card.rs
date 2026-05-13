use leptos::prelude::*;

#[component]
pub fn Card(
    #[prop(into, optional)] title: Option<String>,
    #[prop(into, optional)] code: Option<String>,
    #[prop(into, optional)] sub: Option<String>,
    #[prop(into, optional)] class: Option<String>,
    #[prop(optional)] actions: Option<AnyView>,
    #[prop(optional)] foot: Option<AnyView>,
    children: Children,
) -> impl IntoView {
    let class = format!("card {}", class.unwrap_or_default());
    let title_clone = title.clone();
    // Drop empty strings so an upstream `sub=""` doesn't render a stray
    // `<div class="card-sub">` (used by today/finance to suppress the
    // sub-label when the card is empty).
    let sub = sub.filter(|s| !s.trim().is_empty());
    let has_head = title.is_some() || code.is_some() || sub.is_some() || actions.is_some();
    view! {
        <div class=class>
            {has_head.then(|| view! {
                <div class="card-head">
                    <div>
                        <div class="card-title">
                            {title_clone}
                            {code.map(|c| view! { <span class="code mono">{c}</span> })}
                        </div>
                        {sub.map(|s| view! { <div class="card-sub">{s}</div> })}
                    </div>
                    {actions.map(|a| view! { <div class="hstack" style="gap:6px">{a}</div> })}
                </div>
            })}
            <div class="card-body">{children()}</div>
            {foot.map(|f| view! { <div class="card-foot">{f}</div> })}
        </div>
    }
}
