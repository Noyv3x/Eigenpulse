#![allow(
    clippy::result_large_err,
    reason = "Axum handlers use Response as their rejection type"
)]

use crate::model::*;
use crate::server_fns::*;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{delete, get, patch, post};
use axum::{Extension, Json, Router};
use ep_auth::{require_scope, AuthPat};
use ep_core::{ApiJson, AppState, EntityId};
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

pub fn open_api(_state: AppState) -> Router<AppState> {
    Router::<AppState>::new()
        .route("/exercises", get(list_exercises).post(post_exercise))
        .route(
            "/exercises/:id",
            get(get_exercise)
                .patch(patch_exercise)
                .delete(archive_exercise),
        )
        .route("/exercises/:id/media", get(list_exercise_media))
        .route("/exercises/:id/media/order", patch(order_exercise_media))
        .route("/plans", get(list_plans).post(post_plan))
        .route("/plans/:id", get(get_plan).put(put_plan))
        .route("/sessions/active", get(get_active_session))
        .route("/sessions/start", post(post_start_session))
        .route("/sessions/:id/pause", post(post_pause_session))
        .route("/sessions/:id/resume", post(post_resume_session))
        .route("/sessions/:id/finish", post(post_finish_session))
        .route("/sessions/:id/discard", post(post_discard_session))
        .route("/sessions/:id/exercises", post(post_session_exercise))
        .route("/sessions/:id/sets", post(post_session_set))
        .route("/sessions/:id/sets/:set_id", patch(patch_session_set))
        .route("/workouts", get(list_workouts))
        .route("/workouts/:id", get(get_workout).delete(delete_workout))
        .route("/workouts/:id/sets/:set_id", patch(patch_historical_set))
        .route("/quick-log", post(post_quick_log))
        .route(
            "/measurements",
            get(list_measurements).post(post_measurement),
        )
        .route("/measurements/:id", delete(delete_measurement))
        .route("/summary", get(get_summary))
        .route("/analytics/strength", get(get_strength_analytics))
        .route("/analytics/body", get(get_body_analytics))
}

#[derive(Debug, Deserialize)]
struct IncludeArchived {
    #[serde(default)]
    include_archived: bool,
}

async fn list_exercises(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Query(query): Query<IncludeArchived>,
) -> Result<Json<Vec<ExerciseDetail>>, Response> {
    read(&pat)?;
    load_exercises(&state.db, query.include_archived)
        .await
        .map(Json)
        .map_err(db_error)
}

