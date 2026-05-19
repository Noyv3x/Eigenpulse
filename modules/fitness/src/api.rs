use crate::model::Workout;
use crate::server_fns::{
    add_workout_inner, delete_workout_inner, normalize_doc_id, normalize_workout_input,
    update_workout_inner, AddWorkoutFields,
};
use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::{get, patch};
use axum::{Extension, Json, Router};
use ep_auth::{require_scope, AuthPat};
use ep_core::{ApiJson, AppState};
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

pub fn open_api(_state: AppState) -> Router<AppState> {
    Router::<AppState>::new()
        .route("/workout", get(list_workouts).post(post_workout))
        .route(
            "/workout/:doc_id",
            patch(patch_workout).delete(delete_workout),
        )
}

#[derive(Debug, Deserialize)]
pub struct WorkoutInput {
    pub occurred_on: Option<String>,
    pub kind: String,
    pub program: Option<String>,
    pub duration_m: i64,
    pub load_text: Option<String>,
    pub strain: Option<String>,
    pub rpe: Option<i64>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkoutCreated {
    pub doc_id: String,
}

#[derive(Debug, Serialize)]
pub struct WorkoutDeleted {
    pub doc_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchWorkoutInput {
    pub occurred_on: Option<String>,
    pub kind: Option<String>,
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    pub program: Option<Option<String>>,
    pub duration_m: Option<i64>,
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    pub load_text: Option<Option<String>>,
    pub strain: Option<String>,
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    pub rpe: Option<Option<i64>>,
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    pub notes: Option<Option<String>>,
}

#[derive(Debug, Serialize)]
pub struct WorkoutUpdated {
    pub doc_id: String,
}

async fn post_workout(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<WorkoutInput>,
) -> Result<Json<WorkoutCreated>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIT_WRITE)?;
    let workout = add_workout_inner(
        &state.db,
        AddWorkoutFields {
            occurred_on: input.occurred_on.unwrap_or_default(),
            kind: input.kind,
            program: input.program.unwrap_or_default(),
            duration_m: input.duration_m,
            load_text: input.load_text.unwrap_or_default(),
            strain: input.strain.unwrap_or_default(),
            rpe: input.rpe.map(|rpe| rpe.to_string()).unwrap_or_default(),
            notes: input.notes.unwrap_or_default(),
        },
    )
    .await
    .map_err(server_err_to_response)?;

    Ok(Json(WorkoutCreated {
        doc_id: workout.doc_id,
    }))
}

async fn list_workouts(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<Workout>>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIT_READ)?;
    let workouts = sqlx::query_as::<_, Workout>(
        "SELECT doc_id, occurred_at, kind, program, duration_m, load_text, strain, rpe, notes
           FROM fit_workout ORDER BY occurred_at DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_err_response)?;
    Ok(Json(workouts))
}

async fn delete_workout(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
) -> Result<Json<WorkoutDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIT_WRITE)?;
    let doc_id = normalize_doc_id(&doc_id).map_err(server_err_to_response)?;
    delete_workout_inner(&state.db, &doc_id)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(WorkoutDeleted { doc_id }))
}

async fn patch_workout(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
    ApiJson(input): ApiJson<PatchWorkoutInput>,
) -> Result<Json<WorkoutUpdated>, Response> {
    require_scope(&pat, ep_core::SCOPE_FIT_WRITE)?;
    let doc_id = normalize_doc_id(&doc_id).map_err(server_err_to_response)?;
    let cur: Option<Workout> = sqlx::query_as(
        "SELECT doc_id, occurred_at, kind, program, duration_m, load_text, strain, rpe, notes
           FROM fit_workout WHERE doc_id = ?1",
    )
    .bind(&doc_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err_response)?;
    let Some(cur) = cur else {
        return Err(server_err_to_response(ep_i18n::err_with(
            "fitness.err.workout_not_found",
            &doc_id,
        )));
    };
    let kind = input.kind.unwrap_or(cur.kind);
    let program = ep_core::apply_nullable_patch_or_default(input.program, cur.program);
    let duration_m = input.duration_m.unwrap_or(cur.duration_m);
    let load_text = ep_core::apply_nullable_patch_or_default(input.load_text, cur.load_text);
    let strain = input.strain.or(cur.strain).unwrap_or_default();
    let rpe = ep_core::apply_nullable_patch(input.rpe, cur.rpe)
        .map(|rpe| rpe.to_string())
        .unwrap_or_default();
    let notes = ep_core::apply_nullable_patch_or_default(input.notes, cur.notes);
    let occurred_on = input.occurred_on.unwrap_or_default();
    let normalized = normalize_workout_input(&AddWorkoutFields {
        occurred_on,
        kind,
        program,
        duration_m,
        load_text,
        strain,
        rpe,
        notes,
    })
    .map_err(server_err_to_response)?;
    update_workout_inner(&state.db, &doc_id, normalized)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(WorkoutUpdated { doc_id }))
}

// Error mapping delegates to the shared implementation in `ep_i18n::api_error`.

fn server_err_to_response(e: ServerFnError) -> Response {
    ep_i18n::i18n_error_response(e, "fitness open api")
}

