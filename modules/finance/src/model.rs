use serde::{Deserialize, Serialize};

/// Wire form for a transaction's `tag` column. The DB column stays TEXT
/// (Txn::tag is `String`) so no `sqlx::Type` impl is needed; this enum is the
/// single source of truth for the persisted `exp | inc | tfr` set. User-created
/// one-leg transactions only accept `exp | inc`; paired transfers are created
/// through the transfer flow and still persist as `tfr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tag {
    Exp,
    Inc,
    Tfr,
}

impl Tag {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Exp => "exp",
            Self::Inc => "inc",
            Self::Tfr => "tfr",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exp" => Some(Self::Exp),
            "inc" => Some(Self::Inc),
            "tfr" => Some(Self::Tfr),
            _ => None,
        }
    }

    #[cfg(feature = "ssr")]
    pub const fn is_single_entry(self) -> bool {
        matches!(self, Self::Exp | Self::Inc)
    }
}

/// Allowed values for `fin_account.type`. Single source of truth so the
/// validation in account CRUD and the dropdown in the management UI stay
/// in sync. `Other` is the catch-all; do not add new values without the UI.
pub const ACCOUNT_TYPES: &[&str] = &[
    "Checking",
    "Savings",
    "Cash",
    "Investment",
    "Credit",
    "Other",
];

/// Allowed values for the optional `tone` column on accounts/categories.
/// Empty string is also accepted (rendered as no-tone). Mirrors the visual
/// `Tone` enum in `ep_core`, which already has `from_str`/`class` helpers.
pub const TONES: &[&str] = &["green", "amber", "rose", "blue", "violet"];

/// Column order in `fin_account` / `fin_category` / `fin_txn` matches these
/// structs field-for-field, so `sqlx::FromRow` (server-only) lets every
/// full-row `SELECT` decode straight into the model with no hand-written
/// tuple mapping. `FromRow` matches by column name, so query column order
/// is irrelevant — only the names have to line up.
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub code: String,
    pub name: String,
    // DB column is `type`; the field is the raw identifier `r#type`.
    #[cfg_attr(feature = "ssr", sqlx(rename = "type"))]
    pub r#type: String,
    pub tone: String,
    pub balance: f64,
    pub archived: bool,
    pub created_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub code: String,
    pub name: String,
    pub tone: String,
    pub sort_order: i64,
    pub archived: bool,
    pub created_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Txn {
    pub doc_id: String,
    pub occurred_at: i64,
    pub merchant: String,
    pub category_code: String,
    pub account_code: String,
    pub amount: f64,
    pub tag: String,
    pub note: Option<String>,
    pub linked_doc_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetEntry {
    pub category_code: String,
    pub amount: f64,
    /// Magnitude of expenses in this category for the budget's period
    /// (matched on `period = 'YYYY-MM'`).
    pub used: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySummary {
    pub code: String,
    pub name: String,
    pub tone: String,
    pub value: f64,
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthSummary {
    pub income: f64,
    pub expense: f64,
    /// `income - expense` for the current month.
    pub savings: f64,
    /// Sum of every account's current balance.
    pub balance: f64,
    /// Net (`income - expense`) over the last 7 days. Signed.
    pub balance_delta: f64,
    pub budget_total: f64,
    /// `(income - expense) / income`, clamped to [0, 1]. Zero when income is 0.
    pub savings_rate: f32,
    /// Liquid balance divided by the 3-month rolling average expense, capped
    /// to 99 to keep KPI rendering sane on fresh installs (zero expense → ∞).
    pub emergency_months: f32,
    /// Sum of `Checking | Savings | Cash` account balances.
    pub liquid_balance: f64,
    /// Days elapsed in the current month, in user-local time. Always ≥ 1.
    pub days_elapsed: u32,
    /// 3-month rolling average expense magnitude.
    pub avg_expense_3m: f64,
    /// Total fin_txn rows in the current month. Distinct from
    /// `LedgerData.txns.len()` which is capped at 50 for the list view.
    pub total_txn_count: i64,
    /// Period the budget queries used, e.g. "2026-05".
    pub period: String,
}

/// Per-account derived stats, parallel-indexed with `LedgerData.accounts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStats {
    /// Most recent occurred_at of any txn touching this account, in unix
    /// seconds. `None` when the account has never been used.
    pub last_seen_at: Option<i64>,
    /// 14-day expense magnitude per day, oldest → newest. Always 14 entries
    /// (zero-padded for days with no spend) so ChartBars renders a
    /// consistent width across accounts.
    pub history_14d: Vec<f64>,
}

/// One bar of the 12-month trend (oldest → newest). `net = income - expense`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonthBucket {
    pub period: String,
    pub income: f64,
    pub expense: f64,
    pub net: f64,
}
