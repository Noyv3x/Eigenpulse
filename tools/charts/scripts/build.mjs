import { build } from "esbuild";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const project = resolve(here, "..");
const output = resolve(project, "../../assets/vendor/eigenpulse-charts-6.1.0.js");
const licenseOutput = resolve(
  project,
  "../../assets/vendor/eigenpulse-charts-6.1.0.LICENSE.txt",
);
const check = process.argv.includes("--check");

const licenseParts = await Promise.all(
  [
    ["Apache ECharts 6.1.0", "node_modules/echarts/LICENSE"],
    ["zrender 6.1.0", "node_modules/zrender/LICENSE"],
    ["tslib 2.3.0", "node_modules/tslib/LICENSE.txt"],
  ].map(async ([name, path]) => {
    const text = await readFile(join(project, path), "utf8");
    return `${"=".repeat(78)}\n${name}\n${"=".repeat(78)}\n\n${text.trim()}\n`;
  }),
);
const licenses = Buffer.from(
  "Third-party licenses for assets/vendor/eigenpulse-charts-6.1.0.js\n\n" +
    licenseParts.join("\n"),
  "utf8",
);

const result = await build({
  entryPoints: [join(project, "src/index.js")],
  bundle: true,
  write: false,
  format: "esm",
  platform: "browser",
  target: ["es2022"],
  minify: true,
  treeShaking: true,
  charset: "utf8",
  legalComments: "eof",
  banner: {
    js: "/*! Eigenpulse chart runtime · Apache-2.0 ECharts 6.1.0 · SVG renderer */",
  },
});

const bundled = result.outputFiles[0].contents;
if (check) {
  let committed;
  try {
    committed = await readFile(output);
  } catch {
    throw new Error(`missing generated chart bundle: ${output}`);
  }
  if (!committed.equals(bundled)) {
    throw new Error("chart bundle is stale; run npm run build in tools/charts");
  }
  const committedLicenses = await readFile(licenseOutput).catch(() => null);
  if (!committedLicenses || !committedLicenses.equals(licenses)) {
    throw new Error("chart third-party license artifact is stale; run npm run build in tools/charts");
  }
} else {
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, bundled);
  await writeFile(licenseOutput, licenses);
}
