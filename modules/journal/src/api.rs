#![allow(
    clippy::result_large_err,
    reason = "Axum handlers use Response as their shared rejection type"
)]

use crate::model::{JournalEntry, JournalEntryInput, JournalEntryListItem};
use crate::server_fns::{
    create_entry_inner, delete_entry_inner, get_entry_inner, list_entries_inner, patch_entry_inner,
    summary_inner, validate_date, JournalDateContext, JournalEntryPatchInput,
    JOURNAL_API_MAX_PAGE_SIZE, JOURNAL_PAGE_SIZE,
};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::get;
use axum::{Extension, Json, Router};
use ep_auth::{require_scope, AuthPat};
use ep_core::{ApiJson, AppState};
use serde::{Deserialize, Serialize};

pub fn open_api(_state: AppState) -> Router<AppState> {
    Router::<AppState>::new()
        .route("/entries", get(list_entries).post(post_entry))
        .route(
            "/entries/:id",
            get(get_entry).patch(patch_entry).delete(delete_entry),
        )
        .route("/summary", get(get_summary))
}

#[derive(Debug, Default, Deserialize)]
struct EntryQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    include_archived: bool,
    date_from: Option<String>,
    date_to: Option<String>,
    #[serde(default)]
    offset: u32,
    #[serde(default = "default_page_size")]
    limit: u32,
}

fn default_page_size() -> u32 {
    JOURNAL_PAGE_SIZE
}

async fn list_entries(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Query(query): Query<EntryQuery>,
) -> Result<Json<Vec<JournalEntryListItem>>, Response> {
    read(&pat)?;
    if query.q.chars().count() > 200 {
        return Err(bad_request("q must be at most 200 characters"));
    }
    if query.limit == 0 || query.limit > JOURNAL_API_MAX_PAGE_SIZE {
        return Err(bad_request("limit must be between 1 and 100"));
    }
    validate_optional_date(query.date_from.as_deref())?;
    validate_optional_date(query.date_to.as_deref())?;
    if query
        .date_from
        .as_ref()
        .zip(query.date_to.as_ref())
        .is_some_and(|(from, to)| from > to)
    {
        return Err(bad_request("date_from must not be after date_to"));
    }
    list_entries_inner(
        &state.db,
        &query.q,
        query.include_archived,
        query.date_from.as_deref(),
        query.date_to.as_deref(),
        query.offset,
        query.limit,
    )
    .await
    .map(|page| Json(page.entries))
    .map_err(db_error)
}

