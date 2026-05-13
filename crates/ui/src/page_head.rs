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
    // Drop title_cn when it is empty, blank, or identical to the primary
    // title (case-insensitive trim compare). This is the common locale
    // catalog footprint for the English bundle: many `*.title_cn` strings
    // are filled with the same English label as `*.title`, which previously
    // rendered "Finance · Finance".
    let title_cn = title_cn.and_then(|raw| {
        let cn = raw.trim();
        if cn.is_empty() || cn.eq_ignore_ascii_case(title.trim()) {
            None
        } else {
            Some(raw)
        }
    });
    view! {
        <div class="page-head">
            <div class="page-head-left">
                <div class="page-meta">
                    <span class="pill mono">{code}</span>
                    <span>{module}</span>
                </div>
                <h1 class="page-title">
                    <span>{title}</span>
                    {title_cn.map(|t| view! { <span class="title-cn">{t}</span> })}
                </h1>
                {sub.map(|s| view! { <p class="page-sub">{s}</p> })}
            </div>
            {actions.map(|a| view! { <div class="page-actions">{a}</div> })}
        </div>
    }
}
