//! Rule-based finance hints. Pure function over already-loaded `LedgerData`,
//! so it runs identically on SSR and hydrate without an extra round-trip.
//!
//! The bar for adding a rule is: the data the rule needs is already in
//! `LedgerData`, the heuristic is named after a thing the user can act on,
//! and the title/meta strings render to <= 2 lines in the FIN-RUL-01 card.
//! Anything more contextual (statistical anomaly detection, multi-month
//! seasonality, etc.) belongs in a separate analytics module, not here.
//!
//! All amounts in `LedgerData` are integer minor units of `LedgerData.currency`;
//! the rules format with that currency's symbol and precision.

use crate::server_fns::LedgerData;
use ep_core::IconKind;
use ep_i18n::{tf, Locale};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub icon: IconKind,
    pub title: String,
    pub meta: String,
    pub link: Option<String>,
}

/// Threshold (0..1) at which the budget rule starts to nag. 80% leaves
/// some headroom before month-end without crying wolf at 50%.
const BUDGET_OVERAGE_THRESHOLD: f64 = 0.8;

/// Threshold for the "investable surplus" rule, in *major* units of the
/// scoped currency — scaled to minor units against the currency's precision
/// before comparing. Below this the user probably needs the cash for daily
/// flow; above it, parking it as ETF drips is the standard advice.
const SURPLUS_BALANCE_FLOOR_MAJOR: i64 = 10_000;

/// Conservative DCA fraction (30%) of the surplus — leaves 70% liquid.
const SURPLUS_DCA_RATIO: f64 = 0.30;

