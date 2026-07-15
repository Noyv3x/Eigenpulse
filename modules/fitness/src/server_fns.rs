#![cfg_attr(
    not(feature = "ssr"),
    allow(
        unused_variables,
        reason = "Leptos server-function parameters are serialized by client builds while their implementations are SSR-only"
    )
)]

use crate::model::*;
#[cfg(feature = "ssr")]
use ep_core::server_err;
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
#[cfg(feature = "ssr")]
use std::collections::HashMap;

#[cfg(feature = "ssr")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct FitnessTime {
    pub(crate) timezone: ep_core::AppTimezone,
    pub(crate) now: i64,
}

#[cfg(feature = "ssr")]
impl FitnessTime {
    pub(crate) fn capture(state: &ep_core::AppState) -> Self {
        Self {
            timezone: state.timezone(),
            now: ep_core::unix_now(),
        }
    }

    pub(crate) fn end_exclusive(self) -> Result<i64, ServerFnError> {
        self.now
            .checked_add(1)
            .ok_or_else(|| server_err("fitness time range overflow"))
    }

    pub(crate) fn trailing_days(self, days: u16) -> Result<(i64, i64), ServerFnError> {
        let start = self
            .timezone
            .trailing_days_start(self.now, days)
            .ok_or_else(|| server_err("fitness calendar range is invalid"))?;
        Ok((start, self.end_exclusive()?))
    }

    fn workout_date(self, timestamp: i64) -> Result<String, ServerFnError> {
        self.timezone
            .date(timestamp)
            .map(ep_core::CalendarDate::ymd)
            .ok_or_else(|| server_err("fitness workout timestamp is invalid"))
    }

    pub(crate) fn trailing_workout_dates(
        self,
        days: u16,
    ) -> Result<(String, String), ServerFnError> {
        let start = self
            .timezone
            .trailing_days_start(self.now, days)
            .ok_or_else(|| server_err("fitness calendar range is invalid"))?;
        let today = self
            .timezone
            .date(self.now)
            .ok_or_else(|| server_err("fitness current date is invalid"))?;
        let end = self
            .timezone
            .shift_date(today, 1)
            .ok_or_else(|| server_err("fitness calendar range is invalid"))?;
        Ok((self.workout_date(start)?, end.ymd()))
    }
}

#[cfg(feature = "ssr")]
const MAX_NAME_CHARS: usize = 120;
#[cfg(feature = "ssr")]
const MAX_NOTES_CHARS: usize = 4_000;

#[cfg(feature = "ssr")]
fn user_error(message: impl Into<String>) -> ServerFnError {
    ServerFnError::ServerError(message.into())
}

#[server(LoadFitness, "/api/_internal/fitness", "Url", "load")]
pub async fn load_fitness() -> Result<FitnessData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        load_fitness_inner(&state.db, time).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(LoadFitnessAnalytics, "/api/_internal/fitness", "Url", "analytics")]
pub async fn load_fitness_analytics(
    body_metric: String,
    body_days: u16,
    strength_exercise_id: Option<i64>,
    strength_days: u16,
) -> Result<FitnessAnalytics, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        load_fitness_analytics_inner(
            &state.db,
            &body_metric,
            body_days,
            strength_exercise_id,
            strength_days,
            time,
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(
    LoadFitnessHomeSummary,
    "/api/_internal/fitness",
    "Url",
    "home_summary"
)]
pub async fn load_home_summary() -> Result<ep_core::ModuleSummary, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        let summary = home_summary_inner(&state.db, time).await?;
        let summary_state = if summary.active_workout_id.is_some() {
            ep_core::ModuleSummaryState::Active
        } else if summary.completed_workouts_this_week == 0 {
            ep_core::ModuleSummaryState::Empty
        } else {
            ep_core::ModuleSummaryState::Ready
        };
        Ok(ep_core::ModuleSummary {
            slug: DESCRIPTOR.slug.to_string(),
            state: summary_state,
            metrics: vec![
                ep_core::SummaryMetric {
                    label_key: "fitness.summary.active".into(),
                    value: summary.active_status.unwrap_or_else(|| "—".to_string()),
                    detail: summary.active_workout_id.map(|id| format!("#{id}")),
                },
                ep_core::SummaryMetric {
                    label_key: "fitness.summary.workouts_week".into(),
                    value: summary.completed_workouts_this_week.to_string(),
                    detail: None,
                },
                ep_core::SummaryMetric {
                    label_key: "fitness.summary.sets_week".into(),
                    value: summary.completed_sets_this_week.to_string(),
                    detail: None,
                },
                ep_core::SummaryMetric {
                    label_key: "fitness.summary.streak".into(),
                    value: summary.streak_days.to_string(),
                    detail: Some("fitness.summary.days".into()),
                },
            ],
            trend: fitness_summary_trend(&state.db, time).await?,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(SaveFitnessSettings, "/api/_internal/fitness", "Url", "save_settings")]
pub async fn save_settings(
    unit_system: String,
    weekly_workout_target: i64,
    weekly_cardio_minutes_target: i64,
) -> Result<FitnessSettings, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        save_settings_inner(
            &state.db,
            &unit_system,
            weekly_workout_target,
            weekly_cardio_minutes_target,
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(CreateExercise, "/api/_internal/fitness", "Url", "create_exercise")]
pub async fn create_exercise(
    name: String,
    category: String,
    tracking_mode: String,
    primary_muscle: String,
    equipment: String,
    notes: String,
) -> Result<ep_core::EntityId, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let id = create_exercise_inner(
            &state.db,
            ExerciseInput {
                name,
                category,
                tracking_mode,
                primary_muscle: optional_text(primary_muscle),
                equipment: optional_text(equipment),
                notes: optional_text(notes),
            },
        )
        .await?;
        Ok(ep_core::EntityId::new(id))
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(ArchiveExercise, "/api/_internal/fitness", "Url", "archive_exercise")]
pub async fn archive_exercise(id: i64, archived: bool) -> Result<ep_core::EntityId, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        archive_exercise_inner(&state.db, id, archived).await?;
        Ok(ep_core::EntityId::new(id))
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "flat fields are required for Leptos ActionForm serialization"
)]
#[server(
    CreateSimplePlan,
    "/api/_internal/fitness",
    "Url",
    "create_simple_plan"
)]
pub async fn create_simple_plan(
    name: String,
    notes: String,
    exercise_id: i64,
    sets: i64,
    target_reps: Option<i64>,
    target_weight: Option<f64>,
    rest_seconds: i64,
    unit_system: String,
) -> Result<ep_core::EntityId, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if !(1..=20).contains(&sets) {
            return Err(user_error("sets must be between 1 and 20"));
        }
        let state = ep_auth::authed_state().await?;
        let units = input_unit_system(&unit_system)?;
        let set = PlanSetInput {
            target_reps,
            target_weight_g: optional_display_weight(target_weight, units)?,
            target_duration_s: None,
            target_distance_m: None,
            target_rpe_x10: None,
            set_type: "working".into(),
            rest_seconds,
        };
        let id = create_plan_inner(
            &state.db,
            PlanInput {
                name,
                notes: optional_text(notes),
                exercises: vec![PlanExerciseInput {
                    exercise_id,
                    notes: None,
                    sets: vec![set; sets as usize],
                }],
            },
        )
        .await?;
        Ok(ep_core::EntityId::new(id))
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(StartWorkout, "/api/_internal/fitness", "Url", "start")]
pub async fn start_workout(
    plan_id: Option<i64>,
    notes: String,
) -> Result<WorkoutDetail, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        start_workout_inner(&state.db, plan_id, optional_text(notes), time).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(PauseWorkout, "/api/_internal/fitness", "Url", "pause")]
pub async fn pause_workout(
    id: i64,
    expected_revision: i64,
) -> Result<WorkoutDetail, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        pause_workout_inner(&state.db, id, expected_revision, time.now).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(ResumeWorkout, "/api/_internal/fitness", "Url", "resume")]
pub async fn resume_workout(
    id: i64,
    expected_revision: i64,
) -> Result<WorkoutDetail, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        resume_workout_inner(&state.db, id, expected_revision, time.now).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(AddWorkoutExercise, "/api/_internal/fitness", "Url", "add_exercise")]
pub async fn add_workout_exercise(
    workout_id: i64,
    expected_revision: i64,
    exercise_id: i64,
) -> Result<WorkoutDetail, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        add_workout_exercise_inner(
            &state.db,
            workout_id,
            expected_revision,
            exercise_id,
            time.now,
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "flat fields are required for Leptos ActionForm serialization"
)]
#[server(AddWorkoutSet, "/api/_internal/fitness", "Url", "add_set")]
pub async fn add_workout_set(
    workout_id: i64,
    expected_revision: i64,
    workout_exercise_id: i64,
    target_reps: Option<i64>,
    target_weight: Option<f64>,
    target_duration_s: Option<i64>,
    target_distance_m: Option<i64>,
    target_rpe_x10: Option<i64>,
    set_type: String,
    rest_seconds: i64,
    unit_system: String,
) -> Result<WorkoutDetail, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        let units = input_unit_system(&unit_system)?;
        add_workout_set_inner(
            &state.db,
            workout_id,
            expected_revision,
            workout_exercise_id,
            PlanSetInput {
                target_reps,
                target_weight_g: optional_display_weight(target_weight, units)?,
                target_duration_s,
                target_distance_m,
                target_rpe_x10,
                set_type,
                rest_seconds,
            },
            time.now,
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "flat fields are required for Leptos ActionForm serialization"
)]
#[server(SaveWorkoutSet, "/api/_internal/fitness", "Url", "save_set")]
pub async fn save_workout_set(
    workout_id: i64,
    set_id: i64,
    expected_revision: i64,
    actual_reps: Option<i64>,
    actual_weight: Option<f64>,
    actual_duration_s: Option<i64>,
    actual_distance_m: Option<i64>,
    actual_rpe_x10: Option<i64>,
    status: String,
    unit_system: String,
) -> Result<WorkoutDetail, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        let units = input_unit_system(&unit_system)?;
        save_workout_set_inner(
            &state.db,
            workout_id,
            set_id,
            expected_revision,
            SetResultInput {
                actual_reps,
                actual_weight_g: optional_display_weight(actual_weight, units)?,
                actual_duration_s,
                actual_distance_m,
                actual_rpe_x10,
                status,
            },
            time.now,
        )
        .await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(FinishWorkout, "/api/_internal/fitness", "Url", "finish")]
pub async fn finish_workout(
    id: i64,
    expected_revision: i64,
) -> Result<FinishWorkoutResult, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        let result = finish_workout_inner(&state.db, id, expected_revision, time).await?;
        notify_personal_records(&state.notify, &result.new_records).await;
        Ok(result)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(DiscardWorkout, "/api/_internal/fitness", "Url", "discard")]
pub async fn discard_workout(
    id: i64,
    expected_revision: i64,
) -> Result<ep_core::EntityId, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        discard_workout_inner(&state.db, id, expected_revision).await?;
        Ok(ep_core::EntityId::new(id))
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(QuickLogWorkout, "/api/_internal/fitness", "Url", "quick_log")]
pub async fn quick_log_workout(
    exercise_id: i64,
    occurred_on: String,
    sets: i64,
    reps: i64,
    weight: Option<f64>,
    notes: String,
    unit_system: String,
) -> Result<FinishWorkoutResult, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        if !(1..=100).contains(&sets) {
            return Err(user_error("sets must be between 1 and 100"));
        }
        if reps <= 0 {
            return Err(user_error("reps must be positive"));
        }
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        let units = input_unit_system(&unit_system)?;
        let set = QuickLogSetInput {
            reps: Some(reps),
            weight_g: optional_display_weight(weight, units)?,
            duration_s: None,
            distance_m: None,
            rpe_x10: None,
            set_type: "working".into(),
        };
        let input = QuickLogInput {
            occurred_at: optional_local_date(&occurred_on, time.timezone)?,
            notes: optional_text(notes),
            exercises: vec![QuickLogExerciseInput {
                exercise_id: Some(exercise_id),
                new_exercise_name: None,
                tracking_mode: None,
                sets: vec![set; sets as usize],
            }],
        };
        let result = quick_log_inner(&state.db, input, time).await?;
        notify_personal_records(&state.notify, &result.new_records).await;
        Ok(result)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[server(AddBodyMeasurement, "/api/_internal/fitness", "Url", "add_measurement")]
pub async fn add_body_measurement(
    measured_at: Option<i64>,
    weight: Option<f64>,
    body_fat_percent: Option<f64>,
    waist: Option<f64>,
    notes: String,
    unit_system: String,
) -> Result<ep_core::EntityId, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        let state = ep_auth::authed_state().await?;
        let time = FitnessTime::capture(&state);
        let units = input_unit_system(&unit_system)?;
        let id = add_body_measurement_inner(
            &state.db,
            measured_at,
            time.now,
            optional_display_weight(weight, units)?,
            optional_body_fat_percent(body_fat_percent)?,
            optional_display_waist(waist, units)?,
            optional_text(notes),
        )
        .await?;
        Ok(ep_core::EntityId::new(id))
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ep_core::server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
fn optional_text(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(feature = "ssr")]
fn input_unit_system(value: &str) -> Result<UnitSystem, ServerFnError> {
    UnitSystem::from_storage(value)
        .ok_or_else(|| user_error("unit_system must be metric or imperial"))
}

#[cfg(feature = "ssr")]
fn optional_display_weight(
    value: Option<f64>,
    units: UnitSystem,
) -> Result<Option<i64>, ServerFnError> {
    value
        .map(|value| {
            display_weight_to_grams(value, units)
                .filter(|value| *value > 0)
                .ok_or_else(|| user_error("weight must be a positive finite number"))
        })
        .transpose()
}

#[cfg(feature = "ssr")]
fn optional_display_waist(
    value: Option<f64>,
    units: UnitSystem,
) -> Result<Option<i64>, ServerFnError> {
    value
        .map(|value| {
            display_waist_to_millimetres(value, units)
                .filter(|value| *value > 0)
                .ok_or_else(|| user_error("waist must be a positive finite number"))
        })
        .transpose()
}

