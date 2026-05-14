use crate::model::*;
#[cfg(feature = "ssr")]
use ep_core::server_err;
#[cfg(feature = "ssr")]
use ep_i18n::{err, err_with};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;

pub(crate) const MAX_BOOK_NAME_CHARS: usize = 128;
pub(crate) const MAX_BOOK_AUTHOR_CHARS: usize = 128;
pub(crate) const MAX_COURSE_NAME_CHARS: usize = 128;
pub(crate) const MAX_COURSE_PROVIDER_CHARS: usize = 128;
pub(crate) const MAX_NOTE_TITLE_CHARS: usize = 128;
pub(crate) const MAX_NOTE_BODY_CHARS: usize = 10_000;

#[cfg(feature = "ssr")]
#[derive(Debug)]
pub struct AddNoteFields {
    pub title: String,
    pub body: String,
}

#[cfg(feature = "ssr")]
#[derive(Debug)]
pub(crate) struct BookInput {
    pub(crate) name: String,
    pub(crate) author: Option<String>,
    pub(crate) status: String,
    pub(crate) progress: f64,
}

#[cfg(feature = "ssr")]
#[derive(Debug)]
pub(crate) struct CourseInput {
    pub(crate) name: String,
    pub(crate) provider: Option<String>,
    pub(crate) progress: f64,
    pub(crate) due_on: Option<String>,
    pub(crate) tone: Option<String>,
}

#[cfg(feature = "ssr")]
pub(crate) fn normalize_book_input(
    name: &str,
    author: &str,
    status: &str,
) -> Result<BookInput, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(err("learning.err.name_required"));
    }
    if name.chars().count() > MAX_BOOK_NAME_CHARS {
        return Err(err_with("learning.err.name_too_long", MAX_BOOK_NAME_CHARS));
    }

    let author = ep_core::trim_to_option(author);
    if author
        .as_deref()
        .is_some_and(|author| author.chars().count() > MAX_BOOK_AUTHOR_CHARS)
    {
        return Err(err_with(
            "learning.err.author_too_long",
            MAX_BOOK_AUTHOR_CHARS,
        ));
    }

    let status = status.trim();
    let status = match status {
        "reading" | "done" | "todo" => status.to_string(),
        "" => "todo".to_string(),
        other => return Err(err_with("learning.err.status_invalid", other)),
    };
    let progress = if status == "done" { 1.0 } else { 0.0 };

    Ok(BookInput {
        name: name.to_string(),
        author,
        status,
        progress,
    })
}

#[cfg(feature = "ssr")]
#[derive(Debug)]
pub(crate) struct NoteInput {
    pub(crate) title: String,
    pub(crate) body: Option<String>,
}

#[cfg(feature = "ssr")]
pub(crate) fn normalize_note_input(title: &str, body: &str) -> Result<NoteInput, ServerFnError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(err("learning.err.title_required"));
    }
    if title.chars().count() > MAX_NOTE_TITLE_CHARS {
        return Err(err_with(
            "learning.err.title_too_long",
            MAX_NOTE_TITLE_CHARS,
        ));
    }

    let body = ep_core::trim_to_option(body);
    if body
        .as_deref()
        .is_some_and(|body| body.chars().count() > MAX_NOTE_BODY_CHARS)
    {
        return Err(err_with("learning.err.body_too_long", MAX_NOTE_BODY_CHARS));
    }

    Ok(NoteInput {
        title: title.to_string(),
        body,
    })
}

#[cfg(feature = "ssr")]
pub(crate) fn normalize_course_input(
    name: &str,
    provider: &str,
    progress_pct: f64,
    due_on: &str,
    tone: &str,
) -> Result<CourseInput, ServerFnError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(err("learning.err.course_name_required"));
    }
    if name.chars().count() > MAX_COURSE_NAME_CHARS {
        return Err(err_with(
            "learning.err.course_name_too_long",
            MAX_COURSE_NAME_CHARS,
        ));
    }

    let provider = ep_core::trim_to_option(provider);
    if provider
        .as_deref()
        .is_some_and(|provider| provider.chars().count() > MAX_COURSE_PROVIDER_CHARS)
    {
        return Err(err_with(
            "learning.err.course_provider_too_long",
            MAX_COURSE_PROVIDER_CHARS,
        ));
    }

    let progress = normalize_progress_pct(progress_pct)?;
    let due_on = normalize_due_on(due_on)?;
    let tone = normalize_tone(tone)?;

    Ok(CourseInput {
        name: name.to_string(),
        provider,
        progress,
        due_on,
        tone,
    })
}

#[cfg(feature = "ssr")]
fn normalize_progress_pct(progress_pct: f64) -> Result<f64, ServerFnError> {
    if !progress_pct.is_finite() {
        return Err(err("learning.err.progress_invalid"));
    }
    if !(0.0..=100.0).contains(&progress_pct) {
        return Err(err("learning.err.progress_invalid"));
    }
    Ok(progress_pct / 100.0)
}