async fn get_exercise(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<ExerciseDetail>, Response> {
    read(&pat)?;
    let exercise = load_exercises(&state.db, true)
        .await
        .map_err(db_error)?
        .into_iter()
        .find(|item| item.exercise.id == id)
        .ok_or_else(not_found)?;
    Ok(Json(exercise))
}

async fn post_exercise(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<ExerciseInput>,
) -> Result<(StatusCode, Json<EntityId>), Response> {
    write(&pat)?;
    let id = create_exercise_inner(&state.db, input)
        .await
        .map_err(server_error)?;
    Ok((StatusCode::CREATED, Json(EntityId::new(id))))
}

async fn patch_exercise(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<ExerciseInput>,
) -> Result<Json<Exercise>, Response> {
    write(&pat)?;
    update_exercise_inner(&state.db, id, input)
        .await
        .map(Json)
        .map_err(server_error)
}

async fn archive_exercise(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<EntityId>, Response> {
    write(&pat)?;
    archive_exercise_inner(&state.db, id, true)
        .await
        .map_err(server_error)?;
    Ok(Json(EntityId::new(id)))
}

async fn list_exercise_media(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<ExerciseMedia>>, Response> {
    read(&pat)?;
    let item = load_exercises(&state.db, true)
        .await
        .map_err(db_error)?
        .into_iter()
        .find(|item| item.exercise.id == id)
        .ok_or_else(not_found)?;
    Ok(Json(item.media))
}

#[derive(Debug, Deserialize)]
struct MediaOrder {
    ids: Vec<i64>,
}

async fn order_exercise_media(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<MediaOrder>,
) -> Result<StatusCode, Response> {
    write(&pat)?;
    reorder_media_inner(&state.db, id, &input.ids)
        .await
        .map_err(server_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_plans(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<PlanDetail>>, Response> {
    read(&pat)?;
    load_plans(&state.db).await.map(Json).map_err(db_error)
}

async fn get_plan(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<PlanDetail>, Response> {
    read(&pat)?;
    load_plans(&state.db)
        .await
        .map_err(db_error)?
        .into_iter()
        .find(|item| item.plan.id == id)
        .map(Json)
        .ok_or_else(not_found)
}

async fn post_plan(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<PlanInput>,
) -> Result<(StatusCode, Json<EntityId>), Response> {
    write(&pat)?;
    let id = create_plan_inner(&state.db, input)
        .await
        .map_err(server_error)?;
    Ok((StatusCode::CREATED, Json(EntityId::new(id))))
}

async fn put_plan(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<PlanInput>,
) -> Result<Json<EntityId>, Response> {
    write(&pat)?;
    replace_plan_inner(&state.db, id, input)
        .await
        .map_err(server_error)?;
    Ok(Json(EntityId::new(id)))
}

async fn get_active_session(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Option<WorkoutDetail>>, Response> {
    read(&pat)?;
    let active_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM fit_workout WHERE status IN ('in_progress', 'paused') LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(db_error)?
    .flatten();
    let workout = match active_id {
        Some(id) => load_workout_by_id(&state.db, id).await.map_err(db_error)?,
        None => None,
    };
    Ok(Json(workout))
}

#[derive(Debug, Deserialize)]
struct StartSessionInput {
    plan_id: Option<i64>,
    notes: Option<String>,
}

async fn post_start_session(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<StartSessionInput>,
) -> Result<(StatusCode, Json<WorkoutDetail>), Response> {
    write(&pat)?;
    let time = FitnessTime::capture(&state);
    let workout = start_workout_inner(&state.db, input.plan_id, input.notes, time)
        .await
        .map_err(server_error)?;
    Ok((StatusCode::CREATED, Json(workout)))
}

#[derive(Debug, Deserialize)]
struct RevisionInput {
    expected_revision: i64,
}

async fn post_pause_session(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<RevisionInput>,
) -> Result<Json<WorkoutDetail>, Response> {
    write(&pat)?;
    let time = FitnessTime::capture(&state);
    pause_workout_inner(&state.db, id, input.expected_revision, time.now)
        .await
        .map(Json)
        .map_err(server_error)
}

async fn post_resume_session(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<RevisionInput>,
) -> Result<Json<WorkoutDetail>, Response> {
    write(&pat)?;
    let time = FitnessTime::capture(&state);
    resume_workout_inner(&state.db, id, input.expected_revision, time.now)
        .await
        .map(Json)
        .map_err(server_error)
}

async fn post_finish_session(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<RevisionInput>,
) -> Result<Json<FinishWorkoutResult>, Response> {
    write(&pat)?;
    let time = FitnessTime::capture(&state);
    let result = finish_workout_inner(&state.db, id, input.expected_revision, time)
        .await
        .map_err(server_error)?;
    notify_personal_records(&state.notify, &result.new_records).await;
    Ok(Json(result))
}

async fn post_discard_session(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<RevisionInput>,
) -> Result<Json<EntityId>, Response> {
    write(&pat)?;
    discard_workout_inner(&state.db, id, input.expected_revision)
        .await
        .map_err(server_error)?;
    Ok(Json(EntityId::new(id)))
}

#[derive(Debug, Deserialize)]
struct AddSessionExerciseInput {
    expected_revision: i64,
    exercise_id: i64,
}

async fn post_session_exercise(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<AddSessionExerciseInput>,
) -> Result<Json<WorkoutDetail>, Response> {
    write(&pat)?;
    let time = FitnessTime::capture(&state);
    add_workout_exercise_inner(
        &state.db,
        id,
        input.expected_revision,
        input.exercise_id,
        time.now,
    )
    .await
    .map(Json)
    .map_err(server_error)
}

#[derive(Debug, Deserialize)]
struct AddSessionSetInput {
    expected_revision: i64,
    workout_exercise_id: i64,
    set: PlanSetInput,
}

async fn post_session_set(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(input): ApiJson<AddSessionSetInput>,
) -> Result<Json<WorkoutDetail>, Response> {
    write(&pat)?;
    let time = FitnessTime::capture(&state);
    add_workout_set_inner(
        &state.db,
        id,
        input.expected_revision,
        input.workout_exercise_id,
        input.set,
        time.now,
    )
    .await
    .map(Json)
    .map_err(server_error)
}

#[derive(Debug, Deserialize)]
struct SaveSessionSetInput {
    expected_revision: i64,
    result: SetResultInput,
}

async fn patch_session_set(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path((id, set_id)): Path<(i64, i64)>,
    ApiJson(input): ApiJson<SaveSessionSetInput>,
) -> Result<Json<WorkoutDetail>, Response> {
    write(&pat)?;
    let time = FitnessTime::capture(&state);
    save_workout_set_inner(
        &state.db,
        id,
        set_id,
        input.expected_revision,
        input.result,
        time.now,
    )
    .await
    .map(Json)
    .map_err(server_error)
}

#[derive(Debug, Deserialize)]
struct ListLimit {
    limit: Option<i64>,
}

async fn list_workouts(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Query(input): Query<ListLimit>,
) -> Result<Json<Vec<Workout>>, Response> {
    read(&pat)?;
    let limit = input.limit.unwrap_or(50).clamp(1, 200);
    sqlx::query_as(
        "SELECT id, plan_id, plan_name_snapshot, status, workout_date, started_at, ended_at,
                paused_at, paused_seconds, revision, notes
           FROM fit_workout WHERE status = 'completed'
          ORDER BY ended_at DESC, id DESC LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map(Json)
    .map_err(db_error)
}

async fn get_workout(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<WorkoutDetail>, Response> {
    read(&pat)?;
    load_workout_by_id(&state.db, id)
        .await
        .map_err(db_error)?
        .map(Json)
        .ok_or_else(not_found)
}

async fn delete_workout(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<EntityId>, Response> {
    write(&pat)?;
    delete_completed_workout_inner(&state.db, id)
        .await
        .map_err(server_error)?;
    Ok(Json(EntityId::new(id)))
}

async fn patch_historical_set(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path((id, set_id)): Path<(i64, i64)>,
    ApiJson(input): ApiJson<SetResultInput>,
) -> Result<Json<WorkoutDetail>, Response> {
    write(&pat)?;
    let time = FitnessTime::capture(&state);
    revise_completed_set_inner(&state.db, id, set_id, input, time.now)
        .await
        .map(Json)
        .map_err(server_error)
}

async fn post_quick_log(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<QuickLogInput>,
) -> Result<(StatusCode, Json<FinishWorkoutResult>), Response> {
    write(&pat)?;
    let time = FitnessTime::capture(&state);
    let workout = quick_log_inner(&state.db, input, time)
        .await
        .map_err(server_error)?;
    notify_personal_records(&state.notify, &workout.new_records).await;
    Ok((StatusCode::CREATED, Json(workout)))
}

async fn list_measurements(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Query(input): Query<ListLimit>,
) -> Result<Json<Vec<BodyMeasurement>>, Response> {
    read(&pat)?;
    let limit = input.limit.unwrap_or(180).clamp(1, 500);
    sqlx::query_as(
        "SELECT id, measured_at, weight_g, body_fat_bp, waist_mm, notes
           FROM fit_body_measurement ORDER BY measured_at DESC, id DESC LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map(Json)
    .map_err(db_error)
}

#[derive(Debug, Deserialize)]
struct MeasurementInput {
    measured_at: Option<i64>,
    weight_g: Option<i64>,
    body_fat_bp: Option<i64>,
    waist_mm: Option<i64>,
    notes: Option<String>,
}

async fn post_measurement(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<MeasurementInput>,
) -> Result<(StatusCode, Json<EntityId>), Response> {
    write(&pat)?;
    let time = FitnessTime::capture(&state);
    let id = add_body_measurement_inner(
        &state.db,
        input.measured_at,
        time.now,
        input.weight_g,
        input.body_fat_bp,
        input.waist_mm,
        input.notes,
    )
    .await
    .map_err(server_error)?;
    Ok((StatusCode::CREATED, Json(EntityId::new(id))))
}

async fn delete_measurement(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<EntityId>, Response> {
    write(&pat)?;
    delete_body_measurement_inner(&state.db, id)
        .await
        .map_err(server_error)?;
    Ok(Json(EntityId::new(id)))
}

async fn get_summary(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<FitnessHomeSummary>, Response> {
    read(&pat)?;
    let time = FitnessTime::capture(&state);
    home_summary_inner(&state.db, time)
        .await
        .map(Json)
        .map_err(server_error)
}

#[derive(Debug, Deserialize)]
struct StrengthAnalyticsQuery {
    #[serde(default = "default_strength_days")]
    days: u16,
}

const fn default_strength_days() -> u16 {
    365
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct StrengthPointRow {
    exercise_id: i64,
    exercise_name: String,
    workout_date: String,
    ended_at: i64,
    weight_g: i64,
    reps: i64,
}

#[derive(Debug, Serialize)]
struct StrengthPoint {
    exercise_id: i64,
    exercise_name: String,
    workout_date: String,
    ended_at: i64,
    weight_g: i64,
    reps: i64,
    volume_g: i64,
    estimated_1rm_g: Option<i64>,
}

async fn get_strength_analytics(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Query(query): Query<StrengthAnalyticsQuery>,
) -> Result<Json<Vec<StrengthPoint>>, Response> {
    read(&pat)?;
    if !matches!(query.days, 90 | 180 | 365) {
        return Err(ep_core::api_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_fitness_range",
            "strength range must be 90, 180, or 365 days",
        ));
    }
    let time = FitnessTime::capture(&state);
    let (start_date, end_date) = time
        .trailing_workout_dates(query.days)
        .map_err(server_error)?;
    let rows = sqlx::query_as::<_, StrengthPointRow>(
        "SELECT e.exercise_id, e.exercise_name_snapshot AS exercise_name,
                w.workout_date, w.ended_at,
                s.actual_weight_g AS weight_g, s.actual_reps AS reps
           FROM fit_workout_set s
           JOIN fit_workout_exercise e ON e.id = s.workout_exercise_id
           JOIN fit_workout w ON w.id = e.workout_id
          WHERE w.status = 'completed' AND s.status = 'completed'
            AND s.actual_weight_g > 0 AND s.actual_reps > 0
            AND e.exercise_id IS NOT NULL
            AND w.workout_date >= ?1 AND w.workout_date < ?2
            AND w.ended_at < ?3
          ORDER BY w.ended_at, s.id",
    )
    .bind(start_date)
    .bind(end_date)
    .bind(time.end_exclusive().map_err(server_error)?)
    .fetch_all(&state.db)
    .await
    .map_err(db_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| StrengthPoint {
                exercise_id: row.exercise_id,
                exercise_name: row.exercise_name,
                workout_date: row.workout_date,
                ended_at: row.ended_at,
                weight_g: row.weight_g,
                reps: row.reps,
                volume_g: row.weight_g.saturating_mul(row.reps),
                estimated_1rm_g: epley_1rm_g(row.weight_g, row.reps),
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize)]
struct BodyAnalyticsQuery {
    #[serde(default = "default_body_days")]
    days: u16,
}

const fn default_body_days() -> u16 {
    365
}

async fn get_body_analytics(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Query(query): Query<BodyAnalyticsQuery>,
) -> Result<Json<Vec<BodyMeasurement>>, Response> {
    read(&pat)?;
    if !matches!(query.days, 30 | 90 | 365) {
        return Err(ep_core::api_error_response(
            StatusCode::BAD_REQUEST,
            "invalid_fitness_range",
            "body range must be 30, 90, or 365 days",
        ));
    }
    let time = FitnessTime::capture(&state);
    let (start, end) = time.trailing_days(query.days).map_err(server_error)?;
    sqlx::query_as(
        "SELECT id, measured_at, weight_g, body_fat_bp, waist_mm, notes
           FROM fit_body_measurement
          WHERE measured_at >= ?1 AND measured_at < ?2
          ORDER BY measured_at, id",
    )
    .bind(start)
    .bind(end)
    .fetch_all(&state.db)
    .await
    .map(Json)
    .map_err(db_error)
}

fn read(pat: &AuthPat) -> Result<(), Response> {
    require_scope(pat, crate::SCOPE_READ)
}

fn write(pat: &AuthPat) -> Result<(), Response> {
    require_scope(pat, crate::SCOPE_WRITE)
}

fn not_found() -> Response {
    ep_core::api_error_response(StatusCode::NOT_FOUND, "not_found", "resource not found")
}

fn server_error(error: ServerFnError) -> Response {
    let message = error.to_string();
    let status = if message.contains("another tab") || message.contains("already exists") {
        StatusCode::CONFLICT
    } else if message.contains("not found") {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    };
    tracing::warn!(error = %message, "fitness API request rejected");
    ep_core::api_error_response(status, "invalid_fitness_request", message)
}

fn db_error(error: sqlx::Error) -> Response {
    tracing::error!(error = %error, "fitness API database error");
    ep_core::api_error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "internal server error",
    )
}
