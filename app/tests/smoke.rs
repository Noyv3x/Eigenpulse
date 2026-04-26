//! End-to-end smoke test against the SSR binary.
//!
//! Run with: `cargo test --features ssr -p eigenpulse --test smoke -- --nocapture`
//!
//! Requires: `cargo leptos build --release` has produced `target/site/`
//! beforehand (the binary needs the static site root). The test spawns the
//! binary on an ephemeral port, exercises the auth flow, and verifies
//! `/api/v1/*` PAT gating. The temp directory is removed on Drop.

#![cfg(feature = "ssr")]

use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_eigenpulse");

struct Server {
    child: Child,
    base: String,
    _data_dir: tempfile::TempDir,
}

impl Server {
    fn start() -> Self {
        let data_dir = tempfile::tempdir().expect("tempdir");
        let port = pick_free_port();
        let base = format!("http://127.0.0.1:{port}");
        let db_url = format!("sqlite://{}/test.db?mode=rwc", data_dir.path().display());
        let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap().to_owned();
        let site_root = workspace_root.join("target").join("site");

        let child = Command::new(BIN)
            .env("EP_ADMIN_PASSWORD", "test-pw")
            .env("EP_SECRET", "x".repeat(64))
            .env("EP_COOKIE_SECURE", "0")
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

        let server = Self { child, base, _data_dir: data_dir };
        server.wait_ready();
        server
    }

    fn url(&self, path: &str) -> String { format!("{}{}", self.base, path) }

    fn wait_ready(&self) {
        let plain = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(500))
            .build().unwrap();
        for _ in 0..40 {
            if let Ok(r) = plain.get(self.url("/healthz")).send() {
                if r.status().is_success() { return; }
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
    std::net::TcpListener::bind("127.0.0.1:0").unwrap()
        .local_addr().unwrap().port()
}

fn no_redirect_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .timeout(Duration::from_secs(3))
        .build().unwrap()
}

#[test]
fn full_flow() {
    let server = Server::start();
    let client = no_redirect_client();

    // /healthz — public
    let r = client.get(server.url("/healthz")).send().unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.text().unwrap(), "ok");

    // GET / unauthed → 307 → /login
    let r = client.get(server.url("/")).send().unwrap();
    assert_eq!(r.status(), 307);
    let loc = r.headers().get("location").unwrap().to_str().unwrap();
    assert!(loc.starts_with("/login"), "expected redirect to /login, got: {loc}");

    // GET /login → 200, form contains PASSWORD label
    let r = client.get(server.url("/login")).send().unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(body.contains("PASSWORD"));

    // POST /login wrong password → 307 back to /login?error=1
    let r = client.post(server.url("/login"))
        .form(&[("password", "wrong-pw")])
        .send().unwrap();
    assert_eq!(r.status(), 307);
    let loc = r.headers().get("location").unwrap().to_str().unwrap();
    assert!(loc.contains("error=1"), "expected ?error=1 in {loc}");

    // POST /login correct password → 303 → /
    let r = client.post(server.url("/login"))
        .form(&[("password", "test-pw")])
        .send().unwrap();
    assert_eq!(r.status(), 303);

    // GET / authed (cookie kept by jar) → 200 with Dashboard markers
    let r = client.get(server.url("/")).send().unwrap();
    assert_eq!(r.status(), 200);
    let body = r.text().unwrap();
    assert!(body.contains("Eigenpulse"), "missing brand");
    assert!(body.contains("FIN-K01"),    "missing finance KPI code");

    // /api/v1/healthz public
    let r = client.get(server.url("/api/v1/healthz")).send().unwrap();
    assert_eq!(r.status(), 200);

    // /api/v1/whoami without PAT → 401 + JSON envelope
    let r = client.get(server.url("/api/v1/whoami")).send().unwrap();
    assert_eq!(r.status(), 401);
    assert!(r.text().unwrap().contains("unauthorized"));

    // /api/v1/whoami with bad PAT → 401
    let r = client.get(server.url("/api/v1/whoami"))
        .bearer_auth("ep_pat_obviously_invalid_token")
        .send().unwrap();
    assert_eq!(r.status(), 401);
}
