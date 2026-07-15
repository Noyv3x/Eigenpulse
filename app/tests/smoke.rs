//! End-to-end smoke test against the SSR binary.
//!
//! Run with:
//! `cargo leptos build --release && cargo test --features ssr -p eigenpulse --test smoke --release --no-default-features --locked -- --nocapture`
//!
//! Requires: `cargo leptos build --release` has produced `target/site/`
//! beforehand (the binary needs the static site root). The test spawns the
//! binary on an ephemeral port, exercises the auth flow, and verifies
//! `/api/v1/*` PAT gating. The temp directory is removed on Drop.

#![cfg(feature = "ssr")]

use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use reqwest::header::{ACCEPT_LANGUAGE, COOKIE, SET_COOKIE};

const BIN: &str = env!("CARGO_BIN_EXE_eigenpulse");

struct Server {
    child: Child,
    base: String,
    db_url: String,
    _data_dir: tempfile::TempDir,
}

impl Server {
    fn start() -> Self {
        Self::start_with_trusted_proxy(None)
    }

    fn start_with_trusted_proxy(trusted_proxy_cidrs: Option<&str>) -> Self {
        let data_dir = tempfile::tempdir().expect("tempdir");
        let port = pick_free_port();
        let base = format!("http://127.0.0.1:{port}");
        let db_url = format!("sqlite://{}/test.db?mode=rwc", data_dir.path().display());
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let site_root = workspace_root.join("target").join("site");

        let mut command = Command::new(BIN);
        command
            .env("EP_ADMIN_PASSWORD", "test-pw")
            .env("EP_SECRET", "x".repeat(64))
            .env("EP_COOKIE_SECURE", "0")
            .env(
                "EP_MODULE_DATA_ROOT",
                data_dir.path().join("modules").display().to_string(),
            )
            // Debug SSR of the finance page builds a deep Leptos view tree.
            // Keep the smoke harness from failing with a tokio worker stack
            // overflow before it can verify the rendered response.
            .env("RUST_MIN_STACK", "16777216")
            .env("DATABASE_URL", &db_url)
            .env("LEPTOS_SITE_ADDR", format!("127.0.0.1:{port}"))
            .env("LEPTOS_SITE_ROOT", site_root.display().to_string())
            .env("LEPTOS_OUTPUT_NAME", "eigenpulse")
            .env("LEPTOS_SITE_PKG_DIR", "pkg")
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(cidrs) = trusted_proxy_cidrs {
            command.env("EP_TRUSTED_PROXY_CIDRS", cidrs);
        }
        let child = command.spawn().expect("spawn eigenpulse binary");

        let server = Self {
            child,
            base,
            db_url,
            _data_dir: data_dir,
        };
        server.wait_ready();
        server
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn healthcheck_addr(&self) -> String {
        self.base
            .strip_prefix("http://")
            .unwrap_or(&self.base)
            .to_string()
    }

    fn wait_ready(&self) {
        let plain = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .unwrap();
        for _ in 0..40 {
            if let Ok(r) = plain.get(self.url("/readyz")).send() {
                if r.status().is_success() {
                    return;
                }
            }
            sleep(Duration::from_millis(200));
        }
        panic!("server did not start within 8s");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn pick_free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn no_redirect_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap()
}

/// Extract the hidden CSRF token value from a login page body. The form embeds
/// `<input type="hidden" name="csrf" value="…"/>`.
fn extract_csrf_token(body: &str) -> String {
    let marker = r#"name="csrf" value=""#;
    let start = body
        .find(marker)
        .map(|i| i + marker.len())
        .expect("login page should embed a csrf hidden field");
    let rest = &body[start..];
    let end = rest.find('"').expect("csrf value should be quoted");
    rest[..end].to_string()
}

/// Perform the full login handshake against a CSRF-protected `/login`: GET to
/// mint the `ep_csrf` cookie + token, then POST password + token. The client's
/// cookie store carries the csrf cookie back automatically. Returns the response.
fn login_with_csrf(
    client: &reqwest::blocking::Client,
    server: &Server,
    password: &str,
    next: Option<&str>,
) -> reqwest::blocking::Response {
    login_with_csrf_from(client, server, password, next, None)
}

fn login_with_csrf_from(
    client: &reqwest::blocking::Client,
    server: &Server,
    password: &str,
    next: Option<&str>,
    forwarded_for: Option<&str>,
) -> reqwest::blocking::Response {
    let page = client.get(server.url("/login")).send().unwrap();
    let body = page.text().unwrap();
    let csrf = extract_csrf_token(&body);
    let mut form = vec![("password", password), ("csrf", csrf.as_str())];
    if let Some(next) = next {
        form.push(("next", next));
    }
    let mut request = client.post(server.url("/login")).form(&form);
    if let Some(forwarded_for) = forwarded_for {
        request = request.header("x-forwarded-for", forwarded_for);
    }
    request.send().unwrap()
}

fn login_and_extract_session_cookie(server: &Server) -> String {
    let client = no_redirect_client();
    let r = login_with_csrf(&client, server, "test-pw", None);
    assert_eq!(r.status(), 303);

    r.headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .find(|cookie| cookie.starts_with("ep_sid="))
        .expect("login response should set ep_sid")
        .to_string()
}

fn generate_test_pat(server: &Server, scopes: &[&str]) -> String {
    let db_url = server.db_url.clone();
    let scopes = scopes.to_vec();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move {
            let pool = sqlx::SqlitePool::connect(&db_url).await.unwrap();
            let (token, _) = ep_auth::generate_pat(&pool, "smoke open api", &scopes, None)
                .await
                .unwrap();
            pool.close().await;
            token
        })
}

fn multipart_media_body(
    boundary: &str,
    filename: &str,
    content_type: &str,
    bytes: &[u8],
) -> Vec<u8> {
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"media\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

#[test]
fn full_flow() {
    let server = Server::start();
    let client = no_redirect_client();

    // Liveness is process-only; readiness verifies the DB plus the hydration
    // artifact.
    let r = client.get(server.url("/livez")).send().unwrap();
    assert_eq!(r.status(), 200);
    let r = client.get(server.url("/readyz")).send().unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.text().unwrap(), "ok");

    let status = Command::new(BIN)
        .arg("--healthcheck")
        .env("LEPTOS_SITE_ADDR", server.healthcheck_addr())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run binary healthcheck");
    assert!(
        status.success(),
        "binary healthcheck should pass against running server"
    );

    // cargo-leptos publishes the one canonical hydration artifact. Missing
    // package paths must stay real 404s instead of falling back to WASM bytes.
    let r = client
        .get(server.url("/pkg/eigenpulse.wasm"))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/wasm")
    );
    assert!(
        !r.bytes().unwrap().is_empty(),
        "hydration wasm should not be empty"
    );
    let r = client
        .get(server.url("/pkg/eigenpulse_bg.wasm"))
        .send()
        .unwrap();
    assert_eq!(r.status(), 404);
    let r = client.get(server.url("/pkg/missing.js")).send().unwrap();
    assert_eq!(r.status(), 404);

