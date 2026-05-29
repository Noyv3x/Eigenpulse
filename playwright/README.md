# Playwright hydration smoke (light)

A deliberately small harness: **one** Playwright test that proves the WASM
hydrate bundle actually runs in a real browser and wires up Leptos reactivity.
It is **not** a full E2E suite and is **not** wired as a required CI gate — it
complements the Rust `app/tests/smoke.rs` harness (which checks the HTTP layer:
`/pkg/<name>_bg.wasm` alias, login redirect, PAT 401) with a single
browser-level hydration assertion that those server-side checks cannot make.

## What the test asserts

After the SSR HTML loads, the hydrate bundle must run and attach the reactive
`on:click` handler to the Topbar theme toggle. The test logs in, then clicks
the toggle and asserts the `data-theme` attribute on `<html>` flips. If
hydration silently fell back to the SSR snapshot — the historical failure mode
documented in `AGENTS.md` (wrong wasm filename, a tachys text-node panic, an
SSR-only dep leaking into the wasm graph) — the click does nothing and the test
fails. See `tests/hydration.spec.ts`.

## Running it

The harness does **not** build or boot the app; that is the caller's job
(building the leptos site is slow and arch-specific). Steps:

```bash
# 1. Build the leptos site (SSR binary + wasm hydrate bundle).
cargo leptos build --release

# 2. Boot the binary with a known owner password on a fresh DB.
EP_ADMIN_PASSWORD=dev EP_SECRET="$(openssl rand -hex 64)" \
  LEPTOS_OUTPUT_NAME=eigenpulse LEPTOS_SITE_ROOT=target/site \
  LEPTOS_SITE_PKG_DIR=pkg LEPTOS_SITE_ADDR=127.0.0.1:3000 \
  DATABASE_URL='sqlite:///tmp/ep-pw.db?mode=rwc' \
  ./target/release/eigenpulse &

# 3. Install deps + browser, then run the one test.
cd playwright
npm install
npm run install-browsers   # playwright install --with-deps chromium
EP_LOGIN_PASSWORD=dev npm test
```

If the SSR binary is not reachable at `EP_BASE_URL` (default
`http://127.0.0.1:3000`), the test **self-skips** rather than failing, so
`npm test` is safe to run without a server.

### Env

| var                 | default                 | meaning                              |
| ------------------- | ----------------------- | ------------------------------------ |
| `EP_BASE_URL`       | `http://127.0.0.1:3000` | base URL of a running SSR binary     |
| `EP_LOGIN_PASSWORD` | `dev`                   | owner password to log in with        |

## TODO / not done here (intentionally light)

- Not added to `.github/workflows/ci.yml` as a gate. Wiring it would mean
  building the leptos site (already done in the `smoke` job), booting the
  binary as a background service, and `npm ci && npx playwright install`. If a
  maintainer wants it gated, add a job that reuses the `smoke` job's build,
  runs the binary with `EP_ADMIN_PASSWORD`, then `cd playwright && npm ci &&
  npm run install-browsers && EP_LOGIN_PASSWORD=… npm test`.
- No `package-lock.json` is committed yet (run `npm install` to generate one);
  a gated CI job should switch to `npm ci` once the lockfile lands.
- Single browser (chromium). Add firefox/webkit projects in
  `playwright.config.ts` if cross-engine hydration coverage is ever needed.
