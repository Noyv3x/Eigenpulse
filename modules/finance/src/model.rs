use serde::{Deserialize, Serialize};

/// Wire form for a transaction's `tag` column. The DB column stays TEXT
/// (Txn::tag is `String`) so no `sqlx::Type` impl is needed; this enum is the
/// single source of truth for the `exp | inc | tfr` set, used by add_txn
/// validation, the row-decorator class, and the server-side sign convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tag {
    Exp,
    Inc,
    Tfr,
}

impl Tag {
    pub const fn as_str(&self) -> &'static str {
        match self { Self::Exp => "exp", Self::Inc => "inc", Self::Tfr => "tfr" }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "exp" => Some(Self::Exp),
            "inc" => Some(Self::Inc),
            "tfr" => Some(Self::Tfr),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub code: String,
    pub name: String,
    pub r#type: String,
    pub tone: String,
    pub balance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub code: String,
    pub name: String,
    pub tone: String,
    pub sort_order: i64,
}

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
    pub savings: f64,
    pub balance: f64,
    pub balance_delta: f64,
    pub budget_used: f64,
    pub budget_total: f64,
}
