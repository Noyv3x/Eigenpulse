use ep_core::IconKind;
use ep_ui::{Card, ChartBars, Icon, Kpi, kpi::Direction, PageHead, Ring, Tag};
use leptos::prelude::*;

#[component]
pub fn LearningView() -> impl IntoView {
    view! {
        <div class="view">
            <PageHead
                code="LRN-03".to_string()
                module="LEARNING · 学习管理".to_string()
                title="Learning".to_string()
                title_cn="学习管理"
                sub="课程、书籍、笔记与 Anki 复习。以每周 14 小时为基准。"
                actions=view! {
                    <>
                        <button class="btn"><Icon kind=IconKind::Book size=14/>"添加书籍"</button>
                        <button class="btn"><Icon kind=IconKind::Export size=14/>"笔记导出"</button>
                        <button class="btn primary"><Icon kind=IconKind::Plus size=14/>"新建课程"</button>
                    </>
                }.into_any()
            />

            <div class="module-banner">
                <div class="module-glyph lrn mono">"LRN"</div>
                <div style="flex:1">
                    <div class="hstack" style="margin-bottom:6px;gap:8px">
                        <span class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">"本周学习 / WEEKLY STUDY"</span>
                        <Tag tone=ep_core::Tone::Blue dot=true>"进行中"</Tag>
                    </div>
                    <div style="font-size:22px;font-weight:600;letter-spacing:-0.01em">
                        "12.4 " <span class="mono dim" style="font-size:14px;font-weight:500">"/ 14 小时"</span>
                    </div>
                    <div class="hstack" style="gap:16px;margin-top:8px;font-size:12.5px;color:var(--ink-3)">
                        <span class="mono">"4 门课程"</span>
                        <span class="mono">"4 本书"</span>
                        <span class="mono">"3 条笔记"</span>
                        <span class="mono">"Anki 复习 · 今日 60 张"</span>
                    </div>
                </div>
                <div style="text-align:center">
                    <Ring pct=89 size=80 thick=6 children_text="12.4h".to_string()/>
                </div>
            </div>

            <div class="kpi-grid">
                <Kpi code="LRN-K01" label="本周时长"     value="12.4".to_string() unit="h".to_string() delta="目标 14h · 89%".to_string() dir=Direction::Up/>
                <Kpi code="LRN-K02" label="待复习卡片"   value="60".to_string()                       delta="-18 vs 昨日".to_string()    dir=Direction::Down/>
                <Kpi code="LRN-K03" label="笔记总数"     value="221".to_string()                      delta="+3 本周".to_string()        dir=Direction::Up/>
                <Kpi code="LRN-K04" label="专注时段"     value="2h 40m".to_string()                   delta="平均 · 日".to_string()      dir=Direction::Flat/>
            </div>

            <div class="grid-2">
                <Card title="进行中的课程" code="LRN-CRS-01">
                    <div class="placeholder-img" style="min-height:200px">"课程列表 · 后续迭代"</div>
                </Card>
                <Card title="本周学习时长" code="LRN-R01" sub="按日 · 小时">
                    <ChartBars data=vec![1.2, 2.4, 1.8, 2.6, 1.4, 2.0, 1.0]
                               labels=vec!["一","二","三","四","五","六","日"].into_iter().map(String::from).collect()
                               highlight=Some(2)/>
                </Card>
            </div>
        </div>
    }
}
