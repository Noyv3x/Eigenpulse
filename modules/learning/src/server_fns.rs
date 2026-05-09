use crate::model::*;
#[cfg(feature = "ssr")]
use ep_core::server_err;
#[cfg(feature = "ssr")]
use ep_i18n::{err, err_with};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;

pub(crate) const MAX_BOOK_NAME_CHARS: usize = 128;
pub(crate) const MAX_BOOK_AUTHOR_CHARS: usize = 128;
pub(crate) const MAX_NOTE_TITLE_CHARS: usize = 128;
pub(crate) const MAX_NOTE_BODY_CHARS: usize = 10_000;

#[cfg(feature = "ssr")]
#[derive(Debug)]
struct BookInput {
    name: String,
    author: Option<String>,
    status: String,
    progress: f64,
}

#[cfg(feature = "ssr")]
fn normalize_book_input(
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
struct NoteInput {
    title: String,
    body: Option<String>,
}

#[cfg(feature = "ssr")]
fn normalize_note_input(title: &str, body: &str) -> Result<NoteInput, ServerFnError> {
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
fn normalize_doc_id(doc_id: &str) -> Result<String, ServerFnError> {
    let doc_id =
        ep_core::trim_to_option(doc_id).ok_or_else(|| err("learning.err.doc_id_required"))?;
    if ep_core::safe_doc_id(&doc_id).is_some() {
        Ok(doc_id)
    } else {
        Err(err_with("learning.err.doc_id_invalid", &doc_id))
    }
}

#[server(LoadLearning, "/api/_internal/lrn", "Url", "load_learning")]
pub async fn load_learning() -> Result<LearningData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st = ep_core::app_state_context()?;

        type BookRow = (String, String, Option<String>, String, f64);
        type NoteRow = (String, String, Option<String>, i64);
        type CourseRow = (
            String,
            String,
            Option<String>,
            f64,
            Option<String>,
            Option<String>,
        );

        let books_q = sqlx::query_as::<_, BookRow>(
            "SELECT doc_id, name, author, status, progress FROM lrn_book ORDER BY status, doc_id",
        )
        .fetch_all(&st.db);
        let notes_q = sqlx::query_as::<_, NoteRow>(
            "SELECT doc_id, title, body, updated_at FROM lrn_note ORDER BY updated_at DESC LIMIT 30"
        ).fetch_all(&st.db);
        let courses_q = sqlx::query_as::<_, CourseRow>(
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

        Ok(LearningData {
            books: books
                .into_iter()
                .map(|r| Book {
                    doc_id: r.0,
                    name: r.1,
                    author: r.2,
                    status: r.3,
                    progress: r.4,
                })
                .collect(),
            notes: notes
                .into_iter()
                .map(|r| Note {
                    doc_id: r.0,
                    title: r.1,
                    body: r.2,
                    updated_at: r.3,
                })
                .collect(),
            courses: courses
                .into_iter()
                .map(|r| Course {
                    doc_id: r.0,
                    name: r.1,
                    provider: r.2,
                    progress: r.3,
                    due_on: r.4,
                    tone: r.5,
                })
                .collect(),
            summary: LearningSummary {
                notes_30d: notes_30d as u32,
                books_done,
                books_reading,
                books_todo,
                courses_avg_progress: courses_avg.unwrap_or(0.0) as f32,
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
        let mut tx = st.db.begin().await.map_err(server_err)?;
        let doc_id = ep_core::next_doc_id(
            &mut tx,
            "LRN",
            ep_core::DocIdShape::TypeSerial4 { kind: "B" },
        )
        .await
        .map_err(server_err)?;
        sqlx::query(
            "INSERT INTO lrn_book (doc_id, name, author, status, progress) VALUES (?1, ?2, ?3, ?4, ?5)"
        ).bind(&doc_id).bind(&input.name).bind(&input.author).bind(&input.status).bind(input.progress)
         .execute(&mut *tx).await.map_err(server_err)?;
        let occurred = time::OffsetDateTime::now_utc().unix_timestamp();
        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary, status) VALUES (?1, 'LRN', ?2, ?3, ?4)"
        ).bind(occurred).bind(&doc_id).bind(&input.name).bind(&input.status)
         .execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(Book {
            doc_id,
            name: input.name,
            author: input.author,
            status: input.status,
            progress: input.progress,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
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

#[server(AddNote, "/api/_internal/lrn", "Url", "add_note")]
pub async fn add_note(title: String, body: String) -> Result<Note, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let input = normalize_note_input(&title, &body)?;
        let st = ep_core::app_state_context()?;
        let mut tx = st.db.begin().await.map_err(server_err)?;
        let doc_id = ep_core::next_doc_id(
            &mut tx,
            "LRN",
            ep_core::DocIdShape::TypeSerial4 { kind: "N" },
        )
        .await
        .map_err(server_err)?;
        let updated_at = time::OffsetDateTime::now_utc().unix_timestamp();
        sqlx::query(
            "INSERT INTO lrn_note (doc_id, title, body, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(&doc_id)
        .bind(&input.title)
        .bind(&input.body)
        .bind(updated_at)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary) VALUES (?1, 'LRN', ?2, ?3)"
        ).bind(updated_at).bind(&doc_id).bind(&input.title)
         .execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(Note {
            doc_id,
            title: input.title,
            body: input.body,
            updated_at,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(server_err("ssr-only"))
    }
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
async fn delete_book_inner(pool: &sqlx::SqlitePool, doc_id: &str) -> Result<(), ServerFnError> {
    delete_learning_doc_inner(pool, "lrn_book", "learning.err.book_not_found", doc_id).await
}

#[cfg(feature = "ssr")]
async fn delete_note_inner(pool: &sqlx::SqlitePool, doc_id: &str) -> Result<(), ServerFnError> {
    delete_learning_doc_inner(pool, "lrn_note", "learning.err.note_not_found", doc_id).await
}

#[cfg(feature = "ssr")]
async fn delete_learning_doc_inner(
    pool: &sqlx::SqlitePool,
    table: &str,
    not_found_key: &'static str,
    doc_id: &str,
) -> Result<(), ServerFnError> {
    let mut tx = pool.begin().await.map_err(server_err)?;
    if table == "lrn_book" {
        sqlx::query("UPDATE lrn_note SET book_doc = NULL WHERE book_doc = ?1")
            .bind(doc_id)
            .execute(&mut *tx)
            .await
            .map_err(server_err)?;
    }
    let sql = format!("DELETE FROM {table} WHERE doc_id = ?1");
    let deleted = sqlx::query(&sql)
        .bind(doc_id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    if deleted.rows_affected() == 0 {
        return Err(err_with(not_found_key, doc_id));
    }
    sqlx::query("DELETE FROM activity WHERE module = 'LRN' AND doc_id = ?1")
        .bind(doc_id)
        .execute(&mut *tx)
        .await
        .map_err(server_err)?;
    ep_core::clear_doc_references(&mut tx, doc_id)
        .await
        .map_err(server_err)?;
    tx.commit().await.map_err(server_err)?;
    Ok(())
}
