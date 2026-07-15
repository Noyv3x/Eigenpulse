const LIGHT = Object.freeze({
  ink: "#26313b",
  muted: "#65717c",
  faint: "#dfe3e5",
  surface: "#ffffff",
  primary: "#3d9a69",
  positive: "#2f9862",
  negative: "#c85c50",
  warning: "#b7792b",
  neutral: "#7b8791",
  categorical: ["#3d9a69", "#477ec2", "#8b67bd", "#c77b32", "#bf596b", "#4d9394"],
});

const DARK = Object.freeze({
  ink: "#edf0f2",
  muted: "#bdc5cb",
  faint: "#3d464e",
  surface: "#242b31",
  primary: "#68c995",
  positive: "#66cb91",
  negative: "#ed8b7e",
  warning: "#e6b766",
  neutral: "#9ca8b1",
  categorical: ["#68c995", "#72a6e8", "#ae8dde", "#e1a35e", "#df8191", "#6ebbbc"],
});

export function richEscape(value) {
  return String(value ?? "")
    .replaceAll("{", "｛")
    .replaceAll("}", "｝")
    .replaceAll("|", "¦");
}

function validNumber(value, fallback = 0) {
  return Number.isFinite(Number(value)) ? Number(value) : fallback;
}

function pointValue(point) {
  return point == null ? null : validNumber(point.value);
}

function pointDisplay(point) {
  return String(point?.display ?? point?.value ?? "—");
}

function datumValue(datum) {
  return pointValue(datum?.value);
}

function datumDisplay(datum) {
  return pointDisplay(datum?.value);
}

function toneColor(tone, palette, index = 0, categorical = false) {
  if ((tone == null || tone === "primary") && categorical) {
    return palette.categorical[index % palette.categorical.length];
  }
  return palette[tone] ?? palette.primary;
}

function environment(input = {}) {
  const dark = input.theme === "dark";
  const compact = input.density === "compact";
  return {
    dark,
    compact,
    reducedMotion: Boolean(input.reducedMotion),
    palette: dark ? DARK : LIGHT,
    fontSize: compact ? 11 : 12,
    gap: compact ? 8 : 12,
  };
}

function baseOption(env, label, description) {
  return {
    backgroundColor: "transparent",
    animation: !env.reducedMotion,
    animationDuration: env.reducedMotion ? 0 : 360,
    animationDurationUpdate: env.reducedMotion ? 0 : 220,
    animationEasing: "cubicOut",
    textStyle: {
      color: env.palette.ink,
      fontFamily: "Inter, system-ui, sans-serif",
      fontSize: env.fontSize,
    },
    aria: {
      enabled: true,
      decal: { show: true },
      description: description || label,
    },
  };
}

function richTooltip(env, formatter, trigger = "item") {
  return {
    trigger,
    renderMode: "richText",
    confine: true,
    appendToBody: false,
    backgroundColor: env.palette.surface,
    borderColor: env.palette.faint,
    borderWidth: 1,
    padding: env.compact ? 7 : 10,
    textStyle: {
      color: env.palette.ink,
      fontSize: env.fontSize,
      rich: {
        title: {
          color: env.palette.ink,
          fontSize: env.fontSize,
          fontWeight: 600,
          lineHeight: env.compact ? 18 : 20,
        },
        marker: { fontSize: env.fontSize, lineHeight: env.compact ? 17 : 20 },
        name: {
          color: env.palette.muted,
          fontSize: env.fontSize,
          padding: [0, 10, 0, 3],
          lineHeight: env.compact ? 17 : 20,
        },
        value: {
          color: env.palette.ink,
          fontFamily: "JetBrains Mono, ui-monospace, monospace",
          fontSize: env.fontSize,
          fontWeight: 600,
          align: "right",
          lineHeight: env.compact ? 17 : 20,
        },
      },
    },
    formatter,
  };
}

function marker(color) {
  return `{marker|●}{name|` + richEscape(color.name) + `}{value|` + richEscape(color.value) + `}`;
}

function itemTooltip(env) {
  return richTooltip(env, (parameter) => {
    const item = Array.isArray(parameter) ? parameter[0] : parameter;
    const data = item?.data ?? {};
    const name = data.epLabel ?? item?.name ?? "";
    const display = data.epDisplay ?? item?.value ?? "—";
    return `{title|${richEscape(name)}}\n{value|${richEscape(display)}}`;
  });
}