    // Hydrate registers the root, version-templated service worker. Guard the
    // exact route so a future switch back to the raw `/static/sw.js` asset
    // cannot silently pin every deployment to the placeholder cache key.
    let r = client.get(server.url("/sw.js")).send().unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers()
            .get("service-worker-allowed")
            .and_then(|value| value.to_str().ok()),
        Some("/")
    );
    let body = r.text().unwrap();
    assert!(!body.contains("__EP_SW_VERSION__"));
    assert!(body.contains(&format!("ep-{}", env!("CARGO_PKG_VERSION"))));

    // The dependency-free loader and custom ECharts bundle are both stable
    // URLs whose implementation can change between Eigenpulse releases. They
    // must revalidate instead of being pinned by the browser or service worker.
    let r = client
        .get(server.url("/static/chart-loader.js"))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-cache")
    );
    let body = r.text().unwrap();
    assert!(body.contains("/static/vendor/eigenpulse-charts-6.1.0.js"));
    assert!(body.contains("data-ep-hydrated"));

    let r = client
        .get(server.url("/static/vendor/eigenpulse-charts-6.1.0.js"))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-cache")
    );
    let vendor = r.bytes().unwrap();
    assert!(vendor.len() > 100_000, "chart runtime should not be empty");

    // GET / unauthed → 307 → /login
    let r = client.get(server.url("/")).send().unwrap();
    assert_eq!(r.status(), 307);
    let loc = r.headers().get("location").unwrap().to_str().unwrap();
    assert!(
        loc.starts_with("/login"),
        "expected redirect to /login, got: {loc}"
    );

    // GET /login → 200, form contains PASSWORD label and a header-delivered
    // nonce CSP governing its inline anti-FOUC script.
    let r = client.get(server.url("/login")).send().unwrap();
    assert_eq!(r.status(), 200);
    let csp = r
        .headers()
        .get("content-security-policy")
        .and_then(|value| value.to_str().ok())
        .expect("login must emit a CSP header")
        .to_string();
    assert!(csp.contains("script-src 'self' 'nonce-"));
    assert!(!csp.contains("'unsafe-inline'"));
    let body = r.text().unwrap();
    assert!(body.contains("PASSWORD"));
    assert!(body.contains("<script nonce=\""));

    // The unauthenticated login form has its own small body ceiling. The
    // extractor must reject this before buffering a multi-megabyte password or
    // reaching CSRF/rate-limit/Argon2 work.
    let r = client
        .post(server.url("/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(format!("password={}", "x".repeat(32 * 1024)))
        .send()
        .unwrap();
    assert_eq!(r.status(), 413);

    // SSE endpoint is public at the outer middleware layer so EventSource can
    // connect without a redirect, but the handler itself must still reject
    // missing/invalid session cookies.
    let r = client
        .get(server.url("/events/notifications"))
        .send()
        .unwrap();
    assert_eq!(r.status(), 401);

    // Missing CSRF is rejected before password verification. 303 avoids a
    // POST-preserving redirect loop.
    let r = client
        .post(server.url("/login"))
        .form(&[("password", "wrong-pw")])
        .send()
        .unwrap();
    assert_eq!(r.status(), 303);
    let loc = r.headers().get("location").unwrap().to_str().unwrap();
    assert!(loc.contains("error=1"), "expected ?error=1 in {loc}");

    // Failed login should keep a sanitized deep link so a typo does not lose
    // the user's destination.
    // This one completes the real CSRF handshake and reaches Argon2.
    let r = login_with_csrf(&client, &server, "wrong-pw", Some("/finance?tab=budget"));
    assert_eq!(r.status(), 303);
    let loc = r.headers().get("location").unwrap().to_str().unwrap();
    assert_eq!(loc, "/login?error=1&next=%2Ffinance%3Ftab%3Dbudget");

    let r = login_with_csrf(&client, &server, "wrong-pw", Some("https://example.com"));
    assert_eq!(r.status(), 303);
    let loc = r.headers().get("location").unwrap().to_str().unwrap();
    assert_eq!(loc, "/login?error=1");

    // POST /login correct password (with the CSRF handshake) → 303 → /
    let r = login_with_csrf(&client, &server, "test-pw", None);
    assert_eq!(r.status(), 303);

    // GET / authed (cookie kept by jar) → 200 with exactly the three bundled
    // independent application cards.
    let r = client.get(server.url("/")).send().unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(body.contains("Eigenpulse"), "missing brand");
    assert_eq!(
        body.matches("hub-module-card").count(),
        3,
        "the hub should render exactly Finance, Fitness, and Journal cards"
    );
    assert!(body.contains(r#"href="/finance""#));
    assert!(body.contains(r#"href="/fitness""#));
    assert!(body.contains(r#"href="/journal""#));
    assert!(
        body.contains("/pkg/eigenpulse.wasm"),
        "document bootstrap must load the canonical hydration artifact"
    );
    assert!(
        !body.contains("eigenpulse_bg.wasm"),
        "document bootstrap must not reference a compatibility artifact"
    );

    // All three business applications render independently on first boot. Stable
    // structural markers keep the checks independent from localized copy.
    let r = client.get(server.url("/finance")).send().unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(
        body.contains("finance-toolbar"),
        "missing accounting workspace"
    );
    assert!(
        body.contains("data-ep-chart-spec"),
        "finance charts should render progressively in SSR"
    );

    let r = client.get(server.url("/fitness")).send().unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(body.contains("fitness-panel"), "missing fitness workspace");
    assert!(
        body.contains("data-ep-chart-spec"),
        "fitness charts should render progressively in SSR"
    );

    let r = client.get(server.url("/journal")).send().unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(
        body.contains(r#"data-testid="journal-view""#),
        "missing journal workspace"
    );
    assert_eq!(
        body.matches("data-ep-chart-spec").count(),
        3,
        "journal should expose monthly, calendar, and tag charts"
    );

    // Unknown web paths resolve to a real HTTP 404 rather than an authenticated
    // business shell.
    let r = client
        .get(server.url("/definitely-not-a-route"))
        .send()
        .unwrap();
    assert_eq!(r.status(), 404);
    let body = r.text().unwrap();
    assert!(!body.contains("finance-toolbar"));
    assert!(!body.contains("fitness-panel"));
    assert!(!body.contains(r#"data-testid="journal-view""#));

    // Logout is state-changing: GET must not destroy the session.
    let r = client.get(server.url("/logout")).send().unwrap();
    assert_eq!(r.status(), 405);
    let r = client.get(server.url("/")).send().unwrap();
    assert_eq!(r.status(), 200);

    // /api/v1/healthz public
    let r = client.get(server.url("/api/v1/healthz")).send().unwrap();
    assert_eq!(r.status(), 200);

    // /api/v1/whoami without PAT → 401 + JSON envelope
    let r = client.get(server.url("/api/v1/whoami")).send().unwrap();
    assert_eq!(r.status(), 401);
    assert!(r.text().unwrap().contains("unauthorized"));

    // /api/v1/whoami with bad PAT → 401
    let r = client
        .get(server.url("/api/v1/whoami"))
        .bearer_auth("ep_pat_obviously_invalid_token")
        .send()
        .unwrap();
    assert_eq!(r.status(), 401);

    let pat = generate_test_pat(
        &server,
        &[
            ep_finance::DESCRIPTOR.read_scope,
            ep_finance::DESCRIPTOR.write_scope,
            ep_fitness::DESCRIPTOR.read_scope,
            ep_fitness::DESCRIPTOR.write_scope,
            ep_journal::DESCRIPTOR.read_scope,
            ep_journal::DESCRIPTOR.write_scope,
        ],
    );

    // Accounting resources expose positive integer ids.
    let r = client
        .get(server.url("/api/v1/finance/currencies"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let currencies = r.json::<serde_json::Value>().unwrap();
    let currency_id = currencies
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("id"))
        .and_then(serde_json::Value::as_i64)
        .expect("default finance currency should have an integer id");
    assert!(currency_id > 0);

    let r = client
        .get(server.url(&format!("/finance/export.csv?currency_id={currency_id}")))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("private, no-store")
    );
    assert!(r
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("finance-CNY.csv")));
    assert!(r
        .text()
        .unwrap()
        .starts_with("id,occurred_on,occurred_at,merchant"));

    let r = client
        .post(server.url("/api/v1/finance/accounts"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "currency_id": currency_id,
            "name": "Smoke Wallet",
            "type": "Cash",
            "tone": "blue",
            "opening_balance": "0"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 201);
    let account = r.json::<serde_json::Value>().unwrap();
    let account_id = account["id"]
        .as_i64()
        .expect("finance create response should contain integer id");
    assert!(account_id > 0);

    let r = client
        .get(server.url("/api/v1/finance/accounts"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let accounts = r.json::<serde_json::Value>().unwrap();
    assert!(accounts.as_array().unwrap().iter().any(|row| {
        row["id"].as_i64() == Some(account_id) && row["name"].as_str() == Some("Smoke Wallet")
    }));

    let r = client
        .delete(server.url(&format!("/api/v1/finance/accounts/{account_id}")))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    // Fitness follows the same integer-id boundary, but owns a completely
    // separate resource tree and scope pair.
    let r = client
        .post(server.url("/api/v1/fitness/exercises"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "name": "Smoke Squat",
            "category": "strength",
            "tracking_mode": "weighted",
            "primary_muscle": "legs",
            "equipment": "barbell",
            "notes": "created from smoke"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 201);
    let created = r.json::<serde_json::Value>().unwrap();
    let exercise_id = created["id"]
        .as_i64()
        .expect("fitness create response should contain integer id");
    assert!(exercise_id > 0);

    // Exercise guidance media is a cookie-session feature, not a PAT file
    // path API. Upload a real multipart MP4-shaped payload, verify the tab
    // redirect, authenticated streaming, no compression transform, and Range.
    let mut media_bytes = b"\0\0\0\x18ftypisom\0\0\0\0isommp42".to_vec();
    media_bytes.extend_from_slice(&[0x5a; 64]);
    let boundary = "ep-smoke-media-boundary";
    let r = client
        .post(server.url(&format!("/fitness/media/exercises/{exercise_id}")))
        .header("origin", server.url(""))
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(multipart_media_body(
            boundary,
            "guide.mp4",
            "video/mp4",
            &media_bytes,
        ))
        .send()
        .unwrap();
    assert_eq!(r.status(), 303);
    assert_eq!(
        r.headers()
            .get("location")
            .and_then(|value| value.to_str().ok()),
        Some("/fitness?tab=exercises")
    );

    let r = client
        .patch(server.url(&format!("/api/v1/fitness/exercises/{exercise_id}")))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "name": "Smoke Front Squat",
            "category": "strength",
            "tracking_mode": "weighted",
            "primary_muscle": "legs",
            "equipment": "barbell",
            "notes": "revised"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .get(server.url("/api/v1/fitness/exercises"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let rows = r.json::<serde_json::Value>().unwrap();
    let media_id = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row.pointer("/exercise/id").and_then(|v| v.as_i64()) == Some(exercise_id))
        .and_then(|row| row.pointer("/media/0/id"))
        .and_then(serde_json::Value::as_i64)
        .expect("uploaded exercise media should have an integer id");
    assert!(
        rows.as_array().unwrap().iter().any(|row| {
            row.pointer("/exercise/id").and_then(|v| v.as_i64()) == Some(exercise_id)
                && row.pointer("/exercise/name").and_then(|v| v.as_str())
                    == Some("Smoke Front Squat")
                && row.pointer("/media").and_then(|v| v.as_array()).is_some()
        }),
        "patched fitness exercise should be visible in its module API"
    );

    let raw_client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .gzip(false)
        .build()
        .unwrap();
    let session_cookie = login_and_extract_session_cookie(&server);
    let r = raw_client
        .get(server.url(&format!("/fitness/media/{media_id}")))
        .header(COOKIE, &session_cookie)
        .header("accept-encoding", "gzip")
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers()
            .get("content-encoding")
            .and_then(|value| value.to_str().ok()),
        None,
        "already-compressed videos must bypass HTTP compression"
    );
    assert_eq!(
        r.headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("private, no-cache, no-transform")
    );
    assert_eq!(r.bytes().unwrap().as_ref(), media_bytes.as_slice());

    let r = raw_client
        .get(server.url(&format!("/fitness/media/{media_id}")))
        .header(COOKIE, &session_cookie)
        .header("range", "bytes=4-11")
        .send()
        .unwrap();
    assert_eq!(r.status(), 206);
    let expected_content_range = format!("bytes 4-11/{}", media_bytes.len());
    assert_eq!(
        r.headers()
            .get("content-range")
            .and_then(|value| value.to_str().ok()),
        Some(expected_content_range.as_str())
    );
    assert_eq!(r.bytes().unwrap().as_ref(), &media_bytes[4..=11]);

    let r = client
        .post(server.url(&format!("/fitness/media/{media_id}/delete")))
        .header("origin", server.url(""))
        .send()
        .unwrap();
    assert_eq!(r.status(), 303);
    assert_eq!(
        r.headers()
            .get("location")
            .and_then(|value| value.to_str().ok()),
        Some("/fitness?tab=exercises")
    );

    let r = client
        .delete(server.url(&format!("/api/v1/fitness/exercises/{exercise_id}")))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    // Journal owns a third isolated integer-id resource tree. Exercise its
    // create, list/search, read, update/archive, summary, and delete boundary.
    let r = client
        .post(server.url("/api/v1/journal/entries"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "title": "Smoke journal entry",
            "body": "A journal body created through the PAT API.",
            "entry_date": "2026-07-12",
            "mood": "calm",
            "tags": "smoke, journal"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 201);
    let created = r.json::<serde_json::Value>().unwrap();
    let journal_entry_id = created["id"]
        .as_i64()
        .expect("journal create response should contain integer id");
    assert!(journal_entry_id > 0);
    assert_eq!(created["title"], "Smoke journal entry");

    let r = client
        .get(server.url("/api/v1/journal/entries?q=journal"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let entries = r.json::<serde_json::Value>().unwrap();
    assert!(entries.as_array().unwrap().iter().any(|entry| {
        entry["id"].as_i64() == Some(journal_entry_id)
            && entry["title"].as_str() == Some("Smoke journal entry")
    }));

    let r = client
        .patch(server.url(&format!("/api/v1/journal/entries/{journal_entry_id}")))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "title": "Smoke journal entry revised",
            "archived": true
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 204);

    let r = client
        .get(server.url(&format!("/api/v1/journal/entries/{journal_entry_id}")))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let revised = r.json::<serde_json::Value>().unwrap();
    assert_eq!(revised["title"], "Smoke journal entry revised");
    assert!(
        !revised["archived_at"].is_null(),
        "archiving should set archived_at"
    );

    let r = client
        .get(server.url("/api/v1/journal/entries?include_archived=true"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(r
        .json::<serde_json::Value>()
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["id"].as_i64() == Some(journal_entry_id)));

    let r = client
        .get(server.url("/api/v1/journal/summary"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .delete(server.url(&format!("/api/v1/journal/entries/{journal_entry_id}")))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 204);

    // Module scopes do not bleed across boundaries.
    let finance_only = generate_test_pat(&server, &[ep_finance::DESCRIPTOR.read_scope]);
    let r = client
        .get(server.url("/api/v1/fitness/exercises"))
        .bearer_auth(&finance_only)
        .send()
        .unwrap();
    assert_eq!(r.status(), 403);

    let r = client
        .get(server.url("/api/v1/journal/entries"))
        .bearer_auth(&finance_only)
        .send()
        .unwrap();
    assert_eq!(r.status(), 403);

    let journal_only = generate_test_pat(&server, &[ep_journal::DESCRIPTOR.read_scope]);
    let r = client
        .get(server.url("/api/v1/journal/entries"))
        .bearer_auth(&journal_only)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .get(server.url("/api/v1/finance/currencies"))
        .bearer_auth(&journal_only)
        .send()
        .unwrap();
    assert_eq!(r.status(), 403);

    let r = client
        .get(server.url("/api/v1/fitness/exercises"))
        .bearer_auth(&journal_only)
        .send()
        .unwrap();
    assert_eq!(r.status(), 403);

    // Unknown API paths return the stable JSON 404 envelope.
    let r = client
        .get(server.url("/api/v1/not-a-module/resource"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 404);
    assert!(r.text().unwrap().contains("not_found"));

    let r = client
        .post(server.url("/logout"))
        .header("origin", "http://127.0.0.1:1")
        .send()
        .unwrap();
    assert_eq!(r.status(), 403, "cross-origin logout must be rejected");
    let r = client.get(server.url("/")).send().unwrap();
    assert_eq!(r.status(), 200, "rejected CSRF must preserve the session");

    let r = client
        .post(server.url("/logout"))
        .header("origin", server.url(""))
        .send()
        .unwrap();
    assert_eq!(r.status(), 303);
    let r = client.get(server.url("/")).send().unwrap();
    assert_eq!(r.status(), 307);

    // Five CSRF-valid password attempts are allowed after the successful login
    // reset; the sixth is rejected without another Argon2 verification.
    let rate_client = no_redirect_client();
    for _ in 0..5 {
        let r = login_with_csrf(&rate_client, &server, "still-wrong", None);
        assert_eq!(r.status(), 303);
    }
    let r = login_with_csrf(&rate_client, &server, "still-wrong", None);
    assert_eq!(r.status(), 429);
    assert!(r.headers().contains_key("retry-after"));
}

#[test]
fn login_page_uses_accept_language_for_ssr_lang() {
    let server = Server::start();
    let client = no_redirect_client();

    let r = client
        .get(server.url("/login"))
        .header(ACCEPT_LANGUAGE, "en-GB,en;q=0.9")
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(
        body.contains(r#"<html lang="en">"#),
        "login page should render English html lang"
    );
    assert!(
        body.contains("Login"),
        "login page should render English chrome"
    );
    assert!(
        !body.contains("登录"),
        "login page should not render Chinese chrome under en"
    );
}

#[test]
fn finance_page_uses_locale_cookie_for_ssr_chrome() {
    let server = Server::start();
    let client = no_redirect_client();
    let session_cookie = login_and_extract_session_cookie(&server);

    let r = client
        .get(server.url("/finance"))
        .header(COOKIE, format!("{session_cookie}; ep_locale=en"))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(
        body.contains(r#"<html lang="en""#),
        "finance page should render English html lang"
    );
    assert!(
        body.contains("Accounting"),
        "finance page should render English chrome"
    );
    assert!(
        !body.contains("财务管理"),
        "finance page should not render Chinese nav label under en"
    );
}

#[test]
fn trusted_proxy_rate_limit_ignores_spoofed_leftmost_xff() {
    let server = Server::start_with_trusted_proxy(Some("127.0.0.1/32"));
    let client = no_redirect_client();

    // The edge proxy appended the real client at the right. Changing the
    // attacker-controlled leftmost value must not mint a fresh throttle key.
    for spoof in ["1", "2", "3", "4", "5"] {
        let chain = format!("198.51.100.{spoof}, 203.0.113.8");
        let response = login_with_csrf_from(&client, &server, "wrong-password", None, Some(&chain));
        assert_eq!(response.status(), 303);
    }
    let response = login_with_csrf_from(
        &client,
        &server,
        "wrong-password",
        None,
        Some("198.51.100.99, 203.0.113.8"),
    );
    assert_eq!(response.status(), 429);

    // A genuinely different rightmost untrusted client retains its own bucket.
    let response = login_with_csrf_from(
        &client,
        &server,
        "test-pw",
        None,
        Some("198.51.100.99, 203.0.113.9"),
    );
    assert_eq!(response.status(), 303);
}
