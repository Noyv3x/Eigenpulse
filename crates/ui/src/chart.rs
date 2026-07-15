use ep_i18n::{t, use_locale};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// A numeric chart value and its exact, already-localized presentation.
///
/// `value` is used only for geometry. Tooltips and the accessible table use
/// `display`, so business modules do not lose money precision or duplicate
/// formatting rules in JavaScript.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChartValue {
    pub value: f64,
    pub display: String,
}

impl ChartValue {
    pub fn new(value: f64, display: impl Into<String>) -> Self {
        Self {
            value,
            display: display.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartTone {
    #[default]
    Primary,
    Positive,
    Negative,
    Warning,
    Neutral,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChartDatum {
    pub label: String,
    pub value: ChartValue,
    #[serde(default)]
    pub tone: ChartTone,
}

impl ChartDatum {
    pub fn new(label: impl Into<String>, value: ChartValue) -> Self {
        Self {
            label: label.into(),
            value,
            tone: ChartTone::default(),
        }
    }

    pub fn with_tone(mut self, tone: ChartTone) -> Self {
        self.tone = tone;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AxisSeriesKind {
    Bar,
    Line {
        #[serde(default)]
        smooth: bool,
        #[serde(default)]
        area: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisSeries {
    pub name: String,
    pub kind: AxisSeriesKind,
    pub values: Vec<Option<ChartValue>>,
    #[serde(default)]
    pub tone: ChartTone,
}

impl AxisSeries {
    pub fn bar(name: impl Into<String>, values: Vec<Option<ChartValue>>) -> Self {
        Self {
            name: name.into(),
            kind: AxisSeriesKind::Bar,
            values,
            tone: ChartTone::default(),
        }
    }

    pub fn line(name: impl Into<String>, values: Vec<Option<ChartValue>>) -> Self {
        Self {
            name: name.into(),
            kind: AxisSeriesKind::Line {
                smooth: false,
                area: false,
            },
            values,
            tone: ChartTone::default(),
        }
    }

    pub fn with_tone(mut self, tone: ChartTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn smooth(mut self, smooth: bool) -> Self {
        if let AxisSeriesKind::Line {
            smooth: current, ..
        } = &mut self.kind
        {
            *current = smooth;
        }
        self
    }

    pub fn area(mut self, area: bool) -> Self {
        if let AxisSeriesKind::Line { area: current, .. } = &mut self.kind {
            *current = area;
        }
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AxisChart {
    pub categories: Vec<String>,
    pub series: Vec<AxisSeries>,
    pub y_label: Option<String>,
    #[serde(default)]
    pub stacked: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DonutChart {
    pub segments: Vec<ChartDatum>,
    pub center_label: Option<String>,
    pub center_value: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HorizontalBarChart {
    pub items: Vec<ChartDatum>,
    pub max: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalendarHeatmapChart {
    pub year: i32,
    pub points: Vec<ChartDatum>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GaugeChart {
    pub name: String,
    pub value: ChartValue,
    pub min: f64,
    pub max: f64,
    #[serde(default)]
    pub tone: ChartTone,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SparklineChart {
    pub points: Vec<ChartDatum>,
    #[serde(default)]
    pub tone: ChartTone,
    #[serde(default)]
    pub area: bool,
}

/// High-level, renderer-independent chart contract exposed to business modules.
/// Raw ECharts options are intentionally not accepted here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChartSpec {
    Axis(AxisChart),
    Donut(DonutChart),
    HorizontalBar(HorizontalBarChart),
    CalendarHeatmap(CalendarHeatmapChart),
    Gauge(GaugeChart),
    Sparkline(SparklineChart),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChartHeight {
    Compact,
    #[default]
    Standard,
    Tall,
}

impl ChartHeight {
    fn class(self) -> &'static str {
        match self {
            Self::Compact => "ep-chart--compact",
            Self::Standard => "ep-chart--standard",
            Self::Tall => "ep-chart--tall",
        }
    }
}

/// Progressive chart component.
///
/// SSR emits a labelled chart host plus a real HTML table. The tiny shell
/// loader observes `data-ep-chart-spec`, lazily imports the pinned ECharts SVG
/// runtime, and owns resize/theme/disposal lifecycle. With JavaScript disabled
/// or a vendor load failure, every value remains available in the table.
#[component]
pub fn Chart(
    #[prop(into)] label: String,
    #[prop(into, optional)] description: Option<String>,
    #[prop(into)] spec: Signal<ChartSpec>,
    #[prop(default = ChartHeight::Standard)] height: ChartHeight,
) -> impl IntoView {
    let locale = use_locale();
    let chart_label = label.clone();
    let table_label = label.clone();
    let chart_description = description.clone();
    let serialized =
        move || serde_json::to_string(&spec.get()).unwrap_or_else(|_| "null".to_string());

    view! {
        <figure class=format!("ep-chart {}", height.class()) aria-label=label>
            {description.map(|text| view! { <figcaption class="ep-chart__description">{text}</figcaption> })}
            <div
                class="ep-chart__canvas"
                role="img"
                aria-label=chart_label
                data-ep-chart-spec=serialized
                data-ep-chart-description=chart_description
            >
                <span class="ep-chart__loading" aria-hidden="true">
                    {t(locale, "ui.chart.loading")}
                </span>
            </div>
            <details class="ep-chart__data">
                <summary>{t(locale, "ui.chart.show_table")}</summary>
                <div class="scroll-x">
                    {move || accessible_table(spec.get(), table_label.clone(), locale)}
                </div>
            </details>
        </figure>
    }
}

fn accessible_table(spec: ChartSpec, label: String, locale: ep_i18n::Locale) -> AnyView {
    let (headers, rows) = table_data(spec, locale);
    view! {
        <table class="ep-chart__table">
            <caption>{label}</caption>
            <thead>
                <tr>
                    {headers.into_iter().map(|header| view! {
                        <th scope="col">{header}</th>
                    }).collect_view()}
                </tr>
            </thead>
            <tbody>
                {if rows.is_empty() {
                    view! {
                        <tr><td colspan="99">{t(locale, "ui.chart.no_data")}</td></tr>
                    }.into_any()
                } else {
                    rows.into_iter().map(|row| view! {
                        <tr>
                            {row.into_iter().enumerate().map(|(index, value)| {
                                if index == 0 {
                                    view! { <th scope="row">{value}</th> }.into_any()
                                } else {
                                    view! { <td>{value}</td> }.into_any()
                                }
                            }).collect_view()}
                        </tr>
                    }).collect_view().into_any()
                }}
            </tbody>
        </table>
    }
    .into_any()
}

fn table_data(spec: ChartSpec, locale: ep_i18n::Locale) -> (Vec<String>, Vec<Vec<String>>) {
    let category = t(locale, "ui.chart.category").to_string();
    let value = t(locale, "ui.chart.value").to_string();
    match spec {
        ChartSpec::Axis(chart) => {
            let mut headers = vec![category];
            headers.extend(chart.series.iter().map(|series| series.name.clone()));
            let row_count = chart
                .series
                .iter()
                .map(|series| series.values.len())
                .max()
                .unwrap_or(0)
                .max(chart.categories.len());
            let rows = (0..row_count)
                .map(|index| {
                    let mut row = vec![chart
                        .categories
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| (index + 1).to_string())];
                    row.extend(chart.series.iter().map(|series| {
                        series
                            .values
                            .get(index)
                            .and_then(Option::as_ref)
                            .map(|point| point.display.clone())
                            .unwrap_or_else(|| "—".to_string())
                    }));
                    row
                })
                .collect();
            (headers, rows)
        }
        ChartSpec::Donut(chart) => datum_rows(chart.segments, category, value),
        ChartSpec::HorizontalBar(chart) => datum_rows(chart.items, category, value),
        ChartSpec::CalendarHeatmap(chart) => {
            datum_rows(chart.points, t(locale, "ui.chart.date").to_string(), value)
        }
        ChartSpec::Gauge(chart) => (
            vec![category, value],
            vec![vec![chart.name, chart.value.display]],
        ),
        ChartSpec::Sparkline(chart) => datum_rows(chart.points, category, value),
    }
}

fn datum_rows(
    data: Vec<ChartDatum>,
    first_header: String,
    value_header: String,
) -> (Vec<String>, Vec<Vec<String>>) {
    (
        vec![first_header, value_header],
        data.into_iter()
            .map(|datum| vec![datum.label, datum.value.display])
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_contract_keeps_geometry_and_display_separate() {
        let spec = ChartSpec::Axis(AxisChart {
            categories: vec!["Jan".into()],
            series: vec![
                AxisSeries::bar("Income", vec![Some(ChartValue::new(1234.0, "$12.34"))])
                    .with_tone(ChartTone::Positive),
            ],
            y_label: Some("USD".into()),
            stacked: false,
        });
        let json = serde_json::to_value(spec).expect("serialize chart spec");
        assert_eq!(json["kind"], "axis");
        assert_eq!(json["series"][0]["kind"]["kind"], "bar");
        assert_eq!(json["series"][0]["values"][0]["value"], 1234.0);
        assert_eq!(json["series"][0]["values"][0]["display"], "$12.34");
        assert_eq!(json["series"][0]["tone"], "positive");
    }

    #[test]
    fn accessible_rows_cover_all_axis_categories_and_missing_values() {
        let (headers, rows) = table_data(
            ChartSpec::Axis(AxisChart {
                categories: vec!["Mon".into(), "Tue".into()],
                series: vec![AxisSeries::line(
                    "Minutes",
                    vec![Some(ChartValue::new(30.0, "30 min")), None],
                )],
                ..Default::default()
            }),
            ep_i18n::Locale::En,
        );
        assert_eq!(headers, ["Category", "Minutes"]);
        assert_eq!(rows[0], ["Mon", "30 min"]);
        assert_eq!(rows[1], ["Tue", "—"]);
    }

    #[test]
    fn builders_only_apply_line_options_to_line_series() {
        let bar = AxisSeries::bar("Bar", vec![]).smooth(true).area(true);
        assert_eq!(bar.kind, AxisSeriesKind::Bar);
        let line = AxisSeries::line("Line", vec![]).smooth(true).area(true);
        assert_eq!(
            line.kind,
            AxisSeriesKind::Line {
                smooth: true,
                area: true
            }
        );
    }

    #[test]
    fn every_public_variant_has_the_stable_runtime_discriminant() {
        let point = || ChartDatum::new("point", ChartValue::new(1.0, "one"));
        let specs = [
            ChartSpec::Donut(DonutChart {
                segments: vec![point()],
                ..Default::default()
            }),
            ChartSpec::HorizontalBar(HorizontalBarChart {
                items: vec![point()],
                max: Some(1.0),
            }),
            ChartSpec::CalendarHeatmap(CalendarHeatmapChart {
                year: 2026,
                points: vec![point()],
                min: Some(0.0),
                max: Some(1.0),
            }),
            ChartSpec::Gauge(GaugeChart {
                name: "goal".into(),
                value: ChartValue::new(1.0, "one"),
                min: 0.0,
                max: 1.0,
                tone: ChartTone::Warning,
            }),
            ChartSpec::Sparkline(SparklineChart {
                points: vec![point()],
                ..Default::default()
            }),
        ];
        let kinds = specs
            .into_iter()
            .map(|spec| {
                serde_json::to_value(spec).expect("serialize chart spec")["kind"]
                    .as_str()
                    .expect("kind string")
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                "donut",
                "horizontal_bar",
                "calendar_heatmap",
                "gauge",
                "sparkline"
            ]
        );
    }
}
