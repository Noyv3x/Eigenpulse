import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import { login, watchConsole } from "./helpers.js";

const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];
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

for (const theme of ["light", "dark"] as const) {
  test(`authenticated routes have no WCAG A/AA violations in ${theme} theme`, async ({
    page,
  }) => {
    const sink = watchConsole(page);
    await login(page);

    for (const route of ROUTES) {
      await page.goto(route, { waitUntil: "load" });
      await expect(page.locator("aside.sidebar")).toBeVisible();

      // Force the visual variant under test. Axe reads computed styles, so
      // this exercises the actual foreground/background contrast tokens
      // without depending on a previous test's localStorage state.
      await page.locator("html").evaluate((element, selectedTheme) => {
        element.setAttribute("data-theme", selectedTheme);
      }, theme);

      // Scan the settled UI rather than the intentionally translucent frames
      // of the 280 ms staggered entrance animations. Contrast requirements
      // apply to the final readable state; reduced-motion users skip these
      // frames entirely via the stylesheet's media query.
      await page.waitForTimeout(650);

      const results = await new AxeBuilder({ page })
        .withTags(WCAG_TAGS)
        .analyze();

      expect(
        results.violations,
        `${route} (${theme})\n${results.violations
          .map(
            (violation) =>
              `${violation.id}: ${violation.help}\n${violation.nodes
                .map(
                  (node) => `  ${node.target.join(" ")}: ${node.failureSummary}`,
                )
                .join("\n")}`,
          )
          .join("\n\n")}`,
      ).toEqual([]);
      sink.assertNoFatal(`${route} ${theme} accessibility scan`);
    }
  });
}
