use crate::model::Workout;
use crate::server_fns::*;
use ep_core::{IconKind, Tone};
use ep_ui::{Card, Icon, Kpi, kpi::Direction, PageHead, Ring, StatRow, Tag};
use leptos::prelude::*;

#[component]
pub fn FitnessView() -> impl IntoView {
    let workouts = Resource::new(|| (), |_| async { load_fitness().await });
    let add = ServerAction::<AddWorkout>::new();
    let delete = ServerAction::<DeleteWorkout>::new();

    Effect::new(move |prev: Option<()>| {
        add.version().get();
        delete.version().get();
        if prev.is_some() {
            workouts.refetch();
        }
    });

    view! {
        <div class="view">
            <PageHead
                code="FIT-02"
                module="FITNESS · 健身管理"
                title="Fitness"
                title_cn="健身管理"
                sub="训练计划、动作库、恢复指标。与饮食、睡眠、财务装备互联。"
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
                <Card title="新增训练" code="FIT-S-NEW" sub="记录刚完成的一次训练">
                    <ActionForm action=add attr:class="vstack" attr:style="gap:10px">
                        <div style="display:grid;grid-template-columns:2fr 1fr;gap:10px">
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"类型"</span>
                                <input name="kind" required placeholder="力量 · 推日"
                                       style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"计划"</span>
                                <input name="program" placeholder="PPL-5D"
                                       style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono)"/>
                            </label>
                        </div>
                        <div style="display:grid;grid-template-columns:1fr 2fr 1fr;gap:10px">
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"时长 (min)"</span>
                                <input name="duration_m" type="number" min="1" required placeholder="60"
                                       style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono)"/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"负荷 / 距离"</span>
                                <input name="load_text" placeholder="7,840kg 或 5km"
                                       style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono)"/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"强度"</span>
                                <select name="strain" style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)">
                                    <option value="L">"L · 轻"</option>
                                    <option value="M" selected="selected">"M · 中"</option>
                                    <option value="H">"H · 高"</option>
                                </select>
                            </label>
                        </div>
                        <div class="hstack" style="gap:8px">
                            <button class="btn primary" type="submit"><Icon kind=IconKind::Plus size=14/>"记录"</button>
                            <span class="error-slot">
                                {move || add.value().get().and_then(|r| r.err()).map(|e| view! {
                                    <span class="tag rose">{e.to_string()}</span>
                                })}
                            </span>
                        </div>
                    </ActionForm>
                </Card>

                <Card title="身体 · 今日" code="FIT-BIO-01" sub="Wearable · 占位">
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

            <div style="margin-top:24px"></div>

            <Card title="训练记录" code="FIT-SES-01" sub="近 30 次">
                <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:160px">"loading…"</div> }>
                    {move || workouts.get().map(|res| match res {
                        Err(e) => view! { <p>"加载失败 · " {e.to_string()}</p> }.into_any(),
                        Ok(rows) if rows.is_empty() => view! { <p class="muted">"还没有训练记录。先用左侧表单记一次。"</p> }.into_any(),
                        Ok(rows) => render_workouts(rows, delete).into_any(),
                    })}
                </Suspense>
            </Card>
        </div>
    }
}

fn render_workouts(rows: Vec<Workout>, delete: ServerAction<DeleteWorkout>) -> impl IntoView {
    view! {
        <table class="tbl">
            <thead>
                <tr>
                    <th style="width:80px">"日期"</th>
                    <th style="width:120px">"单号"</th>
                    <th>"类型"</th>
                    <th style="width:120px">"计划"</th>
                    <th class="num" style="width:90px">"时长"</th>
                    <th class="num" style="width:120px">"负荷"</th>
                    <th style="width:80px">"强度"</th>
                    <th class="num" style="width:90px">"操作"</th>
                </tr>
            </thead>
            <tbody>
                {rows.into_iter().map(|w| {
                    let doc = w.doc_id.clone();
                    let date = ep_core::fmt_ts_date(Some(w.occurred_at));
                    let strain_tone = match w.strain.as_deref() {
                        Some("H") => Tone::Rose,
                        Some("M") => Tone::Amber,
                        _ => Tone::Green,
                    };
                    let strain_label = w.strain.clone().unwrap_or_default();
                    view! {
                        <tr>
                            <td class="mono dim">{date}</td>
                            <td class="doc">{w.doc_id}</td>
                            <td>{w.kind}</td>
                            <td class="mono dim">{w.program.unwrap_or_default()}</td>
                            <td class="num">{w.duration_m}" min"</td>
                            <td class="num">{w.load_text.unwrap_or_default()}</td>
                            <td><Tag tone=strain_tone>{strain_label}</Tag></td>
                            <td class="num">
                                <span class="row-actions-slot">
                                    <ActionForm action=delete attr:style="display:inline">
                                        <input type="hidden" name="doc_id" value=doc/>
                                        <button class="btn sm" type="submit"
                                                style="color:var(--rose-ink)"
                                                onclick="return confirm('删除该训练？')">"删除"</button>
                                    </ActionForm>
                                </span>
                            </td>
                        </tr>
                    }
                }).collect_view()}
            </tbody>
        </table>
    }
}
