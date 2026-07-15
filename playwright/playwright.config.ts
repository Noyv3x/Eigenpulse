import { defineConfig, devices } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";

// End-to-end hydration suite for Eigenpulse.
//
// This config boots the REAL release SSR binary against a REAL built leptos
// site and drives it through a headless Chromium. It is the only guard that
// catches the bug class that compile / clippy / unit / HTTP-smoke all miss:
// a Content-Security-Policy or hydration footgun that silently degrades every
// page to a dead, non-interactive SSR snapshot. Such a bug is only observable
// in a browser actually executing the hydrate WASM, which is what this does.
//
// Consistency invariant (a hydration footgun in its own right): the binary and
// the wasm hydrate bundle MUST come from the SAME `cargo leptos build`. A stale
// wasm paired with a fresh binary is itself a hydration mismatch. So the build
// runs in `global-setup.ts` immediately before the binary is booted by
// `webServer` below — never assume a pre-existing target/site/.
//
// Env knobs:
//   EP_E2E_PORT    fixed test port (default 31734 — uncommon, avoids dev 3000)
//   EP_SKIP_BUILD  if "1", skip `cargo leptos build --release` in global-setup
//                  (CI sets this because it builds the site in a prior step)
//   EP_E2E_PASSWORD owner password the suite bootstraps + logs in with
//                   (default "e2e-test-pw"; also injected as EP_ADMIN_PASSWORD
//                   into the booted binary so first-boot creates this owner)

const __dirname = dirname(fileURLToPath(import.meta.url));
const WORKSPACE_ROOT = join(__dirname, "..");

const PORT = Number(process.env.EP_E2E_PORT ?? "31734");
const BASE_URL = `http://127.0.0.1:${PORT}`;
const PASSWORD = process.env.EP_E2E_PASSWORD ?? "e2e-test-pw";

// Each `npx playwright test` run gets a throwaway SQLite file so first-boot
// bootstrap reliably creates the owner with PASSWORD (EP_ADMIN_PASSWORD is
// ignored once app_user is non-empty — a reused DB would silently use the old
// password). `mode=rwc` creates the file on first open.
const dataDir = mkdtempSync(join(tmpdir(), "ep-e2e-"));
const dbUrl = `sqlite://${join(dataDir, "e2e.db")}?mode=rwc`;
const moduleDataRoot = join(dataDir, "modules");
const siteRoot = join(WORKSPACE_ROOT, "target", "site");
const binary = join(WORKSPACE_ROOT, "target", "release", "eigenpulse");

export default defineConfig({
  testDir: "./tests",
  // The build (when not skipped) runs inside global-setup; give the whole run
  // generous headroom but keep per-test timeouts tight so a wedged hydrate
  // fails fast rather than hanging.
  timeout: 45_000,
  expect: { timeout: 15_000 },
  globalTimeout: 30 * 60_000,
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? [["github"], ["list"]] : "list",

  // `cargo leptos build --release` runs here, before `webServer` boots the
  // binary — guaranteeing the binary and wasm bundle are from one build.
  globalSetup: "./global-setup.ts",

  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },

  // Boot the freshly built release binary with the exact env the Rust smoke
  // harness uses (app/tests/smoke.rs::Server::start), plus the owner password.
  // Playwright waits until BASE_URL answers before running specs.
  webServer: {
    command: binary,
    url: BASE_URL,
    timeout: 120_000,
    reuseExistingServer: false,
    stdout: "pipe",
    stderr: "pipe",
    env: {
      EP_ADMIN_PASSWORD: PASSWORD,
      // EP_SECRET must be >= 64 chars (session cookie signing key).
      EP_SECRET: "e".repeat(64),
      // LAN/NAS HTTP deployment: no Secure cookie over plain http.
      EP_COOKIE_SECURE: "0",
      // Deep SSR view trees can overflow the default tokio worker stack on
      // first render; matches the smoke harness.
      RUST_MIN_STACK: "16777216",
      DATABASE_URL: dbUrl,
      EP_MODULE_DATA_ROOT: moduleDataRoot,
      LEPTOS_SITE_ADDR: `127.0.0.1:${PORT}`,
      LEPTOS_SITE_ROOT: siteRoot,
      LEPTOS_OUTPUT_NAME: "eigenpulse",
      LEPTOS_SITE_PKG_DIR: "pkg",
      RUST_LOG: "warn",
    },
  },

  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});

// Re-exported so specs share the single source of truth for the password.
export { PASSWORD as E2E_PASSWORD, BASE_URL };
