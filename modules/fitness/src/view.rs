use crate::model::Workout;
use crate::server_fns::*;
use ep_core::{IconKind, Tone};
use ep_ui::{Card, ChartBars, Icon, Kpi, kpi::Direction, PageHead, Ring, RowDeleteAction, StatRow, Tag};
use leptos::prelude::*;

#[component]
pub fn FitnessView() -> impl IntoView {
    let data = Resource::new(|| (), |_| async { load_fitness().await });
    let add = ServerAction::<AddWorkout>::new();
    let delete = ServerAction::<DeleteWorkout>::new();

    Effect::new(move |prev: Option<()>| {
        add.version().get();
        delete.version().get();
        if prev.is_some() {
            data.refetch();
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

            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:200px">"loading…"</div> }>
                {move || data.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">"加载失败 · " {e.to_string()}</div></div> }.into_any(),
                    Ok(d) => render_fitness(d, add, delete).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_fitness(d: FitnessData, add: ServerAction<AddWorkout>, delete: ServerAction<DeleteWorkout>) -> impl IntoView {
    let workouts = d.workouts;
    let s = d.summary;

    let week_pct = if s.this_week_target > 0 {
        ((s.this_week_count as f32 / s.this_week_target as f32) * 100.0).min(100.0) as u32
    } else { 0 };
    let last_load = s.weekly_load.last().copied().unwrap_or(0.0);
    let prev_load = if s.weekly_load.len() >= 2 {
        s.weekly_load[s.weekly_load.len() - 2]
    } else { 0.0 };
    let load_delta_pct = if prev_load > 0.0 {
        ((last_load - prev_load) / prev_load * 100.0).round() as i32
    } else { 0 };
    let load_dir = if load_delta_pct > 0 { Direction::Up }
                   else if load_delta_pct < 0 { Direction::Down }
                   else { Direction::Flat };
    let load_delta_text = if prev_load > 0.0 {
        format!("{:+}% vs 上周", load_delta_pct)
    } else {
        "尚无上周对照".to_string()
    };
    let aerobic_target = 150_u32;
    let aerobic_pct_str = if s.aerobic_min_this_week >= aerobic_target {
        "已达成 150min".to_string()
    } else {
        format!("距 {}min 还差 {}", aerobic_target,
                aerobic_target.saturating_sub(s.aerobic_min_this_week))
    };
    let aerobic_dir = if s.aerobic_min_this_week >= aerobic_target { Direction::Up } else { Direction::Down };
    let heaviest = s.heaviest_strain.as_deref().and_then(crate::model::Strain::parse);
    let strain_label = heaviest.map(|k| k.as_str().to_string()).unwrap_or_else(|| "—".into());
    let strain_meta = heaviest
        .map(|k| match k {
            crate::model::Strain::H => "高强度",
            crate::model::Strain::M => "中强度",
            crate::model::Strain::L => "轻量",
        })
        .unwrap_or("近 7 天无记录");

    let week_count_text = format!("{}/{}", s.this_week_count, s.this_week_target);
    let weekly_load_data = s.weekly_load.clone();
    let week_labels = s.week_labels.clone();
    let workouts_for_table = workouts;

    view! {
        <div class="module-banner">
            <div class="module-glyph fit mono">"FIT"</div>
            <div style="flex:1">
                <div class="hstack" style="margin-bottom:6px;gap:8px">
                    <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"训练状态 / STATUS"</span>
                    <Tag tone=Tone::Green dot=true>{format!("本周 {}/{}", s.this_week_count, s.this_week_target)}</Tag>
                </div>
                <div style="font-size:22px;font-weight:600;letter-spacing:-0.01em">
                    {format!("近 12 周累计 {} 分钟", s.weekly_load.iter().sum::<f64>().round() as u32)}
                    <span class="mono dim" style="font-size:13px;font-weight:500">{format!(" · 平均 {} min/次", s.avg_duration_min)}</span>
                </div>
                <div class="hstack" style="gap:16px;margin-top:8px;font-size:12.5px;color:var(--ink-3)">
                    <span class="mono"><Icon kind=IconKind::Flame size=12/>{format!(" 连续 {} 天", s.streak_days)}</span>
                    <span class="mono">{format!("本周有氧 {}min", s.aerobic_min_this_week)}</span>
                    <span class="mono">{format!("近 7 天最重 · {}", strain_label)}</span>
                </div>
            </div>
            <div class="hstack" style="gap:16px">
                <div style="text-align:center">
                    <Ring pct=week_pct size=64 children_text=week_count_text/>
                    <div class="mono dim" style="font-size:10px;margin-top:6px;text-transform:uppercase;letter-spacing:0.06em">"周训练"</div>
                </div>
            </div>
        </div>

        <div class="kpi-grid">
            <Kpi code="FIT-K01" label="本周总负荷"
                 value=format!("{}", last_load.round() as u32) unit="min·sf".to_string()
                 delta=load_delta_text dir=load_dir/>
            <Kpi code="FIT-K02" label="本周有氧"
                 value=format!("{}", s.aerobic_min_this_week) unit="min".to_string()
                 delta=aerobic_pct_str
                 dir=aerobic_dir/>
            <Kpi code="FIT-K03" label="平均时长 · 30 天"
                 value=format!("{}", s.avg_duration_min) unit="min".to_string()
                 delta="近 30 天均值".to_string() dir=Direction::Flat/>
            <Kpi code="FIT-K04" label="近 7 天最重"
                 value=strain_label
                 delta=strain_meta.to_string() dir=Direction::Flat/>
        </div>

        <Card title="周负荷趋势" code="FIT-LOAD-01"
              sub=format!("近 12 周 · 加权 (L=0.6 / M=1.0 / H=1.4) · 单位 min·sf")>
            <ChartBars data=weekly_load_data labels=week_labels/>
        </Card>

        <div style="margin-top:20px"></div>

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
                                <option value=crate::model::Strain::L.as_str()>{format!("{} · {}", crate::model::Strain::L.as_str(), crate::model::Strain::L.label_cn())}</option>
                                <option value=crate::model::Strain::M.as_str() selected="selected">{format!("{} · {}", crate::model::Strain::M.as_str(), crate::model::Strain::M.label_cn())}</option>
                                <option value=crate::model::Strain::H.as_str()>{format!("{} · {}", crate::model::Strain::H.as_str(), crate::model::Strain::H.label_cn())}</option>
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

            <Card title="身体 · 今日" code="FIT-BIO-01" sub="Wearable · 占位 · 待接入">
                <div class="vstack" style="gap:0">
                    <StatRow label="睡眠时长" value="占位".to_string()/>
                    <StatRow label="深睡比例" value="占位".to_string()/>
                    <StatRow label="步数"   value="占位".to_string()/>
                    <StatRow label="热量消耗" value="占位".to_string()/>
                    <StatRow label="压力指数" value="占位".to_string()/>
                    <StatRow label="体重"   value="占位".to_string()/>
                </div>
            </Card>
        </div>

        <div style="margin-top:24px"></div>

        <Card title="训练记录" code="FIT-SES-01" sub=format!("共 {} 次 · 近 30 次", workouts_for_table.len())>
            {if workouts_for_table.is_empty() {
                view! { <p class="muted">"还没有训练记录。先用左侧表单记一次。"</p> }.into_any()
            } else {
                view! { {render_workouts(workouts_for_table, delete)} }.into_any()
            }}
        </Card>
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
                    let strain_kind = w.strain.as_deref().and_then(crate::model::Strain::parse);
                    let strain_tone = strain_kind.map(|k| k.tone()).unwrap_or(Tone::None);
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
                                <RowDeleteAction action=delete value=doc confirm="删除该训练？"/>
                            </td>
                        </tr>
                    }
                }).collect_view()}
            </tbody>
        </table>
    }
}
