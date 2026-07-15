use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::Response;
use axum_extra::extract::cookie::SignedCookieJar;
use ep_core::{AppState, NotifyEvent};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::{BroadcastStream, IntervalStream};
use tokio_stream::{Stream, StreamExt};

enum SseInput {
    Notification(NotifyEvent),
    Lagged(u64),
    Revalidate,
}

enum StreamStep {
    Event(Event),
    Skip,
    Stop,
}

pub async fn notifications_stream(
    State(state): State<AppState>,
    jar: SignedCookieJar,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, Response> {
    let token = jar.get(ep_auth::COOKIE_NAME).map(|c| c.value().to_string());
    let Some(token) = token else {
        return Err(ep_auth::unauthorized("missing or invalid session"));
    };
    match ep_auth::session_is_valid(&state.db, &token).await {
        Ok(true) => {}
        _ => return Err(ep_auth::unauthorized("missing or invalid session")),
    }

    let rx = state.notify.subscribe();
    let notifications = BroadcastStream::new(rx).map(|result| match result {
        Ok(event) => SseInput::Notification(event),
        Err(BroadcastStreamRecvError::Lagged(skipped)) => SseInput::Lagged(skipped),
    });
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // The handshake above already validated the session; avoid an immediate
    // duplicate query from tokio's first interval tick.
    interval.tick().await;
    let revalidation = IntervalStream::new(interval).map(|_| SseInput::Revalidate);
    let pool = state.db.clone();
    let stream_token = token.clone();
    let stream = notifications
        .merge(revalidation)
        .then(move |input| {
            let pool = pool.clone();
            let token = stream_token.clone();
            async move { validated_step(&pool, &token, input).await }
        })
        .take_while(|step| !matches!(step, StreamStep::Stop))
        .filter_map(|step| match step {
            StreamStep::Event(event) => Some(Ok(event)),
            StreamStep::Skip | StreamStep::Stop => None,
        });
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

async fn validated_step(pool: &sqlx::SqlitePool, token: &str, input: SseInput) -> StreamStep {
    match ep_auth::session_is_valid(pool, token).await {
        Ok(true) => {}
        Ok(false) => return StreamStep::Stop,
        Err(error) => {
            tracing::warn!(%error, "SSE session revalidation failed; closing stream");
            return StreamStep::Stop;
        }
    }

    match input {
        SseInput::Notification(event) => match serde_json::to_string(&event.message) {
            Ok(payload) => {
                StreamStep::Event(Event::default().id(event.id.to_string()).data(payload))
            }
            Err(error) => {
                tracing::warn!(%error, "failed to serialize SSE notification");
                StreamStep::Skip
            }
        },
        SseInput::Lagged(skipped) => StreamStep::Event(
            Event::default()
                .event("resync")
                .data(format!(r#"{{"skipped":{skipped}}}"#)),
        ),
        SseInput::Revalidate => StreamStep::Skip,
    }
}

#[cfg(test)]
mod tests {
    use super::{validated_step, SseInput, StreamStep};

    async fn fixture() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("CREATE TABLE app_user(id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE session(
                token TEXT PRIMARY KEY,
                user_id INTEGER NOT NULL,
                issued_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                last_seen INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO app_user VALUES (1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO session VALUES ('stream', 1, 0, ?1, 0)")
            .bind(ep_core::unix_now() + 3_600)
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn every_step_observes_session_revocation() {
        let pool = fixture().await;
        assert!(matches!(
            validated_step(&pool, "stream", SseInput::Revalidate).await,
            StreamStep::Skip
        ));
        sqlx::query("DELETE FROM session WHERE token = 'stream'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            validated_step(&pool, "stream", SseInput::Lagged(3)).await,
            StreamStep::Stop
        ));
    }

    #[tokio::test]
    async fn lag_emits_resync_for_a_valid_session() {
        let pool = fixture().await;
        assert!(matches!(
            validated_step(&pool, "stream", SseInput::Lagged(7)).await,
            StreamStep::Event(_)
        ));
    }
}
