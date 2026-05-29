use leptos::prelude::*;

#[component]
pub fn ChartBars(
    data: Vec<f64>,
    #[prop(default = vec![])] labels: Vec<String>,
    /// Extra class on the `.chart-bars` element, e.g. `"expense"` to recolour
    /// the bars amber. Defaults to none (the standard green series).
    #[prop(into, optional)]
    class: Option<String>,
) -> impl IntoView {
    let max = chart_max(&data);
    let bars = data
        .iter()
        .map(|v| {
            let h = bar_height(*v, max);
            view! { <div class="bar-cell" style=format!("height:{h:.1}%")></div> }
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
    let bars_class = match class {
        Some(c) if !c.trim().is_empty() => format!("chart-bars {c}"),
        _ => "chart-bars".to_string(),
    };
    view! {
        <div>
            <div class=bars_class>{bars}</div>
            {label_row}
        </div>
    }
}

fn chart_max(data: &[f64]) -> f64 {
    data.iter()
        .copied()
        .filter(|v| v.is_finite() && *v > 0.0)
        .fold(1.0_f64, f64::max)
}

fn bar_height(value: f64, max: f64) -> f64 {
    if !value.is_finite() || !max.is_finite() || max <= 0.0 {
        return 4.0;
    }
    (value.max(0.0) / max * 100.0).clamp(4.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::{bar_height, chart_max};

    #[test]
    fn chart_max_ignores_non_positive_and_non_finite_values() {
        assert_eq!(chart_max(&[-5.0, 0.0, f64::NAN, f64::INFINITY]), 1.0);
        assert_eq!(chart_max(&[2.0, 8.0, f64::NAN]), 8.0);
    }

    #[test]
    fn bar_height_is_always_a_finite_percentage() {
        assert_eq!(bar_height(f64::NAN, 10.0), 4.0);
        assert_eq!(bar_height(-5.0, 10.0), 4.0);
        assert_eq!(bar_height(5.0, 10.0), 50.0);
        assert_eq!(bar_height(15.0, 10.0), 100.0);
    }
}
