use leptos::prelude::*;

#[component]
pub fn Donut(
    pct: u32,
    #[prop(into, optional)] label: Option<String>,
    #[prop(default = "var(--primary)".to_string())] color: String,
) -> impl IntoView {
    let bg = format!("conic-gradient({color} {pct}%, var(--bg-2) 0)");
    view! {
        <div class="donut" style=format!("background:{bg}")>
            <div class="donut-label">
                <div class="v mono">{pct.to_string()} <small style="font-size:12px;color:var(--ink-3)">"%"</small></div>
                {label.map(|l| view! { <div class="k">{l}</div> })}
            </div>
        </div>
    }
}