function axisTooltip(env) {
  return richTooltip(
    env,
    (parameters) => {
      const items = Array.isArray(parameters) ? parameters : [parameters];
      if (!items.length) return "";
      const heading = richEscape(items[0]?.axisValueLabel ?? items[0]?.name ?? "");
      const lines = items.map((item) =>
        marker({
          name: item?.seriesName ?? "",
          value: item?.data?.epDisplay ?? item?.value ?? "—",
        }),
      );
      return [`{title|${heading}}`, ...lines].join("\n");
    },
    "axis",
  );
}

function legend(env, show = true) {
  return {
    show,
    type: "scroll",
    bottom: 0,
    itemWidth: env.compact ? 10 : 12,
    itemHeight: env.compact ? 7 : 8,
    textStyle: { color: env.palette.muted, fontSize: env.fontSize },
  };
}

function axisOption(spec, env, label, description) {
  const categories = Array.isArray(spec.categories) ? spec.categories.map(String) : [];
  const sourceSeries = Array.isArray(spec.series) ? spec.series : [];
  const series = sourceSeries.map((item, index) => {
    const kind = item?.kind?.kind === "line" ? "line" : "bar";
    const color = toneColor(item?.tone, env.palette, index, true);
    const common = {
      name: String(item?.name ?? ""),
      type: kind,
      color,
      data: (Array.isArray(item?.values) ? item.values : []).map((point) =>
        point == null
          ? null
          : {
              value: pointValue(point),
              epDisplay: pointDisplay(point),
              itemStyle: { color },
            },
      ),
      emphasis: { focus: "series" },
      animation: !env.reducedMotion,
    };
    if (kind === "line") {
      common.smooth = Boolean(item.kind.smooth);
      common.showSymbol = categories.length <= 16;
      common.symbolSize = env.compact ? 5 : 7;
      common.lineStyle = { width: env.compact ? 2 : 2.5, color };
      if (item.kind.area) common.areaStyle = { color, opacity: env.dark ? 0.16 : 0.1 };
    } else {
      common.barMaxWidth = env.compact ? 20 : 28;
      common.itemStyle = { color, borderRadius: [3, 3, 0, 0] };
      if (spec.stacked) common.stack = "total";
    }
    return common;
  });
  const hasLegend = series.length > 1;
  return {
    ...baseOption(env, label, description),
    color: series.map((item) => item.color),
    tooltip: axisTooltip(env),
    legend: legend(env, hasLegend),
    grid: {
      left: env.compact ? 42 : 52,
      right: env.compact ? 10 : 16,
      top: hasLegend ? 18 : 12,
      bottom: hasLegend ? 46 : 30,
      containLabel: false,
    },
    xAxis: {
      type: "category",
      boundaryGap: series.some((item) => item.type === "bar"),
      data: categories,
      axisLine: { lineStyle: { color: env.palette.faint } },
      axisTick: { show: false },
      axisLabel: {
        color: env.palette.muted,
        fontSize: env.fontSize,
        hideOverlap: true,
      },
    },
    yAxis: {
      type: "value",
      name: spec.y_label || "",
      nameTextStyle: { color: env.palette.muted, fontSize: env.fontSize },
      axisLabel: { color: env.palette.muted, fontSize: env.fontSize },
      axisLine: { show: false },
      axisTick: { show: false },
      splitLine: { lineStyle: { color: env.palette.faint, type: "dashed" } },
    },
    series,
  };
}

