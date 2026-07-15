#![cfg(feature = "ssr")]
#![recursion_limit = "256"]

mod admin;
mod app;
mod login;
mod modules;
mod security;
mod sse;
mod views;

use anyhow::Context as _;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use ep_core::AppState;
use leptos::prelude::*;
use leptos_axum::{generate_route_list, LeptosRoutes};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

#[derive(rust_embed::RustEmbed)]
#[folder = "../assets/"]
struct Assets;

/// Login credentials and their CSRF/return-path fields are tiny. Keep a strict
/// limit so an unauthenticated client cannot make `Form<LoginInput>` buffer a
/// large body before the CSRF and rate-limit checks run.
const LOGIN_BODY_LIMIT_BYTES: usize = 16 * 1024;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = startup_command(std::env::args_os().skip(1))?;
    if matches!(&command, StartupCommand::Healthcheck) {
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

    if let StartupCommand::Restore(archive) = command {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://data/eigenpulse.db?mode=rwc".into());
        let database = ep_db::database_path_from_url(&db_url).ok_or_else(|| {
            anyhow::anyhow!("--restore requires a file-backed SQLite DATABASE_URL")
        })?;
        let media = ep_core::module_data_root().join("fitness/media/objects");
        let manifest = ep_db::backup::restore_epbackup_offline(
            &archive,
            &database,
            &media,
            ep_db::backup::RestoreLimits::default(),
        )
        .await
        .with_context(|| format!("restore {}", archive.display()))?;
        tracing::info!(
            archive = %archive.display(),
            database = %database.display(),
            media_files = manifest.media.len(),
            "portable backup restored; all browser sessions were invalidated"
        );
        return Ok(());
    }

    let trusted_proxies = ep_auth::init_trusted_proxies_from_env()?;
    if !trusted_proxies.is_empty() {
        tracing::info!("trusted reverse-proxy CIDRs configured");
    }

    // Leptos config (reads LEPTOS_* env vars; cargo-leptos sets them in dev/release).
    let conf = leptos::config::get_configuration(None)?;
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;

    // DB pool + global migrations.
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/eigenpulse.db?mode=rwc".into());
    let _database_lock = ep_db::database_path_from_url(&db_url)
        .as_deref()
        .map(ep_db::acquire_database_lock)
        .transpose()?;
    let db = ep_db::open_pool(&db_url).await?;

    // Cookie key.
    let secret = ep_secret_or_create().await?;

    // First-boot admin.
    ep_auth::bootstrap_admin(&db).await?;
    let timezone = load_persisted_timezone(&db).await?;
    tracing::info!(timezone = timezone.name(), "application timezone loaded");

    // NotifyBus.
    let notify = Arc::new(ep_notify::NotifyBus::new(db.clone()));

    // Module registry + per-module migrations.
    let registry = crate::modules::registry();
    if registry.has_pending_migrations(&db).await? {
        match ep_db::pre_migration_snapshot(&db).await {
            Ok(Some(snapshot_path)) => tracing::info!(
                snapshot = %snapshot_path.display(),
                "pre-module-migration snapshot written"
            ),
            Ok(None) => {}
            Err(e) if ep_db::unbacked_migration_allowed() => tracing::warn!(
                error = %e,
                "pre-module-migration snapshot failed — emergency override allows migration without backup"
            ),
            Err(e) => return Err(e.context(
                "pre-module-migration snapshot failed; refusing to migrate without EP_ALLOW_UNBACKED_MIGRATION=1",
            )),
        }
    }
    registry.run_migrations(&db).await?;
    let notify_worker = notify.start_worker();

    let state = AppState {
        db,
        cookie_key: cookie::Key::from(secret.as_bytes()),
        notify: notify.clone(),
        leptos_options: leptos_options.clone(),
        timezone: ep_core::TimezoneStore::new(timezone),
    };

    // Periodic expired-session sweep. `lookup_session` already reaps the row it
    // touches, but sessions for users who never return would otherwise pile up.
    // The task is detached and runs every hour; it shuts down with the runtime
    // when the process exits (it holds a pool clone, no extra handshake needed).
    spawn_session_gc(state.db.clone());

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
        .route("/livez", get(|| async { "ok" }))
        .route("/readyz", get(readiness))
        .route(
            "/login",
            get(login::page)
                .post(login::submit)
                .layer(axum::extract::DefaultBodyLimit::max(LOGIN_BODY_LIMIT_BYTES)),
        )
        .route("/logout", post(login::logout))
        .route("/events/notifications", get(sse::notifications_stream))
        .route(
            "/api/_internal/admin/backup/latest",
            get(admin::download_latest),
        )
        // Dedicated handler so the SW cache key tracks CARGO_PKG_VERSION.
        .route("/sw.js", get(service_worker_handler))
        .nest_service("/static", axum::routing::any(static_handler))
        .nest_service(
            "/pkg",
            // cargo-leptos publishes the canonical `<output-name>.wasm` that
            // the document bootstrap references. Missing package resources stay
            // missing instead of being rewritten to the WASM payload.
            tower_http::services::ServeDir::new(format!("{}/pkg", leptos_options.site_root)),
        )
        // Module-owned browser routes remain behind the same cookie-session
        // boundary as their UIs: Finance downloads CSV, while Fitness streams
        // media without exposing its object directory.
        .merge(ep_finance::browser_router())
        .merge(ep_fitness::media_router())
        .leptos_routes_with_context(&state, leptos_routes, provide_state, {
            let opts = leptos_options_for_shell.clone();
            move || crate::app::shell(opts.clone())
        })
        .layer(axum::middleware::from_fn(ep_i18n::locale_layer))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            ep_auth::require_session,
        ))
        // Server functions and ordinary forms stay small. Fitness media upload
        // has its own streaming route and independent size enforcement.
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        // Security headers (CSP / nosniff / frame-deny / referrer / opt-in HSTS)
        // are applied to the browser-facing web app only — not the JSON
        // `/api/v1/*` group, whose responses are not rendered as HTML documents.
        // Outermost layer so it stamps every web response, including redirects
        // from the auth/login handlers. See `crate::security`.
        .layer(axum::middleware::from_fn(security::security_headers));

    let router: Router = Router::<AppState>::new()
        .nest("/api/v1", open_api_routes)
        .merge(web_routes)
        .layer(
            ServiceBuilder::new()
                .layer(
                    CompressionLayer::new().compress_when(
                        DefaultPredicate::new()
                            .and(NotForContentType::const_new("video/"))
                            .and(NotForContentType::const_new(
                                "application/vnd.eigenpulse.backup+zip",
                            )),
                    ),
                )
                .layer(TraceLayer::new_for_http()),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(?addr, "eigenpulse listening");
    // `into_make_service_with_connect_info` makes the peer `SocketAddr`
    // available as `ConnectInfo<SocketAddr>` so the login handler can key its
    // brute-force throttle on the client IP (falling back from X-Forwarded-For).
    let serve_result = axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await;
    notify_worker.shutdown().await;
    serve_result?;
    Ok(())
}

