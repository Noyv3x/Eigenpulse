# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**Eigenpulse** — A modular personal-life ERP. Full-stack Rust: Leptos 0.7 SSR + WASM hydration, axum, SQLite via sqlx. Single binary, single distroless container, multi-arch (amd64+arm64) for NAS deployment. See `README.md` for the user-facing pitch.

## Toolchain

- **Rust ≥ 1.88** is required (transitive deps `time 0.3.47`, `home`, `icu_*` need edition2024). The repo's `rust-toolchain.toml` pins `channel = "stable"`; rustup will fetch latest stable on first `cargo` invocation. The Dockerfile uses `rust:1-bookworm` for the same reason — **do not pin to a specific minor like `1.83-bookworm`**, builds will fail.
- `wasm32-unknown-unknown` target is needed for the hydration bundle.
- Build orchestration is `cargo-leptos`, not plain `cargo build`.
- The codebase uses **runtime sqlx** (`sqlx::query`, `sqlx::query_as`, `sqlx::query_scalar`) — **no `sqlx::query!` compile-time macros**. Therefore `SQLX_OFFLINE`, `cargo sqlx prepare`, and a checked-in `.sqlx/` cache are **not** required. `sqlx-cli` is not a build dependency. `sqlx::migrate!()` runs the global migrations on pool open; per-module SQL is applied by `ModuleRegistry::run_migrations()` against the `_ep_module_migration` ledger.

## Common commands

```bash
# Type-check the SSR side (binary + libs). Use this for normal feedback loops.
cargo check -p eigenpulse --features ssr

# Type-check entire workspace (default features, exercises hydrate-side code paths). Slower; use before commits.
cargo check --workspace

# Run dev server with file-watching, runs both SSR and hydrate WASM builds.
# Only EP_ADMIN_PASSWORD is required (for first boot). DATABASE_URL has a default
# (sqlite://data/eigenpulse.db?mode=rwc); `mode=rwc` creates the file on first open.
EP_ADMIN_PASSWORD=dev cargo leptos watch       # http://127.0.0.1:3000

# Production build → target/release/eigenpulse + target/site/{pkg,...}.
cargo leptos build --release

# Multi-arch container build (CI / release).
docker buildx build --platform linux/amd64,linux/arm64 -t eigenpulse:0.1.0 .

# Type-check just one crate.
cargo check -p ep-finance --features ssr
```

There are **no unit tests yet** — `cargo test` runs no suites. Validation is by `cargo check`, manual smoke (see README §9), and the runtime checklist.

## Architecture — the parts that span files

### Module system (the core abstraction)

Every feature lives in `modules/<x>/` as its own crate that implements `ep_core::Module` (defined in `crates/core/src/module.rs`). The trait declares: code/name/icon/section, embedded SQL migrations, `routes(state)` (axum sub-router), `open_api(state)` (mounted under `/api/v1/<x>` with PAT middleware), `dashboard_widgets`, `today_items`, and cross-module `links`.

- Each module exports `pub static MODULE: &dyn Module = &<X>Module;` (a unit struct, zero-cost).
- `app/src/main.rs` registers them with `ModuleRegistry::new().with(ep_finance::MODULE)…` — **one line per module**. To add a new module, see README §"添加新模块".
- The registry runs migrations idempotently via the `_ep_module_migration` ledger table; per-module SQL goes in `modules/<x>/migrations/`. Global tables live in the workspace-root `migrations/0001_init.sql` and are run by `sqlx::migrate!()` at pool open.
- Module-trait impls are gated `#[cfg(feature = "ssr")]` because `routes()` returns `axum::Router`. Everything else (views, models) stays compilable on both targets.

### Hydrate vs SSR feature gating

This is the single most fragile part of the project. `sqlx`, `argon2`, `lettre`, `reqwest`, `tokio` etc. **must not** enter the WASM bundle or the size budget (target < 450 KB gzipped) blows up.