pub fn compute_suggestions(d: &LedgerData, locale: Locale) -> Vec<Suggestion> {
    let mut out = Vec::new();
    let decimals = d.currency.decimals;
    let symbol = d.currency.symbol.as_str();
    // Compact, symbol-prefixed money string in the scoped currency.
    let money = |minor: i64| format!("{symbol}{}", ep_core::fmt_minor_compact(minor, decimals));

    // Rule 1 — budgets approaching overage. Pick the worst (highest
    // used/amount ratio) so the card stays scoped; if multiple categories
    // tie at >80%, only the first one shows. Users with multiple overages
    // see the dominant one and can drill into Finance for the rest.
    if let Some(worst) = d
        .budgets
        .iter()
        .filter(|b| b.amount > 0 && b.used as f64 / b.amount as f64 > BUDGET_OVERAGE_THRESHOLD)
        .max_by(|a, b| {
            (a.used as f64 / a.amount as f64)
                .partial_cmp(&(b.used as f64 / b.amount as f64))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    {
        let pct = (worst.used as f64 / worst.amount as f64 * 100.0).round() as u32;
        let cat_name = d
            .categories
            .iter()
            .find(|c| c.code == worst.category_code)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| worst.category_code.clone());
        let remaining = money((worst.amount - worst.used).max(0));
        let pct = pct.to_string();
        out.push(Suggestion {
            icon: IconKind::Sparkle,
            title: tf(
                locale,
                "finance.suggestion.budget.title",
                &[("category", &cat_name), ("pct", &pct)],
            ),
            meta: tf(
                locale,
                "finance.suggestion.budget.meta",
                &[("remaining", &remaining)],
            ),
            link: Some("/finance".into()),
        });
    }

    // Rule 2 — investable surplus on any savings-type account beyond the
    // floor. Filters by `r#type == "Savings"` rather than a hardcoded
    // `code == "ACC-02"` so the rule survives a user renaming or replacing
    // their savings account. Picks the highest-balance one when there are
    // multiple, since that's the obvious target.
    let floor_minor = ep_core::major_to_minor(SURPLUS_BALANCE_FLOOR_MAJOR, decimals);
    if let Some(savings) = d
        .accounts
        .iter()
        .filter(|a| a.r#type == "Savings" && a.balance > floor_minor)
        .max_by(|a, b| a.balance.cmp(&b.balance))
    {
        let dca_amount = money((savings.balance as f64 * SURPLUS_DCA_RATIO).round() as i64);
        let balance = money(savings.balance);
        out.push(Suggestion {
            icon: IconKind::Coin,
            title: tf(
                locale,
                "finance.suggestion.surplus.title",
                &[("amount", &dca_amount)],
            ),
            meta: tf(
                locale,
                "finance.suggestion.surplus.meta",
                &[("account", &savings.name), ("balance", &balance)],
            ),
            link: Some("/finance".into()),
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::server_fns::LedgerData;

    fn cny() -> Currency {
        Currency {
            code: "CNY".into(),
            symbol: "¥".into(),
            name: "人民币".into(),
            decimals: 2,
            is_primary: true,
            sort_order: 0,
        }
    }

    // Amounts are integer minor units (CNY at 2 decimals → ×100).
    fn fixture_data() -> LedgerData {
        LedgerData {
            currency: cny(),
            currencies: vec![cny()],
            accounts: vec![
                Account {
                    currency_code: "CNY".into(),
                    code: "ACC-01".into(),
                    name: "Main Checking".into(),
                    r#type: "Checking".into(),
                    tone: "blue".into(),
                    balance: 500_000,
                    archived: false,
                    created_at: 0,
                },
                Account {
                    currency_code: "CNY".into(),
                    code: "ACC-02".into(),
                    name: "Savings".into(),
                    r#type: "Savings".into(),
                    tone: "green".into(),
                    balance: 2_280_000,
                    archived: false,
                    created_at: 0,
                },
            ],
            transfer_accounts: vec![],
            categories: vec![
                Category {
                    currency_code: "CNY".into(),
                    code: "F&B".into(),
                    name: "Food".into(),
                    tone: "amber".into(),
                    sort_order: 1,
                    archived: false,
                    created_at: 0,
                },
                Category {
                    currency_code: "CNY".into(),
                    code: "HLT".into(),
                    name: "Fitness".into(),
                    tone: "green".into(),
                    sort_order: 3,
                    archived: false,
                    created_at: 0,
                },
            ],
            txns: vec![Txn {
                doc_id: "FIN-X".into(),
                currency_code: "CNY".into(),
                occurred_at: 0,
                merchant: "Keep Gym".into(),
                category_code: "HLT".into(),
                account_code: "ACC-01".into(),
                amount: -29_800,
                tag: "exp".into(),
                note: None,
                linked_doc_id: None,
            }],
            category_summary: vec![],
            month: MonthSummary {
                income: 0,
                expense: 0,
                savings: 0,
                balance: 0,
                balance_delta: 0,
                budget_total: 0,
                savings_rate: 0.0,
                emergency_months: 0.0,
                liquid_balance: 0,
                days_elapsed: 1,
                avg_expense_3m: 0,
                total_txn_count: 0,
                period: "2026-05".into(),
            },
            budgets: vec![
                BudgetEntry {
                    category_code: "F&B".into(),
                    amount: 320_000,
                    used: 290_000,
                }, // 90% — over threshold
                BudgetEntry {
                    category_code: "HLT".into(),
                    amount: 120_000,
                    used: 60_000,
                }, // 50% — fine
            ],
            account_stats: vec![
                AccountStats {
                    last_seen_at: None,
                    history_14d: vec![0; 14],
                },
                AccountStats {
                    last_seen_at: None,
                    history_14d: vec![0; 14],
                },
            ],
            months_12: Vec::new(),
            category_usage: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn budget_rule_picks_worst_overage() {
        let s = compute_suggestions(&fixture_data(), Locale::En);
        let budget = s
            .iter()
            .find(|s| s.icon == IconKind::Sparkle)
            .expect("expected budget rule hit");
        assert!(budget.title.contains("Food"), "title: {}", budget.title);
        // 290_000 / 320_000 ≈ 90.625% → rounds to 91. Just check it's > 80 and ≤ 100.
        assert!(
            budget.title.contains("91%") || budget.title.contains("90%"),
            "expected 91% (or 90% on different round mode), got: {}",
            budget.title
        );
    }

    #[test]
    fn surplus_rule_fires_above_floor() {
        let s = compute_suggestions(&fixture_data(), Locale::En);
        let coin = s
            .iter()
            .find(|s| s.icon == IconKind::Coin)
            .expect("expected surplus rule hit");
        // 2_280_000 minor × 0.30 = 684_000 minor → ¥6,840.
        assert!(coin.title.contains("6,840"), "title: {}", coin.title);
    }

    #[test]
    fn no_rules_when_data_is_clean() {
        let mut d = fixture_data();
        // budgets all under threshold
        for b in &mut d.budgets {
            b.used = 0;
        }
        // savings under the floor (¥5,000 < ¥10,000)
        d.accounts[1].balance = 500_000;
        let s = compute_suggestions(&d, Locale::En);
        assert!(
            s.is_empty(),
            "expected zero suggestions, got {} ({:?})",
            s.len(),
            s
        );
    }

    #[test]
    fn suggestions_are_localized() {
        let s = compute_suggestions(&fixture_data(), Locale::ZhCn);
        let budget = s
            .iter()
            .find(|s| s.icon == IconKind::Sparkle)
            .expect("expected budget rule hit");
        assert!(budget.title.contains("已用"), "title: {}", budget.title);
        assert!(
            budget.meta.contains("本月底"),
            "meta should be zh-CN: {}",
            budget.meta
        );
    }
}
