use crate::model::Workout;
use crate::server_fns::*;
use ep_core::{IconKind, Tone};
use ep_i18n::{server_fn_error_text, t, tf, use_locale};
use ep_ui::{
    Card, ChartBars, Direction, ErrorSlot, Icon, Kpi, LoadError, PageHead, Ring, RowDeleteAction,
    SkeletonCard, SkeletonKpi, Tag, FIELD_LABEL, INPUT_STYLE, INPUT_STYLE_MONO,
};
use leptos::prelude::*;

#[component]
pub fn FitnessView() -> impl IntoView {
    let locale = use_locale();
    let data = Resource::new(|| (), |_| async { load_fitness().await });
    let add = ServerAction::<AddWorkout>::new();
    let update = ServerAction::<UpdateWorkout>::new();
    let delete = ServerAction::<DeleteWorkout>::new();

    Effect::new(move |prev: Option<()>| {
        add.version().get();
        update.version().get();
        delete.version().get();
        if prev.is_some() {
            data.refetch();
        }
    });

    view! {
        <div class="view">
            <PageHead
                code="FIT-02"
                module=t(locale, "fitness.page.module")
                title=t(locale, "fitness.page.title")
                title_cn=t(locale, "fitness.page.title_cn")
                sub=t(locale, "fitness.page.sub")
            />

            <Suspense fallback=move || view! {
                <div style="margin-bottom:20px"><SkeletonCard rows=0/></div>
                <SkeletonKpi count=4/>
                <SkeletonCard rows=2/>
            }>
                {move || data.get().map(|res| match res {
                    Err(e) => view! { <LoadError detail=server_fn_error_text(&e)/> }.into_any(),
                    Ok(d) => render_fitness(d, add, update, delete).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_fitness(
    d: FitnessData,
    add: ServerAction<AddWorkout>,
    update: ServerAction<UpdateWorkout>,
    delete: ServerAction<DeleteWorkout>,
) -> impl IntoView {
    let locale = use_locale();
    let workouts = d.workouts;
    let s = d.summary;

    let week_pct = if s.this_week_target > 0 {
        ((s.this_week_count as f32 / s.this_week_target as f32) * 100.0).min(100.0) as u32
    } else {
        0
    };
    let last_load = s.weekly_load.last().copied().unwrap_or(0.0);
    let prev_load = if s.weekly_load.len() >= 2 {
        s.weekly_load[s.weekly_load.len() - 2]
    } else {
        0.0
    };
    let load_delta_pct = if prev_load > 0.0 {
        ((last_load - prev_load) / prev_load * 100.0).round() as i32
    } else {
        0
    };
    let load_dir = if load_delta_pct > 0 {
        Direction::Up
    } else if load_delta_pct < 0 {
        Direction::Down
    } else {
        Direction::Flat
    };
    let load_delta_text = if prev_load > 0.0 {
        tf(
            locale,
            "fitness.kpi.load_delta",
            &[("pct", &format!("{load_delta_pct:+}"))],
        )
    } else {
        t(locale, "fitness.kpi.load_no_prev").to_string()
    };
    let aerobic_target = s.aerobic_target_min;
    let aerobic_pct_str = if s.aerobic_min_this_week >= aerobic_target {
        t(locale, "fitness.kpi.target_done").to_string()
    } else {
        tf(
            locale,
            "fitness.kpi.target_gap",
            &[
                ("target", &aerobic_target.to_string()),
                (
                    "gap",
                    &aerobic_target
                        .saturating_sub(s.aerobic_min_this_week)
                        .to_string(),
                ),
            ],
        )
    };
    let aerobic_dir = if s.aerobic_min_this_week >= aerobic_target {
        Direction::Up
    } else {
        Direction::Down
    };
    let heaviest = s
        .heaviest_strain
        .as_deref()
        .and_then(crate::model::Strain::parse);
    let strain_label = heaviest
        .map(|k| k.as_str().to_string())
        .unwrap_or_else(|| "—".into());
    let strain_meta = heaviest
        .map(|k| match k {
            crate::model::Strain::H => t(locale, "fitness.strain.h"),
            crate::model::Strain::M => t(locale, "fitness.strain.m"),
            crate::model::Strain::L => t(locale, "fitness.strain.l"),
        })
        .unwrap_or_else(|| t(locale, "fitness.strain.empty"));

    let week_count_text = format!("{}/{}", s.this_week_count, s.this_week_target);
    let weekly_load_data = s.weekly_load.clone();
    let week_labels = s.week_labels.clone();
    let workouts_for_table = workouts;

    view! {
        <div class="module-banner">
            <div class="module-glyph fit mono">"FIT"</div>
            <div style="flex:1">
                <div class="hstack" style="margin-bottom:6px;gap:8px">
                    <span class="mono dim" style=FIELD_LABEL>{t(locale, "fitness.banner.status")}</span>
                    <Tag tone=Tone::Green dot=true>{tf(locale, "fitness.banner.tag", &[("done", &s.this_week_count.to_string()), ("target", &s.this_week_target.to_string())])}</Tag>
                </div>
                <div style="font-size:22px;font-weight:600;letter-spacing:-0.01em">
                    {tf(locale, "fitness.banner.total", &[("min", &(s.weekly_load.iter().sum::<f64>().round() as u32).to_string())])}
                    <span class="mono dim" style="font-size:13px;font-weight:500">{tf(locale, "fitness.banner.avg", &[("min", &s.avg_duration_min.to_string())])}</span>
                </div>
                <div class="hstack" style="gap:16px;margin-top:8px;font-size:12.5px;color:var(--ink-3)">
                    <span class="mono"><Icon kind=IconKind::Flame size=12/>{tf(locale, "fitness.banner.streak", &[("days", &s.streak_days.to_string())])}</span>
                    <span class="mono">{tf(locale, "fitness.banner.aerobic", &[("min", &s.aerobic_min_this_week.to_string())])}</span>
                    <span class="mono">{tf(locale, "fitness.banner.heaviest", &[("strain", &strain_label)])}</span>
                </div>
            </div>
            <div class="hstack" style="gap:16px">
                <div style="text-align:center">
                    <Ring pct=week_pct size=64 children_text=week_count_text/>
                    <div class="mono dim" style="font-size:10px;margin-top:6px;text-transform:uppercase;letter-spacing:0.06em">{t(locale, "fitness.weekly.label")}</div>
                </div>
            </div>
        </div>

        <div class="kpi-grid">
            <Kpi code="FIT-K01" label=t(locale, "fitness.kpi.load")
                 value=format!("{}", last_load.round() as u32) unit="min·sf".to_string()
                 delta=load_delta_text dir=load_dir/>
            <Kpi code="FIT-K02" label=t(locale, "fitness.kpi.aerobic")
                 value=format!("{}", s.aerobic_min_this_week) unit="min".to_string()
                 delta=aerobic_pct_str
                 dir=aerobic_dir/>
            <Kpi code="FIT-K03" label=t(locale, "fitness.kpi.avg_duration")
                 value=format!("{}", s.avg_duration_min) unit="min".to_string()
                 delta=t(locale, "fitness.kpi.avg_duration_delta").to_string() dir=Direction::Flat/>
            <Kpi code="FIT-K04" label=t(locale, "fitness.kpi.heaviest")
                 value=strain_label
                 delta=strain_meta.to_string() dir=Direction::Flat/>
        </div>

        <Card title=t(locale, "fitness.card.load.title") code="FIT-LOAD-01"
              sub=t(locale, "fitness.card.load.sub")>
            <ChartBars data=weekly_load_data labels=week_labels/>
        </Card>

        <div style="margin-top:20px"></div>

        <div class="grid-2">
            <Card title=t(locale, "fitness.card.form.title") code="FIT-S-NEW" sub=t(locale, "fitness.card.form.sub")>
                <ActionForm action=add attr:class="vstack" attr:style="gap:10px">
                    <div style="display:grid;grid-template-columns:140px 2fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "fitness.field.date")}</span>
                            <input name="occurred_on" type="date"
                                   title=t(locale, "fitness.placeholder.date_now")
                                   style=INPUT_STYLE_MONO/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "fitness.field.kind")}</span>
                            <input name="kind" required maxlength=MAX_WORKOUT_KIND_CHARS.to_string()
                                   placeholder=t(locale, "fitness.placeholder.kind")
                                   style=INPUT_STYLE/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "fitness.field.plan")}</span>
                            <input name="program" maxlength=MAX_WORKOUT_PROGRAM_CHARS.to_string()
                                   placeholder="PPL-5D"
                                   style=INPUT_STYLE_MONO/>
                        </label>
                    </div>
                    <div style="display:grid;grid-template-columns:1fr 2fr 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "fitness.field.duration")}</span>
                            <input name="duration_m" type="number" min="1" max=MAX_WORKOUT_DURATION_MINUTES.to_string() required placeholder="60"
                                   style=INPUT_STYLE_MONO/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "fitness.field.load")}</span>
                            <input name="load_text" maxlength=MAX_WORKOUT_LOAD_TEXT_CHARS.to_string()
                                   placeholder=t(locale, "fitness.placeholder.load")
                                   style=INPUT_STYLE_MONO/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "fitness.field.strain")}</span>
                            <select name="strain" style=INPUT_STYLE>
                                <option value=crate::model::Strain::L.as_str()>{format!("{} · {}", crate::model::Strain::L.as_str(), t(locale, "fitness.strain.l"))}</option>
                                <option value=crate::model::Strain::M.as_str() selected="selected">{format!("{} · {}", crate::model::Strain::M.as_str(), t(locale, "fitness.strain.m"))}</option>
                                <option value=crate::model::Strain::H.as_str()>{format!("{} · {}", crate::model::Strain::H.as_str(), t(locale, "fitness.strain.h"))}</option>
                            </select>
                        </label>
                    </div>
                    <div style="display:grid;grid-template-columns:120px 1fr;gap:10px">
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "fitness.field.rpe")}</span>
                            <input name="rpe" type="number" min="1" max="10" placeholder="7"
                                   style=INPUT_STYLE_MONO/>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="mono dim" style=FIELD_LABEL>{t(locale, "fitness.field.notes")}</span>
                            <textarea name="notes" rows="2" maxlength=MAX_WORKOUT_NOTES_CHARS.to_string()
                                      placeholder=t(locale, "fitness.placeholder.notes")
                                      style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono);font-size:12px"></textarea>
                        </label>
                    </div>
                    <div class="hstack" style="gap:8px">
                        <button class="btn primary" type="submit"><Icon kind=IconKind::Plus size=14/>{t(locale, "fitness.submit.record")}</button>
                        <ErrorSlot action=add/>
                    </div>
                </ActionForm>
            </Card>
        </div>

        <div style="margin-top:24px"></div>

        <Card title=t(locale, "fitness.card.sessions.title") code="FIT-SES-01" sub=tf(locale, "fitness.card.sessions.sub", &[("count", &workouts_for_table.len().to_string())])>
            {if workouts_for_table.is_empty() {
                view! { <p class="muted">{t(locale, "fitness.card.sessions.empty")}</p> }.into_any()
            } else {
                view! { {render_workouts(workouts_for_table, update, delete)} }.into_any()
            }}
            <ErrorSlot action=update/>
        </Card>
    }
}