#[cfg(feature = "ssr")]
fn optional_body_fat_percent(value: Option<f64>) -> Result<Option<i64>, ServerFnError> {
    value
        .map(|value| {
            body_fat_percent_to_basis_points(value)
                .ok_or_else(|| user_error("body fat must be between 0.01% and 100%"))
        })
        .transpose()
}

#[cfg(feature = "ssr")]
fn optional_local_date(
    value: &str,
    timezone: ep_core::AppTimezone,
) -> Result<Option<i64>, ServerFnError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let (year, month, day) =
        ep_core::parse_ymd(value).ok_or_else(|| user_error("workout date must use YYYY-MM-DD"))?;
    timezone
        .date_midpoint(ep_core::CalendarDate { year, month, day })
        .map(Some)
        .ok_or_else(|| user_error("workout date is invalid in the configured timezone"))
}

#[cfg(feature = "ssr")]
pub(crate) async fn notify_personal_records(
    notify: &ep_core::NotifyBusHandle,
    records: &[PersonalRecord],
) {
    if records.is_empty() {
        return;
    }
    let summary = records
        .iter()
        .map(|record| record.exercise_name.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join("、");
    let message = ep_core::NotifyMessage::info("刷新个人纪录")
        .source("fitness")
        .body(format!("{summary} · {} 项新纪录", records.len()))
        .link("/fitness");
    if let Err(error) = notify.dispatch(message).await {
        tracing::warn!(error = %error, "failed to dispatch fitness PR notification");
    }
}

#[cfg(feature = "ssr")]
const EXERCISE_COLUMNS: &str =
    "id, name, category, tracking_mode, primary_muscle, equipment, notes, archived, created_at, updated_at";
#[cfg(feature = "ssr")]
const PLAN_COLUMNS: &str = "id, name, notes, archived, created_at, updated_at";
#[cfg(feature = "ssr")]
const WORKOUT_COLUMNS: &str = "id, plan_id, plan_name_snapshot, status, workout_date, started_at, ended_at, paused_at, paused_seconds, revision, notes";

#[cfg(feature = "ssr")]
pub(crate) async fn load_fitness_inner(
    pool: &sqlx::SqlitePool,
    time: FitnessTime,
) -> Result<FitnessData, ServerFnError> {
    let settings = load_settings(pool).await.map_err(server_err)?;
    let home = home_summary_inner(pool, time).await?;
    let exercises = load_exercises(pool, true).await.map_err(server_err)?;
    let strength_exercises = sqlx::query_as::<_, StrengthExerciseOption>(
        "SELECT exercise.id, exercise.name
           FROM fit_exercise exercise
          WHERE (exercise.archived = 0 AND exercise.tracking_mode = 'weighted')
             OR EXISTS (
                SELECT 1
                  FROM fit_workout_exercise workout_exercise
                  JOIN fit_workout workout ON workout.id = workout_exercise.workout_id
                 WHERE workout_exercise.exercise_id = exercise.id
                   AND workout_exercise.tracking_mode_snapshot = 'weighted'
                   AND workout.status = 'completed'
             )
          ORDER BY exercise.archived, exercise.name, exercise.id",
    )
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let plans = load_plans(pool).await.map_err(server_err)?;
    let active = sqlx::query_as::<_, Workout>(&format!(
        "SELECT {WORKOUT_COLUMNS} FROM fit_workout
          WHERE status IN ('in_progress', 'paused') LIMIT 1"
    ))
    .fetch_optional(pool)
    .await
    .map_err(server_err)?;
    let active_workout = match active {
        Some(workout) => Some(
            load_workout_detail(pool, workout)
                .await
                .map_err(server_err)?,
        ),
        None => None,
    };
    let history_rows = sqlx::query_as::<_, Workout>(&format!(
        "SELECT {WORKOUT_COLUMNS} FROM fit_workout
          WHERE status = 'completed' ORDER BY ended_at DESC, id DESC LIMIT 30"
    ))
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let mut history = Vec::with_capacity(history_rows.len());
    for workout in history_rows {
        history.push(
            load_workout_detail(pool, workout)
                .await
                .map_err(server_err)?,
        );
    }
    let measurements = sqlx::query_as::<_, BodyMeasurement>(
        "SELECT id, measured_at, weight_g, body_fat_bp, waist_mm, notes
           FROM fit_body_measurement ORDER BY measured_at DESC, id DESC LIMIT 365",
    )
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let personal_records = load_personal_records(pool).await.map_err(server_err)?;
    let workout_dates = history
        .iter()
        .map(|item| (item.workout.id, item.workout.workout_date.clone()))
        .chain(active_workout.iter().map(|item| {
            (
                item.workout.id,
                time.timezone.fmt_minute(Some(item.workout.started_at)),
            )
        }))
        .collect();
    let measurement_dates = measurements
        .iter()
        .map(|item| (item.id, time.timezone.fmt_ymd(Some(item.measured_at))))
        .collect();
    Ok(FitnessData {
        settings,
        today: time.timezone.fmt_ymd(Some(time.now)),
        home,
        exercises,
        strength_exercises,
        plans,
        active_workout,
        history,
        measurements,
        personal_records,
        workout_dates,
        measurement_dates,
    })
}

#[cfg(feature = "ssr")]
pub(crate) async fn load_settings(pool: &sqlx::SqlitePool) -> sqlx::Result<FitnessSettings> {
    sqlx::query_as(
        "SELECT unit_system, weekly_workout_target, weekly_cardio_minutes_target, updated_at
           FROM fit_settings WHERE id = 1",
    )
    .fetch_one(pool)
    .await
}

#[cfg(feature = "ssr")]
pub(crate) async fn home_summary_inner(
    pool: &sqlx::SqlitePool,
    time: FitnessTime,
) -> Result<FitnessHomeSummary, ServerFnError> {
    let mut tx = pool.begin().await.map_err(server_err)?;
    let active: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, status FROM fit_workout
          WHERE status IN ('in_progress', 'paused') LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(server_err)?;
    let current_week = time
        .timezone
        .recent_weeks(time.now, 1)
        .and_then(|mut weeks| weeks.pop())
        .ok_or_else(|| server_err("fitness current week is invalid"))?;
    let current_week_end = time.workout_date(current_week.end)?;
    let (completed_workouts_this_week, completed_sets_this_week): (i64, i64) = sqlx::query_as(
        "WITH week_workouts AS (
            SELECT id FROM fit_workout
             WHERE status = 'completed'
               AND workout_date >= ?1
               AND workout_date < ?2
               AND ended_at < ?3
         )
         SELECT COUNT(DISTINCT ww.id),
                COUNT(CASE WHEN s.status = 'completed' THEN 1 END)
           FROM week_workouts ww
           LEFT JOIN fit_workout_exercise e ON e.workout_id = ww.id
           LEFT JOIN fit_workout_set s ON s.workout_exercise_id = e.id",
    )
    .bind(&current_week.label)
    .bind(current_week_end)
    .bind(time.end_exclusive()?)
    .fetch_one(&mut *tx)
    .await
    .map_err(server_err)?;
    let workout_date_rows: Vec<String> = sqlx::query_scalar(
        "SELECT workout_date FROM fit_workout
          WHERE status = 'completed' AND ended_at < ?1
          ORDER BY workout_date DESC",
    )
    .bind(time.end_exclusive()?)
    .fetch_all(&mut *tx)
    .await
    .map_err(server_err)?;
    let workout_days = workout_date_rows
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let today = time
        .timezone
        .date(time.now)
        .ok_or_else(|| server_err("fitness current date is invalid"))?;
    let mut streak_days = 0_i64;
    let mut date = today;
    loop {
        let next_date = time
            .timezone
            .shift_date(date, 1)
            .ok_or_else(|| server_err("fitness streak date is invalid"))?;
        let date_start = time
            .timezone
            .date_start(date)
            .ok_or_else(|| server_err("fitness streak date is invalid"))?;
        let next_start = time
            .timezone
            .date_start(next_date)
            .ok_or_else(|| server_err("fitness streak date is invalid"))?;
        if date_start == next_start {
            date = time
                .timezone
                .shift_date(date, -1)
                .ok_or_else(|| server_err("fitness streak date is invalid"))?;
            continue;
        }
        if !workout_days.contains(&date.ymd()) {
            break;
        }
        streak_days = streak_days
            .checked_add(1)
            .ok_or_else(|| server_err("fitness streak overflow"))?;
        date = time
            .timezone
            .shift_date(date, -1)
            .ok_or_else(|| server_err("fitness streak date is invalid"))?;
    }
    tx.commit().await.map_err(server_err)?;
    Ok(FitnessHomeSummary {
        active_workout_id: active.as_ref().map(|row| row.0),
        active_status: active.map(|row| row.1),
        completed_workouts_this_week,
        completed_sets_this_week,
        streak_days,
    })
}

#[cfg(feature = "ssr")]
async fn fitness_summary_trend(
    pool: &sqlx::SqlitePool,
    time: FitnessTime,
) -> Result<Option<ep_core::SummaryTrend>, ServerFnError> {
    let weeks = weekly_activity_inner(pool, time, 8).await?;
    Ok(fitness_summary_trend_from_weeks(weeks))
}

#[cfg(feature = "ssr")]
fn fitness_summary_trend_from_weeks(
    weeks: impl IntoIterator<Item = WeeklyActivityPoint>,
) -> Option<ep_core::SummaryTrend> {
    ep_core::normalize_summary_trend(
        "fitness.chart.weekly_workouts",
        weeks.into_iter().map(|week| {
            (
                week.label,
                week.completed_workouts,
                week.completed_workouts.to_string(),
            )
        }),
    )
}

#[cfg(feature = "ssr")]
pub(crate) async fn load_fitness_analytics_inner(
    pool: &sqlx::SqlitePool,
    body_metric: &str,
    body_days: u16,
    strength_exercise_id: Option<i64>,
    strength_days: u16,
    time: FitnessTime,
) -> Result<FitnessAnalytics, ServerFnError> {
    if !matches!(body_days, 30 | 90 | 365) {
        return Err(user_error("body range must be 30, 90, or 365 days"));
    }
    if !matches!(strength_days, 90 | 180 | 365) {
        return Err(user_error("strength range must be 90, 180, or 365 days"));
    }
    if strength_exercise_id.is_some_and(|id| id <= 0) {
        return Err(user_error("strength exercise id must be positive"));
    }
    if !ep_core::is_valid_app_timestamp(time.now) {
        return Err(user_error("invalid analytics timestamp"));
    }

    let metric = BodyMetric::parse(body_metric)?;
    let settings = load_settings(pool).await.map_err(server_err)?;
    let units = UnitSystem::from_storage(&settings.unit_system).unwrap_or_default();
    let weekly_activity = weekly_activity_inner(pool, time, 52).await?;
    let workout_target = WeeklyWorkoutGauge {
        completed: weekly_activity
            .last()
            .map_or(0, |week| week.completed_workouts),
        target: settings.weekly_workout_target,
    };
    let body_metric = body_metric_trend_inner(pool, metric, body_days, units, time).await?;
    let strength_trend = match strength_exercise_id {
        Some(exercise_id) => {
            strength_trend_inner(pool, exercise_id, strength_days, units, time).await?
        }
        None => Vec::new(),
    };

    Ok(FitnessAnalytics {
        weekly_activity,
        workout_target,
        body_metric,
        strength_trend,
    })
}

#[cfg(feature = "ssr")]
async fn weekly_activity_inner(
    pool: &sqlx::SqlitePool,
    time: FitnessTime,
    week_count: u16,
) -> Result<Vec<WeeklyActivityPoint>, ServerFnError> {
    debug_assert!(week_count > 0);
    let ranges = time
        .timezone
        .recent_weeks(time.now, week_count)
        .ok_or_else(|| server_err("fitness weekly calendar range is invalid"))?;
    let Some(first) = ranges.first() else {
        return Ok(Vec::new());
    };
    let range_ends = ranges
        .iter()
        .map(|range| time.workout_date(range.end))
        .collect::<Result<Vec<_>, _>>()?;
    let end_date = range_ends
        .last()
        .ok_or_else(|| server_err("fitness weekly calendar range is invalid"))?;
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT workout.workout_date,
                COUNT(workout_set.id) FILTER (WHERE workout_set.status = 'completed')
           FROM fit_workout workout
           LEFT JOIN fit_workout_exercise workout_exercise
             ON workout_exercise.workout_id = workout.id
           LEFT JOIN fit_workout_set workout_set
             ON workout_set.workout_exercise_id = workout_exercise.id
          WHERE workout.status = 'completed'
            AND workout.workout_date >= ?1
            AND workout.workout_date < ?2
            AND workout.ended_at < ?3
          GROUP BY workout.id, workout.workout_date
          ORDER BY workout.workout_date, workout.id",
    )
    .bind(&first.label)
    .bind(end_date)
    .bind(time.end_exclusive()?)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;
    let mut points = ranges
        .iter()
        .map(|range| WeeklyActivityPoint {
            week_start: range.start,
            label: range.label.clone(),
            completed_workouts: 0,
            completed_sets: 0,
        })
        .collect::<Vec<_>>();
    let mut range_index = 0_usize;
    for (workout_date, completed_sets) in rows {
        while range_index < ranges.len() && workout_date >= range_ends[range_index] {
            range_index += 1;
        }
        if let (Some(range), Some(point)) = (ranges.get(range_index), points.get_mut(range_index)) {
            if workout_date >= range.label {
                point.completed_workouts = point.completed_workouts.saturating_add(1);
                point.completed_sets = point.completed_sets.saturating_add(completed_sets);
            }
        }
    }
    Ok(points)
}

#[cfg(feature = "ssr")]
#[derive(Clone, Copy)]
enum BodyMetric {
    Weight,
    BodyFat,
    Waist,
}

