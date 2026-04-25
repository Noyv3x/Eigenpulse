use ep_ui::{Card, Kpi, kpi::Direction, PageHead};
use leptos::prelude::*;

#[component]
pub fn TodayView() -> impl IntoView {
    view! {
        <div class="view">
            <PageHead
                code="TDY-01".to_string()
                module="TODAY · 今日聚焦".to_string()
                title="Today".to_string()
                title_cn="今日 · 2026-04-25"
                sub="来自各模块的今日事项 · 按时间排序"
            />
            <div class="kpi-grid">
                <Kpi code="TDY-01" label="今日待办" value="6/8".to_string() delta="2 已完成".to_string() dir=Direction::Up/>
                <Kpi code="TDY-02" label="今日预算" value="¥612".to_string() delta="剩余 ¥388".to_string() dir=Direction::Flat/>
                <Kpi code="TDY-03" label="今日训练" value="待完成".to_string() delta="推日 A · 60min".to_string() dir=Direction::Flat/>
                <Kpi code="TDY-04" label="今日学习" value="2h".to_string() delta="目标 2h 40m".to_string() dir=Direction::Down/>
            </div>
            <Card title="今日时间线" code="TDY-LN-01" sub="按小时分布 · 点击条目跳转至源模块">
                <div class="today-list">
                    {today_seed().into_iter().map(|(time, kind, text, doc_ref)| {
                        let cls = format!("today-item {}", kind);
                        view! {
                            <div class=cls>
                                <span class="time mono">{time}</span>
                                <span class="mark"></span>
                                <div><div class="text">{text}</div></div>
                                <span class="ref mono">{doc_ref}</span>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </Card>
        </div>
    }
}

fn today_seed() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        ("07:00", "done",    "晨跑 · Z2 · 5km",                           "FIT-S-0412"),
        ("09:00", "done",    "每日回顾 · 前一日支出审阅",                  "FIN-R-0425"),
        ("10:30", "pending", "System Design 第 12 章 · 缓存一致性",        "LRN-C-08"),
        ("12:00", "pending", "午餐预算 ¥45 以内",                         "FIN-B-食"),
        ("14:00", "pending", "推日训练 · Push A",                         "FIT-P-0425"),
        ("16:00", "blocked", "体检预约 · 待出报告",                        "HLT-A-04"),
        ("20:00", "pending", "日语 · Anki 60 张",                          "LRN-C-09"),
        ("22:00", "pending", "每日总结 · 日记",                            "JRN-D-0425"),
    ]
}
