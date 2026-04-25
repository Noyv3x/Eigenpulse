use ep_core::{AppState, DashboardWidget, WidgetKind};
use leptos::prelude::*;

pub static WIDGETS: &[DashboardWidget] = &[
    DashboardWidget { code: "FIN-K01", kind: WidgetKind::Kpi, render: render_savings },
    DashboardWidget { code: "FIN-K02", kind: WidgetKind::Kpi, render: render_budget_pct },
];

fn render_savings(_state: AppState) -> AnyView {
    use ep_ui::{Kpi, kpi::Direction};
    view! {
        <Kpi code="FIN-K01" label="月度结余" value="¥8,788".to_string()
             delta="+18.4% vs 上月".to_string() dir=Direction::Up/>
    }.into_any()
}

fn render_budget_pct(_state: AppState) -> AnyView {
    use ep_ui::{Kpi, kpi::Direction};
    view! {
        <Kpi code="FIN-K02" label="预算使用率" value="69".to_string() unit="%".to_string()
             delta="¥4,388 剩余".to_string() dir=Direction::Flat/>
    }.into_any()
}