#[cfg(feature = "ssr")]
impl BodyMetric {
    fn parse(value: &str) -> Result<Self, ServerFnError> {
        match value {
            "weight" => Ok(Self::Weight),
            "body_fat" => Ok(Self::BodyFat),
            "waist" => Ok(Self::Waist),
            _ => Err(user_error("body metric must be weight, body_fat, or waist")),
        }
    }

    const fn storage_name(self) -> &'static str {
        match self {
            Self::Weight => "weight",
            Self::BodyFat => "body_fat",
            Self::Waist => "waist",
        }
    }

    const fn column(self) -> &'static str {
        match self {
            Self::Weight => "weight_g",
            Self::BodyFat => "body_fat_bp",
            Self::Waist => "waist_mm",
        }
    }

    fn unit(self, units: UnitSystem) -> &'static str {
        match self {
            Self::Weight => units.weight_symbol(),
            Self::BodyFat => "%",
            Self::Waist => units.waist_symbol(),
        }
    }

    fn chart_value(self, canonical: i64, units: UnitSystem) -> FitnessChartValue {
        match self {
            Self::Weight => FitnessChartValue {
                value: grams_to_display_weight(canonical, units),
                display: format_weight(canonical, units),
            },
            Self::BodyFat => FitnessChartValue {
                value: canonical as f64 / 100.0,
                display: format_body_fat(canonical),
            },
            Self::Waist => FitnessChartValue {
                value: millimetres_to_display_waist(canonical, units),
                display: format_waist(canonical, units),
            },
        }
    }
}

#[cfg(feature = "ssr")]
async fn body_metric_trend_inner(
    pool: &sqlx::SqlitePool,
    metric: BodyMetric,
    days: u16,
    units: UnitSystem,
    time: FitnessTime,
) -> Result<BodyMetricTrend, ServerFnError> {
    let (start, end) = time.trailing_days(days)?;
    let query = format!(
        "SELECT measured_at, {} FROM fit_body_measurement
          WHERE {} IS NOT NULL
            AND measured_at >= ?1
            AND measured_at < ?2
          ORDER BY measured_at, id",
        metric.column(),
        metric.column(),
    );
    let rows: Vec<(i64, i64)> = sqlx::query_as(&query)
        .bind(start)
        .bind(end)
        .fetch_all(pool)
        .await
        .map_err(server_err)?;
    Ok(BodyMetricTrend {
        metric: metric.storage_name().to_string(),
        unit: metric.unit(units).to_string(),
        points: rows
            .into_iter()
            .map(|(measured_at, canonical)| BodyMetricPoint {
                label: time.timezone.fmt_ymd(Some(measured_at)),
                value: metric.chart_value(canonical, units),
            })
            .collect(),
    })
}

#[cfg(feature = "ssr")]
#[derive(sqlx::FromRow)]
struct StrengthSetRow {
    workout_id: i64,
    workout_date: String,
    actual_weight_g: i64,
    actual_reps: i64,
}

#[cfg(feature = "ssr")]
async fn strength_trend_inner(
    pool: &sqlx::SqlitePool,
    exercise_id: i64,
    days: u16,
    units: UnitSystem,
    time: FitnessTime,
) -> Result<Vec<StrengthTrendPoint>, ServerFnError> {
    let strength_eligible: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
              FROM fit_exercise exercise
             WHERE exercise.id = ?1
               AND (
                    (exercise.archived = 0 AND exercise.tracking_mode = 'weighted')
                    OR EXISTS (
                        SELECT 1
                          FROM fit_workout_exercise workout_exercise
                          JOIN fit_workout workout
                            ON workout.id = workout_exercise.workout_id
                         WHERE workout_exercise.exercise_id = exercise.id
                           AND workout_exercise.tracking_mode_snapshot = 'weighted'
                           AND workout.status = 'completed'
                    )
               )
        )",
    )
    .bind(exercise_id)
    .fetch_one(pool)
    .await
    .map_err(server_err)?;
    if !strength_eligible {
        return Err(user_error("strength trend requires a weighted exercise"));
    }

    let (start_date, end_date) = time.trailing_workout_dates(days)?;
    let rows: Vec<StrengthSetRow> = sqlx::query_as(
        "WITH recent_workouts AS (
            SELECT workout.id, workout.workout_date, workout.ended_at
              FROM fit_workout workout
              JOIN fit_workout_exercise workout_exercise
                ON workout_exercise.workout_id = workout.id
              JOIN fit_workout_set workout_set
                ON workout_set.workout_exercise_id = workout_exercise.id
             WHERE workout.status = 'completed'
               AND workout.workout_date >= ?2
               AND workout.workout_date < ?3
               AND workout.ended_at < ?4
               AND workout_exercise.exercise_id = ?1
               AND workout_exercise.tracking_mode_snapshot = 'weighted'
               AND workout_set.status = 'completed'
               AND workout_set.actual_weight_g > 0
               AND workout_set.actual_reps > 0
             GROUP BY workout.id, workout.ended_at
             ORDER BY workout.workout_date DESC, workout.ended_at DESC, workout.id DESC
             LIMIT 200
         )
         SELECT recent_workouts.id AS workout_id,
                recent_workouts.workout_date,
                workout_set.actual_weight_g,
                workout_set.actual_reps
           FROM recent_workouts
           JOIN fit_workout_exercise workout_exercise
             ON workout_exercise.workout_id = recent_workouts.id
            AND workout_exercise.exercise_id = ?1
            AND workout_exercise.tracking_mode_snapshot = 'weighted'
           JOIN fit_workout_set workout_set
             ON workout_set.workout_exercise_id = workout_exercise.id
            AND workout_set.status = 'completed'
            AND workout_set.actual_weight_g > 0
            AND workout_set.actual_reps > 0
          ORDER BY recent_workouts.workout_date, recent_workouts.id, workout_set.position",
    )
    .bind(exercise_id)
    .bind(start_date)
    .bind(end_date)
    .bind(time.end_exclusive()?)
    .fetch_all(pool)
    .await
    .map_err(server_err)?;

    let mut points = Vec::new();
    let mut current_workout_id = None;
    let mut current_workout_date = String::new();
    let mut estimated_1rm_g = None;
    let mut volume_g = 0_i128;
    for row in rows {
        if current_workout_id.is_some_and(|id| id != row.workout_id) {
            points.push(strength_point(
                current_workout_date,
                estimated_1rm_g,
                volume_g,
                units,
            ));
            estimated_1rm_g = None;
            volume_g = 0;
        }
        current_workout_id = Some(row.workout_id);
        current_workout_date = row.workout_date;
        estimated_1rm_g = estimated_1rm_g.max(epley_1rm_g(row.actual_weight_g, row.actual_reps));
        volume_g = volume_g.saturating_add(
            i128::from(row.actual_weight_g).saturating_mul(i128::from(row.actual_reps)),
        );
    }
    if current_workout_id.is_some() {
        points.push(strength_point(
            current_workout_date,
            estimated_1rm_g,
            volume_g,
            units,
        ));
    }
    Ok(points)
}

