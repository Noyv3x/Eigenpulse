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
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

#[derive(rust_embed::RustEmbed)]
#[folder = "../assets/"]
struct Assets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().any(|arg| arg == "--healthcheck") {
        if let Err(e) = run_healthcheck() {
            eprintln!("healthcheck failed: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }

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
            // The hydration wasm filename is chosen by Leptos's
            // `<HydrationScripts/>` (the `wasm_output_name` it hands the
            // wasm-bindgen loader), and it has differed across versions: the
            // current one loads `<output-name>.wasm` (exactly what cargo-leptos
            // 0.3.6 publishes), while older Leptos — and wasm-bindgen's own
            // default path — use `<output-name>_bg.wasm`. A postbuild copy step
            // used to bridge that, but any build that skipped it (every
            // `cargo leptos watch` recompile) could then silently degrade pages
            // to their SSR snapshot. Serving the real `.wasm` whenever the
            // `_bg.wasm` name 404s makes the bundle resolve under either naming
            // with no postbuild copy: `fallback` keeps ServeFile's `200` +
            // `application/wasm`, and a genuine `_bg.wasm` is still served first
            // if one exists.
            tower_http::services::ServeDir::new(format!("{}/pkg", leptos_options.site_root))
                .fallback(tower_http::services::ServeFile::new(format!(
                    "{}/pkg/{}.wasm",
                    leptos_options.site_root, leptos_options.output_name
                ))),
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

fn run_healthcheck() -> anyhow::Result<()> {
    let addr = healthcheck_addr(std::env::var("LEPTOS_SITE_ADDR").ok().as_deref());
    let mut last_err = None;
    for socket in addr.to_socket_addrs()? {
        match TcpStream::connect_timeout(&socket, Duration::from_secs(2)) {
            Ok(mut stream) => {
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                stream.write_all(
                    b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                )?;
                let mut buf = [0u8; 128];
                let n = stream.read(&mut buf)?;
                let status = std::str::from_utf8(&buf[..n]).unwrap_or_default();
                if status.starts_with("HTTP/1.1 200") || status.starts_with("HTTP/1.0 200") {
                    return Ok(());
                }
                anyhow::bail!("unexpected /healthz response: {status:?}");
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(match last_err {
        Some(e) => anyhow::anyhow!("failed to connect to {addr}: {e}"),
        None => anyhow::anyhow!("failed to resolve healthcheck address {addr}"),
    })
}

fn healthcheck_addr(raw: Option<&str>) -> String {
    let raw = raw.unwrap_or("127.0.0.1:3000").trim();
    if let Some(port) = raw.strip_prefix("0.0.0.0:") {
        return format!("127.0.0.1:{port}");
    }
    if let Some(port) = raw.strip_prefix("[::]:") {
        return format!("127.0.0.1:{port}");
    }
    if raw.is_empty() {
        "127.0.0.1:3000".to_string()
    } else {
        raw.to_string()
    }
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
    if let Some(secret) = read_stored_secret(&path).await? {
        return Ok(secret);
    }

    let mut buf = [0u8; 64];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut buf);
    let s = hex::encode(buf);
    write_secret_file(&path, &s).await?;
    tracing::warn!("EP_SECRET not set; generated and persisted to {:?}", path);
    Ok(s)
}

async fn write_secret_file(path: &std::path::Path, secret: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, secret.as_bytes()).await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let perms = std::fs::Permissions::from_mode(0o600);
        tokio::fs::set_permissions(path, perms).await?;
    }
    Ok(())
}

async fn read_stored_secret(path: &std::path::Path) -> anyhow::Result<Option<String>> {
    let source = path.display().to_string();
    match tokio::fs::read(path).await {
        Ok(b) => {
            let stored = String::from_utf8(b).map_err(|e| {
                anyhow::anyhow!("{} must contain UTF-8 secret text: {e}", path.display())
            })?;
            normalize_secret(&stored, &source).map(Some)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::anyhow!(
            "failed to read EP_SECRET_FILE {}: {e}",
            path.display()
        )),
    }
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
        "sw.js"
        | "static/sw.js"
        | "theme-init.js"
        | "static/theme-init.js"
        | "styles.css"
        | "static/styles.css" => "no-cache",
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
        normalize_secret, read_stored_secret, secret_file_path, service_worker_allowed,
        static_cache_control, static_handler,
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
    fn healthcheck_addr_targets_loopback_for_bind_all_addresses() {
        assert_eq!(super::healthcheck_addr(None), "127.0.0.1:3000");
        assert_eq!(super::healthcheck_addr(Some("")), "127.0.0.1:3000");
        assert_eq!(
            super::healthcheck_addr(Some("0.0.0.0:3000")),
            "127.0.0.1:3000"
        );
        assert_eq!(super::healthcheck_addr(Some("[::]:3000")), "127.0.0.1:3000");
        assert_eq!(
            super::healthcheck_addr(Some("127.0.0.1:4000")),
            "127.0.0.1:4000"
        );
    }

    #[tokio::test]
    async fn read_stored_secret_treats_missing_file_as_unset() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing-secret.key");
        assert_eq!(read_stored_secret(&path).await.expect("read secret"), None);
    }

    #[tokio::test]
    async fn read_stored_secret_loads_and_normalizes_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret.key");
        let secret = "b".repeat(64);
        tokio::fs::write(&path, format!("{secret}\n"))
            .await
            .expect("write secret");

        assert_eq!(
            read_stored_secret(&path).await.expect("read secret"),
            Some(secret)
        );
    }

    #[tokio::test]
    async fn read_stored_secret_rejects_malformed_existing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret.key");
        tokio::fs::write(&path, "short")
            .await
            .expect("write secret");

        let err = read_stored_secret(&path)
            .await
            .expect_err("malformed stored secret should fail");
        assert!(err.to_string().contains("at least 64"));
    }

    #[tokio::test]
    async fn write_secret_file_creates_parent_dirs_and_writes_secret() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("secret.key");
        let secret = "c".repeat(64);

        super::write_secret_file(&path, &secret)
            .await
            .expect("write secret");

        let stored = tokio::fs::read_to_string(&path).await.expect("read secret");
        assert_eq!(stored, secret);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn write_secret_file_sets_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret.key");
        super::write_secret_file(&path, &"d".repeat(64))
            .await
            .expect("write secret");

        let mode = tokio::fs::metadata(&path)
            .await
            .expect("secret metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn static_cache_control_keeps_bootstrap_assets_revalidating() {
        assert_eq!(static_cache_control("sw.js"), "no-cache");
        assert_eq!(static_cache_control("static/sw.js"), "no-cache");
        assert_eq!(static_cache_control("theme-init.js"), "no-cache");
        assert_eq!(static_cache_control("static/theme-init.js"), "no-cache");
        assert_eq!(static_cache_control("styles.css"), "no-cache");
        assert_eq!(static_cache_control("static/styles.css"), "no-cache");
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

    #[tokio::test]
    async fn static_handler_serves_css_as_revalidating_asset() {
        let response = static_handler("/static/styles.css".parse::<Uri>().unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("no-cache")
        );
    }

    #[tokio::test]
    async fn service_worker_fetches_css_network_first() {
        let response = static_handler("/static/sw.js".parse::<Uri>().unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("read sw body");
        let body = std::str::from_utf8(&body).expect("sw body should be utf8");
        assert!(!body.contains("'/static/styles.css',"));
        assert!(body.contains("url.pathname === '/static/styles.css'"));
        assert!(body.contains("fetch(req).then((res)"));
    }
}
