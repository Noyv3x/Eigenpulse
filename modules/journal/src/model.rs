use ep_core::{IconKind, ModuleDescriptor};
use serde::{Deserialize, Serialize};

pub const DESCRIPTOR: ModuleDescriptor = ModuleDescriptor {
    slug: "journal",
    route: "/journal",
    name_key: "journal.module.name",
    description_key: "journal.module.description",
    icon: IconKind::Journal,
    read_scope: crate::SCOPE_READ,
    write_scope: crate::SCOPE_WRITE,
    read_scope_label_key: "journal.scope.read",
    write_scope_label_key: "journal.scope.write",
};

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub entry_date: String,
    pub mood: Option<String>,
    pub tags: String,
    pub archived_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntryListItem {
    pub id: i64,
    pub title: String,
    pub body_preview: String,
    pub body_truncated: bool,
    pub entry_date: String,
    pub mood: Option<String>,
    pub tags: String,
    pub archived_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalData {
    pub today: String,
    pub entries: Vec<JournalEntryListItem>,
    pub next_offset: Option<u32>,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalMonthBucket {
    pub period: String,
    pub entries: i64,
}

#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalDayBucket {
    pub entry_date: String,
    pub entries: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalTagBucket {
    pub name: String,
    pub entries: i64,
    pub is_other: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalAnalytics {
    pub current_year: i32,
    pub previous_year: i32,
    pub months: Vec<JournalMonthBucket>,
    pub days: Vec<JournalDayBucket>,
    pub tags: Vec<JournalTagBucket>,
}

#[cfg(feature = "ssr")]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JournalPage {
    pub entries: Vec<JournalEntryListItem>,
    pub next_offset: Option<u32>,
}

#[cfg(feature = "ssr")]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntryInput {
    pub title: String,
    pub body: String,
    pub entry_date: String,
    pub mood: Option<String>,
    pub tags: String,
}