async fn get_entry(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<Json<JournalEntry>, Response> {
    read(&pat)?;
    positive_id(id)?;
    get_entry_inner(&state.db, id)
        .await
        .map_err(db_error)?
        .map(Json)
        .ok_or_else(not_found)
}

async fn post_entry(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    ApiJson(input): ApiJson<JournalEntryInput>,
) -> Result<(StatusCode, Json<JournalEntry>), Response> {
    write(&pat)?;
    let entry = create_entry_inner(&state.db, input)
        .await
        .map_err(request_error)?;
    Ok((StatusCode::CREATED, Json(entry)))
}

#[derive(Debug, Default, Deserialize)]
struct JournalEntryPatch {
    title: Option<String>,
    body: Option<String>,
    entry_date: Option<String>,
    #[serde(default, deserialize_with = "deserialize_nullable_mood")]
    mood: Option<Option<String>>,
    tags: Option<String>,
    archived: Option<bool>,
}

async fn patch_entry(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
    ApiJson(patch): ApiJson<JournalEntryPatch>,
) -> Result<StatusCode, Response> {
    patch_entry_for_pat(&state.db, &pat, id, patch).await
}

async fn patch_entry_for_pat(
    pool: &sqlx::SqlitePool,
    pat: &AuthPat,
    id: i64,
    patch: JournalEntryPatch,
) -> Result<StatusCode, Response> {
    write(pat)?;
    positive_id(id)?;
    let patch = JournalEntryPatchInput {
        title: patch.title,
        body: patch.body,
        entry_date: patch.entry_date,
        mood: patch.mood,
        tags: patch.tags,
        archived: patch.archived,
    };
    if patch.is_empty() {
        return Err(bad_request("patch must contain at least one field"));
    }
    if !patch_entry_inner(pool, id, patch)
        .await
        .map_err(request_error)?
    {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

fn deserialize_nullable_mood<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

async fn delete_entry(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
    Path(id): Path<i64>,
) -> Result<StatusCode, Response> {
    write(&pat)?;
    positive_id(id)?;
    if !delete_entry_inner(&state.db, id)
        .await
        .map_err(request_error)?
    {
        return Err(not_found());
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
struct JournalApiSummary {
    active_entries: i64,
    archived_entries: i64,
    entries_this_month: i64,
    latest_entry_date: Option<String>,
}

async fn get_summary(
    State(state): State<AppState>,
    Extension(pat): Extension<AuthPat>,
) -> Result<Json<JournalApiSummary>, Response> {
    read(&pat)?;
    let timezone = state.timezone();
    let now = ep_core::unix_now();
    let dates = JournalDateContext::from_snapshot(timezone, now)
        .ok_or_else(|| calendar_error("invalid journal date context"))?;
    let summary = summary_inner(&state.db, &dates.current_month)
        .await
        .map_err(db_error)?;
    Ok(Json(JournalApiSummary {
        active_entries: summary.active,
        archived_entries: summary.archived,
        entries_this_month: summary.this_month,
        latest_entry_date: summary.latest,
    }))
}

fn read(pat: &AuthPat) -> Result<(), Response> {
    require_scope(pat, crate::SCOPE_READ)
}

fn write(pat: &AuthPat) -> Result<(), Response> {
    require_scope(pat, crate::SCOPE_WRITE)
}

fn positive_id(id: i64) -> Result<(), Response> {
    if id > 0 {
        Ok(())
    } else {
        Err(bad_request("entry id must be a positive integer"))
    }
}

fn validate_optional_date(value: Option<&str>) -> Result<(), Response> {
    value
        .map(validate_date)
        .transpose()
        .map(|_| ())
        .map_err(request_error)
}

fn bad_request(message: &str) -> Response {
    ep_core::api_error_response(StatusCode::BAD_REQUEST, "invalid_journal_request", message)
}

fn not_found() -> Response {
    ep_core::api_error_response(
        StatusCode::NOT_FOUND,
        "not_found",
        "journal entry not found",
    )
}

fn request_error(error: leptos::server_fn::ServerFnError) -> Response {
    ep_i18n::i18n_error_response(error, "journal open api")
}

fn db_error(error: sqlx::Error) -> Response {
    ep_i18n::db_error_response(error, "journal open api")
}

fn calendar_error(message: &str) -> Response {
    request_error(ep_core::server_err(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::JournalEntryInput;
    use crate::server_fns::{create_entry_inner, get_entry_inner};
    use axum::body;
    use axum::response::IntoResponse;

    fn write_only_pat() -> AuthPat {
        AuthPat {
            id: 1,
            name: "journal writer".into(),
            scopes: vec![crate::SCOPE_WRITE.into()],
        }
    }

    async fn entry(pool: &sqlx::SqlitePool) -> JournalEntry {
        create_entry_inner(
            pool,
            JournalEntryInput {
                title: "Private title".into(),
                body: "Private body".into(),
                entry_date: "2026-07-12".into(),
                mood: Some("calm".into()),
                tags: "private".into(),
            },
        )
        .await
        .expect("create entry")
    }

    #[tokio::test]
    async fn write_only_patch_returns_no_content_without_exposing_the_entry() {
        let pool = crate::crud_tests::migrated_pool().await;
        let created = entry(&pool).await;
        let pat = write_only_pat();
        assert!(
            read(&pat).is_err(),
            "write-only PAT must not gain read scope"
        );

        let status = patch_entry_for_pat(
            &pool,
            &pat,
            created.id,
            JournalEntryPatch {
                archived: Some(true),
                ..Default::default()
            },
        )
        .await
        .expect("archive patch");
        assert_eq!(status, StatusCode::NO_CONTENT);

        let response = status.into_response();
        let response_body = body::to_bytes(response.into_body(), 1024)
            .await
            .expect("response body");
        assert!(response_body.is_empty());

        let persisted = get_entry_inner(&pool, created.id)
            .await
            .expect("read persisted entry")
            .expect("entry exists");
        assert_eq!(persisted.title, created.title);
        assert_eq!(persisted.body, created.body);
        assert!(persisted.archived_at.is_some());
    }

    #[tokio::test]
    async fn patch_rejects_empty_input_and_preserves_not_found_status() {
        let pool = crate::crud_tests::migrated_pool().await;
        let created = entry(&pool).await;
        let pat = write_only_pat();

        let empty = patch_entry_for_pat(&pool, &pat, created.id, JournalEntryPatch::default())
            .await
            .expect_err("empty patch must be rejected");
        assert_eq!(empty.status(), StatusCode::BAD_REQUEST);

        let missing = patch_entry_for_pat(
            &pool,
            &pat,
            created.id + 1,
            JournalEntryPatch {
                title: Some("Still missing".into()),
                ..Default::default()
            },
        )
        .await
        .expect_err("unknown entry must remain not found");
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }
}
