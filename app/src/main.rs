#![cfg(feature = "ssr")]

mod app;
mod views;
mod login;
mod sse;

use axum::{routing::{get, post}, Router};
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
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into()))
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
    let open_api_routes = ep_api::router(state.clone(), &registry)
        .layer(axum::middleware::from_fn_with_state(state.clone(), ep_auth::pat::require_pat));

    // Web app routes — protected by cookie session middleware (white-list inside the layer).
    let provide_state_state = state.clone();
    let provide_state = move || {
        provide_context(provide_state_state.clone());
    };
    let leptos_options_for_shell = leptos_options.clone();

    let web_routes: Router<AppState> = Router::<AppState>::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/login", get(login::page).post(login::submit))
        .route("/logout", post(login::logout).get(login::logout))
        .route("/events/notifications", get(sse::notifications_stream))
        .nest_service(
            "/static",
            axum::routing::any(static_handler),
        )
        .nest_service(
            "/pkg",
            tower_http::services::ServeDir::new(format!("{}/pkg", leptos_options.site_root)),
        )
        .leptos_routes_with_context(
            &state,
            leptos_routes,
            move || provide_state(),
            {
                let opts = leptos_options_for_shell.clone();
                move || crate::app::shell(opts.clone())
            },
        )
        .layer(axum::middleware::from_fn_with_state(state.clone(), ep_auth::middleware::require_session));

    let router: Router = Router::<AppState>::new()
        .nest("/api/v1", open_api_routes)
        .merge(web_routes)
        .layer(ServiceBuilder::new()
            .layer(CompressionLayer::new())
            .layer(TraceLayer::new_for_http()))
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
    let ctrl_c = async { let _ = signal::ctrl_c().await; };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = signal::unix::signal(signal::unix::SignalKind::terminate()) {
            let _ = s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term: std::future::Pending<()> = std::future::pending();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutdown signal received");
}

async fn ep_secret_or_create() -> anyhow::Result<String> {
    if let Ok(s) = std::env::var("EP_SECRET") {
        if s.len() >= 64 { return Ok(s); }
        anyhow::bail!("EP_SECRET must be at least 64 characters");
    }
    let path = std::path::PathBuf::from("data/secret.key");
    if let Ok(b) = tokio::fs::read(&path).await {
        if b.len() >= 64 {
            return Ok(String::from_utf8_lossy(&b).into_owned());
        }
    }
    use rand::RngCore;
    let mut buf = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut buf);
    let s = hex::encode(buf);
    if let Some(parent) = path.parent() { tokio::fs::create_dir_all(parent).await.ok(); }
    let _ = tokio::fs::write(&path, s.as_bytes()).await;
    tracing::warn!("EP_SECRET not set; generated and persisted to {:?}", path);
    Ok(s)
}

async fn static_handler(uri: axum::http::Uri) -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;
    let path = uri.path().trim_start_matches('/');
    match Assets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref().to_string()),
                 (header::CACHE_CONTROL, "public, max-age=86400".into())],
                Body::from(file.data.into_owned()),
            ).into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
