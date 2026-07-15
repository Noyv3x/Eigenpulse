import { type Page, type ConsoleMessage, expect } from "@playwright/test";
import { E2E_PASSWORD } from "../playwright.config.js";

export { E2E_PASSWORD };

// Substrings that mark the exact bug class this suite exists to catch: a CSP
// rejection or a hydration panic that silently degrades a page to a dead SSR
// snapshot. If any console error (or pageerror) contains one of these, the
// page did not hydrate and the test must fail. Matching is case-insensitive.
const FATAL_CONSOLE_PATTERNS = [
  "content security policy",
  "refused to execute", // CSP script-src rejection wording in Chromium
  "refused to load",
  "failed to load resource",
  "hydration",
  "unreachable", // wasm `unreachable` trap (panic in hydrate)
  "failed_to_cast", // tachys text-node walker panic (failed_to_cast_text_node)
  "recoverable_error",
  "panicked",
] as const;

export interface ConsoleSink {
  /** All console-error / pageerror texts seen so far, newest last. */
  readonly errors: string[];
  /** Throw if any captured error matched a FATAL_CONSOLE_PATTERN. */
  assertNoFatal(routeLabel: string): void;
}

/**
 * Attach console + pageerror listeners to a page and return a sink that can be
 * asserted against. Call BEFORE navigating so nothing is missed.
 */
export function watchConsole(page: Page): ConsoleSink {
  const errors: string[] = [];

  const record = (text: string) => {
    errors.push(text);
  };

  page.on("console", (msg: ConsoleMessage) => {
    if (msg.type() === "error") {
      record(msg.text());
    }
  });
  // Uncaught exceptions (a wasm panic surfaces here too).
  page.on("pageerror", (err: Error) => {
    record(`${err.name}: ${err.message}`);
  });

  return {
    errors,
    assertNoFatal(routeLabel: string) {
      const fatal = errors.filter((text) => {
        const lower = text.toLowerCase();
        return FATAL_CONSOLE_PATTERNS.some((pat) => lower.includes(pat));
      });
      expect(
        fatal,
        `CSP / hydration console errors on ${routeLabel} — this is the regression class this suite guards. ` +
          `Offending messages:\n${fatal.join("\n")}`,
      ).toEqual([]);
    },
  };
}

/**
 * Log in through the REAL CSRF double-submit flow:
 *   1. GET /login — the server sets the signed `ep_csrf` cookie and embeds the
 *      matching token in a hidden form field.
 *   2. Fill the password + submit; the browser replays the csrf cookie and the
 *      hidden token, the server verifies they match, then 303-redirects to
 *      `next` (default /).
 *
 * Single-user app: the password is whatever EP_ADMIN_PASSWORD bootstrapped the
 * owner with (E2E_PASSWORD, injected into the booted binary by the config).
 *
 * After this resolves the page is on an authenticated route. Hydration is NOT
 * asserted here — that is each spec's job.
 */
export async function login(page: Page): Promise<void> {
  await page.goto("/login", { waitUntil: "domcontentloaded" });

  // The hidden csrf field is rendered server-side; its presence proves the GET
  // minted the double-submit token. (We submit through the real form so the
  // browser handles the cookie/field pairing exactly like a user would.)
  const csrf = page.locator('input[name="csrf"]');
  await expect(csrf).toHaveCount(1);
  expect(await csrf.inputValue()).not.toEqual("");

  await page.locator("#login-password").fill(E2E_PASSWORD);
  await Promise.all([
    page.waitForURL((url) => !url.pathname.startsWith("/login"), {
      timeout: 15_000,
    }),
    page.locator("button.login-submit").click(),
  ]);
}

/**
 * The Topbar theme toggle: the `<button class="icon-btn">` that flips the
 * theme. The Topbar renders three button.icon-btn (menu, lang-toggle, theme);
 * the theme button is the only one that is neither the menu (first) nor the
 * `.lang-toggle`. Selecting `button.icon-btn:not(.lang-toggle)` yields
 * [menu, theme]; the theme toggle is the last of those. Robust against new
 * non-button `.icon-btn` anchors (e.g. the notifications bell is an <a>).
 */
export function themeToggle(page: Page) {
  return page.locator("button.icon-btn:not(.lang-toggle)").last();
}
