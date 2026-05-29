import { defineConfig, devices } from "@playwright/test";

// Light hydration smoke. The base URL points at an already-running Eigenpulse
// SSR binary (default the dev/prod port 3000). This config deliberately does
// NOT spawn the server itself — building the leptos site + booting the binary
// is the caller's job (see README.md in this dir). The single test self-skips
// if the server is unreachable, so `npx playwright test` is safe to run
// without a server and never wedges CI.
const BASE_URL = process.env.EP_BASE_URL ?? "http://127.0.0.1:3000";

export default defineConfig({
  testDir: "./tests",
  timeout: 30_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  retries: 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
});
