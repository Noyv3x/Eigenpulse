use crate::model::*;
use ep_core::server_err;
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;

#[server(LoadFitness, "/api/_internal/fit", "Url", "load_fitness")]
pub async fn load_fitness() -> Result<Vec<Workout>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        type Row = (String, i64, String, Option<String>, i64, Option<String>, Option<String>, Option<i64>, Option<String>);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT doc_id, occurred_at, kind, program, duration_m, load_text, strain, rpe, notes
               FROM fit_workout ORDER BY occurred_at DESC LIMIT 30"
        ).fetch_all(&st.db).await.map_err(server_err)?;
        Ok(rows.into_iter().map(|r| Workout {
            doc_id: r.0, occurred_at: r.1, kind: r.2, program: r.3,
            duration_m: r.4, load_text: r.5, strain: r.6, rpe: r.7, notes: r.8,
        }).collect())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(AddWorkout, "/api/_internal/fit", "Url", "add_workout")]
pub async fn add_workout(
    kind: String,
    program: String,
    duration_m: i64,
    load_text: String,
    strain: String,
) -> Result<Workout, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        if kind.trim().is_empty() {
            return Err(ServerFnError::Args("kind is required".into()));
        }
        if duration_m <= 0 {
            return Err(ServerFnError::Args("duration must be positive".into()));
        }
        let strain_norm = match strain.as_str() {
            "L" | "M" | "H" => strain.clone(),
            "" => "M".to_string(),
            other => return Err(ServerFnError::Args(format!("strain must be L/M/H, got '{other}'"))),
        };
        let st: ep_core::AppState = expect_context();
        let occurred = time::OffsetDateTime::now_utc().unix_timestamp();
        let mut tx = st.db.begin().await.map_err(server_err)?;
        let doc_id = ep_core::next_doc_id(&mut tx, "FIT", ep_core::DocIdShape::TypeSerial4 { kind: "S" })
            .await.map_err(server_err)?;
        let program_opt = if program.trim().is_empty() { None } else { Some(program.clone()) };
        let load_opt = if load_text.trim().is_empty() { None } else { Some(load_text.clone()) };
        sqlx::query(
            "INSERT INTO fit_workout (doc_id, occurred_at, kind, program, duration_m, load_text, strain)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        )
        .bind(&doc_id).bind(occurred).bind(&kind)
        .bind(&program_opt).bind(duration_m).bind(&load_opt).bind(&strain_norm)
        .execute(&mut *tx).await.map_err(server_err)?;
        sqlx::query(
            "INSERT INTO activity (occurred_at, module, doc_id, summary, status)
             VALUES (?1, 'FIT', ?2, ?3, ?4)"
        )
        .bind(occurred).bind(&doc_id).bind(&kind).bind(&strain_norm)
        .execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(Workout {
            doc_id, occurred_at: occurred, kind, program: program_opt,
            duration_m, load_text: load_opt, strain: Some(strain_norm),
            rpe: None, notes: None,
        })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}

#[server(DeleteWorkout, "/api/_internal/fit", "Url", "delete_workout")]
pub async fn delete_workout(doc_id: String) -> Result<(), ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let st: ep_core::AppState = expect_context();
        let mut tx = st.db.begin().await.map_err(server_err)?;
        sqlx::query("DELETE FROM fit_workout WHERE doc_id = ?1")
            .bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        sqlx::query("DELETE FROM activity WHERE module = 'FIT' AND doc_id = ?1")
            .bind(&doc_id).execute(&mut *tx).await.map_err(server_err)?;
        tx.commit().await.map_err(server_err)?;
        Ok(())
    }
    #[cfg(not(feature = "ssr"))]
    { Err(server_err("ssr-only")) }
}
