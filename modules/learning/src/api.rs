use crate::model::{Book, Course, Note};
use crate::server_fns::{
    add_book_inner, add_course_inner, add_note_inner, delete_book_inner, delete_course_inner,
    delete_note_inner, normalize_book_input, normalize_course_input, normalize_doc_id,
    normalize_note_input, update_book_inner, update_course_inner, update_note_inner, AddNoteFields,
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
        .route("/note", get(list_notes).post(post_note))
        .route("/note/:doc_id", patch(patch_note).delete(delete_note))
        .route("/book", get(list_books).post(post_book))
        .route("/book/:doc_id", patch(patch_book).delete(delete_book))
        .route("/course", get(list_courses).post(post_course))
        .route("/course/:doc_id", patch(patch_course).delete(delete_course))
}

#[derive(Debug, Deserialize)]
pub struct NoteInput {
    pub title: String,
    pub body: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NoteCreated {
    pub doc_id: String,
}

#[derive(Debug, Serialize)]
pub struct NoteDeleted {
    pub doc_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchNoteInput {
    pub title: Option<String>,
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    pub body: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
pub struct BookInput {
    pub name: String,
    pub author: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchBookInput {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    pub author: Option<Option<String>>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BookCreated {
    pub doc_id: String,
}

#[derive(Debug, Serialize)]
pub struct BookDeleted {
    pub doc_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CourseInput {
    pub name: String,
    pub provider: Option<String>,
    pub progress_pct: Option<f64>,
    pub due_on: Option<String>,
    pub tone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchCourseInput {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    pub provider: Option<Option<String>>,
    pub progress_pct: Option<f64>,
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    pub due_on: Option<Option<String>>,
    #[serde(default, deserialize_with = "ep_core::deserialize_nullable_patch")]
    pub tone: Option<Option<String>>,
}

#[derive(Debug, Serialize)]
pub struct CourseCreated {
    pub doc_id: String,
}

#[derive(Debug, Serialize)]
pub struct CourseDeleted {
    pub doc_id: String,
}

#[derive(Debug, Serialize)]
pub struct DocUpdated {
    pub doc_id: String,
}

async fn post_note(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<NoteInput>,
) -> Result<Json<NoteCreated>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_WRITE)?;
    let note = add_note_inner(
        &state.db,
        AddNoteFields {
            title: input.title,
            body: input.body.unwrap_or_default(),
        },
    )
    .await
    .map_err(server_err_to_response)?;

    Ok(Json(NoteCreated {
        doc_id: note.doc_id,
    }))
}

async fn post_book(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<BookInput>,
) -> Result<Json<BookCreated>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_WRITE)?;
    let author = input.author.unwrap_or_default();
    let status = input.status.unwrap_or_default();
    let book = add_book_inner(
        &state.db,
        normalize_book_input(&input.name, &author, &status).map_err(server_err_to_response)?,
    )
    .await
    .map_err(server_err_to_response)?;

    Ok(Json(BookCreated {
        doc_id: book.doc_id,
    }))
}

async fn post_course(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<CourseInput>,
) -> Result<Json<CourseCreated>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_WRITE)?;
    let provider = input.provider.unwrap_or_default();
    let due_on = input.due_on.unwrap_or_default();
    let tone = input.tone.unwrap_or_default();
    let course = add_course_inner(
        &state.db,
        normalize_course_input(
            &input.name,
            &provider,
            input.progress_pct.unwrap_or_default(),
            &due_on,
            &tone,
        )
        .map_err(server_err_to_response)?,
    )
    .await
    .map_err(server_err_to_response)?;

    Ok(Json(CourseCreated {
        doc_id: course.doc_id,
    }))
}

async fn list_notes(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<Note>>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_READ)?;
    let notes = sqlx::query_as::<_, Note>(
        "SELECT doc_id, title, body, updated_at
           FROM lrn_note ORDER BY updated_at DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_err_response)?;
    Ok(Json(notes))
}

async fn list_books(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<Book>>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_READ)?;
    let books = sqlx::query_as::<_, Book>(
        "SELECT doc_id, name, author, status, progress
           FROM lrn_book ORDER BY rowid DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_err_response)?;
    Ok(Json(books))
}

