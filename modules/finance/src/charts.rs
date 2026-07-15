//! Finance-owned projections for generic chart components.
//!
//! Money stays exact as [`MinorAmount`] until the last step. `f64` values in
//! this module are presentation geometry only; every visible value and chart
//! tooltip is built from the exact minor-unit amount.

use crate::model::{Budget, CategorySummary, MinorAmount, MonthBucket};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TrendPoint {
    pub label: String,
    pub income_geometry: f64,
    pub expense_geometry: f64,
    pub net_geometry: f64,
    pub income_display: String,
    pub expense_display: String,
    pub net_display: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CategorySlice {
    pub label: String,
    pub tone: String,
    pub geometry: f64,
    pub display: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BudgetBar {
    pub label: String,
    /// Clamped 0..=1 fill geometry. Exact used/limit values remain in display.
    pub fill: f64,
    pub over_budget: bool,
    pub display: String,
}

pub(crate) fn format_money(symbol: &str, decimals: u8, amount: MinorAmount) -> String {
    format!("{symbol}{}", crate::amount::fmt_minor(amount, decimals))
}

/// Project the trailing window of the canonical 12-month series.
pub(crate) fn trend_points(
    months: &[MonthBucket],
    window: usize,
    symbol: &str,
    decimals: u8,
) -> Vec<TrendPoint> {
    let start = months.len().saturating_sub(window.clamp(1, 12));
    months[start..]
        .iter()
        .map(|month| TrendPoint {
            label: month.period.clone(),
            income_geometry: major_geometry(month.income, decimals),
            expense_geometry: major_geometry(month.expense, decimals),
            net_geometry: major_geometry(month.net, decimals),
            income_display: format_money(symbol, decimals, month.income),
            expense_display: format_money(symbol, decimals, month.expense),
            net_display: format_money(symbol, decimals, month.net),
        })
        .collect()
}

/// Keep the six largest categories and merge the remainder into one exact
/// "Other" slice. Returns `None` only if the server-provided amounts cannot be
/// safely aggregated, which should already have been rejected by Finance's
/// checked report query.
pub(crate) fn category_slices(
    categories: &[CategorySummary],
    other_label: &str,
    symbol: &str,
    decimals: u8,
) -> Option<Vec<CategorySlice>> {
    let mut categories = categories.to_vec();
    categories.sort_by_key(|category| std::cmp::Reverse(category.value));
    let split = categories.len().min(6);
    let (top, rest) = categories.split_at(split);
    let mut slices = top
        .iter()
        .map(|category| CategorySlice {
            label: if category.icon.trim().is_empty() {
                category.name.clone()
            } else {
                format!("{} {}", category.icon, category.name)
            },
            tone: category.tone.clone(),
            geometry: major_geometry(category.value, decimals),
            display: format_money(symbol, decimals, category.value),
        })
        .collect::<Vec<_>>();
    if !rest.is_empty() {
        let other = MinorAmount::try_sum(rest.iter().map(|category| category.value))?;
        slices.push(CategorySlice {
            label: other_label.to_string(),
            tone: "muted".into(),
            geometry: major_geometry(other, decimals),
            display: format_money(symbol, decimals, other),
        });
    }
    Some(slices)
}

pub(crate) fn budget_bars(budgets: &[Budget], symbol: &str, decimals: u8) -> Vec<BudgetBar> {
    budgets
        .iter()
        .map(|budget| {
            let ratio = if budget.amount.is_positive() {
                major_geometry(budget.used, decimals) / major_geometry(budget.amount, decimals)
            } else {
                0.0
            };
            let ratio = finite_or_zero(ratio);
            BudgetBar {
                label: budget.category_name.clone(),
                fill: ratio.clamp(0.0, 1.0),
                over_budget: budget.used > budget.amount,
                display: format!(
                    "{} / {}",
                    format_money(symbol, decimals, budget.used),
                    format_money(symbol, decimals, budget.amount)
                ),
            }
        })
        .collect()
}

#[cfg(any(feature = "ssr", test))]
pub(crate) fn summary_net_trend(
    months: &[MonthBucket],
    symbol: &str,
    decimals: u8,
) -> Option<ep_core::SummaryTrend> {
    ep_core::normalize_summary_trend(
        "finance.chart.net",
        months.iter().rev().take(6).rev().map(|month| {
            (
                month.period.clone(),
                month.net.as_i128(),
                format_money(symbol, decimals, month.net),
            )
        }),
    )
}

fn major_geometry(amount: MinorAmount, decimals: u8) -> f64 {
    let scale = 10_f64.powi(i32::from(decimals));
    finite_or_zero(amount.to_f64() / scale)
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn month(period: &str, income: i64, expense: i64) -> MonthBucket {
        let income = MinorAmount::from(income);
        let expense = MinorAmount::from(expense);
        MonthBucket {
            period: period.into(),
            income,
            expense,
            net: income.checked_sub(expense).unwrap(),
        }
    }

    #[test]
    fn trend_window_preserves_exact_labels_and_uses_finite_major_geometry() {
        let months = (1..=12)
            .map(|month_number| {
                month(
                    &format!("2026-{month_number:02}"),
                    i64::from(month_number) * 10_001,
                    i64::from(month_number) * 3_000,
                )
            })
            .collect::<Vec<_>>();
        let points = trend_points(&months, 3, "$", 2);
        assert_eq!(
            points
                .iter()
                .map(|point| point.label.as_str())
                .collect::<Vec<_>>(),
            ["2026-10", "2026-11", "2026-12"]
        );
        assert_eq!(points[0].income_geometry, 1_000.1);
        assert_eq!(points[0].income_display, "$1,000.10");
        assert_eq!(points[0].net_display, "$700.10");
        assert!(points.iter().all(|point| {
            point.income_geometry.is_finite()
                && point.expense_geometry.is_finite()
                && point.net_geometry.is_finite()
        }));
    }

    #[test]
    fn donut_keeps_top_six_and_aggregates_other_exactly() {
        let categories = (1..=8)
            .map(|index| CategorySummary {
                category_id: index,
                name: format!("C{index}"),
                icon: String::new(),
                tone: "blue".into(),
                value: MinorAmount::from(index * 100),
                pct: 0.0,
            })
            .collect::<Vec<_>>();
        let slices = category_slices(&categories, "Other", "$", 2).unwrap();
        assert_eq!(slices.len(), 7);
        assert_eq!(slices[0].label, "C8");
        assert_eq!(slices[0].display, "$8.00");
        assert_eq!(slices[6].label, "Other");
        assert_eq!(slices[6].display, "$3.00");
    }

    #[test]
    fn budget_geometry_is_clamped_but_tooltip_remains_exact() {
        let budget = Budget {
            id: 1,
            currency_id: 1,
            currency_code: "USD".into(),
            period: "2026-07".into(),
            category_id: 2,
            category_name: "Food".into(),
            amount: MinorAmount::from(10_000),
            used: MinorAmount::from(12_345),
            created_at: 0,
            updated_at: 0,
        };
        let bars = budget_bars(&[budget], "$", 2);
        assert_eq!(bars[0].fill, 1.0);
        assert!(bars[0].over_budget);
        assert_eq!(bars[0].display, "$123.45 / $100.00");
    }

    #[test]
    fn home_trend_uses_last_six_signed_net_values() {
        let months = (1..=8)
            .map(|index| month(&format!("M{index}"), index * 100, index * 120))
            .collect::<Vec<_>>();
        let trend = summary_net_trend(&months, "$", 0).unwrap();
        assert_eq!(trend.points.len(), 6);
        assert_eq!(trend.points[0].label, "M3");
        assert!(trend.points.iter().all(|point| point.position < 0));
        assert_eq!(trend.points[5].display, "$-160");
    }
}
