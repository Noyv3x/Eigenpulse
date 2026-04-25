use leptos::prelude::*;

#[component]
pub fn StatRow(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
) -> impl IntoView {
    view! {
        <div class="stat-row">
            <span class="stat-label">{label}</span>
            <span class="stat-value">{value}</span>
        </div>
    }
}
