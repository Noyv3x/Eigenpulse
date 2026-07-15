import assert from "node:assert/strict";
import { test } from "node:test";
import { buildOption, richEscape } from "../src/options.js";

const value = (number, display) => ({ value: number, display });

test("axis combo preserves exact display values in richText tooltips", () => {
  const option = buildOption(
    {
      kind: "axis",
      categories: ["Jan"],
      y_label: "USD",
      stacked: false,
      series: [
        { name: "Income", kind: { kind: "bar" }, values: [value(1234, "$12.34")], tone: "positive" },
        {
          name: "Balance",
          kind: { kind: "line", smooth: true, area: false },
          values: [value(876, "$8.76")],
          tone: "primary",
        },
      ],
    },
    { theme: "dark", density: "compact", reducedMotion: true },
    "Cash flow",
  );
  assert.equal(option.series[0].type, "bar");
  assert.equal(option.series[1].type, "line");
  assert.equal(option.series[0].data[0].value, 1234);
  assert.equal(option.series[0].data[0].epDisplay, "$12.34");
  assert.equal(option.tooltip.renderMode, "richText");
  assert.equal(option.animation, false);
  const tooltip = option.tooltip.formatter([
    { axisValueLabel: "Jan", seriesName: "Income", data: option.series[0].data[0] },
  ]);
  assert.match(tooltip, /\$12\.34/);
});

test("every high-level chart kind builds an SVG-runtime-compatible option", () => {
  const specs = [
    { kind: "donut", segments: [{ label: "Food", value: value(20, "$20"), tone: "negative" }] },
    {
      kind: "horizontal_bar",
      items: [{ label: "Squat", value: value(0.8, "80%"), tone: "positive" }],
      max: 1,
    },
    { kind: "calendar_heatmap", year: 2026, points: [{ label: "2026-01-01", value: value(1, "1 workout") }] },
    { kind: "gauge", name: "Goal", value: value(75, "75%"), min: 0, max: 100, tone: "warning" },
    { kind: "sparkline", points: [{ label: "Mon", value: value(2, "2") }], tone: "primary", area: true },
  ];
  for (const spec of specs) {
    const option = buildOption(spec, {}, "Chart");
    assert.ok(Array.isArray(option.series));
    assert.equal(option.aria.enabled, true);
    assert.equal(option.tooltip.renderMode, "richText");
  }
});

test("horizontal bar honors an explicit semantic maximum", () => {
  const option = buildOption({
    kind: "horizontal_bar",
    items: [{ label: "Budget", value: value(0.6, "60%") }],
    max: 1,
  });
  assert.equal(option.xAxis.max, 1);
  const automatic = buildOption({
    kind: "horizontal_bar",
    items: [{ label: "Tag", value: value(6, "6 entries") }],
    max: null,
  });
  assert.equal(automatic.xAxis.max, undefined);
});

test("horizontal bar labels preserve ECharts template characters literally", () => {
  const option = buildOption({
    kind: "horizontal_bar",
    items: [{ label: "Literal", value: value(0.5, "{c} / {b}") }],
  });
  const datum = option.series[0].data[0];
  assert.equal(datum.label.formatter({ data: datum }), "{c} / {b}");
  assert.equal(datum.label.overflow, "truncate");
  assert.equal(datum.label.width, 64);
});

test("rich tooltip text neutralizes ECharts rich-text control characters", () => {
  assert.equal(richEscape("{danger|x}"), "｛danger¦x｝");
});

test("calendar bounds are finite when all values are equal", () => {
  const option = buildOption({
    kind: "calendar_heatmap",
    year: 2026,
    points: [{ label: "2026-02-02", value: value(0, "none") }],
    min: 0,
    max: 0,
  });
  assert.equal(option.visualMap.min, 0);
  assert.equal(option.visualMap.max, 1);
});
