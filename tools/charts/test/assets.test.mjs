import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { gzipSync } from "node:zlib";

const loaderUrl = new URL("../../../assets/chart-loader.js", import.meta.url);
const bundleUrl = new URL("../../../assets/vendor/eigenpulse-charts-6.1.0.js", import.meta.url);
const bundledLicensesUrl = new URL(
  "../../../assets/vendor/eigenpulse-charts-6.1.0.LICENSE.txt",
  import.meta.url,
);
const packageUrl = new URL("../package.json", import.meta.url);
const noticesUrl = new URL("../THIRD_PARTY_NOTICES.md", import.meta.url);

test("runtime dependencies are exact pins", async () => {
  const manifest = JSON.parse(await readFile(packageUrl, "utf8"));
  assert.equal(manifest.dependencies.echarts, "6.1.0");
  assert.equal(manifest.devDependencies.esbuild, "0.28.1");
  assert.doesNotMatch(manifest.dependencies.echarts, /^[~^]/);
  assert.doesNotMatch(manifest.devDependencies.esbuild, /^[~^]/);
});

test("third-party runtime and build dependencies have checked-in attribution", async () => {
  const notices = await readFile(noticesUrl, "utf8");
  const licenses = await readFile(bundledLicensesUrl, "utf8");
  for (const required of [
    "Apache ECharts | 6.1.0 | Apache-2.0",
    "zrender | 6.1.0 | BSD-3-Clause",
    "tslib | 2.3.0 | 0BSD",
    "esbuild 0.28.1",
  ]) {
    assert.ok(notices.includes(required), `missing notice: ${required}`);
  }
  assert.match(licenses, /Apache ECharts 6\.1\.0/);
  assert.match(licenses, /Apache License\s+Version 2\.0/);
  assert.match(licenses, /zrender 6\.1\.0/);
  assert.match(licenses, /BSD 3-Clause License/);
  assert.match(licenses, /tslib 2\.3\.0/);
  assert.ok(licenses.length > 10_000, "aggregate license artifact looks truncated");
});

test("loader is lazy, same-origin, and observes update/disposal lifecycle", async () => {
  const loader = await readFile(loaderUrl, "utf8");
  assert.match(loader, /import\(RUNTIME_URL\)/);
  assert.match(loader, /\/static\/vendor\/eigenpulse-charts-6\.1\.0\.js/);
  assert.match(loader, /MutationObserver/);
  assert.match(loader, /removedNodes\.forEach\(disposeTree\)/);
  assert.match(loader, /eigenpulse:hydrated/);
  assert.match(loader, /data-ep-hydrated/);
  assert.doesNotMatch(loader, /setTimeout/);
  assert.match(loader, /data-theme/);
  assert.match(loader, /data-density/);
  assert.match(loader, /prefers-reduced-motion/);
});

test("committed vendor bundle is ESM, SVG-only, and CSP-safe", async () => {
  const bundle = await readFile(bundleUrl, "utf8");
  assert.match(bundle, /^\/\*! Eigenpulse chart runtime .* SVG renderer \*\//);
  assert.match(bundle, /6\.1\.0/);
  assert.match(bundle, /export\{/);
  assert.doesNotMatch(bundle, /CanvasRenderer/);
  assert.doesNotMatch(bundle, /\beval\s*\(/);
  assert.ok(Buffer.byteLength(bundle) < 750_000, "tree-shaken chart bundle unexpectedly grew");
  assert.ok(
    gzipSync(bundle, { level: 9 }).byteLength <= 320 * 1024,
    "gzip chart bundle exceeds the 320 KiB budget",
  );
});
