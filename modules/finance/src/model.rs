use serde::{Deserialize, Serialize};

/// Exact decimal wire/storage amount owned by the Finance module.
pub type MinorAmount = crate::amount::MinorAmount;

#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tag {
    Exp,
    Inc,
    Tfr,
}

#[cfg(feature = "ssr")]
impl Tag {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exp => "exp",
            Self::Inc => "inc",
            Self::Tfr => "tfr",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
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

pub const ACCOUNT_TYPES: &[&str] = &[
    "Checking",
    "Savings",
    "Cash",
    "Investment",
    "Credit",
    "Other",
];
pub const TONES: &[&str] = &["green", "amber", "rose", "blue", "violet"];

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Currency {
    pub id: i64,
    pub code: String,
    pub symbol: String,
    pub remark: String,
    pub decimals: u8,
    pub is_primary: bool,
    pub sort_order: i64,
    pub created_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub id: i64,
    pub currency_id: i64,
    pub currency_code: String,
    pub name: String,
    #[cfg_attr(feature = "ssr", sqlx(rename = "type"))]
    pub r#type: String,
    pub tone: String,
    pub balance: MinorAmount,
    pub archived: bool,
    pub created_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferAccountRef {
    pub id: i64,
    pub currency_id: i64,
    pub currency_code: String,
    pub name: String,
    pub archived: bool,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Category {
    pub id: i64,
    pub currency_id: i64,
    pub currency_code: String,
    pub name: String,
    pub icon: String,
    pub tone: String,
    pub sort_order: i64,
    pub archived: bool,
    pub created_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Txn {
    pub id: i64,
    pub currency_id: i64,
    pub currency_code: String,
    pub occurred_at: i64,
    /// Persisted Finance business date. It is intentionally independent from
    /// later display-timezone changes; hydrate code must not derive it from the
    /// raw Unix timestamp.
    pub occurred_date: String,
    pub merchant: String,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub account_id: i64,
    pub account_name: String,
    pub amount: MinorAmount,
    pub tag: String,
    pub note: Option<String>,
    pub transfer_id: Option<i64>,
    pub transfer_role: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transfer {
    pub id: i64,
    pub occurred_at: i64,
    /// Persisted Finance business date shared by the transfer aggregate and
    /// both of its ledger legs.
    pub occurred_date: String,
    pub from_account_id: i64,
    pub from_account_name: String,
    pub from_currency_id: i64,
    pub from_currency_code: String,
    pub to_account_id: i64,
    pub to_account_name: String,
    pub to_currency_id: i64,
    pub to_currency_code: String,
    pub from_amount: MinorAmount,
    pub to_amount: MinorAmount,
    pub note: Option<String>,
    pub created_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Budget {
    pub id: i64,
    pub currency_id: i64,
    pub currency_code: String,
    pub period: String,
    pub category_id: i64,
    pub category_name: String,
    pub amount: MinorAmount,
    pub used: MinorAmount,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MonthSummary {
    pub currency_id: i64,
    pub currency_code: String,
    pub period: String,
    pub income: MinorAmount,
    pub expense: MinorAmount,
    pub savings: MinorAmount,
    pub balance: MinorAmount,
    pub budget_total: MinorAmount,
    pub transaction_count: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MonthBucket {
    pub period: String,
    pub income: MinorAmount,
    pub expense: MinorAmount,
    pub net: MinorAmount,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CategorySummary {
    pub category_id: i64,
    pub name: String,
    pub icon: String,
    pub tone: String,
    pub value: MinorAmount,
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinanceData {
    pub currency: Currency,
    pub currencies: Vec<Currency>,
    pub accounts: Vec<Account>,
    pub transfer_accounts: Vec<TransferAccountRef>,
    pub categories: Vec<Category>,
    pub transactions: Vec<Txn>,
    pub transfers: Vec<Transfer>,
    pub budgets: Vec<Budget>,
    pub month: MonthSummary,
    pub months_12: Vec<MonthBucket>,
    pub category_summary: Vec<CategorySummary>,
}
