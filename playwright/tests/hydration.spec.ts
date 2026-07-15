import { test, expect, type Page } from "@playwright/test";
import {
  E2E_PASSWORD,
  login,
  themeToggle,
  watchConsole,
} from "./helpers.js";

// End-to-end hydration suite.
//
// This is the highest-value guard in the project: a real headless Chromium
// executes the hydrate WASM against the real release binary. It catches the
// bug class that compile / clippy / unit / HTTP-smoke ALL miss — a CSP or
// hydration footgun that degrades every page to a dead SSR snapshot.
//
// Three things are proven:
//   1. No CSP / hydration console errors on any main authenticated route.
//   2. Hydration is LIVE: the reactive theme toggle flips <html data-theme>.
//   3. Client-side SPA navigation works (a nav-link click changes the route
//      WITHOUT a full document reload — proof the router hydrated).

// The authenticated routes that render inside the hydrated app shell. The
// status route is `/status` (the sidebar "STA" entry); there is no
// `/settings/status`. `/settings/security` exercises the ActionForm +
// stable error-slot wrapper hydration path.
const ROUTES = [
  "/",
  "/finance",
  "/fitness",
  "/journal",
  "/notifications",
  "/settings",
  "/settings/notifications",
  "/status",
  "/settings/security",
] as const;

async function gotoAppRoute(page: Page, route: string) {
  await page.goto(route, { waitUntil: "load" });
  await expect(page.locator("aside.sidebar")).toBeVisible();
  // Authenticated pages open an SSE stream, so Playwright's `networkidle`
  // never becomes stable. A short post-load tick is enough for synchronous
  // hydration/CSP failures to surface through the console/pageerror listeners.
  await page.waitForTimeout(250);
}

test("login survives a concurrent redirected login-page request", async ({
  page,
}) => {
  await page.goto("/login?next=%2F", { waitUntil: "load" });
  const formToken = await page.locator('input[name="csrf"]').inputValue();
  expect(formToken).not.toEqual("");

  // A normal desktop browser may request the conventional favicon path while
  // the login form is visible. The authenticated-route middleware redirects
  // that request to another login page, which must reuse the existing signed
  // token instead of invalidating the form already on screen.
  const favicon = await page.request.get("/favicon.ico");
  expect(new URL(favicon.url()).pathname).toBe("/login");
  expect(await page.locator('input[name="csrf"]').inputValue()).toBe(formToken);

  await page.locator("#login-password").fill(E2E_PASSWORD);
  await Promise.all([
    page.waitForURL((url) => url.pathname === "/"),
    page.locator("button.login-submit").click(),
  ]);
  await expect(page.locator("form.login-card")).toHaveCount(0);
});