fn db_err_response<E: std::fmt::Display>(e: E) -> Response {
    ep_i18n::db_error_response(e, "fitness open api")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::StatusCode;
    use std::sync::Arc;

    struct NoopNotifyBus;

    #[async_trait::async_trait]
    impl ep_core::NotifyBusTrait for NoopNotifyBus {
        async fn dispatch(&self, _msg: ep_core::NotifyMessage) -> anyhow::Result<i64> {
            Ok(0)
        }

        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<ep_core::NotifyMessage> {
            let (_tx, rx) = tokio::sync::broadcast::channel(1);
            rx
        }
    }

    async fn test_state() -> AppState {
        let db = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::query(
            "CREATE TABLE seq (
                module TEXT NOT NULL,
                kind TEXT NOT NULL,
                last_value INTEGER NOT NULL,
                PRIMARY KEY (module, kind)
            )",
        )
        .execute(&db)
        .await
        .expect("seq");
        sqlx::query(
            "CREATE TABLE fit_workout (
                doc_id TEXT PRIMARY KEY,
                occurred_at INTEGER NOT NULL,
                kind TEXT NOT NULL,
                program TEXT,
                duration_m INTEGER NOT NULL,
                load_text TEXT,
                strain TEXT,
                rpe INTEGER,
                notes TEXT
            )",
        )
        .execute(&db)
        .await
        .expect("fit_workout");
        sqlx::query(
            "CREATE TABLE activity (
                occurred_at INTEGER NOT NULL,
                module TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                link_doc TEXT,
                summary TEXT,
                status TEXT
            )",
        )
        .execute(&db)
        .await
        .expect("activity");
        sqlx::query(
            "CREATE TABLE module_link (
                source_doc TEXT NOT NULL,
                target_doc TEXT NOT NULL,
                kind TEXT NOT NULL
            )",
        )
        .execute(&db)
        .await
        .expect("module_link");
        sqlx::query(
            "CREATE TABLE notification (
                id INTEGER PRIMARY KEY,
                doc_ref TEXT
            )",
        )
        .execute(&db)
        .await
        .expect("notification");
        AppState {
            db,
            cookie_key: cookie::Key::generate(),
            notify: Arc::new(NoopNotifyBus),
            leptos_options: Default::default(),
        }
    }

    fn pat(scopes: &[&str]) -> AuthPat {
        AuthPat {
            id: 1,
            name: "test".into(),
            scopes: scopes.iter().map(|s| (*s).into()).collect(),
        }
    }

    #[tokio::test]
    async fn post_workout_requires_write_scope() {
        let state = test_state().await;
        let err = post_workout(
            State(state),
            Extension(pat(&[ep_core::SCOPE_FIT_READ])),
            ep_core::ApiJson(WorkoutInput {
                occurred_on: Some("2026-05-08".into()),
                kind: "Run".into(),
                program: None,
                duration_m: 30,
                load_text: None,
                strain: Some("M".into()),
                rpe: None,
                notes: None,
            }),
        )
        .await
        .expect_err("missing write scope should fail");

        assert_eq!(err.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_and_list_workout_round_trip() {
        let state = test_state().await;
        let Json(created) = post_workout(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_FIT_WRITE])),
            ep_core::ApiJson(WorkoutInput {
                occurred_on: Some("2026-05-08".into()),
                kind: "Run".into(),
                program: Some("Base".into()),
                duration_m: 35,
                load_text: Some("5km".into()),
                strain: Some("M".into()),
                rpe: Some(7),
                notes: Some("felt good".into()),
            }),
        )
        .await
        .expect("create workout");

        assert!(created.doc_id.starts_with("FIT-S-"));

        let Json(rows) = list_workouts(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_FIT_READ])),
        )
        .await
        .expect("list workouts");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].doc_id, created.doc_id);
        assert_eq!(rows[0].kind, "Run");
        assert_eq!(rows[0].rpe, Some(7));

        let Json(updated) = patch_workout(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_FIT_WRITE])),
            Path(created.doc_id.clone()),
            ep_core::ApiJson(PatchWorkoutInput {
                occurred_on: None,
                kind: Some("Tempo Run".into()),
                program: None,
                duration_m: Some(42),
                load_text: Some(Some("6km".into())),
                strain: Some("H".into()),
                rpe: Some(Some(8)),
                notes: None,
            }),
        )
        .await
        .expect("patch workout");
        assert_eq!(updated.doc_id, created.doc_id);

        let Json(rows) = list_workouts(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_FIT_READ])),
        )
        .await
        .expect("list workouts");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].doc_id, created.doc_id);
        assert_eq!(rows[0].kind, "Tempo Run");
        assert_eq!(rows[0].duration_m, 42);
        assert_eq!(rows[0].load_text.as_deref(), Some("6km"));
        assert_eq!(rows[0].strain.as_deref(), Some("H"));
        assert_eq!(rows[0].rpe, Some(8));
        assert_eq!(rows[0].notes.as_deref(), Some("felt good"));

        let Json(cleared) = patch_workout(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_FIT_WRITE])),
            Path(created.doc_id.clone()),
            ep_core::ApiJson(PatchWorkoutInput {
                occurred_on: None,
                kind: None,
                program: Some(None),
                duration_m: None,
                load_text: Some(None),
                strain: None,
                rpe: Some(None),
                notes: Some(None),
            }),
        )
        .await
        .expect("clear nullable workout fields");
        assert_eq!(cleared.doc_id, created.doc_id);

        let Json(rows) = list_workouts(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_FIT_READ])),
        )
        .await
        .expect("list workouts");
        assert_eq!(rows[0].program, None);
        assert_eq!(rows[0].load_text, None);
        assert_eq!(rows[0].rpe, None);
        assert_eq!(rows[0].notes, None);

        let Json(deleted) = delete_workout(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_FIT_WRITE])),
            Path(created.doc_id.clone()),
        )
        .await
        .expect("delete workout");
        assert_eq!(deleted.doc_id, created.doc_id);

        let Json(rows) = list_workouts(State(state), Extension(pat(&[ep_core::SCOPE_FIT_READ])))
            .await
            .expect("list workouts");
        assert!(rows.is_empty());
    }
}