async fn readiness(State(state): State<AppState>) -> Response {
    if let Err(error) = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        tracing::warn!(%error, "readiness database check failed");
        return (StatusCode::SERVICE_UNAVAILABLE, "database unavailable").into_response();
    }

    let wasm = std::path::Path::new(state.leptos_options.site_root.as_ref())
        .join("pkg")
        .join(format!("{}.wasm", state.leptos_options.output_name));
    match tokio::fs::metadata(&wasm).await {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => "ok".into_response(),
        Ok(_) => {
            tracing::warn!(path = %wasm.display(), "readiness hydration asset is empty or not a file");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "hydration asset unavailable",
            )
                .into_response()
        }
        Err(error) => {
            tracing::warn!(%error, path = %wasm.display(), "readiness hydration asset check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "hydration asset unavailable",
            )
                .into_response()
        }
    }
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
                    b"GET /readyz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                )?;
                let mut buf = [0u8; 128];
                let n = stream.read(&mut buf)?;
                let status = std::str::from_utf8(&buf[..n]).unwrap_or_default();
                if status.starts_with("HTTP/1.1 200") || status.starts_with("HTTP/1.0 200") {
                    return Ok(());
                }
                anyhow::bail!("unexpected /readyz response: {status:?}");
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

async fn load_persisted_timezone(pool: &sqlx::SqlitePool) -> anyhow::Result<ep_core::AppTimezone> {
    let stored: String = sqlx::query_scalar("SELECT timezone FROM app_user WHERE id = 1")
        .fetch_one(pool)
        .await?;
    ep_core::AppTimezone::parse(&stored).ok_or_else(|| {
        anyhow::anyhow!("app_user.timezone contains an invalid IANA timezone: {stored:?}")
    })
}

