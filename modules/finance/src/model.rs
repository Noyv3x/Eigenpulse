use serde::{Deserialize, Serialize};

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
pub struct NewTxn {
    pub merchant: String,
    pub category_code: String,
    pub account_code: String,
    pub amount: f64,
    pub tag: String,
    pub note: Option<String>,
    pub linked_doc_id: Option<String>,
    pub occurred_at: Option<i64>,
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