async fn list_courses(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<Vec<Course>>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_READ)?;
    let courses = sqlx::query_as::<_, Course>(
        "SELECT doc_id, name, provider, progress, due_on, tone
           FROM lrn_course WHERE archived = 0 ORDER BY rowid DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_err_response)?;
    Ok(Json(courses))
}

async fn patch_note(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
    ApiJson(input): ApiJson<PatchNoteInput>,
) -> Result<Json<DocUpdated>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_WRITE)?;
    let doc_id = normalize_doc_id(&doc_id).map_err(server_err_to_response)?;
    let cur: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT title, body FROM lrn_note WHERE doc_id = ?1")
            .bind(&doc_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err_response)?;
    let Some((cur_title, cur_body)) = cur else {
        return Err(server_err_to_response(ep_i18n::err_with(
            "learning.err.note_not_found",
            &doc_id,
        )));
    };
    let title = input.title.unwrap_or(cur_title);
    let body = ep_core::apply_nullable_patch_or_default(input.body, cur_body);
    let normalized = normalize_note_input(&title, &body).map_err(server_err_to_response)?;
    update_note_inner(&state.db, &doc_id, normalized)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(DocUpdated { doc_id }))
}

async fn patch_book(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
    ApiJson(input): ApiJson<PatchBookInput>,
) -> Result<Json<DocUpdated>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_WRITE)?;
    let doc_id = normalize_doc_id(&doc_id).map_err(server_err_to_response)?;
    let cur: Option<(String, Option<String>, String)> =
        sqlx::query_as("SELECT name, author, status FROM lrn_book WHERE doc_id = ?1")
            .bind(&doc_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err_response)?;
    let Some((cur_name, cur_author, cur_status)) = cur else {
        return Err(server_err_to_response(ep_i18n::err_with(
            "learning.err.book_not_found",
            &doc_id,
        )));
    };
    let name = input.name.unwrap_or(cur_name);
    let author = ep_core::apply_nullable_patch_or_default(input.author, cur_author);
    let status = input.status.unwrap_or(cur_status);
    let normalized =
        normalize_book_input(&name, &author, &status).map_err(server_err_to_response)?;
    update_book_inner(&state.db, &doc_id, normalized)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(DocUpdated { doc_id }))
}

async fn patch_course(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
    ApiJson(input): ApiJson<PatchCourseInput>,
) -> Result<Json<DocUpdated>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_WRITE)?;
    let doc_id = normalize_doc_id(&doc_id).map_err(server_err_to_response)?;
    type CurrentCourse = (String, Option<String>, f64, Option<String>, Option<String>);
    let cur: Option<CurrentCourse> = sqlx::query_as(
        "SELECT name, provider, progress, due_on, tone
           FROM lrn_course WHERE doc_id = ?1 AND archived = 0",
    )
    .bind(&doc_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err_response)?;
    let Some((cur_name, cur_provider, cur_progress, cur_due_on, cur_tone)) = cur else {
        return Err(server_err_to_response(ep_i18n::err_with(
            "learning.err.course_not_found",
            &doc_id,
        )));
    };
    let name = input.name.unwrap_or(cur_name);
    let provider = ep_core::apply_nullable_patch_or_default(input.provider, cur_provider);
    let progress_pct = input.progress_pct.unwrap_or(cur_progress * 100.0);
    let due_on = ep_core::apply_nullable_patch_or_default(input.due_on, cur_due_on);
    let tone = ep_core::apply_nullable_patch_or_default(input.tone, cur_tone);
    let normalized = normalize_course_input(&name, &provider, progress_pct, &due_on, &tone)
        .map_err(server_err_to_response)?;
    update_course_inner(&state.db, &doc_id, normalized)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(DocUpdated { doc_id }))
}

async fn delete_note(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
) -> Result<Json<NoteDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_WRITE)?;
    let doc_id = normalize_doc_id(&doc_id).map_err(server_err_to_response)?;
    delete_note_inner(&state.db, &doc_id)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(NoteDeleted { doc_id }))
}

