use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use ep_core::{AppState, NotifyMessage};
use futures_util::stream::Stream;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

pub async fn notifications_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.notify.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| {
        let m: NotifyMessage = res.ok()?;
        let payload = serde_json::to_string(&m).ok()?;
        Some(Ok(Event::default().data(payload)))
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}
