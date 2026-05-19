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

/// Code of the per-currency transfer category. Every currency owns one
/// `fin_category` row with this code; `add_transfer_inner` files both legs of
/// a transfer under it. Created automatically alongside each currency.
#[cfg(feature = "ssr")]
pub const TRANSFER_CATEGORY_CODE: &str = "TFR";

/// A currency — the top-level partition of the finance module. Accounts,
/// categories, transactions and budgets each belong to exactly one currency,
/// and currencies never convert into one another ("每个货币独立分页"). `code`
/// is the immutable identifier; `symbol`, `name`, `decimals` and `sort_order`
/// are user-editable.
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Currency {
    pub code: String,
    pub symbol: String,
    pub name: String,
    /// Minor-unit precision: how many fractional digits this currency keeps —
    /// `2` for yuan/dollar cents, `0` for yen, `8` for satoshi-style assets.
    /// Validated to `0..=8` on every write.
    pub decimals: u8,
    pub is_primary: bool,
    pub sort_order: i64,
}

/// Column order in `fin_account` / `fin_category` / `fin_txn` matches these
/// structs field-for-field, so `sqlx::FromRow` (server-only) lets every
/// full-row `SELECT` decode straight into the model with no hand-written
/// tuple mapping. `FromRow` matches by column name, so query column order
/// is irrelevant — only the names have to line up.
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub currency_code: String,
    pub code: String,
    pub name: String,
    // DB column is `type`; the field is the raw identifier `r#type`.
    #[cfg_attr(feature = "ssr", sqlx(rename = "type"))]
    pub r#type: String,
    pub tone: String,
    /// Balance in `currency_code`'s minor units (e.g. cents).
    pub balance: i64,
    pub archived: bool,
    pub created_at: i64,
}

/// Slim projection of `fin_account` for the cross-currency transfer picker:
/// the form only renders the name and the `"{currency}/{code}"` option value,
/// so shipping the full `Account` (balance, tone, archived, created_at) would
/// be wire bloat.
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferAccountRef {
    pub currency_code: String,
    pub code: String,
    pub name: String,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub currency_code: String,
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
    pub currency_code: String,
    pub occurred_at: i64,
    pub merchant: String,
    pub category_code: String,
    pub account_code: String,
    /// Signed amount in `currency_code`'s minor units; `tag` carries the
    /// expense / income / transfer direction.
    pub amount: i64,
    pub tag: String,
    pub note: Option<String>,
    pub linked_doc_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetEntry {
    pub category_code: String,
    /// Budgeted amount in the currency's minor units.
    pub amount: i64,
    /// Magnitude of expenses in this category for the budget's period
    /// (matched on `period = 'YYYY-MM'`), in the currency's minor units.
    pub used: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategorySummary {
    pub code: String,
    pub name: String,
    pub tone: String,
    /// Spend magnitude in the currency's minor units.
    pub value: i64,
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthSummary {
    /// All amounts are in the scoped currency's minor units.
    pub income: i64,
    pub expense: i64,
    /// `income - expense` for the current month.
    pub savings: i64,
    /// Sum of every account's current balance.
    pub balance: i64,
    /// Net (`income - expense`) over the last 7 days. Signed.
    pub balance_delta: i64,
    pub budget_total: i64,
    /// `(income - expense) / income`, clamped to [0, 1]. Zero when income is 0.
    pub savings_rate: f32,
    /// Liquid balance divided by the 3-month rolling average expense, capped
    /// to 99 to keep KPI rendering sane on fresh installs (zero expense → ∞).
    pub emergency_months: f32,
    /// Sum of `Checking | Savings | Cash` account balances.
    pub liquid_balance: i64,
    /// Days elapsed in the current month, in user-local time. Always ≥ 1.
    pub days_elapsed: u32,
    /// 3-month rolling average expense magnitude.
    pub avg_expense_3m: i64,
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
    /// 14-day expense magnitude per day, oldest → newest, in the currency's
    /// minor units. Always 14 entries (zero-padded for days with no spend) so
    /// ChartBars renders a consistent width across accounts.
    pub history_14d: Vec<i64>,
}

/// One bar of the 12-month trend (oldest → newest). `net = income - expense`.
/// All amounts are in the scoped currency's minor units.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonthBucket {
    pub period: String,
    pub income: i64,
    pub expense: i64,
    pub net: i64,
}
