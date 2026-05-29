import { test, expect } from "@playwright/test";
import { login, themeToggle, watchConsole } from "./helpers.js";

// End-to-end hydration suite.
//
// This is the highest-value guard in the project: a real headless Chromium
// executes the hydrate WASM against the real release binary. It catches the
// bug class that compile / clippy / unit / HTTP-smoke ALL miss — a CSP or
// hydration footgun that degrades every page to a dead SSR snapshot. (Such a
// CSP bug shipped to production once and only a browser caught it.)
//
// Three things are proven:
//   1. No CSP / hydration console errors on any main authenticated route.
//   2. Hydration is LIVE: the reactive theme toggle flips <html data-theme>.
//   3. Client-side SPA navigation works (a nav-link click changes the route
//      WITHOUT a full document reload — proof the router hydrated).

// The authenticated routes that render inside the hydrated app shell. The
// status route is `/status` (the sidebar "STA" entry); there is no
// `/settings/status`. `/settings/security` exercises the ActionForm +
// error-slot wrapper hydration path called out in AGENTS.md.
const ROUTES = [
  "/",
  "/finance",
  "/fitness",
  "/learning",
  "/reports",
  "/settings",
  "/status",
  "/settings/security",
] as const;

test.describe("authenticated hydration", () => {
  test("no CSP / hydration console errors on any main route", async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);

    for (const route of ROUTES) {
      await page.goto(route, { waitUntil: "load" });
      // Give the hydrate bundle a beat to run and attach handlers; a CSP
      // rejection or wasm panic fires synchronously during this window.
      await page.waitForLoadState("networkidle");
      // The app shell must be present (proves we are authenticated, not on a
      // login redirect or error page).
      await expect(page.locator("aside.sidebar")).toBeVisible();
      sink.assertNoFatal(route);
    }

    // Final sweep across everything collected during the walk.
    sink.assertNoFatal("the full route walk");
  });

  test("hydration is live — theme toggle flips <html data-theme>", async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);

    // Land on the dashboard; the Topbar (with the theme toggle) renders in the
    // app shell on every authenticated route.
    await page.goto("/", { waitUntil: "networkidle" });

    const html = page.locator("html");
    const toggle = themeToggle(page);
    await expect(toggle).toBeVisible();

    const before = await html.getAttribute("data-theme");
    expect(["light", "dark"]).toContain(before);

    // The on:click handler only exists once the WASM module has run and wired
    // the reactive TweakState signal. Poll the click until data-theme flips;
    // if hydration silently degraded to the SSR snapshot the attribute never
    // changes and this times out (the regression we want to catch).
    await expect(async () => {
      await toggle.click();
      const after = await html.getAttribute("data-theme");
      expect(after, "data-theme should flip after hydration").not.toEqual(
        before,
      );
      expect(["light", "dark"]).toContain(after);
    }).toPass({ timeout: 15_000 });

    sink.assertNoFatal("theme toggle");
  });

  test("hydration is live — client-side SPA navigation (no full reload)", async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);
    await page.goto("/", { waitUntil: "networkidle" });

    // Tag the live document. A hydrated leptos_router intercepts <A> clicks and
    // swaps the view WITHOUT a navigation, so this marker survives. A
    // non-hydrated page would do a full document load on the anchor click and
    // wipe the marker.
    await page.evaluate(() => {
      (window as unknown as Record<string, unknown>).__ep_spa_marker = true;
    });

    // The sidebar nav link to /finance is a hydrated <A> (renders as <a href>).
    const financeLink = page.locator('aside.sidebar a[href="/finance"]');
    await expect(financeLink).toBeVisible();
    await financeLink.click();

    await page.waitForURL((url) => url.pathname === "/finance", {
      timeout: 15_000,
    });

    const survived = await page.evaluate(
      () =>
        (window as unknown as Record<string, unknown>).__ep_spa_marker === true,
    );
    expect(
      survived,
      "window marker should survive a client-side SPA navigation; if it was wiped, the anchor triggered a full reload (router did not hydrate)",
    ).toBe(true);

    // And we are genuinely on the finance route inside the shell.
    await expect(page.locator("aside.sidebar")).toBeVisible();
    sink.assertNoFatal("SPA navigation");
  });
});
