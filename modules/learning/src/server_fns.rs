use crate::model::*;
#[cfg(feature = "ssr")]
use ep_core::server_err;
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;

#[server(LoadLearning, "/api/_internal/lrn", "Url", "load_learning")]
pub async fn load_learning() -> Result<LearningData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();

        type BookRow = (String, String, Option<String>, String, f64);
        type NoteRow = (String, String, Option<String>, i64);
        type CourseRow = (String, String, Option<String>, f64, Option<String>, Option<String>);

        let books_q = sqlx::query_as::<_, BookRow>(
            "SELECT doc_id, name, author, status, progress FROM lrn_book ORDER BY status, doc_id"
        ).fetch_all(&st.db);
        let notes_q = sqlx::query_as::<_, NoteRow>(
            "SELECT doc_id, title, body, updated_at FROM lrn_note ORDER BY updated_at DESC LIMIT 30"
        ).fetch_all(&st.db);
        let courses_q = sqlx::query_as::<_, CourseRow>(
            "SELECT doc_id, name, provider, progress, due_on, tone FROM lrn_course WHERE archived = 0 ORDER BY due_on"
        ).fetch_all(&st.db);

        // Summary aggregates run alongside the detail queries on the same try_join.
        let notes_30d_q = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM lrn_note
              WHERE updated_at >= unixepoch('now','localtime','-30 days','utc')"
        ).fetch_one(&st.db);
        // (status, COUNT) — we'll fan out into reading/done/todo client-side.
        let book_status_q = sqlx::query_as::<_, (String, i64)>(
            "SELECT status, COUNT(*) FROM lrn_book GROUP BY status"
        ).fetch_all(&st.db);
        let courses_avg_q = sqlx::query_scalar::<_, Option<f64>>(
            "SELECT AVG(progress) FROM lrn_course WHERE archived = 0"
        ).fetch_one(&st.db);
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
            tokio::try_join!(books_q, notes_q, courses_q,
                             notes_30d_q, book_status_q, courses_avg_q, heatmap_q)
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
            books: books.into_iter().map(|r| Book {
                doc_id: r.0, name: r.1, author: r.2, status: r.3, progress: r.4
            }).collect(),
            notes: notes.into_iter().map(|r| Note {
                doc_id: r.0, title: r.1, body: r.2, updated_at: r.3
            }).collect(),
            courses: courses.into_iter().map(|r| Course {
                doc_id: r.0, name: r.1, provider: r.2, progress: r.3, due_on: r.4, tone: r.5
            }).collect(),
            summary: LearningSummary {
                notes_30d: notes_30d as u32,
                books_done, books_reading, books_todo,
                courses_avg_progress: courses_avg.unwrap_or(0.0) as f32,
                note_heatmap_28d,
            },
        })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(AddBook, "/api/_internal/lrn", "Url", "add_book")]
pub async fn add_book(name: String, author: String, status: String) -> Result<Book, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        if name.trim().is_empty() { return Err(ServerFnError::Args("name is required".into())); }
        let status_norm = match status.as_str() {
            "reading" | "done" | "todo" => status,
            "" => "todo".to_string(),
            other => return Err(ServerFnError::Args(format!("status must be reading/done/todo, got '{other}'"))),
        };
        let st: ep_core::AppState = expect_context();
        let mut tx = st.db.begin().await.map_err(server_err)?;
        let doc_id = ep_core::next_doc_id(&mut tx, "LRN", ep_core::DocIdShape::TypeSerial4 { kind: "B" })
            .await.map_err(server_err)?;
        let progress = if status_norm == "done" { 1.0 } else { 0.0 };
        let author_opt = if author.trim().is_empty() { None } else { Some(author.clone()) };
        sqlx::query(
            "INSERT INTO lrn_book (doc_id, name, author, status, progress) VALUES (?1, ?2, ?3, ?4, ?5)"
        ).bind(&doc_id).bind(&name).bind(&author_opt).bind(&status_norm).bind(progress)
         .execute(&mut *tx).await.map_err(server_err)?;
        let occurred = time::OffsetDateTime::now_utc().unix_timestamp();
        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary, status) VALUES (?1, 'LRN', ?2, ?3, ?4)"
        ).bind(occurred).bind(&doc_id).bind(&name).bind(&status_norm)
         .execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(Book { doc_id, name, author: author_opt, status: status_norm, progress })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(CycleBookStatus, "/api/_internal/lrn", "Url", "cycle_book_status")]
pub async fn cycle_book_status(doc_id: String) -> Result<String, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        // status cycle: todo -> reading -> done -> todo
        // progress is bumped to mirror the next state:
        //   reading (was 'todo')      -> 0.5  (in-progress; arbitrary midpoint)
        //   done    (was 'reading')   -> 1.0
        //   todo    (was 'done')      -> 0.0
        // The CASE is keyed on the OLD status because SQLite evaluates both
        // SET expressions against the row before any updates land.
        let next: String = sqlx::query_scalar(
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
               RETURNING status"#
        ).bind(&doc_id).fetch_one(&st.db).await.map_err(server_err)?;
        Ok(next)
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(DeleteBook, "/api/_internal/lrn", "Url", "delete_book")]
pub async fn delete_book(doc_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        let mut tx = st.db.begin().await.map_err(server_err)?;
        sqlx::query("DELETE FROM lrn_book WHERE doc_id = ?1")
            .bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        // add_book wrote a matching `activity` row; clear it so Today /
        // Activity Journal don't keep dangling references after the book
        // is gone (mirrors finance::delete_txn / fitness::delete_workout).
        sqlx::query("DELETE FROM activity WHERE module = 'LRN' AND doc_id = ?1")
            .bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(AddNote, "/api/_internal/lrn", "Url", "add_note")]
pub async fn add_note(title: String, body: String) -> Result<Note, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        if title.trim().is_empty() { return Err(ServerFnError::Args("title is required".into())); }
        let st: ep_core::AppState = expect_context();
        let mut tx = st.db.begin().await.map_err(server_err)?;
        let doc_id = ep_core::next_doc_id(&mut tx, "LRN", ep_core::DocIdShape::TypeSerial4 { kind: "N" })
            .await.map_err(server_err)?;
        let updated_at = time::OffsetDateTime::now_utc().unix_timestamp();
        let body_opt = if body.trim().is_empty() { None } else { Some(body.clone()) };
        sqlx::query(
            "INSERT INTO lrn_note (doc_id, title, body, updated_at) VALUES (?1, ?2, ?3, ?4)"
        ).bind(&doc_id).bind(&title).bind(&body_opt).bind(updated_at)
         .execute(&mut *tx).await.map_err(server_err)?;
        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary) VALUES (?1, 'LRN', ?2, ?3)"
        ).bind(updated_at).bind(&doc_id).bind(&title)
         .execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(Note { doc_id, title, body: body_opt, updated_at })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(DeleteNote, "/api/_internal/lrn", "Url", "delete_note")]
pub async fn delete_note(doc_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        let mut tx = st.db.begin().await.map_err(server_err)?;
        sqlx::query("DELETE FROM lrn_note WHERE doc_id = ?1")
            .bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        sqlx::query("DELETE FROM activity WHERE module = 'LRN' AND doc_id = ?1")
            .bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}
