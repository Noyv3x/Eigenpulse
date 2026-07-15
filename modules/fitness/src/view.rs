use crate::model::*;
use crate::server_fns::*;
use ep_core::Tone;
use ep_i18n::{server_fn_error_text, t, use_locale};
use ep_ui::{
    AxisChart, AxisSeries, Card, Chart, ChartHeight, ChartSpec, ChartTone, ChartValue, ErrorSlot,
    GaugeChart, LoadError, PageHead, SkeletonCard, TabSpec, Tabs, Tag,
};
use leptos::prelude::*;
use leptos_router::hooks::query_signal;

const FITNESS_TAB_IDS: &[&str] = &["training", "history", "plans", "exercises", "progress"];

fn normalize_fitness_tab(value: Option<&str>) -> &'static str {
    value
        .and_then(|value| {
            FITNESS_TAB_IDS
                .iter()
                .copied()
                .find(|candidate| *candidate == value)
        })
        .unwrap_or("training")
}

#[component]
pub fn FitnessView() -> impl IntoView {
    let locale = use_locale();
    let (query_tab, set_query_tab) = query_signal::<String>("tab");
    let active_tab =
        RwSignal::new(normalize_fitness_tab(query_tab.get_untracked().as_deref()).to_string());
    Effect::new(move |_| {
        let normalized = normalize_fitness_tab(query_tab.get().as_deref());
        if active_tab.get_untracked() != normalized {
            active_tab.set(normalized.to_string());
        }
    });
    Effect::new(move |_| {
        let current = active_tab.get();
        if query_tab.get_untracked().as_deref() != Some(current.as_str()) {
            set_query_tab.set(Some(current));
        }
    });
    let data = Resource::new(|| (), |_| load_fitness());

    let save_settings = ServerAction::<SaveFitnessSettings>::new();
    let create_exercise = ServerAction::<CreateExercise>::new();
    let archive_exercise = ServerAction::<ArchiveExercise>::new();
    let create_plan = ServerAction::<CreateSimplePlan>::new();
    let start = ServerAction::<StartWorkout>::new();
    let pause = ServerAction::<PauseWorkout>::new();
    let resume = ServerAction::<ResumeWorkout>::new();
    let add_exercise = ServerAction::<AddWorkoutExercise>::new();
    let add_set = ServerAction::<AddWorkoutSet>::new();
    let save_set = ServerAction::<SaveWorkoutSet>::new();
    let finish = ServerAction::<FinishWorkout>::new();
    let discard = ServerAction::<DiscardWorkout>::new();
    let quick_log = ServerAction::<QuickLogWorkout>::new();
    let measurement = ServerAction::<AddBodyMeasurement>::new();

    Effect::new(move |first: Option<()>| {
        save_settings.version().get();
        create_exercise.version().get();
        archive_exercise.version().get();
        create_plan.version().get();
        start.version().get();
        pause.version().get();
        resume.version().get();
        add_exercise.version().get();
        add_set.version().get();
        save_set.version().get();
        finish.version().get();
        discard.version().get();
        quick_log.version().get();
        measurement.version().get();
        if first.is_some() {
            data.refetch();
        }
    });

    let tabs = vec![
        TabSpec::new("training", t(locale, "fitness.tab.training")),
        TabSpec::new("history", t(locale, "fitness.tab.history")),
        TabSpec::new("plans", t(locale, "fitness.tab.plans")),
        TabSpec::new("exercises", t(locale, "fitness.tab.exercises")),
        TabSpec::new("progress", t(locale, "fitness.tab.progress")),
    ];

    view! {
        <div class="view">
            <PageHead
                module=t(locale, "fitness.module.name")
                title=t(locale, "fitness.page.title")
                title_cn=t(locale, "fitness.page.title_cn")
                sub=t(locale, "fitness.module.description")
            />
            <Tabs tabs=tabs active=active_tab panel_id="fitness-panel"/>
            <div id="fitness-panel" role="tabpanel" style="margin-top:20px">
                <Suspense fallback=move || view! { <SkeletonCard rows=5/> }>
                    {move || data.get().map(|result| match result {
                        Err(error) => view! {
                            <LoadError detail=server_fn_error_text(&error)/>
                        }.into_any(),
                        Ok(data) => render_fitness(
                            data,
                            active_tab,
                            save_settings,
                            create_exercise,
                            archive_exercise,
                            create_plan,
                            start,
                            pause,
                            resume,
                            add_exercise,
                            add_set,
                            save_set,
                            finish,
                            discard,
                            quick_log,
                            measurement,
                        ).into_any(),
                    })}
                </Suspense>
            </div>
        </div>
    }
}

