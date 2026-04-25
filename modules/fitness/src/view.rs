use ep_core::{IconKind, Tone};
use ep_ui::{Card, Icon, Kpi, kpi::Direction, PageHead, Ring, StatRow, Tag};
use leptos::prelude::*;

#[component]
pub fn FitnessView() -> impl IntoView {
    view! {
        <div class="view">
            <PageHead
                code="FIT-02".to_string()
                module="FITNESS · 健身管理".to_string()
                title="Fitness".to_string()
                title_cn="健身管理"
                sub="训练计划、动作库、恢复指标。与饮食、睡眠、财务装备互联。"
                actions=view! {
                    <>
                        <button class="btn"><Icon kind=IconKind::Heart size=14/>"身体指标"</button>
                        <button class="btn"><Icon kind=IconKind::Export size=14/>"周报"</button>
                        <button class="btn primary"><Icon kind=IconKind::Plus size=14/>"开始训练"</button>
                    </>
                }.into_any()
            />

            <div class="module-banner">
                <div class="module-glyph fit mono">"FIT"</div>
                <div style="flex:1">
                    <div class="hstack" style="margin-bottom:6px;gap:8px">
                        <span class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">"当前计划 / PROGRAM"</span>
                        <Tag tone=Tone::Green dot=true>"进行中 · Week 4/8"</Tag>
                    </div>
                    <div style="font-size:22px;font-weight:600;letter-spacing:-0.01em">
                        "Push · Pull · Legs " <span class="mono dim" style="font-size:13px;font-weight:500">"· PPL-5D · 力量主导"</span>
                    </div>
                    <div class="hstack" style="gap:16px;margin-top:8px;font-size:12.5px;color:var(--ink-3)">
                        <span class="mono"><Icon kind=IconKind::Flame size=12/>" 连续 14 天"</span>
                        <span class="mono">"本周 5/6 次"</span>
                        <span class="mono">"VO₂max 46.2"</span>
                    </div>
                </div>
                <div class="hstack" style="gap:16px">
                    <div style="text-align:center">
                        <Ring pct=83 size=64 children_text="5/6".to_string()/>
                        <div class="mono" style="font-size:10px;color:var(--ink-3);margin-top:6px;text-transform:uppercase;letter-spacing:0.06em">"周训练"</div>
                    </div>
                    <div style="text-align:center">
                        <Ring pct=74 size=64 color="var(--amber)".to_string() children_text="74".to_string()/>
                        <div class="mono" style="font-size:10px;color:var(--ink-3);margin-top:6px;text-transform:uppercase;letter-spacing:0.06em">"恢复度"</div>
                    </div>
                    <div style="text-align:center">
                        <Ring pct=88 size=64 color="var(--blue)".to_string() children_text="88".to_string()/>
                        <div class="mono" style="font-size:10px;color:var(--ink-3);margin-top:6px;text-transform:uppercase;letter-spacing:0.06em">"睡眠分"</div>
                    </div>
                </div>
            </div>

            <div class="kpi-grid">
                <Kpi code="FIT-K01" label="本周总负荷" value="26.4".to_string() unit="t".to_string() delta="+8% vs 上周".to_string() dir=Direction::Up/>
                <Kpi code="FIT-K02" label="本周有氧"  value="73".to_string()  unit="min".to_string() delta="目标 150min".to_string() dir=Direction::Down/>
                <Kpi code="FIT-K03" label="静息心率"  value="58".to_string()  unit="bpm".to_string() delta="-3 vs 均值".to_string() dir=Direction::Down/>
                <Kpi code="FIT-K04" label="HRV"      value="58".to_string()  unit="ms".to_string()  delta="+6 vs 均值".to_string() dir=Direction::Up/>
            </div>

            <div class="grid-2">
                <Card title="今日训练计划" code="FIT-S-0422" sub="PPL-5D · Week 4 · Day 1">
                    <div class="placeholder-img" style="min-height:200px">"训练详情 · 后续迭代"</div>
                </Card>
                <Card title="身体 · 今日" code="FIT-BIO-01" sub="Wearable · 04-22">
                    <div class="vstack" style="gap:0">
                        <StatRow label="睡眠时长" value="7.4h · 目标 8h".to_string()/>
                        <StatRow label="深睡比例" value="22%".to_string()/>
                        <StatRow label="步数"   value="8,421 步".to_string()/>
                        <StatRow label="热量消耗" value="2,340 kcal".to_string()/>
                        <StatRow label="压力指数" value="低 · 24".to_string()/>
                        <StatRow label="体重"   value="74.2 kg · −0.3".to_string()/>
                    </div>
                </Card>
            </div>
        </div>
    }
}
