#[cfg(feature = "ssr")]
mod errors;
#[cfg(feature = "ssr")]
mod healthz;
#[cfg(feature = "ssr")]
mod notify;
#[cfg(feature = "ssr")]
mod today;
#[cfg(feature = "ssr")]
mod whoami;

#[cfg(feature = "ssr")]
use axum::{
    http::StatusCode,
    response::Response,
    routing::{get, post},
    Router,
};
#[cfg(feature = "ssr")]
use ep_core::{AppState, ModuleRegistry};

#[cfg(feature = "ssr")]
pub fn router(state: AppState, registry: &ModuleRegistry) -> Router<AppState> {
    Router::new()
        .route("/healthz", get(healthz::ok))
        .route("/whoami", get(whoami::handler))
        .route("/today", get(today::handler))
        .route("/notify", post(notify::handler))
        .merge(registry.open_api_router(state.clone()))
        .fallback(api_not_found)
        .method_not_allowed_fallback(api_method_not_allowed)
        // PAT middleware is attached by the binary to the whole `/api/v1/*` group.
        .with_state(state)
}

#[cfg(feature = "ssr")]
async fn api_not_found() -> Response {
    ep_core::api_error_response(StatusCode::NOT_FOUND, "not_found", "api route not found")
}

#[cfg(feature = "ssr")]
async fn api_method_not_allowed() -> Response {
    ep_core::api_error_response(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "method not allowed",
    )
}

#[cfg(all(test, feature = "ssr"))]
pub(crate) mod test_support {
    use ep_core::{AppState, NotifyMessage};
    use std::sync::Arc;

    pub(crate) struct NoopNotifyBus;

    #[async_trait::async_trait]
    impl ep_core::NotifyBusTrait for NoopNotifyBus {
        async fn dispatch(&self, _msg: NotifyMessage) -> anyhow::Result<i64> {
            Ok(0)
        }

        fn subscribe(&self) -> tokio::sync::broadcast::Receiver<NotifyMessage> {
            let (_tx, rx) = tokio::sync::broadcast::channel(1);
            rx
        }
    }

    pub(crate) fn app_state<T>(db: sqlx::SqlitePool, notify: Arc<T>) -> AppState
    where
        T: ep_core::NotifyBusTrait,
    {
        AppState {
            db,
            cookie_key: cookie::Key::generate(),
            notify,
            leptos_options: Default::default(),
        }
    }

    pub(crate) fn noop_notify() -> Arc<NoopNotifyBus> {
        Arc::new(NoopNotifyBus)
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::router;
    use crate::test_support::{app_state, noop_notify};
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use ep_core::ModuleRegistry;
    use tower::ServiceExt;

    async fn test_router() -> axum::Router {
        let db = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("pool");
        let state = app_state(db, noop_notify());
        router(state.clone(), &ModuleRegistry::new()).with_state(state)
    }

    #[tokio::test]
    async fn api_router_returns_json_404_for_unknown_paths() {
        let response = test_router()
            .await
            .oneshot(
                Request::builder()
                    .uri("/missing")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
            .await
            .expect("body");
        let err: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(
            err.pointer("/error/code").and_then(|v| v.as_str()),
            Some("not_found")
        );
    }

    #[tokio::test]
    async fn api_router_returns_json_405_for_known_path_wrong_method() {
        let response = test_router()
            .await
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        let body = axum::body::to_bytes(response.into_body(), 16 * 1024)
            .await
            .expect("body");
        let err: serde_json::Value = serde_json::from_slice(&body).expect("json");
        assert_eq!(
            err.pointer("/error/code").and_then(|v| v.as_str()),
            Some("method_not_allowed")
        );
    }
}