#[allow(clippy::too_many_arguments)]
fn render_fitness(
    data: FitnessData,
    active_tab: RwSignal<String>,
    save_settings: ServerAction<SaveFitnessSettings>,
    create_exercise: ServerAction<CreateExercise>,
    archive_exercise: ServerAction<ArchiveExercise>,
    create_plan: ServerAction<CreateSimplePlan>,
    start: ServerAction<StartWorkout>,
    pause: ServerAction<PauseWorkout>,
    resume: ServerAction<ResumeWorkout>,
    add_exercise: ServerAction<AddWorkoutExercise>,
    add_set: ServerAction<AddWorkoutSet>,
    save_set: ServerAction<SaveWorkoutSet>,
    finish: ServerAction<FinishWorkout>,
    discard: ServerAction<DiscardWorkout>,
    quick_log: ServerAction<QuickLogWorkout>,
    measurement: ServerAction<AddBodyMeasurement>,
) -> impl IntoView {
    let locale = use_locale();
    let active_exercises = data
        .exercises
        .iter()
        .filter(|item| !item.exercise.archived)
        .cloned()
        .collect::<Vec<_>>();
    let summary = data.home.clone();
    let active = data.active_workout.clone();
    let history = data.history.clone();
    let workout_dates = data.workout_dates.clone();
    let plans = data.plans.clone();
    let exercise_library = data.exercises.clone();
    let strength_exercises = data.strength_exercises;
    let settings = data.settings.clone();
    let units = UnitSystem::from_storage(&settings.unit_system).unwrap_or_default();
    let today = data.today.clone();
    let measurements = data.measurements.clone();
    let measurement_dates = data.measurement_dates.clone();
    let records = data.personal_records.clone();

    view! {
        <div style=move || if active_tab.get() == "training" { "" } else { "display:none" }>
            <section class="vstack" style="gap:18px">
                <div class="kpi-grid">
                    <SummaryCard label=t(locale, "fitness.summary.active").to_string() value=summary.active_status.clone().unwrap_or_else(|| "—".into())/>
                    <SummaryCard label=t(locale, "fitness.summary.workouts_week").to_string() value=summary.completed_workouts_this_week.to_string()/>
                    <SummaryCard label=t(locale, "fitness.summary.sets_week").to_string() value=summary.completed_sets_this_week.to_string()/>
                    <SummaryCard label=t(locale, "fitness.summary.streak").to_string() value=format!("{} {}", summary.streak_days, t(locale, "fitness.summary.days"))/>
                </div>
                {match active {
                    Some(workout) => render_active_workout(
                        workout,
                        active_exercises.clone(),
                        pause,
                        resume,
                        add_exercise,
                        add_set,
                        save_set,
                        finish,
                        discard,
                        units,
                    ).into_any(),
                    None => render_start_panel(plans.clone(), active_exercises.is_empty(), start).into_any(),
                }}
                {render_quick_log(active_exercises.clone(), today, units, quick_log)}
            </section>
        </div>

        <div style=move || if active_tab.get() == "history" { "" } else { "display:none" }>
            <Card title=t(locale, "fitness.history.title")>
                {if history.is_empty() {
                    view! { <p class="muted">{t(locale, "fitness.history.empty")}</p> }.into_any()
                } else {
                    view! {
                        <div class="vstack" style="gap:12px">
                            {history.into_iter().map(|workout| {
                                let date = workout_dates.get(&workout.workout.id).cloned().unwrap_or_default();
                                let completed_sets = workout.exercises.iter().flat_map(|item| &item.sets)
                                    .filter(|set| set.status == "completed").count();
                                view! {
                                    <article style="padding:14px;border:1px solid var(--border);border-radius:10px">
                                        <div class="hstack" style="justify-content:space-between;gap:12px">
                                            <strong>{workout.workout.plan_name_snapshot.clone().unwrap_or_else(|| t(locale, "fitness.workout.free").into())}</strong>
                                            <span class="mono dim">{date}</span>
                                        </div>
                                        <p class="muted" style="margin:6px 0 0">
                                            {format!("{} · {} {}", workout.exercises.len(), completed_sets, t(locale, "fitness.unit.sets"))}
                                        </p>
                                        <div class="hstack" style="gap:6px;flex-wrap:wrap;margin-top:8px">
                                            {workout.exercises.into_iter().map(|item| view! {
                                                <Tag tone=Tone::None>{item.exercise.exercise_name_snapshot}</Tag>
                                            }).collect_view()}
                                        </div>
                                    </article>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </Card>
        </div>

        <div style=move || if active_tab.get() == "plans" { "" } else { "display:none" }>
            {render_plans(plans, active_exercises.clone(), units, create_plan, start)}
        </div>

        <div style=move || if active_tab.get() == "exercises" { "" } else { "display:none" }>
            {render_exercises(exercise_library, create_exercise, archive_exercise)}
        </div>

        <div style=move || if active_tab.get() == "progress" { "" } else { "display:none" }>
            {render_progress(
                settings,
                strength_exercises,
                measurements,
                measurement_dates,
                records,
                save_settings,
                measurement,
            )}
        </div>
    }
}

#[component]
fn SummaryCard(label: String, value: String) -> impl IntoView {
    view! {
        <div class="kpi">
            <div class="kpi-label">{label}</div>
            <div class="kpi-value mono">{value}</div>
        </div>
    }
}

fn render_start_panel(
    plans: Vec<PlanDetail>,
    exercise_library_empty: bool,
    start: ServerAction<StartWorkout>,
) -> impl IntoView {
    let locale = use_locale();
    view! {
        <Card title=t(locale, "fitness.training.start") sub=t(locale, "fitness.training.start_sub")>
            {if exercise_library_empty {
                view! { <p class="muted">{t(locale, "fitness.training.exercise_first")}</p> }.into_any()
            } else {
                view! {
                    <div class="hstack" style="gap:10px;flex-wrap:wrap">
                        <ActionForm action=start>
                            <input type="hidden" name="notes" value=""/>
                            <button class="btn primary" type="submit">{t(locale, "fitness.training.free")}</button>
                        </ActionForm>
                        {plans.into_iter().filter(|item| !item.plan.archived).map(|item| view! {
                            <ActionForm action=start>
                                <input type="hidden" name="plan_id" value=item.plan.id/>
                                <input type="hidden" name="notes" value=""/>
                                <button class="btn" type="submit">{item.plan.name}</button>
                            </ActionForm>
                        }).collect_view()}
                    </div>
                    <ErrorSlot action=start/>
                }.into_any()
            }}
        </Card>
    }
}

fn render_quick_log(
    exercises: Vec<ExerciseDetail>,
    today: String,
    units: UnitSystem,
    quick_log: ServerAction<QuickLogWorkout>,
) -> impl IntoView {
    let locale = use_locale();
    let weight_placeholder = format!(
        "{} ({})",
        t(locale, "fitness.field.weight"),
        units.weight_symbol()
    );
    view! {
        <Card title=t(locale, "fitness.quick.title") sub=t(locale, "fitness.quick.sub")>
            {if exercises.is_empty() {
                view! { <p class="muted">{t(locale, "fitness.training.exercise_first")}</p> }.into_any()
            } else {
                view! {
                    <ActionForm action=quick_log attr:class="vstack" attr:style="gap:10px">
                        <input type="hidden" name="unit_system" value=units.as_storage()/>
                        <div class="hstack" style="gap:8px;flex-wrap:wrap">
                            <label class="vstack" style="gap:4px;min-width:180px">
                                <span class="muted" style="font-size:12px">{t(locale, "fitness.exercise.name")}</span>
                                <select class="ep-select" name="exercise_id" required>
                                    {exercises.into_iter().map(|item| view! {
                                        <option value=item.exercise.id>{item.exercise.name}</option>
                                    }).collect_view()}
                                </select>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="muted" style="font-size:12px">{t(locale, "fitness.field.date")}</span>
                                <input class="ep-input mono" type="date" name="occurred_on" value=today required/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="muted" style="font-size:12px">{t(locale, "fitness.field.sets")}</span>
                                <input class="ep-input mono" style="width:92px" type="number" name="sets" min="1" max="100" value="3" required/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="muted" style="font-size:12px">{t(locale, "fitness.field.reps")}</span>
                                <input class="ep-input mono" style="width:92px" type="number" name="reps" min="1" value="8" required/>
                            </label>
                            <label class="vstack" style="gap:4px">
                                <span class="muted" style="font-size:12px">{weight_placeholder.clone()}</span>
                                <input class="ep-input mono" style="width:130px" type="number" name="weight" min="0.01" step="0.01" placeholder=weight_placeholder/>
                            </label>
                        </div>
                        <input class="ep-input" name="notes" maxlength="4000" placeholder=t(locale, "fitness.field.notes")/>
                        <div><button class="btn primary" type="submit">{t(locale, "fitness.quick.save")}</button></div>
                        <ErrorSlot action=quick_log/>
                    </ActionForm>
                }.into_any()
            }}
        </Card>
    }
}

#[allow(clippy::too_many_arguments)]
fn render_active_workout(
    workout: WorkoutDetail,
    exercises: Vec<ExerciseDetail>,
    pause: ServerAction<PauseWorkout>,
    resume: ServerAction<ResumeWorkout>,
    add_exercise: ServerAction<AddWorkoutExercise>,
    add_set: ServerAction<AddWorkoutSet>,
    save_set: ServerAction<SaveWorkoutSet>,
    finish: ServerAction<FinishWorkout>,
    discard: ServerAction<DiscardWorkout>,
    units: UnitSystem,
) -> impl IntoView {
    let locale = use_locale();
    let id = workout.workout.id;
    let revision = workout.workout.revision;
    let status = workout.workout.status.clone();
    let status_tag = status.clone();
    let rest_seconds = workout
        .exercises
        .iter()
        .flat_map(|item| &item.sets)
        .find(|set| set.status == "pending")
        .or_else(|| workout.exercises.iter().flat_map(|item| &item.sets).next())
        .map_or(90, |set| set.rest_seconds);
    let workout_exercises = workout.exercises;
    let weight_placeholder = format!(
        "{} ({})",
        t(locale, "fitness.field.weight"),
        units.weight_symbol()
    );
    view! {
        <Card title=workout.workout.plan_name_snapshot.unwrap_or_else(|| t(locale, "fitness.workout.free").into())>
            <div class="hstack" style="justify-content:space-between;gap:12px;flex-wrap:wrap;margin-bottom:16px">
                <div class="hstack" style="gap:8px">
                    <Tag tone=if status_tag == "paused" { Tone::Amber } else { Tone::Green } dot=true>{status_tag}</Tag>
                    <span class="mono dim">{format!("revision {revision}")}</span>
                </div>
                <div class="hstack" style="gap:8px">
                    {if status == "paused" {
                        view! {
                            <ActionForm action=resume>
                                <input type="hidden" name="id" value=id/>
                                <input type="hidden" name="expected_revision" value=revision/>
                                <button class="btn" type="submit">{t(locale, "fitness.training.resume")}</button>
                            </ActionForm>
                        }.into_any()
                    } else {
                        view! {
                            <ActionForm action=pause>
                                <input type="hidden" name="id" value=id/>
                                <input type="hidden" name="expected_revision" value=revision/>
                                <button class="btn" type="submit">{t(locale, "fitness.training.pause")}</button>
                            </ActionForm>
                        }.into_any()
                    }}
                    <ActionForm action=finish>
                        <input type="hidden" name="id" value=id/>
                        <input type="hidden" name="expected_revision" value=revision/>
                        <button class="btn primary" type="submit">{t(locale, "fitness.training.finish")}</button>
                    </ActionForm>
                    <ActionForm action=discard>
                        <input type="hidden" name="id" value=id/>
                        <input type="hidden" name="expected_revision" value=revision/>
                        <button class="btn danger" type="submit">{t(locale, "fitness.training.discard")}</button>
                    </ActionForm>
                </div>
            </div>

            <RestTimer initial_seconds=rest_seconds/>

            <div class="vstack" style="gap:16px">
                {workout_exercises.into_iter().map(|item| {
                    let workout_exercise_id = item.exercise.id;
                    let set_weight_placeholder = weight_placeholder.clone();
                    let add_weight_placeholder = weight_placeholder.clone();
                    view! {
                        <article style="padding:14px;border:1px solid var(--border);border-radius:10px">
                            <div class="hstack" style="justify-content:space-between;gap:12px">
                                <strong>{item.exercise.exercise_name_snapshot.clone()}</strong>
                                <span class="mono dim">{item.exercise.tracking_mode_snapshot.clone()}</span>
                            </div>
                            {render_media(item.media, false)}
                            <div class="vstack" style="gap:8px;margin-top:12px">
                                {item.sets.into_iter().map(|set| {
                                    let actual_weight = set.actual_weight_g
                                        .map(|value| format!("{:.2}", grams_to_display_weight(value, units)))
                                        .unwrap_or_default();
                                    let actual_weight_placeholder = set_weight_placeholder.clone();
                                    view! {
                                    <ActionForm action=save_set attr:class="hstack" attr:style="gap:7px;flex-wrap:wrap;padding:8px;background:var(--bg-2);border-radius:8px">
                                        <input type="hidden" name="unit_system" value=units.as_storage()/>
                                        <input type="hidden" name="workout_id" value=id/>
                                        <input type="hidden" name="set_id" value=set.id/>
                                        <input type="hidden" name="expected_revision" value=revision/>
                                        <span class="mono dim">{format!("#{}", set.position + 1)}</span>
                                        <input class="ep-input mono" style="width:88px" type="number" min="1" name="actual_reps" value=set.actual_reps placeholder="reps"/>
                                        <input class="ep-input mono" style="width:112px" type="number" min="0.01" step="0.01" name="actual_weight" value=actual_weight placeholder=actual_weight_placeholder/>
                                        <input class="ep-input mono" style="width:100px" type="number" min="1" name="actual_duration_s" value=set.actual_duration_s placeholder="seconds"/>
                                        <input class="ep-input mono" style="width:100px" type="number" min="1" name="actual_distance_m" value=set.actual_distance_m placeholder="metres"/>
                                        <input class="ep-input mono" style="width:82px" type="number" min="10" max="100" name="actual_rpe_x10" value=set.actual_rpe_x10 placeholder="RPE×10"/>
                                        <select class="ep-select" name="status">
                                            <option value="pending" selected=set.status == "pending">{t(locale, "fitness.set.pending")}</option>
                                            <option value="completed" selected=set.status == "completed">{t(locale, "fitness.set.completed")}</option>
                                            <option value="skipped" selected=set.status == "skipped">{t(locale, "fitness.set.skipped")}</option>
                                        </select>
                                        <button class="btn sm" type="submit">{t(locale, "fitness.action.save")}</button>
                                        <span class="mono dim">{format!("{}s", set.rest_seconds)}</span>
                                    </ActionForm>
                                }}).collect_view()}
                            </div>
                            <ActionForm action=add_set attr:class="hstack" attr:style="gap:7px;flex-wrap:wrap;margin-top:10px">
                                <input type="hidden" name="unit_system" value=units.as_storage()/>
                                <input type="hidden" name="workout_id" value=id/>
                                <input type="hidden" name="expected_revision" value=revision/>
                                <input type="hidden" name="workout_exercise_id" value=workout_exercise_id/>
                                <input class="ep-input mono" style="width:84px" type="number" min="1" name="target_reps" placeholder="reps"/>
                                <input class="ep-input mono" style="width:110px" type="number" min="0.01" step="0.01" name="target_weight" placeholder=add_weight_placeholder/>
                                <input type="hidden" name="set_type" value="working"/>
                                <input class="ep-input mono" style="width:92px" type="number" min="0" max="3600" name="rest_seconds" value="90"/>
                                <button class="btn sm" type="submit">{t(locale, "fitness.set.add")}</button>
                            </ActionForm>
                        </article>
                    }
                }).collect_view()}
            </div>

            <div class="hstack" style="gap:8px;flex-wrap:wrap;margin-top:16px">
                <ActionForm action=add_exercise attr:class="hstack" attr:style="gap:8px">
                    <input type="hidden" name="workout_id" value=id/>
                    <input type="hidden" name="expected_revision" value=revision/>
                    <select class="ep-select" name="exercise_id" required>
                        <option value="">{t(locale, "fitness.exercise.choose")}</option>
                        {exercises.into_iter().map(|item| view! {
                            <option value=item.exercise.id>{item.exercise.name}</option>
                        }).collect_view()}
                    </select>
                    <button class="btn" type="submit">{t(locale, "fitness.exercise.add_to_workout")}</button>
                </ActionForm>
            </div>
            <div class="error-slot">
                <ErrorSlot action=pause/><ErrorSlot action=resume/><ErrorSlot action=add_exercise/>
                <ErrorSlot action=add_set/><ErrorSlot action=save_set/><ErrorSlot action=finish/>
                <ErrorSlot action=discard/>
            </div>
        </Card>
    }
}

#[component]
fn RestTimer(initial_seconds: i64) -> impl IntoView {
    let locale = use_locale();
    let initial_seconds = initial_seconds.clamp(0, 3_600);
    let remaining = RwSignal::new(initial_seconds);
    let running = RwSignal::new(false);

    Effect::new(move |_| {
        #[cfg(feature = "hydrate")]
        {
            let handle = set_interval_with_handle(
                move || {
                    if !running.get_untracked() {
                        return;
                    }
                    let (next, keep_running) =
                        countdown_tick(remaining.get_untracked(), running.get_untracked());
                    remaining.set(next);
                    running.set(keep_running);
                },
                std::time::Duration::from_secs(1),
            )
            .ok();
            on_cleanup(move || {
                if let Some(handle) = handle {
                    handle.clear();
                }
            });
        }
    });

    view! {
        <section style="padding:12px 14px;margin-bottom:16px;border:1px solid var(--border);border-radius:10px;background:var(--bg-2)" aria-label=t(locale, "fitness.rest.title")>
            <div class="hstack" style="gap:12px;justify-content:space-between;flex-wrap:wrap">
                <div>
                    <div class="muted" style="font-size:12px">{t(locale, "fitness.rest.title")}</div>
                    <output class="mono" style="font-size:28px;font-weight:700" aria-live="polite">
                        {move || format_countdown(remaining.get())}
                    </output>
                </div>
                <div class="hstack" style="gap:7px;flex-wrap:wrap">
                    <button
                        class="btn sm"
                        type="button"
                        disabled=move || remaining.get() == 0
                        on:click=move |_| running.update(|value| *value = !*value)
                    >
                        {move || if running.get() { t(locale, "fitness.rest.pause") } else { t(locale, "fitness.rest.start") }}
                    </button>
                    <button
                        class="btn sm"
                        type="button"
                        on:click=move |_| {
                            running.set(false);
                            remaining.set(initial_seconds);
                        }
                    >{t(locale, "fitness.rest.reset")}</button>
                    <button
                        class="btn sm"
                        type="button"
                        on:click=move |_| remaining.update(|value| *value = (*value + 30).min(3_600))
                    >{t(locale, "fitness.rest.add_30")}</button>
                </div>
            </div>
        </section>
    }
}

fn render_media(media: Vec<ExerciseMedia>, editable: bool) -> impl IntoView {
    let locale = use_locale();
    view! {
        <div class="hstack" style="gap:10px;overflow-x:auto;margin-top:10px">
            {media.into_iter().map(|item| {
                let src = format!("/fitness/media/{}", item.id);
                let delete_action = format!("/fitness/media/{}/delete", item.id);
                let media_view = if item.media_type == "gif" {
                    view! { <img src=src alt=item.title.unwrap_or_else(|| t(locale, "fitness.media.guide_alt").to_string()) style="max-width:220px;max-height:160px;border-radius:8px" loading="lazy"/> }.into_any()
                } else {
                    view! { <video src=src title=item.title.unwrap_or_default() controls muted loop playsinline preload="metadata" style="max-width:260px;max-height:180px;border-radius:8px"></video> }.into_any()
                };
                view! {
                    <div class="vstack" style="gap:6px;align-items:flex-start">
                        {media_view}
                        {editable.then(|| view! {
                            <form method="post" action=delete_action>
                                <button class="btn sm danger" type="submit">{t(locale, "fitness.media.delete")}</button>
                            </form>
                        })}
                    </div>
                }
            }).collect_view()}
        </div>
    }
}

fn render_plans(
    plans: Vec<PlanDetail>,
    exercises: Vec<ExerciseDetail>,
    units: UnitSystem,
    create: ServerAction<CreateSimplePlan>,
    start: ServerAction<StartWorkout>,
) -> impl IntoView {
    let locale = use_locale();
    let has_exercises = !exercises.is_empty();
    let weight_placeholder = format!(
        "{} ({})",
        t(locale, "fitness.field.weight"),
        units.weight_symbol()
    );
    view! {
        <div class="grid-2">
            <Card title=t(locale, "fitness.plan.new")>
                {if has_exercises {
                    view! {
                        <ActionForm action=create attr:class="vstack" attr:style="gap:10px">
                            <input type="hidden" name="unit_system" value=units.as_storage()/>
                            <input class="ep-input" name="name" required maxlength="120" placeholder=t(locale, "fitness.plan.name")/>
                            <select class="ep-select" name="exercise_id" required>
                                {exercises.into_iter().map(|item| view! {
                                    <option value=item.exercise.id>{item.exercise.name}</option>
                                }).collect_view()}
                            </select>
                            <div class="hstack" style="gap:8px">
                                <input class="ep-input mono" type="number" name="sets" min="1" max="20" value="3" aria-label="sets"/>
                                <input class="ep-input mono" type="number" name="target_reps" min="1" placeholder="reps"/>
                                <input class="ep-input mono" type="number" name="target_weight" min="0.01" step="0.01" placeholder=weight_placeholder/>
                                <input class="ep-input mono" type="number" name="rest_seconds" min="0" max="3600" value="90" aria-label="rest seconds"/>
                            </div>
                            <textarea class="ep-textarea" name="notes" maxlength="4000" placeholder=t(locale, "fitness.field.notes")></textarea>
                            <button class="btn primary" type="submit">{t(locale, "fitness.action.create")}</button>
                            <ErrorSlot action=create/>
                        </ActionForm>
                    }.into_any()
                } else {
                    view! { <p class="muted">{t(locale, "fitness.training.exercise_first")}</p> }.into_any()
                }}
            </Card>
            <Card title=t(locale, "fitness.plan.list")>
                <div class="vstack" style="gap:10px">
                    {plans.into_iter().filter(|item| !item.plan.archived).map(|item| {
                        let plan_id = item.plan.id;
                        view! {
                            <div style="padding:12px;border:1px solid var(--border);border-radius:8px">
                                <div class="hstack" style="justify-content:space-between;gap:10px">
                                    <strong>{item.plan.name}</strong>
                                    <ActionForm action=start>
                                        <input type="hidden" name="plan_id" value=plan_id/>
                                        <input type="hidden" name="notes" value=""/>
                                        <button class="btn sm" type="submit">{t(locale, "fitness.training.start")}</button>
                                    </ActionForm>
                                </div>
                                <p class="muted" style="margin:6px 0 0">
                                    {item.exercises.into_iter().map(|entry| entry.exercise.exercise_name).collect::<Vec<_>>().join(" · ")}
                                </p>
                            </div>
                        }
                    }).collect_view()}
                    <ErrorSlot action=start/>
                </div>
            </Card>
        </div>
    }
}

fn render_exercises(
    exercises: Vec<ExerciseDetail>,
    create: ServerAction<CreateExercise>,
    archive: ServerAction<ArchiveExercise>,
) -> impl IntoView {
    let locale = use_locale();
    view! {
        <div class="grid-2">
            <Card title=t(locale, "fitness.exercise.new")>
                <ActionForm action=create attr:class="vstack" attr:style="gap:10px">
                    <input class="ep-input" name="name" required maxlength="120" placeholder=t(locale, "fitness.exercise.name")/>
                    <div class="hstack" style="gap:8px">
                        <select class="ep-select" name="category">
                            <option value="strength">{t(locale, "fitness.category.strength")}</option>
                            <option value="cardio">{t(locale, "fitness.category.cardio")}</option>
                            <option value="mobility">{t(locale, "fitness.category.mobility")}</option>
                            <option value="other">{t(locale, "fitness.category.other")}</option>
                        </select>
                        <select class="ep-select" name="tracking_mode">
                            <option value="weighted">{t(locale, "fitness.tracking.weighted")}</option>
                            <option value="reps">{t(locale, "fitness.tracking.reps")}</option>
                            <option value="duration">{t(locale, "fitness.tracking.duration")}</option>
                            <option value="distance">{t(locale, "fitness.tracking.distance")}</option>
                            <option value="bodyweight">{t(locale, "fitness.tracking.bodyweight")}</option>
                            <option value="assisted">{t(locale, "fitness.tracking.assisted")}</option>
                        </select>
                    </div>
                    <input class="ep-input" name="primary_muscle" maxlength="120" placeholder=t(locale, "fitness.exercise.muscle")/>
                    <input class="ep-input" name="equipment" maxlength="120" placeholder=t(locale, "fitness.exercise.equipment")/>
                    <textarea class="ep-textarea" name="notes" maxlength="4000" placeholder=t(locale, "fitness.field.notes")></textarea>
                    <button class="btn primary" type="submit">{t(locale, "fitness.action.create")}</button>
                    <ErrorSlot action=create/>
                </ActionForm>
            </Card>
            <Card title=t(locale, "fitness.exercise.library")>
                {if exercises.is_empty() {
                    view! { <p class="muted">{t(locale, "fitness.exercise.empty")}</p> }.into_any()
                } else {
                    view! {
                        <div class="vstack" style="gap:12px">
                            {exercises.into_iter().map(|item| {
                                let id = item.exercise.id;
                                let upload_action = format!("/fitness/media/exercises/{id}");
                                view! {
                                    <article style="padding:12px;border:1px solid var(--border);border-radius:9px;opacity:var(--exercise-opacity,1)">
                                        <div class="hstack" style="justify-content:space-between;gap:10px">
                                            <div>
                                                <strong>{item.exercise.name.clone()}</strong>
                                                <div class="mono dim" style="font-size:12px;margin-top:3px">{format!("{} · {}", item.exercise.category, item.exercise.tracking_mode)}</div>
                                            </div>
                                            <ActionForm action=archive>
                                                <input type="hidden" name="id" value=id/>
                                                <input type="hidden" name="archived" value=!item.exercise.archived/>
                                                <button class="btn sm" type="submit">
                                                    {if item.exercise.archived { t(locale, "fitness.action.restore") } else { t(locale, "fitness.action.archive") }}
                                                </button>
                                            </ActionForm>
                                        </div>
                                        {render_media(item.media.clone(), true)}
                                        <form method="post" enctype="multipart/form-data" action=upload_action class="hstack" style="gap:8px;margin-top:10px;flex-wrap:wrap">
                                            <input type="file" name="media" accept="image/gif,video/mp4,video/webm" multiple required/>
                                            <input class="ep-input" style="max-width:180px" name="title" maxlength="120" placeholder=t(locale, "fitness.media.title")/>
                                            <button class="btn sm" type="submit" disabled=item.media.len() >= MAX_EXERCISE_MEDIA as usize>{t(locale, "fitness.media.upload")}</button>
                                            <span class="mono dim">{format!("{}/{}", item.media.len(), MAX_EXERCISE_MEDIA)}</span>
                                        </form>
                                    </article>
                                }
                            }).collect_view()}
                            <ErrorSlot action=archive/>
                        </div>
                    }.into_any()
                }}
            </Card>
        </div>
    }
}

fn render_progress(
    settings: FitnessSettings,
    strength_exercises: Vec<StrengthExerciseOption>,
    measurements: Vec<BodyMeasurement>,
    dates: std::collections::HashMap<i64, String>,
    records: Vec<PersonalRecord>,
    save_settings: ServerAction<SaveFitnessSettings>,
    measurement: ServerAction<AddBodyMeasurement>,
) -> impl IntoView {
    let locale = use_locale();
    let units = UnitSystem::from_storage(&settings.unit_system).unwrap_or_default();
    let body_metric = RwSignal::new("weight".to_string());
    let body_days = RwSignal::new(90_u16);
    let weekly_weeks = RwSignal::new(12_usize);
    let strength_exercise_id =
        RwSignal::new(strength_exercises.first().map(|exercise| exercise.id));
    let no_strength_exercises = strength_exercises.is_empty();
    let strength_days = RwSignal::new(180_u16);
    let analytics = Resource::new(
        move || {
            (
                body_metric.get(),
                body_days.get(),
                strength_exercise_id.get(),
                strength_days.get(),
            )
        },
        |(metric, body_range, exercise_id, strength_range)| async move {
            load_fitness_analytics(metric, body_range, exercise_id, strength_range).await
        },
    );
    let weight_label = format!(
        "{} ({})",
        t(locale, "fitness.field.weight"),
        units.weight_symbol()
    );
    let waist_label = format!(
        "{} ({})",
        t(locale, "fitness.measurement.waist"),
        units.waist_symbol()
    );
    let weight_input_label = weight_label.clone();
    let waist_input_label = waist_label.clone();
    view! {
        <div class="vstack" style="gap:18px" data-testid="fitness-progress-analytics">
            <Suspense fallback=move || view! { <SkeletonCard rows=4/> }>
                {move || analytics.get().map(|result| match result {
                    Err(error) => view! {
                        <LoadError detail=server_fn_error_text(&error)/>
                    }.into_any(),
                    Ok(analytics) => render_fitness_analytics(analytics, weekly_weeks).into_any(),
                })}
            </Suspense>

            <div class="grid-2">
                <Card title=t(locale, "fitness.chart.body.title")>
                    <div class="hstack" style="gap:8px;flex-wrap:wrap;margin-bottom:12px">
                        <label class="vstack" style="gap:4px">
                            <span class="muted">{t(locale, "fitness.chart.body.metric")}</span>
                            <select
                                class="ep-select"
                                data-testid="fitness-body-metric"
                                aria-label=t(locale, "fitness.chart.body.metric")
                                on:change=move |event| body_metric.set(event_target_value(&event))
                            >
                                <option value="weight" selected=move || body_metric.get() == "weight">{t(locale, "fitness.field.weight")}</option>
                                <option value="body_fat" selected=move || body_metric.get() == "body_fat">{t(locale, "fitness.measurement.body_fat")}</option>
                                <option value="waist" selected=move || body_metric.get() == "waist">{t(locale, "fitness.measurement.waist")}</option>
                            </select>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="muted">{t(locale, "fitness.chart.range")}</span>
                            <select
                                class="ep-select"
                                data-testid="fitness-body-range"
                                aria-label=t(locale, "fitness.chart.range")
                                on:change=move |event| {
                                    if let Ok(days) = event_target_value(&event).parse() {
                                        body_days.set(days);
                                    }
                                }
                            >
                                <option value="30" selected=move || body_days.get() == 30>{t(locale, "fitness.chart.range_30")}</option>
                                <option value="90" selected=move || body_days.get() == 90>{t(locale, "fitness.chart.range_90")}</option>
                                <option value="365" selected=move || body_days.get() == 365>{t(locale, "fitness.chart.range_365")}</option>
                            </select>
                        </label>
                    </div>
                    <Suspense fallback=move || view! { <p class="muted">{"…"}</p> }>
                        {move || analytics.get().map(|result| match result {
                            Ok(analytics) => render_body_chart(analytics.body_metric).into_any(),
                            Err(_) => ().into_any(),
                        })}
                    </Suspense>
                </Card>

                <Card title=t(locale, "fitness.chart.strength.title")>
                    <div class="hstack" style="gap:8px;flex-wrap:wrap;margin-bottom:12px">
                        <label class="vstack" style="gap:4px">
                            <span class="muted">{t(locale, "fitness.exercise.name")}</span>
                            <select
                                class="ep-select"
                                data-testid="fitness-strength-exercise"
                                aria-label=t(locale, "fitness.exercise.name")
                                disabled=no_strength_exercises
                                on:change=move |event| {
                                    strength_exercise_id.set(event_target_value(&event).parse().ok());
                                }
                            >
                                {strength_exercises.into_iter().map(|exercise| {
                                    let id = exercise.id;
                                    view! {
                                        <option value=id selected=move || strength_exercise_id.get() == Some(id)>{exercise.name}</option>
                                    }
                                }).collect_view()}
                            </select>
                        </label>
                        <label class="vstack" style="gap:4px">
                            <span class="muted">{t(locale, "fitness.chart.range")}</span>
                            <select
                                class="ep-select"
                                data-testid="fitness-strength-range"
                                aria-label=t(locale, "fitness.chart.range")
                                on:change=move |event| {
                                    if let Ok(days) = event_target_value(&event).parse() {
                                        strength_days.set(days);
                                    }
                                }
                            >
                                <option value="90" selected=move || strength_days.get() == 90>{t(locale, "fitness.chart.range_90")}</option>
                                <option value="180" selected=move || strength_days.get() == 180>{t(locale, "fitness.chart.range_180")}</option>
                                <option value="365" selected=move || strength_days.get() == 365>{t(locale, "fitness.chart.range_365")}</option>
                            </select>
                        </label>
                    </div>
                    {if no_strength_exercises {
                        view! { <p class="muted">{t(locale, "fitness.chart.strength.no_exercise")}</p> }.into_any()
                    } else {
                        view! {
                            <Suspense fallback=move || view! { <p class="muted">{"…"}</p> }>
                                {move || analytics.get().map(|result| match result {
                                    Ok(analytics) => render_strength_chart(analytics.strength_trend).into_any(),
                                    Err(_) => ().into_any(),
                                })}
                            </Suspense>
                        }.into_any()
                    }}
                </Card>
            </div>

            <div class="grid-2">
                <div class="vstack" style="gap:18px">
                    <Card title=t(locale, "fitness.settings.title")>
                        <ActionForm action=save_settings attr:class="vstack" attr:style="gap:10px">
                            <select class="ep-select" name="unit_system">
                                <option value="metric" selected=settings.unit_system == "metric">{t(locale, "fitness.units.metric")}</option>
                                <option value="imperial" selected=settings.unit_system == "imperial">{t(locale, "fitness.units.imperial")}</option>
                            </select>
                            <input class="ep-input mono" type="number" name="weekly_workout_target" min="1" max="14" value=settings.weekly_workout_target/>
                            <input class="ep-input mono" type="number" name="weekly_cardio_minutes_target" min="0" max="10080" value=settings.weekly_cardio_minutes_target/>
                            <button class="btn" type="submit">{t(locale, "fitness.action.save")}</button>
                            <ErrorSlot action=save_settings/>
                        </ActionForm>
                    </Card>
                    <Card title=t(locale, "fitness.measurement.new")>
                        <ActionForm action=measurement attr:class="vstack" attr:style="gap:10px">
                            <input type="hidden" name="unit_system" value=units.as_storage()/>
                            <div class="hstack" style="gap:8px;flex-wrap:wrap">
                                <input class="ep-input mono" type="number" min="0.01" step="0.01" name="weight" placeholder=weight_input_label/>
                                <input class="ep-input mono" type="number" min="0.01" max="100" step="0.01" name="body_fat_percent" placeholder=t(locale, "fitness.measurement.body_fat_percent")/>
                                <input class="ep-input mono" type="number" min="0.1" step="0.1" name="waist" placeholder=waist_input_label/>
                            </div>
                            <input class="ep-input" name="notes" maxlength="4000" placeholder=t(locale, "fitness.field.notes")/>
                            <button class="btn primary" type="submit">{t(locale, "fitness.action.record")}</button>
                            <ErrorSlot action=measurement/>
                        </ActionForm>
                    </Card>
                </div>
                <div class="vstack" style="gap:18px">
                    <Card title=t(locale, "fitness.pr.title")>
                        <table class="tbl">
                            <thead><tr><th>{t(locale, "fitness.exercise.name")}</th><th>{t(locale, "fitness.pr.kind")}</th><th class="num">{t(locale, "fitness.pr.value")}</th></tr></thead>
                            <tbody>{records.into_iter().map(|record| view! {
                                <tr><td>{record.exercise_name}</td><td class="mono dim">{record.kind}</td><td class="num mono">{format_weight(record.value_g, units)}</td></tr>
                            }).collect_view()}</tbody>
                        </table>
                    </Card>
                    <Card title=t(locale, "fitness.measurement.history")>
                        <table class="tbl">
                            <thead><tr><th>{t(locale, "fitness.field.date")}</th><th class="num">{weight_label}</th><th class="num">{t(locale, "fitness.measurement.body_fat")}</th><th class="num">{waist_label}</th></tr></thead>
                            <tbody>{measurements.into_iter().map(|item| view! {
                                <tr>
                                    <td class="mono dim">{dates.get(&item.id).cloned().unwrap_or_default()}</td>
                                    <td class="num mono">{item.weight_g.map(|value| format_weight(value, units)).unwrap_or_else(|| "—".into())}</td>
                                    <td class="num mono">{item.body_fat_bp.map(format_body_fat).unwrap_or_else(|| "—".into())}</td>
                                    <td class="num mono">{item.waist_mm.map(|value| format_waist(value, units)).unwrap_or_else(|| "—".into())}</td>
                                </tr>
                            }).collect_view()}</tbody>
                        </table>
                    </Card>
                </div>
            </div>
        </div>
    }
}

fn render_fitness_analytics(
    analytics: FitnessAnalytics,
    weekly_weeks: RwSignal<usize>,
) -> impl IntoView {
    let locale = use_locale();
    let weekly_activity = analytics.weekly_activity;
    let weekly = Signal::derive(move || {
        weekly_activity_spec(weekly_window(&weekly_activity, weekly_weeks.get()), locale)
    });
    let gauge = workout_gauge_spec(&analytics.workout_target, locale);
    view! {
        <div class="grid-2">
            <Card title=t(locale, "fitness.chart.weekly.title")>
                <div class="hstack" style="gap:6px;flex-wrap:wrap;margin-bottom:10px" role="group" aria-label=t(locale, "fitness.chart.weekly.range")>
                    {[4_usize, 12, 26, 52].into_iter().map(|weeks| view! {
                        <button
                            class=move || if weekly_weeks.get() == weeks { "btn sm primary" } else { "btn sm" }
                            type="button"
                            data-testid=format!("fitness-week-range-{weeks}")
                            aria-pressed=move || (weekly_weeks.get() == weeks).to_string()
                            on:click=move |_| weekly_weeks.set(weeks)
                        >
                            {t(locale, match weeks {
                                4 => "fitness.chart.weekly.range_4",
                                12 => "fitness.chart.weekly.range_12",
                                26 => "fitness.chart.weekly.range_26",
                                _ => "fitness.chart.weekly.range_52",
                            })}
                        </button>
                    }).collect_view()}
                </div>
                <Chart
                    label=t(locale, "fitness.chart.weekly.title").to_string()
                    description=t(locale, "fitness.chart.weekly.description").to_string()
                    spec=weekly
                    height=ChartHeight::Standard
                />
            </Card>
            <Card title=t(locale, "fitness.chart.target.title")>
                <Chart
                    label=t(locale, "fitness.chart.target.title").to_string()
                    description=t(locale, "fitness.chart.target.description").to_string()
                    spec=chart_signal(gauge)
                    height=ChartHeight::Standard
                />
            </Card>
        </div>
    }
}

fn weekly_window(points: &[WeeklyActivityPoint], weeks: usize) -> &[WeeklyActivityPoint] {
    &points[points.len().saturating_sub(weeks)..]
}

fn render_body_chart(trend: BodyMetricTrend) -> impl IntoView {
    let locale = use_locale();
    if trend.points.is_empty() {
        return view! { <p class="muted">{t(locale, "fitness.chart.no_data")}</p> }.into_any();
    }
    let spec = body_metric_spec(&trend, locale);
    view! {
        <Chart
            label=t(locale, "fitness.chart.body.title").to_string()
            spec=chart_signal(spec)
            height=ChartHeight::Standard
        />
    }
    .into_any()
}

fn render_strength_chart(points: Vec<StrengthTrendPoint>) -> impl IntoView {
    let locale = use_locale();
    if points.is_empty() {
        return view! { <p class="muted">{t(locale, "fitness.chart.no_data")}</p> }.into_any();
    }
    let spec = strength_trend_spec(&points, locale);
    view! {
        <Chart
            label=t(locale, "fitness.chart.strength.title").to_string()
            spec=chart_signal(spec)
            height=ChartHeight::Standard
        />
    }
    .into_any()
}

fn chart_signal(spec: ChartSpec) -> Signal<ChartSpec> {
    Signal::derive(move || spec.clone())
}

fn weekly_activity_spec(points: &[WeeklyActivityPoint], locale: ep_i18n::Locale) -> ChartSpec {
    ChartSpec::Axis(AxisChart {
        categories: points.iter().map(|point| point.label.clone()).collect(),
        series: vec![
            AxisSeries::bar(
                t(locale, "fitness.chart.weekly.workouts"),
                points
                    .iter()
                    .map(|point| {
                        Some(ChartValue::new(
                            point.completed_workouts as f64,
                            point.completed_workouts.to_string(),
                        ))
                    })
                    .collect(),
            )
            .with_tone(ChartTone::Primary),
            AxisSeries::line(
                t(locale, "fitness.chart.weekly.sets"),
                points
                    .iter()
                    .map(|point| {
                        Some(ChartValue::new(
                            point.completed_sets as f64,
                            point.completed_sets.to_string(),
                        ))
                    })
                    .collect(),
            )
            .with_tone(ChartTone::Positive),
        ],
        y_label: None,
        stacked: false,
    })
}

fn workout_gauge_spec(gauge: &WeeklyWorkoutGauge, locale: ep_i18n::Locale) -> ChartSpec {
    ChartSpec::Gauge(GaugeChart {
        name: t(locale, "fitness.chart.target.progress").to_string(),
        value: ChartValue::new(
            gauge.completed as f64,
            format!("{} / {}", gauge.completed, gauge.target),
        ),
        min: 0.0,
        max: gauge.target.max(gauge.completed).max(1) as f64,
        tone: if gauge.completed >= gauge.target {
            ChartTone::Positive
        } else {
            ChartTone::Primary
        },
    })
}

fn body_metric_spec(trend: &BodyMetricTrend, locale: ep_i18n::Locale) -> ChartSpec {
    let series_name = match trend.metric.as_str() {
        "weight" => t(locale, "fitness.chart.body.weight"),
        "body_fat" => t(locale, "fitness.chart.body.body_fat"),
        "waist" => t(locale, "fitness.chart.body.waist"),
        _ => t(locale, "fitness.chart.body.metric"),
    };
    ChartSpec::Axis(AxisChart {
        categories: trend
            .points
            .iter()
            .map(|point| point.label.clone())
            .collect(),
        series: vec![AxisSeries::line(
            series_name,
            trend
                .points
                .iter()
                .map(|point| {
                    Some(ChartValue::new(
                        point.value.value,
                        point.value.display.clone(),
                    ))
                })
                .collect(),
        )
        .with_tone(ChartTone::Primary)
        .smooth(true)],
        y_label: Some(trend.unit.clone()),
        stacked: false,
    })
}

fn strength_trend_spec(points: &[StrengthTrendPoint], locale: ep_i18n::Locale) -> ChartSpec {
    ChartSpec::Axis(AxisChart {
        categories: points.iter().map(|point| point.label.clone()).collect(),
        series: vec![
            AxisSeries::line(
                t(locale, "fitness.chart.strength.estimated_1rm"),
                points
                    .iter()
                    .map(|point| {
                        point
                            .estimated_1rm
                            .as_ref()
                            .map(|value| ChartValue::new(value.value, value.display.clone()))
                    })
                    .collect(),
            )
            .with_tone(ChartTone::Primary)
            .smooth(true),
            AxisSeries::line(
                t(locale, "fitness.chart.strength.volume"),
                points
                    .iter()
                    .map(|point| {
                        Some(ChartValue::new(
                            point.volume.value,
                            point.volume.display.clone(),
                        ))
                    })
                    .collect(),
            )
            .with_tone(ChartTone::Positive),
        ],
        y_label: None,
        stacked: false,
    })
}

#[cfg(test)]
mod tab_tests {
    use super::{normalize_fitness_tab, weekly_window};
    use crate::model::WeeklyActivityPoint;

    #[test]
    fn query_tab_is_whitelisted() {
        assert_eq!(normalize_fitness_tab(Some("exercises")), "exercises");
        assert_eq!(normalize_fitness_tab(Some("progress")), "progress");
        assert_eq!(normalize_fitness_tab(Some("unknown")), "training");
        assert_eq!(normalize_fitness_tab(None), "training");
    }

    #[test]
    fn weekly_ranges_keep_the_most_recent_points() {
        let points = (0..52)
            .map(|index| WeeklyActivityPoint {
                week_start: index,
                label: index.to_string(),
                completed_workouts: index,
                completed_sets: index * 2,
            })
            .collect::<Vec<_>>();

        for weeks in [4, 12, 26, 52] {
            let window = weekly_window(&points, weeks);
            assert_eq!(window.len(), weeks);
            assert_eq!(window.last().unwrap().week_start, 51);
            assert_eq!(window.first().unwrap().week_start, 52 - weeks as i64);
        }
    }
}