function donutOption(spec, env, label, description) {
  const segments = Array.isArray(spec.segments) ? spec.segments : [];
  const data = segments.map((datum, index) => ({
    name: String(datum?.label ?? ""),
    value: datumValue(datum),
    epLabel: String(datum?.label ?? ""),
    epDisplay: datumDisplay(datum),
    itemStyle: { color: toneColor(datum?.tone, env.palette, index, true) },
  }));
  const center = [];
  if (spec.center_value) {
    center.push({
      type: "text",
      left: "center",
      top: "39%",
      silent: true,
      style: {
        text: String(spec.center_value),
        fill: env.palette.ink,
        font: `${env.compact ? 600 : 700} ${env.compact ? 15 : 18}px JetBrains Mono, monospace`,
        textAlign: "center",
      },
    });
  }
  if (spec.center_label) {
    center.push({
      type: "text",
      left: "center",
      top: "50%",
      silent: true,
      style: {
        text: String(spec.center_label),
        fill: env.palette.muted,
        font: `${env.fontSize}px Inter, sans-serif`,
        textAlign: "center",
      },
    });
  }
  return {
    ...baseOption(env, label, description),
    tooltip: itemTooltip(env),
    legend: legend(env, segments.length > 1),
    graphic: center,
    series: [
      {
        name: label,
        type: "pie",
        radius: [env.compact ? "47%" : "50%", env.compact ? "68%" : "72%"],
        center: ["50%", "43%"],
        avoidLabelOverlap: true,
        padAngle: segments.length > 1 ? 1.5 : 0,
        itemStyle: { borderColor: env.palette.surface, borderWidth: segments.length > 1 ? 2 : 0 },
        label: { show: false },
        emphasis: { scale: !env.reducedMotion, scaleSize: 4 },
        data,
      },
    ],
  };
}

function horizontalBarOption(spec, env, label, description) {
  const items = Array.isArray(spec.items) ? spec.items : [];
  const configuredMax = Number(spec.max);
  return {
    ...baseOption(env, label, description),
    tooltip: itemTooltip(env),
    grid: { left: 8, right: env.compact ? 54 : 72, top: 6, bottom: 6, containLabel: true },
    xAxis: {
      type: "value",
      show: false,
      min: 0,
      max: Number.isFinite(configuredMax) && configuredMax > 0 ? configuredMax : undefined,
    },
    yAxis: {
      type: "category",
      inverse: true,
      data: items.map((item) => String(item?.label ?? "")),
      axisLine: { show: false },
      axisTick: { show: false },
      axisLabel: { color: env.palette.muted, fontSize: env.fontSize },
    },
    series: [
      {
        name: label,
        type: "bar",
        barMaxWidth: env.compact ? 13 : 17,
        showBackground: true,
        backgroundStyle: { color: env.palette.faint, borderRadius: 4 },
        data: items.map((datum, index) => ({
          value: datumValue(datum),
          epLabel: String(datum?.label ?? ""),
          epDisplay: datumDisplay(datum),
          itemStyle: {
            color: toneColor(datum?.tone, env.palette, index, false),
            borderRadius: 4,
          },
          label: {
            show: true,
            position: "right",
            color: env.palette.ink,
            fontFamily: "JetBrains Mono, monospace",
            fontSize: env.fontSize,
            // Keep the plot readable on phone-width cards. The tooltip and
            // native details table retain the unabridged exact value.
            width: env.compact ? 48 : 64,
            overflow: "truncate",
            ellipsis: "…",
            // A formatter string is an ECharts template: user-controlled
            // display text such as "{c}" would be expanded to chart data.
            // A callback preserves the exact server-formatted value.
            formatter: (parameter) => String(parameter?.data?.epDisplay ?? "—"),
          },
        })),
      },
    ],
  };
}

function calendarOption(spec, env, label, description) {
  const points = Array.isArray(spec.points) ? spec.points : [];
  const values = points.map(datumValue).filter(Number.isFinite);
  const inferredMin = values.length ? Math.min(...values) : 0;
  const inferredMax = values.length ? Math.max(...values) : 1;
  const min = Number.isFinite(spec.min) ? Number(spec.min) : inferredMin;
  const maxCandidate = Number.isFinite(spec.max) ? Number(spec.max) : inferredMax;
  const max = maxCandidate > min ? maxCandidate : min + 1;
  return {
    ...baseOption(env, label, description),
    tooltip: itemTooltip(env),
    visualMap: {
      min,
      max,
      calculable: false,
      orient: "horizontal",
      left: "center",
      bottom: 0,
      itemWidth: env.compact ? 12 : 16,
      itemHeight: env.compact ? 80 : 100,
      textStyle: { color: env.palette.muted, fontSize: env.fontSize },
      inRange: {
        color: env.dark
          ? ["#303a40", "#315c49", "#4b9a70", "#79d4a2"]
          : ["#edf2ef", "#b7ddc8", "#73bb91", "#32835b"],
      },
    },
    calendar: {
      top: env.compact ? 24 : 30,
      left: env.compact ? 28 : 38,
      right: env.compact ? 8 : 16,
      bottom: env.compact ? 38 : 44,
      range: String(spec.year),
      cellSize: ["auto", env.compact ? 12 : 15],
      itemStyle: { color: env.palette.surface, borderColor: env.palette.faint, borderWidth: 2 },
      splitLine: { show: false },
      dayLabel: { color: env.palette.muted, fontSize: env.fontSize, firstDay: 1 },
      monthLabel: { color: env.palette.muted, fontSize: env.fontSize },
      yearLabel: { show: false },
    },
    series: [
      {
        name: label,
        type: "heatmap",
        coordinateSystem: "calendar",
        data: points.map((datum) => ({
          value: [String(datum?.label ?? ""), datumValue(datum)],
          epLabel: String(datum?.label ?? ""),
          epDisplay: datumDisplay(datum),
        })),
      },
    ],
  };
}