- Every workspace crate has `ssr` and (where applicable) `hydrate` features. Heavy deps are `optional = true` and only enabled under `ssr`.
- `app` is a hybrid: `[lib]` (compiled both for SSR and hydrate-as-rlib) + `[[bin]] required-features = ["ssr"]`. `app-client` is the `cdylib` that pulls `app` with `features = ["hydrate"]`.
- Inside `#[server]` functions, wrap server-only code in `#[cfg(feature = "ssr")]` and provide a stub for `#[cfg(not(feature = "ssr"))]` returning `ServerFnError::ServerError("ssr-only".into())`. See `modules/finance/src/server_fns.rs` for the canonical pattern.

### State propagation (axum + Leptos)

`AppState` (`crates/core/src/state.rs`) holds `db: SqlitePool`, `cookie_key: cookie::Key`, `notify: NotifyBusHandle`, `leptos_options`. It implements `FromRef<AppState>` for `SqlitePool` / `cookie::Key` / `LeptosOptions`, so all three extractors work on `Router<AppState>`.

- The whole axum router uses `Router::<AppState>::new()`. **Don't** revert this to `Router::new()` — type inference picks `LeptosOptions` and breaks `leptos_routes_with_context`.
- `leptos_routes_with_context` takes `&state` (not `&leptos_options`) and provides `AppState` via `provide_context` so `#[server]` functions get it through `expect_context::<AppState>()`.
- The PAT-protected `/api/v1/*` group is layered separately with `from_fn_with_state(state.clone(), ep_auth::pat::require_pat)`; the rest of the app sits under `ep_auth::middleware::require_session`. Public allowlist is in `crates/auth/src/middleware.rs::PUBLIC_PREFIXES`.

### Auth (cookie + PAT, two parallel mechanisms)

- **Cookie session** (`crates/auth/src/session.rs`): browser-only. `ep_sid` cookie, signed with `EP_SECRET` (or generated `data/secret.key`), 30-day sliding renewal. `Secure` flag is **off** by default — controlled by `EP_COOKIE_SECURE=1` because the LAN/NAS HTTP deployment cannot persist a `Secure` cookie. SameSite is `Lax`.
- **Personal Access Tokens** (`crates/auth/src/pat.rs`): `Authorization: Bearer ep_pat_…`. Stored as plain `sha256(token)` (no HMAC, no `EP_SECRET` involvement) + a 12-char visible prefix; verification is byte-equality on the hash. Scopes are space-separated strings declared by each module's `Module::open_api_scopes()`. `require_scope(&pat, "fin:write")` is the gate inside handlers. Revocation = `UPDATE pat SET revoked_at = now WHERE id = ?`; rotating `EP_SECRET` does **not** invalidate tokens.
- First-boot bootstrap (`crates/auth/src/bootstrap.rs`) reads `EP_ADMIN_PASSWORD` and creates the single OWNER row. **Missing → process panics**, by design.
- Argon2id is run inside `tokio::task::spawn_blocking` in `app/src/login.rs::submit` because verify takes ~150–250 ms on Celeron-class NAS hardware.

### Notifications (`Notifier` trait + `NotifyBus`)

`crates/notify/src/bus.rs` owns a `tokio::sync::broadcast::Sender<NotifyMessage>` for SSE fan-out plus a `dispatch()` that: (1) writes to `notification` table, (2) broadcasts, (3) iterates `notify_channel` rows and invokes per-row notifier instance built via `build_notifier(kind, config_json)`. Five impls live alongside: `inapp`, `smtp` (lettre), `bark` / `telegram` / `discord` (reqwest, sharing one global `http_client()`).

The bus exposes itself to other crates as `dyn NotifyBusTrait` (defined in `crates/core/src/notify_msg.rs`) so modules don't depend on `ep-notify` directly.

### Frontend conventions

