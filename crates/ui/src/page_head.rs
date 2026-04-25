use leptos::prelude::*;

#[component]
pub fn PageHead(
    #[prop(into)] code: String,
    #[prop(into)] module: String,
    #[prop(into)] title: String,
    #[prop(into, optional)] title_cn: Option<String>,
    #[prop(into, optional)] sub: Option<String>,
    #[prop(optional)] actions: Option<AnyView>,
) -> impl IntoView {
    view! {
        <div class="page-head">
            <div class="page-head-left">
                <div class="page-meta">
                    <span class="pill mono">{code}</span>
                    <span>{module}</span>
                </div>
                <h1 class="page-title">
                    <span>{title}</span>
                    {title_cn.map(|t| view! { <span class="serif muted" style="font-size:18px;font-weight:400">"· " {t}</span> })}
                </h1>
                {sub.map(|s| view! { <p class="page-sub">{s}</p> })}
            </div>
            {actions.map(|a| view! { <div class="page-actions">{a}</div> })}
        </div>
    }
}