function gaugeOption(spec, env, label, description) {
  const min = validNumber(spec.min);
  const maxCandidate = validNumber(spec.max, min + 1);
  const max = maxCandidate > min ? maxCandidate : min + 1;
  const color = toneColor(spec.tone, env.palette);
  return {
    ...baseOption(env, label, description),
    tooltip: itemTooltip(env),
    series: [
      {
        name: String(spec.name ?? label),
        type: "gauge",
        min,
        max,
        startAngle: 210,
        endAngle: -30,
        progress: { show: true, width: env.compact ? 10 : 13, itemStyle: { color } },
        axisLine: { lineStyle: { width: env.compact ? 10 : 13, color: [[1, env.palette.faint]] } },
        pointer: { show: false },
        axisTick: { show: false },
        splitLine: { show: false },
        axisLabel: { show: false },
        anchor: { show: false },
        title: { color: env.palette.muted, fontSize: env.fontSize, offsetCenter: [0, "28%"] },
        detail: {
          color: env.palette.ink,
          fontFamily: "JetBrains Mono, monospace",
          fontSize: env.compact ? 17 : 21,
          fontWeight: 700,
          offsetCenter: [0, "-2%"],
          formatter: () => pointDisplay(spec.value),
        },
        data: [
          {
            value: pointValue(spec.value),
            name: String(spec.name ?? ""),
            epLabel: String(spec.name ?? label),
            epDisplay: pointDisplay(spec.value),
          },
        ],
      },
    ],
  };
}

function sparklineOption(spec, env, label, description) {
  const points = Array.isArray(spec.points) ? spec.points : [];
  const color = toneColor(spec.tone, env.palette);
  return {
    ...baseOption(env, label, description),
    tooltip: axisTooltip(env),
    grid: { left: 3, right: 3, top: 6, bottom: 4 },
    xAxis: {
      type: "category",
      boundaryGap: false,
      show: false,
      data: points.map((datum) => String(datum?.label ?? "")),
    },
    yAxis: { type: "value", show: false, scale: true },
    series: [
      {
        name: label,
        type: "line",
        smooth: true,
        symbol: "none",
        emphasis: { disabled: env.reducedMotion },
        lineStyle: { color, width: env.compact ? 1.75 : 2.25 },
        areaStyle: spec.area ? { color, opacity: env.dark ? 0.14 : 0.09 } : undefined,
        data: points.map((datum) => ({
          value: datumValue(datum),
          epDisplay: datumDisplay(datum),
          itemStyle: { color },
        })),
      },
    ],
  };
}

export function buildOption(spec, inputEnvironment = {}, label = "", description = "") {
  const env = environment(inputEnvironment);
  switch (spec?.kind) {
    case "axis":
      return axisOption(spec, env, label, description);
    case "donut":
      return donutOption(spec, env, label, description);
    case "horizontal_bar":
      return horizontalBarOption(spec, env, label, description);
    case "calendar_heatmap":
      return calendarOption(spec, env, label, description);
    case "gauge":
      return gaugeOption(spec, env, label, description);
    case "sparkline":
      return sparklineOption(spec, env, label, description);
    default:
      throw new TypeError(`unsupported chart kind: ${String(spec?.kind)}`);
  }
}