#[cfg(feature = "ssr")]
fn normalize_due_on(due_on: &str) -> Result<Option<String>, ServerFnError> {
    let Some(due_on) = ep_core::trim_to_option(due_on) else {
        return Ok(None);
    };
    // Stored as the raw `YYYY-MM-DD` string; only validate that it is a real
    // calendar date before persisting.
    let valid = ep_core::parse_ymd(&due_on).is_some_and(|(year, month, day)| {
        ep_core::ymd_to_unix_midnight(year, month, day).is_some()
    });
    if valid {
        Ok(Some(due_on))
    } else {
        Err(err_with("learning.err.due_on_invalid", &due_on))
    }
}

#[cfg(feature = "ssr")]
fn normalize_tone(tone: &str) -> Result<Option<String>, ServerFnError> {
    let Some(tone) = ep_core::trim_to_option(tone) else {
        return Ok(None);
    };
    if ep_core::Tone::parse(&tone) == ep_core::Tone::None && tone != "none" {
        return Err(err_with("learning.err.tone_invalid", &tone));
    }
    if tone == "none" {
        Ok(None)
    } else {
        Ok(Some(tone))
    }
}

#[cfg(feature = "ssr")]
pub(crate) fn normalize_doc_id(doc_id: &str) -> Result<String, ServerFnError> {
    match ep_core::normalize_doc_id_input(doc_id) {
        Ok(doc_id) => Ok(doc_id),
        Err(ep_core::DocIdInputError::Required) => Err(err("learning.err.doc_id_required")),
        Err(ep_core::DocIdInputError::Invalid(doc_id)) => {
            Err(err_with("learning.err.doc_id_invalid", &doc_id))
        }
    }
}

#[server(AddCourse, "/api/_internal/lrn", "Url", "add_course")]
pub async fn add_course(
    name: String,
    provider: String,
    progress_pct: f64,
    due_on: String,
    tone: String,
) -> Result<Course, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;
        let input = normalize_course_input(&name, &provider, progress_pct, &due_on, &tone)?;
        add_course_inner(&st.db, input).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn add_course_inner(
    pool: &sqlx::SqlitePool,
    input: CourseInput,
) -> Result<Course, ServerFnError> {
    let mut tx = pool.begin().await.map_err(server_err)?;
    let doc_id = ep_core::next_doc_id(
        &mut tx,
        "LRN",
        ep_core::DocIdShape::TypeSerial4 { kind: "C" },
    )
    .await
    .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO lrn_course (doc_id, name, provider, progress, due_on, tone)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&doc_id)
    .bind(&input.name)
    .bind(&input.provider)
    .bind(input.progress)
    .bind(&input.due_on)
    .bind(&input.tone)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, status)
         VALUES (?1, 'LRN', ?2, ?3, ?4)",
    )
    .bind(ep_core::unix_now())
    .bind(&doc_id)
    .bind(&input.name)
    .bind(format!("{:.0}%", input.progress * 100.0))
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(Course {
        doc_id,
        name: input.name,
        provider: input.provider,
        progress: input.progress,
        due_on: input.due_on,
        tone: input.tone,
    })
}