- All design tokens are in `assets/styles.css` (844 lines, **byte-for-byte from the design bundle** — do not modify defensively; preserve the look). Density is via `data-density="compact|comfortable"` on the root, theme via `data-theme="light|dark"`.
- `crates/ui/src/` has the shared Leptos components (`Kpi`, `Card`, `Tag`, `Tabs`, `PageHead`, `SectionLabel`, `ChartBars`, `Donut`, `Ring`, `Heatmap`, `Sidebar`, `Topbar`, `TweaksPanel`, `Icon`). String props use `#[prop(into, optional)] Option<String>` so call sites can write `title="…"` without `.to_string()` or `Some(…)`.
- `crates/ui/src/sidebar.rs::NAV` is **hardcoded** static; this is intentional because hydrate-side has no `ModuleRegistry`. New modules require a new `NAV` entry in addition to the registry registration.
- Anti-FOUC: `assets/theme-init.js` is inlined in `<head>` to set `data-theme` before paint; `crates/ui/src/tweaks.rs::provide_tweak_state` then takes over with a deduped Effect (compares `prev` to skip no-op writes).

### IDs and the `seq` table

Doc IDs come from `ep_core::next_doc_id(tx, module_code, shape)`:
- `DocIdShape::YearSerial5` → `FIN-26092` (year + 5-digit running)
- `DocIdShape::TypeSerial4 { kind: "S" }` → `FIT-S-0412`

The generator uses `INSERT … ON CONFLICT DO UPDATE … RETURNING last_value` for atomic increment. **Always call inside a transaction** with the row insert so they commit together.

### Shared helpers (don't reinvent)

- Number formatting: `ep_core::{fmt_int, fmt_money, thousands_sep}` — used by both `dashboard.rs` and `finance::view`.
- HTML escape: `ep_core::html_escape`.
- Unauthorized JSON response: `ep_auth::unauthorized(message)`.
- HTTP client (notifiers): `ep_notify::http_client()` (single global `OnceLock<reqwest::Client>`).

### Deploy

- Single distroless container, runs as `nonroot` (uid 65532, the upstream `gcr.io/distroless/cc-debian12:nonroot` default). When mounting a host path for `/data`, `chown -R 65532:65532 <path>`.
- The Dockerfile pins `rust:1-bookworm` (latest stable 1.x) — keep it that way; specific minor pins will break against `time` / `icu_*` / `home`.
- `EP_SECRET` is **only** the signing key for the `ep_sid` session cookie (`SignedCookieJar` in `crates/auth/src/middleware.rs` and `app/src/login.rs`). Rotating it invalidates all browser sessions; **PATs are unaffected** — `crates/auth/src/pat.rs::hash_token` is a plain unkeyed `sha256(token)` and never reads `EP_SECRET`. Don't suggest rotating `EP_SECRET` as a way to revoke leaked tokens; revoke individual rows in `pat` (set `revoked_at`) instead.
- `EP_ADMIN_PASSWORD` is read **only** when `app_user` is empty (first boot). After bootstrap the row stays; the env var has no further effect. To rotate, change the row directly (a `/settings/security` UI for this is on the roadmap).
- `cargo-leptos` is installed and invoked in the build stage; the runtime stage only contains the `eigenpulse` binary + `target/site/`.

## Things I keep getting wrong (avoid these)

- **Don't** import `leptos::view::AnyView` — it's `leptos::prelude::AnyView` in 0.7.
- **Don't** pass `actions=Some(Box::new(|| view!{}.into_any()))` — the prop is `Option<AnyView>`, write `actions=view!{}.into_any()` (typed-builder strips `Option` when `#[prop(optional)]`).
- **Don't** wrap `ServerFnError::ServerError(...)` directly inside `.map_err(|e| …)` — type inference fails on the bare enum constructor. Define a local `fn err(msg: &str) -> ServerFnError` or use a typed closure return.
- **Don't** call `Router::new()` without the explicit `Router::<AppState>::new()` turbofish at the workspace root; it'll be inferred as `Router<LeptosOptions>` and `leptos_routes_with_context` will reject `&state`.
- **Don't** mark cookies `.secure(true)` unconditionally — local HTTP/NAS LAN deployment relies on `EP_COOKIE_SECURE=0` (the default).

## Plan reference

A complete implementation plan with every Phase milestone is at `~/.claude/plans/erp-rust-docker-nas-docker-fetch-this-d-virtual-brook.md`.
