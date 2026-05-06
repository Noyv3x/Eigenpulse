use leptos::prelude::*;

#[component]
pub fn Ring(
    pct: u32,
    #[prop(default = 56)] size: u32,
    #[prop(default = 5)] thick: u32,
    #[prop(default = "var(--primary)".to_string())] color: String,
    #[prop(into, optional)] children_text: Option<String>,
) -> impl IntoView {
    let style =
        format!(
        "--p:{p};--s:{s}px;--t:{t}px;background:conic-gradient({c} calc({p} * 1%), var(--bg-2) 0)",
        p = pct, s = size, t = thick, c = color
    );
    let label = children_text.unwrap_or_else(|| format!("{pct}%"));
    view! {
        <div class="ring" style=style>
            <span>{label}</span>
        </div>
    }
}
