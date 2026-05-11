//! End-to-end smoke test against the SSR binary.
//!
//! Run with:
//! `cargo leptos build --release && ./scripts/leptos-postbuild.sh && cargo test --features ssr -p eigenpulse --test smoke --release --no-default-features --locked -- --nocapture`
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
        let data_dir = tempfile::tempdir().expect("tempdir");
        let port = pick_free_port();
        let base = format!("http://127.0.0.1:{port}");
        let db_url = format!("sqlite://{}/test.db?mode=rwc", data_dir.path().display());
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        let site_root = workspace_root.join("target").join("site");

        let child = Command::new(BIN)
            .env("EP_ADMIN_PASSWORD", "test-pw")
            .env("EP_SECRET", "x".repeat(64))
            .env("EP_COOKIE_SECURE", "0")
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
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn eigenpulse binary");

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
            if let Ok(r) = plain.get(self.url("/healthz")).send() {
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

fn login_and_extract_session_cookie(server: &Server) -> String {
    let client = no_redirect_client();
    let r = client
        .post(server.url("/login"))
        .form(&[("password", "test-pw")])
        .send()
        .unwrap();
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

#[test]
fn full_flow() {
    let server = Server::start();
    let client = no_redirect_client();

    // /healthz — public
    let r = client.get(server.url("/healthz")).send().unwrap();
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

    // Hydration artifact alias must be present after scripts/leptos-postbuild.sh.
    // Without this file, SSR pages render but the browser silently falls back to
    // a non-hydrated snapshot.
    let r = client
        .get(server.url("/pkg/eigenpulse_bg.wasm"))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(
        !r.bytes().unwrap().is_empty(),
        "hydration wasm alias should not be empty"
    );

    // GET / unauthed → 307 → /login
    let r = client.get(server.url("/")).send().unwrap();
    assert_eq!(r.status(), 307);
    let loc = r.headers().get("location").unwrap().to_str().unwrap();
    assert!(
        loc.starts_with("/login"),
        "expected redirect to /login, got: {loc}"
    );

    // GET /login → 200, form contains PASSWORD label
    let r = client.get(server.url("/login")).send().unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(body.contains("PASSWORD"));

    // SSE endpoint is public at the outer middleware layer so EventSource can
    // connect without a redirect, but the handler itself must still reject
    // missing/invalid session cookies.
    let r = client
        .get(server.url("/events/notifications"))
        .send()
        .unwrap();
    assert_eq!(r.status(), 401);

    // POST /login wrong password → 303 back to /login?error=1.
    // 307 would preserve POST and can create an infinite redirect loop.
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
    let r = client
        .post(server.url("/login"))
        .form(&[("password", "wrong-pw"), ("next", "/finance?tab=budget")])
        .send()
        .unwrap();
    assert_eq!(r.status(), 303);
    let loc = r.headers().get("location").unwrap().to_str().unwrap();
    assert_eq!(loc, "/login?error=1&next=%2Ffinance%3Ftab%3Dbudget");

    let r = client
        .post(server.url("/login"))
        .form(&[("password", "wrong-pw"), ("next", "https://example.com")])
        .send()
        .unwrap();
    assert_eq!(r.status(), 303);
    let loc = r.headers().get("location").unwrap().to_str().unwrap();
    assert_eq!(loc, "/login?error=1");

    // POST /login correct password → 303 → /
    let r = client
        .post(server.url("/login"))
        .form(&[("password", "test-pw")])
        .send()
        .unwrap();
    assert_eq!(r.status(), 303);

    // GET / authed (cookie kept by jar) → 200 with Dashboard markers
    let r = client.get(server.url("/")).send().unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(body.contains("Eigenpulse"), "missing brand");
    assert!(body.contains("FIN-K01"), "missing finance KPI code");

    // The in-progress modules should SSR successfully after login. These
    // checks intentionally use stable component/card codes instead of
    // localized copy.
    let r = client.get(server.url("/fitness")).send().unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(body.contains("FIT-K01"), "missing fitness KPI code");
    assert!(body.contains("FIT-S-NEW"), "missing fitness entry form");
    assert!(
        body.contains("FIT-SES-01"),
        "missing fitness sessions table"
    );

    let r = client.get(server.url("/learning")).send().unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(body.contains("LRN-K01"), "missing learning KPI code");
    assert!(body.contains("LRN-BK-01"), "missing learning books card");
    assert!(body.contains("LRN-N-01"), "missing learning notes card");
    assert!(body.contains("LRN-CRS-01"), "missing learning courses card");

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
            ep_core::SCOPE_FIT_READ,
            ep_core::SCOPE_FIT_WRITE,
            ep_core::SCOPE_LRN_READ,
            ep_core::SCOPE_LRN_WRITE,
        ],
    );

    let r = client
        .post(server.url("/api/v1/fit/workout"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "occurred_on": "2026-05-10",
            "kind": "Smoke Run",
            "program": "API",
            "duration_m": 31,
            "load_text": "5km",
            "strain": "M",
            "rpe": 7,
            "notes": "created from smoke"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let fit_doc = r
        .json::<serde_json::Value>()
        .unwrap()
        .pointer("/doc_id")
        .and_then(|v| v.as_str())
        .expect("fit create doc_id")
        .to_string();
    assert!(fit_doc.starts_with("FIT-S-"));

    let r = client
        .patch(server.url(&format!("/api/v1/fit/workout/{fit_doc}")))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "kind": "Smoke Tempo",
            "duration_m": 36,
            "strain": "H"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .get(server.url("/api/v1/fit/workout"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let rows = r.json::<serde_json::Value>().unwrap();
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(
                |row| row.pointer("/doc_id").and_then(|v| v.as_str()) == Some(&fit_doc)
                    && row.pointer("/kind").and_then(|v| v.as_str()) == Some("Smoke Tempo")
            ),
        "patched fitness workout should be visible in open API list"
    );

    let r = client
        .delete(server.url(&format!("/api/v1/fit/workout/{fit_doc}")))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .post(server.url("/api/v1/lrn/note"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "title": "Smoke note",
            "body": "created from smoke"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let note_doc = r
        .json::<serde_json::Value>()
        .unwrap()
        .pointer("/doc_id")
        .and_then(|v| v.as_str())
        .expect("note create doc_id")
        .to_string();
    assert!(note_doc.starts_with("LRN-N-"));

    let r = client
        .patch(server.url(&format!("/api/v1/lrn/note/{note_doc}")))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "title": "Smoke note revised"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .get(server.url("/api/v1/lrn/note"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let rows = r.json::<serde_json::Value>().unwrap();
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(
                |row| row.pointer("/doc_id").and_then(|v| v.as_str()) == Some(&note_doc)
                    && row.pointer("/title").and_then(|v| v.as_str()) == Some("Smoke note revised")
            ),
        "patched learning note should be visible in open API list"
    );

    let r = client
        .delete(server.url(&format!("/api/v1/lrn/note/{note_doc}")))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .post(server.url("/api/v1/lrn/book"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "name": "Smoke Book",
            "author": "Integration",
            "status": "reading"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let book_doc = r
        .json::<serde_json::Value>()
        .unwrap()
        .pointer("/doc_id")
        .and_then(|v| v.as_str())
        .expect("book create doc_id")
        .to_string();
    assert!(book_doc.starts_with("LRN-B-"));

    let r = client
        .patch(server.url(&format!("/api/v1/lrn/book/{book_doc}")))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "status": "done"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .get(server.url("/api/v1/lrn/book"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let rows = r.json::<serde_json::Value>().unwrap();
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(
                |row| row.pointer("/doc_id").and_then(|v| v.as_str()) == Some(&book_doc)
                    && row.pointer("/status").and_then(|v| v.as_str()) == Some("done")
            ),
        "patched learning book should be visible in open API list"
    );

    let r = client
        .delete(server.url(&format!("/api/v1/lrn/book/{book_doc}")))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .post(server.url("/api/v1/lrn/course"))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "name": "Smoke Course",
            "provider": "Integration",
            "progress_pct": 25.0,
            "due_on": "2026-06-30",
            "tone": "amber"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let course_doc = r
        .json::<serde_json::Value>()
        .unwrap()
        .pointer("/doc_id")
        .and_then(|v| v.as_str())
        .expect("course create doc_id")
        .to_string();
    assert!(course_doc.starts_with("LRN-C-"));

    let r = client
        .patch(server.url(&format!("/api/v1/lrn/course/{course_doc}")))
        .bearer_auth(&pat)
        .json(&serde_json::json!({
            "progress_pct": 80.0,
            "tone": "green"
        }))
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client
        .get(server.url("/api/v1/lrn/course"))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);
    let rows = r.json::<serde_json::Value>().unwrap();
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(
                |row| row.pointer("/doc_id").and_then(|v| v.as_str()) == Some(&course_doc)
                    && row.pointer("/progress").and_then(|v| v.as_f64()) == Some(0.8)
                    && row.pointer("/tone").and_then(|v| v.as_str()) == Some("green")
            ),
        "patched learning course should be visible in open API list"
    );

    let r = client
        .delete(server.url(&format!("/api/v1/lrn/course/{course_doc}")))
        .bearer_auth(&pat)
        .send()
        .unwrap();
    assert_eq!(r.status(), 200);

    let r = client.post(server.url("/logout")).send().unwrap();
    assert_eq!(r.status(), 303);
    let r = client.get(server.url("/")).send().unwrap();
    assert_eq!(r.status(), 307);
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
        body.contains(r#"<html lang="en">"#),
        "finance page should render English html lang"
    );
    assert!(
        body.contains("Finance"),
        "finance page should render English chrome"
    );
    assert!(
        !body.contains("财务管理"),
        "finance page should not render Chinese nav label under en"
    );
}
