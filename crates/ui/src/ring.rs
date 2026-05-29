use leptos::prelude::*;

#[component]
pub fn Ring(
    pct: u32,
    #[prop(default = 56)] size: u32,
    #[prop(default = 5)] thick: u32,
    #[prop(default = "var(--primary)".to_string())] color: String,
    #[prop(into, optional)] children_text: Option<String>,
) -> impl IntoView {
    // Clamp the rendered fill to 0..=100 so an over-100 input (e.g. a goal
    // exceeded) draws a full ring instead of overshooting the conic-gradient
    // and the "%" label reads "100%" rather than "150%". The geometry sweep
    // is capped; the caller keeps the raw value for any text it supplies.
    let p = pct.min(100);
    let style =
        format!(
        "--p:{p};--s:{s}px;--t:{t}px;background:conic-gradient({c} calc({p} * 1%), var(--bg-2) 0)",
        p = p, s = size, t = thick, c = color
    );
    let label = children_text.unwrap_or_else(|| format!("{p}%"));
    view! {
        <div class="ring" style=style>
            <span>{label}</span>
        </div>
    }
}