fn render_workouts(
    rows: Vec<Workout>,
    update: ServerAction<UpdateWorkout>,
    delete: ServerAction<DeleteWorkout>,
) -> impl IntoView {
    let locale = use_locale();
    view! {
        <table class="tbl">
            <thead>
                <tr>
                    <th style="width:80px">{t(locale, "fitness.table.date")}</th>
                    <th style="width:120px">{t(locale, "fitness.table.doc")}</th>
                    <th>{t(locale, "fitness.field.kind")}</th>
                    <th style="width:120px">{t(locale, "fitness.field.plan")}</th>
                    <th class="num" style="width:90px">{t(locale, "fitness.field.duration")}</th>
                    <th class="num" style="width:120px">{t(locale, "fitness.field.load")}</th>
                    <th style="width:80px">{t(locale, "fitness.field.strain")}</th>
                    <th class="num" style="width:70px">{t(locale, "fitness.field.rpe")}</th>
                    <th class="num" style="width:90px">{t(locale, "fitness.field.ops")}</th>
                </tr>
            </thead>
            <tbody>
                {rows.into_iter().map(|w| {
                    let doc = w.doc_id.clone();
                    let edit_doc = w.doc_id.clone();
                    let date = ep_core::fmt_ts_date(Some(w.occurred_at));
                    let edit_date = date.clone();
                    let edit_kind = w.kind.clone();
                    let edit_program = w.program.clone().unwrap_or_default();
                    let edit_duration = w.duration_m.to_string();
                    let edit_load = w.load_text.clone().unwrap_or_default();
                    let edit_strain = w.strain.clone().unwrap_or_else(|| crate::model::Strain::M.as_str().into());
                    let edit_rpe = w.rpe.map(|rpe| rpe.to_string()).unwrap_or_default();
                    let edit_notes = w.notes.clone().unwrap_or_default();
                    let strain_kind = w.strain.as_deref().and_then(crate::model::Strain::parse);
                    let strain_tone = strain_kind.map(|k| k.tone()).unwrap_or(Tone::None);
                    let strain_label = w.strain.clone().unwrap_or_default();
                    view! {
                        <>
                        <tr>
                            <td class="mono dim">{date}</td>
                            <td class="doc">{w.doc_id}</td>
                            <td>
                                <div>{w.kind}</div>
                                {w.notes.map(|notes| view! {
                                    <div class="muted" style="font-size:12px;margin-top:2px;white-space:pre-wrap">{notes}</div>
                                })}
                            </td>
                            <td class="mono dim">{w.program.unwrap_or_default()}</td>
                            <td class="num">{w.duration_m}" min"</td>
                            <td class="num">{w.load_text.unwrap_or_default()}</td>
                            <td><Tag tone=strain_tone>{strain_label}</Tag></td>
                            <td class="num mono dim">{w.rpe.map(|rpe| rpe.to_string()).unwrap_or_else(|| "—".into())}</td>
                            <td class="num">
                                <RowDeleteAction action=delete value=doc confirm=t(locale, "fitness.confirm.delete")/>
                            </td>
                        </tr>
                        <tr>
                            <td colspan="9" style="padding-top:0">
                                <ActionForm action=update attr:class="hstack" attr:style="gap:8px;align-items:flex-end;flex-wrap:wrap">
                                    <input type="hidden" name="doc_id" value=edit_doc/>
                                    <input name="occurred_on" type="date" value=edit_date
                                           style="width:138px;padding:5px 8px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono);font-size:12px"/>
                                    <input name="kind" required maxlength=MAX_WORKOUT_KIND_CHARS.to_string()
                                           value=edit_kind
                                           style="width:180px;padding:5px 8px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-size:12px"/>
                                    <input name="program" maxlength=MAX_WORKOUT_PROGRAM_CHARS.to_string()
                                           value=edit_program
                                           style="width:110px;padding:5px 8px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono);font-size:12px"/>
                                    <input name="duration_m" type="number" min="1" max=MAX_WORKOUT_DURATION_MINUTES.to_string()
                                           value=edit_duration
                                           style="width:82px;padding:5px 8px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono);font-size:12px"/>
                                    <input name="load_text" maxlength=MAX_WORKOUT_LOAD_TEXT_CHARS.to_string()
                                           value=edit_load
                                           style="width:120px;padding:5px 8px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono);font-size:12px"/>
                                    <select name="strain" style="padding:5px 8px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-size:12px">
                                        <option value=crate::model::Strain::L.as_str() selected=edit_strain == "L">"L"</option>
                                        <option value=crate::model::Strain::M.as_str() selected=edit_strain == "M">"M"</option>
                                        <option value=crate::model::Strain::H.as_str() selected=edit_strain == "H">"H"</option>
                                    </select>
                                    <input name="rpe" type="number" min="1" max="10" value=edit_rpe
                                           style="width:64px;padding:5px 8px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono);font-size:12px"/>
                                    <input name="notes" maxlength=MAX_WORKOUT_NOTES_CHARS.to_string()
                                           value=edit_notes
                                           style="min-width:180px;flex:1;padding:5px 8px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-size:12px"/>
                                    <button class="btn sm" type="submit">{t(locale, "fitness.submit.update")}</button>
                                </ActionForm>
                            </td>
                        </tr>
                        </>
                    }
                }).collect_view()}
            </tbody>
        </table>
    }
}
