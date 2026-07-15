import * as echarts from "echarts/core";
import { BarChart, GaugeChart, HeatmapChart, LineChart, PieChart } from "echarts/charts";
import {
  AriaComponent,
  CalendarComponent,
  GraphicComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  VisualMapComponent,
} from "echarts/components";
import { LabelLayout } from "echarts/features";
import { SVGRenderer } from "echarts/renderers";
import { buildOption } from "./options.js";

echarts.use([
  BarChart,
  GaugeChart,
  HeatmapChart,
  LineChart,
  PieChart,
  AriaComponent,
  CalendarComponent,
  GraphicComponent,
  GridComponent,
  LegendComponent,
  TooltipComponent,
  VisualMapComponent,
  LabelLayout,
  SVGRenderer,
]);

const instances = new WeakMap();
const mounted = new Set();

function currentEnvironment() {
  const root = document.documentElement;
  return {
    theme: root.dataset.theme === "dark" ? "dark" : "light",
    density: root.dataset.density === "compact" ? "compact" : "comfortable",
    reducedMotion: window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false,
  };
}

function update(element, state, spec) {
  state.spec = spec;
  state.chart.setOption(
    buildOption(
      spec,
      currentEnvironment(),
      element.getAttribute("aria-label") || "",
      element.dataset.epChartDescription || "",
    ),
    { notMerge: true, lazyUpdate: false },
  );
}

export function mountOrUpdate(element, spec) {
  if (!(element instanceof HTMLElement) || !element.isConnected) return;
  let state = instances.get(element);
  if (!state) {
    element.replaceChildren();
    const chart = echarts.init(element, undefined, { renderer: "svg" });
    const resize = new ResizeObserver(() => chart.resize());
    resize.observe(element);
    state = { chart, resize, spec };
    instances.set(element, state);
    mounted.add(element);
  }
  update(element, state, spec);
}

export function refreshAll() {
  for (const element of [...mounted]) {
    const state = instances.get(element);
    if (!state || !element.isConnected) {
      dispose(element);
      continue;
    }
    update(element, state, state.spec);
    state.chart.resize();
  }
}

export function dispose(element) {
  const state = instances.get(element);
  if (!state) return;
  state.resize.disconnect();
  state.chart.dispose();
  instances.delete(element);
  mounted.delete(element);
}

export const runtimeVersion = "echarts-6.1.0-svg";