test.describe("authenticated hydration", () => {
  test("no CSP / hydration console errors on any main route", async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);

    for (const route of ROUTES) {
      await gotoAppRoute(page, route);
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
    await gotoAppRoute(page, "/");

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
    await gotoAppRoute(page, "/");

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

  test("manual timezone persists, validates, and can return to automatic mode", async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);
    await gotoAppRoute(page, "/settings");

    const form = page.getByTestId("timezone-form");
    const input = page.getByTestId("timezone-input");
    const browserTimezone = page.getByTestId("browser-timezone");
    let errorsBeforeInvalidSubmission: number | undefined;
    try {
      await expect(browserTimezone).not.toHaveText(/Detecting|正在检测/);
      const detected = (await browserTimezone.textContent())?.trim();
      expect(detected).toBeTruthy();
      const manualTimezone =
        detected === "Asia/Shanghai" ? "America/New_York" : "Asia/Shanghai";

      await input.fill(manualTimezone);
      await form.locator('button[type="submit"]').click();
      await expect(form.getByRole("status")).toBeVisible();
      await expect(page.getByTestId("timezone-mode")).toHaveAttribute(
        "data-timezone-mode",
        "manual",
      );
      await page.reload({ waitUntil: "load" });
      await expect(page.getByTestId("timezone-input")).toHaveValue(manualTimezone);
      sink.assertNoFatal("timezone save and reload");

      // Leptos transports a rejected server-function Result as HTTP 500, so
      // Chromium emits one expected resource error for this deliberate
      // validation failure. Keep that separate from the hydration/CSP guard
      // while still rejecting every other console error from this point on.
      errorsBeforeInvalidSubmission = sink.errors.length;

      // Server-side validation is authoritative even when a client supplies a
      // value outside the datalist suggestions.
      await page.getByTestId("timezone-input").fill("Not/A_Timezone");
      await page
        .getByTestId("timezone-form")
        .locator('button[type="submit"]')
        .click();
      await expect(
        page.getByTestId("timezone-form").getByRole("alert"),
      ).toBeVisible();
      await page.reload({ waitUntil: "load" });
      await expect(page.getByTestId("timezone-input")).toHaveValue(manualTimezone);
      await expect(page.getByTestId("timezone-mode")).toHaveAttribute(
        "data-timezone-mode",
        "manual",
      );
    } finally {
      // Restore the deterministic suite baseline even when an assertion above
      // fails, because every Playwright worker shares this throwaway database.
      await page.goto("/settings", { waitUntil: "load" });
      await expect(page.getByTestId("timezone-auto-button")).toBeEnabled();
      await page.getByTestId("timezone-auto-button").click();
      await expect(
        page.getByTestId("timezone-auto-form").getByRole("status"),
      ).toBeVisible();
      await expect(page.getByTestId("timezone-mode")).toHaveAttribute(
        "data-timezone-mode",
        "auto",
      );

      if (errorsBeforeInvalidSubmission !== undefined) {
        const unexpectedErrors = sink.errors
          .slice(errorsBeforeInvalidSubmission)
          .filter(
            (message) =>
              !message.includes(
                "Failed to load resource: the server responded with a status of 500",
              ),
          );
        expect(
          unexpectedErrors,
          "the deliberate validation rejection must not hide hydration/CSP errors",
        ).toEqual([]);
      }
    }
  });

  test("automatic timezone follows travel, reloads once, and respects manual mode", async ({
    browser,
  }, testInfo) => {
    const baseURL = testInfo.project.use.baseURL;
    expect(baseURL).toBeTruthy();
    const utcContext = await browser.newContext({ baseURL, timezoneId: "UTC" });
    const tokyoContext = await browser.newContext({
      baseURL,
      timezoneId: "Asia/Tokyo",
    });
    const aucklandContext = await browser.newContext({
      baseURL,
      timezoneId: "Pacific/Auckland",
    });
    const utcPage = await utcContext.newPage();
    const tokyoPage = await tokyoContext.newPage();
    const aucklandPage = await aucklandContext.newPage();
    const utcSink = watchConsole(utcPage);
    const tokyoSink = watchConsole(tokyoPage);
    const aucklandSink = watchConsole(aucklandPage);

    try {
      // Establish a deterministic automatic UTC baseline through the public UI.
      await login(utcPage);
      await gotoAppRoute(utcPage, "/settings");
      await expect(utcPage.getByTestId("browser-timezone")).toHaveText("UTC");
      await utcPage.getByTestId("timezone-auto-button").click();
      await expect(
        utcPage.getByTestId("timezone-auto-form").getByRole("status"),
      ).toBeVisible();
      await expect(utcPage.getByTestId("timezone-input")).toHaveValue("UTC");

      let tokyoSyncRequests = 0;
      let tokyoDashboardLoads = 0;
      tokyoPage.on("request", (request) => {
        if (request.url().includes("/sync_browser_timezone")) {
          tokyoSyncRequests += 1;
        }
      });
      tokyoPage.on("framenavigated", (frame) => {
        if (
          frame === tokyoPage.mainFrame() &&
          new URL(frame.url()).pathname === "/"
        ) {
          tokyoDashboardLoads += 1;
        }
      });

      // First Tokyo hydration changes UTC -> Tokyo and reloads exactly once;
      // the second sync observes the persisted value and terminates the loop.
      await login(tokyoPage);
      await expect.poll(() => tokyoSyncRequests).toBe(2);
      await expect.poll(() => tokyoDashboardLoads).toBe(2);
      await tokyoPage.waitForTimeout(500);
      expect(tokyoSyncRequests).toBe(2);
      expect(tokyoDashboardLoads).toBe(2);

      await tokyoPage.locator('aside.sidebar a[href="/settings"]').click();
      await tokyoPage.waitForURL((url) => url.pathname === "/settings");
      await expect(tokyoPage.getByTestId("browser-timezone")).toHaveText(
        "Asia/Tokyo",
      );
      await expect(tokyoPage.getByTestId("timezone-input")).toHaveValue(
        "Asia/Tokyo",
      );
      await expect(tokyoPage.getByTestId("timezone-mode")).toHaveAttribute(
        "data-timezone-mode",
        "auto",
      );

      // Pin a different manual zone, then open Eigenpulse from a third country.
      await tokyoPage
        .getByTestId("timezone-input")
        .fill("America/New_York");
      await tokyoPage
        .getByTestId("timezone-form")
        .locator('button[type="submit"]')
        .click();
      await expect(
        tokyoPage.getByTestId("timezone-form").getByRole("status"),
      ).toBeVisible();
      await expect(tokyoPage.getByTestId("timezone-mode")).toHaveAttribute(
        "data-timezone-mode",
        "manual",
      );

      await login(aucklandPage);
      await gotoAppRoute(aucklandPage, "/settings");
      await expect(aucklandPage.getByTestId("browser-timezone")).toHaveText(
        "Pacific/Auckland",
      );
      await expect(aucklandPage.getByTestId("timezone-input")).toHaveValue(
        "America/New_York",
      );
      await expect(aucklandPage.getByTestId("timezone-mode")).toHaveAttribute(
        "data-timezone-mode",
        "manual",
      );

      // The explicit button is the only path from manual back to auto.
      await aucklandPage.getByTestId("timezone-auto-button").click();
      await expect(
        aucklandPage.getByTestId("timezone-auto-form").getByRole("status"),
      ).toBeVisible();
      await expect(aucklandPage.getByTestId("timezone-input")).toHaveValue(
        "Pacific/Auckland",
      );
      await expect(aucklandPage.getByTestId("timezone-mode")).toHaveAttribute(
        "data-timezone-mode",
        "auto",
      );

      tokyoSink.assertNoFatal("Tokyo automatic timezone sync");
      aucklandSink.assertNoFatal("manual override and auto re-enable");
    } finally {
      // Return the shared throwaway database to automatic UTC for later tests.
      await utcPage.goto("/settings", { waitUntil: "load" });
      await expect(utcPage.getByTestId("timezone-input")).toHaveValue("UTC");
      await expect(utcPage.getByTestId("timezone-mode")).toHaveAttribute(
        "data-timezone-mode",
        "auto",
      );
      utcSink.assertNoFatal("automatic UTC restore");
      await Promise.all([
        utcContext.close(),
        tokyoContext.close(),
        aucklandContext.close(),
      ]);
    }
  });

  test("home catalog contains exactly the three independent business apps", async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);
    await gotoAppRoute(page, "/");

    const cards = page.locator(".hub-module-card");
    await expect(cards).toHaveCount(3);
    await expect(cards.locator('a[href="/finance"]')).toHaveCount(1);
    await expect(cards.locator('a[href="/fitness"]')).toHaveCount(1);
    await expect(cards.locator('a[href="/journal"]')).toHaveCount(1);
    sink.assertNoFatal("three-app home catalog");
  });

  test("module charts render, stay accessible, and react to range controls", async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);

    await gotoAppRoute(page, "/journal");
    const journal = page.getByTestId("journal-analytics");
    const journalCharts = journal.locator(".ep-chart__canvas");
    await expect(journalCharts).toHaveCount(3);
    for (let index = 0; index < 3; index += 1) {
      await expect(journalCharts.nth(index)).toHaveAttribute(
        "data-ep-chart-state",
        "ready",
      );
      await expect(journalCharts.nth(index).locator("svg")).toHaveCount(1);
    }

    // The real HTML table is a progressive/accessibility fallback, not a
    // JavaScript-generated approximation.
    const firstTable = journal.locator(".ep-chart__data").first();
    await firstTable.locator("summary").click();
    await expect(firstTable.locator("table")).toBeVisible();
    await expect(firstTable.locator("tbody tr")).toHaveCount(12);

    await page.getByTestId("journal-range-3").click();
    await expect(page.getByTestId("journal-range-3")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect
      .poll(async () => {
        const raw = await journalCharts.first().getAttribute(
          "data-ep-chart-spec",
        );
        return raw ? JSON.parse(raw).categories.length : 0;
      })
      .toBe(3);
    await expect(journalCharts.first()).toHaveAttribute(
      "data-ep-chart-state",
      "ready",
    );
    await expect(journalCharts.first().locator("svg")).toHaveCount(1);

    const calendarBefore = JSON.parse(
      (await journalCharts.nth(1).getAttribute("data-ep-chart-spec")) ?? "{}",
    ).year;
    await page.getByTestId("journal-year-previous").click();
    await expect(page.getByTestId("journal-year-previous")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect
      .poll(async () => {
        const raw = await journalCharts.nth(1).getAttribute(
          "data-ep-chart-spec",
        );
        return raw ? JSON.parse(raw).year : null;
      })
      .toBe(calendarBefore - 1);
    await expect(journalCharts.nth(1)).toHaveAttribute(
      "data-ep-chart-state",
      "ready",
    );
    await expect(journalCharts.nth(1).locator("svg")).toHaveCount(1);

    // Seed one module-owned trend and prove the homepage card can render it
    // without the shell learning anything about Journal records.
    const form = page.getByTestId("journal-create-form");
    await form.locator('[name="title"]').fill("Chart trend seed");
    await form.locator('[name="body"]').fill("Dashboard sparkline seed.");
    await form.locator('button[type="submit"]').click();
    await expect(
      page
        .getByTestId("journal-entry-list")
        .locator('[data-testid^="journal-entry-"]')
        .filter({ hasText: "Chart trend seed" }),
    ).toBeVisible();
    await gotoAppRoute(page, "/");
    const journalCard = page.locator(".hub-module-card").filter({
      has: page.locator('a[href="/journal"]'),
    });
    const homeSparkline = journalCard.locator(".ep-chart__canvas");
    await expect(homeSparkline).toHaveCount(1);
    await expect(homeSparkline).toHaveAttribute(
      "data-ep-chart-state",
      "ready",
    );

    await gotoAppRoute(page, "/finance");
    const financeTrend = page.locator(".ep-chart__canvas").first();
    await expect(financeTrend).toHaveAttribute("data-ep-chart-state", "ready");
    await page.getByTestId("finance-trend-range-3").click();
    await expect(page.getByTestId("finance-trend-range-3")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect
      .poll(async () => {
        const raw = await financeTrend.getAttribute("data-ep-chart-spec");
        return raw ? JSON.parse(raw).categories.length : 0;
      })
      .toBe(3);
    await expect(financeTrend).toHaveAttribute(
      "data-ep-chart-state",
      "ready",
    );
    await expect(financeTrend.locator("svg")).toHaveCount(1);

    await gotoAppRoute(page, "/fitness");
    await page.locator("#ep-tab-progress").click();
    const fitness = page.getByTestId("fitness-progress-analytics");
    await expect(fitness).toBeVisible();
    const fitnessCharts = fitness.locator(".ep-chart__canvas");
    await expect(fitnessCharts).toHaveCount(2);
    for (let index = 0; index < 2; index += 1) {
      await expect(fitnessCharts.nth(index)).toHaveAttribute(
        "data-ep-chart-state",
        "ready",
      );
    }
    await page.getByTestId("fitness-week-range-4").click();
    await expect(page.getByTestId("fitness-week-range-4")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await expect
      .poll(async () => {
        const raw = await fitnessCharts.first().getAttribute(
          "data-ep-chart-spec",
        );
        return raw ? JSON.parse(raw).categories.length : 0;
      })
      .toBe(4);
    await expect(fitnessCharts.first()).toHaveAttribute(
      "data-ep-chart-state",
      "ready",
    );
    await expect(fitnessCharts.first().locator("svg")).toHaveCount(1);

    sink.assertNoFatal("interactive module charts");
  });

  test("an unknown route returns a real 404 instead of a business page", async ({
    page,
  }) => {
    await login(page);

    const route = "/definitely-not-a-route";
    // Assert the HTTP contract directly. Navigating a browser tab to an
    // intentional 404 produces Chromium's generic "failed to load resource"
    // console message, which is indistinguishable from the missing-WASM signal
    // guarded by watchConsole().
    const response = await page.request.get(route);
    expect(response.status()).toBe(404);
    const body = await response.text();
    expect(body).not.toContain("finance-toolbar");
    expect(body).not.toContain("fitness-panel");
    expect(body).not.toContain('data-testid="journal-view"');
  });

  test("mobile navigation has independent state and dismisses accessibly", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 390, height: 800 });
    const sink = watchConsole(page);
    await login(page);
    await page.goto("/", { waitUntil: "load" });
    await page.waitForTimeout(250);

    const toggle = page.locator("button.mobile-nav-toggle");
    const drawer = page.locator("#app-sidebar");
    await expect(toggle).toBeVisible();
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(drawer).toBeHidden();

    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    await expect(drawer).toBeVisible();
    await expect(drawer.locator('a[aria-current="page"]')).toBeFocused();
    await expect(page.locator(".topbar")).toHaveAttribute("inert", "");
    await expect(page.locator("main.main")).toHaveAttribute("inert", "");

    // The drawer is modal for keyboard users: Shift+Tab from its first link
    // wraps to logout, and Tab wraps back without entering inert page chrome.
    await page.keyboard.press("Shift+Tab");
    await expect(page.locator("#sidebar-logout")).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(page.locator("#sidebar-first-nav")).toBeFocused();

    await page.keyboard.press("Escape");
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(drawer).toBeHidden();
    await expect(toggle).toBeFocused();

    await toggle.click();
    await drawer.locator('a[href="/finance"]').click();
    await page.waitForURL((url) => url.pathname === "/finance");
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(drawer).toBeHidden();

    await toggle.click();
    await page.locator("button.mobile-scrim").click({ position: { x: 300, y: 300 } });
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(drawer).toBeHidden();
    await expect(toggle).toBeFocused();
    sink.assertNoFatal("mobile navigation");
  });

  test("fitness tabs and exercise media settings hydrate end to end", async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);
    await gotoAppRoute(page, "/fitness");

    const tabs = page.getByRole("tab");
    await expect(tabs).toHaveCount(5);
    for (let index = 0; index < 5; index += 1) {
      const controls = await tabs.nth(index).getAttribute("aria-controls");
      expect(controls).toBe("fitness-panel");
      await expect(page.locator(`#${controls}`)).toHaveCount(1);
    }

    await page.locator("#ep-tab-exercises").click();
    const create = page.locator(
      'form[action="/api/_internal/fitness/create_exercise"]',
    );
    await create.locator('input[name="name"]').fill("E2E Squat");
    await Promise.all([
      page.waitForResponse(
        (response) =>
          response.url().includes("/api/_internal/fitness/create_exercise") &&
          response.request().method() === "POST",
      ),
      create.locator('button[type="submit"]').click(),
    ]);

    const exercise = page.locator("article").filter({ hasText: "E2E Squat" });
    await expect(exercise).toBeVisible();
    const upload = exercise.locator('form[enctype="multipart/form-data"]');
    await expect(upload).toHaveAttribute(
      "action",
      /\/fitness\/media\/exercises\/\d+/,
    );
    const media = upload.locator('input[type="file"][name="media"]');
    await expect(media).toHaveAttribute(
      "accept",
      "image/gif,video/mp4,video/webm",
    );
    await expect(media).toHaveAttribute("multiple", "");
    sink.assertNoFatal("fitness exercise media settings");
  });

  test("journal entry creation hydrates and updates its independent list", async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);
    await gotoAppRoute(page, "/journal");

    const view = page.getByTestId("journal-view");
    const form = page.getByTestId("journal-create-form");
    const entries = page.getByTestId("journal-entry-list");
    await expect(view).toBeVisible();
    await expect(form).toBeVisible();
    await expect(entries).toBeVisible();

    await form.locator('[name="title"]').fill("E2E Journal Entry");
    await form
      .locator('[name="body"]')
      .fill("Created by a hydrated Journal ActionForm.");
    await form.locator('[name="entry_date"]').fill("2026-07-12");
    await form.locator('button[type="submit"]').click();

    const created = entries
      .locator('[data-testid^="journal-entry-"]')
      .filter({ hasText: "E2E Journal Entry" });
    await expect(created).toBeVisible();
    await created.locator("summary").click();
    await expect(created.locator('textarea[name="body"]')).toHaveValue(
      "Created by a hydrated Journal ActionForm.",
    );
    sink.assertNoFatal("journal entry creation");
  });

  test("PAT generation disables submission while the token is pending", async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);
    await gotoAppRoute(page, "/settings/security");

    let releaseRequest!: () => void;
    const requestGate = new Promise<void>((resolve) => {
      releaseRequest = resolve;
    });
    let generationRequests = 0;
    await page.route("**/api/_internal/cfg/generate_pat", async (route) => {
      generationRequests += 1;
      await requestGate;
      await route.continue();
    });

    const form = page.locator('form[action="/api/_internal/cfg/generate_pat"]');
    const submit = form.locator('button[type="submit"]');
    await expect(form.locator('input[name="scopes"]')).toHaveValue(
      "finance:read finance:write fitness:read fitness:write journal:read journal:write notifications:write",
    );
    await form.locator('input[name="name"]').fill("pending-guard-e2e");
    await submit.click();
    await expect.poll(() => generationRequests).toBe(1);
    await expect(submit).toBeDisabled();
    await expect(submit).toHaveAttribute("aria-busy", "true");

    // Even a scripted second click cannot dispatch while the native button is
    // disabled. This models rapid repeat activation without relying on timing.
    await submit.evaluate((element) => (element as HTMLButtonElement).click());
    expect(generationRequests).toBe(1);
    releaseRequest();

    await expect(submit).toBeEnabled();
    await expect(page.locator(".new-token-slot code")).toContainText("ep_pat_");
    expect(generationRequests).toBe(1);
    sink.assertNoFatal("PAT pending guard");
  });

  test("insecure LAN contexts skip service-worker registration", async ({
    page,
  }) => {
    // Chromium treats loopback HTTP as a secure context. Override the browser
    // capability before any app script runs to exercise the plain-LAN branch
    // used by NAS deployments without TLS.
    await page.addInitScript(() => {
      Object.defineProperty(window, "isSecureContext", {
        configurable: true,
        value: false,
      });
    });
    let serviceWorkerRequests = 0;
    page.on("request", (request) => {
      if (new URL(request.url()).pathname === "/sw.js") {
        serviceWorkerRequests += 1;
      }
    });
    const sink = watchConsole(page);

    await login(page);
    await gotoAppRoute(page, "/");
    expect(await page.evaluate(() => window.isSecureContext)).toBe(false);
    expect(serviceWorkerRequests).toBe(0);
    sink.assertNoFatal("insecure-context service-worker guard");
  });

});
