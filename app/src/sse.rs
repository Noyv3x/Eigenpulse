use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::Response;
use axum_extra::extract::cookie::SignedCookieJar;
use ep_core::{AppState, NotifyMessage};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

pub async fn notifications_stream(
    State(state): State<AppState>,
    jar: SignedCookieJar,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, Response> {
    let token = jar.get(ep_auth::COOKIE_NAME).map(|c| c.value().to_string());
    let Some(token) = token else {
        return Err(ep_auth::unauthorized("missing or invalid session"));
    };
    match ep_auth::lookup_session(&state.db, &token).await {
        Ok(Some(_)) => {}
        _ => return Err(ep_auth::unauthorized("missing or invalid session")),
    }

    let rx = state.notify.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| {
        let m: NotifyMessage = res.ok()?;
        let payload = serde_json::to_string(&m).ok()?;
        Some(Ok(Event::default().data(payload)))
    });
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}