#[derive(Debug, PartialEq, Eq)]
enum StartupCommand {
    Serve,
    Healthcheck,
    Restore(std::path::PathBuf),
}

fn startup_command(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> anyhow::Result<StartupCommand> {
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Ok(StartupCommand::Serve);
    };
    let command = match first.to_str() {
        Some("--healthcheck") => StartupCommand::Healthcheck,
        Some("--restore") => {
            let archive = args
                .next()
                .filter(|value| !value.is_empty())
                .map(std::path::PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("usage: eigenpulse --restore <file.epbackup>"))?;
            StartupCommand::Restore(archive)
        }
        Some(value) => anyhow::bail!("unknown argument: {value}"),
        None => anyhow::bail!("command-line arguments must be valid UTF-8"),
    };
    if args.next().is_some() {
        anyhow::bail!("too many command-line arguments");
    }
    Ok(command)
}

/// Hourly expired-session GC. Detached background task; logs the row count when
/// it removes anything. Errors are logged at `warn` and the loop continues.
fn spawn_session_gc(db: sqlx::SqlitePool) {
    const GC_INTERVAL: Duration = Duration::from_secs(60 * 60);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(GC_INTERVAL);
        // Skip the immediate first tick `interval` fires at t=0 so startup isn't
        // blocked by (or racing) the first sweep.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            match ep_auth::delete_expired_sessions(&db).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(removed = n, "session GC swept expired sessions"),
                Err(e) => tracing::warn!(error = %e, "session GC failed"),
            }
        }
    });
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
    if write_secret_file(&path, &s).await? {
        tracing::warn!("EP_SECRET not set; generated and persisted to {:?}", path);
        Ok(s)
    } else {
        read_stored_secret(&path)
            .await?
            .ok_or_else(|| anyhow::anyhow!("concurrent secret creation completed without a file"))
    }
}

/// Atomically publish a new secret without following or replacing an existing
/// path. Returns false when another process won the create race.
async fn write_secret_file(path: &std::path::Path, secret: &str) -> anyhow::Result<bool> {
    if let Some(parent) = path.parent() {
        let existed = tokio::fs::try_exists(parent).await?;
        tokio::fs::create_dir_all(parent).await?;
        if !existed || is_managed_secret_dir(parent) {
            set_secret_mode(parent, 0o700).await?;
        }
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("EP_SECRET_FILE must have a UTF-8 filename"))?;
    use rand::RngCore as _;
    let suffix = rand::thread_rng().next_u64();
    let temp = path.with_file_name(format!(
        ".{file_name}.tmp-{}-{suffix:016x}",
        std::process::id()
    ));
    create_secret_temp(&temp, secret.as_bytes()).await?;

    let published = match tokio::fs::hard_link(&temp, path).await {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(hard_link_error) => {
            // Some NAS filesystems reject hard links. The process already
            // holds DatabaseLock, so a same-directory rename is an atomic and
            // race-free fallback between Eigenpulse instances.
            if tokio::fs::try_exists(path).await? {
                false
            } else {
                tracing::debug!(
                    error = %hard_link_error,
                    path = %path.display(),
                    "hard-link secret publication unavailable; using atomic rename"
                );
                tokio::fs::rename(&temp, path).await?;
                true
            }
        }
    };
    if tokio::fs::try_exists(&temp).await? {
        tokio::fs::remove_file(&temp).await?;
    }
    if let Some(parent) = path.parent() {
        sync_secret_directory(parent).await?;
    }
    Ok(published)
}

