use leptos::prelude::*;

/// `data` is intensity 0..=4. Renders 7-row × N-col grid (CSS auto-flows).
#[component]
pub fn Heatmap(data: Vec<u8>) -> impl IntoView {
    let cells = data
        .into_iter()
        .map(|lvl| {
            let cls = if lvl == 0 {
                "c".to_string()
            } else {
                format!("c l{lvl}")
            };
            view! { <div class=cls></div> }
        })
        .collect_view();
    view! { <div class="heatmap">{cells}</div> }
}
