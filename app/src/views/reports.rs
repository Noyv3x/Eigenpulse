use ep_ui::{Card, PageHead};
use leptos::prelude::*;

#[component]
pub fn ReportsView() -> impl IntoView {
    view! {
        <div class="view">
            <PageHead
                code="RPT-01".to_string()
                module="REPORTS · 报表中心".to_string()
                title="Reports".to_string()
                title_cn="报表中心"
                sub="跨模块交叉报表 · 周报 / 月报 / 年报"
            />
            <div class="grid-3">
                {["周报 · Weekly","月报 · Monthly","年度回顾 · Annual"].into_iter().enumerate().map(|(i, t)| {
                    let code = format!("RPT-0{}", i+1);
                    view! {
                        <Card title=t.to_string() code=code sub="自动生成 · 可导出 PDF">
                            <div class="placeholder-img" style="min-height:180px">"report preview"</div>
                        </Card>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}