async fn delete_book(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
) -> Result<Json<BookDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_WRITE)?;
    let doc_id = normalize_doc_id(&doc_id).map_err(server_err_to_response)?;
    delete_book_inner(&state.db, &doc_id)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(BookDeleted { doc_id }))
}

async fn delete_course(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(doc_id): Path<String>,
) -> Result<Json<CourseDeleted>, Response> {
    require_scope(&pat, ep_core::SCOPE_LRN_WRITE)?;
    let doc_id = normalize_doc_id(&doc_id).map_err(server_err_to_response)?;
    delete_course_inner(&state.db, &doc_id)
        .await
        .map_err(server_err_to_response)?;
    Ok(Json(CourseDeleted { doc_id }))
}

// Error mapping delegates to the shared implementation in `ep_i18n::api_error`.

fn server_err_to_response(e: ServerFnError) -> Response {
    ep_i18n::i18n_error_response(e, "learning open api")
}

fn db_err_response<E: std::fmt::Display>(e: E) -> Response {
    ep_i18n::db_error_response(e, "learning open api")
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
            "CREATE TABLE lrn_course (
                doc_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT,
                progress REAL NOT NULL DEFAULT 0,
                due_on TEXT,
                tone TEXT,
                archived INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&db)
        .await
        .expect("lrn_course");
        sqlx::query(
            "CREATE TABLE lrn_book (
                doc_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                author TEXT,
                status TEXT NOT NULL DEFAULT 'reading',
                progress REAL NOT NULL DEFAULT 0
            )",
        )
        .execute(&db)
        .await
        .expect("lrn_book");
        sqlx::query(
            "CREATE TABLE lrn_note (
                doc_id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                body TEXT,
                tags TEXT,
                course_doc TEXT REFERENCES lrn_course(doc_id),
                book_doc TEXT REFERENCES lrn_book(doc_id),
                updated_at INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&db)
        .await
        .expect("lrn_note");
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
    async fn post_note_requires_write_scope() {
        let state = test_state().await;
        let err = post_note(
            State(state),
            Extension(pat(&[ep_core::SCOPE_LRN_READ])),
            ep_core::ApiJson(NoteInput {
                title: "Note".into(),
                body: None,
            }),
        )
        .await
        .expect_err("missing write scope should fail");

        assert_eq!(err.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn post_and_list_note_round_trip() {
        let state = test_state().await;
        let Json(created) = post_note(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            ep_core::ApiJson(NoteInput {
                title: "Cache modes".into(),
                body: Some("write-through vs write-back".into()),
            }),
        )
        .await
        .expect("create note");

        assert!(created.doc_id.starts_with("LRN-N-"));

        let Json(rows) = list_notes(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_READ])),
        )
        .await
        .expect("list notes");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].doc_id, created.doc_id);
        assert_eq!(rows[0].title, "Cache modes");
        assert_eq!(rows[0].body.as_deref(), Some("write-through vs write-back"));

        let Json(updated) = patch_note(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            Path(created.doc_id.clone()),
            ep_core::ApiJson(PatchNoteInput {
                title: Some("Cache modes revised".into()),
                body: None,
            }),
        )
        .await
        .expect("patch note");
        assert_eq!(updated.doc_id, created.doc_id);

        let Json(rows) = list_notes(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_READ])),
        )
        .await
        .expect("list notes");
        assert_eq!(rows[0].title, "Cache modes revised");
        assert_eq!(rows[0].body.as_deref(), Some("write-through vs write-back"));

        let Json(cleared) = patch_note(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            Path(created.doc_id.clone()),
            ep_core::ApiJson(PatchNoteInput {
                title: None,
                body: Some(None),
            }),
        )
        .await
        .expect("clear note body");
        assert_eq!(cleared.doc_id, created.doc_id);

        let Json(rows) = list_notes(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_READ])),
        )
        .await
        .expect("list notes");
        assert_eq!(rows[0].title, "Cache modes revised");
        assert_eq!(rows[0].body, None);

        let Json(deleted) = delete_note(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            Path(created.doc_id.clone()),
        )
        .await
        .expect("delete note");
        assert_eq!(deleted.doc_id, created.doc_id);

        let Json(rows) = list_notes(State(state), Extension(pat(&[ep_core::SCOPE_LRN_READ])))
            .await
            .expect("list notes");
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn book_and_course_round_trips_cover_patch_and_delete() {
        let state = test_state().await;

        let Json(book) = post_book(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            ep_core::ApiJson(BookInput {
                name: "Domain Modeling".into(),
                author: Some("Evans".into()),
                status: Some("reading".into()),
            }),
        )
        .await
        .expect("create book");
        assert!(book.doc_id.starts_with("LRN-B-"));

        let Json(updated_book) = patch_book(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            Path(book.doc_id.clone()),
            ep_core::ApiJson(PatchBookInput {
                name: None,
                author: None,
                status: Some("done".into()),
            }),
        )
        .await
        .expect("patch book");
        assert_eq!(updated_book.doc_id, book.doc_id);

        let Json(books) = list_books(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_READ])),
        )
        .await
        .expect("list books");
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].doc_id, book.doc_id);
        assert_eq!(books[0].status, "done");
        assert_eq!(books[0].progress, 1.0);

        let Json(updated_book) = patch_book(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            Path(book.doc_id.clone()),
            ep_core::ApiJson(PatchBookInput {
                name: None,
                author: Some(None),
                status: None,
            }),
        )
        .await
        .expect("clear book author");
        assert_eq!(updated_book.doc_id, book.doc_id);

        let Json(books) = list_books(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_READ])),
        )
        .await
        .expect("list books");
        assert_eq!(books[0].author, None);
        assert_eq!(books[0].status, "done");

        let Json(course) = post_course(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            ep_core::ApiJson(CourseInput {
                name: "Rust".into(),
                provider: Some("Book".into()),
                progress_pct: Some(25.0),
                due_on: Some("2026-06-30".into()),
                tone: Some("amber".into()),
            }),
        )
        .await
        .expect("create course");
        assert!(course.doc_id.starts_with("LRN-C-"));

        let Json(updated_course) = patch_course(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            Path(course.doc_id.clone()),
            ep_core::ApiJson(PatchCourseInput {
                name: None,
                provider: None,
                progress_pct: Some(80.0),
                due_on: None,
                tone: Some(Some("green".into())),
            }),
        )
        .await
        .expect("patch course");
        assert_eq!(updated_course.doc_id, course.doc_id);

        let Json(courses) = list_courses(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_READ])),
        )
        .await
        .expect("list courses");
        assert_eq!(courses.len(), 1);
        assert_eq!(courses[0].doc_id, course.doc_id);
        assert_eq!(courses[0].progress, 0.8);
        assert_eq!(courses[0].tone.as_deref(), Some("green"));

        let Json(updated_course) = patch_course(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            Path(course.doc_id.clone()),
            ep_core::ApiJson(PatchCourseInput {
                name: None,
                provider: Some(None),
                progress_pct: None,
                due_on: Some(None),
                tone: Some(None),
            }),
        )
        .await
        .expect("clear course nullable fields");
        assert_eq!(updated_course.doc_id, course.doc_id);

        let Json(courses) = list_courses(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_READ])),
        )
        .await
        .expect("list courses");
        assert_eq!(courses[0].provider, None);
        assert_eq!(courses[0].progress, 0.8);
        assert_eq!(courses[0].due_on, None);
        assert_eq!(courses[0].tone, None);

        let Json(deleted_book) = delete_book(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            Path(book.doc_id.clone()),
        )
        .await
        .expect("delete book");
        assert_eq!(deleted_book.doc_id, book.doc_id);

        let Json(deleted_course) = delete_course(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_WRITE])),
            Path(course.doc_id.clone()),
        )
        .await
        .expect("delete course");
        assert_eq!(deleted_course.doc_id, course.doc_id);

        let Json(books) = list_books(
            State(state.clone()),
            Extension(pat(&[ep_core::SCOPE_LRN_READ])),
        )
        .await
        .expect("list books");
        assert!(books.is_empty());
        let Json(courses) = list_courses(State(state), Extension(pat(&[ep_core::SCOPE_LRN_READ])))
            .await
            .expect("list courses");
        assert!(courses.is_empty());
    }
}
