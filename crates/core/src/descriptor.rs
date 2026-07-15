use crate::IconKind;
use serde::{Deserialize, Serialize};

/// Hydrate-safe metadata for one compile-time bundled application module.
///
/// This is presentation and integration metadata only. It deliberately has
/// no database handles or cross-module domain types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleDescriptor {
    pub slug: &'static str,
    pub route: &'static str,
    pub name_key: &'static str,
    pub description_key: &'static str,
    pub icon: IconKind,
    pub read_scope: &'static str,
    pub write_scope: &'static str,
    pub read_scope_label_key: &'static str,
    pub write_scope_label_key: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryMetric {
    pub label_key: String,
    pub value: String,
    pub detail: Option<String>,
}

/// Small, domain-neutral trend rendered inside a module card on the hub.
///
/// `position` is display geometry only: every module normalizes its own values
/// into a signed -1000..=1000 range and keeps the exact, formatted value in
/// `display`. This lets the shell draw comparable sparklines without learning
/// anything about money, workouts, journal records, or module-owned IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryTrendPoint {
    pub label: String,
    pub position: i32,
    pub display: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummaryTrend {
    pub label_key: String,
    pub points: Vec<SummaryTrendPoint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleSummaryState {
    Ready,
    Empty,
    Active,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleSummary {
    pub slug: String,
    pub state: ModuleSummaryState,
    pub metrics: Vec<SummaryMetric>,
    pub trend: Option<SummaryTrend>,
}

/// Normalize signed values for a dashboard sparkline without exposing their
/// domain-specific scale to the application shell.
pub fn normalize_summary_trend<T>(
    label_key: impl Into<String>,
    values: impl IntoIterator<Item = (String, T, String)>,
) -> Option<SummaryTrend>
where
    T: Into<i128> + Copy,
{
    let values = values
        .into_iter()
        .map(|(label, value, display)| (label, value.into(), display))
        .collect::<Vec<_>>();
    let max_abs = values
        .iter()
        .map(|(_, value, _)| value.unsigned_abs())
        .max()
        .unwrap_or(0);
    if values.is_empty() || max_abs == 0 {
        return None;
    }
    let points = values
        .into_iter()
        .map(|(label, value, display)| {
            let absolute = value.unsigned_abs();
            let magnitude = absolute
                .checked_mul(1000)
                .map(|scaled| scaled / max_abs)
                .unwrap_or_else(|| ((absolute as f64 / max_abs as f64) * 1000.0).round() as u128);
            let magnitude = i32::try_from(magnitude).unwrap_or(1000);
            SummaryTrendPoint {
                label,
                position: if value.is_negative() {
                    -magnitude
                } else {
                    magnitude
                },
                display,
            }
        })
        .collect();
    Some(SummaryTrend {
        label_key: label_key.into(),
        points,
    })
}

#[cfg(test)]
mod tests {
    use super::normalize_summary_trend;

    #[test]
    fn trend_normalization_preserves_sign_and_exact_display() {
        let trend = normalize_summary_trend(
            "finance.chart.net",
            [
                ("May".into(), -25_i64, "-$25".into()),
                ("Jun".into(), 50_i64, "$50".into()),
            ],
        )
        .unwrap();
        assert_eq!(trend.points[0].position, -500);
        assert_eq!(trend.points[1].position, 1000);
        assert_eq!(trend.points[0].display, "-$25");
    }

    #[test]
    fn empty_or_flat_zero_trend_is_omitted() {
        assert!(normalize_summary_trend::<i64>("x", []).is_none());
        assert!(normalize_summary_trend("x", [("now".into(), 0_i64, "0".into())]).is_none());
    }

    #[test]
    fn trend_normalization_handles_the_full_i128_range() {
        let trend = normalize_summary_trend(
            "x",
            [
                ("min".into(), i128::MIN, "min".into()),
                ("half".into(), i128::MIN / 2, "half".into()),
            ],
        )
        .unwrap();
        assert_eq!(trend.points[0].position, -1000);
        assert_eq!(trend.points[1].position, -500);
    }
}