async fn read_stored_secret(path: &std::path::Path) -> anyhow::Result<Option<String>> {
    let source = path.display().to_string();
    match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            anyhow::bail!("EP_SECRET_FILE must be a regular non-symlink file: {source}");
        }
        Ok(_) => set_secret_mode(path, 0o600).await?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
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

fn is_managed_secret_dir(path: &std::path::Path) -> bool {
    path == std::path::Path::new("/data")
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "data")
}

#[cfg(unix)]
async fn create_secret_temp(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt;
    let path = path.to_owned();
    let bytes = bytes.to_vec();
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(&bytes)?;
        file.sync_all()
    })
    .await??;
    Ok(())
}

#[cfg(not(unix))]
async fn create_secret_temp(path: &std::path::Path, bytes: &[u8]) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt as _;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    file.write_all(bytes).await?;
    file.sync_all().await?;
    Ok(())
}

#[cfg(unix)]
async fn set_secret_mode(path: &std::path::Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).await {
        Ok(()) => Ok(()),
        Err(error) if insecure_file_permissions_allowed() => {
            tracing::warn!(path = %path.display(), %error, "could not enforce private secret permissions; emergency override active");
            Ok(())
        }
        Err(error) => Err(error).with_context(|| format!(
            "could not set private permissions on {}; use EP_ALLOW_INSECURE_FILE_PERMISSIONS=1 only with equivalent filesystem ACLs",
            path.display()
        )),
    }
}

#[cfg(not(unix))]
async fn set_secret_mode(_path: &std::path::Path, _mode: u32) -> anyhow::Result<()> {
    Ok(())
}

fn insecure_file_permissions_allowed() -> bool {
    std::env::var("EP_ALLOW_INSECURE_FILE_PERMISSIONS")
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

#[cfg(unix)]
async fn sync_secret_directory(path: &std::path::Path) -> anyhow::Result<()> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || std::fs::File::open(path)?.sync_all()).await??;
    Ok(())
}

#[cfg(not(unix))]
async fn sync_secret_directory(_path: &std::path::Path) -> anyhow::Result<()> {
    Ok(())
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
        "sw.js" | "chart-loader.js" | "styles.css" | "vendor/eigenpulse-charts-6.1.0.js" => {
            "no-cache"
        }
        _ => "public, max-age=86400",
    }
}

/// Placeholder token in `assets/sw.js` that the handler swaps for the crate
/// version so the SW cache key is always `ep-<CARGO_PKG_VERSION>`.
const SW_VERSION_TOKEN: &str = "__EP_SW_VERSION__";

/// Render `sw.js` with its cache-version token substituted for the compile-time
/// `CARGO_PKG_VERSION`. Pure string op so it is unit-testable without HTTP.
fn render_sw_js(template: &str) -> String {
    template.replace(SW_VERSION_TOKEN, env!("CARGO_PKG_VERSION"))
}

