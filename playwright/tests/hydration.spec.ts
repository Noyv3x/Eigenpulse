import { test, expect, type Page } from "@playwright/test";

// Single hydration smoke test.
//
// What it proves: after the SSR HTML loads, the WASM hydrate bundle runs and
// wires up the reactive Topbar theme toggle. Before hydration the toggle is an
// inert SSR <button>; after hydration, clicking it flips the `data-theme`
// attribute on <html> through the TweakState signal + persist Effect. If
// hydration silently fell back to the SSR snapshot (the historical failure
// mode documented in AGENTS.md — wrong wasm filename, a tachys text-node
// panic, an SSR-only dep leaking into wasm), the click does nothing and this
// test fails. That is exactly the regression we want to catch.
//
// The authenticated shell (Sidebar/Topbar) is the only place hydration
// matters — `/login` is a plain server-rendered <form> with no Leptos
// hydration — so the test logs in first.
//
// Env:
//   EP_BASE_URL        SSR binary base URL (default http://127.0.0.1:3000)
//   EP_LOGIN_PASSWORD  owner password to log in with (default "dev")
//
// The whole test self-skips if the server is unreachable, so running the suite
// without a booted binary is a no-op rather than a hard failure.

const LOGIN_PASSWORD = process.env.EP_LOGIN_PASSWORD ?? "dev";

async function serverIsUp(page: Page): Promise<boolean> {
  try {
    const res = await page.request.get("/healthz", { timeout: 3_000 });
    return res.ok();
  } catch {
    return false;
  }
}

test.describe("hydration smoke", () => {
  test("WASM hydration wires the reactive theme toggle", async ({ page }) => {
    test.skip(
      !(await serverIsUp(page)),
      "Eigenpulse SSR binary not reachable at EP_BASE_URL — boot it first (see playwright/README.md).",
    );

    // 1. Authenticate via the server-rendered login form.
    await page.goto("/login");
    await page.locator("#login-password").fill(LOGIN_PASSWORD);
    await Promise.all([
      page.waitForURL((url) => !url.pathname.startsWith("/login"), {
        timeout: 10_000,
      }),
      page.locator("button.login-submit").click(),
    ]);

    // 2. We should now be on an authenticated, hydrated route (the Topbar with
    //    the theme toggle renders inside the app shell).
    const html = page.locator("html");
    const themeToggle = page.locator("button.icon-btn").last();
    await expect(themeToggle).toBeVisible();

    // 3. Wait for hydration: the toggle only mutates state once the WASM
    //    module has run and attached the reactive on:click handler. Poll the
    //    click until `data-theme` flips (or the assertion times out).
    const before = await html.getAttribute("data-theme");

    await expect(async () => {
      await themeToggle.click();
      const after = await html.getAttribute("data-theme");
      expect(after, "data-theme should flip after hydration").not.toEqual(
        before,
      );
      expect(["light", "dark"]).toContain(after);
    }).toPass({ timeout: 10_000 });
  });
});
