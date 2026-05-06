use leptos::prelude::*;

#[component]
pub fn ChartBars(
    data: Vec<f64>,
    #[prop(default = None)] highlight: Option<usize>,
    #[prop(default = vec![])] labels: Vec<String>,
) -> impl IntoView {
    let max = data.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
    let bars = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let h = (v / max * 100.0).max(4.0);
            let cls = if Some(i) == highlight {
                "bar-cell hi"
            } else {
                "bar-cell"
            };
            view! { <div class=cls style=format!("height:{h:.1}%")></div> }
        })
        .collect_view();
    let label_row = if labels.is_empty() {
        view! { <div></div> }.into_any()
    } else {
        let cols = format!("repeat({}, 1fr)", labels.len());
        view! {
            <div style=format!("display:grid;grid-template-columns:{cols};gap:3px;font-size:10px;color:var(--ink-4);font-family:var(--font-mono);text-align:center;margin-top:4px")>
                {labels.into_iter().map(|l| view! { <div>{l}</div> }).collect_view()}
            </div>
        }.into_any()
    };
    view! {
        <div>
            <div class="chart-bars">{bars}</div>
            {label_row}
        </div>
    }
}
