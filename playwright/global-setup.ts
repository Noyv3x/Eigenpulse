import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

// Playwright global setup: produce a CONSISTENT release build (SSR binary +
// wasm hydrate bundle from one `cargo leptos build`) before `webServer` boots
// the binary. A stale wasm vs a fresh binary is itself a hydration-mismatch
// footgun, so we never reuse a possibly-out-of-date target/site.
//
// Set EP_SKIP_BUILD=1 to skip the build. CI restores the matching binary/site
// artifact produced by the smoke job before invoking Playwright; in that mode
// this setup only asserts that both artifacts exist.

const __dirname = dirname(fileURLToPath(import.meta.url));
const WORKSPACE_ROOT = join(__dirname, "..");

export default async function globalSetup(): Promise<void> {
  const binary = join(WORKSPACE_ROOT, "target", "release", "eigenpulse");
  const siteRoot = join(WORKSPACE_ROOT, "target", "site");

  if (process.env.EP_SKIP_BUILD === "1") {
    if (!existsSync(binary) || !existsSync(siteRoot)) {
      throw new Error(
        `EP_SKIP_BUILD=1 but the release artifacts are missing.\n` +
          `  expected binary: ${binary}\n` +
          `  expected site:   ${siteRoot}\n` +
          `Run \`cargo leptos build --release\` first, or unset EP_SKIP_BUILD.`,
      );
    }
    return;
  }

  // eslint-disable-next-line no-console
  console.log("[global-setup] cargo leptos build --release (this is slow) ...");
  const res = spawnSync("cargo", ["leptos", "build", "--release"], {
    cwd: WORKSPACE_ROOT,
    stdio: "inherit",
    env: process.env,
  });

  if (res.error) {
    throw new Error(
      `failed to spawn \`cargo leptos build --release\`: ${res.error.message}\n` +
        `Is cargo-leptos installed? (cargo install cargo-leptos --locked --version 0.3.6)`,
    );
  }
  if (res.status !== 0) {
    throw new Error(
      `\`cargo leptos build --release\` exited with status ${res.status}`,
    );
  }
  if (!existsSync(binary) || !existsSync(siteRoot)) {
    throw new Error(
      `build finished but artifacts are missing (binary: ${binary}, site: ${siteRoot})`,
    );
  }
}
