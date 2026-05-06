//! Rule-based finance hints. Pure function over already-loaded `LedgerData`,
//! so it runs identically on SSR and hydrate without an extra round-trip.
//!
//! The bar for adding a rule is: the data the rule needs is already in
//! `LedgerData`, the heuristic is named after a thing the user can act on,
//! and the title/meta strings render to ≤ 2 lines in the FIN-AI-01 card.
//! Anything more contextual (statistical anomaly detection, multi-month
//! seasonality, etc.) belongs in a separate analytics module, not here.

use crate::model::Tag;
use crate::server_fns::LedgerData;
use ep_core::IconKind;
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

/// Threshold for the "investable surplus" rule. Below this the user
/// probably needs the cash for daily flow; above it, parking it as ETF
/// drips is the standard advice.
const SURPLUS_BALANCE_FLOOR: f64 = 10_000.0;

/// Conservative DCA fraction (30%) of the surplus — leaves 70% liquid.
const SURPLUS_DCA_RATIO: f64 = 0.30;

pub fn compute_suggestions(d: &LedgerData) -> Vec<Suggestion> {
    let mut out = Vec::new();

    // Rule 1 — budgets approaching overage. Pick the worst (highest
    // used/amount ratio) so the card stays scoped; if multiple categories
    // tie at >80%, only the first one shows. Users with multiple overages
    // see the dominant one and can drill into Finance for the rest.
    if let Some(worst) = d
        .budgets
        .iter()
        .filter(|b| b.amount > 0.0 && b.used / b.amount > BUDGET_OVERAGE_THRESHOLD)
        .max_by(|a, b| {
            (a.used / a.amount)
                .partial_cmp(&(b.used / b.amount))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    {
        let pct = (worst.used / worst.amount * 100.0).round() as u32;
        let cat_name = d
            .categories
            .iter()
            .find(|c| c.code == worst.category_code)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| worst.category_code.clone());
        let remaining = (worst.amount - worst.used).max(0.0);
        out.push(Suggestion {
            icon: IconKind::Sparkle,
            title: format!("{cat_name} used {pct}%"),
            meta: format!(
                "¥{} remaining · spend carefully through month end",
                ep_core::fmt_int(remaining)
            ),
            link: Some("/finance".into()),
        });
    }

    // Rule 2 — orphan cross-module candidate. Fitness/learning expenses
    // that aren't yet linked to a workout / book / course suggest a manual
    // link (the user did the activity but forgot to record the doc id).
    if let Some(orphan) = d.txns.iter().find(|t| {
        t.linked_doc_id.is_none()
            && (t.category_code == "HLT" || t.category_code == "EDU")
            && Tag::parse(&t.tag) == Some(Tag::Exp)
    }) {
        let target_module = if orphan.category_code == "HLT" {
            "Fitness"
        } else {
            "Learning"
        };
        out.push(Suggestion {
            icon: IconKind::Link,
            title: format!("{} · can link to {}", orphan.merchant, target_module),
            meta: format!(
                "{} category spending is not linked to a {} module doc ID",
                orphan.category_code, target_module
            ),
            link: Some("/finance".into()),
        });
    }

    // Rule 3 — investable surplus on any savings-type account beyond the
    // floor. Filters by `r#type == "Savings"` rather than a hardcoded
    // `code == "ACC-02"` so the rule survives a user renaming or replacing
    // their savings account. Picks the highest-balance one when there are
    // multiple, since that's the obvious target.
    if let Some(savings) = d
        .accounts
        .iter()
        .filter(|a| a.r#type == "Savings" && a.balance > SURPLUS_BALANCE_FLOOR)
        .max_by(|a, b| {
            a.balance
                .partial_cmp(&b.balance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    {
        let dca_amount = (savings.balance * SURPLUS_DCA_RATIO).round();
        out.push(Suggestion {
            icon: IconKind::Coin,
            title: format!("Can DCA ¥{}", ep_core::fmt_int(dca_amount)),
            meta: format!(
                "{} balance ¥{} · suggest moving 30% to ETF in batches",
                savings.name,
                ep_core::fmt_int(savings.balance)
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

    fn fixture_data() -> LedgerData {
        LedgerData {
            accounts: vec![
                Account {
                    code: "ACC-01".into(),
                    name: "Main Checking".into(),
                    r#type: "Checking".into(),
                    tone: "blue".into(),
                    balance: 5_000.0,
                    archived: false,
                    created_at: 0,
                },
                Account {
                    code: "ACC-02".into(),
                    name: "Savings".into(),
                    r#type: "Savings".into(),
                    tone: "green".into(),
                    balance: 22_800.0,
                    archived: false,
                    created_at: 0,
                },
            ],
            categories: vec![
                Category {
                    code: "F&B".into(),
                    name: "Food".into(),
                    tone: "amber".into(),
                    sort_order: 1,
                    archived: false,
                    created_at: 0,
                },
                Category {
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
                occurred_at: 0,
                merchant: "Keep Gym".into(),
                category_code: "HLT".into(),
                account_code: "ACC-01".into(),
                amount: -298.0,
                tag: "exp".into(),
                note: None,
                linked_doc_id: None,
            }],
            category_summary: vec![],
            month: MonthSummary {
                income: 0.0,
                expense: 0.0,
                savings: 0.0,
                balance: 0.0,
                balance_delta: 0.0,
                budget_total: 0.0,
                savings_rate: 0.0,
                emergency_months: 0.0,
                liquid_balance: 0.0,
                days_elapsed: 1,
                avg_expense_3m: 0.0,
                total_txn_count: 0,
                period: "2026-05".into(),
            },
            budgets: vec![
                BudgetEntry {
                    category_code: "F&B".into(),
                    amount: 3_200.0,
                    used: 2_900.0,
                }, // 90% — over threshold
                BudgetEntry {
                    category_code: "HLT".into(),
                    amount: 1_200.0,
                    used: 600.0,
                }, // 50% — fine
            ],
            account_stats: vec![
                AccountStats {
                    last_seen_at: None,
                    history_14d: vec![0.0; 14],
                },
                AccountStats {
                    last_seen_at: None,
                    history_14d: vec![0.0; 14],
                },
            ],
            months_12: Vec::new(),
            category_usage: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn budget_rule_picks_worst_overage() {
        let s = compute_suggestions(&fixture_data());
        let budget = s
            .iter()
            .find(|s| s.icon == IconKind::Sparkle)
            .expect("expected budget rule hit");
        assert!(budget.title.contains("Food"), "title: {}", budget.title);
        // 2900 / 3200 ≈ 90.625% → rounds to 91. Just check it's > 80 and ≤ 100.
        assert!(
            budget.title.contains("91%") || budget.title.contains("90%"),
            "expected 91% (or 90% on different round mode), got: {}",
            budget.title
        );
    }

    #[test]
    fn orphan_link_rule_finds_unlinked_hlt_expense() {
        let s = compute_suggestions(&fixture_data());
        let link = s
            .iter()
            .find(|s| s.icon == IconKind::Link)
            .expect("expected link rule hit");
        assert!(link.title.contains("Keep Gym"));
        assert!(link.title.contains("Fitness"));
    }

    #[test]
    fn surplus_rule_fires_above_floor() {
        let s = compute_suggestions(&fixture_data());
        let coin = s
            .iter()
            .find(|s| s.icon == IconKind::Coin)
            .expect("expected surplus rule hit");
        // 22_800 × 0.30 = 6_840
        assert!(coin.title.contains("6,840"));
    }

    #[test]
    fn no_rules_when_data_is_clean() {
        let mut d = fixture_data();
        // budgets all under threshold
        for b in &mut d.budgets {
            b.used = 0.0;
        }
        // no orphan: link the txn
        d.txns[0].linked_doc_id = Some("FIT-S-0001".into());
        // savings under floor
        d.accounts[1].balance = 5_000.0;
        let s = compute_suggestions(&d);
        assert!(
            s.is_empty(),
            "expected zero suggestions, got {} ({:?})",
            s.len(),
            s
        );
    }
}