#[server(
    UpdateCourseProgress,
    "/api/_internal/lrn",
    "Url",
    "update_course_progress"
)]
pub async fn update_course_progress(
    doc_id: String,
    progress_pct: f64,
) -> Result<Course, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let progress = normalize_progress_pct(progress_pct)?;
        let st = ep_core::app_state_context()?;
        let mut tx = st.db.begin().await.map_err(server_err)?;
        let row: Option<Course> = sqlx::query_as(
            "UPDATE lrn_course
                SET progress = ?1
              WHERE doc_id = ?2 AND archived = 0
              RETURNING doc_id, name, provider, progress, due_on, tone",
        )
        .bind(progress)
        .bind(&doc_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(server_err)?;
        let mut course = match row {
            Some(row) => row,
            None => return Err(err_with("learning.err.course_not_found", &doc_id)),
        };
        sqlx::query("UPDATE activity SET status = ?1 WHERE module = 'LRN' AND doc_id = ?2")
            .bind(format!("{:.0}%", progress * 100.0))
            .bind(&doc_id)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        course.progress = normalize_progress(course.progress);
        Ok(course)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[server(UpdateCourse, "/api/_internal/lrn", "Url", "update_course")]
pub async fn update_course(
    doc_id: String,
    name: String,
    provider: String,
    progress_pct: f64,
    due_on: String,
    tone: String,
) -> Result<Course, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let input = normalize_course_input(&name, &provider, progress_pct, &due_on, &tone)?;
        let st = ep_core::app_state_context()?;
        update_course_inner(&st.db, &doc_id, input).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn update_course_inner(
    pool: &sqlx::SqlitePool,
    doc_id: &str,
    input: CourseInput,
) -> Result<Course, ServerFnError> {
    let mut tx = pool.begin().await.map_err(server_err)?;
    let row: Option<Course> = sqlx::query_as(
        "UPDATE lrn_course
            SET name = ?1,
                provider = ?2,
                progress = ?3,
                due_on = ?4,
                tone = ?5
          WHERE doc_id = ?6 AND archived = 0
          RETURNING doc_id, name, provider, progress, due_on, tone",
    )
    .bind(&input.name)
    .bind(&input.provider)
    .bind(input.progress)
    .bind(&input.due_on)
    .bind(&input.tone)
    .bind(doc_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(server_err)?;
    let mut course = match row {
        Some(row) => row,
        None => return Err(err_with("learning.err.course_not_found", doc_id)),
    };
    sqlx::query(
        "UPDATE activity
            SET summary = ?1,
                status = ?2
          WHERE module = 'LRN' AND doc_id = ?3",
    )
    .bind(&course.name)
    .bind(format!("{:.0}%", course.progress * 100.0))
    .bind(doc_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    course.progress = normalize_progress(course.progress);
    Ok(course)
}

#[cfg(feature = "ssr")]
fn normalize_progress(progress: f64) -> f64 {
    if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[server(LoadLearning, "/api/_internal/lrn", "Url", "load_learning")]
pub async fn load_learning() -> Result<LearningData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;

        // Full-row SELECTs decode into the model structs via `sqlx::FromRow`.
        let books_q = sqlx::query_as::<_, Book>(
            "SELECT doc_id, name, author, status, progress FROM lrn_book ORDER BY status, doc_id",
        )
        .fetch_all(&st.db);
        let notes_q = sqlx::query_as::<_, Note>(
            "SELECT doc_id, title, body, updated_at FROM lrn_note ORDER BY updated_at DESC LIMIT 30"
        ).fetch_all(&st.db);
        let courses_q = sqlx::query_as::<_, Course>(
            "SELECT doc_id, name, provider, progress, due_on, tone FROM lrn_course WHERE archived = 0 ORDER BY due_on"
        ).fetch_all(&st.db);

        // Summary aggregates run alongside the detail queries on the same try_join.
        let notes_30d_q = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM lrn_note
              WHERE updated_at >= unixepoch('now','localtime','-30 days','utc')",
        )
        .fetch_one(&st.db);
        // (status, COUNT) — we'll fan out into reading/done/todo client-side.
        let book_status_q = sqlx::query_as::<_, (String, i64)>(
            "SELECT status, COUNT(*) FROM lrn_book GROUP BY status",
        )
        .fetch_all(&st.db);
        let courses_avg_q = sqlx::query_scalar::<_, Option<f64>>(
            "SELECT AVG(progress) FROM lrn_course WHERE archived = 0",
        )
        .fetch_one(&st.db);
        // 28-day note density: integer day distance between each note's
        // local date and today's local date. The `'start of day'` modifier
        // on BOTH sides is load-bearing — without it, `julianday(...)`
        // preserves the sub-day fractional, so a 02:00 note and a 22:00
        // prior-day note both round to the same integer JD even though
        // they're on different local calendar days. With `'start of day'`
        // both anchor to local 00:00 and the diff is whole-day-aligned.
        let heatmap_q = sqlx::query_as::<_, (i64, i64)>(
            "SELECT CAST(julianday('now','localtime','start of day')
                         - julianday(updated_at,'unixepoch','localtime','start of day') AS INTEGER) AS days_ago,
                    COUNT(*)
               FROM lrn_note
              WHERE updated_at >= unixepoch('now','localtime','-28 days','utc')
              GROUP BY days_ago"
        ).fetch_all(&st.db);

        let (books, notes, courses, notes_30d, book_status, courses_avg, heatmap_rows) =
            tokio::try_join!(
                books_q,
                notes_q,
                courses_q,
                notes_30d_q,
                book_status_q,
                courses_avg_q,
                heatmap_q
            )
            .map_err(server_err)?;

        let mut books_done = 0u32;
        let mut books_reading = 0u32;
        let mut books_todo = 0u32;
        for (status, count) in &book_status {
            match status.as_str() {
                "done" => books_done = *count as u32,
                "reading" => books_reading = *count as u32,
                "todo" => books_todo = *count as u32,
                _ => {}
            }
        }

        // 28-day note density: index 0 = 27-days-ago, index 27 = today, with
        // the count clamped to 0..4 so it fits the Heatmap component's
        // intensity scale.
        let mut note_heatmap_28d = vec![0u8; 28];
        for (days_ago, count) in heatmap_rows {
            if (0..28).contains(&days_ago) {
                let idx = (27 - days_ago) as usize;
                // count is COUNT(*) so always ≥ 0; capping at 4 lands in the
                // Heatmap component's 0..=4 intensity scale.
                note_heatmap_28d[idx] = count.min(4) as u8;
            }
        }

        // Defensive clamp on stored course progress (bad/legacy data → 0..1).
        let courses = courses
            .into_iter()
            .map(|mut c| {
                c.progress = normalize_progress(c.progress);
                c
            })
            .collect();

        Ok(LearningData {
            books,
            notes,
            courses,
            summary: LearningSummary {
                notes_30d: notes_30d as u32,
                books_done,
                books_reading,
                books_todo,
                courses_avg_progress: courses_avg.map(normalize_progress).unwrap_or(0.0) as f32,
                note_heatmap_28d,
            },
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[server(AddBook, "/api/_internal/lrn", "Url", "add_book")]
pub async fn add_book(name: String, author: String, status: String) -> Result<Book, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let input = normalize_book_input(&name, &author, &status)?;
        let st = ep_core::app_state_context()?;
        add_book_inner(&st.db, input).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn add_book_inner(
    pool: &sqlx::SqlitePool,
    input: BookInput,
) -> Result<Book, ServerFnError> {
    let mut tx = pool.begin().await.map_err(server_err)?;
    let doc_id = ep_core::next_doc_id(
        &mut tx,
        "LRN",
        ep_core::DocIdShape::TypeSerial4 { kind: "B" },
    )
    .await
    .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO lrn_book (doc_id, name, author, status, progress) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&doc_id)
    .bind(&input.name)
    .bind(&input.author)
    .bind(&input.status)
    .bind(input.progress)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    let occurred = ep_core::unix_now();
    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary, status) VALUES (?1, 'LRN', ?2, ?3, ?4)",
    )
    .bind(occurred)
    .bind(&doc_id)
    .bind(&input.name)
    .bind(&input.status)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(Book {
        doc_id,
        name: input.name,
        author: input.author,
        status: input.status,
        progress: input.progress,
    })
}

#[server(UpdateBook, "/api/_internal/lrn", "Url", "update_book")]
pub async fn update_book(
    doc_id: String,
    name: String,
    author: String,
    status: String,
) -> Result<Book, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let input = normalize_book_input(&name, &author, &status)?;
        let st = ep_core::app_state_context()?;
        update_book_inner(&st.db, &doc_id, input).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn update_book_inner(
    pool: &sqlx::SqlitePool,
    doc_id: &str,
    input: BookInput,
) -> Result<Book, ServerFnError> {
    let mut tx = pool.begin().await.map_err(server_err)?;
    let row: Option<Book> = sqlx::query_as(
        "UPDATE lrn_book
            SET name = ?1,
                author = ?2,
                status = ?3,
                progress = ?4
          WHERE doc_id = ?5
          RETURNING doc_id, name, author, status, progress",
    )
    .bind(&input.name)
    .bind(&input.author)
    .bind(&input.status)
    .bind(input.progress)
    .bind(doc_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(server_err)?;
    let book = match row {
        Some(row) => row,
        None => return Err(err_with("learning.err.book_not_found", doc_id)),
    };
    sqlx::query(
        "UPDATE activity
            SET summary = ?1,
                status = ?2
          WHERE module = 'LRN' AND doc_id = ?3",
    )
    .bind(&book.name)
    .bind(&book.status)
    .bind(doc_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(book)
}

#[server(CycleBookStatus, "/api/_internal/lrn", "Url", "cycle_book_status")]
pub async fn cycle_book_status(doc_id: String) -> Result<String, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let st = ep_core::app_state_context()?;
        let mut tx = st.db.begin().await.map_err(server_err)?;
        // status cycle: todo -> reading -> done -> todo
        // progress is bumped to mirror the next state:
        //   reading (was 'todo')      -> 0.5  (in-progress; arbitrary midpoint)
        //   done    (was 'reading')   -> 1.0
        //   todo    (was 'done')      -> 0.0
        // The CASE is keyed on the OLD status because SQLite evaluates both
        // SET expressions against the row before any updates land.
        let next: Option<String> = sqlx::query_scalar(
            r#"UPDATE lrn_book
               SET status = CASE status
                   WHEN 'todo' THEN 'reading'
                   WHEN 'reading' THEN 'done'
                   ELSE 'todo' END,
                   progress = CASE status
                   WHEN 'todo' THEN 0.5
                   WHEN 'reading' THEN 1.0
                   ELSE 0.0 END
               WHERE doc_id = ?1
               RETURNING status"#,
        )
        .bind(&doc_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(server_err)?;
        let next = match next {
            Some(next) => next,
            None => return Err(err_with("learning.err.book_not_found", &doc_id)),
        };
        sqlx::query("UPDATE activity SET status = ?1 WHERE module = 'LRN' AND doc_id = ?2")
            .bind(&next)
            .bind(&doc_id)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(next)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[server(DeleteBook, "/api/_internal/lrn", "Url", "delete_book")]
pub async fn delete_book(doc_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let st = ep_core::app_state_context()?;
        delete_book_inner(&st.db, &doc_id).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[server(DeleteCourse, "/api/_internal/lrn", "Url", "delete_course")]
pub async fn delete_course(doc_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let st = ep_core::app_state_context()?;
        delete_course_inner(&st.db, &doc_id).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[server(AddNote, "/api/_internal/lrn", "Url", "add_note")]
pub async fn add_note(title: String, body: String) -> Result<Note, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;
        add_note_inner(&st.db, AddNoteFields { title, body }).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
pub async fn add_note_inner(
    pool: &sqlx::SqlitePool,
    fields: AddNoteFields,
) -> Result<Note, ServerFnError> {
    let input = normalize_note_input(&fields.title, &fields.body)?;
    let mut tx = pool.begin().await.map_err(server_err)?;
    let doc_id = ep_core::next_doc_id(
        &mut tx,
        "LRN",
        ep_core::DocIdShape::TypeSerial4 { kind: "N" },
    )
    .await
    .map_err(server_err)?;
    let updated_at = ep_core::unix_now();
    sqlx::query("INSERT INTO lrn_note (doc_id, title, body, updated_at) VALUES (?1, ?2, ?3, ?4)")
        .bind(&doc_id)
        .bind(&input.title)
        .bind(&input.body)
        .bind(updated_at)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    sqlx::query(
        "INSERT INTO activity (occurred_at, module, doc_id, summary) VALUES (?1, 'LRN', ?2, ?3)",
    )
    .bind(updated_at)
    .bind(&doc_id)
    .bind(&input.title)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(Note {
        doc_id,
        title: input.title,
        body: input.body,
        updated_at,
    })
}

#[server(UpdateNote, "/api/_internal/lrn", "Url", "update_note")]
pub async fn update_note(
    doc_id: String,
    title: String,
    body: String,
) -> Result<Note, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let input = normalize_note_input(&title, &body)?;
        let st = ep_core::app_state_context()?;
        update_note_inner(&st.db, &doc_id, input).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn update_note_inner(
    pool: &sqlx::SqlitePool,
    doc_id: &str,
    input: NoteInput,
) -> Result<Note, ServerFnError> {
    let updated_at = ep_core::unix_now();
    let mut tx = pool.begin().await.map_err(server_err)?;
    let row: Option<Note> = sqlx::query_as(
        "UPDATE lrn_note
            SET title = ?1,
                body = ?2,
                updated_at = ?3
          WHERE doc_id = ?4
          RETURNING doc_id, title, body, updated_at",
    )
    .bind(&input.title)
    .bind(&input.body)
    .bind(updated_at)
    .bind(doc_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(server_err)?;
    let note = match row {
        Some(row) => row,
        None => return Err(err_with("learning.err.note_not_found", doc_id)),
    };
    sqlx::query(
        "UPDATE activity
            SET occurred_at = ?1,
                summary = ?2
          WHERE module = 'LRN' AND doc_id = ?3",
    )
    .bind(note.updated_at)
    .bind(&note.title)
    .bind(doc_id)
    .execute(&mut *tx)
    .await
    .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(note)
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    #[test]
    fn normalize_book_input_trims_and_defaults_status() {
        let got = normalize_book_input("  Domain Modeling  ", "  Evans  ", "   ").unwrap();
        assert_eq!(got.name, "Domain Modeling");
        assert_eq!(got.author.as_deref(), Some("Evans"));
        assert_eq!(got.status, "todo");
        assert_eq!(got.progress, 0.0);
    }

    #[test]
    fn normalize_book_input_done_sets_full_progress() {
        let got = normalize_book_input("Book", "", " done ").unwrap();
        assert_eq!(got.author, None);
        assert_eq!(got.status, "done");
        assert_eq!(got.progress, 1.0);
    }

    #[test]
    fn normalize_book_input_rejects_blank_name_and_invalid_status() {
        assert!(normalize_book_input("   ", "", "todo").is_err());
        assert!(normalize_book_input("Book", "", "paused").is_err());
    }

    #[test]
    fn normalize_book_input_enforces_text_lengths() {
        let name_err = normalize_book_input(&"x".repeat(MAX_BOOK_NAME_CHARS + 1), "", "todo")
            .expect_err("long name should fail");
        assert_eq!(
            ep_i18n::parse_err(&name_err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("learning.err.name_too_long", "128"))
        );

        let author_err =
            normalize_book_input("Book", &"x".repeat(MAX_BOOK_AUTHOR_CHARS + 1), "todo")
                .expect_err("long author should fail");
        assert_eq!(
            ep_i18n::parse_err(&author_err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("learning.err.author_too_long", "128"))
        );
    }

    #[test]
    fn normalize_note_input_trims_optional_body() {
        let got = normalize_note_input("  Note  ", "  body  ").unwrap();
        assert_eq!(got.title, "Note");
        assert_eq!(got.body.as_deref(), Some("body"));

        let empty = normalize_note_input("Title", "   ").unwrap();
        assert_eq!(empty.body, None);
    }

    #[test]
    fn normalize_note_input_rejects_blank_title_and_overlong_text() {
        assert!(normalize_note_input("   ", "").is_err());

        let title_err = normalize_note_input(&"x".repeat(MAX_NOTE_TITLE_CHARS + 1), "")
            .expect_err("long title should fail");
        assert_eq!(
            ep_i18n::parse_err(&title_err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("learning.err.title_too_long", "128"))
        );

        let body_err = normalize_note_input("Title", &"x".repeat(MAX_NOTE_BODY_CHARS + 1))
            .expect_err("long body should fail");
        assert_eq!(
            ep_i18n::parse_err(&body_err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("learning.err.body_too_long", "10000"))
        );
    }

    #[test]
    fn normalize_course_input_trims_and_converts_progress() {
        let got = normalize_course_input("  Rust  ", "  Book  ", 42.5, " 2026-07-15 ", " blue ")
            .expect("valid course");
        assert_eq!(got.name, "Rust");
        assert_eq!(got.provider.as_deref(), Some("Book"));
        assert_eq!(got.progress, 0.425);
        assert_eq!(got.due_on.as_deref(), Some("2026-07-15"));
        assert_eq!(got.tone.as_deref(), Some("blue"));
    }

    #[test]
    fn normalize_course_input_rejects_invalid_values() {
        assert!(normalize_course_input("   ", "", 10.0, "", "").is_err());
        assert!(normalize_course_input("Course", "", -1.0, "", "").is_err());
        assert!(normalize_course_input("Course", "", 101.0, "", "").is_err());
        assert!(normalize_course_input("Course", "", f64::NAN, "", "").is_err());
        assert!(normalize_course_input("Course", "", 10.0, "2026/07/15", "").is_err());
        assert!(normalize_course_input("Course", "", 10.0, "2026-02-31", "").is_err());
        assert!(normalize_course_input("Course", "", 10.0, "", "cyan").is_err());
    }

    #[test]
    fn normalize_doc_id_trims_and_rejects_blank() {
        assert_eq!(normalize_doc_id("  LRN-B-0001  ").unwrap(), "LRN-B-0001");
        assert!(normalize_doc_id("   ").is_err());
    }

    #[test]
    fn normalize_doc_id_rejects_invalid_shape() {
        let err = normalize_doc_id("https://example.com").expect_err("invalid doc id");

        assert_eq!(
            ep_i18n::parse_err(&err).map(|(code, payload)| (code, payload.unwrap_or(""))),
            Some(("learning.err.doc_id_invalid", "https://example.com"))
        );
    }

    #[test]
    fn normalize_progress_clamps_invalid_course_values() {
        assert_eq!(normalize_progress(-0.2), 0.0);
        assert_eq!(normalize_progress(0.4), 0.4);
        assert_eq!(normalize_progress(1.2), 1.0);
        assert_eq!(normalize_progress(f64::INFINITY), 0.0);
        assert_eq!(normalize_progress(f64::NAN), 0.0);
    }

    async fn ref_cleanup_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE lrn_book (
                doc_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                author TEXT,
                status TEXT NOT NULL DEFAULT 'reading',
                progress REAL NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE lrn_note (
                doc_id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                body TEXT,
                tags TEXT,
                course_doc TEXT,
                book_doc TEXT,
                updated_at INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
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
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE module_link (
                source_doc TEXT NOT NULL,
                target_doc TEXT NOT NULL,
                kind TEXT NOT NULL DEFAULT 'ref',
                PRIMARY KEY (source_doc, target_doc, kind)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE activity (
                module TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                occurred_at INTEGER,
                summary TEXT,
                status TEXT,
                link_doc TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("CREATE TABLE notification (id INTEGER PRIMARY KEY, doc_ref TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    async fn seed_external_refs(pool: &sqlx::SqlitePool, target_doc: &str) {
        sqlx::query(
            "INSERT INTO module_link (source_doc, target_doc, kind)
             VALUES ('FIN-26092', ?1, 'ref')",
        )
        .bind(target_doc)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO activity (module, doc_id, link_doc) VALUES
             ('LRN', ?1, NULL),
             ('FIN', 'FIN-26092', ?1)",
        )
        .bind(target_doc)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO notification (id, doc_ref) VALUES (1, ?1)")
            .bind(target_doc)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn assert_external_refs_cleared(pool: &sqlx::SqlitePool, target_doc: &str) {
        let links: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM module_link WHERE source_doc = ?1 OR target_doc = ?1",
        )
        .bind(target_doc)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(links, 0);

        let own_activity: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM activity WHERE module = 'LRN' AND doc_id = ?1",
        )
        .bind(target_doc)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(own_activity, 0);

        let external_link: Option<String> =
            sqlx::query_scalar("SELECT link_doc FROM activity WHERE doc_id = 'FIN-26092'")
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(external_link, None);

        let doc_ref: Option<String> =
            sqlx::query_scalar("SELECT doc_ref FROM notification WHERE id = 1")
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(doc_ref, None);
    }

    #[tokio::test]
    async fn delete_book_inner_clears_external_references() {
        let pool = ref_cleanup_pool().await;
        sqlx::query(
            "INSERT INTO lrn_book (doc_id, name, status, progress)
             VALUES ('LRN-B-0001', 'Book', 'todo', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        seed_external_refs(&pool, "LRN-B-0001").await;

        delete_book_inner(&pool, "LRN-B-0001")
            .await
            .expect("delete book");

        let books: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lrn_book")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(books, 0);
        assert_external_refs_cleared(&pool, "LRN-B-0001").await;
    }

    #[tokio::test]
    async fn delete_book_inner_clears_note_book_references() {
        let pool = ref_cleanup_pool().await;
        sqlx::query(
            "INSERT INTO lrn_book (doc_id, name, status, progress)
             VALUES ('LRN-B-0001', 'Book', 'todo', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO lrn_note (doc_id, title, book_doc, updated_at)
             VALUES ('LRN-N-0001', 'Note', 'LRN-B-0001', 1_700_000_000)",
        )
        .execute(&pool)
        .await
        .unwrap();

        delete_book_inner(&pool, "LRN-B-0001")
            .await
            .expect("delete book");

        let book_doc: Option<String> =
            sqlx::query_scalar("SELECT book_doc FROM lrn_note WHERE doc_id = 'LRN-N-0001'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(book_doc, None);
    }

    #[tokio::test]
    async fn update_book_inner_updates_book_and_activity() {
        let pool = ref_cleanup_pool().await;
        sqlx::query(
            "INSERT INTO lrn_book (doc_id, name, author, status, progress)
             VALUES ('LRN-B-0001', 'Old', 'A', 'todo', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO activity (module, doc_id, summary, status)
             VALUES ('LRN', 'LRN-B-0001', 'Old', 'todo')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let input = normalize_book_input(" New ", " B ", "done").unwrap();
        let got = update_book_inner(&pool, "LRN-B-0001", input)
            .await
            .expect("update book");

        assert_eq!(got.name, "New");
        assert_eq!(got.author.as_deref(), Some("B"));
        assert_eq!(got.status, "done");
        assert_eq!(got.progress, 1.0);

        let activity: (String, String) = sqlx::query_as(
            "SELECT summary, status FROM activity WHERE module = 'LRN' AND doc_id = 'LRN-B-0001'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(activity, ("New".into(), "done".into()));
    }

    #[tokio::test]
    async fn delete_note_inner_clears_external_references() {
        let pool = ref_cleanup_pool().await;
        sqlx::query(
            "INSERT INTO lrn_note (doc_id, title, updated_at)
             VALUES ('LRN-N-0001', 'Note', 1_700_000_000)",
        )
        .execute(&pool)
        .await
        .unwrap();
        seed_external_refs(&pool, "LRN-N-0001").await;

        delete_note_inner(&pool, "LRN-N-0001")
            .await
            .expect("delete note");

        let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lrn_note")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(notes, 0);
        assert_external_refs_cleared(&pool, "LRN-N-0001").await;
    }

    #[tokio::test]
    async fn delete_course_inner_clears_note_course_references_and_external_refs() {
        let pool = ref_cleanup_pool().await;
        sqlx::query(
            "INSERT INTO lrn_course (doc_id, name, progress)
             VALUES ('LRN-C-0001', 'Course', 0.4)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO lrn_note (doc_id, title, course_doc, updated_at)
             VALUES ('LRN-N-0001', 'Note', 'LRN-C-0001', 1_700_000_000)",
        )
        .execute(&pool)
        .await
        .unwrap();
        seed_external_refs(&pool, "LRN-C-0001").await;

        delete_course_inner(&pool, "LRN-C-0001")
            .await
            .expect("delete course");

        let courses: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lrn_course")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(courses, 0);
        let course_doc: Option<String> =
            sqlx::query_scalar("SELECT course_doc FROM lrn_note WHERE doc_id = 'LRN-N-0001'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(course_doc, None);
        assert_external_refs_cleared(&pool, "LRN-C-0001").await;
    }

    #[tokio::test]
    async fn update_course_inner_updates_course_and_activity() {
        let pool = ref_cleanup_pool().await;
        sqlx::query(
            "INSERT INTO lrn_course (doc_id, name, provider, progress, due_on, tone)
             VALUES ('LRN-C-0001', 'Old', 'Provider A', 0.2, '2026-05-01', 'blue')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO activity (module, doc_id, summary, status)
             VALUES ('LRN', 'LRN-C-0001', 'Old', '20%')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let input =
            normalize_course_input("New", "Provider B", 75.0, "2026-06-30", "green").unwrap();
        let got = update_course_inner(&pool, "LRN-C-0001", input)
            .await
            .expect("update course");

        assert_eq!(got.name, "New");
        assert_eq!(got.provider.as_deref(), Some("Provider B"));
        assert_eq!(got.progress, 0.75);
        assert_eq!(got.due_on.as_deref(), Some("2026-06-30"));
        assert_eq!(got.tone.as_deref(), Some("green"));

        let activity: (String, String) = sqlx::query_as(
            "SELECT summary, status FROM activity WHERE module = 'LRN' AND doc_id = 'LRN-C-0001'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(activity, ("New".into(), "75%".into()));
    }

    #[tokio::test]
    async fn update_note_inner_updates_note_and_activity() {
        let pool = ref_cleanup_pool().await;
        sqlx::query(
            "INSERT INTO lrn_note (doc_id, title, body, updated_at)
             VALUES ('LRN-N-0001', 'Old', 'old body', 1_700_000_000)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO activity (module, doc_id, occurred_at, summary)
             VALUES ('LRN', 'LRN-N-0001', 1_700_000_000, 'Old')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let input = normalize_note_input("  New  ", "  new body  ").unwrap();
        let got = update_note_inner(&pool, "LRN-N-0001", input)
            .await
            .expect("update note");

        assert_eq!(got.title, "New");
        assert_eq!(got.body.as_deref(), Some("new body"));
        assert!(got.updated_at >= 1_700_000_000);

        let activity: (String, i64) = sqlx::query_as(
            "SELECT summary, occurred_at FROM activity WHERE module = 'LRN' AND doc_id = 'LRN-N-0001'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(activity.0, "New");
        assert_eq!(activity.1, got.updated_at);
    }
}

#[server(DeleteNote, "/api/_internal/lrn", "Url", "delete_note")]
pub async fn delete_note(doc_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let doc_id = normalize_doc_id(&doc_id)?;
        let st = ep_core::app_state_context()?;
        delete_note_inner(&st.db, &doc_id).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn delete_book_inner(
    pool: &sqlx::SqlitePool,
    doc_id: &str,
) -> Result<(), ServerFnError> {
    delete_learning_doc_inner(pool, LearningDocKind::Book, doc_id).await
}

#[cfg(feature = "ssr")]
pub async fn delete_note_inner(pool: &sqlx::SqlitePool, doc_id: &str) -> Result<(), ServerFnError> {
    delete_learning_doc_inner(pool, LearningDocKind::Note, doc_id).await
}

#[cfg(feature = "ssr")]
pub(crate) async fn delete_course_inner(
    pool: &sqlx::SqlitePool,
    doc_id: &str,
) -> Result<(), ServerFnError> {
    delete_learning_doc_inner(pool, LearningDocKind::Course, doc_id).await
}

#[cfg(feature = "ssr")]
#[derive(Clone, Copy)]
enum LearningDocKind {
    Book,
    Course,
    Note,
}

#[cfg(feature = "ssr")]
impl LearningDocKind {
    fn delete_sql(self) -> &'static str {
        match self {
            Self::Book => "DELETE FROM lrn_book WHERE doc_id = ?1",
            Self::Course => "DELETE FROM lrn_course WHERE doc_id = ?1",
            Self::Note => "DELETE FROM lrn_note WHERE doc_id = ?1",
        }
    }

    fn not_found_key(self) -> &'static str {
        match self {
            Self::Book => "learning.err.book_not_found",
            Self::Course => "learning.err.course_not_found",
            Self::Note => "learning.err.note_not_found",
        }
    }
}

#[cfg(feature = "ssr")]
async fn delete_learning_doc_inner(
    pool: &sqlx::SqlitePool,
    kind: LearningDocKind,
    doc_id: &str,
) -> Result<(), ServerFnError> {
    let mut tx = pool.begin().await.map_err(server_err)?;
    if matches!(kind, LearningDocKind::Book) {
        sqlx::query("UPDATE lrn_note SET book_doc = NULL WHERE book_doc = ?1")
            .bind(doc_id)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
    }
    if matches!(kind, LearningDocKind::Course) {
        sqlx::query("UPDATE lrn_note SET course_doc = NULL WHERE course_doc = ?1")
            .bind(doc_id)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
    }
    let deleted = sqlx::query(kind.delete_sql())
        .bind(doc_id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    if deleted.rows_affected() == 0 {
        return Err(err_with(kind.not_found_key(), doc_id));
    }
    ep_core::delete_doc_activity_and_references(&mut tx, "LRN", doc_id)
        .await
        .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(())
}
