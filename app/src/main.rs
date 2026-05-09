#![cfg(feature = "ssr")]

mod app;
mod login;
mod sse;
mod views;

use axum::{
    routing::{get, post},
    Router,
};
use ep_core::{AppState, ModuleRegistry};
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

#[derive(rust_embed::RustEmbed)]
#[folder = "../assets/"]
struct Assets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .init();

    // Leptos config (reads LEPTOS_* env vars; cargo-leptos sets them in dev/release).
    let conf = leptos::config::get_configuration(None)?;
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;

    // DB pool + global migrations.
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/eigenpulse.db?mode=rwc".into());
    let db = ep_db::open_pool(&db_url).await?;

    // Cookie key.
    let secret = ep_secret_or_create().await?;

    // First-boot admin.
    ep_auth::bootstrap_admin(&db).await?;

    // NotifyBus.
    let notify = Arc::new(ep_notify::NotifyBus::new(db.clone()));

    // Module registry + per-module migrations.
    let registry = ModuleRegistry::new()
        .with(ep_finance::MODULE)
        .with(ep_fitness::MODULE)
        .with(ep_learning::MODULE)
        .with(ep_marketplace::MODULE);
    registry.run_migrations(&db).await?;

    let state = AppState {
        db,
        cookie_key: cookie::Key::from(secret.as_bytes()),
        notify: notify.clone(),
        leptos_options: leptos_options.clone(),
    };

    // Leptos routes from <App/>.
    let leptos_routes = generate_route_list(crate::app::App);

    // /api/v1/*  — protected by PAT middleware.
    let open_api_routes = ep_api::router(state.clone(), &registry).layer(
        axum::middleware::from_fn_with_state(state.clone(), ep_auth::require_pat),
    );

    // Web app routes — protected by cookie session middleware (white-list inside the layer).
    // The per-request `provide_state` closure pulls the locale that
    // `ep_i18n::locale_layer` stuck in request extensions and propagates
    // it to leptos context, so `crate::app::shell()` can write `<html lang>`
    // and views can call `t!(use_locale(), ...)` against the right table.
    let provide_state_state = state.clone();
    let provide_state = move || {
        provide_context(provide_state_state.clone());
        // Reads request `Parts` from leptos context (provided by
        // `leptos_axum::handle_response_inner` just before this closure
        // runs) and forwards the resolved `Locale` into leptos context.
        let _ = ep_i18n::provide_locale_from_request_parts();
    };
    let leptos_options_for_shell = leptos_options.clone();

    let web_routes: Router<AppState> = Router::<AppState>::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/login", get(login::page).post(login::submit))
        .route("/logout", post(login::logout))
        .route("/events/notifications", get(sse::notifications_stream))
        .route("/favicon.svg", get(static_handler))
        .route("/manifest.webmanifest", get(static_handler))
        .route("/sw.js", get(static_handler))
        .route("/theme-init.js", get(static_handler))
        .nest_service("/static", axum::routing::any(static_handler))
        .nest_service(
            "/pkg",
            tower_http::services::ServeDir::new(format!("{}/pkg", leptos_options.site_root)),
        )
        .leptos_routes_with_context(&state, leptos_routes, provide_state, {
            let opts = leptos_options_for_shell.clone();
            move || crate::app::shell(opts.clone())
        })
        .layer(axum::middleware::from_fn(ep_i18n::locale_layer))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            ep_auth::require_session,
        ));

    let router: Router = Router::<AppState>::new()
        .nest("/api/v1", open_api_routes)
        .merge(web_routes)
        .layer(
            ServiceBuilder::new()
                .layer(CompressionLayer::new())
                .layer(TraceLayer::new_for_http()),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(?addr, "eigenpulse listening");
    axum::serve(listener, router.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                let _ = s.recv().await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let term: std::future::Pending<()> = std::future::pending();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutdown signal received");
}

async fn ep_secret_or_create() -> anyhow::Result<String> {
    if let Ok(s) = std::env::var("EP_SECRET") {
        if explicit_secret_value(&s).is_some() {
            return normalize_secret(&s, "EP_SECRET");
        }
    }
    let path = secret_file_path(std::env::var_os("EP_SECRET_FILE"));
    let source = path.display().to_string();
    if let Ok(b) = tokio::fs::read(&path).await {
        let stored = String::from_utf8(b).map_err(|e| {
            anyhow::anyhow!("{} must contain UTF-8 secret text: {e}", path.display())
        })?;
        match normalize_secret(&stored, &source) {
            Ok(secret) => return Ok(secret),
            Err(e) => {
                tracing::warn!(error = %e, "stored EP_SECRET is invalid; rotating generated secret")
            }
        }
    }

    let mut buf = [0u8; 64];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut buf);
    let s = hex::encode(buf);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, s.as_bytes()).await?;
    tracing::warn!("EP_SECRET not set; generated and persisted to {:?}", path);
    Ok(s)
}

fn explicit_secret_value(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}

fn secret_file_path(explicit: Option<std::ffi::OsString>) -> std::path::PathBuf {
    explicit
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("data/secret.key"))
}

fn normalize_secret(value: &str, source: &str) -> anyhow::Result<String> {
    let trimmed = value.trim();
    if trimmed.len() < 64 {
        anyhow::bail!("{source} must be at least 64 characters");
    }
    Ok(trimmed.to_string())
}

