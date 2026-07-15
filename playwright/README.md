# Playwright E2E hydration suite

A real browser-driven end-to-end suite that boots the **release** SSR binary
against a **freshly built** leptos site and drives it through headless Chromium.

This is the highest-value automated guard in the project. It catches the bug
class that `cargo check`, `clippy`, the unit tests, and the HTTP-level
`app/tests/smoke.rs` harness **all miss**: a Content-Security-Policy or
hydration footgun that silently degrades every page to a dead, non-interactive
SSR snapshot. That class of bug is only observable in a browser actually
executing the hydrate WASM — exactly what this suite does.

## What it asserts (`tests/hydration.spec.ts`)

After logging in through the real CSRF double-submit flow, four guarantees:

1. **No CSP / hydration console errors** on every main authenticated route
   (`/`, `/finance`, `/fitness`, `/journal`, `/notifications`, `/settings`,
   `/settings/notifications`, `/status`, `/settings/security`). Any console error / pageerror matching
   `Content Security Policy` / `Refused to execute` / `hydration` /
   `unreachable` (wasm trap) / `failed_to_cast` (tachys text-node panic) /
   `panicked` fails the test — those are the signatures of the bug class.
2. **Hydration is LIVE** — clicking the Topbar theme toggle flips
   `<html data-theme>` via the reactive `TweakState` signal. An un-hydrated
   page leaves the attribute unchanged.
3. **Client-side SPA navigation works** — a `window` marker survives a
   sidebar nav-link click that changes the route, proving `leptos_router`
   intercepted the `<A>` click instead of doing a full document reload.
4. **The product catalog stays modular** — the homepage exposes exactly the
   Finance, Fitness, and Journal cards, an arbitrary unknown path returns HTTP 404,
   an exercise exposes a local multi-GIF/video upload control after creation, and
   the Journal create form updates its hydrated entry list without a full reload.

## Build/boot consistency invariant

The binary and the wasm hydrate bundle **must** come from the same
`cargo leptos build` — a stale wasm paired with a fresh binary is itself a
hydration mismatch. So `global-setup.ts` runs `cargo leptos build --release`
immediately before `playwright.config.ts`'s `webServer` boots
`target/release/eigenpulse`. The binary is launched with the same env the Rust
smoke harness uses (`EP_SECRET` 64 chars, `EP_COOKIE_SECURE=0`,
`RUST_MIN_STACK=16777216`, a temp `DATABASE_URL`, the `LEPTOS_*` site vars) plus
`EP_ADMIN_PASSWORD` so first-boot bootstraps the owner with the login password.

## Running it locally

```bash
cd playwright
npm install
npm run install-browsers        # playwright install --with-deps chromium
npm test                        # global-setup builds, webServer boots, specs run
```

To reuse an already-built site (skip the slow `cargo leptos build`):

```bash
cargo leptos build --release    # from the workspace root
EP_SKIP_BUILD=1 npm test        # global-setup asserts target/site + binary exist
```

### Env knobs

| var               | default       | meaning                                                       |
| ----------------- | ------------- | ------------------------------------------------------------- |
| `EP_E2E_PORT`     | `31734`       | fixed test port for the booted binary                         |
| `EP_E2E_PASSWORD` | `e2e-test-pw` | owner password (bootstrapped + used to log in)                |
| `EP_SKIP_BUILD`   | _(unset)_     | `1` ⇒ skip the leptos build, assert artifacts already present |

## CI

Wired as the `e2e` job in `.github/workflows/ci.yml`. The preceding `smoke` job
builds one release binary/WASM pair and uploads it as an artifact. `e2e`
restores that artifact, installs Node + Chromium, type-checks the suite, then
runs `EP_SKIP_BUILD=1 npx playwright test` without rebuilding Rust.

## Type-checking

`npm run typecheck` (`tsc --noEmit`) validates the specs/config against
`tsconfig.json`. The `.ts` files import sibling modules with a `.js` specifier
(the ESM runtime convention Playwright's loader expects); `moduleResolution:
"bundler"` resolves those back to the `.ts` source for the check.