/// Serve `/sw.js` with the cache version templated from `CARGO_PKG_VERSION`,
/// keeping the existing `no-cache` + root-scope semantics. The network-first
/// fetch logic in `sw.js` is untouched; only the `CACHE` constant is rewritten.
async fn service_worker_handler() -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{header, HeaderName, HeaderValue, StatusCode};
    use axum::response::IntoResponse;

    match Assets::get("sw.js") {
        Some(file) => {
            let raw = String::from_utf8_lossy(&file.data);
            let body = render_sw_js(&raw);
            let mut response = (
                [
                    (
                        header::CONTENT_TYPE,
                        "text/javascript; charset=utf-8".to_string(),
                    ),
                    (header::CACHE_CONTROL, static_cache_control("sw.js").into()),
                ],
                Body::from(body),
            )
                .into_response();
            response.headers_mut().insert(
                HeaderName::from_static("service-worker-allowed"),
                HeaderValue::from_static("/"),
            );
            response
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn static_handler(uri: axum::http::Uri) -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;
    // `nest_service("/static", ...)` strips the mount prefix before dispatch.
    let path = uri.path().trim_start_matches('/');
    // The service worker has one version-templated root route, while the
    // pre-paint theme script is inline-only. Do not expose raw aliases through
    // the generic embedded-asset handler.
    if matches!(path, "sw.js" | "theme-init.js") {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    match Assets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let cache_control = static_cache_control(path);
            (
                [
                    (header::CONTENT_TYPE, mime.as_ref().to_string()),
                    (header::CACHE_CONTROL, cache_control.into()),
                ],
                Body::from(file.data.into_owned()),
            )
                .into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        load_persisted_timezone, normalize_secret, read_stored_secret, render_sw_js,
        secret_file_path, service_worker_handler, startup_command, static_cache_control,
        static_handler, StartupCommand, SW_VERSION_TOKEN,
    };
    use axum::http::{header, StatusCode, Uri};

    #[test]
    fn render_sw_js_substitutes_crate_version() {
        let rendered = render_sw_js("const CACHE = 'ep-__EP_SW_VERSION__';");
        assert!(!rendered.contains(SW_VERSION_TOKEN));
        assert_eq!(
            rendered,
            format!("const CACHE = 'ep-{}';", env!("CARGO_PKG_VERSION"))
        );
    }

    #[tokio::test]
    async fn service_worker_handler_templates_version_and_sets_scope() {
        let response = service_worker_handler().await;
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
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("read sw body");
        let body = std::str::from_utf8(&body).expect("sw body utf8");
        assert!(
            !body.contains(SW_VERSION_TOKEN),
            "version token not substituted"
        );
        assert!(body.contains(&format!("ep-{}", env!("CARGO_PKG_VERSION"))));
        // Network-first logic must survive the substitution.
        assert!(body.contains("fetch(req).then((res)"));
    }

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

    #[test]
    fn startup_command_parses_offline_restore_strictly() {
        assert_eq!(
            startup_command(["--restore".into(), "/backup/data.epbackup".into()]).unwrap(),
            StartupCommand::Restore("/backup/data.epbackup".into())
        );
        assert!(startup_command(["--restore".into()]).is_err());
        assert!(startup_command(["--unknown".into()]).is_err());
        assert!(startup_command(["--healthcheck".into(), "extra".into()]).is_err());
    }

    async fn timezone_pool(stored: Option<&str>) -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::query(
            "CREATE TABLE app_user (\
                 id INTEGER PRIMARY KEY, \
                 timezone TEXT NOT NULL DEFAULT 'UTC' \
                     CHECK (length(timezone) BETWEEN 1 AND 64 AND timezone = trim(timezone)), \
                 timezone_mode TEXT NOT NULL DEFAULT 'auto' \
                     CHECK (timezone_mode IN ('auto', 'manual'))\
             )",
        )
        .execute(&pool)
        .await
        .expect("table");
        match stored {
            Some(stored) => {
                sqlx::query("INSERT INTO app_user (id, timezone) VALUES (1, ?1)")
                    .bind(stored)
                    .execute(&pool)
                    .await
                    .expect("owner");
            }
            None => {
                sqlx::query("INSERT INTO app_user (id) VALUES (1)")
                    .execute(&pool)
                    .await
                    .expect("owner");
            }
        }
        pool
    }

    #[tokio::test]
    async fn first_boot_uses_the_persisted_utc_baseline() {
        let pool = timezone_pool(None).await;

        let loaded = load_persisted_timezone(&pool).await.expect("load timezone");

        let stored: (String, String) =
            sqlx::query_as("SELECT timezone, timezone_mode FROM app_user WHERE id = 1")
                .fetch_one(&pool)
                .await
                .expect("stored timezone baseline");
        assert_eq!(loaded.name(), "UTC");
        assert_eq!(stored, ("UTC".into(), "auto".into()));
    }

    #[tokio::test]
    async fn persisted_timezone_is_authoritative() {
        let pool = timezone_pool(Some("America/New_York")).await;

        let loaded = load_persisted_timezone(&pool).await.expect("load timezone");

        assert_eq!(loaded.name(), "America/New_York");
    }

    #[tokio::test]
    async fn invalid_persisted_timezone_fails_closed() {
        let pool = timezone_pool(Some("Not/A_Timezone")).await;

        let error = load_persisted_timezone(&pool)
            .await
            .expect_err("invalid timezone must fail startup");

        assert!(error.to_string().contains("invalid IANA timezone"));
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

    #[cfg(unix)]
    #[tokio::test]
    async fn read_stored_secret_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("target");
        std::fs::write(&target, "s".repeat(64)).expect("target");
        let link = dir.path().join("secret.key");
        symlink(&target, &link).expect("symlink");

        let error = read_stored_secret(&link)
            .await
            .expect_err("secret symlink must be rejected");
        assert!(error.to_string().contains("non-symlink"));
    }

    #[tokio::test]
    async fn concurrent_secret_publish_has_exactly_one_winner() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("secret.key");
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let (first, second) = tokio::join!(
            super::write_secret_file(&path, &a),
            super::write_secret_file(&path, &b)
        );
        let first = first.expect("first");
        let second = second.expect("second");
        assert_ne!(first, second, "exactly one publisher must win");
        let stored = read_stored_secret(&path)
            .await
            .expect("read")
            .expect("stored");
        assert!(stored == "a".repeat(64) || stored == "b".repeat(64));
    }

    #[test]
    fn static_cache_control_keeps_bootstrap_assets_revalidating() {
        assert_eq!(static_cache_control("sw.js"), "no-cache");
        assert_eq!(static_cache_control("chart-loader.js"), "no-cache");
        assert_eq!(static_cache_control("styles.css"), "no-cache");
        assert_eq!(
            static_cache_control("vendor/eigenpulse-charts-6.1.0.js"),
            "no-cache"
        );
    }

    #[tokio::test]
    async fn static_handler_accepts_mounted_asset_paths() {
        let response = static_handler("/styles.css".parse::<Uri>().unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);

        let missing = static_handler("/missing.css".parse::<Uri>().unwrap()).await;
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn static_handler_rejects_traversal_shaped_paths() {
        for raw in ["/../Cargo.toml", "/%2e%2e/Cargo.toml"] {
            let response = static_handler(raw.parse::<Uri>().unwrap()).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "raw={raw}");
        }
    }

    #[tokio::test]
    async fn static_handler_serves_pwa_assets() {
        for raw in [
            "/favicon.svg",
            "/manifest.webmanifest",
            "/icons/icon-192.svg",
            "/icons/icon-512.svg",
            "/icons/maskable.svg",
            "/chart-loader.js",
            "/vendor/eigenpulse-charts-6.1.0.js",
            "/vendor/eigenpulse-charts-6.1.0.LICENSE.txt",
        ] {
            let response = static_handler(raw.parse::<Uri>().unwrap()).await;
            assert_eq!(response.status(), StatusCode::OK, "raw={raw}");
        }
    }

    #[tokio::test]
    async fn static_handler_rejects_noncanonical_bootstrap_assets() {
        for raw in ["/sw.js", "/theme-init.js"] {
            let response = static_handler(raw.parse::<Uri>().unwrap()).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "raw={raw}");
        }
    }

    #[tokio::test]
    async fn static_handler_serves_css_as_revalidating_asset() {
        let response = static_handler("/styles.css".parse::<Uri>().unwrap()).await;
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
        let response = service_worker_handler().await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("read sw body");
        let body = std::str::from_utf8(&body).expect("sw body should be utf8");
        assert!(!body.contains("'/static/styles.css',"));
        assert!(body.contains("url.pathname === '/static/styles.css'"));
        assert!(body.contains("fetch(req).then((res)"));
        assert!(body.contains("url.pathname === '/static/chart-loader.js'"));
        assert!(body.contains("url.pathname === '/static/vendor/eigenpulse-charts-6.1.0.js'"));
    }
}