fn static_cache_control(path: &str) -> &'static str {
    match path {
        "sw.js" | "static/sw.js" | "theme-init.js" | "static/theme-init.js" => "no-cache",
        _ => "public, max-age=86400",
    }
}

fn service_worker_allowed(path: &str) -> Option<&'static str> {
    matches!(path, "sw.js" | "static/sw.js").then_some("/")
}

async fn static_handler(uri: axum::http::Uri) -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{header, HeaderName, HeaderValue, StatusCode};
    use axum::response::IntoResponse;
    let path = uri
        .path()
        .trim_start_matches('/')
        .strip_prefix("static/")
        .unwrap_or_else(|| uri.path().trim_start_matches('/'));
    match Assets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let cache_control = static_cache_control(path);
            let mut response = (
                [
                    (header::CONTENT_TYPE, mime.as_ref().to_string()),
                    (header::CACHE_CONTROL, cache_control.into()),
                ],
                Body::from(file.data.into_owned()),
            )
                .into_response();
            if let Some(scope) = service_worker_allowed(path) {
                response.headers_mut().insert(
                    HeaderName::from_static("service-worker-allowed"),
                    HeaderValue::from_static(scope),
                );
            }
            response
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_secret, secret_file_path, service_worker_allowed, static_cache_control,
        static_handler,
    };
    use axum::http::{header, StatusCode, Uri};

    #[test]
    fn secret_file_path_defaults_to_local_data_dir() {
        assert_eq!(
            secret_file_path(None),
            std::path::PathBuf::from("data/secret.key")
        );
    }

    #[test]
    fn secret_file_path_honors_explicit_env_path() {
        assert_eq!(
            secret_file_path(Some("/data/secret.key".into())),
            std::path::PathBuf::from("/data/secret.key")
        );
    }

    #[test]
    fn normalize_secret_trims_trailing_newline() {
        let input = format!("{}\n", "a".repeat(64));
        assert_eq!(normalize_secret(&input, "test").unwrap(), "a".repeat(64));
    }

    #[test]
    fn normalize_secret_rejects_short_values() {
        let err = normalize_secret("short", "test").expect_err("short secret should fail");
        assert!(err.to_string().contains("at least 64"));
    }

    #[test]
    fn normalize_secret_rejects_blank_values() {
        let err = normalize_secret("  ", "test").expect_err("blank secret should fail");
        assert!(err.to_string().contains("at least 64"));
    }

    #[test]
    fn explicit_secret_value_treats_blank_env_as_unset() {
        assert_eq!(super::explicit_secret_value(""), None);
        assert_eq!(super::explicit_secret_value(" \n\t "), None);
        let secret = "a".repeat(64);
        assert_eq!(super::explicit_secret_value(&secret), Some(secret.as_str()));
    }

    #[test]
    fn static_cache_control_keeps_bootstrap_assets_revalidating() {
        assert_eq!(static_cache_control("sw.js"), "no-cache");
        assert_eq!(static_cache_control("static/sw.js"), "no-cache");
        assert_eq!(static_cache_control("theme-init.js"), "no-cache");
        assert_eq!(static_cache_control("static/theme-init.js"), "no-cache");
        assert_eq!(static_cache_control("styles.css"), "public, max-age=86400");
    }

    #[test]
    fn service_worker_can_claim_root_scope() {
        assert_eq!(service_worker_allowed("sw.js"), Some("/"));
        assert_eq!(service_worker_allowed("static/sw.js"), Some("/"));
        assert_eq!(service_worker_allowed("styles.css"), None);
    }

    #[tokio::test]
    async fn static_handler_accepts_mounted_and_full_static_paths() {
        for raw in ["/styles.css", "/static/styles.css"] {
            let response = static_handler(raw.parse::<Uri>().unwrap()).await;
            assert_eq!(response.status(), StatusCode::OK, "raw={raw}");
        }

        let missing = static_handler("/static/missing.css".parse::<Uri>().unwrap()).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn static_handler_rejects_traversal_shaped_paths() {
        for raw in ["/static/../Cargo.toml", "/static/%2e%2e/Cargo.toml"] {
            let response = static_handler(raw.parse::<Uri>().unwrap()).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "raw={raw}");
        }
    }

    #[tokio::test]
    async fn static_handler_serves_pwa_assets() {
        for raw in [
            "/favicon.svg",
            "/manifest.webmanifest",
            "/sw.js",
            "/theme-init.js",
            "/static/favicon.svg",
            "/static/manifest.webmanifest",
            "/static/icons/icon-192.svg",
            "/static/icons/icon-512.svg",
            "/static/icons/maskable.svg",
            "/static/sw.js",
            "/static/theme-init.js",
        ] {
            let response = static_handler(raw.parse::<Uri>().unwrap()).await;
            assert_eq!(response.status(), StatusCode::OK, "raw={raw}");
        }
    }

    #[tokio::test]
    async fn static_handler_sets_service_worker_scope_header() {
        let response = static_handler("/static/sw.js".parse::<Uri>().unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("service-worker-allowed")
                .and_then(|v| v.to_str().ok()),
            Some("/")
        );
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-cache")
        );
    }
}