#[cfg(feature = "ssr")]
fn strength_point(
    workout_date: String,
    estimated_1rm_g: Option<i64>,
    volume_g: i128,
    units: UnitSystem,
) -> StrengthTrendPoint {
    let volume_g = i64::try_from(volume_g).unwrap_or(i64::MAX);
    StrengthTrendPoint {
        label: workout_date,
        estimated_1rm: estimated_1rm_g.map(|grams| FitnessChartValue {
            value: grams_to_display_weight(grams, units),
            display: format_weight(grams, units),
        }),
        volume: FitnessChartValue {
            value: grams_to_display_weight(volume_g, units),
            display: format_weight(volume_g, units),
        },
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn load_exercises(
    pool: &sqlx::SqlitePool,
    include_archived: bool,
) -> sqlx::Result<Vec<ExerciseDetail>> {
    let filter = if include_archived {
        ""
    } else {
        "WHERE archived = 0"
    };
    let exercises = sqlx::query_as::<_, Exercise>(&format!(
        "SELECT {EXERCISE_COLUMNS} FROM fit_exercise {filter} ORDER BY archived, name"
    ))
    .fetch_all(pool)
    .await?;
    let media = sqlx::query_as::<_, ExerciseMedia>(
        "SELECT id, exercise_id, object_key, title, media_type, byte_size, sha256, sort_order, created_at
           FROM fit_exercise_media ORDER BY exercise_id, sort_order",
    )
    .fetch_all(pool)
    .await?;
    let mut by_exercise: HashMap<i64, Vec<ExerciseMedia>> = HashMap::new();
    for item in media {
        by_exercise.entry(item.exercise_id).or_default().push(item);
    }
    Ok(exercises
        .into_iter()
        .map(|exercise| ExerciseDetail {
            media: by_exercise.remove(&exercise.id).unwrap_or_default(),
            exercise,
        })
        .collect())
}

#[cfg(feature = "ssr")]
pub(crate) async fn load_plans(pool: &sqlx::SqlitePool) -> sqlx::Result<Vec<PlanDetail>> {
    let plans = sqlx::query_as::<_, Plan>(&format!(
        "SELECT {PLAN_COLUMNS} FROM fit_plan ORDER BY archived, name, id"
    ))
    .fetch_all(pool)
    .await?;
    let exercises = sqlx::query_as::<_, PlanExercise>(
        "SELECT pe.id, pe.plan_id, pe.exercise_id, e.name AS exercise_name,
                e.tracking_mode, pe.position, pe.notes
           FROM fit_plan_exercise pe JOIN fit_exercise e ON e.id = pe.exercise_id
          ORDER BY pe.plan_id, pe.position",
    )
    .fetch_all(pool)
    .await?;
    let sets = sqlx::query_as::<_, PlanSet>(
        "SELECT id, plan_exercise_id, position, target_reps, target_weight_g,
                target_duration_s, target_distance_m, target_rpe_x10, set_type, rest_seconds
           FROM fit_plan_set ORDER BY plan_exercise_id, position",
    )
    .fetch_all(pool)
    .await?;
    let mut sets_by_exercise: HashMap<i64, Vec<PlanSet>> = HashMap::new();
    for set in sets {
        sets_by_exercise
            .entry(set.plan_exercise_id)
            .or_default()
            .push(set);
    }
    let mut exercises_by_plan: HashMap<i64, Vec<PlanExerciseDetail>> = HashMap::new();
    for exercise in exercises {
        exercises_by_plan
            .entry(exercise.plan_id)
            .or_default()
            .push(PlanExerciseDetail {
                sets: sets_by_exercise.remove(&exercise.id).unwrap_or_default(),
                exercise,
            });
    }
    Ok(plans
        .into_iter()
        .map(|plan| PlanDetail {
            exercises: exercises_by_plan.remove(&plan.id).unwrap_or_default(),
            plan,
        })
        .collect())
}

#[cfg(feature = "ssr")]
pub(crate) async fn load_workout_by_id(
    pool: &sqlx::SqlitePool,
    id: i64,
) -> sqlx::Result<Option<WorkoutDetail>> {
    let workout = sqlx::query_as::<_, Workout>(&format!(
        "SELECT {WORKOUT_COLUMNS} FROM fit_workout WHERE id = ?1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    match workout {
        Some(workout) => load_workout_detail(pool, workout).await.map(Some),
        None => Ok(None),
    }
}

#[cfg(feature = "ssr")]
async fn load_workout_detail(
    pool: &sqlx::SqlitePool,
    workout: Workout,
) -> sqlx::Result<WorkoutDetail> {
    let exercises = sqlx::query_as::<_, WorkoutExercise>(
        "SELECT id, workout_id, exercise_id, exercise_name_snapshot,
                tracking_mode_snapshot, position, notes
           FROM fit_workout_exercise WHERE workout_id = ?1 ORDER BY position",
    )
    .bind(workout.id)
    .fetch_all(pool)
    .await?;
    let sets = sqlx::query_as::<_, WorkoutSet>(
        "SELECT s.id, s.workout_exercise_id, s.position,
                s.target_reps, s.target_weight_g, s.target_duration_s, s.target_distance_m,
                s.target_rpe_x10, s.actual_reps, s.actual_weight_g, s.actual_duration_s,
                s.actual_distance_m, s.actual_rpe_x10, s.set_type, s.status,
                s.rest_seconds, s.completed_at
           FROM fit_workout_set s JOIN fit_workout_exercise e ON e.id = s.workout_exercise_id
          WHERE e.workout_id = ?1 ORDER BY s.workout_exercise_id, s.position",
    )
    .bind(workout.id)
    .fetch_all(pool)
    .await?;
    let media = sqlx::query_as::<_, ExerciseMedia>(
        "SELECT m.id, m.exercise_id, m.object_key, m.title, m.media_type, m.byte_size,
                m.sha256, m.sort_order, m.created_at
           FROM fit_exercise_media m
          WHERE m.exercise_id IN (
                SELECT DISTINCT exercise_id FROM fit_workout_exercise
                 WHERE workout_id = ?1 AND exercise_id IS NOT NULL
          )
          ORDER BY m.exercise_id, m.sort_order",
    )
    .bind(workout.id)
    .fetch_all(pool)
    .await?;
    let mut sets_by_exercise: HashMap<i64, Vec<WorkoutSet>> = HashMap::new();
    for set in sets {
        sets_by_exercise
            .entry(set.workout_exercise_id)
            .or_default()
            .push(set);
    }
    let mut media_by_exercise: HashMap<i64, Vec<ExerciseMedia>> = HashMap::new();
    for item in media {
        media_by_exercise
            .entry(item.exercise_id)
            .or_default()
            .push(item);
    }
    let exercises = exercises
        .into_iter()
        .map(|exercise| WorkoutExerciseDetail {
            sets: sets_by_exercise.remove(&exercise.id).unwrap_or_default(),
            media: exercise
                .exercise_id
                .and_then(|id| media_by_exercise.get(&id).cloned())
                .unwrap_or_default(),
            exercise,
        })
        .collect();
    Ok(WorkoutDetail { workout, exercises })
}

#[cfg(feature = "ssr")]
pub(crate) async fn load_personal_records(
    pool: &sqlx::SqlitePool,
) -> sqlx::Result<Vec<PersonalRecord>> {
    sqlx::query_as(
        "SELECT pr.id, pr.exercise_id, e.name AS exercise_name, pr.kind, pr.value_g,
                pr.workout_set_id, pr.achieved_at
           FROM fit_personal_record pr JOIN fit_exercise e ON e.id = pr.exercise_id
          ORDER BY e.name, pr.kind",
    )
    .fetch_all(pool)
    .await
}

#[cfg(feature = "ssr")]
pub(crate) async fn save_settings_inner(
    pool: &sqlx::SqlitePool,
    unit_system: &str,
    weekly_workout_target: i64,
    weekly_cardio_minutes_target: i64,
) -> Result<FitnessSettings, ServerFnError> {
    if !matches!(unit_system, "metric" | "imperial") {
        return Err(user_error("unit_system must be metric or imperial"));
    }
    if !(1..=14).contains(&weekly_workout_target) {
        return Err(user_error("weekly workout target must be between 1 and 14"));
    }
    if !(0..=10_080).contains(&weekly_cardio_minutes_target) {
        return Err(user_error(
            "weekly cardio target must be between 0 and 10080 minutes",
        ));
    }
    sqlx::query_as(
        "UPDATE fit_settings
            SET unit_system = ?1, weekly_workout_target = ?2,
                weekly_cardio_minutes_target = ?3, updated_at = unixepoch()
          WHERE id = 1
          RETURNING unit_system, weekly_workout_target,
                    weekly_cardio_minutes_target, updated_at",
    )
    .bind(unit_system)
    .bind(weekly_workout_target)
    .bind(weekly_cardio_minutes_target)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
fn validate_exercise_input(input: &mut ExerciseInput) -> Result<(), ServerFnError> {
    input.name = clean_required(&input.name, "exercise name", MAX_NAME_CHARS)?;
    if !matches!(
        input.category.as_str(),
        "strength" | "cardio" | "mobility" | "other"
    ) {
        return Err(user_error("invalid exercise category"));
    }
    if !matches!(
        input.tracking_mode.as_str(),
        "weighted" | "reps" | "duration" | "distance" | "bodyweight" | "assisted"
    ) {
        return Err(user_error("invalid exercise tracking mode"));
    }
    input.primary_muscle = clean_optional(input.primary_muscle.take(), MAX_NAME_CHARS)?;
    input.equipment = clean_optional(input.equipment.take(), MAX_NAME_CHARS)?;
    input.notes = clean_optional(input.notes.take(), MAX_NOTES_CHARS)?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub(crate) async fn create_exercise_inner(
    pool: &sqlx::SqlitePool,
    mut input: ExerciseInput,
) -> Result<i64, ServerFnError> {
    validate_exercise_input(&mut input)?;
    sqlx::query_scalar(
        "INSERT INTO fit_exercise
            (name, category, tracking_mode, primary_muscle, equipment, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
    )
    .bind(input.name)
    .bind(input.category)
    .bind(input.tracking_mode)
    .bind(input.primary_muscle)
    .bind(input.equipment)
    .bind(input.notes)
    .fetch_one(pool)
    .await
    .map_err(|error| {
        if is_unique_violation(&error) {
            user_error("an exercise with this name already exists")
        } else {
            server_err(error)
        }
    })
}

#[cfg(feature = "ssr")]
pub(crate) async fn update_exercise_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
    mut input: ExerciseInput,
) -> Result<Exercise, ServerFnError> {
    require_positive_id(id)?;
    validate_exercise_input(&mut input)?;
    sqlx::query_as::<_, Exercise>(&format!(
        "UPDATE fit_exercise
            SET name = ?1, category = ?2, tracking_mode = ?3, primary_muscle = ?4,
                equipment = ?5, notes = ?6, updated_at = unixepoch()
          WHERE id = ?7 RETURNING {EXERCISE_COLUMNS}"
    ))
    .bind(input.name)
    .bind(input.category)
    .bind(input.tracking_mode)
    .bind(input.primary_muscle)
    .bind(input.equipment)
    .bind(input.notes)
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(server_err)?
    .ok_or_else(|| user_error("exercise not found"))
}

#[cfg(feature = "ssr")]
pub(crate) async fn archive_exercise_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
    archived: bool,
) -> Result<(), ServerFnError> {
    require_positive_id(id)?;
    let result = sqlx::query(
        "UPDATE fit_exercise SET archived = ?1, updated_at = unixepoch() WHERE id = ?2",
    )
    .bind(archived)
    .bind(id)
    .execute(pool)
    .await
    .map_err(server_err)?;
    if result.rows_affected() == 0 {
        return Err(user_error("exercise not found"));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
#[derive(Debug)]
pub(crate) struct MediaMetadataError {
    pub error: ServerFnError,
    pub cleanup_objects: bool,
}

#[cfg(feature = "ssr")]
impl From<ServerFnError> for MediaMetadataError {
    fn from(error: ServerFnError) -> Self {
        Self {
            error,
            cleanup_objects: true,
        }
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn add_media_metadata_batch_inner(
    pool: &sqlx::SqlitePool,
    exercise_id: i64,
    mut inputs: Vec<ExerciseMediaInput>,
) -> Result<Vec<i64>, MediaMetadataError> {
    require_positive_id(exercise_id)?;
    if inputs.is_empty() || inputs.len() > MAX_EXERCISE_MEDIA as usize {
        return Err(user_error("media batch must contain between 1 and 12 items").into());
    }
    for input in &mut inputs {
        validate_media_input(input)?;
    }
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fit_exercise WHERE id = ?1)")
            .bind(exercise_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(server_err)?;
    if !exists {
        return Err(user_error("exercise not found").into());
    }
    let existing_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM fit_exercise_media WHERE exercise_id = ?1")
            .bind(exercise_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(server_err)?;
    if existing_count.saturating_add(inputs.len() as i64) > MAX_EXERCISE_MEDIA {
        return Err(user_error("an exercise can have at most 12 media items").into());
    }
    let object_keys = inputs
        .iter()
        .map(|input| input.object_key.clone())
        .collect::<Vec<_>>();
    let mut ids = Vec::with_capacity(inputs.len());
    for (offset, input) in inputs.into_iter().enumerate() {
        let id = sqlx::query_scalar(
            "INSERT INTO fit_exercise_media
                (exercise_id, object_key, title, media_type, byte_size, sha256, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) RETURNING id",
        )
        .bind(exercise_id)
        .bind(input.object_key)
        .bind(input.title)
        .bind(input.media_type)
        .bind(input.byte_size)
        .bind(input.sha256)
        .bind(existing_count + offset as i64)
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| {
            if is_unique_violation(&error) {
                user_error("media object already exists")
            } else if error.to_string().contains("media limit") {
                user_error("an exercise can have at most 12 media items")
            } else {
                server_err(error)
            }
        })?;
        ids.push(id);
    }
    match tx.commit().await {
        Ok(()) => Ok(ids),
        Err(commit_error) => {
            let mut present = 0usize;
            for object_key in &object_keys {
                match sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM fit_exercise_media WHERE object_key = ?1)",
                )
                .bind(object_key)
                .fetch_one(pool)
                .await
                {
                    Ok(true) => present += 1,
                    Ok(false) => {}
                    Err(verification_error) => {
                        tracing::error!(
                            error = %commit_error,
                            verification_error = %verification_error,
                            "fitness media commit outcome could not be verified; preserving objects"
                        );
                        return Err(MediaMetadataError {
                            error: user_error(
                                "media metadata commit outcome is uncertain; uploaded objects were preserved for recovery",
                            ),
                            cleanup_objects: false,
                        });
                    }
                }
            }
            if present == object_keys.len() {
                tracing::warn!(
                    error = %commit_error,
                    "fitness media commit returned an error but every row is present"
                );
                Ok(ids)
            } else if present == 0 {
                Err(MediaMetadataError {
                    error: server_err(commit_error),
                    cleanup_objects: true,
                })
            } else {
                tracing::error!(
                    error = %commit_error,
                    present,
                    expected = object_keys.len(),
                    "fitness media commit outcome is partial; preserving objects"
                );
                Err(MediaMetadataError {
                    error: user_error(
                        "media metadata commit outcome is inconsistent; uploaded objects were preserved for recovery",
                    ),
                    cleanup_objects: false,
                })
            }
        }
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn delete_media_metadata_inner(
    pool: &sqlx::SqlitePool,
    exercise_id: i64,
    media_id: i64,
) -> Result<ExerciseMedia, ServerFnError> {
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let media = sqlx::query_as::<_, ExerciseMedia>(
        "DELETE FROM fit_exercise_media WHERE id = ?1 AND exercise_id = ?2
         RETURNING id, exercise_id, object_key, title, media_type, byte_size,
                   sha256, sort_order, created_at",
    )
    .bind(media_id)
    .bind(exercise_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(server_err)?
    .ok_or_else(|| user_error("exercise media not found"))?;
    normalize_media_order_tx(&mut tx, exercise_id).await?;
    tx.commit().await.map_err(server_err)?;
    Ok(media)
}

#[cfg(feature = "ssr")]
pub(crate) async fn reorder_media_inner(
    pool: &sqlx::SqlitePool,
    exercise_id: i64,
    ordered_ids: &[i64],
) -> Result<(), ServerFnError> {
    if ordered_ids.len() > MAX_EXERCISE_MEDIA as usize {
        return Err(user_error("an exercise can have at most 12 media items"));
    }
    let mut unique = ordered_ids.to_vec();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != ordered_ids.len() {
        return Err(user_error("media order contains duplicate ids"));
    }
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let current: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM fit_exercise_media WHERE exercise_id = ?1 ORDER BY sort_order",
    )
    .bind(exercise_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(server_err)?;
    let mut sorted_current = current;
    sorted_current.sort_unstable();
    if sorted_current != unique {
        return Err(user_error(
            "media order must contain every media item exactly once",
        ));
    }
    sqlx::query(
        "UPDATE fit_exercise_media SET sort_order = sort_order + 100 WHERE exercise_id = ?1",
    )
    .bind(exercise_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    for (position, id) in ordered_ids.iter().enumerate() {
        sqlx::query("UPDATE fit_exercise_media SET sort_order = ?1 WHERE id = ?2")
            .bind(position as i64)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
    }
    tx.commit().await.map_err(server_err)
}

#[cfg(feature = "ssr")]
async fn normalize_media_order_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    exercise_id: i64,
) -> Result<(), ServerFnError> {
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM fit_exercise_media WHERE exercise_id = ?1 ORDER BY sort_order, id",
    )
    .bind(exercise_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(server_err)?;
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE fit_exercise_media SET sort_order = sort_order + 100 WHERE exercise_id = ?1",
    )
    .bind(exercise_id)
    .execute(&mut **tx)
    .await
    .map_err(server_err)?;
    for (position, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE fit_exercise_media SET sort_order = ?1 WHERE id = ?2")
            .bind(position as i64)
            .bind(id)
            .execute(&mut **tx)
            .await
            .map_err(server_err)?;
    }
    Ok(())
}

#[cfg(feature = "ssr")]
fn validate_media_input(input: &mut ExerciseMediaInput) -> Result<(), ServerFnError> {
    input.object_key = input.object_key.trim().to_string();
    if input.object_key.is_empty()
        || input.object_key.len() > 128
        || input.object_key.contains('/')
        || input.object_key.contains('\\')
        || input.object_key == "."
        || input.object_key == ".."
        || !input
            .object_key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
    {
        return Err(user_error("invalid opaque media object key"));
    }
    if !matches!(input.media_type.as_str(), "gif" | "mp4" | "webm") {
        return Err(user_error("media type must be gif, mp4, or webm"));
    }
    if input.byte_size <= 0 {
        return Err(user_error("media size must be positive"));
    }
    input.sha256.make_ascii_lowercase();
    if input.sha256.len() != 64 || !input.sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(user_error("invalid media sha256"));
    }
    input.title = clean_optional(input.title.take(), MAX_NAME_CHARS)?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub(crate) async fn create_plan_inner(
    pool: &sqlx::SqlitePool,
    mut input: PlanInput,
) -> Result<i64, ServerFnError> {
    validate_plan_input(&mut input)?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let id: i64 =
        sqlx::query_scalar("INSERT INTO fit_plan (name, notes) VALUES (?1, ?2) RETURNING id")
            .bind(input.name)
            .bind(input.notes)
            .fetch_one(&mut *tx)
            .await
            .map_err(server_err)?;
    insert_plan_children(&mut tx, id, &input.exercises).await?;
    tx.commit().await.map_err(server_err)?;
    Ok(id)
}

#[cfg(feature = "ssr")]
pub(crate) async fn replace_plan_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
    mut input: PlanInput,
) -> Result<(), ServerFnError> {
    require_positive_id(id)?;
    validate_plan_input(&mut input)?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let changed = sqlx::query(
        "UPDATE fit_plan SET name = ?1, notes = ?2, updated_at = unixepoch() WHERE id = ?3",
    )
    .bind(input.name)
    .bind(input.notes)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    if changed.rows_affected() == 0 {
        return Err(user_error("plan not found"));
    }
    sqlx::query("DELETE FROM fit_plan_exercise WHERE plan_id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    insert_plan_children(&mut tx, id, &input.exercises).await?;
    tx.commit().await.map_err(server_err)
}

#[cfg(feature = "ssr")]
fn validate_plan_input(input: &mut PlanInput) -> Result<(), ServerFnError> {
    input.name = clean_required(&input.name, "plan name", MAX_NAME_CHARS)?;
    input.notes = clean_optional(input.notes.take(), MAX_NOTES_CHARS)?;
    if input.exercises.len() > 100 {
        return Err(user_error("a plan can contain at most 100 exercises"));
    }
    for exercise in &mut input.exercises {
        require_positive_id(exercise.exercise_id)?;
        exercise.notes = clean_optional(exercise.notes.take(), MAX_NOTES_CHARS)?;
        if exercise.sets.len() > 100 {
            return Err(user_error("an exercise can contain at most 100 sets"));
        }
        for set in &exercise.sets {
            validate_plan_set(set)?;
        }
    }
    Ok(())
}

#[cfg(feature = "ssr")]
fn validate_plan_set(set: &PlanSetInput) -> Result<(), ServerFnError> {
    if !matches!(
        set.set_type.as_str(),
        "warmup" | "working" | "drop" | "failure"
    ) {
        return Err(user_error("invalid set type"));
    }
    if !(0..=3_600).contains(&set.rest_seconds) {
        return Err(user_error("rest must be between 0 and 3600 seconds"));
    }
    positive_optional(set.target_reps, "target reps")?;
    nonnegative_optional(set.target_weight_g, "target weight")?;
    positive_optional(set.target_duration_s, "target duration")?;
    positive_optional(set.target_distance_m, "target distance")?;
    if set
        .target_rpe_x10
        .is_some_and(|value| !(10..=100).contains(&value))
    {
        return Err(user_error("target RPE must be between 1.0 and 10.0"));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn insert_plan_children(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    plan_id: i64,
    exercises: &[PlanExerciseInput],
) -> Result<(), ServerFnError> {
    for (position, exercise) in exercises.iter().enumerate() {
        let plan_exercise_id: i64 = sqlx::query_scalar(
            "INSERT INTO fit_plan_exercise (plan_id, exercise_id, position, notes)
             VALUES (?1, ?2, ?3, ?4) RETURNING id",
        )
        .bind(plan_id)
        .bind(exercise.exercise_id)
        .bind(position as i64)
        .bind(&exercise.notes)
        .fetch_one(&mut **tx)
        .await
        .map_err(server_err)?;
        for (set_position, set) in exercise.sets.iter().enumerate() {
            insert_plan_set(tx, plan_exercise_id, set_position as i64, set).await?;
        }
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn insert_plan_set(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    plan_exercise_id: i64,
    position: i64,
    set: &PlanSetInput,
) -> Result<i64, ServerFnError> {
    sqlx::query_scalar(
        "INSERT INTO fit_plan_set
            (plan_exercise_id, position, target_reps, target_weight_g,
             target_duration_s, target_distance_m, target_rpe_x10, set_type, rest_seconds)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) RETURNING id",
    )
    .bind(plan_exercise_id)
    .bind(position)
    .bind(set.target_reps)
    .bind(set.target_weight_g)
    .bind(set.target_duration_s)
    .bind(set.target_distance_m)
    .bind(set.target_rpe_x10)
    .bind(&set.set_type)
    .bind(set.rest_seconds)
    .fetch_one(&mut **tx)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub(crate) async fn start_workout_inner(
    pool: &sqlx::SqlitePool,
    plan_id: Option<i64>,
    notes: Option<String>,
    time: FitnessTime,
) -> Result<WorkoutDetail, ServerFnError> {
    if let Some(id) = plan_id {
        require_positive_id(id)?;
    }
    let notes = clean_optional(notes, MAX_NOTES_CHARS)?;
    let workout_date = time.workout_date(time.now)?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let plan_name: Option<String> = if let Some(plan_id) = plan_id {
        let plan_name =
            sqlx::query_scalar("SELECT name FROM fit_plan WHERE id = ?1 AND archived = 0")
                .bind(plan_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(server_err)?;
        Some(plan_name.ok_or_else(|| user_error("plan not found or archived"))?)
    } else {
        None
    };
    let workout_id: i64 = sqlx::query_scalar(
        "INSERT INTO fit_workout
            (plan_id, plan_name_snapshot, notes, workout_date,
             started_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?5) RETURNING id",
    )
    .bind(plan_id)
    .bind(&plan_name)
    .bind(notes)
    .bind(workout_date)
    .bind(time.now)
    .fetch_one(&mut *tx)
    .await
    .map_err(|error| {
        if is_unique_violation(&error) {
            user_error("an active workout already exists")
        } else {
            server_err(error)
        }
    })?;
    if let Some(plan_id) = plan_id {
        clone_plan_into_workout(&mut tx, plan_id, workout_id).await?;
    }
    tx.commit().await.map_err(server_err)?;
    load_workout_by_id(pool, workout_id)
        .await
        .map_err(server_err)?
        .ok_or_else(|| server_err("new workout disappeared"))
}

#[cfg(feature = "ssr")]
async fn clone_plan_into_workout(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    plan_id: i64,
    workout_id: i64,
) -> Result<(), ServerFnError> {
    let exercises: Vec<(i64, i64, String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT pe.id, pe.exercise_id, e.name, e.tracking_mode, pe.position, pe.notes
           FROM fit_plan_exercise pe JOIN fit_exercise e ON e.id = pe.exercise_id
          WHERE pe.plan_id = ?1 ORDER BY pe.position",
    )
    .bind(plan_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(server_err)?;
    for (plan_exercise_id, exercise_id, name, tracking_mode, position, notes) in exercises {
        let workout_exercise_id: i64 = sqlx::query_scalar(
            "INSERT INTO fit_workout_exercise
                (workout_id, exercise_id, exercise_name_snapshot,
                 tracking_mode_snapshot, position, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
        )
        .bind(workout_id)
        .bind(exercise_id)
        .bind(name)
        .bind(tracking_mode)
        .bind(position)
        .bind(notes)
        .fetch_one(&mut **tx)
        .await
        .map_err(server_err)?;
        sqlx::query(
            "INSERT INTO fit_workout_set
                (workout_exercise_id, position, target_reps, target_weight_g,
                 target_duration_s, target_distance_m, target_rpe_x10, set_type, rest_seconds)
             SELECT ?1, position, target_reps, target_weight_g, target_duration_s,
                    target_distance_m, target_rpe_x10, set_type, rest_seconds
               FROM fit_plan_set WHERE plan_exercise_id = ?2 ORDER BY position",
        )
        .bind(workout_exercise_id)
        .bind(plan_exercise_id)
        .execute(&mut **tx)
        .await
        .map_err(server_err)?;
    }
    Ok(())
}

#[cfg(feature = "ssr")]
pub(crate) async fn pause_workout_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
    expected_revision: i64,
    now: i64,
) -> Result<WorkoutDetail, ServerFnError> {
    require_revision(expected_revision)?;
    let result = sqlx::query(
        "UPDATE fit_workout
            SET status = 'paused', paused_at = ?3, revision = revision + 1,
                updated_at = ?3
          WHERE id = ?1 AND revision = ?2 AND status = 'in_progress'",
    )
    .bind(id)
    .bind(expected_revision)
    .bind(now)
    .execute(pool)
    .await
    .map_err(server_err)?;
    ensure_revision_update(pool, id, result.rows_affected()).await?;
    required_workout(pool, id).await
}

#[cfg(feature = "ssr")]
pub(crate) async fn resume_workout_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
    expected_revision: i64,
    now: i64,
) -> Result<WorkoutDetail, ServerFnError> {
    require_revision(expected_revision)?;
    let result = sqlx::query(
        "UPDATE fit_workout
            SET status = 'in_progress',
                paused_seconds = paused_seconds + MAX(0, ?3 - paused_at),
                paused_at = NULL, revision = revision + 1, updated_at = ?3
          WHERE id = ?1 AND revision = ?2 AND status = 'paused'",
    )
    .bind(id)
    .bind(expected_revision)
    .bind(now)
    .execute(pool)
    .await
    .map_err(server_err)?;
    ensure_revision_update(pool, id, result.rows_affected()).await?;
    required_workout(pool, id).await
}

#[cfg(feature = "ssr")]
pub(crate) async fn add_workout_exercise_inner(
    pool: &sqlx::SqlitePool,
    workout_id: i64,
    expected_revision: i64,
    exercise_id: i64,
    now: i64,
) -> Result<WorkoutDetail, ServerFnError> {
    require_revision(expected_revision)?;
    require_positive_id(exercise_id)?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    bump_active_revision(&mut tx, workout_id, expected_revision, now).await?;
    let exercise: Option<(String, String)> = sqlx::query_as(
        "SELECT name, tracking_mode FROM fit_exercise WHERE id = ?1 AND archived = 0",
    )
    .bind(exercise_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(server_err)?;
    let (name, tracking_mode) =
        exercise.ok_or_else(|| user_error("exercise not found or archived"))?;
    sqlx::query(
        "INSERT INTO fit_workout_exercise
            (workout_id, exercise_id, exercise_name_snapshot, tracking_mode_snapshot, position)
         VALUES (?1, ?2, ?3, ?4,
                 COALESCE((SELECT MAX(position) + 1 FROM fit_workout_exercise WHERE workout_id = ?1), 0))",
    )
    .bind(workout_id)
    .bind(exercise_id)
    .bind(name)
    .bind(tracking_mode)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    required_workout(pool, workout_id).await
}

#[cfg(feature = "ssr")]
pub(crate) async fn add_workout_set_inner(
    pool: &sqlx::SqlitePool,
    workout_id: i64,
    expected_revision: i64,
    workout_exercise_id: i64,
    input: PlanSetInput,
    now: i64,
) -> Result<WorkoutDetail, ServerFnError> {
    require_revision(expected_revision)?;
    validate_plan_set(&input)?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    bump_active_revision(&mut tx, workout_id, expected_revision, now).await?;
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM fit_workout_exercise
             WHERE id = ?1 AND workout_id = ?2
         )",
    )
    .bind(workout_exercise_id)
    .bind(workout_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(server_err)?;
    if !belongs {
        return Err(user_error("workout exercise not found"));
    }
    sqlx::query(
        "INSERT INTO fit_workout_set
            (workout_exercise_id, position, target_reps, target_weight_g,
             target_duration_s, target_distance_m, target_rpe_x10, set_type, rest_seconds)
         VALUES (?1,
                 COALESCE((SELECT MAX(position) + 1 FROM fit_workout_set WHERE workout_exercise_id = ?1), 0),
                 ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(workout_exercise_id)
    .bind(input.target_reps)
    .bind(input.target_weight_g)
    .bind(input.target_duration_s)
    .bind(input.target_distance_m)
    .bind(input.target_rpe_x10)
    .bind(input.set_type)
    .bind(input.rest_seconds)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    required_workout(pool, workout_id).await
}

#[cfg(feature = "ssr")]
pub(crate) async fn save_workout_set_inner(
    pool: &sqlx::SqlitePool,
    workout_id: i64,
    set_id: i64,
    expected_revision: i64,
    input: SetResultInput,
    now: i64,
) -> Result<WorkoutDetail, ServerFnError> {
    require_revision(expected_revision)?;
    validate_set_result(&input)?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    bump_active_revision(&mut tx, workout_id, expected_revision, now).await?;
    let completed_at = (input.status == "completed").then_some(now);
    let result = sqlx::query(
        "UPDATE fit_workout_set
            SET actual_reps = ?1, actual_weight_g = ?2, actual_duration_s = ?3,
                actual_distance_m = ?4, actual_rpe_x10 = ?5, status = ?6, completed_at = ?7
          WHERE id = ?8 AND workout_exercise_id IN
                (SELECT id FROM fit_workout_exercise WHERE workout_id = ?9)",
    )
    .bind(input.actual_reps)
    .bind(input.actual_weight_g)
    .bind(input.actual_duration_s)
    .bind(input.actual_distance_m)
    .bind(input.actual_rpe_x10)
    .bind(input.status)
    .bind(completed_at)
    .bind(set_id)
    .bind(workout_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    if result.rows_affected() == 0 {
        return Err(user_error("workout set not found"));
    }
    tx.commit().await.map_err(server_err)?;
    required_workout(pool, workout_id).await
}

#[cfg(feature = "ssr")]
fn validate_set_result(input: &SetResultInput) -> Result<(), ServerFnError> {
    if !matches!(input.status.as_str(), "pending" | "completed" | "skipped") {
        return Err(user_error("invalid set status"));
    }
    positive_optional(input.actual_reps, "actual reps")?;
    nonnegative_optional(input.actual_weight_g, "actual weight")?;
    positive_optional(input.actual_duration_s, "actual duration")?;
    positive_optional(input.actual_distance_m, "actual distance")?;
    if input
        .actual_rpe_x10
        .is_some_and(|value| !(10..=100).contains(&value))
    {
        return Err(user_error("actual RPE must be between 1.0 and 10.0"));
    }
    if input.status == "completed"
        && input.actual_reps.is_none()
        && input.actual_duration_s.is_none()
        && input.actual_distance_m.is_none()
    {
        return Err(user_error(
            "a completed set needs reps, duration, or distance",
        ));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
pub(crate) async fn finish_workout_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
    expected_revision: i64,
    time: FitnessTime,
) -> Result<FinishWorkoutResult, ServerFnError> {
    require_revision(expected_revision)?;
    let workout_date = time.workout_date(time.now)?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let completed_sets: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fit_workout_set s
          JOIN fit_workout_exercise e ON e.id = s.workout_exercise_id
         WHERE e.workout_id = ?1 AND s.status = 'completed'",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(server_err)?;
    if completed_sets == 0 {
        return Err(user_error("complete at least one set before finishing"));
    }
    let old_records: HashMap<(i64, String), i64> = sqlx::query_as::<_, (i64, String, i64)>(
        "SELECT exercise_id, kind, value_g FROM fit_personal_record",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(server_err)?
    .into_iter()
    .map(|(exercise_id, kind, value)| ((exercise_id, kind), value))
    .collect();
    sqlx::query(
        "UPDATE fit_workout_set SET status = 'skipped', completed_at = NULL
          WHERE status = 'pending' AND workout_exercise_id IN
                (SELECT id FROM fit_workout_exercise WHERE workout_id = ?1)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    let result = sqlx::query(
        "UPDATE fit_workout
            SET status = 'completed', ended_at = ?3, workout_date = ?4,
                paused_seconds = paused_seconds +
                    CASE WHEN status = 'paused' THEN MAX(0, ?3 - paused_at) ELSE 0 END,
                paused_at = NULL, revision = revision + 1, updated_at = ?3
          WHERE id = ?1 AND revision = ?2 AND status IN ('in_progress', 'paused')",
    )
    .bind(id)
    .bind(expected_revision)
    .bind(time.now)
    .bind(workout_date)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    ensure_revision_update_tx(&mut tx, id, result.rows_affected()).await?;
    recompute_personal_records(&mut tx).await?;
    let records = load_personal_records_tx(&mut tx).await?;
    let current_set_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT s.id FROM fit_workout_set s JOIN fit_workout_exercise e
             ON e.id = s.workout_exercise_id WHERE e.workout_id = ?1",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(server_err)?;
    let new_records = records
        .into_iter()
        .filter(|record| {
            current_set_ids.contains(&record.workout_set_id)
                && old_records
                    .get(&(record.exercise_id, record.kind.clone()))
                    .is_none_or(|old| record.value_g > *old)
        })
        .collect();
    tx.commit().await.map_err(server_err)?;
    let workout = required_workout(pool, id).await?;
    Ok(FinishWorkoutResult {
        workout,
        new_records,
    })
}

#[cfg(feature = "ssr")]
pub(crate) async fn discard_workout_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
    expected_revision: i64,
) -> Result<(), ServerFnError> {
    require_revision(expected_revision)?;
    let result = sqlx::query(
        "DELETE FROM fit_workout
          WHERE id = ?1 AND revision = ?2 AND status IN ('in_progress', 'paused')",
    )
    .bind(id)
    .bind(expected_revision)
    .execute(pool)
    .await
    .map_err(server_err)?;
    ensure_revision_update(pool, id, result.rows_affected()).await
}

#[cfg(feature = "ssr")]
pub(crate) async fn quick_log_inner(
    pool: &sqlx::SqlitePool,
    mut input: QuickLogInput,
    time: FitnessTime,
) -> Result<FinishWorkoutResult, ServerFnError> {
    if input.exercises.is_empty() || input.exercises.len() > 100 {
        return Err(user_error("quick log needs between 1 and 100 exercises"));
    }
    input.notes = clean_optional(input.notes.take(), MAX_NOTES_CHARS)?;
    let occurred_at = input.occurred_at.unwrap_or(time.now);
    if !ep_core::is_valid_app_timestamp(occurred_at) {
        return Err(user_error("invalid workout timestamp"));
    }
    let workout_date = time.workout_date(occurred_at)?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let old_records: HashMap<(i64, String), i64> = sqlx::query_as::<_, (i64, String, i64)>(
        "SELECT exercise_id, kind, value_g FROM fit_personal_record",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(server_err)?
    .into_iter()
    .map(|(exercise_id, kind, value)| ((exercise_id, kind), value))
    .collect();
    let workout_id: i64 = sqlx::query_scalar(
        "INSERT INTO fit_workout
            (status, workout_date, started_at, ended_at, paused_seconds, revision, notes)
         VALUES ('completed', ?1, ?2, ?2, 0, 1, ?3) RETURNING id",
    )
    .bind(workout_date)
    .bind(occurred_at)
    .bind(&input.notes)
    .fetch_one(&mut *tx)
    .await
    .map_err(server_err)?;
    let mut workout_set_ids = Vec::new();
    for (position, exercise_input) in input.exercises.into_iter().enumerate() {
        if exercise_input.sets.is_empty() || exercise_input.sets.len() > 100 {
            return Err(user_error(
                "each quick-log exercise needs between 1 and 100 sets",
            ));
        }
        let (exercise_id, name, tracking_mode) = resolve_quick_log_exercise(
            &mut tx,
            exercise_input.exercise_id,
            exercise_input.new_exercise_name,
            exercise_input.tracking_mode,
        )
        .await?;
        let workout_exercise_id: i64 = sqlx::query_scalar(
            "INSERT INTO fit_workout_exercise
                (workout_id, exercise_id, exercise_name_snapshot,
                 tracking_mode_snapshot, position)
             VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
        )
        .bind(workout_id)
        .bind(exercise_id)
        .bind(name)
        .bind(tracking_mode)
        .bind(position as i64)
        .fetch_one(&mut *tx)
        .await
        .map_err(server_err)?;
        for (set_position, set) in exercise_input.sets.into_iter().enumerate() {
            let result = SetResultInput {
                actual_reps: set.reps,
                actual_weight_g: set.weight_g,
                actual_duration_s: set.duration_s,
                actual_distance_m: set.distance_m,
                actual_rpe_x10: set.rpe_x10,
                status: "completed".into(),
            };
            validate_set_result(&result)?;
            if !matches!(
                set.set_type.as_str(),
                "warmup" | "working" | "drop" | "failure"
            ) {
                return Err(user_error("invalid set type"));
            }
            let set_id: i64 = sqlx::query_scalar(
                "INSERT INTO fit_workout_set
                    (workout_exercise_id, position, actual_reps, actual_weight_g,
                     actual_duration_s, actual_distance_m, actual_rpe_x10,
                     set_type, status, rest_seconds, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'completed', 0, ?9)
                 RETURNING id",
            )
            .bind(workout_exercise_id)
            .bind(set_position as i64)
            .bind(result.actual_reps)
            .bind(result.actual_weight_g)
            .bind(result.actual_duration_s)
            .bind(result.actual_distance_m)
            .bind(result.actual_rpe_x10)
            .bind(set.set_type)
            .bind(occurred_at)
            .fetch_one(&mut *tx)
            .await
            .map_err(server_err)?;
            workout_set_ids.push(set_id);
        }
    }
    recompute_personal_records(&mut tx).await?;
    let records = load_personal_records_tx(&mut tx).await?;
    let new_records = records
        .into_iter()
        .filter(|record| {
            workout_set_ids.contains(&record.workout_set_id)
                && old_records
                    .get(&(record.exercise_id, record.kind.clone()))
                    .is_none_or(|old| record.value_g > *old)
        })
        .collect();
    tx.commit().await.map_err(server_err)?;
    Ok(FinishWorkoutResult {
        workout: required_workout(pool, workout_id).await?,
        new_records,
    })
}

#[cfg(feature = "ssr")]
async fn resolve_quick_log_exercise(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    exercise_id: Option<i64>,
    new_name: Option<String>,
    tracking_mode: Option<String>,
) -> Result<(i64, String, String), ServerFnError> {
    match exercise_id {
        Some(id) => {
            require_positive_id(id)?;
            sqlx::query_as(
                "SELECT id, name, tracking_mode FROM fit_exercise
                  WHERE id = ?1 AND archived = 0",
            )
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(server_err)?
            .ok_or_else(|| user_error("exercise not found or archived"))
        }
        None => {
            let name = clean_required(
                new_name.as_deref().unwrap_or_default(),
                "new exercise name",
                MAX_NAME_CHARS,
            )?;
            let tracking_mode = tracking_mode.unwrap_or_else(|| "weighted".into());
            if !matches!(
                tracking_mode.as_str(),
                "weighted" | "reps" | "duration" | "distance" | "bodyweight" | "assisted"
            ) {
                return Err(user_error("invalid exercise tracking mode"));
            }
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO fit_exercise (name, tracking_mode)
                 VALUES (?1, ?2) RETURNING id",
            )
            .bind(&name)
            .bind(&tracking_mode)
            .fetch_one(&mut **tx)
            .await
            .map_err(|error| {
                if is_unique_violation(&error) {
                    user_error("an exercise with this name already exists")
                } else {
                    server_err(error)
                }
            })?;
            Ok((id, name, tracking_mode))
        }
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn add_body_measurement_inner(
    pool: &sqlx::SqlitePool,
    measured_at: Option<i64>,
    now: i64,
    weight_g: Option<i64>,
    body_fat_bp: Option<i64>,
    waist_mm: Option<i64>,
    notes: Option<String>,
) -> Result<i64, ServerFnError> {
    if weight_g.is_none() && body_fat_bp.is_none() && waist_mm.is_none() {
        return Err(user_error("enter at least one body measurement"));
    }
    positive_optional(weight_g, "weight")?;
    positive_optional(waist_mm, "waist")?;
    if body_fat_bp.is_some_and(|value| !(1..=10_000).contains(&value)) {
        return Err(user_error("body fat must be between 0.01% and 100%"));
    }
    let measured_at = measured_at.unwrap_or(now);
    if !ep_core::is_valid_app_timestamp(measured_at) {
        return Err(user_error("invalid measurement timestamp"));
    }
    let notes = clean_optional(notes, MAX_NOTES_CHARS)?;
    sqlx::query_scalar(
        "INSERT INTO fit_body_measurement
            (measured_at, weight_g, body_fat_bp, waist_mm, notes)
         VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
    )
    .bind(measured_at)
    .bind(weight_g)
    .bind(body_fat_bp)
    .bind(waist_mm)
    .bind(notes)
    .fetch_one(pool)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
pub(crate) async fn delete_body_measurement_inner(
    pool: &sqlx::SqlitePool,
    id: i64,
) -> Result<(), ServerFnError> {
    let result = sqlx::query("DELETE FROM fit_body_measurement WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(server_err)?;
    if result.rows_affected() == 0 {
        return Err(user_error("body measurement not found"));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
pub(crate) async fn revise_completed_set_inner(
    pool: &sqlx::SqlitePool,
    workout_id: i64,
    set_id: i64,
    input: SetResultInput,
    now: i64,
) -> Result<WorkoutDetail, ServerFnError> {
    validate_set_result(&input)?;
    if input.status == "pending" {
        return Err(user_error("a historical set cannot be pending"));
    }
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let completed_at = (input.status == "completed").then_some(now);
    let result = sqlx::query(
        "UPDATE fit_workout_set
            SET actual_reps = ?1, actual_weight_g = ?2, actual_duration_s = ?3,
                actual_distance_m = ?4, actual_rpe_x10 = ?5, status = ?6,
                completed_at = ?7
          WHERE id = ?8 AND workout_exercise_id IN (
                SELECT e.id FROM fit_workout_exercise e JOIN fit_workout w ON w.id = e.workout_id
                 WHERE e.workout_id = ?9 AND w.status = 'completed'
          )",
    )
    .bind(input.actual_reps)
    .bind(input.actual_weight_g)
    .bind(input.actual_duration_s)
    .bind(input.actual_distance_m)
    .bind(input.actual_rpe_x10)
    .bind(input.status)
    .bind(completed_at)
    .bind(set_id)
    .bind(workout_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    if result.rows_affected() == 0 {
        return Err(user_error("completed workout set not found"));
    }
    recompute_personal_records(&mut tx).await?;
    tx.commit().await.map_err(server_err)?;
    required_workout(pool, workout_id).await
}

#[cfg(feature = "ssr")]
pub(crate) async fn delete_completed_workout_inner(
    pool: &sqlx::SqlitePool,
    workout_id: i64,
) -> Result<(), ServerFnError> {
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(server_err)?;
    let result = sqlx::query("DELETE FROM fit_workout WHERE id = ?1 AND status = 'completed'")
        .bind(workout_id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    if result.rows_affected() == 0 {
        return Err(user_error("completed workout not found"));
    }
    recompute_personal_records(&mut tx).await?;
    tx.commit().await.map_err(server_err)
}

#[cfg(feature = "ssr")]
async fn recompute_personal_records(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<(), ServerFnError> {
    #[derive(sqlx::FromRow)]
    struct Candidate {
        set_id: i64,
        exercise_id: i64,
        achieved_at: i64,
        weight_g: i64,
        reps: i64,
    }

    let candidates = sqlx::query_as::<_, Candidate>(
        "SELECT s.id AS set_id, e.exercise_id, w.ended_at AS achieved_at,
                s.actual_weight_g AS weight_g, s.actual_reps AS reps
           FROM fit_workout_set s
           JOIN fit_workout_exercise e ON e.id = s.workout_exercise_id
           JOIN fit_workout w ON w.id = e.workout_id
          WHERE w.status = 'completed'
            AND s.status = 'completed'
            AND s.set_type <> 'warmup'
            AND e.exercise_id IS NOT NULL
            AND e.tracking_mode_snapshot = 'weighted'
            AND s.actual_weight_g > 0
            AND s.actual_reps > 0
          ORDER BY w.ended_at, s.id",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(server_err)?;

    #[derive(Clone)]
    struct Best {
        value: i64,
        set_id: i64,
        achieved_at: i64,
    }
    let mut best: HashMap<(i64, &'static str), Best> = HashMap::new();
    for candidate in candidates {
        let max_weight = Best {
            value: candidate.weight_g,
            set_id: candidate.set_id,
            achieved_at: candidate.achieved_at,
        };
        let max_key = (candidate.exercise_id, "max_weight");
        match best.get_mut(&max_key) {
            Some(current) if max_weight.value > current.value => *current = max_weight,
            None => {
                best.insert(max_key, max_weight);
            }
            _ => {}
        }
        if let Some(value) = epley_1rm_g(candidate.weight_g, candidate.reps) {
            let one_rm = Best {
                value,
                set_id: candidate.set_id,
                achieved_at: candidate.achieved_at,
            };
            let one_rm_key = (candidate.exercise_id, "estimated_1rm");
            match best.get_mut(&one_rm_key) {
                Some(current) if one_rm.value > current.value => *current = one_rm,
                None => {
                    best.insert(one_rm_key, one_rm);
                }
                _ => {}
            }
        }
    }
    sqlx::query("DELETE FROM fit_personal_record")
        .execute(&mut **tx)
        .await
        .map_err(server_err)?;
    for ((exercise_id, kind), record) in best {
        sqlx::query(
            "INSERT INTO fit_personal_record
                (exercise_id, kind, value_g, workout_set_id, achieved_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(exercise_id)
        .bind(kind)
        .bind(record.value)
        .bind(record.set_id)
        .bind(record.achieved_at)
        .execute(&mut **tx)
        .await
        .map_err(server_err)?;
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn load_personal_records_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<Vec<PersonalRecord>, ServerFnError> {
    sqlx::query_as(
        "SELECT pr.id, pr.exercise_id, e.name AS exercise_name, pr.kind, pr.value_g,
                pr.workout_set_id, pr.achieved_at
           FROM fit_personal_record pr JOIN fit_exercise e ON e.id = pr.exercise_id
          ORDER BY e.name, pr.kind",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(server_err)
}

#[cfg(feature = "ssr")]
async fn bump_active_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    workout_id: i64,
    expected_revision: i64,
    now: i64,
) -> Result<(), ServerFnError> {
    let result = sqlx::query(
        "UPDATE fit_workout SET revision = revision + 1, updated_at = ?3
          WHERE id = ?1 AND revision = ?2 AND status IN ('in_progress', 'paused')",
    )
    .bind(workout_id)
    .bind(expected_revision)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(server_err)?;
    ensure_revision_update_tx(tx, workout_id, result.rows_affected()).await
}

#[cfg(feature = "ssr")]
async fn ensure_revision_update(
    pool: &sqlx::SqlitePool,
    id: i64,
    rows_affected: u64,
) -> Result<(), ServerFnError> {
    if rows_affected > 0 {
        return Ok(());
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fit_workout WHERE id = ?1)")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(server_err)?;
    if exists {
        Err(user_error(
            "workout changed in another tab; reload before saving",
        ))
    } else {
        Err(user_error("workout not found"))
    }
}

#[cfg(feature = "ssr")]
async fn ensure_revision_update_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    rows_affected: u64,
) -> Result<(), ServerFnError> {
    if rows_affected > 0 {
        return Ok(());
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fit_workout WHERE id = ?1)")
        .bind(id)
        .fetch_one(&mut **tx)
        .await
        .map_err(server_err)?;
    if exists {
        Err(user_error(
            "workout changed in another tab; reload before saving",
        ))
    } else {
        Err(user_error("workout not found"))
    }
}

#[cfg(feature = "ssr")]
async fn required_workout(
    pool: &sqlx::SqlitePool,
    id: i64,
) -> Result<WorkoutDetail, ServerFnError> {
    load_workout_by_id(pool, id)
        .await
        .map_err(server_err)?
        .ok_or_else(|| user_error("workout not found"))
}

#[cfg(feature = "ssr")]
fn require_revision(revision: i64) -> Result<(), ServerFnError> {
    if revision <= 0 {
        Err(user_error("expected_revision must be positive"))
    } else {
        Ok(())
    }
}

#[cfg(feature = "ssr")]
fn require_positive_id(id: i64) -> Result<(), ServerFnError> {
    if id <= 0 {
        Err(user_error("id must be positive"))
    } else {
        Ok(())
    }
}

#[cfg(feature = "ssr")]
fn clean_required(value: &str, field: &str, max_chars: usize) -> Result<String, ServerFnError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(user_error(format!("{field} is required")));
    }
    if value.chars().count() > max_chars {
        return Err(user_error(format!(
            "{field} must be at most {max_chars} characters"
        )));
    }
    Ok(value.to_string())
}

#[cfg(feature = "ssr")]
fn clean_optional(
    value: Option<String>,
    max_chars: usize,
) -> Result<Option<String>, ServerFnError> {
    let value = value.map(|value| value.trim().to_string());
    if value
        .as_deref()
        .is_some_and(|value| value.chars().count() > max_chars)
    {
        return Err(user_error(format!(
            "value must be at most {max_chars} characters"
        )));
    }
    Ok(value.filter(|value| !value.is_empty()))
}

#[cfg(feature = "ssr")]
fn positive_optional(value: Option<i64>, field: &str) -> Result<(), ServerFnError> {
    if value.is_some_and(|value| value <= 0) {
        Err(user_error(format!("{field} must be positive")))
    } else {
        Ok(())
    }
}

#[cfg(feature = "ssr")]
fn nonnegative_optional(value: Option<i64>, field: &str) -> Result<(), ServerFnError> {
    if value.is_some_and(|value| value < 0) {
        Err(user_error(format!("{field} cannot be negative")))
    } else {
        Ok(())
    }
}

#[cfg(feature = "ssr")]
fn is_unique_violation(error: &sqlx::Error) -> bool {
    match error {
        sqlx::Error::Database(error) => error.is_unique_violation(),
        _ => false,
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    fn test_time(now: i64) -> FitnessTime {
        FitnessTime {
            timezone: ep_core::AppTimezone::utc(),
            now,
        }
    }

    fn test_time_in(timezone: &str, now: i64) -> FitnessTime {
        FitnessTime {
            timezone: ep_core::AppTimezone::parse(timezone).expect("valid test timezone"),
            now,
        }
    }

    async fn pool() -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../migrations/001_fitness.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    async fn exercise(pool: &sqlx::SqlitePool, name: &str) -> i64 {
        create_exercise_inner(
            pool,
            ExerciseInput {
                name: name.into(),
                category: "strength".into(),
                tracking_mode: "weighted".into(),
                primary_muscle: None,
                equipment: None,
                notes: None,
            },
        )
        .await
        .unwrap()
    }

    async fn completed_weighted_workout(
        pool: &sqlx::SqlitePool,
        exercise_id: i64,
        ended_at: i64,
        sets: &[(i64, i64)],
    ) -> i64 {
        let workout_date = ep_core::AppTimezone::utc().fmt_ymd(Some(ended_at));
        completed_weighted_workout_on(pool, exercise_id, ended_at, &workout_date, sets).await
    }

    async fn completed_weighted_workout_on(
        pool: &sqlx::SqlitePool,
        exercise_id: i64,
        ended_at: i64,
        workout_date: &str,
        sets: &[(i64, i64)],
    ) -> i64 {
        let workout_id: i64 = sqlx::query_scalar(
            "INSERT INTO fit_workout (status, workout_date, started_at, ended_at)
             VALUES ('completed', ?1, ?2, ?3) RETURNING id",
        )
        .bind(workout_date)
        .bind(ended_at - 3_600)
        .bind(ended_at)
        .fetch_one(pool)
        .await
        .unwrap();
        let workout_exercise_id: i64 = sqlx::query_scalar(
            "INSERT INTO fit_workout_exercise
                (workout_id, exercise_id, exercise_name_snapshot,
                 tracking_mode_snapshot, position)
             SELECT ?1, id, name, tracking_mode, 0
               FROM fit_exercise WHERE id = ?2
             RETURNING id",
        )
        .bind(workout_id)
        .bind(exercise_id)
        .fetch_one(pool)
        .await
        .unwrap();
        for (position, (weight_g, reps)) in sets.iter().enumerate() {
            sqlx::query(
                "INSERT INTO fit_workout_set
                    (workout_exercise_id, position, actual_weight_g, actual_reps,
                     status, completed_at)
                 VALUES (?1, ?2, ?3, ?4, 'completed', ?5)",
            )
            .bind(workout_exercise_id)
            .bind(i64::try_from(position).unwrap())
            .bind(weight_g)
            .bind(reps)
            .bind(ended_at)
            .execute(pool)
            .await
            .unwrap();
        }
        workout_id
    }

    fn quick_log_input(exercise_id: i64, occurred_at: i64) -> QuickLogInput {
        QuickLogInput {
            occurred_at: Some(occurred_at),
            notes: None,
            exercises: vec![QuickLogExerciseInput {
                exercise_id: Some(exercise_id),
                new_exercise_name: None,
                tracking_mode: None,
                sets: vec![QuickLogSetInput {
                    reps: Some(5),
                    weight_g: Some(80_000),
                    duration_s: None,
                    distance_m: None,
                    rpe_x10: None,
                    set_type: "working".into(),
                }],
            }],
        }
    }

    #[tokio::test]
    async fn analytics_zero_fill_weeks_and_convert_body_and_strength_units() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "Analytics squat").await;
        let now = 1_768_392_000_i64; // 2026-01-14 12:00:00 UTC
        completed_weighted_workout(
            &pool,
            exercise_id,
            now - 86_400,
            &[(100_000, 5), (80_000, 10)],
        )
        .await;
        completed_weighted_workout(&pool, exercise_id, now - 60 * 86_400, &[(90_000, 5)]).await;
        sqlx::query(
            "INSERT INTO fit_body_measurement
                (measured_at, weight_g, body_fat_bp, waist_mm)
             VALUES (?1, 45359, 1825, 813), (?2, 50000, 2000, 900)",
        )
        .bind(now - 10 * 86_400)
        .bind(now - 100 * 86_400)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE fit_settings
                SET unit_system = 'imperial', weekly_workout_target = 3
              WHERE id = 1",
        )
        .execute(&pool)
        .await
        .unwrap();

        let analytics = load_fitness_analytics_inner(
            &pool,
            "weight",
            30,
            Some(exercise_id),
            90,
            test_time(now),
        )
        .await
        .unwrap();
        assert_eq!(analytics.weekly_activity.len(), 52);
        assert!(analytics
            .weekly_activity
            .windows(2)
            .all(|weeks| weeks[0].week_start < weeks[1].week_start));
        assert_eq!(analytics.workout_target.completed, 1);
        assert_eq!(analytics.workout_target.target, 3);
        let summary_trend = fitness_summary_trend_from_weeks(
            analytics
                .weekly_activity
                .iter()
                .rev()
                .take(8)
                .cloned()
                .rev(),
        )
        .unwrap();
        assert_eq!(summary_trend.points.len(), 8);
        assert_eq!(summary_trend.points.last().unwrap().display, "1");
        assert_eq!(analytics.body_metric.unit, "lb");
        assert_eq!(analytics.body_metric.points.len(), 1);
        assert!((analytics.body_metric.points[0].value.value - 100.0).abs() < 0.01);
        assert_eq!(analytics.body_metric.points[0].value.display, "100.00 lb");
        assert_eq!(analytics.strength_trend.len(), 2);

        let latest = analytics.strength_trend.last().unwrap();
        let estimated = latest.estimated_1rm.as_ref().unwrap();
        assert!(
            (estimated.value - grams_to_display_weight(116_667, UnitSystem::Imperial)).abs()
                < 0.001
        );
        assert!(estimated.display.ends_with(" lb"));
        assert!(
            (latest.volume.value - grams_to_display_weight(1_300_000, UnitSystem::Imperial)).abs()
                < 0.001
        );
    }

    #[tokio::test]
    async fn weekly_activity_uses_rust_iso_week_boundaries_across_dst() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "DST squat").await;
        let timezone = ep_core::AppTimezone::parse("America/New_York").unwrap();
        let now = timezone
            .date_midpoint(ep_core::CalendarDate {
                year: 2024,
                month: 3,
                day: 17,
            })
            .unwrap();
        let time = test_time_in("America/New_York", now);
        let ranges = timezone.recent_weeks(now, 2).unwrap();
        assert_eq!(ranges[0].label, "2024-03-04");
        assert_eq!(ranges[1].label, "2024-03-11");
        assert_eq!(ranges[0].end - ranges[0].start, 167 * 3_600);

        completed_weighted_workout_on(
            &pool,
            exercise_id,
            ranges[0].end - 1,
            "2024-03-10",
            &[(80_000, 5)],
        )
        .await;
        completed_weighted_workout_on(
            &pool,
            exercise_id,
            ranges[1].start,
            "2024-03-11",
            &[(90_000, 5)],
        )
        .await;
        completed_weighted_workout_on(&pool, exercise_id, now + 1, "2024-03-17", &[(100_000, 5)])
            .await;

        let weeks = weekly_activity_inner(&pool, time, 2).await.unwrap();
        assert_eq!(weeks.len(), 2);
        assert_eq!(weeks[0].completed_workouts, 1);
        assert_eq!(weeks[0].completed_sets, 1);
        assert_eq!(weeks[1].completed_workouts, 1);
        assert_eq!(weeks[1].completed_sets, 1);
    }

    #[tokio::test]
    async fn body_trend_uses_local_calendar_days_and_half_open_now_bound() {
        let pool = pool().await;
        let timezone = ep_core::AppTimezone::parse("America/Los_Angeles").unwrap();
        let now = timezone
            .date_midpoint(ep_core::CalendarDate {
                year: 2024,
                month: 3,
                day: 10,
            })
            .unwrap();
        let time = test_time_in("America/Los_Angeles", now);
        let (start, end) = time.trailing_days(2).unwrap();
        sqlx::query(
            "INSERT INTO fit_body_measurement (measured_at, weight_g)
             VALUES (?1, 70000), (?2, 71000), (?3, 72000), (?4, 73000)",
        )
        .bind(start - 1)
        .bind(start)
        .bind(now)
        .bind(end)
        .execute(&pool)
        .await
        .unwrap();

        let trend = body_metric_trend_inner(&pool, BodyMetric::Weight, 2, UnitSystem::Metric, time)
            .await
            .unwrap();
        assert_eq!(
            trend
                .points
                .iter()
                .map(|point| point.label.as_str())
                .collect::<Vec<_>>(),
            ["2024-03-09", "2024-03-10"]
        );
    }

    #[tokio::test]
    async fn streak_uses_the_persisted_workout_date() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "Midnight row").await;
        let timezone = ep_core::AppTimezone::parse("America/New_York").unwrap();
        let january_first = timezone
            .date_start(ep_core::CalendarDate {
                year: 2026,
                month: 1,
                day: 1,
            })
            .unwrap();
        let january_second = timezone
            .date_start(ep_core::CalendarDate {
                year: 2026,
                month: 1,
                day: 2,
            })
            .unwrap();
        let now = january_second + 30 * 60;
        completed_weighted_workout_on(
            &pool,
            exercise_id,
            january_first + 23 * 3_600 + 30 * 60,
            "2026-01-01",
            &[(80_000, 5)],
        )
        .await;
        completed_weighted_workout_on(
            &pool,
            exercise_id,
            january_second + 10 * 60,
            "2026-01-02",
            &[(80_000, 5)],
        )
        .await;

        let summary = home_summary_inner(&pool, test_time_in("America/New_York", now))
            .await
            .unwrap();
        assert_eq!(summary.streak_days, 2);
    }

    #[tokio::test]
    async fn streak_skips_a_nonexistent_civil_date() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "Dateline row").await;
        let timezone = ep_core::AppTimezone::parse("Pacific/Apia").unwrap();
        let december_29 = ep_core::CalendarDate {
            year: 2011,
            month: 12,
            day: 29,
        };
        let december_31 = ep_core::CalendarDate {
            year: 2011,
            month: 12,
            day: 31,
        };
        let first = timezone.date_midpoint(december_29).unwrap();
        let now = timezone.date_midpoint(december_31).unwrap();
        completed_weighted_workout_on(&pool, exercise_id, first, "2011-12-29", &[(80_000, 5)])
            .await;
        completed_weighted_workout_on(&pool, exercise_id, now, "2011-12-31", &[(80_000, 5)]).await;

        let summary = home_summary_inner(&pool, test_time_in("Pacific/Apia", now))
            .await
            .unwrap();
        assert_eq!(summary.streak_days, 2);
    }

    #[tokio::test]
    async fn quick_log_business_dates_survive_a_timezone_change() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "Timezone squat").await;
        let shanghai = ep_core::AppTimezone::parse("Asia/Shanghai").unwrap();
        let los_angeles = ep_core::AppTimezone::parse("America/Los_Angeles").unwrap();
        let january_13 = ep_core::CalendarDate {
            year: 2026,
            month: 1,
            day: 13,
        };
        let january_14 = ep_core::CalendarDate {
            year: 2026,
            month: 1,
            day: 14,
        };
        let first = shanghai.date_midpoint(january_13).unwrap();
        let second = shanghai.date_midpoint(january_14).unwrap();
        let now = los_angeles.date_midpoint(january_14).unwrap();
        let shanghai_time = test_time_in("Asia/Shanghai", now);
        quick_log_inner(&pool, quick_log_input(exercise_id, first), shanghai_time)
            .await
            .unwrap();
        quick_log_inner(&pool, quick_log_input(exercise_id, second), shanghai_time)
            .await
            .unwrap();

        let los_angeles_time = test_time_in("America/Los_Angeles", now);
        let summary = home_summary_inner(&pool, los_angeles_time).await.unwrap();
        assert_eq!(summary.completed_workouts_this_week, 2);
        assert_eq!(summary.streak_days, 2);

        let analytics = load_fitness_analytics_inner(
            &pool,
            "weight",
            30,
            Some(exercise_id),
            90,
            los_angeles_time,
        )
        .await
        .unwrap();
        assert_eq!(
            analytics
                .strength_trend
                .iter()
                .map(|point| point.label.as_str())
                .collect::<Vec<_>>(),
            ["2026-01-13", "2026-01-14"]
        );

        let data = load_fitness_inner(&pool, los_angeles_time).await.unwrap();
        assert_eq!(
            data.history
                .iter()
                .map(|item| item.workout.workout_date.as_str())
                .collect::<Vec<_>>(),
            ["2026-01-14", "2026-01-13"]
        );
        assert!(data.history.iter().all(
            |item| data.workout_dates.get(&item.workout.id) == Some(&item.workout.workout_date)
        ));
    }

    #[test]
    fn quick_log_date_rejects_a_skipped_local_day() {
        let apia = ep_core::AppTimezone::parse("Pacific/Apia").unwrap();
        assert!(optional_local_date("2011-12-30", apia).is_err());
        let timestamp = optional_local_date("2011-12-31", apia).unwrap().unwrap();
        assert_eq!(apia.fmt_ymd(Some(timestamp)), "2011-12-31");
    }

    #[tokio::test]
    async fn strength_analytics_is_bounded_to_two_hundred_workouts() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "Bounded bench").await;
        let now = 1_768_392_000_i64;
        for index in 0..201_i64 {
            completed_weighted_workout(&pool, exercise_id, now - index * 1_000, &[(50_000, 5)])
                .await;
        }

        let analytics = load_fitness_analytics_inner(
            &pool,
            "body_fat",
            365,
            Some(exercise_id),
            365,
            test_time(now),
        )
        .await
        .unwrap();
        assert_eq!(analytics.strength_trend.len(), 200);
    }

    #[tokio::test]
    async fn strength_analytics_uses_workout_tracking_mode_snapshots() {
        let pool = pool().await;
        let now = 1_768_392_000_i64;

        let historical_weighted = exercise(&pool, "Historical weighted").await;
        completed_weighted_workout(&pool, historical_weighted, now - 86_400, &[(80_000, 5)]).await;
        sqlx::query("UPDATE fit_exercise SET tracking_mode = 'reps' WHERE id = ?1")
            .bind(historical_weighted)
            .execute(&pool)
            .await
            .unwrap();
        let preserved = load_fitness_analytics_inner(
            &pool,
            "weight",
            30,
            Some(historical_weighted),
            90,
            test_time(now),
        )
        .await
        .unwrap();
        assert_eq!(preserved.strength_trend.len(), 1);

        let historical_reps = exercise(&pool, "Historical reps").await;
        sqlx::query("UPDATE fit_exercise SET tracking_mode = 'reps' WHERE id = ?1")
            .bind(historical_reps)
            .execute(&pool)
            .await
            .unwrap();
        completed_weighted_workout(&pool, historical_reps, now - 86_400, &[(70_000, 5)]).await;
        sqlx::query("UPDATE fit_exercise SET tracking_mode = 'weighted' WHERE id = ?1")
            .bind(historical_reps)
            .execute(&pool)
            .await
            .unwrap();
        let excluded = load_fitness_analytics_inner(
            &pool,
            "weight",
            30,
            Some(historical_reps),
            90,
            test_time(now),
        )
        .await
        .unwrap();
        assert!(excluded.strength_trend.is_empty());

        let data = load_fitness_inner(&pool, test_time(now)).await.unwrap();
        assert!(data
            .strength_exercises
            .iter()
            .any(|exercise| exercise.id == historical_weighted));
    }

    #[tokio::test]
    async fn analytics_rejects_unbounded_ranges_and_non_weighted_exercises() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "Duration bike").await;
        sqlx::query("UPDATE fit_exercise SET tracking_mode = 'duration' WHERE id = ?1")
            .bind(exercise_id)
            .execute(&pool)
            .await
            .unwrap();
        let now = 1_768_392_000_i64;

        assert!(
            load_fitness_analytics_inner(&pool, "weight", 31, None, 90, test_time(now))
                .await
                .is_err()
        );
        assert!(
            load_fitness_analytics_inner(&pool, "weight", 30, None, 30, test_time(now))
                .await
                .is_err()
        );
        assert!(load_fitness_analytics_inner(
            &pool,
            "weight",
            30,
            Some(exercise_id),
            90,
            test_time(now),
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn only_one_active_workout_and_revision_is_optimistic() {
        let pool = pool().await;
        let now = 1_700_000_000;
        let first = start_workout_inner(&pool, None, None, test_time(now))
            .await
            .unwrap();
        assert!(start_workout_inner(&pool, None, None, test_time(now + 1))
            .await
            .is_err());

        let paused = pause_workout_inner(&pool, first.workout.id, first.workout.revision, now + 10)
            .await
            .unwrap();
        assert_eq!(paused.workout.status, "paused");
        assert!(
            resume_workout_inner(&pool, first.workout.id, first.workout.revision, now + 20,)
                .await
                .is_err()
        );
        let resumed =
            resume_workout_inner(&pool, paused.workout.id, paused.workout.revision, now + 20)
                .await
                .unwrap();
        assert_eq!(resumed.workout.status, "in_progress");
        assert_eq!(resumed.workout.paused_seconds, 10);
    }

    #[tokio::test]
    async fn plan_is_deep_copied_into_workout() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "Squat").await;
        let plan_id = create_plan_inner(
            &pool,
            PlanInput {
                name: "Lower".into(),
                notes: None,
                exercises: vec![PlanExerciseInput {
                    exercise_id,
                    notes: None,
                    sets: vec![PlanSetInput {
                        target_reps: Some(5),
                        target_weight_g: Some(100_000),
                        target_duration_s: None,
                        target_distance_m: None,
                        target_rpe_x10: Some(80),
                        set_type: "working".into(),
                        rest_seconds: 180,
                    }],
                }],
            },
        )
        .await
        .unwrap();
        let workout = start_workout_inner(&pool, Some(plan_id), None, test_time(1_700_000_000))
            .await
            .unwrap();
        sqlx::query("UPDATE fit_exercise SET name = 'Renamed' WHERE id = ?1")
            .bind(exercise_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            workout.exercises[0].exercise.exercise_name_snapshot,
            "Squat"
        );
        assert_eq!(workout.exercises[0].sets[0].target_reps, Some(5));
    }

    #[tokio::test]
    async fn repeated_exercise_instances_each_receive_one_media_list() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "Cable row").await;
        add_media_metadata_batch_inner(
            &pool,
            exercise_id,
            vec![ExerciseMediaInput {
                object_key: "row-guide.gif".into(),
                title: Some("Form guide".into()),
                media_type: "gif".into(),
                byte_size: 42,
                sha256: "a".repeat(64),
            }],
        )
        .await
        .unwrap();
        let repeated = PlanExerciseInput {
            exercise_id,
            notes: None,
            sets: vec![PlanSetInput {
                target_reps: Some(8),
                target_weight_g: Some(20_000),
                target_duration_s: None,
                target_distance_m: None,
                target_rpe_x10: None,
                set_type: "working".into(),
                rest_seconds: 90,
            }],
        };
        let plan_id = create_plan_inner(
            &pool,
            PlanInput {
                name: "Repeated row".into(),
                notes: None,
                exercises: vec![repeated.clone(), repeated],
            },
        )
        .await
        .unwrap();

        let workout = start_workout_inner(&pool, Some(plan_id), None, test_time(1_700_000_000))
            .await
            .unwrap();

        assert_eq!(workout.exercises.len(), 2);
        assert_eq!(workout.exercises[0].media.len(), 1);
        assert_eq!(workout.exercises[1].media.len(), 1);
        assert_eq!(
            workout.exercises[0].media[0].id,
            workout.exercises[1].media[0].id
        );
    }

    #[tokio::test]
    async fn finish_requires_completed_set_and_reports_first_personal_records() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "Deadlift").await;
        let now = 1_700_000_000;
        let started = start_workout_inner(&pool, None, None, test_time(now))
            .await
            .unwrap();
        let with_exercise = add_workout_exercise_inner(
            &pool,
            started.workout.id,
            started.workout.revision,
            exercise_id,
            now + 1,
        )
        .await
        .unwrap();
        let with_set = add_workout_set_inner(
            &pool,
            with_exercise.workout.id,
            with_exercise.workout.revision,
            with_exercise.exercises[0].exercise.id,
            PlanSetInput {
                target_reps: Some(5),
                target_weight_g: Some(100_000),
                target_duration_s: None,
                target_distance_m: None,
                target_rpe_x10: None,
                set_type: "working".into(),
                rest_seconds: 120,
            },
            now + 2,
        )
        .await
        .unwrap();
        assert!(finish_workout_inner(
            &pool,
            with_set.workout.id,
            with_set.workout.revision,
            test_time(now + 3),
        )
        .await
        .is_err());
        let saved = save_workout_set_inner(
            &pool,
            with_set.workout.id,
            with_set.exercises[0].sets[0].id,
            with_set.workout.revision,
            SetResultInput {
                actual_reps: Some(5),
                actual_weight_g: Some(100_000),
                actual_duration_s: None,
                actual_distance_m: None,
                actual_rpe_x10: Some(80),
                status: "completed".into(),
            },
            now + 4,
        )
        .await
        .unwrap();
        let finished = finish_workout_inner(
            &pool,
            saved.workout.id,
            saved.workout.revision,
            test_time(now + 5),
        )
        .await
        .unwrap();
        assert_eq!(finished.workout.workout.status, "completed");
        assert_eq!(finished.new_records.len(), 2);
        let records = load_personal_records(&pool).await.unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(
            records
                .iter()
                .find(|record| record.kind == "estimated_1rm")
                .unwrap()
                .value_g,
            116_667
        );
    }

    #[tokio::test]
    async fn media_limit_is_enforced_in_database() {
        let pool = pool().await;
        let exercise_id = exercise(&pool, "Bench").await;
        let oversized = (0..=MAX_EXERCISE_MEDIA)
            .map(|position| ExerciseMediaInput {
                object_key: format!("oversized-{position}.gif"),
                title: None,
                media_type: "gif".into(),
                byte_size: 42,
                sha256: "0".repeat(64),
            })
            .collect();
        assert!(
            add_media_metadata_batch_inner(&pool, exercise_id, oversized)
                .await
                .is_err()
        );
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM fit_exercise_media WHERE exercise_id = ?1")
                .bind(exercise_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "an oversized batch must be all-or-nothing");

        for position in 0..MAX_EXERCISE_MEDIA {
            add_media_metadata_batch_inner(
                &pool,
                exercise_id,
                vec![ExerciseMediaInput {
                    object_key: format!("object-{position}.gif"),
                    title: None,
                    media_type: "gif".into(),
                    byte_size: 42,
                    sha256: "a".repeat(64),
                }],
            )
            .await
            .unwrap();
        }
        assert!(add_media_metadata_batch_inner(
            &pool,
            exercise_id,
            vec![ExerciseMediaInput {
                object_key: "overflow.gif".into(),
                title: None,
                media_type: "gif".into(),
                byte_size: 42,
                sha256: "b".repeat(64),
            }],
        )
        .await
        .is_err());

        let mut ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM fit_exercise_media WHERE exercise_id = ?1 ORDER BY sort_order",
        )
        .bind(exercise_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        ids.reverse();
        reorder_media_inner(&pool, exercise_id, &ids).await.unwrap();
        let reordered: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM fit_exercise_media WHERE exercise_id = ?1 ORDER BY sort_order",
        )
        .bind(exercise_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(reordered, ids);

        delete_media_metadata_inner(&pool, exercise_id, ids[0])
            .await
            .unwrap();
        let positions: Vec<i64> = sqlx::query_scalar(
            "SELECT sort_order FROM fit_exercise_media WHERE exercise_id = ?1 ORDER BY sort_order",
        )
        .bind(exercise_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(positions, (0..11).collect::<Vec<_>>());
    }
}
